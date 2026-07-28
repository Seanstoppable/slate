# AGENTS.md — Slate Terminal Dashboard

## Project Overview

Slate is a terminal info dashboard with a WASM plugin ecosystem. Think wtfutil rewritten in Rust with first-class plugin support — Extism-sandboxed WASM modules, Lua scripting, vim-plug style management, and update notifications.

## Architecture

```
slate/ (Rust workspace, edition 2021)
├── crates/
│   ├── slate-plugin-sdk/       # Widget trait, types (no runtime deps)
│   ├── slate-core/             # TUI engine (ratatui), config, layout, app loop
│   ├── slate-plugin-host/      # WASM (Extism) + Lua (mlua) runtimes, host functions
│   ├── slate-plugin-manager/   # GitHub install, lockfile, version resolution, registry
│   └── slate-cli/              # Binary entrypoint, CLI commands, 9 builtin widgets
├── plugins/                    # 10 WASM plugins (Rust, Go, JS, Zig, AssemblyScript)
├── scripts/                    # Lua script examples
├── docs/                       # Plugin docs site (template.html)
├── examples/                   # Example slate.toml
└── .github/workflows/ci.yml    # Tests (3 OS), lint, coverage (85% threshold)
```

## Crate Dependency Graph

```
slate-plugin-sdk (trait definitions, zero runtime deps)
    ↑
slate-core (ratatui TUI, config, layout)
    ↑
slate-plugin-host (Extism WASM + mlua Lua runtimes)
    ↑
slate-plugin-manager (GitHub install, lockfile, registry)
    ↑
slate-cli (binary, commands, builtins)
```

## Key Types

### Widget Trait (`slate-plugin-sdk/src/widget.rs`)
All widgets implement this regardless of runtime tier:
- `metadata()` → name, description, version, author
- `init(config)` → receive settings
- `refresh()` → return WidgetContent
- `on_key(key, action)` → handle keyboard input
- `on_action(action_id, item_id)` → handle list item actions, returns optional WidgetAction
- `on_focus()` / `on_blur()` → lifecycle hooks

### WidgetContent (`slate-plugin-sdk/src/content.rs`)
Six display types: `Text`, `Table`, `KeyValue`, `List`, `Chart`, `Empty`

### WidgetAction
Three host actions a widget can request: `OpenUrl(String)`, `Notify(String)`, `ShowDetail(String)`

### Position
Grid placement with spanning: `{ row, col, row_span, col_span }`

## Config

**Path:** `~/.config/slate/slate.toml` (Linux/macOS) or `%APPDATA%\slate\slate.toml` (Windows)

**Format:**
```toml
[global]
refresh_interval = 300

[layout]
rows = 2
cols = 3

[[widget]]
type = "builtin:resource_usage"           # Builtin
position = { row = 0, col = 0, row_span = 2 }
# type = "wasm:path/to/plugin.wasm"      # Local WASM
# type = "lua:~/.config/slate/script.lua" # Lua script
# type = "github.com/owner/repo"         # GitHub-sourced WASM

[updates]
check_interval = "daily"
notify = true
auto_update = false
```

Environment variable interpolation: `token = "${GITHUB_TOKEN}"`

## Builtin Widgets

| Name | Config Type | Output |
|------|-------------|--------|
| clock | `builtin:clock` | KeyValue (time, date, timezone, unix) |
| digital_clock | `builtin:digital_clock` | Text (ASCII art time) |
| resource_usage | `builtin:resource_usage` | KeyValue (CPU, memory, swap, temp) |
| power | `builtin:power` | KeyValue (battery, charge state) |
| firewall | `builtin:firewall` | KeyValue (status, rule count) |
| ipaddresses | `builtin:ipaddresses` | KeyValue (interface IPs) |
| vcs | `builtin:vcs` | KeyValue (git/hg repo info) |

## WASM Plugins (in `plugins/`)

| Plugin | Language | Permissions |
|--------|----------|-------------|
| clock | Rust | None |
| ipinfo | Rust | `network: [ipinfo.io]` |
| hackernews | Rust | `network: [hacker-news.firebaseio.com]` |
| github | Rust | `network: [api.github.com]`, `secrets: [token]` |
| weather | Rust | `network: [api.openweathermap.org]`, `secrets: [api_key]` |
| feedreader | Rust | `network: ["*"]` |
| status-pages | TypeScript | `network: [www.githubstatus.com, status.slack.com]` |
| brew-outdated | Go (TinyGo) | `exec: [brew]` |
| istats | Zig | `exec: [istats]` |
| wego | AssemblyScript | `exec: [wego]` |

## Host Functions

**WASM plugins** get Extism HTTP (permission-gated) + `exec_command` host function.

**Lua scripts** get injected `slate` table:
- `slate.exec(cmd, args?)` → `{stdout, stderr, exit_code}`
- `slate.read_file(path)` → string
- `slate.time()` → `{hour, min, sec, year, month, day, weekday, timestamp}`
- `slate.env(name)` → string or nil

## Permissions Model

Declared in each plugin's `plugin.toml`:
```toml
[permissions]
network = ["api.github.com"]     # HTTP to specific hosts
exec = ["docker"]                # Specific binaries only
filesystem_read = ["~/.config"]  # Read specific paths
storage = true                   # Sandboxed KV store
raw_network = true               # ICMP/ping
secrets = ["token"]              # Masked in UI
```

Enforcement: `PermissionGuard` checks every host function call. WASM sandbox prevents bypass architecturally.

## CLI Commands

```
slate run [--config path]    # Launch dashboard
slate install                # Install all declared plugins
slate update                 # Update to latest compatible versions
slate outdated               # Show available updates
slate list                   # List installed plugins
slate remove <name>          # Remove a plugin
slate create <name>          # Scaffold new plugin project
slate search <query>         # Search registry
slate check [--config path]  # Validate config + plugins without launching
slate docs [--output dir]    # Generate plugin documentation site
slate migrate <path>         # Convert wtfutil config (not yet implemented)
```

## Building & Testing

```bash
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test -p slate-core

# Build the binary
cargo build --release

# Build a WASM plugin
cd plugins/clock && cargo build --release --target wasm32-unknown-unknown

# Check code coverage (outputs JSON to target/)
cargo tarpaulin --workspace -o json --skip-clean --output-dir target
```

## CI

- **Tests**: Run on ubuntu, windows, macos
- **Lint**: `cargo fmt --check` + `cargo clippy --workspace`
- **Coverage**: Must stay above **85%** (currently ~90%)
- Triggers on PR and push to main branch

## Plugin Authoring

Four required WASM exports:
1. `metadata(input: String) -> String` — JSON with name, description, version
2. `refresh(input: String) -> String` — JSON WidgetContent (settings passed as input)
3. `on_key(input: String) -> String` — Handle key press, return action or empty
4. `on_action(input: String) -> String` — Handle list action, return action or empty

Scaffold with `slate create my-plugin`, build with `cargo build --target wasm32-unknown-unknown --release`.

## Code Conventions

- Tests live in `#[cfg(test)] mod tests` at bottom of each file
- Async functions use `#[tokio::test]` in tests
- Browser-launching code is gated with `#[cfg(not(test))]`
- Dev dependencies: `tempfile` for filesystem tests, `wat` for WASM binary creation
- Platform-specific code uses `cfg(target_os = "...")` with fallbacks
- Widget settings flow: TOML → `HashMap<String, serde_json::Value>` → passed to `init()`/`refresh()`
