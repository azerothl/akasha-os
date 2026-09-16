# run-user-journeys.ps1 - End-user journey suite against a live Preview bus.
#
# Exercises real session switching, image/video generation (real engines),
# result discoverability, and optional restart persistence. Optionally captures
# marketing screenshots into website/media/.
#
# Usage:
#   .\demo\run-user-journeys.ps1
#   .\demo\run-user-journeys.ps1 -SkipVideo
#   .\demo\run-user-journeys.ps1 -Screenshots
#   .\demo\run-user-journeys.ps1 -NoStart -SkipVideo   # bus already up
#
# Env:
#   AOS_HOME          Preview data/home (default: %LOCALAPPDATA%\AgentOS-Preview)
#   AOS_PROBE_BUS     Bus address (default 127.0.0.1:24701)
#   AOS_SESSION_EXE   Override aos-session.exe path
param(
    [switch]$SkipVideo,
    [switch]$Screenshots,
    [switch]$NoStart,
    [switch]$NoBuild,
    [switch]$NoRestart,
    [int]$ImageTimeoutSec = 600,
    [int]$VideoTimeoutSec = 1800,
    [int]$DefaultTimeoutSec = 120
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$journeysDir = Join-Path $PSScriptRoot "user-journeys"
$dateStamp = Get-Date -Format "yyyy-MM-dd"
$reportDir = Join-Path $root "var\recette"
$shotDir = Join-Path $root "var\recette\screenshots-$dateStamp"
$websiteMedia = Join-Path $root "website\media"
$busAddr = if ($env:AOS_PROBE_BUS) { $env:AOS_PROBE_BUS } else { "127.0.0.1:24701" }

function Resolve-AosHome {
    if ($env:AOS_HOME -and (Test-Path $env:AOS_HOME)) {
        return (Resolve-Path $env:AOS_HOME).Path
    }
    $preview = Join-Path $env:LOCALAPPDATA "AgentOS-Preview"
    if (Test-Path (Join-Path $preview "share\models\v1-5-pruned-emaonly.safetensors")) {
        return $preview
    }
    return $root
}

$aosHome = Resolve-AosHome
$env:AOS_HOME = $aosHome
$env:AOS_PROBE_BUS = $busAddr

New-Item -ItemType Directory -Path $reportDir -Force | Out-Null

$probeExe = Join-Path $root "target\release\examples\preview_probe.exe"
if (-not (Test-Path $probeExe)) {
    $probeExe = Join-Path $root "target\release\preview_probe.exe"
}

function Find-SessionExe {
    if ($env:AOS_SESSION_EXE -and (Test-Path $env:AOS_SESSION_EXE)) {
        return $env:AOS_SESSION_EXE
    }
    $candidates = @(
        (Join-Path $aosHome "bin\aos-session.exe"),
        (Join-Path $aosHome "aos-session.exe"),
        (Join-Path $root "target\release\aos-session.exe")
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    return $null
}

function Test-BusUp {
    try {
        $client = New-Object System.Net.Sockets.TcpClient
        $iar = $client.BeginConnect("127.0.0.1", 24701, $null, $null)
        $ok = $iar.AsyncWaitHandle.WaitOne(800, $false)
        if ($ok -and $client.Connected) {
            $client.Close()
            return $true
        }
        $client.Close()
    } catch {}
    return $false
}

function Assert-RealEngines {
    $sd = @(
        (Join-Path $aosHome "bin\sd.exe"),
        (Join-Path $aosHome "bin\sd-cli.exe"),
        (Join-Path $root "bin\sd.exe")
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    $weights = Join-Path $aosHome "share\models\v1-5-pruned-emaonly.safetensors"
    if (-not $sd) {
        throw "real engine required: sd.exe not found under $aosHome\bin (refuse stub)"
    }
    if (-not (Test-Path $weights)) {
        throw "real engine required: SD 1.5 weights missing at $weights"
    }
    Write-Host "engines OK - sd=$sd weights=$weights"
    if (-not $SkipVideo) {
        $ltx = Join-Path $aosHome "share\models\ltx-2.3-22b-dev-Q4_K_M.gguf"
        if (-not (Test-Path $ltx)) {
            throw "LTX weights missing at $ltx (use -SkipVideo to skip J3)"
        }
    }
}

function Invoke-Probe {
    param(
        [string]$Intent,
        [string]$RequestJson,
        [int]$TimeoutSec = $DefaultTimeoutSec
    )
    if (-not (Test-Path $probeExe)) {
        throw "preview_probe missing - build with: cargo build -p aos-agent --example preview_probe --release"
    }
    $env:AOS_PROBE_BUS = $busAddr
    $env:AOS_HOME = $aosHome
    $reqFile = Join-Path $env:TEMP ("aos-probe-req-" + [guid]::NewGuid().ToString() + ".json")
    $utf8 = New-Object System.Text.UTF8Encoding $false
    if ([string]::IsNullOrWhiteSpace($RequestJson) -or $RequestJson -eq "null") {
        [System.IO.File]::WriteAllText($reqFile, "null", $utf8)
    } else {
        [System.IO.File]::WriteAllText($reqFile, $RequestJson, $utf8)
    }
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $stdout = & $probeExe $Intent ("@" + $reqFile) "$TimeoutSec" 2>&1 | ForEach-Object { "$_" }
    $exit = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    Remove-Item $reqFile -ErrorAction SilentlyContinue
    $text = ($stdout -join "`n")
    if ($exit -ne 0) {
        throw "probe $Intent failed (exit $exit): $text"
    }
    $line = ($text -split "`r?`n" | Where-Object { $_.Trim() -match '^\{' } | Select-Object -Last 1)
    if ($line) { $line = $line.Trim() }
    if (-not $line) { throw "probe $Intent returned no JSON: $text" }
    return ($line | ConvertFrom-Json)
}

function Invoke-ProbeBatch {
    param(
        [string]$JsonlPath,
        [hashtable]$Vars,
        [int]$TimeoutSec = $DefaultTimeoutSec,
        [string]$OutputPath
    )
    $raw = Get-Content -Raw -Path $JsonlPath
    foreach ($key in $Vars.Keys) {
        $raw = $raw.Replace("{{$key}}", [string]$Vars[$key])
    }
    if ($raw -match '\{\{[a-zA-Z0-9_]+\}\}') {
        throw "unresolved placeholders remain in $JsonlPath"
    }
    $expanded = Join-Path $reportDir ("expanded-" + [IO.Path]::GetFileName($JsonlPath))
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($expanded, $raw, $utf8)
    $env:AOS_PROBE_BATCH_OUTPUT = $OutputPath
    & $probeExe "batch" $expanded "$TimeoutSec"
    if ($LASTEXITCODE -ne 0) {
        throw "batch failed for $JsonlPath"
    }
    Remove-Item Env:AOS_PROBE_BATCH_OUTPUT -ErrorAction SilentlyContinue
    return (Get-Content -Raw $OutputPath | ConvertFrom-Json)
}

function Get-BatchEntry {
    param($Results, [string]$FixtureId)
    $hit = $Results | Where-Object { $_.fixture_id -eq $FixtureId } | Select-Object -First 1
    if (-not $hit) { throw "fixture $FixtureId missing from batch results" }
    return $hit
}

function Assert-Ok {
    param($Entry, [string]$Label)
    if (-not $Entry.ok) {
        throw "$Label failed: $($Entry.error)"
    }
}

function Host-DownloadsPath {
    param([string]$Logical)
    $rel = $Logical.TrimStart('/')
    return (Join-Path $aosHome "var\storage\data\$($rel.Replace('/','\'))")
}

function Test-PngSignature {
    param([string]$Path)
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    return ($bytes.Length -ge 8 -and $bytes[0] -eq 0x89 -and $bytes[1] -eq 0x50 -and $bytes[2] -eq 0x4E -and $bytes[3] -eq 0x47)
}

function Test-WebmSignature {
    param([string]$Path)
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    return ($bytes.Length -ge 4 -and $bytes[0] -eq 0x1A -and $bytes[1] -eq 0x45 -and $bytes[2] -eq 0xDF -and $bytes[3] -eq 0xA3)
}

function Wait-PlatformReady {
    param([int]$TimeoutSec = 180)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            $prevEap = $ErrorActionPreference
            $ErrorActionPreference = "Continue"
            $stdout = & $probeExe "chat.session.list" "null" "15" 2>&1 | ForEach-Object { "$_" }
            $exit = $LASTEXITCODE
            $ErrorActionPreference = $prevEap
            if ($exit -eq 0) {
                Write-Host "platform ready (chat.session.list ok)"
                return
            }
            $text = ($stdout -join " ")
            if ($text -match "aucun service") {
                Start-Sleep -Seconds 2
                continue
            }
        } catch {
            # keep waiting
        }
        Start-Sleep -Seconds 2
    }
    throw "platform services not ready within ${TimeoutSec}s (chat.session.list)"
}

function Start-PreviewSession {
    $exe = Find-SessionExe
    if (-not $exe) { throw "aos-session.exe not found (set AOS_SESSION_EXE or install Preview)" }
    Write-Host "starting Preview: $exe (AOS_HOME=$aosHome)"
    $env:AOS_HOME = $aosHome
    Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe)
    $deadline = (Get-Date).AddMinutes(3)
    while ((Get-Date) -lt $deadline) {
        if (Test-BusUp) {
            Write-Host "bus up on $busAddr"
            Wait-PlatformReady -TimeoutSec 180
            return
        }
        Start-Sleep -Seconds 2
    }
    throw "bus did not come up on $busAddr within 3 minutes"
}

