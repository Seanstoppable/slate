use anyhow::Result;
use std::path::Path;
use std::process::Command;

use slate_core::{App, SlateConfig};
use slate_plugin_host::{LuaPlugin, WasmPlugin};
use slate_plugin_manager::{Lockfile, PluginInstaller, Registry};
use slate_plugin_sdk::{Permissions, WidgetConfig, WidgetMetadata, WidgetContent};

/// A placeholder widget shown when a plugin fails to load.
struct ErrorWidget {
    name: String,
    error: String,
}

impl slate_plugin_sdk::Widget for ErrorWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: self.name.clone(),
            description: "Failed to load".to_string(),
            version: "0.0.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        WidgetContent::Text {
            content: format!("⚠ Plugin load error:\n\n{}", self.error),
            scrollable: true,
            wrap: true,
        }
    }
}

/// Try to load a widget, returning an ErrorWidget on failure instead of crashing.
fn load_widget_or_error(
    entry: &slate_core::config::WidgetEntry,
    widget_config: WidgetConfig,
) -> Box<dyn slate_plugin_sdk::Widget> {
    match try_load_widget(entry, widget_config.clone()) {
        Ok(widget) => widget,
        Err(e) => {
            let name = entry.widget_type.split('/').last()
                .or_else(|| entry.widget_type.split(':').last())
                .unwrap_or(&entry.widget_type)
                .to_string();
            eprintln!("Warning: Failed to load '{}': {}", entry.widget_type, e);
            Box::new(ErrorWidget {
                name,
                error: format!("{:#}", e),
            })
        }
    }
}

/// Attempt to load a single widget from a config entry.
fn try_load_widget(
    entry: &slate_core::config::WidgetEntry,
    widget_config: WidgetConfig,
) -> Result<Box<dyn slate_plugin_sdk::Widget>> {
    if entry.widget_type.starts_with("builtin:") {
        let name = entry.widget_type.trim_start_matches("builtin:");
        create_builtin(name, widget_config)
    } else if entry.widget_type.starts_with("lua:") {
        let path = entry.widget_type.trim_start_matches("lua:");
        let path = shellexpand::tilde(path);
        let mut widget = LuaPlugin::from_file(Path::new(path.as_ref()))?;
        slate_plugin_sdk::Widget::init(&mut widget, widget_config);
        Ok(Box::new(widget))
    } else if entry.widget_type.starts_with("wasm:") {
        let path = entry.widget_type.trim_start_matches("wasm:");
        let path = shellexpand::tilde(path);
        let wasm_path = std::path::PathBuf::from(path.as_ref());

        if wasm_path.exists() {
            let mut widget = WasmPlugin::from_file(&wasm_path, Permissions::default())?;
            slate_plugin_sdk::Widget::init(&mut widget, widget_config);
            Ok(Box::new(widget))
        } else {
            anyhow::bail!("WASM file not found: '{}'. Build it first.", wasm_path.display())
        }
    } else {
        // GitHub-sourced WASM plugin
        let plugin_name = entry
            .widget_type
            .split('/')
            .last()
            .unwrap_or(&entry.widget_type);

        let plugins_dir = PluginInstaller::default_dir()?;
        let wasm_path = plugins_dir.join(plugin_name).join(format!("{}.wasm", plugin_name));

        if wasm_path.exists() {
            let mut widget = WasmPlugin::from_file(&wasm_path, Permissions::default())?;
            slate_plugin_sdk::Widget::init(&mut widget, widget_config);
            Ok(Box::new(widget))
        } else {
            anyhow::bail!("Plugin '{}' not installed. Run `slate install` first.", entry.widget_type)
        }
    }
}

/// Run the dashboard.
pub async fn run(config_path: Option<&str>) -> Result<()> {
    let config = match config_path {
        Some(path) => SlateConfig::load_from(Path::new(path))?,
        None => SlateConfig::load_default()?,
    };

    let mut app = App::new(config.clone());

    // Load widgets based on config — failures are shown in-cell, not fatal
    for entry in &config.widget {
        let widget_config = WidgetConfig {
            position: entry.position.clone(),
            settings: entry
                .settings
                .iter()
                .map(|(k, v)| {
                    let json_val = toml_to_json(v);
                    (k.clone(), json_val)
                })
                .collect(),
            refresh_interval: entry.refresh_interval,
        };

        let widget = load_widget_or_error(entry, widget_config);
        app.add_widget(widget, entry.position.row, entry.position.col, entry.refresh_interval);
    }

    // If no widgets configured, show a welcome message
    if config.widget.is_empty() {
        let widget = WelcomeWidget;
        app.add_widget(Box::new(widget), 0, 0, None);
    }

    app.run()
}

