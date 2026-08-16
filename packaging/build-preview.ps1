# build-preview.ps1 — assemble Akasha OS Preview (Windows x64 + CUDA)
#
# Prérequis : cargo, CUDA Toolkit, cmake/ninja (llama.cpp).
# GGUF : optionnels (-SkipModels) — téléchargés au premier run via manifest.json.
#
# Sortie : dist/AgentOS-Preview-<ver>-windows-x64/
param(
    [string]$OutDir = "",
    [string]$Version = "",
    [switch]$SkipBuild,
    [switch]$SkipModels,
    [switch]$RequireCuda,
    [switch]$CpuOnly
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Version) {
    $verFile = Join-Path $root "VERSION"
    if (Test-Path $verFile) { $Version = (Get-Content $verFile -Raw).Trim() } else { $Version = "0.3.0" }
}
$suffix = if ($CpuOnly) { "windows-x64-cpu" } else { "windows-x64" }
if (-not $OutDir) { $OutDir = Join-Path $root "dist\AgentOS-Preview-$Version-$suffix" }

$env:CARGO_TARGET_DIR = Join-Path $root "target"

if (-not $SkipBuild) {
    Write-Host "== cargo build --release (aos-auditd sans CUDA/llama) =="
    # Separate resolve: aos-platform default features are additive. Building
    # aos-auditd alone keeps embeddings (llama) off for that binary.
    cargo build --release -p aos-auditd
    if ($LASTEXITCODE -ne 0) { throw "build aos-auditd failed" }

    Write-Host "== cargo build --release (bins Preview) =="
    if ($CpuOnly) {
        Write-Host "  (CPU-only: aos-model/aos-llama without CUDA feature)"
        cargo build --release -p aos-session -p aos-ipc -p aos-capkd -p aos-ui-egui `
            -p aos-agent -p aos-auditd
        if ($LASTEXITCODE -ne 0) { throw "build failed" }
        cargo build --release -p aos-model --no-default-features
        if ($LASTEXITCODE -ne 0) { throw "build aos-model cpu failed" }
        cargo build --release -p aos-platform --no-default-features --features embeddings
        if ($LASTEXITCODE -ne 0) { throw "build aos-platform cpu failed" }
    } else {
        cargo build --release -p aos-session -p aos-ipc -p aos-model -p aos-agent `
            -p aos-platform -p aos-capkd -p aos-ui-egui
        if ($LASTEXITCODE -ne 0) { throw "build failed" }
    }

    Write-Host "== package notes module =="
    pwsh -NoProfile -File (Join-Path $root "modules\build-notes.ps1")
    if ($LASTEXITCODE -ne 0) { throw "build-notes.ps1 failed ($LASTEXITCODE)" }
    Write-Host "== package tasks module =="
    pwsh -NoProfile -File (Join-Path $root "modules\build-tasks.ps1")
    if ($LASTEXITCODE -ne 0) { throw "build-tasks.ps1 failed ($LASTEXITCODE)" }
    if (Test-Path (Join-Path $root "modules\build-ext-rt.ps1")) {
        Write-Host "== package ext-rt module =="
        pwsh -NoProfile -File (Join-Path $root "modules\build-ext-rt.ps1")
        if ($LASTEXITCODE -ne 0) { throw "build-ext-rt.ps1 failed ($LASTEXITCODE)" }
    }
}

