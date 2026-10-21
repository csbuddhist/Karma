[CmdletBinding()]
param(
    [ValidateSet('auto', 'lightweight', 'accurate')]
    [string] $OcrProfile = 'auto'
)

$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'Verify-KarmaTestBundle.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$env:KARMA_IMAGE_MODEL_MANIFEST = Join-Path $PSScriptRoot 'models/image/viddexa-nano/manifest.json'
$env:KARMA_OCR_LIGHTWEIGHT_MANIFEST = Join-Path $PSScriptRoot 'models/ocr/pp-ocrv5-mobile/manifest.json'
$env:KARMA_OCR_PROFILE = $OcrProfile

& (Join-Path $PSScriptRoot 'karma-agent-windows.exe')
exit $LASTEXITCODE