/// Install all declared plugins.
pub async fn install() -> Result<()> {
    let config = SlateConfig::load_default()?;
    let installer = PluginInstaller::new(PluginInstaller::default_dir()?);
    let mut lockfile = Lockfile::load_default()?;

    for entry in &config.widget {
        if !entry.widget_type.starts_with("builtin:") && !entry.widget_type.starts_with("lua:") {
            println!("Installing {}...", entry.widget_type);
            match installer.install(&entry.widget_type, None).await {
                Ok(installed) => {
                    lockfile.lock(
                        &installed.name,
                        slate_plugin_manager::lockfile::LockedPlugin {
                            source: installed.source,
                            version: installed.version.clone(),
                            sha256: installed.sha256,
                            permissions_hash: None,
                        },
                    );
                    println!("  ✓ {} v{}", installed.name, installed.version);
                }
                Err(e) => {
                    eprintln!("  ✗ Failed: {}", e);
                }
            }
        }
    }

    lockfile.save_default()?;
    println!("Done. Lockfile updated.");
    Ok(())
}

/// Update plugins to latest versions.
pub async fn update() -> Result<()> {
    let config = SlateConfig::load_default()?;
    let installer = PluginInstaller::new(PluginInstaller::default_dir()?);
    let mut lockfile = Lockfile::load_default()?;

    for entry in &config.widget {
        if !entry.widget_type.starts_with("builtin:") && !entry.widget_type.starts_with("lua:") {
            let plugin_name = entry.widget_type.split('/').last().unwrap_or(&entry.widget_type);
            print!("Updating {}...", plugin_name);
            match installer.install(&entry.widget_type, None).await {
                Ok(installed) => {
                    let old_version = lockfile
                        .get(&installed.name)
                        .map(|l| l.version.clone())
                        .unwrap_or_else(|| "new".to_string());
                    lockfile.lock(
                        &installed.name,
                        slate_plugin_manager::lockfile::LockedPlugin {
                            source: installed.source,
                            version: installed.version.clone(),
                            sha256: installed.sha256,
                            permissions_hash: None,
                        },
                    );
                    println!(" {} → {}", old_version, installed.version);
                }
                Err(e) => {
                    println!(" failed: {}", e);
                }
            }
        }
    }

    lockfile.save_default()?;
    Ok(())
}

/// Show available updates.
pub async fn outdated() -> Result<()> {
    let lockfile = Lockfile::load_default()?;
    let installer = PluginInstaller::new(PluginInstaller::default_dir()?);
    let checker = slate_plugin_manager::update::UpdateChecker::new(installer);
    let updates = checker.check_outdated(&lockfile).await?;

    if updates.is_empty() {
        println!("All plugins are up to date.");
    } else {
        println!("{:<20} {:<12} {:<12} {}", "Plugin", "Current", "Latest", "Source");
        println!("{}", "-".repeat(70));
        for update in &updates {
            println!(
                "{:<20} {:<12} {:<12} {}",
                update.name, update.current_version, update.latest_version, update.source
            );
        }
    }
    Ok(())
}

/// List installed plugins.
pub async fn list() -> Result<()> {
    let installer = PluginInstaller::new(PluginInstaller::default_dir()?);
    let lockfile = Lockfile::load_default()?;

    let installed = installer.list_installed()?;
    if installed.is_empty() {
        println!("No plugins installed.");
    } else {
        println!("{:<20} {:<12} {}", "Plugin", "Version", "Source");
        println!("{}", "-".repeat(60));
        for name in &installed {
            if let Some(locked) = lockfile.get(name) {
                println!("{:<20} {:<12} {}", name, locked.version, locked.source);
            } else {
                println!("{:<20} {:<12} {}", name, "?", "unlocked");
            }
        }
    }
    Ok(())
}

/// Remove a plugin.
pub async fn remove(name: &str) -> Result<()> {
    let installer = PluginInstaller::new(PluginInstaller::default_dir()?);
    let mut lockfile = Lockfile::load_default()?;

    installer.remove(name)?;
    lockfile.unlock(name);
    lockfile.save_default()?;

    println!("Removed '{}'", name);
    Ok(())
}

/// Scaffold a new plugin project.
pub async fn create(name: &str) -> Result<()> {
    let dir = Path::new(name);
    std::fs::create_dir_all(dir.join("src"))?;

    // Cargo.toml for the plugin
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
extism-pdk = "1"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#
    );
    std::fs::write(dir.join("Cargo.toml"), cargo_toml)?;

    // plugin.toml
    let plugin_toml = format!(
        r#"[metadata]
name = "{name}"
description = "A Slate plugin"
version = "0.1.0"

[permissions]
# network = ["api.example.com"]
# storage = true
"#
    );
    std::fs::write(dir.join("plugin.toml"), plugin_toml)?;

    // src/lib.rs
    let lib_rs = r#"use extism_pdk::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": env!("CARGO_PKG_NAME"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "version": env!("CARGO_PKG_VERSION"),
    });
    Ok(meta.to_string())
}

#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    let content = json!({
        "type": "text",
        "content": "Hello from my plugin!",
        "scrollable": false,
        "wrap": true
    });
    Ok(content.to_string())
}

