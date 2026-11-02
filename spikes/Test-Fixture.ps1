#Requires -Version 7.0
<#
.SYNOPSIS
    THROWAWAY SPIKE (#16) - sanitizer check for committed fixtures. Run before every commit of fixtures.
.DESCRIPTION
    Scans every file under crates/core/tests/fixtures/** (or -Path) and fails (exit 1) on JWT-like
    strings, sk-/gho_/ghp_/ghu_ token prefixes, e-mail addresses and UUIDs. Prints file, line and
    the pattern name, never the matched value.
.PARAMETER Path
    Root to scan. Defaults to crates/core/tests/fixtures relative to the repo root.
.PARAMETER SelfTest
    Check the patterns against built-in positive and negative samples, then exit.
.EXAMPLE
    pwsh -NoProfile -File spikes/Test-Fixture.ps1
#>
[CmdletBinding()]
param(
    [string]$Path = (Join-Path $PSScriptRoot '..' 'crates' 'core' 'tests' 'fixtures'),
    [switch]$SelfTest
)
$ErrorActionPreference = 'Stop'

$patterns = [ordered]@{
    jwt          = 'eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]*'
    token_prefix = '(?<![A-Za-z0-9])(sk-|gho_|ghp_|ghu_)[A-Za-z0-9_-]+'
    email        = '[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+'
    uuid         = '(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b'
}

function Find-Secret([string]$Text) {
    foreach ($name in $patterns.Keys) { if ($Text -match $patterns[$name]) { $name } }
}

if ($SelfTest) {
    $hits = @{
        'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig' = 'jwt'
        '"token":"sk-ant-oat01-abc"'               = 'token_prefix'
        'ghp_abcdef123'                            = 'token_prefix'
        'gho_x1'                                   = 'token_prefix'
        'ghu_x1'                                   = 'token_prefix'
        'mail me: a.b+c@example.co.uk'             = 'email'
        '123e4567-E89B-12d3-a456-426614174000'     = 'uuid'
    }
    $misses = @('{"utilization": 45.0}', 'task-runner', '"resets_at":"2026-11-02T08:40:00+01:00"',
        'req_REDACTED', '', '@ alone', 'deadbeef-1234')
    $fail = 0
    foreach ($k in $hits.Keys) {
        if ((Find-Secret $k) -notcontains $hits[$k]) { Write-Host "MISS expected $($hits[$k]): $k"; $fail++ }
    }
    foreach ($m in $misses) {
        $found = Find-Secret $m
        if ($found) { Write-Host "FALSE POSITIVE ($($found -join ',')): $m"; $fail++ }
    }
    Write-Host "selftest: $($hits.Count + $misses.Count - $fail) passed, $fail failed"
    exit ([int]($fail -gt 0))
}

if (-not (Test-Path -LiteralPath $Path)) { Write-Host "no fixtures at $Path"; exit 0 }
$findings = 0
foreach ($file in Get-ChildItem -LiteralPath $Path -Recurse -File) {
    $n = 0
    foreach ($line in Get-Content -LiteralPath $file.FullName) {
        $n++
        foreach ($name in (Find-Secret $line)) {
            Write-Host "FAIL ${name}: $($file.FullName):$n"
            $findings++
        }
    }
}
if ($findings) { Write-Host "Test-Fixture: $findings finding(s)"; exit 1 }
Write-Host 'Test-Fixture: green'
