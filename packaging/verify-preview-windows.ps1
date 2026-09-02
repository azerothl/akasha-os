# verify-preview-windows.ps1 — validate a Windows Preview directory and archive.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$OutDir,

    [Parameter(Mandatory = $true)]
    [string]$Version,

    [string]$ArchivePath = "",

    [switch]$RequireCuda
)

$ErrorActionPreference = "Stop"
$maxReleaseBytes = (2GB) - 1

if (-not (Test-Path -LiteralPath $OutDir -PathType Container)) {
    throw "Preview directory missing: $OutDir"
}
$resolvedOut = (Resolve-Path -LiteralPath $OutDir).Path.TrimEnd('\', '/')

function Assert-PreviewFile {
    param([Parameter(Mandatory = $true)][string]$RelativePath)

    $path = Join-Path $resolvedOut $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Preview file missing: $RelativePath"
    }
    if ((Get-Item -LiteralPath $path).Length -eq 0) {
        throw "Preview file is empty: $RelativePath"
    }
}

$requiredFiles = @(
    "VERSION",
    "README.txt",
    "install.ps1",
    "install.cmd",
    "FIRST-RUN.md",
    "INSTALL.md",
    "TESTER.md",
    "data\models\catalog.yaml",
    "share\models\manifest.json",
    "share\modules\catalogue.yaml",
    "share\modules\catalogue.yaml.sig",
    "share\modules\catalogue.pub",
    "share\mcp\servers.yaml.example"
)

$binaries = @(
    "aos-session.exe",
    "aos-busd.exe",
    "aos-modeld.exe",
    "aos-modeld-cpu.exe",
    "aos-agentd.exe",
    "aos-agent-worker.exe",
    "aos-platformd.exe",
    "aos-capkd.exe",
    "aos-auditd.exe",
    "aos-ui-egui.exe",
    "aos-bridged.exe"
)
foreach ($binary in $binaries) {
    $requiredFiles += "bin\$binary"
}

foreach ($module in @("notes", "tasks", "ext-rt", "canvas")) {
    $requiredFiles += "share\modules\$module.aospkg\manifest.yaml"
    $requiredFiles += "share\modules\$module.aospkg\module.wasm"
}

foreach ($relativePath in $requiredFiles) {
    Assert-PreviewFile $relativePath
}

$catalogueText = Get-Content -LiteralPath `
    (Join-Path $resolvedOut "share\modules\catalogue.yaml") -Raw
foreach ($module in @("notes", "tasks", "ext-rt", "canvas")) {
    $moduleBase = Join-Path $resolvedOut "share\modules\$module.aospkg"
    $wasmHash = (Get-FileHash -Algorithm SHA256 `
        -LiteralPath (Join-Path $moduleBase "module.wasm")).Hash.ToLowerInvariant()
    $manifestText = Get-Content -LiteralPath (Join-Path $moduleBase "manifest.yaml") -Raw
    $manifestMatch = [regex]::Match($manifestText, '(?m)^hash:\s*(?:sha256:)?([a-f0-9]{64})\s*$')
    if (-not $manifestMatch.Success -or $manifestMatch.Groups[1].Value -ne $wasmHash) {
        throw "Module manifest hash mismatch: $module"
    }
    $escapedModule = [regex]::Escape($module)
    $catalogueMatch = [regex]::Match(
        $catalogueText,
        "(?ms)^  - name: $escapedModule\r?\n.*?^    hash: sha256:([a-f0-9]{64})\s*$"
    )
    if (-not $catalogueMatch.Success -or $catalogueMatch.Groups[1].Value -ne $wasmHash) {
        throw "Module catalogue hash mismatch: $module"
    }
}

$packagedVersion = (Get-Content -LiteralPath (Join-Path $resolvedOut "VERSION") -Raw).Trim()
if ($packagedVersion -ne $Version) {
    throw "Preview version mismatch: expected '$Version', found '$packagedVersion'"
}

