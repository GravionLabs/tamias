#Requires -Version 7.0
<#
.SYNOPSIS
    THROWAWAY SPIKE (#17) - not product code, do not reuse. Probes the Copilot usage sources.
.DESCRIPTION
    Takes the token from `gh auth token` (or -Token / $env:TAMIAS_SPIKE_TOKEN for a PAT) and calls
      1. GET https://api.github.com/copilot_internal/user
      2. GET /users/{login}/settings/billing/premium_request/usage?year=&month=[&day=]
    Prints the HTTP status, the response headers that matter (scopes, rate limit) and the JSON body
    with the login, ids, e-mail and URLs masked. The token is never printed.
.PARAMETER Token
    Use this token instead of `gh auth token` (classic or fine-grained PAT).
.PARAMETER InvalidToken
    Send an invalid bearer to capture the unauthorized response.
.PARAMETER Day
    Add day= to the billing call.
.PARAMETER OutDir
    Also write the masked bodies as copilot-user.json and copilot-billing.json to this directory.
.EXAMPLE
    pwsh -NoProfile -File spikes/copilot-usage.ps1
#>
[CmdletBinding()]
param(
    [string]$Token,
    [switch]$InvalidToken,
    [int]$Day,
    [string]$OutDir
)
$ErrorActionPreference = 'Stop'

if (-not $Token) { $Token = $env:TAMIAS_SPIKE_TOKEN }
if (-not $Token) { $Token = (& gh auth token 2>$null) -join ''; if ($LASTEXITCODE -ne 0) { throw 'gh auth token failed.' } }
if ($InvalidToken) { $Token = 'ghp_invalid0000000000000000000000000000' }
$kind = switch -Regex ($Token) { '^github_pat_' { 'fine-grained' } '^ghp_' { 'classic' } '^gho_' { 'oauth(gh)' } '^ghu_' { 'app-user' } default { 'other' } }
Write-Host "gh=$((& gh --version | Select-Object -First 1)) os=$([Runtime.InteropServices.RuntimeInformation]::OSDescription) tokenKind=$kind"

function Invoke-Api([string]$Uri, [hashtable]$Extra = @{}) {
    $h = @{ Authorization = "Bearer $Token"; Accept = 'application/vnd.github+json'; 'User-Agent' = 'tamias-spike/0' } + $Extra
    $r = Invoke-WebRequest -Uri $Uri -Headers $h -Method Get -TimeoutSec 15 -SkipHttpErrorCheck
    $scopes = $r.Headers['X-OAuth-Scopes']; $accepted = $r.Headers['X-Accepted-GitHub-Permissions']
    Write-Host "status=$($r.StatusCode) oauthScopes=[$($scopes -join ',')] acceptedPermissions=[$($accepted -join ';')] rateRemaining=$($r.Headers['X-RateLimit-Remaining'])"
    [string]$r.Content
}

function Mask([string]$Body, [string]$Login) {
    $b = $Body
    if ($Login) { $b = $b -replace [regex]::Escape($Login), 'octocat' }
    $b = $b -replace '[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+', 'user@example.com'
    $b = $b -replace '(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b', '00000000-0000-0000-0000-000000000000'
    $b = $b -replace '("(?:id|user_id|analytics_tracking_id|organization_login_list|login)"\s*:\s*)("[^"]*"|\d+)', '$1"REDACTED"'
    $b
}

$login = ''
$userBody = Invoke-Api 'https://api.github.com/user'
try { $login = ($userBody | ConvertFrom-Json).login } catch { }

Write-Host '--- copilot_internal/user'
$cp = Invoke-Api 'https://api.github.com/copilot_internal/user'
$cpMasked = Mask $cp $login
try { $cpMasked | ConvertFrom-Json | ConvertTo-Json -Depth 20 } catch { $cpMasked }

Write-Host '--- billing premium_request/usage'
$now = Get-Date
$q = "year=$($now.Year)&month=$($now.Month)"
if ($Day) { $q += "&day=$Day" }
$bill = Invoke-Api "https://api.github.com/users/$login/settings/billing/premium_request/usage?$q" @{ 'X-GitHub-Api-Version' = '2022-11-28' }
$billMasked = Mask $bill $login
try { $billMasked | ConvertFrom-Json | ConvertTo-Json -Depth 20 } catch { $billMasked }

if ($OutDir) {
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    Set-Content -LiteralPath (Join-Path $OutDir 'copilot-user.json') -Value $cpMasked -NoNewline
    Set-Content -LiteralPath (Join-Path $OutDir 'copilot-billing.json') -Value $billMasked -NoNewline
}
$Token = $null
