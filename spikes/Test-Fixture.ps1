#Requires -Version 7.0
<#
.SYNOPSIS
    THROWAWAY SPIKE (#16) - sanitizer check for committed fixtures. Run before every commit of fixtures.
.DESCRIPTION
    Scans every file under crates/core/tests/fixtures/** (or -Path) and fails (exit 1) on JWT-like
    strings, sk-/gho_/ghp_/ghu_ token prefixes, e-mail addresses, UUIDs (the all-zero placeholder is
    allowed), 32-hex ids, req_/org_/acct_ ids (req_REDACTED is allowed), home paths (/Users/<name>,
    /home/<name>, C:\Users\<name>; <name> REDACTED or user is allowed) and session/transcript paths.
    Prints file, line and the pattern name, never the matched value.
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
    uuid         = '(?i)\b(?!0{8}-0{4}-0{4}-0{4}-0{12}\b)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b'
    hex32        = '(?i)(?<![0-9a-z])(?!0{32}(?![0-9a-z]))[0-9a-f]{32}(?![0-9a-z])'
    prefixed_id  = '(?<![A-Za-z0-9])(req|org|acct)_(?!REDACTED)[A-Za-z0-9]{6,}'
    home_path    = '(?i)(/(Users|home)/(?!(REDACTED|user)([/\\"\s]|$))[^/\\"\s]+|[A-Z]:\\{1,2}Users\\{1,2}(?!(REDACTED|user)([/\\"\s]|$))[^\\"\s]+)'
    session_path = '(?i)(\.claude[/\\]+(projects|sessions)\b|\.jsonl\b)'
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
        '"session_id":"0123456789abcdef0123456789ABCDEF"' = 'hex32'
        'req_011CUabcDEF123'                       = 'prefixed_id'
        '"org":"org_01AbCdEf"'                     = 'prefixed_id'
        'acct_9f8e7d6c'                            = 'prefixed_id'
        '"cwd":"/home/alice/src/app"'              = 'home_path'
        '/Users/bob/Library'                       = 'home_path'
        '"cwd":"C:\\Users\\carol\\src"'            = 'home_path'
        'C:\Users\carol\.claude'                  = 'home_path'
        '"transcript_path":"/x/.claude/projects/p/s.jsonl"' = 'session_path'
        '~/.claude/sessions/abc'                   = 'session_path'
    }
    $misses = @('{"utilization": 45.0}', 'task-runner', '"resets_at":"2026-11-02T08:40:00+01:00"',
        'req_REDACTED', '', '@ alone', 'deadbeef-1234',
        '00000000-0000-0000-0000-000000000000', '00000000000000000000000000000000', 'org_REDACTED',
        '/home/user/proj', '/Users/REDACTED/proj', 'C:\Users\user\proj', '/usr/local/bin', 'the home page',
        '"transcript_path":"REDACTED"', '1793000000')
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
