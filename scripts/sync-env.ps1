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

function Normalize-EnvPath([string]$value) {
    return $value.Trim().Trim('"').Replace('\', '/')
}

$lines = Get-Content $source | ForEach-Object {
    $line = $_.TrimEnd()
    if ($line -match '^\s*#' -or $line -match '^\s*$') {
        return
    }
    if ($line -match '^\s*([^=]+)=(.*)$') {
        $key = $matches[1].Trim()
        $value = Normalize-EnvPath $matches[2]
        return "${key}=`"$value`""
    }
}

$body = ($lines | Where-Object { $_ }) -join "`n"
$content = $header + $body + "`n"

[System.IO.File]::WriteAllText($dest, $content, [System.Text.UTF8Encoding]::new($false))
Write-Host "Wrote $dest from directories.env"
