# Tauri Design Document — Vibes Left

> **Tauri version:** 2.x (`tauri = "2"`)
> **Last updated:** 2026-06-09

---

## 1. Architecture Overview

```
Frontend (React/Vite)          Backend (Rust)
┌─────────────────────┐        ┌──────────────────────────────┐
│  invoke() / emit()  │◄──────►│  #[tauri::command] handlers  │
│  @tauri-apps/api    │  IPC   │  tauri::Builder + plugins    │
└─────────────────────┘        └──────────────────────────────┘
         │                                  │
         │ webview                          │ OS / FS / system
         ▼                                  ▼
  Content Security Policy          Capabilities ACL
  (tauri.conf.json)                (src-tauri/capabilities/)
```

All communication between the webview and the Rust backend travels through Tauri's IPC layer. There is **no direct DOM → OS path**; every privilege must be declared in a capability file and granted by the ACL.

---

## 2. IPC Command Standards

### 2.1 Defining Commands

```rust
// ✅ Good — typed payload, typed error, explicit Result
#[tauri::command]
async fn fetch_tracks(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Track>, AppError> {
    state.db.search(&query).await.map_err(AppError::from)
}

// ❌ Bad — opaque String error, no async, panic-on-lock
#[tauri::command]
fn fetch_tracks(query: String) -> String {
    todo!()
}
```

Rules:
- Every command returns `Result<T, E>` — never panics, never returns a raw `String` as the error.
- Use `async fn` for any I/O-bound work; use `tokio::sync::Mutex` (not `std::sync::Mutex`) for state held across await points.
- Deserialize inputs through serde — never build shell commands or SQL from raw strings.

### 2.2 Registering Commands

Register all commands in `lib.rs` via `tauri::generate_handler!`. Never call handlers directly from Rust except in tests.

```rust
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            fetch_tracks,
            update_playlist,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 2.3 Calling Commands from the Frontend

```ts
import { invoke } from "@tauri-apps/api/core";

// ✅ Always type the return and handle the rejection
const tracks = await invoke<Track[]>("fetch_tracks", { query });

// ❌ Never ignore the Promise rejection
invoke("fetch_tracks", { query });
```

### 2.4 Naming Conventions

| Layer | Convention | Example |
|---|---|---|
| Rust command fn | `snake_case` | `fetch_tracks` |
| Frontend invoke string | `snake_case` (matches Rust) | `"fetch_tracks"` |
| Capability permission ID | `core:<command>` or `plugin:<cmd>` | `"core:default"` |
| Event names | `kebab-case` | `"track-updated"` |

---

## 3. Error Handling

### 3.1 Typed Error Enum

Define a project-wide error type in `src-tauri/src/error.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    NotFound(String),
    Unauthorized(String),
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(m) | Self::Unauthorized(m) | Self::Internal(m) => write!(f, "{m}"),
        }
    }
}
```

`#[serde(tag = "kind")]` means the frontend receives `{ kind: "NotFound", message: "..." }` — discriminatable without string parsing.

### 3.2 Frontend Error Handling

```ts
try {
    const result = await invoke<Track[]>("fetch_tracks", { query });
} catch (err) {
    const e = err as { kind: string; message: string };
    if (e.kind === "NotFound") { /* show empty state */ }
    else { reportError(e.message); }
}
```

### 3.3 Never Expose Internal Details

- Do **not** forward raw `std::io::Error` strings to the frontend — they can leak file paths.
- Map all third-party errors to `AppError::Internal` with a sanitized message.

---

## 4. Security: Capabilities & Permissions (ACL)

Tauri v2 replaces the v1 allowlist with a capability-based ACL. Every privilege must be granted explicitly.

### 4.1 Capability File Structure

```
src-tauri/capabilities/
  default.json      ← main window, minimal trusted surface
  dev.json          ← debug-only extras (MCP bridge, devtools)
```

