# Build a local Preview 0.7 test package (CPU) + declarative_ui demo module.
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$ver = (Get-Content (Join-Path $root "VERSION") -Raw).Trim()
$dist = Join-Path $root "dist\AgentOS-Preview-$ver-windows-x64-cpu"

$pwsh = Join-Path $env:ProgramFiles "PowerShell\7\pwsh.exe"
$build = Join-Path $PSScriptRoot "build-preview.ps1"
$buildArgs = @("-NoProfile", "-File", $build, "-SkipModels", "-CpuOnly")
if ($SkipBuild) { $buildArgs += "-SkipBuild" }
if (Test-Path $pwsh) {
    & $pwsh @buildArgs
} else {
    & $build @("-SkipModels", "-CpuOnly") + $(if ($SkipBuild) { @("-SkipBuild") } else { @() })
}
if ($LASTEXITCODE -ne 0) { throw "build-preview.ps1 failed" }

& (Join-Path $PSScriptRoot "make-decldemo.ps1") -DestDir (Join-Path $dist "share\modules")
$demo = Join-Path $dist "share\modules\decldemo.aospkg"
Copy-Item $demo (Join-Path $dist "decldemo.aospkg") -Recurse -Force

$howto = @"
Akasha OS Preview $ver — paquet de TEST (CPU, Windows)

Ce n'est PAS un OS bootable. Paquet CPU pour verifier l'hote declarative_ui (E15)
sans recompiler CUDA.

1. Lancer
   - Double-clic : .\install.cmd
     (installe sous %LOCALAPPDATA%\AgentOS-Preview, conserve var/ et les modeles)
   - Ou portable : .\bin\aos-session.exe depuis ce dossier
   - Puis installer le module demo :
       powershell -ExecutionPolicy Bypass -File .\install-decldemo.ps1

2. Dans l'UI
   - Barre laterale : section Modules → **Decl Demo**
   - Verifier : heading, stats, courbe, table (alpha/beta/gamma)
   - Formulaire Message → Echo ; bouton Refresh snapshot
   - Un document UI invalide affiche une banniere rouge (pas de widgets partiels)

3. Notes / Tasks restent les onglets hardcodes (pas Decl Demo)

Inference CPU = lente. Pour tester seulement l'UI, pas besoin d'attendre un chat.
"@
Set-Content -Path (Join-Path $dist "TEST-DECL-UI.txt") -Value $howto -Encoding utf8

$installDemo = @'
# Installs decldemo into the stable Preview home (or this portable dist).
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$pkg = Join-Path $here "share\modules\decldemo.aospkg"
if (-not (Test-Path $pkg)) { $pkg = Join-Path $here "decldemo.aospkg" }
if (-not (Test-Path $pkg)) { throw "decldemo.aospkg introuvable" }

$home = $env:AOS_HOME
if (-not $home) {
    $portable = Test-Path (Join-Path $here ".portable")
    if ($portable -or $env:AOS_PORTABLE -eq "1") {
        $home = $here
    } else {
        $home = Join-Path $env:LOCALAPPDATA "AgentOS-Preview"
    }
}
if (-not (Test-Path $home)) {
    throw "Dossier Preview introuvable ($home). Lancez d'abord install.cmd ou aos-session."
}

$dest = Join-Path $home "var\modules\decldemo"
New-Item -ItemType Directory -Force -Path (Split-Path $dest) | Out-Null
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
Copy-Item $pkg $dest -Recurse -Force

$shareDest = Join-Path $home "share\modules\decldemo.aospkg"
New-Item -ItemType Directory -Force -Path (Split-Path $shareDest) | Out-Null
if (Test-Path $shareDest) { Remove-Item $shareDest -Recurse -Force }
Copy-Item $pkg $shareDest -Recurse -Force

$reg = Join-Path $home "var\modules\registry.yaml"
$entry = @"
- name: decldemo
  granted_caps: []
  quarantined: false
"@
if (Test-Path $reg) {
    $raw = Get-Content $reg -Raw
    if ($raw -notmatch "name: decldemo") {
        Add-Content $reg $entry
    }
} else {
    @"
installed:
- name: decldemo
  granted_caps: []
  quarantined: false
"@ | Set-Content $reg -Encoding utf8
}

Write-Host "decldemo installe dans $dest"
Write-Host "Relancez Akasha OS Preview, puis ouvrez l'onglet Modules → Decl Demo."
'@
Set-Content -Path (Join-Path $dist "install-decldemo.ps1") -Value $installDemo -Encoding utf8

Write-Host "== test package pret : $dist =="
Write-Host "Lancer : $dist\bin\aos-session.exe  puis  $dist\install-decldemo.ps1"
