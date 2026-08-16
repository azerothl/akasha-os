# build-notes.ps1 — construit et package le module « notes » (.aospkg, §7.2)
#
# Produit `modules/notes.aospkg/` : manifest.yaml (avec hash sha256 du
# binaire), module.wasm, ui/, schemas/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

# Join-Path with "a\b" keeps backslashes on Linux pwsh; cargo then looks for a
# literal `modules\notes\Cargo.toml`. Combine segments so CI (ubuntu + pwsh) works.
function Join-OsPath {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Parts)
    $acc = $Parts[0]
    for ($i = 1; $i -lt $Parts.Length; $i++) {
        $acc = [IO.Path]::Combine($acc, $Parts[$i])
    }
    $acc
}

Write-Host "== build wasm32 =="
cargo build --manifest-path (Join-OsPath $root modules notes Cargo.toml) --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw "échec build wasm" }

function Resolve-WasmArtifact {
    param([string]$FileName)
    $candidates = @()
    if ($env:CARGO_TARGET_DIR) {
        $candidates += (Join-OsPath $env:CARGO_TARGET_DIR wasm32-unknown-unknown release $FileName)
    }
    $candidates += (Join-OsPath $root target wasm32-unknown-unknown release $FileName)
    $candidates += (Join-OsPath $root modules notes target wasm32-unknown-unknown release $FileName)
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    Write-Host "WASM introuvable ($FileName). Candidats :"
    $candidates | ForEach-Object { Write-Host "  - $_" }
    Get-ChildItem -Path $root -Recurse -Filter $FileName -ErrorAction SilentlyContinue |
        Select-Object -First 10 -ExpandProperty FullName |
        ForEach-Object { Write-Host "  found: $_" }
    throw "WASM manquant: $FileName"
}

$pkg = Join-OsPath $root modules notes.aospkg
New-Item -ItemType Directory -Path (Join-OsPath $pkg schemas) -Force | Out-Null
New-Item -ItemType Directory -Path (Join-OsPath $pkg ui) -Force | Out-Null

$wasmSrc = Resolve-WasmArtifact "module_notes.wasm"
$wasmDst = Join-OsPath $pkg module.wasm
Copy-Item $wasmSrc $wasmDst -Force
Write-Host "  wasm: $wasmSrc -> $wasmDst"

$hash = (Get-FileHash -Algorithm SHA256 $wasmDst).Hash.ToLower()

$manifest = @"
name: notes
version: 1.1.0
hash: $hash
permissions:
  required_caps:
    - fs.read:/documents/notes/**
    - fs.write:/documents/notes/**
    - mem.write:module:notes
    - mem.query:module:notes
tools:
  - name: notes.create
    description: Create or overwrite a markdown note (file + memory + graph)
    input_schema:
      type: object
      properties:
        title: { type: string }
        content: { type: string }
      required: [title, content]
    output_schema:
      type: object
      properties:
        path: { type: string }
        slug: { type: string }
        version: { type: integer }
        memory_id: { type: integer }
  - name: notes.update
    description: Update an existing note (reindexes memory + graph)
    input_schema:
      type: object
      properties:
        title: { type: string }
        path: { type: string }
        slug: { type: string }
        content: { type: string }
        new_title: { type: string }
      required: [content]
  - name: notes.list
    description: List notes (title, path, excerpt)
    input_schema:
      type: object
    output_schema:
      type: object
      properties:
        notes: { type: array }
  - name: notes.read
    description: Read a note by title, path or slug (includes links)
    input_schema:
      type: object
      properties:
        title: { type: string }
        path: { type: string }
        slug: { type: string }
  - name: notes.search
    description: Semantic search over notes (deduped by path)
    input_schema:
      type: object
      properties:
        query: { type: string }
        k: { type: integer }
      required: [query]
  - name: notes.links
    description: Outgoing links and backlinks for a note
    input_schema:
      type: object
      properties:
        title: { type: string }
        path: { type: string }
        slug: { type: string }
  - name: notes.related
    description: Graph-linked notes scored by semantic relevance to a topic
    input_schema:
      type: object
      properties:
        title: { type: string }
        path: { type: string }
        slug: { type: string }
        topic: { type: string }
        hops: { type: integer }
        k: { type: integer }
ui:
  entry: ui/index.html
  mode: declarative_ui
min_os_api: 1
"@
[System.IO.File]::WriteAllText((Join-OsPath $pkg manifest.yaml), $manifest)

$uiJson = @'
{
  "type": "declarative_ui",
  "title": "Notes",
  "description": "List, read, create and link notes (human surface of the notes module).",
  "commands": ["notes.list", "notes.read", "notes.create", "notes.update", "notes.search", "notes.links", "notes.related"]
}
'@
[System.IO.File]::WriteAllText((Join-OsPath $pkg ui index.html), $uiJson)

Write-Host "== package ready: $pkg (hash $hash) =="
