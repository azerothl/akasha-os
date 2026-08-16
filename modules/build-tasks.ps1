# build-tasks.ps1 — construit et package le module « tasks » (.aospkg)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== build wasm32 (tasks) =="
cargo build --manifest-path "$root\modules\tasks\Cargo.toml" --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw "échec build wasm tasks" }

function Resolve-WasmArtifact {
    param([string]$FileName)
    $candidates = @()
    if ($env:CARGO_TARGET_DIR) {
        $candidates += (Join-Path $env:CARGO_TARGET_DIR "wasm32-unknown-unknown\release\$FileName")
    }
    $candidates += (Join-Path $root "target\wasm32-unknown-unknown\release\$FileName")
    $candidates += (Join-Path $root "modules\tasks\target\wasm32-unknown-unknown\release\$FileName")
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    throw "WASM manquant: $FileName"
}

$pkg = "$root\modules\tasks.aospkg"
New-Item -ItemType Directory -Path "$pkg\schemas" -Force | Out-Null
New-Item -ItemType Directory -Path "$pkg\ui" -Force | Out-Null

$wasmSrc = Resolve-WasmArtifact "module_tasks.wasm"
$wasmDst = "$pkg\module.wasm"
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
[System.IO.File]::WriteAllText("$pkg\manifest.yaml", $manifest)

$uiJson = @'
{
  "type": "declarative_ui",
  "title": "Tasks",
  "description": "Human + agent shared task list.",
  "commands": ["tasks.create", "tasks.list", "tasks.update", "tasks.complete"]
}
'@
[System.IO.File]::WriteAllText("$pkg\ui\index.html", $uiJson)

Write-Host "== package ready: $pkg (hash $hash) =="
