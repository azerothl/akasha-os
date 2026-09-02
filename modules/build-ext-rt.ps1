# build-ext-rt.ps1 — construit le runtime scripté ext-rt (.aospkg)
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

Write-Host "== build ext-rt wasm32 =="
cargo build --manifest-path (Join-OsPath $root modules ext-rt Cargo.toml) --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw "échec build wasm ext-rt" }

function Resolve-WasmArtifact {
    param([string]$FileName)
    $candidates = @()
    if ($env:CARGO_TARGET_DIR) {
        $candidates += (Join-OsPath $env:CARGO_TARGET_DIR wasm32-unknown-unknown release $FileName)
    }
    $candidates += (Join-OsPath $root target wasm32-unknown-unknown release $FileName)
    $candidates += (Join-OsPath $root modules ext-rt target wasm32-unknown-unknown release $FileName)
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    Write-Host "WASM introuvable ($FileName). Candidats :"
    $candidates | ForEach-Object { Write-Host "  - $_" }
    throw "WASM manquant: $FileName"
}

$pkg = Join-OsPath $root modules ext-rt.aospkg
$share = Join-OsPath $root share modules ext-rt.aospkg
New-Item -ItemType Directory -Path (Join-OsPath $pkg schemas) -Force | Out-Null
New-Item -ItemType Directory -Path (Join-OsPath $pkg ui) -Force | Out-Null
New-Item -ItemType Directory -Path (Join-OsPath $pkg assets) -Force | Out-Null

$wasmSrc = Resolve-WasmArtifact "module_ext_rt.wasm"
$wasmDst = Join-OsPath $pkg module.wasm
Copy-Item $wasmSrc $wasmDst -Force
Write-Host "  wasm: $wasmSrc -> $wasmDst"

$hash = (Get-FileHash -Algorithm SHA256 $wasmDst).Hash.ToLower()

@'
tools:
  ext-rt.ping:
    steps:
      - return:
          ok: "true"
'@ | Set-Content -Path (Join-OsPath $pkg handlers.yaml) -Encoding utf8NoBOM
Copy-Item (Join-OsPath $pkg handlers.yaml) (Join-OsPath $pkg assets handlers.yaml) -Force

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
Set-Content -Path (Join-OsPath $pkg manifest.yaml) -Value $manifest -Encoding utf8NoBOM

@'
{"type":"declarative_ui","title":"ext-rt","commands":["ext-rt.ping"]}
'@ | Set-Content -Path (Join-OsPath $pkg ui index.html) -Encoding utf8NoBOM

New-Item -ItemType Directory -Path $share -Force | Out-Null
Copy-Item -Recurse -Force (Join-OsPath $pkg '*') $share
Write-Host "== package prêt : $pkg / $share (hash $hash) =="

$catalogue = Join-OsPath $root share modules catalogue.yaml
if (Test-Path $catalogue) {
    Write-Host "== update catalogue.yaml ext-rt hash =="
    $raw = Get-Content $catalogue -Raw -Encoding UTF8
    $updated = [regex]::Replace(
        $raw,
        '(  - name: ext-rt\r?\n(?:    .*\r?\n)*?    hash: )sha256:[a-f0-9]+',
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
