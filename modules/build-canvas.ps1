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

$manifest = @"
name: canvas
version: 1.0.0
hash: $hash
permissions:
  required_caps:
    - fs.write:/downloads/**
tools:
  - name: canvas.stroke
    description: Draw a polyline stroke (normalized 0..1 points) on the chat session canvas
    input_schema:
      type: object
      properties:
        session_id: { type: string }
        points: { type: array, items: { type: object, properties: { x: { type: number }, y: { type: number } } } }
        color: { type: string }
        width: { type: number }
      required: [session_id, points]
  - name: canvas.rect
    description: Draw a rectangle on the session canvas
    input_schema:
      type: object
      properties:
        session_id: { type: string }
        x: { type: number }
        y: { type: number }
        w: { type: number }
        h: { type: number }
        color: { type: string }
        fill: { type: boolean }
        width: { type: number }
      required: [session_id, x, y, w, h]
  - name: canvas.ellipse
    description: Draw an ellipse on the session canvas
    input_schema:
      type: object
      properties:
        session_id: { type: string }
        x: { type: number }
        y: { type: number }
        w: { type: number }
        h: { type: number }
        color: { type: string }
        fill: { type: boolean }
        width: { type: number }
      required: [session_id, x, y, w, h]
  - name: canvas.erase
    description: Erase along a polyline (paints background)
    input_schema:
      type: object
      properties:
        session_id: { type: string }
        points: { type: array }
        width: { type: number }
      required: [session_id, points]
  - name: canvas.clear
    description: Clear the session canvas
    input_schema:
      type: object
      properties:
        session_id: { type: string }
      required: [session_id]
  - name: canvas.undo
    description: Undo the last canvas operation
    input_schema:
      type: object
      properties:
        session_id: { type: string }
      required: [session_id]
  - name: canvas.get
    description: Read canvas ops (optional after_seq for deltas)
    input_schema:
      type: object
      properties:
        session_id: { type: string }
        after_seq: { type: integer }
      required: [session_id]
  - name: canvas.export
    description: Export canvas as PNG under /downloads
    input_schema:
      type: object
      properties:
        session_id: { type: string }
        path: { type: string }
        width: { type: integer }
        height: { type: integer }
      required: [session_id]
min_os_api: 1
"@
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