function Stop-PreviewSession {
    Write-Host "stopping Preview processes…"
    $names = @(
        "aos-session", "aos-ui-egui", "aos-busd", "aos-modeld", "aos-modeld-cpu",
        "aos-agentd", "aos-agent-worker", "aos-platformd", "aos-capkd", "aos-auditd",
        "aos-bridged", "aos-mcpd", "sd", "sd-cli", "sd-server"
    )
    foreach ($n in $names) {
        Get-Process -Name $n -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 2
}

# --- build probe ---
if (-not $NoBuild) {
    Write-Host "== build preview_probe =="
    Push-Location $root
    cargo build -p aos-agent --example preview_probe --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build preview_probe failed" }
    Pop-Location
    if (-not (Test-Path $probeExe)) {
        # cargo places examples under target/release/examples/
        $alt = Join-Path $root "target\release\examples\preview_probe.exe"
        if (Test-Path $alt) { $probeExe = $alt }
    }
}

Assert-RealEngines

$startedByUs = $false
if (-not $NoStart) {
    if (-not (Test-BusUp)) {
        Start-PreviewSession
        $startedByUs = $true
    } else {
        Write-Host "bus already up - reusing"
        Wait-PlatformReady -TimeoutSec 120
    }
} elseif (-not (Test-BusUp)) {
    throw "bus not reachable on $busAddr (omit -NoStart or start Preview)"
}

$report = [ordered]@{
    date = $dateStamp
    aos_home = $aosHome
    bus = $busAddr
    skip_video = [bool]$SkipVideo
    journeys = [ordered]@{}
    pass = $false
}

$sessionA = $null
$sessionB = $null
$sessionC = $null
$imagePath = $null
$imageEngine = $null
$videoPath = $null

try {
    # -------- J1 sessions --------
    Write-Host "== J1 parallel sessions + context resume =="
    $j1Out = Join-Path $reportDir "j1-batch.json"
    # Create three sessions via individual calls (capture ids), then expand append/get batch
    $ca = Invoke-Probe -Intent "chat.session.create" -RequestJson '{"title":"Journey A - contexte theiere"}'
    $cb = Invoke-Probe -Intent "chat.session.create" -RequestJson '{"title":"Journey B - contexte voyage"}'
    $cc = Invoke-Probe -Intent "chat.session.create" -RequestJson '{"title":"Journey C - contexte code"}'
    $sessionA = $ca.response.id
    $sessionB = $cb.response.id
    $sessionC = $cc.response.id
    if (-not $sessionA -or -not $sessionB -or -not $sessionC) {
        throw "J1: missing session ids from create responses"
    }
    Write-Host "  sessions A=$sessionA B=$sessionB C=$sessionC"

    $markerA = "rouge carmin"
    $null = Invoke-Probe -Intent "chat.session.append" -RequestJson (@{
        session_id = $sessionA; role = "user"; content = "Retiens : ma couleur preferee est le $markerA."
    } | ConvertTo-Json -Compress)
    $null = Invoke-Probe -Intent "chat.session.append" -RequestJson (@{
        session_id = $sessionA; role = "assistant"; content = "Note : tu preferes le $markerA."
    } | ConvertTo-Json -Compress)
    $null = Invoke-Probe -Intent "chat.session.append" -RequestJson (@{
        session_id = $sessionB; role = "user"; content = "Retiens : destination Tokyo en novembre."
    } | ConvertTo-Json -Compress)
    $null = Invoke-Probe -Intent "chat.session.append" -RequestJson (@{
        session_id = $sessionB; role = "assistant"; content = "Note : Tokyo en novembre."
    } | ConvertTo-Json -Compress)
    $null = Invoke-Probe -Intent "chat.session.append" -RequestJson (@{
        session_id = $sessionC; role = "user"; content = "Retiens : on utilise Rust et egui pour Preview."
    } | ConvertTo-Json -Compress)
    $null = Invoke-Probe -Intent "chat.session.append" -RequestJson (@{
        session_id = $sessionC; role = "assistant"; content = "Note : stack Rust + egui."
    } | ConvertTo-Json -Compress)

    # Switch away then back: get B, then get A and assert marker
    $getB = Invoke-Probe -Intent "chat.session.get" -RequestJson (@{ session_id = $sessionB } | ConvertTo-Json -Compress)
    $getA = Invoke-Probe -Intent "chat.session.get" -RequestJson (@{ session_id = $sessionA } | ConvertTo-Json -Compress)
    $textA = ($getA.response.messages | ForEach-Object { $_.content }) -join "`n"
    if ($textA -notmatch [regex]::Escape($markerA)) {
        throw "J1: session A lost context after switch (missing '$markerA')"
    }
    if (($getB.response.messages | Measure-Object).Count -lt 2) {
        throw "J1: session B should retain its own messages"
    }
    $list = Invoke-Probe -Intent "chat.session.list" -RequestJson "null"
    $ids = @($list.response | ForEach-Object { $_.id })
    foreach ($need in @($sessionA, $sessionB, $sessionC)) {
        if ($ids -notcontains $need) { throw "J1: session $need missing from list" }
    }
    $report.journeys.J1 = @{
        ok = $true
        session_a = $sessionA
        session_b = $sessionB
        session_c = $sessionC
        marker = $markerA
    }
    Write-Host "J1 PASS"

    # -------- J2 image --------
    Write-Host "== J2 real image generation (sd-v1-5) =="
    $j2Line = (Get-Content (Join-Path $journeysDir "j2-image.jsonl") | Select-Object -First 1)
    $j2Req = ($j2Line | ConvertFrom-Json).request | ConvertTo-Json -Compress -Depth 8
    $j2 = Invoke-Probe -Intent "media.image.generate" -RequestJson $j2Req -TimeoutSec $ImageTimeoutSec
    $imagePath = $j2.response.path
    $imageEngine = $j2.response.engine
    $imageBytes = [int64]$j2.response.bytes
    if (-not $imagePath) { throw "J2: no path in response" }
    if ($imageEngine -eq "stub" -or [string]::IsNullOrWhiteSpace($imageEngine)) {
        throw "J2: refused stub engine (got '$imageEngine') - real sdcpp required"
    }
    if ($imageBytes -lt 10000) { throw "J2: image too small ($imageBytes bytes)" }
    $hostImg = Host-DownloadsPath $imagePath
    if (-not (Test-Path $hostImg)) { throw "J2: host file missing $hostImg" }
    if (-not (Test-PngSignature $hostImg)) { throw "J2: not a PNG signature at $hostImg" }
    $report.journeys.J2 = @{
        ok = $true
        path = $imagePath
        engine = $imageEngine
        bytes = $imageBytes
        model_id = $j2.response.model_id
        host = $hostImg
    }
    Write-Host "J2 PASS - $imagePath ($imageBytes bytes, engine=$imageEngine)"

    # -------- J3 video --------
    if (-not $SkipVideo) {
        Write-Host "== J3 short video generation (ltx2.3-dev, 9 frames) =="
        $j3Line = (Get-Content (Join-Path $journeysDir "j3-video.jsonl") | Select-Object -First 1)
        $j3Req = ($j3Line | ConvertFrom-Json).request | ConvertTo-Json -Compress -Depth 8
        $j3 = Invoke-Probe -Intent "media.image.generate" -RequestJson $j3Req -TimeoutSec $VideoTimeoutSec
        $videoPath = $j3.response.path
        $videoEngine = $j3.response.engine
        $videoBytes = [int64]$j3.response.bytes
        if (-not $videoPath) { throw "J3: no path in response" }
        if ($videoEngine -eq "stub") { throw "J3: stub engine refused for video" }
        if ($videoBytes -lt 1000) { throw "J3: video too small ($videoBytes bytes)" }
        $hostVid = Host-DownloadsPath $videoPath
        if (-not (Test-Path $hostVid)) { throw "J3: host file missing $hostVid" }
        if (-not (Test-WebmSignature $hostVid)) { throw "J3: missing EBML/WebM signature at $hostVid" }
        $report.journeys.J3 = @{
            ok = $true
            path = $videoPath
            engine = $videoEngine
            bytes = $videoBytes
            model_id = $j3.response.model_id
            host = $hostVid
        }
        Write-Host "J3 PASS - $videoPath ($videoBytes bytes, engine=$videoEngine)"
    } else {
        $report.journeys.J3 = @{ ok = $true; skipped = $true }
        Write-Host "J3 SKIPPED (-SkipVideo)"
    }

    # -------- J4 result access --------
    Write-Host "== J4 result access (fs.list + create history) =="
    $vars = @{
        image_path = $imagePath
        image_engine = $imageEngine
    }
    $j4Out = Join-Path $reportDir "j4-batch.json"
    $j4 = Invoke-ProbeBatch -JsonlPath (Join-Path $journeysDir "j4-result-access.jsonl") -Vars $vars -TimeoutSec $DefaultTimeoutSec -OutputPath $j4Out
    $listEntry = Get-BatchEntry $j4 "j4-fs-list-downloads"
    Assert-Ok $listEntry "J4 fs.list"
    $recordEntry = Get-BatchEntry $j4 "j4-history-record"
    Assert-Ok $recordEntry "J4 history.record"
    $histList = Get-BatchEntry $j4 "j4-history-list"
    Assert-Ok $histList "J4 history.list"
    $docLoad = Get-BatchEntry $j4 "j4-document-load"
    Assert-Ok $docLoad "J4 document.load"
    $histItems = $histList.response.result.items
    if (-not $histItems) { $histItems = $histList.response.result.result.items }
    # ModuleInvokeResponse wraps result; probe returns full ModuleInvokeResponse as response
    $items = $null
    if ($histList.response.result.items) {
        $items = $histList.response.result.items
    } elseif ($histList.response.result -and $histList.response.result.PSObject.Properties['items']) {
        $items = $histList.response.result.items
    } elseif ($histList.response.ok -and $histList.response.result) {
        # result may be { ok, items } from module json_ok nesting - unwrap common shapes
        $inner = $histList.response.result
        if ($inner.items) { $items = $inner.items }
        elseif ($inner.result -and $inner.result.items) { $items = $inner.result.items }
    }
    $paths = @()
    if ($items) { $paths = @($items | ForEach-Object { $_.path }) }
    if ($paths -notcontains $imagePath) {
        # Soft-check: document.load last_result_path
        $last = $docLoad.response.result.state.last_result_path
        if (-not $last) { $last = $docLoad.response.result.result.state.last_result_path }
        if ($last -ne $imagePath) {
            Write-Host "J4 warn: history items=$($paths -join ', '); doc last=$last"
            throw "J4: generated image not discoverable via create.history / document state"
        }
    }
    $report.journeys.J4 = @{
        ok = $true
        image_path = $imagePath
        history_paths = $paths
    }
    Write-Host "J4 PASS"

    # -------- J5 restart --------
    if (-not $NoRestart) {
        Write-Host "== J5 restart - session context persists =="
        Stop-PreviewSession
        Start-PreviewSession
        $startedByUs = $true
        $getA2 = Invoke-Probe -Intent "chat.session.get" -RequestJson (@{ session_id = $sessionA } | ConvertTo-Json -Compress)
        $textA2 = ($getA2.response.messages | ForEach-Object { $_.content }) -join "`n"
        if ($textA2 -notmatch [regex]::Escape($markerA)) {
            throw "J5: session A context lost after restart"
        }
        $list2 = Invoke-Probe -Intent "chat.session.list" -RequestJson "null"
        $ids2 = @($list2.response | ForEach-Object { $_.id })
        foreach ($need in @($sessionA, $sessionB, $sessionC)) {
            if ($ids2 -notcontains $need) { throw "J5: session $need missing after restart" }
        }
        $report.journeys.J5 = @{ ok = $true; session_a = $sessionA }
        Write-Host "J5 PASS"
    } else {
        $report.journeys.J5 = @{ ok = $true; skipped = $true }
        Write-Host "J5 SKIPPED (-NoRestart)"
    }

    $report.pass = $true
}
catch {
    $report.pass = $false
    $report.error = "$_"
    Write-Host "FAIL: $_" -ForegroundColor Red
}

# -------- Screenshots --------
if ($Screenshots -and $report.pass) {
    Write-Host "== marketing screenshots =="
    New-Item -ItemType Directory -Path $shotDir -Force | Out-Null
    if ($startedByUs -or (Test-BusUp)) {
        # Prefer repo UI binary with marketing harness; fall back to installed UI.
        $uiExe = Join-Path $root "target\release\aos-ui-egui.exe"
        if (-not $NoBuild) {
            Push-Location $root
            cargo build -p aos-ui-egui --release
            if ($LASTEXITCODE -ne 0) { throw "cargo build aos-ui-egui failed" }
            Pop-Location
        }
        if (-not (Test-Path $uiExe)) {
            $uiExe = Join-Path $aosHome "bin\aos-ui-egui.exe"
        }
        if (-not (Test-Path $uiExe)) {
            Write-Host "screenshot UI binary missing - skip captures"
        } else {
            # Ensure bus is up for a short UI shot session; UI can also run headless-ish with screenshot env.
            if (-not (Test-BusUp)) { Start-PreviewSession; $startedByUs = $true }
            # Close existing UI window if any so our shot instance owns the viewport.
            Get-Process -Name "aos-ui-egui" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
            Start-Sleep -Seconds 1
            $env:AOS_UI_SCREENSHOT_DIR = $shotDir
            $env:AOS_UI_SCREENSHOT_FOCUS = "marketing"
            if ($imagePath) { $env:AOS_UI_SCREENSHOT_RESULT = $imagePath }
            $uiProc = Start-Process -FilePath $uiExe -PassThru -WorkingDirectory $aosHome
            $uiDeadline = (Get-Date).AddMinutes(2)
            while (-not $uiProc.HasExited -and (Get-Date) -lt $uiDeadline) {
                Start-Sleep -Seconds 2
            }
            if (-not $uiProc.HasExited) {
                Stop-Process -Id $uiProc.Id -Force -ErrorAction SilentlyContinue
            }
            Remove-Item Env:AOS_UI_SCREENSHOT_DIR -ErrorAction SilentlyContinue
            Remove-Item Env:AOS_UI_SCREENSHOT_FOCUS -ErrorAction SilentlyContinue
            Remove-Item Env:AOS_UI_SCREENSHOT_RESULT -ErrorAction SilentlyContinue

            $map = @{
                "m1-rail.png" = "rail.png"
                "m2-chat.png" = "chat.png"
                "m3-create.png" = "create.png"
            }
            New-Item -ItemType Directory -Path $websiteMedia -Force | Out-Null
            foreach ($srcName in $map.Keys) {
                $src = Join-Path $shotDir $srcName
                if (Test-Path $src) {
                    Copy-Item $src (Join-Path $websiteMedia $map[$srcName]) -Force
                    Write-Host "copied $srcName -> website/media/$($map[$srcName])"
                } else {
                    Write-Host "missing screenshot $srcName"
                }
            }
            $report.screenshots = @{
                dir = $shotDir
                website_media = $websiteMedia
            }
        }
    }
}

# -------- write reports --------
$jsonPath = Join-Path $reportDir "user-journeys-$dateStamp.json"
($report | ConvertTo-Json -Depth 8) | Set-Content -Path $jsonPath -Encoding utf8

$mdPath = Join-Path $root "docs\recette-user-journeys-$dateStamp.md"
$j3line = if ($SkipVideo) { "J3 video: skipped" } elseif ($report.journeys.J3.ok) { "J3 video: PASS ($($report.journeys.J3.path), $($report.journeys.J3.bytes) bytes)" } else { "J3 video: FAIL" }
$status = if ($report.pass) { "PASS" } else { "FAIL" }
$md = @"
# Recette parcours utilisateur - $dateStamp

- **Resultat :** $status
- **AOS_HOME :** ``$aosHome``
- **Bus :** ``$busAddr``

## Journeys

- J1 sessions + reprise de contexte : $(if ($report.journeys.J1.ok) { "PASS" } else { "FAIL" }) (A=$sessionA)
- J2 image réelle : $(if ($report.journeys.J2.ok) { "PASS - $imagePath ($imageEngine)" } else { "FAIL" })
- $j3line
- J4 accès Resultat (fs + Create history) : $(if ($report.journeys.J4.ok) { "PASS" } else { "FAIL" })
- J5 reprise après redémarrage : $(if ($report.journeys.J5.skipped) { "skipped" } elseif ($report.journeys.J5.ok) { "PASS" } else { "FAIL" })

$(if ($report.error) { "## Erreur`n`n``````n$($report.error)`n``````" } else { "" })

## Checklist UI manuelle (hors bus)

- [ ] Sidebar Chat : basculer A → B → A montre le bon historique
- [ ] Create : le Resultat image/vidéo est visible dans Preview + History
- [ ] Chat ``/image`` : pièce jointe + **Open in studio**
- [ ] Onglet Files : ``/downloads`` liste le fichier généré

## Artifacts

- JSON : ``var/recette/user-journeys-$dateStamp.json``
- Fixtures : ``demo/user-journeys/``
- Lancer : ``.\demo\run-user-journeys.ps1``
"@
Set-Content -Path $mdPath -Value $md -Encoding utf8

Write-Host ""
Write-Host "report: $jsonPath"
Write-Host "markdown: $mdPath"
if ($report.pass) {
    Write-Host "AOS_USER_JOURNEYS_PASS"
    exit 0
} else {
    Write-Host "AOS_USER_JOURNEYS_FAIL"
    exit 1
}
