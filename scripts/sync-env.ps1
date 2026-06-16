$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root "directories.env"
$dest = Join-Path $root ".env"

if (-not (Test-Path $source)) {
    Write-Error "directories.env not found at $source"
}

$header = @"
# Generated from directories.env — edit directories.env and re-run scripts/sync-env.ps1
# or scripts/sync-env.sh to refresh.

"@

$content = Get-Content -Raw $source
$content = $content -replace '(?m)^#.*\r?\n', ''
$trimmed = $content.Trim()

Set-Content -Path $dest -Value ($header + $trimmed + "`n") -NoNewline
Write-Host "Wrote $dest from directories.env"
