# install-windows.ps1 — installe Agent OS Preview pour l'utilisateur courant
param(
    [string]$Prefix = "$env:LOCALAPPDATA\AgentOS-Preview"
)

$ErrorActionPreference = "Stop"
$here = $PSScriptRoot

Write-Host "Installation vers $Prefix"
New-Item -ItemType Directory -Force -Path $Prefix | Out-Null
Copy-Item "$here\*" $Prefix -Recurse -Force

$exe = Join-Path $Prefix "bin\aos-session.exe"
$wsh = New-Object -ComObject WScript.Shell
$desktop = [Environment]::GetFolderPath("Desktop")
$lnk = $wsh.CreateShortcut((Join-Path $desktop "Agent OS Preview.lnk"))
$lnk.TargetPath = $exe
$lnk.WorkingDirectory = $Prefix
$lnk.Description = "Agent OS Preview 0.1"
$lnk.Save()

$start = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$lnk2 = $wsh.CreateShortcut((Join-Path $start "Agent OS Preview.lnk"))
$lnk2.TargetPath = $exe
$lnk2.WorkingDirectory = $Prefix
$lnk2.Save()

Write-Host "OK. Lancez « Agent OS Preview » depuis le Bureau."
Write-Host "Désinstall : supprimer $Prefix et les raccourcis."
