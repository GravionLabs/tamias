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
| Credential shape | JSON `claudeAiOauth{accessToken, refreshToken, expiresAt (epoch ms), refreshTokenExpiresAt, scopes[], subscriptionType, subscriptionTypeSource, rateLimitTier}` | observed the same keys (Ubuntu 26.04.1, Claude Code 2.1.318): `claudeAiOauth{accessToken, expiresAt, rateLimitTier, refreshToken, refreshTokenExpiresAt, scopes[], subscriptionType, subscriptionTypeSource}`; file mode `0600`, `CLAUDE_CONFIG_DIR` unset | expected the same |
| Request | `GET https://api.anthropic.com/api/oauth/usage`, `Authorization: Bearer`, `anthropic-beta: oauth-2025-04-20` | same, confirmed 2026-11-05 | same (deferred to #20) |
| Result | 200, see fixture | 200, same top-level keys as the macOS fixture; `five_hour` and `seven_day` carry `utilization` and `resets_at` (see "Linux observations") | **pending** (#20: needs a Windows machine) |
| Command | `pwsh -NoProfile -File spikes/claude-usage.ps1` | `pwsh -NoProfile -File spikes/claude-usage.ps1` and `... -InvalidToken` | `pwsh -NoProfile -File spikes\claude-usage.ps1` and `... -InvalidToken` |

Linux observations (2026-11-05, Ubuntu 26.04.1, Claude Code 2.1.318, pwsh 7.5.0):

- `~/.claude/.credentials.json` (mode `0600`) held the token; `CLAUDE_CONFIG_DIR` was unset, so the `$CLAUDE_CONFIG_DIR` path was not exercised. Same scopes as on macOS, including `user:profile`.
- `GET /api/oauth/usage` returned 200 with the same top-level keys as `usage-ok.json`. `five_hour` and `seven_day` carry `utilization` (percent, e.g. `32.0`) and `resets_at`, plus `limit_dollars`, `used_dollars`, `remaining_dollars`, `locked_reason` (all `null`).
- `resets_at` came with the **local UTC offset** (`+01:00`), the macOS fixture has `+00:00`: the parser must accept any offset and compare instants.
- `seven_day_breakdown` was present (`as_of`, `window_started_at`, `rows[]` with `key`, `display_name`, `percent`).
- Invalid token (`-InvalidToken`): HTTP 401, `authentication_error`, "OAuth access token is invalid." (same as macOS).

macOS observations (2026-11-02, the tested subscription):

- The token carries the scopes `user:inference user:profile user:sessions:claude_code user:mcp_servers user:file_upload user:plugins`. Which scope the endpoint needs is an inference, not tested: by the Claude Code bundle (`rate_limits_available` is false without the profile scope) presumably `user:profile`; no call was made with a token lacking it.
- `anthropic-beta` is **not required** today: the same call without it (`-OmitBetaHeader`) also
  returned 200. tamias sends it anyway, like Claude Code.

### Fields used

From `crates/core/tests/fixtures/claude/usage-ok.json`:

| Field | Type / unit | Meaning |
|---|---|---|
| `five_hour.utilization` | number, percent 0-100 (e.g. `42.0`) | five-hour window used |
| `five_hour.resets_at` | ISO-8601 string with offset and microseconds (`2026-11-02T07:40:00.155067+00:00`) | end of the five-hour window |
| `seven_day.utilization` | number, percent 0-100 | seven-day window used (binding limit) |
| `seven_day.resets_at` | ISO-8601 string | end of the seven-day window |

Notes for the parser:

- `resets_at` jitters by up to about one second between calls, across the full second
  (`21:00:00.48` in one call, `20:59:59.52` in another for the same window); round to the **nearest
  minute** before comparing for a window jump. Whole seconds are not enough. After a reset the value
  can come without fractional seconds (`2026-11-09T21:00:00+00:00`).
- The seven-day window is **anchored, not rolling**: `seven_day_breakdown.window_started_at`
  (`2026-10-26T21:00:00.155085+00:00`) is exactly seven days before `seven_day.resets_at` (`2026-11-02T21:00:00.155085+00:00`), and
  `resets_at` stayed the same across calls; one observed reset confirms it (see "Window behaviour").
- `seven_day_breakdown` is `null` right after a reset; do not rely on it.
- Many other keys are present and `null` (`seven_day_opus`, `seven_day_sonnet`, codenamed keys such as
  `tangelo`, `cedar_ember`); ignore unknown keys. `limits[]` repeats the windows as integer `percent`
  with a `severity` (`normal`, `warning`); `extra_usage` / `spend` describe paid overage credits.
  None of these are needed for the first version.

### Window behaviour

Observed reset of the seven-day window (#18), macOS, two runs of `spikes/claude-usage.ps1` from a
temporary LaunchAgent around `seven_day.resets_at`, 2026-11-02 (times UTC):

| | 21:55 local (20:55Z), before | 22:05 local (21:05Z), after |
|---|---|---|
| `seven_day.utilization` | 93.0 | 0.0 |
| `seven_day.resets_at` | `2026-11-02T20:59:59.524353Z` | `2026-11-09T21:00:00Z` |
| `five_hour.utilization` | 23.0 | 23.0 |
| `five_hour.resets_at` | `2026-11-02T22:39:59.524335Z` | `2026-11-02T22:40:00.195041Z` |
| `seven_day_breakdown` | present | `null` |

- **Seven-day window: anchored.** At the reset instant `utilization` drops to 0 and `resets_at`
  moves by exactly seven days (rounded to the minute); before it, `resets_at` stays constant.
- **Five-hour window: independent.** The seven-day reset leaves it unchanged (same `utilization`,
  same `resets_at` within the jitter).
- `limits[]` follows the same values (`weekly_all` 93 → 0, `weekly_scoped` → 0, `session` unchanged).

### Failure modes

| Case | Observed |
|---|---|
| Invalid token | HTTP 401, `{"type":"error","error":{"type":"authentication_error","message":"OAuth access token is invalid."},"request_id":"req_..."}` (fixture `usage-unauthorized.json`, `request_id` replaced); same on Linux (2026-11-05) |
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

To repeat it: the plist hardcodes `/usr/local/bin/pwsh` (Intel Homebrew); on Homebrew arm64 change it to
`/opt/homebrew/bin/pwsh` (`command -v pwsh` shows the path). The job leaves `$TMPDIR/tamias-probe.out` and
`.err` behind: delete them after reading (`rm "$TMPDIR"/tamias-probe.out "$TMPDIR"/tamias-probe.err`).

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

**Real capture (Linux, Claude Code 2.1.318, 2026-11-05):** `crates/core/tests/fixtures/claude/statusline.json` (sanitized: all-zero `session_id`, `null` paths and `pr`). Captured from an interactive `claude --settings <file>` session whose settings file set `statusLine` to `spikes/statusline-dump.ps1`; `claude -p` did not run the status line command (no file written). Observed:

- `rate_limits.five_hour` and `rate_limits.seven_day` are **present** right after the session start (before the first prompt) with `used_percentage` (integer percent 0-100, e.g. `33`) and `resets_at` (Unix epoch **seconds**, e.g. `1793862000`).
- No `spend_limit` key and no `rate_limits_available` key in this capture (the subscription has no spend limit). `rate_limits_available` is only documented in the bundle; do not require it.
- Other top-level keys: `session_id`, `transcript_path`, `cwd`, `scratchpad_dir`, `effort`, `model`, `workspace`, `version`, `output_style`, `cost`, `context_window`, `exceeds_200k_tokens`, `fast_mode`, `thinking`, `rate_limits`, `pr` (the PR of the checked-out branch, here #131). `context_window.used_percentage` and `current_usage` are `null` before the first API call.
- Units differ from the OAuth endpoint (percent as integer here, float `utilization` there; epoch seconds here, ISO-8601 string there): the status line parser must convert `resets_at` to an instant and round it like the endpoint value before comparing windows.

Not tested: Windows and macOS captures, an API-key or Bedrock login (`rate_limits` absent or `null`), a window after its reset.

### Verdict

| Platform | Verdict |
|---|---|
| macOS | **Use.** OAuth usage endpoint via `/usr/bin/security`, works under launchd without a prompt. |
| Linux | **Use.** `~/.claude/.credentials.json` is read, the usage endpoint answers 200 without a prompt (Ubuntu 26.04.1, Claude Code 2.1.318, 2026-11-05). |
| Windows | **pending** (#20): needs a Windows machine; nothing was run or assumed |
| Status line fallback | **Use** as fallback. Real capture on Linux (Claude Code 2.1.318): `rate_limits.five_hour` / `seven_day` present from the session start, percent as integer, `resets_at` in epoch seconds. macOS and Windows captures pending. |

## Copilot

Probe: `spikes/copilot-usage.ps1` (#17). Fixtures: `crates/core/tests/fixtures/copilot/` (`user-ok.json`,
`user-unauthorized.json`, `billing-not-found.json`; login, id and tracking id replaced).

### Results per platform

| | macOS | Linux | Windows |
|---|---|---|---|
| OS / gh | Darwin 27.0.0 arm64 (macOS), gh 2.103.3, pwsh 7.5.4, 2026-11-03 | **pending** | **pending** |
| Personal seat | run, see below | **pending** | **pending** |
| Enterprise-managed seat | **pending** (no such seat available on the test machine) | **pending** | **pending** |
| Fine-grained PAT | **pending** (none available) | **pending** | **pending** |
| Command | `pwsh -NoProfile -File spikes/copilot-usage.ps1` (also `-InvalidToken`, `-Day 3`, `-Token <pat>`) | same | same |

Pending rows go into a follow-up PBI; nothing in them was run or assumed.

### `GET https://api.github.com/copilot_internal/user`

- **Auth:** `Authorization: Bearer <token>` with the output of `gh auth token`. Worked with both a classic PAT
  (`ghp_`, via `GH_TOKEN`) and the gh OAuth token (`gho_`, keyring, scopes `admin:public_key, gist, project,
  read:org, repo, workflow`): the gh token suffices, no Copilot-specific scope was needed.
- **Headers:** `Accept: application/vnd.github+json` and a `User-Agent` were sent. No `Editor-Version`
  or other Copilot headers were needed (not tested without `Accept`). Not rate-limit-relevant: counted
  against the normal 5000/h core limit.
- **Fields used** (`user-ok.json`):

| Field | Meaning |
|---|---|
| `copilot_plan`, `access_type_sku` | plan (`individual`, `free_limited_copilot` on the tested seat) |
| `quota_snapshots.premium_interactions.entitlement` | monthly quota (premium requests) |
| `quota_snapshots.premium_interactions.remaining` / `quota_remaining` | left this month (`quota_remaining` is a float) |
| `quota_snapshots.premium_interactions.percent_remaining` | 0-100 |
| `quota_snapshots.premium_interactions.unlimited` | true for unlimited plans |
| `quota_snapshots.premium_interactions.has_quota` | false when the plan has no premium quota |
| `quota_snapshots.premium_interactions.overage_permitted` / `overage_count` | paid overage |
| `quota_reset_date` (`2026-12-01`), `quota_reset_date_utc` (`2026-12-01T00:00:00Z`) | reset, the 1st of next month |
| `enterprise_list[]`, `organization_login_list[]` | empty here; expected to be filled for managed seats (not tested) |

- **Not usable:** `quota_snapshots.*.quota_reset_at` is `0`; use `quota_reset_date_utc`. `chat` and
  `completions` snapshots exist too (200 and 2000 on the free plan). `token_based_billing: true` and
  `credits_used` appear; their meaning for paid plans is not verified. Ignore unknown keys.
- **Tested seat limit:** the owner's tested seat is a personal **free** plan, so `premium_interactions` reads
  `entitlement 0, has_quota false`. The paid-plan shape (non-zero entitlement, remaining counting down,
  `unlimited: true`) was **not observed**; field names are taken from this response and the same
  object type. The first paid or managed seat run must confirm them.
- **Failure modes:** invalid token: HTTP 401 `{"message":"Bad credentials",...}` (`user-unauthorized.json`).
  Other cases (account without Copilot, 403 for managed seats, 404) not observed.
- **Verdict (macOS, personal free seat):** **Use as primary**; works with the gh token, undocumented, so
  parse tolerantly. Quota semantics for paid/managed seats **pending**. Implemented: tamias polls it
  with the `gh` token; a seat with `has_quota` false or `unlimited` true is stored as `unavailable`.

### `GET /users/{username}/settings/billing/premium_request/usage?year=&month=[&day=]`

- **Auth tried:** classic PAT (scopes incl. `repo`, `read:org`, no `user`) and gh OAuth token, headers
  `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2022-11-28`, `year=2026&month=11` and
  with `day=3`. `{username}` came from `GET /user`.
- **Result:** HTTP 404 `{"message":"Not Found",...}` for both tokens (`billing-not-found.json`), also with `day`.
  GitHub returns 404 instead of 403 when the token lacks access, so this does not tell apart "no billed
  usage", "missing permission" and "not available for free seat". Fine-grained PAT (documented permission
  "Plan" read) was **not tried**; the `X-Accepted-GitHub-Permissions` header was empty.
- **Fields used:** none; no successful response was seen.
- **Verdict:** **not usable on the owner's seat as tested; dropped for v1.** The billing fallback is unconfirmed: it
  returned nothing for a free personal seat with a classic PAT and a gh token, and nothing was run
  for a paid personally billed or enterprise-managed seat (pending). Do not build on it for v1;
  `copilot_internal/user` is the only source, and `stale` carries the risk (ADR 0001).
