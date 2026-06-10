<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **Vibes Left** (529 symbols, 989 relationships, 36 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/Vibes Left/context` | Codebase overview, check index freshness |
| `gitnexus://repo/Vibes Left/clusters` | All functional areas |
| `gitnexus://repo/Vibes Left/processes` | All execution flows |
| `gitnexus://repo/Vibes Left/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->

<!-- tauri:start -->

## Tauri Development Standards

This project uses **Tauri v2**. All IPC, security, and state-management decisions are governed by the design document at [`TAURI_DESIGN.md`](./TAURI_DESIGN.md). Read it before writing or reviewing any Rust backend or frontend IPC code.

Credential/token handling in `src-tauri/src/usage/` is governed by [`TOKEN_LIFECYCLE.md`](./TOKEN_LIFECYCLE.md). Read it before touching any connector, `TokenManager`, or `CredsCache` code.

### Always Do

- **MUST gate `mcp-bridge` behind `#[cfg(debug_assertions)]`** — it must never ship in a production binary.
- **MUST define a typed `AppError` enum** and return `Result<T, AppError>` from every `#[tauri::command]`.
- **MUST validate user-controlled URLs and paths** before passing them to `opener` or any shell/FS operation.
- **MUST mirror Rust payload structs as TypeScript interfaces** in `src/bindings/` when adding or changing a command.
- **MUST use capability files** (`src-tauri/capabilities/`) to grant permissions — never enable `dangerousRemoteUrlIpcAccess` without explicit scoping.

### Never Do

- NEVER set `"csp": null` in `tauri.conf.json` for production builds.
- NEVER hold a `std::sync::Mutex` lock across an `await` point — use `tokio::sync::Mutex` instead.
- NEVER interpolate raw user input into shell commands, SQL, or file paths inside a command handler.
- NEVER grant broad permissions (`fs:default`, `fs:allow-write`) without a matching `deny` scope.

### Key Sections in TAURI_DESIGN.md

| Topic                              | Section                    |
| ---------------------------------- | -------------------------- |
| IPC command patterns & naming      | §2 IPC Command Standards   |
| Typed error enum                   | §3 Error Handling          |
| Capabilities / ACL / CSP           | §4 Security                |
| State management & locking         | §5 State Management        |
| Event system                       | §6 Event System            |
| Plugin safety (MCP bridge, opener) | §7 Plugin Safety Protocols |
| Pre-release checklist              | §9 Pre-Release Checklist   |

<!-- tauri:end -->