$modeldBytes = (Get-Item -LiteralPath (Join-Path $resolvedOut "bin\aos-modeld.exe")).Length
$modeldCpuBytes = (Get-Item -LiteralPath (Join-Path $resolvedOut "bin\aos-modeld-cpu.exe")).Length
$platformdBytes = (Get-Item -LiteralPath (Join-Path $resolvedOut "bin\aos-platformd.exe")).Length
if ($modeldBytes -lt 5MB) {
    throw "aos-modeld.exe is too small ($modeldBytes bytes); the llama backend may be missing"
}
if ($modeldCpuBytes -lt 1MB) {
    throw "aos-modeld-cpu.exe is too small ($modeldCpuBytes bytes); the CPU fallback may be missing"
}
if ($platformdBytes -lt 5MB) {
    throw "aos-platformd.exe is too small ($platformdBytes bytes); embeddings may be missing"
}

if ($RequireCuda) {
    $cudaPatterns = @(
        "cudart64_*.dll",
        "cublas64_*.dll",
        "cublasLt64_*.dll",
        "nvJitLink*.dll",
        "nvrtc64_*.dll",
        "nvrtc-builtins*.dll"
    )
    foreach ($pattern in $cudaPatterns) {
        $matches = @(Get-ChildItem -LiteralPath (Join-Path $resolvedOut "bin") -File -Filter $pattern)
        if ($matches.Count -eq 0) {
            throw "CUDA runtime file missing: bin\$pattern"
        }
        foreach ($match in $matches) {
            $requiredFiles += "bin\$($match.Name)"
        }
    }
}

$packageFiles = @(Get-ChildItem -LiteralPath $resolvedOut -Recurse -File)
[long]$packageBytes = ($packageFiles | Measure-Object -Property Length -Sum).Sum
if ($packageBytes -ge $maxReleaseBytes) {
    throw "Preview directory exceeds the GitHub Release 2 GiB limit ($packageBytes bytes)"
}

if ($ArchivePath) {
    if (-not (Test-Path -LiteralPath $ArchivePath -PathType Leaf)) {
        throw "Preview archive missing: $ArchivePath"
    }
    $resolvedArchive = (Resolve-Path -LiteralPath $ArchivePath).Path
    $archiveItem = Get-Item -LiteralPath $resolvedArchive
    if ($archiveItem.Length -eq 0 -or $archiveItem.Length -ge $maxReleaseBytes) {
        throw "Preview archive has an invalid release size ($($archiveItem.Length) bytes)"
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($resolvedArchive)
    try {
        $archiveFiles = [System.Collections.Generic.HashSet[string]]::new(
            [System.StringComparer]::OrdinalIgnoreCase
        )
        [long]$uncompressedBytes = 0
        foreach ($entry in $archive.Entries) {
            $entryName = $entry.FullName.Replace('\', '/')
            if ([System.IO.Path]::IsPathRooted($entryName) -or $entryName -match '(^|/)\.\.(/|$)') {
                throw "Unsafe archive path: $entryName"
            }
            if (-not $entryName.EndsWith('/')) {
                [void]$archiveFiles.Add($entryName)
            }
            $uncompressedBytes += $entry.Length

            if ($entry.Length -gt 0) {
                $stream = $entry.Open()
                try {
                    $stream.CopyTo([System.IO.Stream]::Null)
                } finally {
                    $stream.Dispose()
                }
            }
        }

        foreach ($relativePath in $requiredFiles) {
            $archivePath = $relativePath.Replace('\', '/')
            if (-not $archiveFiles.Contains($archivePath)) {
                throw "Preview archive entry missing: $archivePath"
            }
        }
        if ($uncompressedBytes -ne $packageBytes) {
            throw "Preview archive size mismatch: directory=$packageBytes, archive=$uncompressedBytes"
        }

        Write-Host "Archive verified: $($archive.Entries.Count) entries, $uncompressedBytes bytes expanded"
    } finally {
        $archive.Dispose()
    }
}

Write-Host "Preview verified: $($packageFiles.Count) files, $packageBytes bytes, version $Version"
