# Smoke: aos-bridged against a running Preview session (bus on 24701).
# Usage (PowerShell): .\demo\smoke-bridge.ps1
# Requires: aos-bridged on PATH or target\release\aos-bridged.exe

$ErrorActionPreference = "Stop"
$port = if ($env:AOS_BRIDGE_PORT) { $env:AOS_BRIDGE_PORT } else { "24710" }
$base = "http://127.0.0.1:$port/v1"

$exe = if (Test-Path ".\target\release\aos-bridged.exe") {
    ".\target\release\aos-bridged.exe"
} elseif (Get-Command aos-bridged -ErrorAction SilentlyContinue) {
    "aos-bridged"
} else {
    Write-Error "Build first: cargo build -p aos-bridge --release"
}

$proc = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 1
try {
    $health = Invoke-RestMethod -Uri "$base/health" -Method Get
    if (-not $health.ok) { throw "health not ok" }
    Write-Host "health OK — bus=$($health.bus)"

    $ctx = Invoke-RestMethod -Uri "$base/mem/context" -Method Post -ContentType "application/json" -Body "{}"
    Write-Host "mem.context OK"

    try {
        Invoke-RestMethod -Uri "$base/secrets/get" -Method Post -ContentType "application/json" `
            -Headers @{ "X-Aos-From" = "agent:smoke" } `
            -Body '{"name":"x"}' | Out-Null
        throw "expected 403 for agent secrets.get"
    } catch {
        if ($_.Exception.Response.StatusCode.value__ -ne 403) {
            throw "expected HTTP 403, got: $_"
        }
        Write-Host "secrets.get agent → 403 OK"
    }
    Write-Host "AOS_BRIDGE_SMOKE_PASS"
} finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
}