$binSrc = Join-Path $root "target\release"
New-Item -ItemType Directory -Force -Path `
    "$OutDir\bin", "$OutDir\etc", "$OutDir\share\models", `
    "$OutDir\share\modules", "$OutDir\share\skills", `
    "$OutDir\data\models", "$OutDir\var", "$OutDir\docs" | Out-Null

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

if ($CpuOnly) {
    Write-Host "== CPU-only package — skipping CUDA runtime DLL copy =="
} else {
# Runtime CUDA (llama.cpp) — requis à côté des .exe.
$cudaCandidates = @(
    $env:CUDA_PATH,
    $env:CUDA_PATH_V12_4,
    $env:CUDA_PATH_V12_6,
    $env:CUDA_PATH_V13_3,
    "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.4",
    "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6",
    "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3"
) | Where-Object { $_ }
$cudaBin = $null
foreach ($rootCuda in $cudaCandidates) {
    foreach ($sub in @("bin\x64", "bin")) {
        $cand = Join-Path $rootCuda $sub
        if (Test-Path (Join-Path $cand "cublas64_12.dll")) { $cudaBin = $cand; break }
        if (Test-Path (Join-Path $cand "cublas64_13.dll")) { $cudaBin = $cand; break }
    }
    if ($cudaBin) { break }
}
$cudaCopied = 0
if ($cudaBin) {
    Write-Host "== CUDA runtime DLLs depuis $cudaBin =="
    $patterns = @(
        "cudart64_*.dll", "cublas64_*.dll", "cublasLt64_*.dll",
        "nvJitLink*.dll", "nvrtc64_*.dll", "nvrtc-builtins*.dll"
    )
    foreach ($pat in $patterns) {
        Get-ChildItem $cudaBin -Filter $pat -ErrorAction SilentlyContinue | ForEach-Object {
            Copy-Item $_.FullName (Join-Path $OutDir "bin\$($_.Name)") -Force
            Write-Host "  + $($_.Name)"
            $cudaCopied++
        }
    }
}
if ($cudaCopied -eq 0) {
    $msg = "CUDA runtime DLLs introuvables — binaires GPU incomplets"
    if ($RequireCuda) { throw $msg } else { Write-Warning $msg }
}
}

Copy-Item (Join-Path $root "data\models\catalog.yaml") "$OutDir\data\models\" -Force
Copy-Item (Join-Path $root "VERSION") "$OutDir\VERSION" -Force
Copy-Item (Join-Path $root "share\models\manifest.json") "$OutDir\share\models\manifest.json" -Force
if (Test-Path (Join-Path $root "share\models\catalog-offerings.json")) {
    Copy-Item (Join-Path $root "share\models\catalog-offerings.json") "$OutDir\share\models\catalog-offerings.json" -Force
}

$notes = Join-Path $root "modules\notes.aospkg"
if (Test-Path $notes) {
    Copy-Item $notes "$OutDir\share\modules\notes.aospkg" -Recurse -Force
} else {
    Write-Warning "notes.aospkg absent — lancer modules\build-notes.ps1"
}

$tasks = Join-Path $root "modules\tasks.aospkg"
if (Test-Path $tasks) {
    Copy-Item $tasks "$OutDir\share\modules\tasks.aospkg" -Recurse -Force
} else {
    Write-Warning "tasks.aospkg absent — lancer modules\build-tasks.ps1"
}

$extrt = Join-Path $root "share\modules\ext-rt.aospkg"
if (-not (Test-Path $extrt)) { $extrt = Join-Path $root "modules\ext-rt.aospkg" }
if (Test-Path $extrt) {
    Copy-Item $extrt "$OutDir\share\modules\ext-rt.aospkg" -Recurse -Force
} else {
    Write-Warning "ext-rt.aospkg absent — lancer modules\build-ext-rt.ps1"
}

$skillsSrc = Join-Path $root "skills"
if (Test-Path $skillsSrc) {
    Copy-Item "$skillsSrc\*" "$OutDir\share\skills\" -Recurse -Force
}

# MCP example config (seeded into var/mcp at first run)
New-Item -ItemType Directory -Force -Path "$OutDir\share\mcp" | Out-Null
$mcpEx = Join-Path $root "var\mcp\servers.yaml.example"
if (Test-Path $mcpEx) {
    Copy-Item $mcpEx "$OutDir\share\mcp\servers.yaml.example" -Force
} else {
    @"
# MCP servers (stdio). Copy to var/mcp/servers.yaml and adapt.
servers: {}
"@ | Set-Content "$OutDir\share\mcp\servers.yaml.example" -Encoding utf8
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
            Write-Warning "GGUF manquant (OK en CI) : $src — téléchargé au premier run"
        }
    }
}

Copy-Item (Join-Path $root "INSTALL.md") "$OutDir\" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "docs\TESTER.md") "$OutDir\TESTER.md" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "docs\FIRST-RUN.md") "$OutDir\docs\FIRST-RUN.md" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "docs\FIRST-RUN.md") "$OutDir\FIRST-RUN.md" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "docs\STATUS.md") "$OutDir\docs\STATUS.md" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "docs\FEATURES.md") "$OutDir\docs\FEATURES.md" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "docs\I18N.md") "$OutDir\docs\I18N.md" -ErrorAction SilentlyContinue
if (Test-Path (Join-Path $root "docs\fr")) {
    New-Item -ItemType Directory -Force -Path "$OutDir\docs\fr" | Out-Null
    Copy-Item (Join-Path $root "docs\fr\*") "$OutDir\docs\fr\" -Recurse -Force -ErrorAction SilentlyContinue
}
Copy-Item (Join-Path $root "LICENSE") "$OutDir\" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "NOTICE") "$OutDir\" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "LICENSE-COMMERCIAL.md") "$OutDir\" -ErrorAction SilentlyContinue
Copy-Item (Join-Path $PSScriptRoot "install-windows.ps1") "$OutDir\install.ps1" -Force
Copy-Item (Join-Path $PSScriptRoot "install-windows.cmd") "$OutDir\install.cmd" -Force

@"
Akasha OS Preview $Version (Windows x64 + NVIDIA)

1. Prérequis : driver NVIDIA récent, nvidia-smi OK, ~4 Go disque
2. Installation : .\install.cmd
   (ou : powershell -ExecutionPolicy Bypass -File .\install.ps1)
3. Premier lancement : télécharge les modèles si besoin, puis ouvre le tutoriel
4. Agents agentic : skills (share/skills), MCP (var/mcp/servers.yaml), sous-agents
5. Voir FIRST-RUN.md, INSTALL.md, TESTER.md, docs/FEATURES.md (et docs/fr/ pour le français)
"@ | Set-Content "$OutDir\README.txt" -Encoding utf8

Write-Host "== package prêt : $OutDir =="
$files = Get-ChildItem $OutDir -Recurse -File
$sum = ($files | Measure-Object -Property Length -Sum).Sum
Write-Host ("taille ~{0:N1} MiB" -f ($sum / 1MB))
Write-Host "== plus gros fichiers =="
$files | Sort-Object Length -Descending | Select-Object -First 15 |
    ForEach-Object { Write-Host ("  {0,8:N1} MiB  {1}" -f ($_.Length / 1MB), $_.FullName.Substring($OutDir.Length).TrimStart('\', '/')) }
$maxBytes = (2GB) - 1
if ($sum -ge $maxBytes) {
    throw "package $OutDir exceeds GitHub Release 2 GiB asset limit ($sum bytes)"
}
