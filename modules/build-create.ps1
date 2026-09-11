# build-create.ps1 — package the Create rich UI module (.aospkg)
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

Write-Host "== build wasm32 (create) =="
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-OsPath $root target }
$env:CARGO_TARGET_DIR = $target
cargo build --manifest-path (Join-OsPath $root modules create Cargo.toml) --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw "create wasm build failed" }

function Resolve-WasmArtifact {
    param([string]$FileName)
    foreach ($c in @(
        (Join-OsPath $target wasm32-unknown-unknown release $FileName),
        (Join-OsPath $root target wasm32-unknown-unknown release $FileName)
    )) {
        if (Test-Path $c) { return $c }
    }
    throw "WASM manquant: module_create.wasm"
}

$pkg = Join-OsPath $root modules create.aospkg
$share = Join-OsPath $root share modules create.aospkg
$modUi = Join-OsPath $root modules create ui index.json
New-Item -ItemType Directory -Path (Join-OsPath $pkg ui) -Force | Out-Null

$uiRaw = Get-Content -LiteralPath $modUi -Raw -Encoding UTF8
if ($uiRaw -match 'Ã|Â') {
    throw "mojibake marker in $modUi"
}
$null = $uiRaw | ConvertFrom-Json

$wasmSrc = Resolve-WasmArtifact "module_create.wasm"
$wasmDst = Join-OsPath $pkg module.wasm
Copy-Item $wasmSrc $wasmDst -Force
Copy-Item $modUi (Join-OsPath $pkg ui index.json) -Force

$hash = (Get-FileHash -Algorithm SHA256 $wasmDst).Hash.ToLower()
$utf8NoBom = New-Object System.Text.UTF8Encoding $false

$manifest = @"
name: create
version: 1.0.0
hash: $hash
permissions:
  required_caps:
    - fs.read:/documents/create/**
    - fs.write:/documents/create/**
    - fs.read:/downloads/**
    - fs.write:/downloads/**
    - media.generate
    - tool.invoke:create
services:
  jobs: 1
  media_image: 1
tools:
  - name: create.history.list
    description: List generation history entries
    input_schema:
      type: object
  - name: create.history.get
    description: Fetch one history entry and restore params
    input_schema:
      type: object
      properties:
        id:
          type: string
      required: [id]
  - name: create.history.record
    description: Append a generation to history
    input_schema:
      type: object
  - name: create.document.load
    description: Load package document state
    input_schema:
      type: object
  - name: create.document.save
    description: Persist package document state
    input_schema:
      type: object
  - name: create.result.get
    description: Return last generated image path for preview
    input_schema:
      type: object
  - name: create.models.list
    description: List image and video model packs for the picker
    input_schema:
      type: object
ui:
  contract: 2
  document: ui/index.json
  entry: ui/index.json
  mode: declarative_ui
min_os_api: 1
"@
[System.IO.File]::WriteAllText((Join-OsPath $pkg manifest.yaml), $manifest, $utf8NoBom)

if (Test-Path -LiteralPath $share) {
    Remove-Item -LiteralPath $share -Recurse -Force
}
Copy-Item -LiteralPath $pkg -Destination $share -Recurse -Force
Write-Host "== package ready: $pkg / $share (hash $hash) =="

$catalogue = Join-OsPath $root share modules catalogue.yaml
if (Test-Path $catalogue) {
    Write-Host "== update catalogue.yaml create hash =="
    $raw = Get-Content $catalogue -Raw -Encoding UTF8
    $updated = [regex]::Replace(
        $raw,
        '(  - name: create\r?\n(?:    .*\r?\n)*?    hash: )sha256:[a-f0-9]+',
        "`${1}sha256:$hash"
    )
    if ($updated -ne $raw) {
        [System.IO.File]::WriteAllText($catalogue, $updated, $utf8NoBom)
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
