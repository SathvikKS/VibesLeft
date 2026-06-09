# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Clearing Caches

### Usage report cache

Stored as `usage-cache.json` in the Tauri app data directory:

```sh
# macOS
rm ~/Library/Application\ Support/com.sathvikks.vibes-left/usage-cache.json
```

```powershell
# Windows
del "%APPDATA%\com.sathvikks.vibes-left\usage-cache.json"
```

### Token cache

**Debug builds** — plain JSON files:

```sh
# macOS
rm -f /tmp/vibes-left-token-cache-*.json
```

```powershell
# Windows
del "%TEMP%\vibes-left-token-cache-*.json"
```

**Release builds** — stored in the OS credential vault:

```sh
# macOS (Keychain)
security delete-generic-password -s "vibes-left-token-cache" -a "claude"
security delete-generic-password -s "vibes-left-token-cache" -a "antigravity"
```

```powershell
# Windows (Credential Manager)
cmdkey /delete:vibes-left-token-cache
```
