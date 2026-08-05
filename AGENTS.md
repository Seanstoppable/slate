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

**Interactivity gotcha:** `on_key` only mutates a widget's internal state — it does not
repaint by itself. `slate-core/src/app.rs`'s key handler calls `widget.refresh()`
immediately after every forwarded `on_key`, so state changes appear on the very next
frame. If you add a new key-handling code path in `app.rs`, make sure it re-renders too,
or keypresses will look like no-ops until the next scheduled refresh tick.

For any widget whose state changes over time on its own (timers, counters, anything
ticking while focused), set a short per-widget `refresh_interval` (e.g. `1` second) in
its `[[widget]]` config entry — the `[global] refresh_interval` default (300s) is tuned
for passive polling widgets, not live countdowns. See `scripts/pomodoro.lua` and its
config entry in `examples/slate.toml` for the pattern.

Focused widgets render with a double-line border (`BorderType::Double` in
`slate-core/src/render.rs`) in addition to the cyan highlight color, so keep both in
sync if you touch focus rendering.

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
| resource_usage | `builtin:resource_usage` | KeyValue (CPU, memory, swap, temp) |
| power | `builtin:power` | KeyValue (battery, charge state) |
| firewall | `builtin:firewall` | KeyValue (status, rule count) |
| ipaddresses | `builtin:ipaddresses` | KeyValue (interface IPs) |
| vcs | `builtin:vcs` | KeyValue (git/hg repo info) |

## WASM Plugins (in `plugins/`)

| Plugin | Language | Permissions |
|--------|----------|-------------|
| clock | Rust | None (uses WASI clock) |
| ipinfo | Rust | `network: [ip-api.com]` |
| hackernews | Rust | `network: [hacker-news.firebaseio.com]` |
| github | Rust | `network: [api.github.com]`, `secrets: [token]` |
| weather | Rust | `network: [api.openweathermap.org]`, `secrets: [api_key]` |
| feedreader | Rust | `network: ["*"]` |
| docker | Rust | `exec: [docker]` |
| lunarphase | Rust | None |
| status-pages | TypeScript | `network: [www.githubstatus.com, status.slack.com]` |
| brew-outdated | Go (TinyGo) | `exec: [brew]` |
| istats | Zig | `exec: [istats]` |
| wego | AssemblyScript | `exec: [wego]` |
| yfinance | Rust | `network: [query1.finance.yahoo.com]` |

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
cd plugins/clock && cargo build --release --target wasm32-wasip1

# Check code coverage (outputs JSON to target/)
cargo tarpaulin --workspace -o json --skip-clean --output-dir target
```

## Before Opening a PR

Always run these locally before submitting a PR — CI's `lint` job runs the same checks and will fail the build otherwise:

```bash
cargo fmt --all -- --check   # or `cargo fmt --all` to auto-fix
cargo clippy --workspace --all-targets
cargo test --workspace
```

If `cargo fmt --all -- --check` reports diffs, run `cargo fmt --all` to fix them in place. If `cargo clippy` reports warnings, fix them or add a scoped `#[allow(...)]` with justification — do not suppress lints workspace-wide.

**Run `cargo fmt --all` right after editing any `.rs` file, not just before opening the PR.** CI's lint job has failed in this project before purely because formatting was fixed up only at commit time; if you edit Rust across multiple turns, reformat after each edit batch so a stray fmt diff doesn't surprise you at PR time.

## Known Flaky CI Checks

- `slate-plugin-manager` has tests (`check_single_returns_update...`, `check_outdated_collects_only...`, and similar) that hit the *real* GitHub releases API. These occasionally fail on `macos-latest` CI runners due to network flakiness, not code changes. If a PR you didn't touch `slate-plugin-manager` in shows these failing, verify the same tests pass locally and on the base branch's latest run, then just re-run CI — do not "fix" by loosening assertions or skipping the tests.
- A macOS job failure in the test matrix can cause ubuntu/windows jobs to show as canceled rather than failed — re-run the whole workflow, not just the failed job, if you see a mix of "failure" and "cancelled".

## CI