#[plugin_fn]
pub fn on_key(input: String) -> FnResult<String> {
    Ok(String::new())
}
"#;
    std::fs::write(dir.join("src").join("lib.rs"), lib_rs)?;

    println!("Created plugin scaffold in '{}'", name);
    println!("  Build with: cargo build --target wasm32-unknown-unknown --release");
    Ok(())
}

/// Search the plugin registry.
pub async fn search(query: &str) -> Result<()> {
    println!("Searching registry for '{}'...", query);
    let registry = Registry::fetch(None).await?;
    let results = registry.search(query);

    if results.is_empty() {
        println!("No plugins found.");
    } else {
        for entry in results {
            println!(
                "  {} - {} [{}]",
                entry.name,
                entry.description,
                entry.tags.join(", ")
            );
            println!("    source: {}", entry.source);
        }
    }
    Ok(())
}

/// Migrate a wtfutil config to Slate format.
pub async fn migrate(path: &str) -> Result<()> {
    println!("Migration from wtfutil configs is not yet implemented.");
    println!("Input: {}", path);
    println!("This will convert YAML widget configs to Slate TOML format.");
    Ok(())
}

/// Required WASM exports for a valid Slate plugin.
const REQUIRED_EXPORTS: &[&str] = &["metadata", "refresh"];
const OPTIONAL_EXPORTS: &[&str] = &["on_key", "on_action"];

