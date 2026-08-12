# run-demo.ps1 — Démo P1 : bus + modeld + agentd (+ gate / UI)
#
# Usage :
#   .\demo\run-demo.ps1            # build + démarre les services + gate P1
#   .\demo\run-demo.ps1 -Ui        # idem puis lance l'UI TUI (terminal courant)
#   .\demo\run-demo.ps1 -NoBuild   # sans rebuild
#   .\demo\run-demo.ps1 -Stop      # arrête les services
param(
    [switch]$Ui,
    [switch]$NoBuild,
    [switch]$Stop
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $root "target\release"
$logs = Join-Path $root "target\demo-logs"
New-Item -ItemType Directory -Path $logs -Force | Out-Null

$services = @("aos-busd", "aos-modeld", "aos-agentd")

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
}

Stop-Demo | Out-Null
Start-Sleep -Milliseconds 500

Write-Host "== démarrage des services =="
Start-Process -FilePath "$bin\aos-busd.exe" -RedirectStandardError "$logs\busd.log" -RedirectStandardOutput "$logs\busd.out.log" -WindowStyle Hidden
Start-Sleep -Seconds 1
Start-Process -FilePath "$bin\aos-modeld.exe" -ArgumentList "demo\modeld.dev.yaml" -WorkingDirectory $root -RedirectStandardError "$logs\modeld.log" -RedirectStandardOutput "$logs\modeld.out.log" -WindowStyle Hidden
Start-Process -FilePath "$bin\aos-agentd.exe" -WorkingDirectory $root -RedirectStandardError "$logs\agentd.log" -RedirectStandardOutput "$logs\agentd.out.log" -WindowStyle Hidden
Start-Sleep -Seconds 2

foreach ($s in $services) {
    $p = Get-Process -Name $s -ErrorAction SilentlyContinue
    if ($p) { Write-Host "  $s up (pid $($p.Id))" } else { Write-Host "  $s ÉCHEC — voir $logs" }
}

Write-Host "== Gate P1 =="
& "$bin\aos-gate-p1.exe"
$gate = $LASTEXITCODE

if ($Ui) {
    Write-Host "== UI =="
    & "$bin\aos-ui.exe"
}

exit $gate
