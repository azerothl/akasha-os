# build-ext-rt.ps1 — construit le runtime scripté ext-rt (.aospkg)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== build ext-rt wasm32 =="
cargo build --manifest-path "$root\modules\ext-rt\Cargo.toml" --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw "échec build wasm ext-rt" }

$pkg = "$root\modules\ext-rt.aospkg"
$share = "$root\share\modules\ext-rt.aospkg"
New-Item -ItemType Directory -Path "$pkg\schemas" -Force | Out-Null
New-Item -ItemType Directory -Path "$pkg\ui" -Force | Out-Null
New-Item -ItemType Directory -Path "$pkg\assets" -Force | Out-Null

$wasmSrc = "$root\modules\ext-rt\target\wasm32-unknown-unknown\release\module_ext_rt.wasm"
$wasmDst = "$pkg\module.wasm"
Copy-Item $wasmSrc $wasmDst -Force

$hash = (Get-FileHash -Algorithm SHA256 $wasmDst).Hash.ToLower()

@'
tools:
  ext-rt.ping:
    steps:
      - return:
          ok: "true"
'@ | Set-Content -Path "$pkg\handlers.yaml" -Encoding utf8NoBOM
Copy-Item "$pkg\handlers.yaml" "$pkg\assets\handlers.yaml" -Force

$manifest = @"
name: ext-rt
version: 1.0.0
hash: $hash
permissions:
  required_caps: []
tools:
  - name: ext-rt.ping
    description: Santé du runtime scripté
    input_schema:
      type: object
    output_schema:
      type: object
ui:
  entry: ui/index.html
  mode: declarative_ui
min_os_api: 1
"@
Set-Content -Path "$pkg\manifest.yaml" -Value $manifest -Encoding utf8NoBOM

@'
{"type":"declarative_ui","title":"ext-rt","commands":["ext-rt.ping"]}
'@ | Set-Content -Path "$pkg\ui\index.html" -Encoding utf8NoBOM

New-Item -ItemType Directory -Path $share -Force | Out-Null
Copy-Item -Recurse -Force "$pkg\*" $share
Write-Host "== package prêt : $pkg / $share (hash $hash) =="