/// Validate a WASM binary's exports without instantiating it.
fn validate_wasm_binary(path: &std::path::Path) -> Vec<String> {
    let mut issues = Vec::new();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            issues.push(format!("Cannot read file: {}", e));
            return issues;
        }
    };

    let parser = wasmparser::Parser::new(0);
    let mut found_exports: Vec<String> = Vec::new();
    let mut bad_imports: Vec<String> = Vec::new();

    for payload in parser.parse_all(&bytes) {
        match payload {
            Ok(wasmparser::Payload::ExportSection(reader)) => {
                for export in reader {
                    if let Ok(export) = export {
                        found_exports.push(export.name.to_string());
                    }
                }
            }
            Ok(wasmparser::Payload::ImportSection(reader)) => {
                for import in reader {
                    if let Ok(import) = import {
                        // Flag imports from unknown namespaces that Extism won't provide
                        let module = import.module;
                        let known_modules = [
                            "extism:host/env",
                            "extism:host/user",
                            "env",
                            "wasi_snapshot_preview1",
                            "wasi_unstable",
                        ];
                        if !known_modules.contains(&module) {
                            bad_imports.push(format!(
                                "{}::{} (unknown module '{}')",
                                module, import.name, module
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                issues.push(format!("Invalid WASM binary: {}", e));
                return issues;
            }
            _ => {}
        }
    }

    for required in REQUIRED_EXPORTS {
        if !found_exports.iter().any(|e| e == required) {
            issues.push(format!("Missing required export: '{}'", required));
        }
    }

    if !bad_imports.is_empty() {
        for imp in &bad_imports {
            issues.push(format!("Unresolvable import: {}", imp));
        }
    }

    let mut info_missing: Vec<&&str> = Vec::new();
    for opt in OPTIONAL_EXPORTS {
        if !found_exports.iter().any(|e| e == opt) {
            info_missing.push(opt);
        }
    }
    if !info_missing.is_empty() {
        issues.push(format!(
            "Optional exports not found (plugin may have limited interactivity): {}",
            info_missing.iter().map(|s| format!("'{}'", s)).collect::<Vec<_>>().join(", ")
        ));
    }

    issues
}

/// Resolve the WASM path for a widget entry (same logic as try_load_widget).
fn resolve_wasm_path(widget_type: &str) -> Result<std::path::PathBuf> {
    if widget_type.starts_with("wasm:") {
        let path = widget_type.trim_start_matches("wasm:");
        let path = shellexpand::tilde(path);
        Ok(std::path::PathBuf::from(path.as_ref()))
    } else {
        let plugin_name = widget_type
            .split('/')
            .last()
            .unwrap_or(widget_type);
        let plugins_dir = PluginInstaller::default_dir()?;
        Ok(plugins_dir.join(plugin_name).join(format!("{}.wasm", plugin_name)))
    }
}

/// Validate config and all plugins without launching the dashboard.
pub async fn check(config_path: Option<&str>) -> Result<()> {
    // 1. Validate config
    print!("Checking config... ");
    let config = match config_path {
        Some(path) => SlateConfig::load_from(Path::new(path)),
        None => SlateConfig::load_default(),
    };
    let config = match config {
        Ok(c) => {
            println!("✓ ({} widgets configured)", c.widget.len());
            c
        }
        Err(e) => {
            println!("✗ Config error: {:#}", e);
            return Ok(());
        }
    };

    let mut errors = 0u32;
    let mut warnings = 0u32;
    let mut ok = 0u32;

    for (i, entry) in config.widget.iter().enumerate() {
        let label = format!(
            "[{},{}] {}",
            entry.position.row, entry.position.col, entry.widget_type
        );

        if entry.widget_type.starts_with("builtin:") {
            let name = entry.widget_type.trim_start_matches("builtin:");
            let known = ["resource_usage", "power", "firewall", "ipaddresses", "vcs"];
            if known.contains(&name) {
                println!("  {:2}. {} ✓ builtin", i + 1, label);
                ok += 1;
            } else {
                println!("  {:2}. {} ✗ unknown builtin '{}'", i + 1, label, name);
                errors += 1;
            }
        } else if entry.widget_type.starts_with("lua:") {
            let path = entry.widget_type.trim_start_matches("lua:");
            let path = shellexpand::tilde(path);
            if Path::new(path.as_ref()).exists() {
                println!("  {:2}. {} ✓ lua script exists", i + 1, label);
                ok += 1;
            } else {
                println!("  {:2}. {} ✗ lua script not found", i + 1, label);
                errors += 1;
            }
        } else {
            // WASM plugin (local or GitHub-sourced)
            match resolve_wasm_path(&entry.widget_type) {
                Ok(wasm_path) => {
                    if !wasm_path.exists() {
                        println!("  {:2}. {} ✗ WASM file not found: {}", i + 1, label, wasm_path.display());
                        errors += 1;
                        continue;
                    }

                    let issues = validate_wasm_binary(&wasm_path);
                    let blocking: Vec<&String> = issues
                        .iter()
                        .filter(|s| !s.starts_with("Optional"))
                        .collect();

                    if blocking.is_empty() {
                        // Try actual Extism instantiation
                        match WasmPlugin::from_file(&wasm_path, Permissions::default()) {
                            Ok(_) => {
                                if issues.is_empty() {
                                    println!("  {:2}. {} ✓", i + 1, label);
                                } else {
                                    for issue in &issues {
                                        println!("  {:2}. {} ⚠ {}", i + 1, label, issue);
                                    }
                                    warnings += 1;
                                }
                                ok += 1;
                            }
                            Err(e) => {
                                println!("  {:2}. {} ✗ Extism load failed: {:#}", i + 1, label, e);
                                errors += 1;
                            }
                        }
                    } else {
                        for issue in &blocking {
                            println!("  {:2}. {} ✗ {}", i + 1, label, issue);
                        }
                        errors += 1;
                    }
                }
                Err(e) => {
                    println!("  {:2}. {} ✗ {}", i + 1, label, e);
                    errors += 1;
                }
            }
        }
    }

    println!();
    println!("Results: {} ok, {} warnings, {} errors", ok, warnings, errors);
    if errors > 0 {
        println!("Fix errors above before running `slate run`.");
    }

    Ok(())
}

/// Create a built-in widget by name.
fn create_builtin(name: &str, config: WidgetConfig) -> Result<Box<dyn slate_plugin_sdk::Widget>> {
    match name {
        "resource_usage" => Ok(Box::new(ResourceUsageWidget::new(config))),
        "power" => Ok(Box::new(PowerWidget::new())),
        "firewall" => Ok(Box::new(FirewallWidget::new())),
        "ipaddresses" => Ok(Box::new(IpAddressesWidget::new())),
        "vcs" => Ok(Box::new(VcsWidget::new(config))),
        _ => anyhow::bail!("Unknown builtin widget: {}", name),
    }
}

// --- Built-in widgets ---

struct WelcomeWidget;

impl slate_plugin_sdk::Widget for WelcomeWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Welcome".to_string(),
            description: "Welcome screen".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        WidgetContent::Text {
            content: concat!(
                "Welcome to Slate!\n\n",
                "Edit %APPDATA%\\slate\\slate.toml to add widgets.\n",
                "Run `slate search` to find plugins.\n",
                "Run `slate install` to install declared plugins.\n\n",
                "Press 'q' to quit."
            ).to_string(),
            scrollable: false,
            wrap: true,
        }
    }
}

struct ResourceUsageWidget {
    sys: sysinfo::System,
    components: sysinfo::Components,
}

impl ResourceUsageWidget {
    fn new(_config: WidgetConfig) -> Self {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        let components = sysinfo::Components::new_with_refreshed_list();
        Self { sys, components }
    }
}

impl slate_plugin_sdk::Widget for ResourceUsageWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Resources".to_string(),
            description: "System resource usage".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        self.sys.refresh_all();
        self.components.refresh(true);

        let cpu_usage = self.sys.global_cpu_usage();

        let total_mem = self.sys.total_memory();
        let used_mem = self.sys.used_memory();
        let mem_pct = if total_mem > 0 {
            (used_mem as f64 / total_mem as f64) * 100.0
        } else {
            0.0
        };
        let total_mem_gb = total_mem as f64 / 1_073_741_824.0;
        let used_mem_gb = used_mem as f64 / 1_073_741_824.0;

        let total_swap = self.sys.total_swap();
        let used_swap = self.sys.used_swap();
        let total_swap_gb = total_swap as f64 / 1_073_741_824.0;
        let used_swap_gb = used_swap as f64 / 1_073_741_824.0;

        let cpu_color = if cpu_usage > 80.0 {
            slate_plugin_sdk::Color::Red
        } else if cpu_usage > 50.0 {
            slate_plugin_sdk::Color::Yellow
        } else {
            slate_plugin_sdk::Color::Green
        };

        let mem_color = if mem_pct > 80.0 {
            slate_plugin_sdk::Color::Red
        } else if mem_pct > 50.0 {
            slate_plugin_sdk::Color::Yellow
        } else {
            slate_plugin_sdk::Color::Green
        };

        let mut pairs = vec![
            ("CPU".to_string(), slate_plugin_sdk::Cell::colored(
                format!("{:.1}%", cpu_usage), cpu_color
            )),
            ("Memory".to_string(), slate_plugin_sdk::Cell::colored(
                format!("{:.1}/{:.1} GB ({:.0}%)", used_mem_gb, total_mem_gb, mem_pct), mem_color
            )),
            ("Swap".to_string(), slate_plugin_sdk::Cell::plain(
                format!("{:.1}/{:.1} GB", used_swap_gb, total_swap_gb)
            )),
            ("CPUs".to_string(), slate_plugin_sdk::Cell::plain(
                format!("{} cores", self.sys.cpus().len())
            )),
        ];

        // Add temperature readings if available
        let temps: Vec<_> = self
            .components
            .iter()
            .filter_map(|component| component.temperature().map(|temp| (component, temp)))
            .filter(|(_, temp)| *temp > 0.0)
            .collect();
        if !temps.is_empty() {
            // Show hottest component
            if let Some((hottest, temp)) = temps.iter().max_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                let temp = *temp;
                let temp_color = if temp > 80.0 {
                    slate_plugin_sdk::Color::Red
                } else if temp > 60.0 {
                    slate_plugin_sdk::Color::Yellow
                } else {
                    slate_plugin_sdk::Color::Green
                };
                pairs.push(("Temp".to_string(), slate_plugin_sdk::Cell::colored(
                    format!("{:.0}°C ({})", temp, hottest.label()), temp_color
                )));
            }
        }

        WidgetContent::KeyValue { pairs }
    }
}

