#Requires -Version 7.0
<#
.SYNOPSIS
    THROWAWAY SPIKE (#16) - temporary Claude Code statusLine command that dumps its stdin JSON.
.DESCRIPTION
    Claude Code pipes a JSON object to the statusLine command on every refresh. This script writes
    the latest object to <OutDir>/statusline.json (default: the system temp dir) and prints a short
    status line. Install it only temporarily (see docs/sources.md) and remove it afterwards.
.PARAMETER OutDir
    Directory for statusline.json.
.EXAMPLE
    "statusLine": { "type": "command", "command": "pwsh -NoProfile -File /abs/path/spikes/statusline-dump.ps1" }
#>
[CmdletBinding()]
param([string]$OutDir = [System.IO.Path]::GetTempPath())
$ErrorActionPreference = 'Stop'

$raw = [Console]::In.ReadToEnd()
Set-Content -LiteralPath (Join-Path $OutDir 'statusline.json') -Value $raw -NoNewline
$rl = try { ($raw | ConvertFrom-Json).rate_limits } catch { $null }
if ($rl) {
    "tamias-spike 5h=$($rl.five_hour.used_percentage)% 7d=$($rl.seven_day.used_percentage)%"
} else {
    'tamias-spike rate_limits=absent'
}
