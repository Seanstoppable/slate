# Slate

A terminal info dashboard with a plugin ecosystem. Think [wtfutil](https://wtfutil.com/) rewritten with first-class plugin support — WASM-sandboxed modules, vim-plug style management, and update notifications.

## Why Slate?

We loved wtfutil but wanted to start from a **blank slate** (pun intended) to address two things it never quite got right:

1. **Code coverage** — wtfutil has minimal test coverage, making contributions risky and regressions hard to catch. Slate is built test-first: plugins gate their logic behind testable pure functions, the host runtime has comprehensive unit tests, and CI enforces coverage on every PR.

2. **Plugins with real third-party support** — wtfutil's modules are compiled into the binary. Adding a widget means forking the whole project. Slate uses WASM sandboxing (via [Extism](https://extism.org/)) so anyone can write a plugin in Rust, Go, JavaScript, Zig, or AssemblyScript, publish it to a GitHub repo, and users install it with one line in their config — no recompilation, no forks, no trust issues (capabilities are explicitly granted).

## Features

- **WASM-sandboxed plugins** — Community plugins run in an Extism sandbox with capability-gated permissions
- **5 built-in widgets** — Resources, power, firewall, network interfaces, VCS (git/hg)
- **6 WASM plugins (Rust)** — Clock, weather, HN, feeds, IP info, GitHub
- **4 WASM plugins (polyglot)** — Status pages (JS), iStats (Zig), wego (AssemblyScript), cmdrunner (Go)
- **Lua scripting** — Quick personal widgets with zero compilation (brew outdated, docker ps, disk usage, git log, todo.txt)
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

| Plugin | Type | Language | Description |
|--------|------|----------|-------------|
| `resource_usage` | Builtin | Rust | CPU, memory, swap, temperatures |
| `power` | Builtin | Rust | Battery status and charging state |
| `ipaddresses` | Builtin | Rust | Local network interface addresses |
| `firewall` | Builtin | Rust | Firewall status and rules |
| `vcs` | Builtin | Rust | Git/Mercurial status (configurable engine) |
| `clock` | WASM | Rust | Current time with timezone |
| `weather` | WASM | Rust | Weather via OpenWeatherMap |
| `ipinfo` | WASM | Rust | Public IP and geolocation (via ipinfo.io) |
| `hackernews` | WASM | Rust | Top stories (interactive list) |
| `feedreader` | WASM | Rust | RSS/Atom feed reader |
| `github` | WASM | Rust | GitHub PRs, issues, repo stats |
| `status-pages` | WASM | JavaScript | Service status page monitor (Statuspage APIs) |
| `brew-outdated` | WASM | Go | Outdated Homebrew packages (polyglot demo; prefer `scripts/brew-outdated.lua`) |
| `istats` | WASM | Zig | System stats via iStats (macOS) |
| `wego` | WASM | AssemblyScript | Weather display via wego CLI |

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

### Multi-Language Plugins

Plugins can be written in any language that compiles to WASM via [Extism PDK](https://extism.org/docs/concepts/pdk):

| Language | Build Command | Binary Size |
|----------|--------------|-------------|
| **Rust** | `cargo build --target wasm32-unknown-unknown --release` | ~200 KB |
| **Go** (TinyGo) | `tinygo build -o plugin.wasm -target wasi main.go` | ~1.1 MB |
| **JavaScript** | `extism-js src/index.js -i src/index.d.ts -o plugin.wasm` | ~2.4 MB |
| **Zig** | `zig build-exe src/main.zig -target wasm32-freestanding ...` | ~2 KB |
| **AssemblyScript** | `npx asc assembly/index.ts --outFile plugin.wasm` | ~16 KB |

All plugins export the same 4 functions: `metadata`, `refresh`, `on_key`, `on_action`.

#### Host Functions

Plugins can call host-provided functions:
- **HTTP** (built-in to Extism) — make network requests
- **exec_command** — run a system command (requires `exec` permission)

```json
// exec_command input
{"cmd": "brew", "args": ["outdated"]}
// exec_command output
{"stdout": "...", "stderr": "...", "exit_code": 0}
```

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
├── plugins/                  # WASM plugin source (10 plugins, multiple languages)
│   ├── clock/                # Rust — multi-location world clocks
│   ├── hackernews/           # Rust
│   ├── ipinfo/               # Rust
│   ├── github/               # Rust
│   ├── weather/              # Rust
│   ├── feedreader/           # Rust
│   ├── status-pages/         # JavaScript (Extism JS PDK)
│   ├── brew-outdated/        # Go (TinyGo) — polyglot demo
│   ├── istats/               # Zig
│   └── wego/                 # AssemblyScript
├── scripts/                  # Lua script examples (no compilation needed)
│   ├── greeting.lua          # Hello-world
│   ├── brew-outdated.lua     # Homebrew outdated packages
│   ├── disk-usage.lua        # df -h with progress bars
│   ├── docker-ps.lua         # Running Docker containers
│   ├── git-recent.lua        # Recent commits in a repo
│   └── todo.lua              # todo.txt reader
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
- Use **WASM plugin** when the widget owns its own data fetching (HTTP APIs, parsing responses) — the sandbox provides real isolation and the plugin is portable/distributable across machines.
- Use **Lua script** for personal command-runner widgets (shell one-liners, file reading, simple formatting). If it's "run a command and display the output" — use Lua. No compilation, instant iteration, full `io.popen`/`os.execute` access.

**Rule of thumb:** If your widget is ≤50 lines and mostly shells out to a CLI tool → Lua script. If it makes HTTP requests to an API and you want to share it → WASM plugin.

All implement the same `Widget` trait and are configured uniformly in `slate.toml`.

### Lua Scripts

Lua scripts are the fastest way to add a personal widget. They run in a sandboxed Luau runtime with access to host-provided `slate.*` functions:

| Function | Description |
|----------|-------------|
| `slate.exec(cmd, args?)` | Run a command, returns `{stdout, stderr, exit_code}` |
| `slate.read_file(path)` | Read a file, returns string or nil |
| `slate.time()` | Current time: `{hour, min, sec, year, month, day, weekday, timestamp}` |
| `slate.env(name)` | Read an environment variable, returns string or nil |

```lua
-- scripts/git-recent.lua
name = "Git Recent"
description = "Shows recent git commits"

function refresh()
    local result = slate.exec("git", {"log", "--oneline", "-n", "8"})
    if result.exit_code ~= 0 then
        return '{"type":"text","content":"Not a git repo","scrollable":false,"wrap":true}'
    end
    -- ... format result.stdout as list items
end
```

Config:
```toml
[[widget]]
type = "lua:scripts/git-recent.lua"
position = { row = 2, col = 0 }
path = "/path/to/repo"
```

See `scripts/` for more examples: brew outdated, disk usage, docker containers, git log, todo.txt.

## Requirements

- Rust 1.70+ with `wasm32-unknown-unknown` target
- Windows: MSVC Build Tools (for native compilation)

```bash
rustup target add wasm32-unknown-unknown
```

## License

MIT