// --- Power Widget (builtin) ---

struct PowerWidget;

impl PowerWidget {
    fn new() -> Self {
        Self
    }
}

impl slate_plugin_sdk::Widget for PowerWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Power".to_string(),
            description: "Battery and power status".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let (has_battery, state, percent) = get_power_info();

        let state_color = match state.as_str() {
            "Charging" => slate_plugin_sdk::Color::Green,
            "Discharging" => {
                if percent < 20 { slate_plugin_sdk::Color::Red }
                else if percent < 50 { slate_plugin_sdk::Color::Yellow }
                else { slate_plugin_sdk::Color::Green }
            }
            "Critical" | "Low" => slate_plugin_sdk::Color::Red,
            _ => slate_plugin_sdk::Color::White,
        };

        let mut pairs = vec![
            ("Status".to_string(), slate_plugin_sdk::Cell::colored(state.clone(), state_color)),
        ];

        if has_battery {
            let pct_color = if percent < 20 { slate_plugin_sdk::Color::Red }
                else if percent < 50 { slate_plugin_sdk::Color::Yellow }
                else { slate_plugin_sdk::Color::Green };
            pairs.push(("Battery".to_string(), slate_plugin_sdk::Cell::colored(
                format!("{}%", percent), pct_color
            )));
        }

        pairs.push(("Source".to_string(), slate_plugin_sdk::Cell::plain(
            if has_battery && state == "Discharging" { "Battery".to_string() } else { "AC Power".to_string() }
        )));

        WidgetContent::KeyValue { pairs }
    }
}

fn get_power_info() -> (bool, String, u64) {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command",
                "(Get-CimInstance Win32_Battery | Select-Object EstimatedChargeRemaining, BatteryStatus | ConvertTo-Json) 2>$null; if (-not $?) { Write-Output '{\"ac_power\": true}' }"])
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                if val.get("ac_power").is_some() {
                    return (false, "AC Power".to_string(), 100);
                }
                let percent = val["EstimatedChargeRemaining"].as_u64().unwrap_or(0);
                let status = match val["BatteryStatus"].as_u64().unwrap_or(0) {
                    1 => "Discharging",
                    2 => "AC Power",
                    3 => "Fully Charged",
                    4 => "Low",
                    5 => "Critical",
                    6..=8 => "Charging",
                    _ => "Unknown",
                };
                return (true, status.to_string(), percent);
            }
        }
        (false, "AC Power".to_string(), 100)
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("pmset").args(["-g", "batt"]).output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.contains("InternalBattery") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    let percent = parts.iter()
                        .find(|p| p.ends_with("%;"))
                        .map(|p| p.trim_end_matches("%;").parse::<u64>().unwrap_or(0))
                        .unwrap_or(0);
                    let state = if line.contains("charging") { "Charging" }
                        else if line.contains("discharging") { "Discharging" }
                        else { "Fully Charged" };
                    return (true, state.to_string(), percent);
                }
            }
        }
        (false, "AC Power".to_string(), 100)
    }
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("cat")
            .arg("/sys/class/power_supply/BAT0/capacity")
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                let percent: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0);
                let status_out = Command::new("cat")
                    .arg("/sys/class/power_supply/BAT0/status")
                    .output();
                let state = status_out
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|_| "Unknown".to_string());
                return (true, state, percent);
            }
        }
        (false, "AC Power".to_string(), 100)
    }
}

