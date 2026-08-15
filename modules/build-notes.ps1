# build-notes.ps1 — construit et package le module « notes » (.aospkg, §7.2)
#
# Produit `modules/notes.aospkg/` : manifest.yaml (avec hash sha256 du
# binaire), module.wasm, ui/, schemas/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== build wasm32 =="
cargo build --manifest-path "$root\modules\notes\Cargo.toml" --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw "échec build wasm" }

function Resolve-WasmArtifact {
    param([string]$FileName)
    $candidates = @()
    if ($env:CARGO_TARGET_DIR) {
        $candidates += (Join-Path $env:CARGO_TARGET_DIR "wasm32-unknown-unknown\release\$FileName")
    }
    $candidates += (Join-Path $root "target\wasm32-unknown-unknown\release\$FileName")
    $candidates += (Join-Path $root "modules\notes\target\wasm32-unknown-unknown\release\$FileName")
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

$pkg = "$root\modules\notes.aospkg"
New-Item -ItemType Directory -Path "$pkg\schemas" -Force | Out-Null
New-Item -ItemType Directory -Path "$pkg\ui" -Force | Out-Null

$wasmSrc = Resolve-WasmArtifact "module_notes.wasm"
$wasmDst = "$pkg\module.wasm"
Copy-Item $wasmSrc $wasmDst -Force
Write-Host "  wasm: $wasmSrc -> $wasmDst"

$hash = (Get-FileHash -Algorithm SHA256 $wasmDst).Hash.ToLower()

$manifest = @"
name: notes
version: 1.0.0
hash: $hash
permissions:
  required_caps:
    - fs.read:/documents/notes/**
    - fs.write:/documents/notes/**
    - mem.write:module:notes
    - mem.query:module:notes
tools:
  - name: notes.create
    description: Créer une note (fichier + mémoire épisodique)
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
        version: { type: integer }
  - name: notes.list
    description: Lister les notes
    input_schema:
      type: object
    output_schema:
      type: object
      properties:
        notes: { type: array }
  - name: notes.read
    description: Lire une note par titre
    input_schema:
      type: object
      properties:
        title: { type: string }
      required: [title]
  - name: notes.search
    description: Recherche sémantique dans les notes
    input_schema:
      type: object
      properties:
        query: { type: string }
        k: { type: integer }
      required: [query]
ui:
  entry: ui/index.html
  mode: declarative_ui
min_os_api: 1
"@
Set-Content -Path "$pkg\manifest.yaml" -Value $manifest -Encoding utf8NoBOM

@'
{
  "type": "declarative_ui",
  "title": "Notes",
  "description": "Liste et crée des notes (surface humaine du module notes).",
  "commands": ["notes.list", "notes.read", "notes.create", "notes.search"]
}
'@ | Set-Content -Path "$pkg\ui\index.html" -Encoding utf8NoBOM

Write-Host "== package prêt : $pkg (hash $hash) =="
