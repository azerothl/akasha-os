# Akasha OS Preview — one-liner installer (Windows x64)
# irm https://azerothl.github.io/akasha-os/install.ps1 | iex
$ErrorActionPreference = "Stop"
$Repo = "azerothl/akasha-os"
$Prefix = if ($env:AOS_HOME) { $env:AOS_HOME } else { Join-Path $env:LOCALAPPDATA "AgentOS-Preview" }
$LatestUrl = "https://github.com/$Repo/releases/latest/download/latest.json"

Write-Host "Akasha OS Preview — fetching latest.json"
$latest = Invoke-RestMethod -Uri $LatestUrl -Headers @{ "User-Agent" = "akasha-os-preview-install" }
$asset = $latest.assets | Where-Object { $_.os -eq "windows" -and $_.name -notmatch "-cpu" } | Select-Object -First 1
if (-not $asset) {
    $asset = $latest.assets | Where-Object { $_.name -like "*windows-x64.zip" } | Select-Object -First 1
}
if (-not $asset -or -not $asset.sha256 -or -not $asset.name) {
    throw "latest.json has no Windows artefact with sha256 (fail-closed)"
}

$downloadUrl = "https://github.com/$Repo/releases/latest/download/$($asset.name)"
Write-Host "version $($latest.version)"
Write-Host "url     $downloadUrl"
Write-Host "sha256  $($asset.sha256)"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("aos-preview-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp $asset.name
try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $zip -UseBasicParsing
    $got = (Get-FileHash -Path $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    $want = ([string]$asset.sha256).ToLowerInvariant()
    if ($got -ne $want) {
        throw "sha256 mismatch (got $got, expected $want) — refuse"
    }
    $extract = Join-Path $tmp "extract"
    Expand-Archive -Path $zip -DestinationPath $extract -Force
    $overlay = Join-Path $extract "install.ps1"
    if (-not (Test-Path $overlay)) {
        $inner = Get-ChildItem $extract -Directory | Where-Object {
            Test-Path (Join-Path $_.FullName "install.ps1")
        } | Select-Object -First 1
        if (-not $inner) { throw "extracted archive missing install.ps1 overlay" }
        $overlay = Join-Path $inner.FullName "install.ps1"
    }
    & $overlay -Prefix $Prefix
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
Write-Host "Installed under $Prefix (var/ and etc/ preserved on overlay)."
