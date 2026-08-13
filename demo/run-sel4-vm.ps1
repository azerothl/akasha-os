# run-sel4-vm.ps1 — Piste VM (ADR 0001) : boot seL4/Microkit sous QEMU, gate P4 CPU-only.
#
# Prérequis : WSL distro Ubuntu. Le SDK Microkit est téléchargé dans vm/sel4/sdk/
# (non versionné).
param(
    [switch]$BootstrapOnly
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$wslRoot = (wsl -d Ubuntu -- bash -lc "wslpath -a '$root'") -replace '\s+$', ''

function Invoke-Wsl {
    param([string]$Cmd)
    wsl -d Ubuntu -- bash -lc $Cmd
    if ($LASTEXITCODE -ne 0) { throw "WSL: échec ($LASTEXITCODE)" }
}

Write-Host "== piste VM seL4 (Ubuntu WSL) =="

# CapStore Rust (staticlib aarch64) — lié dans le PD capkd par make/lld.
$env:CARGO_TARGET_DIR = Join-Path $root "target"
Write-Host "== aos-sel4-capkd (aarch64-unknown-none) =="
rustup target add aarch64-unknown-none
if ($LASTEXITCODE -ne 0) { throw "rustup target add aarch64-unknown-none a échoué" }
cargo rustc -p aos-sel4-capkd --release --target aarch64-unknown-none --offline --crate-type staticlib -- -C panic=abort
if ($LASTEXITCODE -ne 0) { throw "cargo rustc aos-sel4-capkd a échoué" }

Invoke-Wsl "sed -i 's/\r$//' '$wslRoot/vm/sel4/bootstrap.sh' '$wslRoot/vm/sel4/run.sh' ; chmod +x '$wslRoot/vm/sel4/bootstrap.sh' '$wslRoot/vm/sel4/run.sh'"
Invoke-Wsl "'$wslRoot/vm/sel4/bootstrap.sh'"
if ($BootstrapOnly) { exit 0 }
Invoke-Wsl "'$wslRoot/vm/sel4/run.sh'"
