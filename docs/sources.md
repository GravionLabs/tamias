# Usage sources

Findings of the source spikes (ADR 0001, follow-up "Spike, on all three platforms"). The probes live
under `spikes/` and are throwaway; fixtures live under `crates/core/tests/fixtures/`.
Run `pwsh -NoProfile -File spikes/Test-Fixture.ps1` before committing any fixture.

## Claude

### Where the endpoint comes from

Read from the installed Claude Code bundle, version **2.1.316** (macOS arm64 native binary,
`~/.local/share/claude/versions/2.1.316`, inspected with `strings`):

- `BASE_API_URL` = `https://api.anthropic.com`; `/usage` calls `fetchUtilization`, which builds
  `GET /api/oauth/usage` (variants with `?at_wall=1&skip_spend=1` or `?cedar_ember=1&skip_spend=1`
  for other UI surfaces; the plain path is enough for tamias).
- Request: client timeout 5 s, headers `Authorization: Bearer <accessToken>`,
  `anthropic-beta: oauth-2025-04-20` (constant `jc` in the bundle), `Content-Type: application/json`.
  On 401 Claude Code refreshes the OAuth token (`refreshOAuth: true`) and retries once.
- Token refresh endpoint (not used by tamias): `https://platform.claude.com/v1/oauth/token`.

### Request and auth

