[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$version = (Get-Content (Join-Path $repoRoot "VERSION") -Raw).Trim()

if ($version -notmatch '^\d+\.\d+\.\d+$') {
    throw "VERSION must contain a semantic version, got '$version'."
}

$checks = [ordered]@{
    "Cargo.toml"             = "version = `"$version`""
    "README.md"              = "Preview $version is an installable host"
    "docs/PRODUCT.md"         = "Preview $version is an installable host"
    "docs/STATUS.md"          = "**Preview:** $version"
    "docs/INSTALL.md"         = "Akasha OS Preview $version"
    "docs/FIRST-RUN.md"       = "Akasha OS Preview $version"
    "docs/TESTER.md"          = "Akasha OS Preview $version"
    "docs/FEATURES.md"        = "Not in Preview $version"
    "docs/fr/README.md"       = "Preview $version est une application"
    "docs/fr/INSTALL.md"      = "Akasha OS Preview $version"
    "docs/fr/FIRST-RUN.md"    = "Akasha OS Preview $version"
    "docs/fr/TESTER.md"       = "Akasha OS Preview $version"
    "docs/fr/FEATURES.md"     = "Hors Preview $version"
}

$errors = @()
foreach ($entry in $checks.GetEnumerator()) {
    $path = Join-Path $repoRoot $entry.Key
    $content = Get-Content $path -Raw
    if (-not $content.Contains($entry.Value)) {
        $errors += "$($entry.Key): expected '$($entry.Value)'"
    }
}

$websiteFiles = Get-ChildItem (Join-Path $repoRoot "website") -Recurse -File -Include *.html
foreach ($file in $websiteFiles) {
    $content = Get-Content $file.FullName -Raw
    foreach ($match in [regex]::Matches($content, 'data-aos-version="([^"]+)"')) {
        if ($match.Groups[1].Value -ne $version) {
            $relative = [System.IO.Path]::GetRelativePath($repoRoot, $file.FullName)
            $errors += "${relative}: data-aos-version declares '$($match.Groups[1].Value)'"
        }
    }
}

if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Error $_ }
    throw "Version consistency check failed."
}

Write-Host "Version consistency check passed for $version."
