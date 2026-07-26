# Slate

A terminal info dashboard with a plugin ecosystem. Think [wtfutil](https://wtfutil.com/) rewritten with first-class plugin support — WASM-sandboxed modules, vim-plug style management, and update notifications.

## Features

- **WASM-sandboxed plugins** — Community plugins run in an Extism sandbox with capability-gated permissions
- **5 built-in widgets** — Resources, power, firewall, network interfaces, VCS (git/hg)
- **6 WASM plugins** — Clock, weather, HN, feeds, IP info, GitHub
- **Lua scripting** — Quick personal widgets with zero compilation
- **Plugin manager** — Install from GitHub repos, lockfile-based versioning, update notifications
- **Interactive lists** — Navigate items with j/k, open links with Enter
- **Vim-style navigation** — h/j/k/l, Tab cycling, focus management

## Quick Start

```bash
# Build the dashboard
cargo build --release

# Build WASM plugins (from project root)
for dir in plugins/*/; do
  (cd "$dir" && cargo build --release --target wasm32-unknown-unknown)
done

# Run
./target/release/slate
```

## Configuration

Config lives at:
- **Windows**: `%APPDATA%\slate\slate.toml`
- **macOS/Linux**: `~/.config/slate/slate.toml`

```toml
[global]
refresh_interval = 300  # seconds between auto-refresh

[layout]
rows = 2
cols = 2

# Built-in widget (native system access)
[[widget]]
type = "builtin:resource_usage"
position = { row = 0, col = 0 }

# Local WASM plugin
[[widget]]
type = "wasm:/path/to/slate_clock.wasm"
position = { row = 0, col = 1 }

# GitHub-hosted plugin (installed via `slate install`)
[[widget]]
type = "github.com/slate-community/slate-hackernews"
position = { row = 1, col = 0 }

# Lua script
[[widget]]
type = "lua:~/.config/slate/scripts/greeting.lua"
position = { row = 1, col = 1 }

[updates]
check_interval = "daily"
notify = true
auto_update = false
```

### Widget Settings

Widgets receive arbitrary settings from config:

```toml
[[widget]]
type = "wasm:/path/to/slate_weather.wasm"
position = { row = 0, col = 1 }
api_key = "${OPENWEATHER_API_KEY}"
location = "San Francisco"
```

Environment variables are interpolated with `${VAR_NAME}` syntax.

## Plugins

### Available Plugins

| Plugin | Type | Description |
|--------|------|-------------|
| `resource_usage` | Builtin | CPU, memory, swap, temperatures |
| `power` | Builtin | Battery status and charging state |
| `ipaddresses` | Builtin | Local network interface addresses |
| `firewall` | Builtin | Firewall status and rules |
| `vcs` | Builtin | Git/Mercurial status (configurable engine) |
| `clock` | WASM | Current time with timezone |
| `weather` | WASM | Weather via OpenWeatherMap |
| `ipinfo` | WASM | Public IP and geolocation (via ipinfo.io) |
| `hackernews` | WASM | Top stories (interactive list) |
| `feedreader` | WASM | RSS/Atom feed reader |
| `github` | WASM | GitHub PRs, issues, repo stats |

### Plugin Management

```bash
slate install          # Install all declared plugins from config
slate update           # Update to latest compatible versions
slate outdated         # Show available updates
slate list             # List installed plugins
slate remove <name>    # Remove a plugin
slate search <query>   # Search the plugin registry
slate create <name>    # Scaffold a new plugin project
```

### Creating a Plugin

Plugins are Rust crates compiled to `wasm32-unknown-unknown` using [Extism PDK](https://extism.org/):

```rust
use extism_pdk::*;
use serde_json::json;

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "My Widget",
        "description": "Does something cool",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "You"
    }).to_string())
}

#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    // input contains JSON settings from config
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    // Make HTTP requests (if permitted):
    // let req = HttpRequest::new("https://api.example.com/data");
    // let resp = http::request::<String>(&req, None)?;

    Ok(json!({
        "type": "key_value",
        "pairs": [
            {"key": "Status", "value": "OK"},
            {"key": "Info", "value": "Hello from my plugin"}
        ]
    }).to_string())
}

#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}
```

Build: `cargo build --release --target wasm32-unknown-unknown`

### Content Types

Plugins return JSON with one of these types:

```jsonc
// Simple text
{"type": "text", "content": "Hello!", "scrollable": false, "wrap": true}

// Key-value pairs
{"type": "key_value", "pairs": [{"key": "CPU", "value": "42%"}]}

// Interactive list
{"type": "list", "items": [{"id": "1", "title": "Item", "subtitle": "Detail"}], "selectable": true}
```

### Permissions

Plugins declare required permissions in `plugin.toml`:

```toml
[permissions]
network = ["api.github.com"]     # HTTP to specific hosts
exec = ["docker"]                 # Run specific binaries
storage = true                    # Sandboxed key-value store
filesystem_read = ["~/.config"]   # Read specific paths
raw_network = true                # ICMP/ping
secrets = ["token"]               # Masked in UI
```

WASM enforces sandboxing architecturally — plugins cannot bypass permissions.

## Keyboard Navigation

| Key | Action |
|-----|--------|
| `q` | Quit |
| `Tab` / `Shift+Tab` | Cycle widgets (reading order) |
| `←` `→` `↑` `↓` or `h` `j` `k` `l` | Move focus / scroll list |
| `Enter` | Select item (opens URL in browser for links) |
| `r` | Force refresh focused widget |
| `Ctrl+C` | Quit |

When a widget contains a selectable list, `j`/`k` scroll within the list instead of moving focus.

## Architecture

```
slate/
├── crates/
│   ├── slate-core/           # TUI engine (ratatui), layout, config, notifications
│   ├── slate-plugin-host/    # WASM (Extism) + Lua (mlua) runtimes, permissions
│   ├── slate-plugin-sdk/     # Widget trait, WidgetContent types, WidgetAction
│   ├── slate-plugin-manager/ # GitHub download, versions, lockfile, registry
│   └── slate-cli/            # Binary, clap commands, built-in widgets
├── plugins/                  # WASM plugin source (6 plugins)
│   ├── clock/
│   ├── hackernews/
│   ├── ipinfo/
│   ├── github/
│   ├── weather/
│   └── feedreader/
└── .github/extensions/       # Copilot skill for scaffolding plugins
```

### Module Tiers

| Tier | Runtime | Use Case |
|------|---------|----------|
| **Built-in** | Native Rust | Needs direct OS/system access (power, firewall, network interfaces, VCS, CPU/memory) |
| **WASM Plugin** | Extism sandbox | Fetches its own data via capability-gated host functions (HTTP APIs, storage) |
| **Lua Script** | mlua (Luau) | Quick personal widgets, no compilation |

**When to use which:**
- Use **builtin** when the widget needs direct system access (process execution, sysinfo, file reads) that cannot be meaningfully sandboxed — the data source *is* the local machine.
- Use **WASM plugin** when the widget owns its own data fetching (HTTP APIs, parsing responses) — the sandbox provides real isolation and the plugin is portable across machines.
- Use **Lua script** for personal one-off widgets that don't need distribution.

All implement the same `Widget` trait and are configured uniformly in `slate.toml`.

## Requirements

- Rust 1.70+ with `wasm32-unknown-unknown` target
- Windows: MSVC Build Tools (for native compilation)

```bash
rustup target add wasm32-unknown-unknown
```

## License

MIT