**`default.json`** (production surface):

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window — production",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "opener:default"
  ]
}
```

**`dev.json`** (guarded by `#[cfg(debug_assertions)]` in `lib.rs`):

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "dev",
  "description": "Development-only extras",
  "windows": ["main"],
  "permissions": [
    "mcp-bridge:default"
  ]
}
```

### 4.2 Principle of Least Privilege

- Grant only the permissions the window actually needs.
- Prefer granular identifiers (`fs:allow-read-text-file`) over broad bundles (`fs:default`).
- Use `deny` scopes to explicitly block sensitive paths even when a broad `allow` is granted:

```json
{
  "identifier": "fs:scope",
  "allow": ["$APPDATA/vibes-left/**/*"],
  "deny":  ["$APPDATA/vibes-left/secrets.json"]
}
```

### 4.3 Remote URL IPC Access

Do **not** add `dangerousRemoteUrlIpcAccess` unless the domain is fully owned and the scope is minimal. If needed, restrict to specific windows and disable `enableTauriAPI`:

```json
"security": {
  "dangerousRemoteUrlIpcAccess": [
    {
      "windows": ["embed"],
      "domain": "trusted.internal",
      "plugins": ["specific-plugin"],
      "enableTauriAPI": false
    }
  ]
}
```

### 4.4 Content Security Policy (CSP)

Set a strict CSP in `tauri.conf.json`. Never use `null` in production builds:

```json
"security": {
  "csp": {
    "default-src": "'self' asset: https://asset.localhost",
    "connect-src": "ipc: http://ipc.localhost",
    "img-src": "'self' asset: http://asset.localhost blob: data:",
    "style-src": "'unsafe-inline' 'self'",
    "script-src": "'self'"
  }
}
```

> **Current state:** `csp: null` is set in `tauri.conf.json`. This must be replaced before shipping.

---

## 5. State Management

### 5.1 Managed State Pattern

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct AppState {
    pub db: Arc<Mutex<Database>>,
}

// In run():
.manage(AppState::default())
```

### 5.2 Locking Rules

- Use `tokio::sync::Mutex` (not `std::sync::Mutex`) inside `async` commands.
- Never hold a lock across an `await` point with `std::sync::Mutex` — it will deadlock.
- Prefer fine-grained state structs over one monolithic `AppState` blob.

### 5.3 AppHandle for Background Tasks

```rust
use tauri::Manager;

fn spawn_background(app: tauri::AppHandle) {
    tokio::spawn(async move {
        let state = app.state::<Mutex<AppState>>();
        let mut s = state.lock().await;
        // mutate
    });
}
```

---

## 6. Event System

Use events for **backend → frontend** push notifications; use `invoke` for **frontend → backend** requests.

```rust
// Backend emits
app_handle.emit("track-updated", &payload)?;

// Frontend listens
import { listen } from "@tauri-apps/api/event";
const unlisten = await listen<TrackPayload>("track-updated", (ev) => {
    console.log(ev.payload);
});
// Always unlisten on component unmount to avoid leaks
return () => unlisten();
```

---

## 7. Plugin Safety Protocols

### 7.1 MCP Bridge (debug-only)

`tauri-plugin-mcp-bridge` is gated behind `#[cfg(debug_assertions)]` in `lib.rs` and must remain that way. It exposes broad IPC introspection; it **must never ship in a production binary**.

Verify the gate is present before every release:

```rust
#[cfg(debug_assertions)]
{
    builder = builder.plugin(tauri_plugin_mcp_bridge::init());
}
```

### 7.2 Opener Plugin

`tauri-plugin-opener` opens URLs and files in the OS default app. Always validate the scheme before passing user-controlled values:

```rust
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
fn open_url(app: tauri::AppHandle, url: String) -> Result<(), AppError> {
    // Reject non-https schemes
    if !url.starts_with("https://") {
        return Err(AppError::Unauthorized("Only HTTPS URLs allowed".into()));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| AppError::Internal(e.to_string()))
}
```

---

## 8. Frontend ↔ Backend Contract

- Payload types must be mirrored: Rust struct with `#[derive(serde::Serialize, serde::Deserialize)]` ↔ TypeScript interface.
- Use a `src/bindings/` directory for generated or hand-written TS types that match Rust structs exactly.
- When a command changes signature, update the TS binding in the same commit.

---

## 9. Pre-Release Checklist

| Check | How |
|---|---|
| CSP is not `null` | Grep `tauri.conf.json` for `"csp": null` |
| `mcp-bridge` gated behind `debug_assertions` | Review `lib.rs` |
| No broad `fs:all` or `fs:allow-write` in production capability | Audit `capabilities/default.json` |
| All commands return `Result<T, AppError>` | `cargo clippy` + code review |
| No raw string interpolation in shell/SQL paths | Grep `format!` inside commands |
| `dangerousRemoteUrlIpcAccess` absent or scoped | Search `tauri.conf.json` |
| Frontend errors are caught and typed | TypeScript strict mode |

---

## 10. Directory Map

```
src-tauri/
  src/
    lib.rs          ← Builder setup, command registration, plugin gating
    main.rs         ← Entry point (calls lib::run)
    error.rs        ← Typed AppError (create this)
    commands/       ← One module per domain (create as needed)
  capabilities/
    default.json    ← Production permissions
    dev.json        ← Debug-only permissions (create this)
  tauri.conf.json   ← App config, CSP, window definitions

src/
  bindings/         ← TypeScript mirrors of Rust payload types (create this)
  App.tsx
```
