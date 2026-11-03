#Requires -Version 7.0
<#
.SYNOPSIS
    THROWAWAY SPIKE (#16) - not product code, do not reuse. Probes the Claude usage endpoint.
.DESCRIPTION
    Reads the Claude Code OAuth token (macOS: login Keychain item "Claude Code-credentials";
    Linux/Windows: ~/.claude/.credentials.json), calls GET https://api.anthropic.com/api/oauth/usage
    (the request behind Claude Code's /usage, found in the 2.1.316 bundle) and prints the HTTP status
    and the JSON body. The token is never printed; only its expiry is shown.
.PARAMETER InvalidToken
    Send a syntactically plausible but invalid bearer instead of the real token to capture the
    unauthorized response.
.PARAMETER OmitBetaHeader
    Leave out the anthropic-beta header to check whether it is required.
.PARAMETER OutFile
    Also write the raw response body to this path (sanitize before committing it as a fixture).
.EXAMPLE
    pwsh -NoProfile -File spikes/claude-usage.ps1
.EXAMPLE
    pwsh -NoProfile -File spikes/claude-usage.ps1 -InvalidToken
#>
[CmdletBinding()]
param(
    [switch]$InvalidToken,
    [switch]$OmitBetaHeader,
    [string]$OutFile
)
$ErrorActionPreference = 'Stop'

function Get-ClaudeCredentialJson {
    if ($IsMacOS) {
        $json = & /usr/bin/security find-generic-password -s 'Claude Code-credentials' -w 2>$null
        if ($LASTEXITCODE -ne 0 -or -not $json) {
            throw "Keychain item 'Claude Code-credentials' not readable (security exit $LASTEXITCODE)."
        }
        return ($json -join '')
    }
    $homeDir = if ($IsWindows) { $env:USERPROFILE } else { $HOME }
    $configDir = if ($env:CLAUDE_CONFIG_DIR) { $env:CLAUDE_CONFIG_DIR } else { Join-Path $homeDir '.claude' }
    $path = Join-Path $configDir '.credentials.json'
    if (-not (Test-Path -LiteralPath $path)) { throw "Credentials file not found: $path" }
    return Get-Content -LiteralPath $path -Raw
}

$source = if ($IsMacOS) { 'keychain' } else { 'credentials.json' }
$oauth = (Get-ClaudeCredentialJson | ConvertFrom-Json).claudeAiOauth
if (-not $oauth -or -not $oauth.accessToken) { throw "No claudeAiOauth.accessToken in $source." }

$expires = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$oauth.expiresAt)
$expired = $expires -lt [DateTimeOffset]::UtcNow
Write-Host "source=$source tokenExpiresAt=$($expires.ToString('o')) expired=$expired scopes=$($oauth.scopes -join ',')"

$bearer = if ($InvalidToken) { 'sk-ant-oat01-invalid' } else { $oauth.accessToken }
$headers = @{
    'Authorization'  = "Bearer $bearer"
    'anthropic-beta' = 'oauth-2025-04-20'
    'Content-Type'   = 'application/json'
    'User-Agent'     = 'tamias-spike/0'
}
$bearer = $null
if ($OmitBetaHeader) { $headers.Remove('anthropic-beta') }

$resp = Invoke-WebRequest -Uri 'https://api.anthropic.com/api/oauth/usage' -Headers $headers `
    -Method Get -TimeoutSec 15 -SkipHttpErrorCheck
$headers = $null
Write-Host "status=$($resp.StatusCode)"
$body = [string]$resp.Content
if ($OutFile) { Set-Content -LiteralPath $OutFile -Value $body -NoNewline }
try { $body | ConvertFrom-Json | ConvertTo-Json -Depth 20 } catch { $body }
