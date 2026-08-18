# Assemble a local declarative_ui demo module (Preview 0.7 / E15).
# Does not change the signed catalogue. Copies into a Preview dist or AOS_HOME.
param(
    [Parameter(Mandatory = $true)]
    [string]$DestDir
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$wasmSrc = Join-Path $root "share\modules\ext-rt.aospkg\module.wasm"
if (-not (Test-Path $wasmSrc)) {
    throw "ext-rt WASM manquant: $wasmSrc"
}

$hash = (Get-FileHash -Algorithm SHA256 $wasmSrc).Hash.ToLower()
$pkg = Join-Path $DestDir "decldemo.aospkg"
New-Item -ItemType Directory -Force -Path (Join-Path $pkg "ui"), (Join-Path $pkg "assets") | Out-Null
Copy-Item $wasmSrc (Join-Path $pkg "module.wasm") -Force

$handlersYaml = @"
tools:
  decldemo.snapshot:
    steps:
      - return:
          ok: true
          count: 3
          items:
            - name: alpha
              value: 1
            - name: beta
              value: 4
            - name: gamma
              value: 9
          series: [1, 4, 9, 16, 9, 4, 1]
  decldemo.run:
    steps:
      - return:
          echo: "{{args.message}}"
          ok: true
"@
Set-Content -Path (Join-Path $pkg "handlers.yaml") -Value $handlersYaml -Encoding utf8
Copy-Item (Join-Path $pkg "handlers.yaml") (Join-Path $pkg "assets\handlers.yaml") -Force

$handlersJson = @"
{
  "tools": {
    "decldemo.snapshot": {
      "steps": [
        {
          "return": {
            "ok": true,
            "count": 3,
            "items": [
              { "name": "alpha", "value": 1 },
              { "name": "beta", "value": 4 },
              { "name": "gamma", "value": 9 }
            ],
            "series": [1, 4, 9, 16, 9, 4, 1]
          }
        }
      ]
    },
    "decldemo.run": {
      "steps": [
        {
          "return": {
            "echo": "{{args.message}}",
            "ok": true
          }
        }
      ]
    }
  }
}
"@
Set-Content -Path (Join-Path $pkg "handlers.json") -Value $handlersJson -Encoding utf8
Copy-Item (Join-Path $pkg "handlers.json") (Join-Path $pkg "assets\handlers.json") -Force

$manifest = @"
name: decldemo
version: 0.8.0
hash: $hash
permissions:
  required_caps: []
tools:
  - name: decldemo.snapshot
    description: Snapshot demo (table + stats + courbe)
    input_schema:
      type: object
    output_schema:
      type: object
  - name: decldemo.run
    description: Echo d'un message
    input_schema:
      type: object
      properties:
        message:
          type: string
          title: Message
    output_schema:
      type: object
ui:
  entry: ui/index.html
  mode: declarative_ui
min_os_api: 1
"@
Set-Content -Path (Join-Path $pkg "manifest.yaml") -Value $manifest -Encoding utf8

$ui = @'
{
  "type": "declarative_ui",
  "title": "Decl Demo",
  "root": {
    "kind": "column",
    "children": [
      { "kind": "heading", "text": "Preview 0.7 — UI declarative" },
      { "kind": "text", "text": "Table, stats et courbe liees a decldemo.snapshot. Le formulaire invoque decldemo.run." },
      { "kind": "markdown", "text": "Pas de webview. Widgets fermes peints par l'hote egui." },
      { "kind": "stat_row", "bind": "decldemo.snapshot" },
      { "kind": "line_chart", "bind": "decldemo.snapshot", "source": "series" },
      { "kind": "table", "bind": "decldemo.snapshot", "source": "items", "columns": ["name", "value"] },
      { "kind": "stat_row", "bind": "decldemo.run", "items": ["echo"] },
      {
        "kind": "form",
        "tool": "decldemo.run",
        "label": "Echo",
        "args": {
          "type": "object",
          "properties": {
            "message": { "type": "string", "title": "Message" }
          }
        }
      },
      { "kind": "button", "tool": "decldemo.snapshot", "label": "Refresh snapshot" }
    ]
  }
}
'@
Set-Content -Path (Join-Path $pkg "ui\index.html") -Value $ui -Encoding utf8

Write-Host "decldemo.aospkg pret: $pkg (hash $hash)"
