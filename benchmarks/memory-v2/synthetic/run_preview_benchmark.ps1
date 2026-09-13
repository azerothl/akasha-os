param(
    [string]$Dataset = (Join-Path $PSScriptRoot ''),
    [string]$Output = (Join-Path (Get-Location) '.tmp/memory-v2-run'),
    [string]$Bus = '127.0.0.1:24701',
    [string]$Probe = (Join-Path (Get-Location) 'target/debug/examples/preview_probe.exe'),
    [int]$TimeoutSeconds = 120,
    [switch]$SkipV1,
    [switch]$SkipLlm,
    [string]$ModelId
)

$ErrorActionPreference = 'Stop'
$Dataset = (Resolve-Path $Dataset).Path
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Force -Path $Output | Out-Null

function Invoke-Python([string[]]$Arguments) {
    & python @Arguments
    if ($LASTEXITCODE -ne 0) { throw "python a échoué ($LASTEXITCODE): $($Arguments -join ' ')" }
}

function Invoke-Batch([string]$Input, [string]$Result, [bool]$Summary) {
    if (-not (Test-Path $Input)) { return $false }
    $env:AOS_PROBE_BUS = $Bus
    $env:AOS_PROBE_BATCH_OUTPUT = $Result
    if ($Summary) { $env:AOS_PROBE_RESPONSE_SUMMARY = '1' } else { Remove-Item Env:AOS_PROBE_RESPONSE_SUMMARY -ErrorAction SilentlyContinue }
    & $Probe batch $Input $TimeoutSeconds
    if ($LASTEXITCODE -ne 0) { throw "preview_probe a échoué ($LASTEXITCODE) sur $Input" }
    return $true
}

if (-not (Test-Path $Probe)) {
    cargo build -p aos-agent --example preview_probe --no-default-features
    if ($LASTEXITCODE -ne 0) { throw 'Impossible de compiler preview_probe' }
}

Invoke-Python @((Join-Path $Dataset 'prepare_batches.py'), '--dataset', $Dataset, '--output', (Join-Path $Output 'batches'))
Invoke-Batch (Join-Path $Output 'batches/v2_create.jsonl') (Join-Path $Output 'v2-create-results.json') $false | Out-Null

Invoke-Python @((Join-Path $Dataset 'prepare_batches.py'), '--dataset', $Dataset, '--output', (Join-Path $Output 'batches'), '--runtime-results', (Join-Path $Output 'v2-create-results.json'))
Invoke-Batch (Join-Path $Output 'batches/v2_relations.jsonl') (Join-Path $Output 'relation-results.json') $false | Out-Null
Invoke-Batch (Join-Path $Output 'batches/v2_create.jsonl') (Join-Path $Output 'v2-replay-results.json') $false | Out-Null

if (-not $SkipV1) {
    Invoke-Batch (Join-Path $Output 'batches/v1_write.jsonl') (Join-Path $Output 'v1-write-results.json') $false | Out-Null
}
Invoke-Batch (Join-Path $Output 'batches/query_requests.jsonl') (Join-Path $Output 'query-results.json') $true | Out-Null

if (-not $SkipLlm -and (Test-Path (Join-Path $Output 'batches/llm_context_requests.jsonl'))) {
    Invoke-Batch (Join-Path $Output 'batches/llm_context_requests.jsonl') (Join-Path $Output 'llm-context-results.json') $false | Out-Null
    $llmArgs = @((Join-Path $Dataset 'prepare_llm_batches.py'), '--dataset', $Dataset, '--context-results', (Join-Path $Output 'llm-context-results.json'), '--output', (Join-Path $Output 'batches'))
    if ($ModelId) { $llmArgs += @('--model-id', $ModelId) }
    Invoke-Python $llmArgs
    Invoke-Batch (Join-Path $Output 'batches/llm_requests.jsonl') (Join-Path $Output 'llm-results.json') $false | Out-Null
}

$evalArgs = @((Join-Path $Dataset 'evaluate_results.py'), '--dataset', $Dataset, '--v2-results', (Join-Path $Output 'v2-create-results.json'), '--query-results', (Join-Path $Output 'query-results.json'), '--replay-results', (Join-Path $Output 'v2-replay-results.json'), '--api-results', (Join-Path $Output 'query-results.json'), '--output', (Join-Path $Output 'report.json'), '--fail-on-gate')
if (-not $SkipV1) { $evalArgs += @('--v1-results', (Join-Path $Output 'v1-write-results.json')) }
if (-not $SkipLlm -and (Test-Path (Join-Path $Output 'llm-results.json'))) { $evalArgs += @('--llm-results', (Join-Path $Output 'llm-results.json'), '--llm-cases', (Join-Path $Output 'batches/llm_cases.jsonl')) }
Invoke-Python $evalArgs
Write-Host "Rapport écrit dans $Output/report.json"