// --- Firewall Widget (builtin) ---

struct FirewallWidget;

impl FirewallWidget {
    fn new() -> Self {
        Self
    }
}

impl slate_plugin_sdk::Widget for FirewallWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Firewall".to_string(),
            description: "Firewall status and rules".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let (platform, enabled, rules) = get_firewall_info();

        let status_color = if enabled { slate_plugin_sdk::Color::Green } else { slate_plugin_sdk::Color::Red };
        let mut items = vec![
            slate_plugin_sdk::ListItem {
                id: "status".to_string(),
                title: format!("Firewall: {}", if enabled { "Enabled" } else { "Disabled" }),
                subtitle: Some(format!("Platform: {}", platform)),
                icon: None,
                style: slate_plugin_sdk::CellStyle { fg: Some(status_color), ..Default::default() },
            }
        ];

        for (i, rule) in rules.iter().enumerate() {
            items.push(slate_plugin_sdk::ListItem {
                id: format!("rule-{}", i),
                title: rule.clone(),
                subtitle: None,
                icon: None,
                style: Default::default(),
            });
        }

        WidgetContent::List {
            items,
            selectable: true,
            actions: vec![],
        }
    }
}

fn get_firewall_info() -> (String, bool, Vec<String>) {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("netsh")
            .args(["advfirewall", "show", "allprofiles", "state"])
            .output();
        let enabled = if let Ok(out) = &output {
            String::from_utf8_lossy(&out.stdout).contains("ON")
        } else {
            false
        };

        let rules_output = Command::new("netsh")
            .args(["advfirewall", "firewall", "show", "rule", "name=all", "dir=in"])
            .output();
        let mut rules = Vec::new();
        if let Ok(out) = rules_output {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut name = String::new();
            let mut action = String::new();
            let mut port = String::new();
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("Rule Name:") {
                    if !name.is_empty() {
                        rules.push(format!("{} {} IN/{}", action.to_uppercase(), name, port));
                    }
                    name = trimmed.trim_start_matches("Rule Name:").trim().to_string();
                    action = "Allow".to_string();
                    port = "Any".to_string();
                } else if trimmed.starts_with("Action:") {
                    action = trimmed.trim_start_matches("Action:").trim().to_string();
                } else if trimmed.starts_with("LocalPort:") {
                    port = trimmed.trim_start_matches("LocalPort:").trim().to_string();
                }
                if rules.len() >= 15 { break; }
            }
            if !name.is_empty() && rules.len() < 15 {
                rules.push(format!("{} {} IN/{}", action.to_uppercase(), name, port));
            }
        }

        ("Windows".to_string(), enabled, rules)
    }
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("ufw").args(["status"]).output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            let enabled = text.contains("Status: active");
            let rules: Vec<String> = text.lines().skip(4).take(15)
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            return ("Linux (ufw)".to_string(), enabled, rules);
        }
        let ipt = Command::new("iptables").args(["-L", "-n", "--line-numbers"]).output();
        if let Ok(out) = ipt {
            let text = String::from_utf8_lossy(&out.stdout);
            let rules: Vec<String> = text.lines().skip(2).take(15)
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            return ("Linux (iptables)".to_string(), true, rules);
        }
        ("Linux".to_string(), false, Vec::new())
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("pfctl").args(["-sr"]).output();
        let rules: Vec<String> = if let Ok(out) = output {
            String::from_utf8_lossy(&out.stdout).lines().take(15)
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        } else {
            Vec::new()
        };
        ("macOS (pf)".to_string(), !rules.is_empty(), rules)
    }
}

// --- IP Addresses Widget (builtin) ---

struct IpAddressesWidget;

impl IpAddressesWidget {
    fn new() -> Self {
        Self
    }
}

impl slate_plugin_sdk::Widget for IpAddressesWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "IP Addresses".to_string(),
            description: "Network interface addresses".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let interfaces = get_network_interfaces();

        if interfaces.is_empty() {
            return WidgetContent::Text {
                content: "No network interfaces found".to_string(),
                scrollable: false,
                wrap: true,
            };
        }

        let pairs: Vec<(String, slate_plugin_sdk::Cell)> = interfaces
            .into_iter()
            .map(|(name, ip)| {
                let display = if ip.is_empty() { "—".to_string() } else { ip };
                (name, slate_plugin_sdk::Cell::plain(display))
            })
            .collect();

        WidgetContent::KeyValue { pairs }
    }
}

