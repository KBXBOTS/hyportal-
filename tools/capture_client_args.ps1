<#
  Captures how the official launcher spawns HytaleClient.exe.

  HyPortal needs to know which arguments carry the session identity in order to
  launch the game directly instead of delegating. Argument *names* are the
  interesting part; any value that looks like a token is redacted before it is
  written to disk, so the output is safe to paste into an issue.

  Usage:
      powershell -ExecutionPolicy Bypass -File tools\capture_client_args.ps1
  then press Play and sign in as normal.
#>

$ErrorActionPreference = 'Stop'
$out = Join-Path $PSScriptRoot '..\client-args.txt'
$deadline = (Get-Date).AddMinutes(5)

Write-Host 'Watching for HytaleClient.exe (5 min)...  Launch the game now.'

$proc = $null
while ((Get-Date) -lt $deadline) {
    $proc = Get-CimInstance Win32_Process -Filter "Name='HytaleClient.exe'" -ErrorAction SilentlyContinue |
            Select-Object -First 1
    if ($proc) { break }
    Start-Sleep -Milliseconds 400
}

if (-not $proc) {
    Write-Host 'Timed out - HytaleClient.exe never started.'
    return
}

$cmd = $proc.CommandLine

# Redact anything long enough to be a JWT, key, or opaque token, plus GUIDs.
$safe = $cmd -replace '[A-Za-z0-9_\-]{24,}\.[A-Za-z0-9_\-]{16,}\.[A-Za-z0-9_\-]{16,}', '<JWT>'
$safe = $safe -replace '(?<==)[A-Za-z0-9_\-\.]{32,}', '<REDACTED>'
$safe = $safe -replace '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}', '<UUID>'

# Argument names only, for a quick summary.
$names = [regex]::Matches($cmd, '(?<=\s)--?[A-Za-z][A-Za-z0-9\-]*') |
         ForEach-Object { $_.Value } | Select-Object -Unique

$report = @(
    "captured : $(Get-Date -Format o)"
    "pid      : $($proc.ProcessId)"
    "exe      : $($proc.ExecutablePath)"
    ''
    'argument names:'
    ($names | ForEach-Object { "  $_" })
    ''
    'redacted command line:'
    "  $safe"
) -join "`n"

Set-Content -LiteralPath $out -Value $report -Encoding utf8
Write-Host $report
Write-Host "`nSaved to $out"