| | macOS | Linux | Windows |
|---|---|---|---|
| Credential source | login Keychain, generic password, service `Claude Code-credentials`, read via `/usr/bin/security find-generic-password -s "Claude Code-credentials" -w` | `~/.claude/.credentials.json` (or `$CLAUDE_CONFIG_DIR/.credentials.json`) | `%USERPROFILE%\.claude\.credentials.json` (or `%CLAUDE_CONFIG_DIR%`) |
| Credential shape | JSON `claudeAiOauth{accessToken, refreshToken, expiresAt (epoch ms), refreshTokenExpiresAt, scopes[], subscriptionType, subscriptionTypeSource, rateLimitTier}` | expected the same (deferred to #20) | expected the same (deferred to #20) |
| Request | `GET https://api.anthropic.com/api/oauth/usage`, `Authorization: Bearer`, `anthropic-beta: oauth-2025-04-20` | same (deferred to #20) | same (deferred to #20) |
| Result | 200, see fixture | **deferred to #20** | **deferred to #20** |
| Command | `pwsh -NoProfile -File spikes/claude-usage.ps1` | `pwsh -NoProfile -File spikes/claude-usage.ps1` and `... -InvalidToken` | `pwsh -NoProfile -File spikes\claude-usage.ps1` and `... -InvalidToken` |

macOS observations (2026-11-02, the tested subscription):

- The token carries the scopes `user:inference user:profile user:sessions:claude_code user:mcp_servers user:file_upload user:plugins`. Which scope the endpoint needs is an inference, not tested: by the Claude Code bundle (`rate_limits_available` is false without the profile scope) presumably `user:profile`; no call was made with a token lacking it.
- `anthropic-beta` is **not required** today: the same call without it (`-OmitBetaHeader`) also
  returned 200. tamias sends it anyway, like Claude Code.

### Fields used

From `crates/core/tests/fixtures/claude/usage-ok.json`:

| Field | Type / unit | Meaning |
|---|---|---|
| `five_hour.utilization` | number, percent 0-100 (e.g. `42.0`) | five-hour window used |
| `five_hour.resets_at` | ISO-8601 string with offset and microseconds (`2026-11-02T08:40:00.155067+01:00`) | end of the five-hour window |
| `seven_day.utilization` | number, percent 0-100 | seven-day window used (binding limit) |
| `seven_day.resets_at` | ISO-8601 string | end of the seven-day window |

Notes for the parser:

- `resets_at` jitters in the sub-second part between calls (`.155067` vs. `.174235` for the same
  window); round to whole seconds (or minutes) before comparing for a window jump.
- The seven-day window is **anchored, not rolling**: `seven_day_breakdown.window_started_at`
  (`2026-10-26T22:00`) is exactly seven days before `seven_day.resets_at` (`2026-11-02T22:00`), and
  `resets_at` stayed the same across calls. This answers the ADR 0001 open question for this account.
- Many other keys are present and `null` (`seven_day_opus`, `seven_day_sonnet`, codenamed keys such as
  `tangelo`, `cedar_ember`); ignore unknown keys. `limits[]` repeats the windows as integer `percent`
  with a `severity` (`normal`, `warning`); `extra_usage` / `spend` describe paid overage credits.
  None of these are needed for the first version.

### Failure modes

| Case | Observed |
|---|---|
| Invalid token | HTTP 401, `{"type":"error","error":{"type":"authentication_error","message":"OAuth access token is invalid."},"request_id":"req_..."}` (fixture `usage-unauthorized.json`, `request_id` replaced) |
| Expired token | not observed (the token was valid). `expiresAt` is in the credential, so tamias checks it before the call; by the bundle, Claude Code treats an expired token as 401 and refreshes. |
| Missing Keychain item | run on macOS 2026-11-02 with a scratch copy of the probe whose service name was replaced by the non-existent `tamias-spike-missing` (real Keychain item untouched): `security` exited 44, the probe threw `Keychain item 'tamias-spike-missing' not readable (security exit 44).` and exited 1, before any HTTP call. tamias should then report `stale` (intended, not tested) |
| Missing credentials file (Linux/Windows) | not tested (from probe code): the probe throws `Credentials file not found: <path>`; tamias should report `stale` |
| No subscription / API key login | expected: no `claudeAiOauth` entry; not tested |

**Refresh:** tamias must **not** refresh the token itself. A refresh rotates the refresh token, and
Claude Code would then hold a dead one. On an expired or rejected token tamias marks Claude `stale`
and waits until Claude Code (next session) has refreshed the credential.

### launchd and Keychain (macOS)

Ran `spikes/launchd-probe.plist` once as a temporary LaunchAgent (`com.gravionlabs.tamias.probe`,
`launchctl bootstrap gui/$(id -u)`, then `bootout` and plist removed). Result: **no Keychain dialog**;
the job finished in about 2 s with exit code 0, stdout had `status=200` and the usage JSON, stderr
was empty. Likely reason (not verified against the item's ACL): Claude Code writes the item via
`/usr/bin/security`, so `security` is on the item's access list and any caller of it in the user's
GUI session gets the secret without a prompt. A binary that reads the item through the Security
framework directly is expected not to be on that list and to prompt; the collector should therefore
shell out to `/usr/bin/security` (untested alternative).

### Status line

Source of the schema: the statusLine documentation embedded in Claude Code 2.1.316:

```
"rate_limits": {           // Optional: only for subscribers, after the first API response
  "five_hour": { "used_percentage": number /* 0-100 */, "resets_at": number /* Unix epoch seconds */ },
  "seven_day": { "used_percentage": number, "resets_at": number },
  "spend_limit": { ... }   // only behind a gateway with a spend limit
}
```

Each window is present only while the API reports it and its `resets_at` has not passed. The
statusLine input also carries `rate_limits_available` (false for API key, Bedrock, Vertex or missing
profile scope; `rate_limits` is then null).

**Real capture: deferred to #20.** No `statusline.json` fixture is committed yet. Steps for #20
(temporary, revert afterwards):

1. Add to `~/.claude/settings.json`:
   `"statusLine": { "type": "command", "command": "pwsh -NoProfile -File <abs-path>/spikes/statusline-dump.ps1" }`
2. Start `claude`, send one prompt; the status line shows `tamias-spike 5h=..% 7d=..%` or
   `rate_limits=absent`.
3. Copy `$TMPDIR/statusline.json` (Windows: `%TEMP%\statusline.json`) to
   `crates/core/tests/fixtures/claude/statusline.json`, replace `session_id`, `transcript_path`,
   `cwd`/workspace paths and any e-mail, run `spikes/Test-Fixture.ps1`.
4. Record `claude --version`, presence and units of `rate_limits.five_hour` / `seven_day` here.
5. Remove the `statusLine` entry again.

### Verdict

| Platform | Verdict |
|---|---|
| macOS | **Use.** OAuth usage endpoint via `/usr/bin/security`, works under launchd without a prompt. |
| Linux | deferred to #20 (`pwsh -NoProfile -File spikes/claude-usage.ps1`, then `-InvalidToken`) |
| Windows | deferred to #20 (`pwsh -NoProfile -File spikes\claude-usage.ps1`, then `-InvalidToken`) |
| Status line fallback | schema confirmed from the 2.1.316 bundle; real capture deferred to #20 |