fn get_network_interfaces() -> Vec<(String, String)> {
    let networks = sysinfo::Networks::new_with_refreshed_list();
    let mut results: Vec<(String, String)> = Vec::new();

    for (name, _data) in networks.iter() {
        let ip = get_interface_ip(name);
        results.push((name.clone(), ip));
    }

    results
}

fn get_interface_ip(interface_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("netsh")
            .args(["interface", "ip", "show", "addresses", interface_name])
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("IP Address:") || trimmed.starts_with("IP") {
                    if let Some(ip) = trimmed.split_whitespace().last() {
                        if ip.contains('.') {
                            return ip.to_string();
                        }
                    }
                }
            }
        }
        String::new()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("ip")
            .args(["-4", "addr", "show", interface_name])
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("inet ") {
                    if let Some(addr) = trimmed.split_whitespace().nth(1) {
                        return addr.split('/').next().unwrap_or("").to_string();
                    }
                }
            }
        }
        String::new()
    }
}

// --- VCS Widget (builtin) ---

struct VcsWidget {
    engine: String,
    repo_path: String,
}

impl VcsWidget {
    fn new(config: WidgetConfig) -> Self {
        let engine = config.settings.get("engine")
            .and_then(|v| v.as_str())
            .unwrap_or("git")
            .to_string();
        let repo_path = config.settings.get("repo_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();
        Self { engine, repo_path }
    }
}

impl slate_plugin_sdk::Widget for VcsWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: format!("VCS ({})", self.engine),
            description: "Version control status".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, config: WidgetConfig) {
        if let Some(e) = config.settings.get("engine").and_then(|v| v.as_str()) {
            self.engine = e.to_string();
        }
        if let Some(p) = config.settings.get("repo_path").and_then(|v| v.as_str()) {
            self.repo_path = p.to_string();
        }
    }

    fn refresh(&mut self) -> WidgetContent {
        if self.repo_path.trim().is_empty() || self.repo_path == "." {
            return WidgetContent::Text {
                content: "Configure repo_path in settings".to_string(),
                scrollable: false,
                wrap: true,
            };
        }

        let path = std::path::Path::new(&self.repo_path);
        if !path.exists() {
            return WidgetContent::Text {
                content: format!("Repo path not found: {}", self.repo_path),
                scrollable: false,
                wrap: true,
            };
        }

        let (branch, status_entries, log_entries) = match self.engine.as_str() {
            "hg" => get_hg_info(&self.repo_path),
            _ => get_git_info(&self.repo_path),
        };

        let mut modified = 0usize;
        let mut added = 0usize;
        let mut deleted = 0usize;
        let mut untracked = 0usize;
        for (state, _) in &status_entries {
            match state.as_str() {
                "modified" => modified += 1,
                "added" => added += 1,
                "deleted" => deleted += 1,
                "untracked" => untracked += 1,
                _ => {}
            }
        }

        let mut summary_parts = Vec::new();
        if modified > 0 { summary_parts.push(format!("{modified} modified")); }
        if added > 0 { summary_parts.push(format!("{added} added")); }
        if deleted > 0 { summary_parts.push(format!("{deleted} deleted")); }
        if untracked > 0 { summary_parts.push(format!("{untracked} untracked")); }

        let status_summary = if summary_parts.is_empty() {
            "clean".to_string()
        } else {
            summary_parts.join(", ")
        };

        let status_color = if summary_parts.is_empty() {
            slate_plugin_sdk::Color::Green
        } else {
            slate_plugin_sdk::Color::Yellow
        };

        let mut pairs = vec![
            ("Engine".to_string(), slate_plugin_sdk::Cell::plain(self.engine.clone())),
            ("Branch".to_string(), slate_plugin_sdk::Cell::plain(
                if branch.is_empty() { "(detached)".to_string() } else { branch }
            )),
            ("Status".to_string(), slate_plugin_sdk::Cell::colored(status_summary, status_color)),
        ];

        for (i, (hash, message, author, date)) in log_entries.iter().take(5).enumerate() {
            let key = if i == 0 { "Last commit".to_string() } else { format!("Recent {}", i + 1) };
            let mut val = format!("{} {}", hash, message);
            if !author.is_empty() || !date.is_empty() {
                let extra: Vec<&str> = [author.as_str(), date.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect();
                val.push_str(&format!(" ({})", extra.join(" • ")));
            }
            pairs.push((key, slate_plugin_sdk::Cell::plain(val)));
        }

        if log_entries.is_empty() {
            pairs.push(("Last commit".to_string(), slate_plugin_sdk::Cell::plain("No commits available".to_string())));
        }

        WidgetContent::KeyValue { pairs }
    }
}

