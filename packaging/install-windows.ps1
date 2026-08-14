# install-windows.ps1 — installe / met à jour Agent OS Preview (non destructif)
param(
    [string]$Prefix = "$env:LOCALAPPDATA\AgentOS-Preview"
)

$ErrorActionPreference = "Stop"
$here = $PSScriptRoot

Write-Host "Installation / mise à jour vers $Prefix"
New-Item -ItemType Directory -Force -Path $Prefix | Out-Null

# Overlay programme uniquement — ne pas écraser var/ ni etc/ utilisateur.
foreach ($dir in @("bin", "share", "data", "docs")) {
    $src = Join-Path $here $dir
    if (Test-Path $src) {
        $dst = Join-Path $Prefix $dir
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item "$src\*" $dst -Recurse -Force
    }
}
foreach ($f in @("VERSION", "INSTALL.md", "TESTER.md", "FIRST-RUN.md", "README.txt",
                 "LICENSE", "NOTICE", "LICENSE-COMMERCIAL.md", "install.ps1")) {
    $src = Join-Path $here $f
    if (Test-Path $src) { Copy-Item $src (Join-Path $Prefix $f) -Force }
}

# Première install : créer var/ etc/ vides si absents
foreach ($d in @("var", "etc")) {
    New-Item -ItemType Directory -Force -Path (Join-Path $Prefix $d) | Out-Null
}

$exe = Join-Path $Prefix "bin\aos-session.exe"
$wsh = New-Object -ComObject WScript.Shell
$desktop = [Environment]::GetFolderPath("Desktop")
$lnk = $wsh.CreateShortcut((Join-Path $desktop "Agent OS Preview.lnk"))
$lnk.TargetPath = $exe
$lnk.WorkingDirectory = $Prefix
$lnk.Description = "Agent OS Preview"
$lnk.Save()

$start = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$lnk2 = $wsh.CreateShortcut((Join-Path $start "Agent OS Preview.lnk"))
$lnk2.TargetPath = $exe
$lnk2.WorkingDirectory = $Prefix
$lnk2.Save()

Write-Host "OK. Lancez « Agent OS Preview » depuis le Bureau."
Write-Host "Données utilisateur conservées sous $Prefix\var"
Write-Host "Désinstall : supprimer $Prefix et les raccourcis."
