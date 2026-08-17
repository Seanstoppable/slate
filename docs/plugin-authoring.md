# Plugin Authoring Guide

## Overview

Slate plugins can be authored in three ways:

1. **WASM (Rust + Extism PDK)** — The recommended approach for community plugins
2. **Lua scripts** — Quick trusted local widgets, no compilation needed
3. **Native Rust** — For built-in system-level widgets

Use **WASM** for anything you expect other users to install from a release or registry entry.
Use **Lua** for personal automation on machines you already trust — Lua scripts are not sandboxed.

## Creating a WASM Plugin

### Scaffold

```bash
slate create my-plugin
cd my-plugin
```

This creates:
- `Cargo.toml` — Rust library targeting `cdylib`
- `plugin.toml` — Metadata and permissions
- `src/lib.rs` — Plugin entry points

### Required Exports

Your plugin must export these functions:

```rust
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String>;

#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String>;
```

Optional exports:
```rust
#[plugin_fn]
pub fn on_key(input: String) -> FnResult<String>;

#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String>;

#[plugin_fn]
pub fn on_focus(_input: String) -> FnResult<String>;

#[plugin_fn]
pub fn on_blur(_input: String) -> FnResult<String>;
```

### Building

```bash
cargo build --target wasm32-wasip1 --release
```

The `.wasm` file is in `target/wasm32-wasip1/release/`.

> **WASI support:** Plugins are built with WASI Preview 1, giving them access to
> `std::time::SystemTime` (clock) and randomness without any permission declaration.
> Filesystem and environment access are **not** provided by default — those require
> explicit host pre-opens which Slate does not grant.

### Content Types

The `refresh` function returns JSON matching one of these types:

#### Text
```json
{"type": "text", "content": "Hello!", "scrollable": false, "wrap": true}
```

#### Table
```json
{"type": "table", "headers": ["Name", "Value"], "rows": [[{"text": "CPU"}, {"text": "45%"}]], "selectable": false}
```

#### Key-Value
```json
{"type": "key_value", "pairs": [["IP", {"text": "1.2.3.4"}], ["Location", {"text": "US"}]]}
```

#### List
```json
{"type": "list", "items": [{"id": "1", "title": "Item 1", "subtitle": "Details"}], "selectable": true, "actions": [{"id": "open", "label": "Open", "key": "o"}]}
```

### Host Functions

Available to WASM plugins:

| Function | Permission | Description |
|----------|-----------|-------------|
| `http::request` | `network` | Extism HTTP client, limited to hosts declared in `plugin.toml` |
| `exec_command` | `exec` | Run an allowed binary |
| `store_get` / `store_set` | `storage` | Sandboxed key-value store |
| `get_config` | always | Read widget config values from any lifecycle hook |

### Permissions (plugin.toml)

```toml
[permissions]
network = ["api.github.com"]
network_from_config = ["endpoint"] # URL string or array of URL strings
exec = ["docker"]
storage = true
secrets = ["token"] # documents expected secret-backed config values
```

Wildcard network permissions (`network = ["*"]`) are rejected. For user-configured
destinations, declare the relevant URL-valued config fields with `network_from_config`.

## Creating a Lua Plugin

Create a `.lua` file with globals and a `refresh()` function:

```lua
name = "My Widget"
description = "Does something cool"
version = "0.1.0"

function refresh()
    return '{"type":"text","content":"Hello from Lua!"}'
end
```

Reference it in your config:
```toml
[[widget]]
type = "lua:~/.config/slate/scripts/my_widget.lua"
position = { row = 0, col = 0 }
```

Lua scripts can call host helpers like `slate.exec()` and `slate.read_file()` without a permission manifest.
Treat them as trusted local code, not distributable community plugins.

### Making a Lua plugin interactive

Lua plugins get the same `on_key`/`on_action`/`on_focus`/`on_blur` hooks as WASM and builtin
plugins — just define them as top-level functions and the host calls them automatically:

```lua
local count = 0

function refresh()
    return slate.text("Count: " .. count .. "\n[space] increment")
end

-- key is the Rust `KeyCode` debug string, e.g. "Char(' ')", "Esc", "Enter".
-- Reserved navigation keys (h/j/k/l, arrows, tab, enter, r, q) never reach on_key.
function on_key(key, _action)
    if key == "Char(' ')" then
        count = count + 1
    end
end
```

The `Lua` interpreter backing a widget stays alive for the widget's whole lifetime, so local
state set in `on_key` persists and shows up the next time `refresh()` runs — no extra
plumbing required. Note that this state is in-memory only and is lost when Slate restarts; for
state that must survive restarts, use a WASM plugin with the `storage` permission (see
`plugins/pomodoro`).

## Publishing

1. Create a GitHub release with your `.wasm` file as an asset
2. Submit a PR to `slate-community/slate-registry` adding your plugin entry