/// Returns (branch, status_entries[(state, file)], log_entries[(hash, message, author, date)])
fn get_git_info(repo_path: &str) -> (String, Vec<(String, String)>, Vec<(String, String, String, String)>) {
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let status: Vec<(String, String)> = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.len() >= 3).map(|line| {
                let state = match &line[..2] {
                    " M" | "M " | "MM" => "modified",
                    "A " | "AM" => "added",
                    " D" | "D " => "deleted",
                    "??" => "untracked",
                    _ => "other",
                };
                (state.to_string(), line[3..].to_string())
            }).collect()
        })
        .unwrap_or_default();

    let log: Vec<(String, String, String, String)> = Command::new("git")
        .args(["log", "--oneline", "-10", "--format=%h|%s|%an|%ar"])
        .current_dir(repo_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.is_empty()).map(|line| {
                let parts: Vec<&str> = line.splitn(4, '|').collect();
                (
                    parts.first().unwrap_or(&"").to_string(),
                    parts.get(1).unwrap_or(&"").to_string(),
                    parts.get(2).unwrap_or(&"").to_string(),
                    parts.get(3).unwrap_or(&"").to_string(),
                )
            }).collect()
        })
        .unwrap_or_default();

    (branch, status, log)
}

fn get_hg_info(repo_path: &str) -> (String, Vec<(String, String)>, Vec<(String, String, String, String)>) {
    let branch = Command::new("hg")
        .args(["branch"])
        .current_dir(repo_path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "default".to_string());

    let status: Vec<(String, String)> = Command::new("hg")
        .args(["status"])
        .current_dir(repo_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.len() >= 2).map(|line| {
                let state = match line.chars().next().unwrap_or(' ') {
                    'M' => "modified",
                    'A' => "added",
                    'R' => "deleted",
                    '?' => "untracked",
                    _ => "other",
                };
                (state.to_string(), line.get(2..).unwrap_or("").to_string())
            }).collect()
        })
        .unwrap_or_default();

    let log: Vec<(String, String, String, String)> = Command::new("hg")
        .args(["log", "-l", "10", "--template", "{short(node)}|{desc|firstline}|{author|user}|{date|age}\n"])
        .current_dir(repo_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.is_empty()).map(|line| {
                let parts: Vec<&str> = line.splitn(4, '|').collect();
                (
                    parts.first().unwrap_or(&"").to_string(),
                    parts.get(1).unwrap_or(&"").to_string(),
                    parts.get(2).unwrap_or(&"").to_string(),
                    parts.get(3).unwrap_or(&"").to_string(),
                )
            }).collect()
        })
        .unwrap_or_default();

    (branch, status, log)
}

fn toml_to_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(*i),
        toml::Value::Float(f) => serde_json::json!(*f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_to_json).collect())
        }
        toml::Value::Table(table) => {
            let map: serde_json::Map<String, serde_json::Value> = table
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::Widget;

    #[test]
    fn toml_to_json_converts_all_supported_value_types() {
        let datetime = "2024-01-02T03:04:05Z"
            .parse::<toml::value::Datetime>()
            .unwrap();
        let array = toml::Value::Array(vec![
            toml::Value::Integer(1),
            toml::Value::String("two".to_string()),
        ]);
        let table = toml::Value::Table(toml::map::Map::from_iter([
            ("enabled".to_string(), toml::Value::Boolean(true)),
            ("count".to_string(), toml::Value::Integer(3)),
        ]));

        assert_eq!(
            toml_to_json(&toml::Value::String("value".to_string())),
            serde_json::json!("value")
        );
        assert_eq!(toml_to_json(&toml::Value::Integer(7)), serde_json::json!(7));
        assert_eq!(toml_to_json(&toml::Value::Float(2.5)), serde_json::json!(2.5));
        assert_eq!(
            toml_to_json(&toml::Value::Boolean(false)),
            serde_json::json!(false)
        );
        assert_eq!(toml_to_json(&array), serde_json::json!([1, "two"]));
        assert_eq!(
            toml_to_json(&table),
            serde_json::json!({"enabled": true, "count": 3})
        );
        assert_eq!(
            toml_to_json(&toml::Value::Datetime(datetime)),
            serde_json::json!("2024-01-02T03:04:05Z")
        );
    }

    #[test]
    fn resource_usage_widget_returns_expected_key_value_content() {
        let mut widget = ResourceUsageWidget::new(WidgetConfig {
            position: slate_plugin_sdk::Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: None,
        });

        match widget.refresh() {
            WidgetContent::KeyValue { pairs } => {
                let keys: Vec<&str> = pairs.iter().map(|(key, _)| key.as_str()).collect();
                assert_eq!(keys, vec!["CPU", "Memory", "Swap", "CPUs"]);
                assert!(pairs.iter().all(|(_, cell)| !cell.text.is_empty()));
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }
}
