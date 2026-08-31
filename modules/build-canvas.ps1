# build-canvas.ps1 — construit et package le module « canvas » (.aospkg)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Join-OsPath {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Parts)
    $acc = $Parts[0]
    for ($i = 1; $i -lt $Parts.Length; $i++) {
        $acc = [IO.Path]::Combine($acc, $Parts[$i])
    }
    $acc
}

Write-Host "== build wasm32 (canvas) =="
cargo build --manifest-path (Join-OsPath $root modules canvas Cargo.toml) --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw "échec build wasm" }

function Resolve-WasmArtifact {
    param([string]$FileName)
    $candidates = @()
    if ($env:CARGO_TARGET_DIR) {
        $candidates += (Join-OsPath $env:CARGO_TARGET_DIR wasm32-unknown-unknown release $FileName)
    }
    $candidates += (Join-OsPath $root target wasm32-unknown-unknown release $FileName)
    $candidates += (Join-OsPath $root modules canvas target wasm32-unknown-unknown release $FileName)
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    throw "WASM manquant: $FileName"
}

$pkg = Join-OsPath $root modules canvas.aospkg
$share = Join-OsPath $root share modules canvas.aospkg
New-Item -ItemType Directory -Path (Join-OsPath $pkg ui) -Force | Out-Null

$wasmSrc = Resolve-WasmArtifact "module_canvas.wasm"
$wasmDst = Join-OsPath $pkg module.wasm
Copy-Item $wasmSrc $wasmDst -Force

$hash = (Get-FileHash -Algorithm SHA256 $wasmDst).Hash.ToLower()

$manifestTemplate = Join-OsPath $root share modules canvas.aospkg manifest.yaml
if (-not (Test-Path $manifestTemplate)) {
    throw "manifest template missing: $manifestTemplate"
}
$manifest = (Get-Content $manifestTemplate -Raw) -replace '(?m)^hash:\s*.*$', "hash: $hash"
[System.IO.File]::WriteAllText((Join-OsPath $pkg manifest.yaml), $manifest)

$uiJson = @'
{
  "type": "declarative_ui",
  "title": "Canvas",
  "description": "Session drawing canvas — surface is the Chat panel, not a sidebar tab."
}
'@
[System.IO.File]::WriteAllText((Join-OsPath $pkg ui index.html), $uiJson)

New-Item -ItemType Directory -Path $share -Force | Out-Null
Copy-Item (Join-OsPath $pkg *) $share -Recurse -Force
Write-Host "== package ready: $pkg / $share (hash $hash) =="

$catalogue = Join-OsPath $root share modules catalogue.yaml
if (Test-Path $catalogue) {
    Write-Host "== update catalogue.yaml canvas hash =="
    $raw = Get-Content $catalogue -Raw
    $updated = [regex]::Replace(
        $raw,
        '(  - name: canvas\r?\n(?:    .*\r?\n)*?    hash: )sha256:[a-f0-9]+',
        "`${1}sha256:$hash"
    )
    if ($updated -ne $raw) {
        [System.IO.File]::WriteAllText($catalogue, $updated)
        Push-Location $root
        try {
            $env:UPDATE_CATALOGUE = "1"
            cargo test -p aos-platform --no-default-features catalogue::tests::committed_catalogue_signature_matches -- --nocapture 2>$null
        } finally {
            Remove-Item Env:UPDATE_CATALOGUE -ErrorAction SilentlyContinue
            Pop-Location
        }
    }
}