- **Tests**: Run on ubuntu, windows, macos
- **Lint**: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` (any clippy warning fails the build)
- **Coverage**: Must stay above **85%** (currently ~90%)
- Triggers on PR and push to main branch

### Lessons from prior sessions

- **New `WidgetContent` variants need matching parser coverage.** `wasm_host.rs` hand-parses widget JSON with `serde_json::Value` indexing rather than relying on the SDK's tagged-enum deserialization — it's a parallel, easy-to-forget schema. Adding a new content type (e.g. a `table` cell/style branch) requires tests for *every* parsing branch (each color, each style flag, missing/malformed fields), not just the happy path, or the workspace coverage can dip below the 85% CI threshold.
- **Rebase early, and check for upstream CI-relevant changes first.** Before triaging a CI failure as "pre-existing" or "flaky," run `git log --oneline HEAD..origin/<base-branch>` to see whether the base branch has already fixed it or changed enforcement (e.g. added `-D warnings` to clippy). A stale branch can show failures that a simple rebase resolves.
- **Verify "flaky" test failures locally before writing them off.** Run `cargo test -p <crate> --lib <module>::` for the specific failing tests. If they pass locally, the failure is very likely CI-environment or base-branch drift, not a real flake in your changes.
- **Watch for unexpected merge commits after `git rebase`.** In this environment a rebase was once followed by an automatic extra merge commit. Always inspect `git log --graph` and `git reflog` after rebasing, and `git reset --hard` to the actual rebased tip before force-pushing, to keep history linear.

## Plugin Authoring

Four required WASM exports:
1. `metadata(input: String) -> String` — JSON with name, description, version
2. `refresh(input: String) -> String` — JSON WidgetContent (settings passed as input)
3. `on_key(input: String) -> String` — Handle key press, return action or empty
4. `on_action(input: String) -> String` — Handle list action, return action or empty

Scaffold with `slate create my-plugin`, build with `cargo build --target wasm32-wasip1 --release`.

Plugins use WASI Preview 1, granting access to system clock (`std::time::SystemTime`) and randomness without permissions. Filesystem/environment are not pre-opened by the host.

## Code Conventions

- Tests live in `#[cfg(test)] mod tests` at bottom of each file
- Async functions use `#[tokio::test]` in tests
- Browser-launching code is gated with `#[cfg(not(test))]`
- Dev dependencies: `tempfile` for filesystem tests, `wat` for WASM binary creation
- Platform-specific code uses `cfg(target_os = "...")` with fallbacks
- Widget settings flow: TOML → `HashMap<String, serde_json::Value>` → passed to `init()`/`refresh()`
- **All list/array values must be alphabetical (case-insensitive) unless otherwise specified** — this includes `tags` in plugin.toml, permission lists, OS lists, and any other ordered collections

## Delegation Guide

When implementing new WASM plugins or Lua scripts, delegate to a cheaper/faster model (e.g., Sonnet, Haiku, GPT-5-mini). These tasks are well-scoped and follow established patterns.

### What to delegate

- New WASM plugins (follow the pattern in any `plugins/*/src/lib.rs`)
- New Lua scripts (follow the pattern in any `scripts/*.lua` — use `slate.*` helpers, never raw JSON)
- Adding unit tests to existing plugins
- Adding `tags` or config fields to `plugin.toml` files
- Updating `docs/wtfutil-compatibility.md` entries
- Bulk file edits (updating multiple plugin.toml files, sorting lists, etc.)
- Mechanical refactors with clear patterns (e.g., adding a parameter to all call sites)

### What NOT to delegate

Reserve the expensive/high-reasoning model for:
- Architecture decisions (new crate design, trait changes, host function APIs)
- Complex debugging (WASM traps, cross-crate type mismatches, platform-specific issues)
- Multi-system reasoning (config → host → plugin → renderer data flow)
- Security and permissions model changes

### How to prompt the delegated model

Include in the prompt:
1. **The pattern to follow** — point to an existing similar plugin (e.g., "Follow the structure of `plugins/pihole/src/lib.rs`")
2. **The cfg-gating pattern** — `extism-pdk` behind `cfg(target_arch = "wasm32")`, pure logic always available, tests at bottom
3. **Required files** — `Cargo.toml`, `plugin.toml` (with tags), `src/lib.rs`
4. **Cargo.toml template**:
   ```toml
   [target.'cfg(target_arch = "wasm32")'.dependencies]
   extism-pdk = "1"
   ```
5. **Test expectations** — extract pure logic into functions, test those natively with `cargo test`
6. **Add to workspace excludes** in root `Cargo.toml`
7. **API details** — the actual API endpoints, response format, and what fields to display

### Verification after delegation

After receiving the implementation:
- Run `cargo test --manifest-path plugins/<name>/Cargo.toml`
- Verify the plugin is in workspace `exclude` list
- Check that `plugin.toml` has name, description, version, tags, and permissions

## Cost Efficiency

### Reducing credit usage

This project's biggest credit drivers are long iterative sessions and bulk edits. Follow these principles:

1. **Be specific in requests** — "Add `None` as the 8th argument to all `add_widget` calls in tests" is cheaper than "fix the compile errors" (avoids trial-and-error loops).

2. **Batch related changes** — Instead of changing one file at a time across 15 plugin.toml files, describe the full pattern once and let the agent do all files in one pass.

3. **Break mega-sessions into focused sessions** — Context accumulates and every message re-reads the full history. A 200-turn session costs far more per turn than four 50-turn sessions. Start fresh sessions for distinct feature areas.

4. **Run builds/tests locally and report results** — Instead of the agent running `cargo test` and reading 200 lines of output, run it yourself and paste only the failure. This avoids tool-call overhead and large output in context.

5. **Use `--fix` flags** — When the agent suggests a fix, consider if `cargo clippy --fix`, `cargo fmt`, or `slate lint --fix` could do it mechanically without burning credits.

### Model selection by task type

| Task | Recommended Model |
|------|-------------------|
| New WASM plugin | Sonnet / GPT-5-mini |
| New Lua script | Haiku / Sonnet |
| Bulk plugin.toml edits | Haiku |
| Architecture design | Opus |
| Complex debugging | Opus |
| Writing tests for pure functions | Sonnet |
| Documentation updates | Sonnet / Haiku |
| CI/build configuration | Sonnet |
