# build-tasks.ps1 — construit et package le module « tasks » (.aospkg)
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

Write-Host "== build wasm32 (tasks) =="
cargo build --manifest-path (Join-OsPath $root modules tasks Cargo.toml) --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw "échec build wasm tasks" }

function Resolve-WasmArtifact {
    param([string]$FileName)
    $candidates = @()
    if ($env:CARGO_TARGET_DIR) {
        $candidates += (Join-OsPath $env:CARGO_TARGET_DIR wasm32-unknown-unknown release $FileName)
    }
    $candidates += (Join-OsPath $root target wasm32-unknown-unknown release $FileName)
    $candidates += (Join-OsPath $root modules tasks target wasm32-unknown-unknown release $FileName)
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    throw "WASM manquant: $FileName"
}

$pkg = Join-OsPath $root modules tasks.aospkg
New-Item -ItemType Directory -Path (Join-OsPath $pkg schemas) -Force | Out-Null
New-Item -ItemType Directory -Path (Join-OsPath $pkg ui) -Force | Out-Null

$wasmSrc = Resolve-WasmArtifact "module_tasks.wasm"
$wasmDst = Join-OsPath $pkg module.wasm
Copy-Item $wasmSrc $wasmDst -Force
Write-Host "  wasm: $wasmSrc -> $wasmDst"

$hash = (Get-FileHash -Algorithm SHA256 $wasmDst).Hash.ToLower()

$manifest = @"
name: tasks
version: 1.0.0
hash: $hash
permissions:
  required_caps:
    - fs.read:/documents/tasks/**
    - fs.write:/documents/tasks/**
tools:
  - name: tasks.create
    description: Create a task
    input_schema:
      type: object
      properties:
        title: { type: string }
        notes: { type: string }
      required: [title]
  - name: tasks.list
    description: List tasks
    input_schema:
      type: object
  - name: tasks.update
    description: Update a task
    input_schema:
      type: object
      properties:
        id: { type: string }
        title: { type: string }
        notes: { type: string }
        done: { type: boolean }
      required: [id]
  - name: tasks.complete
    description: Mark a task complete (or reopen)
    input_schema:
      type: object
      properties:
        id: { type: string }
        done: { type: boolean }
      required: [id]
ui:
  entry: ui/index.html
  mode: declarative_ui
min_os_api: 1
"@
[System.IO.File]::WriteAllText((Join-OsPath $pkg manifest.yaml), $manifest)

$uiJson = @'
{
  "type": "declarative_ui",
  "title": "Tasks",
  "description": "Human + agent shared task list.",
  "commands": ["tasks.create", "tasks.list", "tasks.update", "tasks.complete"]
}
'@
[System.IO.File]::WriteAllText((Join-OsPath $pkg ui index.html), $uiJson)

$share = Join-OsPath $root share modules tasks.aospkg
New-Item -ItemType Directory -Path $share -Force | Out-Null
Copy-Item -Recurse -Force (Join-OsPath $pkg '*') $share
Write-Host "== package ready: $pkg / $share (hash $hash) =="

$catalogue = Join-OsPath $root share modules catalogue.yaml
if (Test-Path $catalogue) {
    Write-Host "== update catalogue.yaml tasks hash =="
    $raw = Get-Content $catalogue -Raw -Encoding UTF8
    $updated = [regex]::Replace(
        $raw,
        '(  - name: tasks\r?\n(?:    .*\r?\n)*?    hash: )sha256:[a-f0-9]+',
        "`${1}sha256:$hash"
    )
    if ($updated -ne $raw) {
        $utf8NoBom = New-Object System.Text.UTF8Encoding $false
        [System.IO.File]::WriteAllText($catalogue, $updated, $utf8NoBom)
        Push-Location $root
        try {
            $env:UPDATE_CATALOGUE = "1"
            cargo test -p aos-platform --no-default-features `
                catalogue::tests::committed_catalogue_signature_matches -- --nocapture
            if ($LASTEXITCODE -ne 0) { throw "échec mise à jour signature catalogue" }
        } finally {
            Remove-Item Env:UPDATE_CATALOGUE -ErrorAction SilentlyContinue
            Pop-Location
        }
    }
}
