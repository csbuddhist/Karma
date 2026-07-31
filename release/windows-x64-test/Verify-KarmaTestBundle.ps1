[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$bundleRoot = [System.IO.Path]::GetFullPath($PSScriptRoot)
$checksumsPath = Join-Path $bundleRoot 'SHA256SUMS'
$requiredPaths = @(
    'karma-agent-windows.exe',
    'DirectML.dll',
    'models/image/viddexa-nano/manifest.json',
    'models/ocr/pp-ocrv5-mobile/manifest.json'
)

function Stop-Verification([string] $Message) {
    Write-Error "Karma test bundle verification failed: $Message"
    exit 1
}

foreach ($relativePath in $requiredPaths) {
    $candidate = [System.IO.Path]::GetFullPath((Join-Path $bundleRoot $relativePath))
    if (-not $candidate.StartsWith($bundleRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        Stop-Verification "required file is missing: $relativePath"
    }
}

foreach ($manifestPath in @(
    'models/image/viddexa-nano/manifest.json',
    'models/ocr/pp-ocrv5-mobile/manifest.json'
)) {
    try {
        Get-Content -LiteralPath (Join-Path $bundleRoot $manifestPath) -Raw -Encoding UTF8 | ConvertFrom-Json | Out-Null
    } catch {
        Stop-Verification "invalid JSON manifest: $manifestPath"
    }
}

if (-not (Test-Path -LiteralPath $checksumsPath -PathType Leaf)) {
    Stop-Verification 'SHA256SUMS is missing'
}

$entries = Get-Content -LiteralPath $checksumsPath -Encoding UTF8
if ($entries.Count -eq 0) { Stop-Verification 'SHA256SUMS is empty' }

foreach ($entry in $entries) {
    if ($entry -notmatch '^([0-9a-fA-F]{64})  ([^\\/].*)$') {
        Stop-Verification "invalid SHA256SUMS entry: $entry"
    }
    $expectedHash = $Matches[1].ToLowerInvariant()
    $relativePath = $Matches[2]
    if ($relativePath.Contains('..') -or $relativePath.Contains('\\')) {
        Stop-Verification "unsafe SHA256SUMS path: $relativePath"
    }
    $candidate = [System.IO.Path]::GetFullPath((Join-Path $bundleRoot $relativePath))
    if (-not $candidate.StartsWith($bundleRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        Stop-Verification "missing checksummed file: $relativePath"
    }
    $actualHash = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        Stop-Verification "checksum mismatch: $relativePath"
    }
}

Write-Host 'Karma Windows test bundle verification passed.'
