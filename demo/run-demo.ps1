# run-demo.ps1 — Démo P1–P5 : bus + modeld + agentd + platformd + capkd + auditd
#
# Usage :
#   .\demo\run-demo.ps1            # build + démarre les services + gate P1
#   .\demo\run-demo.ps1 -Gate p2   # gate P2
#   .\demo\run-demo.ps1 -Gate p3   # gate P3
#   .\demo\run-demo.ps1 -Gate p5   # gate P5 (continuous batching)
#   .\demo\run-demo.ps1 -Ui        # idem puis lance l'UI TUI (terminal courant)
#   .\demo\run-demo.ps1 -NoBuild   # sans rebuild
#   .\demo\run-demo.ps1 -Stop      # arrête les services
param(
    [switch]$Ui,
    [switch]$NoBuild,
    [switch]$Stop,
    [string]$Gate = "p1"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $root "target\release"
$logs = Join-Path $root "target\demo-logs"
New-Item -ItemType Directory -Path $logs -Force | Out-Null

$services = @("aos-busd", "aos-modeld", "aos-agentd", "aos-platformd", "aos-capkd", "aos-auditd")

function Stop-Demo {
    foreach ($s in $services) {
        Get-Process -Name $s -ErrorAction SilentlyContinue | Stop-Process -Force
    }
    Get-Process -Name "aos-agent-worker" -ErrorAction SilentlyContinue | Stop-Process -Force
    Write-Host "services arrêtés"
}

if ($Stop) { Stop-Demo; exit 0 }

if (-not $NoBuild) {
    Write-Host "== build release =="
    cargo build --release --bins
    if ($LASTEXITCODE -ne 0) { throw "échec du build" }
    if ($Gate -eq "p2") {
        pwsh -NoProfile -File "$root\modules\build-notes.ps1"
        if ($LASTEXITCODE -ne 0) { throw "échec packaging module notes" }
    }
}

Stop-Demo | Out-Null
Start-Sleep -Milliseconds 500

Write-Host "== démarrage des services =="
Start-Process -FilePath "$bin\aos-busd.exe" -RedirectStandardError "$logs\busd.log" -RedirectStandardOutput "$logs\busd.out.log" -WindowStyle Hidden
Start-Sleep -Seconds 1
Start-Process -FilePath "$bin\aos-modeld.exe" -ArgumentList "demo\modeld.dev.yaml" -WorkingDirectory $root -RedirectStandardError "$logs\modeld.log" -RedirectStandardOutput "$logs\modeld.out.log" -WindowStyle Hidden
Start-Process -FilePath "$bin\aos-agentd.exe" -WorkingDirectory $root -RedirectStandardError "$logs\agentd.log" -RedirectStandardOutput "$logs\agentd.out.log" -WindowStyle Hidden
$platformCfg = if ($Gate -eq "p3") { "demo\platformd.p3.yaml" } else { "demo\platformd.dev.yaml" }
Start-Process -FilePath "$bin\aos-platformd.exe" -ArgumentList $platformCfg -WorkingDirectory $root -RedirectStandardError "$logs\platformd.log" -RedirectStandardOutput "$logs\platformd.out.log" -WindowStyle Hidden
Start-Process -FilePath "$bin\aos-capkd.exe" -WorkingDirectory $root -RedirectStandardError "$logs\capkd.log" -RedirectStandardOutput "$logs\capkd.out.log" -WindowStyle Hidden
Start-Process -FilePath "$bin\aos-auditd.exe" -WorkingDirectory $root -RedirectStandardError "$logs\auditd.log" -RedirectStandardOutput "$logs\auditd.out.log" -WindowStyle Hidden
Start-Sleep -Seconds 2

foreach ($s in $services) {
    $p = Get-Process -Name $s -ErrorAction SilentlyContinue
    if ($p) { Write-Host "  $s up (pid $($p.Id))" } else { Write-Host "  $s ÉCHEC — voir $logs" }
}

$gateBin = switch ($Gate) {
    "p2" { "aos-gate-p2.exe" }
    "p3" { "aos-gate-p3.exe" }
    "p4" { "aos-gate-p4.exe" }
    "p5" { "aos-gate-p5.exe" }
    default { "aos-gate-p1.exe" }
}
Write-Host "== Gate $Gate =="
& "$bin\$gateBin"
$gateResult = $LASTEXITCODE

if ($Ui) {
    Write-Host "== UI =="
    & "$bin\aos-ui.exe"
}

exit $gateResult
