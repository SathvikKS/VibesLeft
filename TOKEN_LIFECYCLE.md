# Usage Connector Token Lifecycle Standard

Governs all credential handling in `src-tauri/src/usage/` (claude, antigravity, codex, and any future provider). The lifecycle is implemented **once** — in `token_manager.rs` (`TokenManager<S>` + `CredentialSource` trait) and `mod.rs::run_report_flow` — and connectors only supply provider specifics. Never re-implement any part of this flow inside a connector.

Last verified against the implementation: 2026-06-10.

## Core principles

1. **Assume every provider rotates its refresh token** (worst case; Claude provably does). Design for rotation even if a provider doesn't rotate.
2. **The cache (`CredsCache`) is the source of truth for the refresh token.** The original keychain/file is bootstrap + last-resort only, and is never written back to.
3. **5xx/network during refresh must never destroy the cached RT.** Keep the cache, serve the stale usage report if one exists. Clearing the cache (or overwriting it from a stale source) on a transient error discards the only live token and forces a needless re-login.
4. **Never cache known-expired credentials.** A dead source surfaces exactly one `ReauthRequired`.
5. **Refresh is non-idempotent** → serialized with `tokio::sync::Mutex` held across the await (never `std::sync::Mutex` across an await point, per `TAURI_DESIGN.md`).
6. `refresh()` impls persist `response.refresh_token.unwrap_or(sent_token)` so rotation-safety holds regardless of provider behavior.

## The flow

```
generate_report(force_refresh)
  │
  ├─ usage-report cache fresh (< 5 min) and not forced → serve it (cached: true)
  │
  ├─ get_valid_token()                          [proactive]
  │    ├─ cached AT valid with > 15 min ttl → use it
  │    ├─ near expiry / expired + RT present → refresh; persist new AT/RT/expiry
  │    │     ├─ token endpoint 4xx → RT dead → clear cache → re-read source
  │    │     └─ 5xx/network → keep cache; serve cached AT if ttl > 0, else Internal
  │    └─ no cache → fetch_from_source(); cache only if alive
  │
  ├─ fetch usage stats with the token
  │    ├─ 401/403 → recover_from_rejection(rejected_token)   [reactive]
  │    │     ├─ cache holds a DIFFERENT token with healthy ttl → concurrent
  │    │     │   refresh already happened; use it
  │    │     ├─ otherwise refresh from cached RT (same rules as above)
  │    │     ├─ RT dead → clear cache → re-read source → if source creds also
  │    │     │   dead/expired → ReauthRequired (tell user to re-login)
  │    │     └─ retry the fetch exactly once; second 401/403 → ReauthRequired
  │    └─ other 4xx / 5xx / network → Internal
  │
  └─ every Internal path (token acquisition, recovery, fetch) falls back to the
     stale usage report via stale_report_or() when one exists
```

## Status → error mapping

| Where | Status | Maps to | Effect |
|---|---|---|---|
| Usage API | 401, 403 | `Unauthorized` | triggers reactive recovery |
| Usage API | other 4xx (e.g. 429) | `Internal` | NO refresh/recovery; stale-report fallback |
| Usage API | 5xx / network | `Internal` | stale-report fallback |
| Token endpoint | any 4xx | `ReauthRequired` | RT dead → clear cache → source fallback |
| Token endpoint | 5xx / network | `Internal` | cache preserved |

`error.rs` maps `reqwest::Error` → `Internal` (or `Unauthorized` for 401/403 statuses), so transport errors land on the stale-report fallbacks.

## Semantics that must be preserved

- **`recover_from_rejection(rejected_token)`** — the token comparison is load-bearing. It both dedupes concurrent recoveries (another task already refreshed → reuse) **and** guarantees a revoked-but-locally-unexpired AT still gets refreshed instead of being returned verbatim.
- **`expires_at_ms == 0` means "unknown expiry — use until rejected"** (codex), NOT "expired". Don't "fix" comparisons without honoring this.
- **`CachedCredentials.account_id`** exists solely for codex (serde-default, empty for claude/antigravity).
- **`CredsCache` backend**: OS keychain in release; `$TMPDIR/vibes-left-token-cache-<provider>.json` in debug — unsigned debug binaries would otherwise trigger the macOS keychain ACL prompt on every recompile.
- Cache write/clear failures are logged (`[token_manager] …`) but never abort the flow.

## Adding a new provider

Implement `CredentialSource` only:

- `provider()` — cache key.
- `fetch_from_source()` — read the provider's keychain entry / auth file. Missing or structurally invalid creds → `ReauthRequired`; access failures → `Internal`.
- `supports_refresh()` + `refresh()` — token-endpoint exchange following the status mapping above. Omit `refresh()` while unsupported (default returns `Internal`).

Then delegate `generate_report` to `run_report_flow`. No connector-level retry, cache, or status logic.

## Pending follow-ups

- **Codex refresh-token support**: implement `CodexCredSource::refresh()` and flip `supports_refresh()` to `true`; the lifecycle needs no other changes.
- The `[timing] …` `eprintln!` instrumentation currently ships in release builds; gate it behind `#[cfg(debug_assertions)]` once no longer needed.
