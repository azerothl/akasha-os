# build-preview.ps1 — assemble Agent OS Preview 0.1 (Windows x64 + CUDA)
#
# Prérequis : cargo release build OK, GGUF dans tools/models/, notes.aospkg
# (ou modules/build-notes.ps1), NVIDIA toolchain.
#
# Sortie : dist/AgentOS-Preview-0.1-windows-x64/
param(
    [string]$OutDir = "",
    [switch]$SkipBuild,
    [switch]$SkipModels
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $root "dist\AgentOS-Preview-0.1-windows-x64" }

$env:CARGO_TARGET_DIR = Join-Path $root "target"

if (-not $SkipBuild) {
    Write-Host "== cargo build --release (bins Preview) =="
    cargo build --release -p aos-session -p aos-ipc -p aos-model -p aos-agent `
        -p aos-platform -p aos-capkd -p aos-auditd -p aos-ui-egui
    if ($LASTEXITCODE -ne 0) { throw "build failed" }

    Write-Host "== package notes module =="
    pwsh -NoProfile -File (Join-Path $root "modules\build-notes.ps1")
}

$binSrc = Join-Path $root "target\release"
New-Item -ItemType Directory -Force -Path `
    "$OutDir\bin", "$OutDir\etc", "$OutDir\share\models", `
    "$OutDir\share\modules", "$OutDir\data\models", "$OutDir\var" | Out-Null

$bins = @(
    "aos-session.exe", "aos-busd.exe", "aos-modeld.exe", "aos-agentd.exe",
    "aos-agent-worker.exe", "aos-platformd.exe", "aos-capkd.exe",
    "aos-auditd.exe", "aos-ui-egui.exe"
)
foreach ($b in $bins) {
    $src = Join-Path $binSrc $b
    if (-not (Test-Path $src)) { throw "manque $src" }
    Copy-Item $src (Join-Path $OutDir "bin\$b") -Force
}

Copy-Item (Join-Path $root "data\models\catalog.yaml") "$OutDir\data\models\" -Force

$notes = Join-Path $root "modules\notes.aospkg"
if (Test-Path $notes) {
    Copy-Item $notes "$OutDir\share\modules\notes.aospkg" -Recurse -Force
} else {
    Write-Warning "notes.aospkg absent — lancer modules\build-notes.ps1"
}

if (-not $SkipModels) {
    $models = @(
        "qwen2.5-3b-instruct-q4_k_m.gguf",
        "qwen2.5-0.5b-instruct-q4_k_m.gguf"
    )
    foreach ($m in $models) {
        $src = Join-Path $root "tools\models\$m"
        if (Test-Path $src) {
            Copy-Item $src "$OutDir\share\models\$m" -Force
        } else {
            Write-Warning "GGUF manquant: $src — placez-le dans share\models avant distribution"
        }
    }
}

Copy-Item (Join-Path $root "INSTALL.md") "$OutDir\" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "docs\TESTER.md") "$OutDir\TESTER.md" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $PSScriptRoot "install-windows.ps1") "$OutDir\install.ps1" -Force

# Raccourci / README package
@"
Agent OS Preview 0.1 (Windows x64 + NVIDIA)

1. Prérequis : driver NVIDIA récent, nvidia-smi OK
2. Installation : .\install.ps1   (ou double-clic aos-session via bin\)
3. Lancer : Start Menu / Bureau « Agent OS Preview » ou :
     `$env:AOS_HOME = (Resolve-Path .)
     .\bin\aos-session.exe

Voir INSTALL.md et TESTER.md.
"@ | Set-Content "$OutDir\README.txt" -Encoding utf8

Write-Host "== package prêt : $OutDir =="
Get-ChildItem $OutDir -Recurse -File | Measure-Object -Property Length -Sum |
    ForEach-Object { Write-Host ("taille ~{0:N1} MiB" -f ($_.Sum / 1MB)) }
