[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$mapPath = Join-Path $PSScriptRoot "site-docs-map.json"
$map = Get-Content $mapPath -Raw | ConvertFrom-Json
$errors = [System.Collections.Generic.List[string]]::new()

function Require-File([string]$relative) {
    $full = Join-Path $repoRoot $relative
    if (-not (Test-Path -LiteralPath $full)) {
        $errors.Add("missing file: $relative")
        return $null
    }
    return Get-Content -LiteralPath $full -Raw
}

function Assert-Contains([string]$relative, [string]$needle, [string]$hint) {
    $content = Require-File $relative
    if ($null -eq $content) {
        return
    }
    if (-not $content.Contains($needle)) {
        $errors.Add("${relative}: expected to contain '$needle' ($hint)")
    }
}

foreach ($entry in $map.html_sot) {
    $null = Require-File $entry.html

    if ($entry.offline_en) {
        $null = Require-File $entry.offline_en
        Assert-Contains $entry.offline_en $entry.site_path "offline EN must link canonical site path"
        Assert-Contains $entry.offline_en "Canonical" "offline EN must declare HTML as Canonical"
    }
    if ($entry.offline_fr) {
        $null = Require-File $entry.offline_fr
        Assert-Contains $entry.offline_fr $entry.site_path "offline FR must link canonical site path"
        Assert-Contains $entry.offline_fr "Canonique" "offline FR must declare HTML as Canonique"
    }
    if ($entry.depth_en) {
        $null = Require-File $entry.depth_en
        Assert-Contains $entry.html ($entry.depth_en -replace '\\', '/') "HTML must link depth EN"
    }
    if ($entry.depth_fr) {
        $null = Require-File $entry.depth_fr
        Assert-Contains $entry.html ($entry.depth_fr -replace '\\', '/') "HTML must link depth FR for bilingual digests"
    }
}

foreach ($entry in $map.md_sot_digest) {
    $null = Require-File $entry.html
    $null = Require-File $entry.canonical_en
    Assert-Contains $entry.html ($entry.canonical_en -replace '\\', '/') "digest HTML must link MD SoT EN"
    if ($entry.canonical_en_alt) {
        $null = Require-File $entry.canonical_en_alt
        Assert-Contains $entry.html ($entry.canonical_en_alt -replace '\\', '/') "digest HTML must link MD SoT EN alt"
    }
    if ($entry.canonical_fr) {
        $null = Require-File $entry.canonical_fr
        Assert-Contains $entry.html ($entry.canonical_fr -replace '\\', '/') "digest HTML must link MD SoT FR"
    }
}

if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Error $_ }
    throw "Site docs map check failed ($($errors.Count) issue(s)). See packaging/site-docs-map.json and docs/SITE-MANUAL.md."
}

Write-Host "Site docs map check passed ($($map.html_sot.Count) HTML SoT, $($map.md_sot_digest.Count) MD digests)."
