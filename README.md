# Slate

A terminal info dashboard with a plugin ecosystem. Think wtfutil rewritten with first-class plugin support — WASM-sandboxed modules installable from GitHub repos, vim-plug style management, and update notifications.

## Quick Start

```bash
# Build
cargo build --release

# Run the dashboard
slate run

# Or with a specific config
slate run --config path/to/slate.toml
```

## Configuration

Create `~/.config/slate/slate.toml`:

```toml
[global]
refresh_interval = 300

[layout]
rows = 2
cols = 2

[[widget]]
type = "builtin:clock"
position = { row = 0, col = 0 }

[[widget]]
type = "builtin:resource_usage"
position = { row = 0, col = 1 }
```

## Plugin Management

```bash
slate install          # Install all declared plugins
slate update           # Update to latest versions
slate outdated         # Show available updates
slate list             # List installed plugins
slate remove <name>    # Remove a plugin
slate search <query>   # Search the registry
slate create <name>    # Scaffold a new plugin
```

## Plugin Types

| Tier | Runtime | For |
|------|---------|-----|
| Built-in | Native Rust | System-level: CPU, clock, network |
| WASM Plugin | Extism sandbox | Community: GitHub, weather, etc. |
| Lua Script | mlua (Luau) | Quick personal widgets |

## Keyboard Navigation

- `q` — Quit
- `Tab` — Next widget
- `h/j/k/l` or arrow keys — Navigate grid
- `r` — Force refresh focused widget
- `Enter` — Select/activate

## Architecture

```
slate/
├── crates/
│   ├── slate-core/           # TUI engine, layout, config
│   ├── slate-plugin-host/    # WASM + Lua runtimes, permissions
│   ├── slate-plugin-sdk/     # Widget trait, types
│   ├── slate-plugin-manager/ # GitHub install, lockfile, registry
│   └── slate-cli/            # Binary, clap commands
├── plugins/                  # Example WASM plugins
├── builtins/                 # Native Rust widgets
└── docs/                     # Documentation
```

## License

MIT
