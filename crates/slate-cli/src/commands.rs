use anyhow::Result;
use slate_core::{App, SlateConfig};
use slate_plugin_host::{LuaPlugin, WasmPlugin};
use slate_plugin_manager::{Lockfile, PluginInstaller, Registry};
use slate_plugin_sdk::{Permissions, WidgetConfig, WidgetContent, WidgetMetadata};
use std::path::Path;

use crate::builtins;

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
            let name = entry
                .widget_type
                .split('/')
                .last()
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
        builtins::create_builtin(name, widget_config)
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
            check_os_support(&wasm_path)?;
            let mut widget = WasmPlugin::from_file(&wasm_path, Permissions::default())?;
            slate_plugin_sdk::Widget::init(&mut widget, widget_config);
            Ok(Box::new(widget))
        } else {
            anyhow::bail!(
                "WASM file not found: '{}'. Build it first.",
                wasm_path.display()
            )
        }
    } else {
        // GitHub-sourced WASM plugin
        let plugin_name = entry
            .widget_type
            .split('/')
            .last()
            .unwrap_or(&entry.widget_type);

        let plugins_dir = PluginInstaller::default_dir()?;
        let wasm_path = plugins_dir
            .join(plugin_name)
            .join(format!("{}.wasm", plugin_name));

        if wasm_path.exists() {
            check_os_support(&wasm_path)?;
            let mut widget = WasmPlugin::from_file(&wasm_path, Permissions::default())?;
            slate_plugin_sdk::Widget::init(&mut widget, widget_config);
            Ok(Box::new(widget))
        } else {
            anyhow::bail!(
                "Plugin '{}' not installed. Run `slate install` first.",
                entry.widget_type
            )
        }
    }
}

/// Plugin manifest read from plugin.toml alongside a WASM file.
#[derive(serde::Deserialize, Default)]
struct PluginManifest {
    #[serde(default)]
    plugin: PluginManifestPlugin,
}

#[derive(serde::Deserialize, Default)]
struct PluginManifestPlugin {
    #[serde(default)]
    name: String,
    #[serde(default)]
    os: Vec<String>,
}

/// Find and parse plugin.toml next to a WASM file (in same dir or parent dir).
fn read_plugin_manifest(wasm_path: &std::path::Path) -> Option<PluginManifest> {
    let dir = wasm_path.parent()?;
    // Check same directory first, then parent (for plugins with target/ subdirs)
    for candidate in [
        dir.join("plugin.toml"),
        dir.parent()
            .map(|p| p.join("plugin.toml"))
            .unwrap_or_default(),
    ] {
        if candidate.exists() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                if let Ok(manifest) = toml::from_str::<PluginManifest>(&content) {
                    return Some(manifest);
                }
            }
        }
    }
    None
}

/// Get the current OS as a normalized string matching plugin.toml conventions.
fn current_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    }
}

/// Check if a plugin supports the current OS. Returns Ok if supported or unspecified.
fn check_os_support(wasm_path: &std::path::Path) -> Result<()> {
    if let Some(manifest) = read_plugin_manifest(wasm_path) {
        if !manifest.plugin.os.is_empty() {
            let os = current_os();
            if !manifest.plugin.os.iter().any(|s| s == os) {
                let supported = manifest.plugin.os.join(", ");
                let _name = if manifest.plugin.name.is_empty() {
                    wasm_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("plugin")
                        .to_string()
                } else {
                    manifest.plugin.name.clone()
                };
                anyhow::bail!("Not available on {} (supports: {})", os, supported);
            }
        }
    }
    Ok(())
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
        app.add_widget(
            widget,
            entry.position.row,
            entry.position.col,
            entry.refresh_interval,
        );
    }

    // If no widgets configured, show a welcome message
    if config.widget.is_empty() {
        let widget = builtins::WelcomeWidget;
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
            let plugin_name = entry
                .widget_type
                .split('/')
                .last()
                .unwrap_or(&entry.widget_type);
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
        println!(
            "{:<20} {:<12} {:<12} {}",
            "Plugin", "Current", "Latest", "Source"
        );
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
            info_missing
                .iter()
                .map(|s| format!("'{}'", s))
                .collect::<Vec<_>>()
                .join(", ")
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
        let plugin_name = widget_type.split('/').last().unwrap_or(widget_type);
        let plugins_dir = PluginInstaller::default_dir()?;
        Ok(plugins_dir
            .join(plugin_name)
            .join(format!("{}.wasm", plugin_name)))
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
                        println!(
                            "  {:2}. {} ✗ WASM file not found: {}",
                            i + 1,
                            label,
                            wasm_path.display()
                        );
                        errors += 1;
                        continue;
                    }

                    // Check OS support from plugin.toml
                    if let Some(manifest) = read_plugin_manifest(&wasm_path) {
                        if !manifest.plugin.os.is_empty() {
                            let os = current_os();
                            if !manifest.plugin.os.iter().any(|s| s == os) {
                                let supported = manifest.plugin.os.join(", ");
                                println!(
                                    "  {:2}. {} ⊘ Not available on {} (supports: {})",
                                    i + 1,
                                    label,
                                    os,
                                    supported
                                );
                                warnings += 1;
                                continue;
                            }
                        }
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
    println!(
        "Results: {} ok, {} warnings, {} errors",
        ok, warnings, errors
    );
    if errors > 0 {
        println!("Fix errors above before running `slate run`.");
    }

    Ok(())
}

fn toml_to_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(*i),
        toml::Value::Float(f) => serde_json::json!(*f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(toml_to_json).collect()),
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
    use crate::builtins;
    use slate_plugin_sdk::Position;
    use slate_plugin_sdk::Widget;
    use tempfile::tempdir;

    fn widget_config() -> WidgetConfig {
        WidgetConfig {
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: None,
        }
    }

    fn write_wasm(path: &std::path::Path, source: &str) {
        std::fs::write(path, wat::parse_str(source).unwrap()).unwrap();
    }

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
        assert_eq!(
            toml_to_json(&toml::Value::Float(2.5)),
            serde_json::json!(2.5)
        );
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
    fn error_widget_displays_error_message() {
        let mut widget = ErrorWidget {
            name: "test".to_string(),
            error: "something broke".to_string(),
        };
        let meta = widget.metadata();
        assert_eq!(meta.name, "test");
        assert_eq!(meta.description, "Failed to load");

        let content = widget.refresh();
        match content {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("something broke"));
                assert!(content.contains("Plugin load error"));
            }
            other => panic!("expected text, got {:?}", other),
        }
    }

    #[test]
    fn read_plugin_manifest_finds_manifest_in_same_directory() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("widget.wasm");
        std::fs::write(&wasm_path, b"wasm").unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            "[plugin]\nname = \"same-dir\"\nos = [\"windows\"]\n",
        )
        .unwrap();

        let manifest = read_plugin_manifest(&wasm_path).unwrap();
        assert_eq!(manifest.plugin.name, "same-dir");
        assert_eq!(manifest.plugin.os, vec!["windows"]);
    }

    #[test]
    fn read_plugin_manifest_finds_manifest_in_parent_directory() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("target").join("wasm32");
        std::fs::create_dir_all(&nested).unwrap();
        let wasm_path = nested.join("widget.wasm");
        std::fs::write(&wasm_path, b"wasm").unwrap();
        std::fs::write(
            dir.path().join("target").join("plugin.toml"),
            "[plugin]\nname = \"parent-dir\"\nos = [\"linux\"]\n",
        )
        .unwrap();

        let manifest = read_plugin_manifest(&wasm_path).unwrap();
        assert_eq!(manifest.plugin.name, "parent-dir");
        assert_eq!(manifest.plugin.os, vec!["linux"]);
    }

    #[test]
    fn read_plugin_manifest_returns_none_when_manifest_is_missing_or_invalid() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("widget.wasm");
        std::fs::write(&wasm_path, b"wasm").unwrap();
        assert!(read_plugin_manifest(&wasm_path).is_none());

        std::fs::write(dir.path().join("plugin.toml"), "not valid = [").unwrap();
        assert!(read_plugin_manifest(&wasm_path).is_none());
    }

    #[test]
    fn current_os_matches_platform_constant() {
        let expected = match std::env::consts::OS {
            "macos" => "macos",
            "linux" => "linux",
            "windows" => "windows",
            other => other,
        };
        assert_eq!(current_os(), expected);
    }

    #[test]
    fn check_os_support_allows_missing_empty_and_matching_manifests() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("widget.wasm");
        std::fs::write(&wasm_path, b"wasm").unwrap();

        assert!(check_os_support(&wasm_path).is_ok());

        std::fs::write(dir.path().join("plugin.toml"), "[plugin]\nos = []\n").unwrap();
        assert!(check_os_support(&wasm_path).is_ok());

        std::fs::write(
            dir.path().join("plugin.toml"),
            format!("[plugin]\nname = \"widget\"\nos = [\"{}\"]\n", current_os()),
        )
        .unwrap();
        assert!(check_os_support(&wasm_path).is_ok());
    }

    #[test]
    fn check_os_support_rejects_unsupported_os() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("widget.wasm");
        std::fs::write(&wasm_path, b"wasm").unwrap();

        let unsupported = ["linux", "macos", "windows"]
            .into_iter()
            .find(|os| *os != current_os())
            .unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            format!("[plugin]\nname = \"widget\"\nos = [\"{}\"]\n", unsupported),
        )
        .unwrap();

        let error = check_os_support(&wasm_path).unwrap_err();
        assert!(error.to_string().contains("Not available on"));
        assert!(error.to_string().contains(unsupported));
    }

    #[test]
    fn validate_wasm_binary_reports_missing_required_exports_for_minimal_module() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("minimal.wasm");
        write_wasm(&wasm_path, "(module)");

        let issues = validate_wasm_binary(&wasm_path);
        assert!(issues
            .iter()
            .any(|issue| issue == "Missing required export: 'metadata'"));
        assert!(issues
            .iter()
            .any(|issue| issue == "Missing required export: 'refresh'"));
        assert!(issues
            .iter()
            .any(|issue| issue.contains("Optional exports not found")));
    }

    #[test]
    fn validate_wasm_binary_reports_invalid_binary() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("invalid.wasm");
        std::fs::write(&wasm_path, b"definitely-not-wasm").unwrap();

        let issues = validate_wasm_binary(&wasm_path);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("Invalid WASM binary"));
    }

    #[test]
    fn validate_wasm_binary_accepts_module_with_required_and_optional_exports() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("valid.wasm");
        write_wasm(
            &wasm_path,
            r#"
                (module
                    (func (export "metadata"))
                    (func (export "refresh"))
                    (func (export "on_key"))
                    (func (export "on_action"))
                )
            "#,
        );

        assert!(validate_wasm_binary(&wasm_path).is_empty());
    }

    #[test]
    fn validate_wasm_binary_with_valid_required_exports_only_has_no_errors() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("valid-required-only.wasm");
        write_wasm(
            &wasm_path,
            r#"
                (module
                    (func (export "metadata") (result i32) (i32.const 0))
                    (func (export "refresh") (result i32) (i32.const 0))
                )
            "#,
        );

        let issues = validate_wasm_binary(&wasm_path);
        let errors: Vec<_> = issues
            .iter()
            .filter(|issue| !issue.starts_with("Optional"))
            .collect();
        assert!(errors.is_empty(), "unexpected issues: {:?}", errors);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("Optional exports not found"))
        );
    }

    #[test]
    fn validate_wasm_binary_reports_unresolvable_imports() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("imports.wasm");
        write_wasm(
            &wasm_path,
            r#"
                (module
                    (import "mystery" "call" (func))
                    (func (export "metadata"))
                    (func (export "refresh"))
                )
            "#,
        );

        let issues = validate_wasm_binary(&wasm_path);
        assert!(issues
            .iter()
            .any(|issue| issue.contains("Unresolvable import: mystery::call")));
    }

    #[test]
    fn resolve_wasm_path_handles_explicit_paths_and_github_sources() {
        let direct = resolve_wasm_path("wasm:path\\to\\widget.wasm").unwrap();
        assert_eq!(direct, std::path::PathBuf::from("path\\to\\widget.wasm"));

        let home = std::path::PathBuf::from(shellexpand::tilde("~/widget.wasm").as_ref());
        assert_eq!(resolve_wasm_path("wasm:~/widget.wasm").unwrap(), home);

        let expected = PluginInstaller::default_dir()
            .unwrap()
            .join("repo")
            .join("repo.wasm");
        assert_eq!(
            resolve_wasm_path("github.com/owner/repo").unwrap(),
            expected
        );
    }

    #[test]
    fn toml_to_json_converts_nested_structures_recursively() {
        let value = toml::Value::Table(toml::map::Map::from_iter([(
            "nested".to_string(),
            toml::Value::Array(vec![toml::Value::Table(toml::map::Map::from_iter([(
                "flag".to_string(),
                toml::Value::Boolean(true),
            )]))]),
        )]));

        assert_eq!(
            toml_to_json(&value),
            serde_json::json!({"nested": [{"flag": true}]})
        );
    }

    #[test]
    fn toml_to_json_preserves_nested_datetimes() {
        let datetime = "2024-01-02T03:04:05Z"
            .parse::<toml::value::Datetime>()
            .unwrap();
        let value = toml::Value::Table(toml::map::Map::from_iter([(
            "schedule".to_string(),
            toml::Value::Table(toml::map::Map::from_iter([(
                "start".to_string(),
                toml::Value::Datetime(datetime),
            )])),
        )]));

        assert_eq!(
            toml_to_json(&value),
            serde_json::json!({"schedule": {"start": "2024-01-02T03:04:05Z"}})
        );
    }

    #[test]
    fn create_builtin_returns_all_known_widgets() {
        assert!(builtins::create_builtin("resource_usage", widget_config()).is_ok());
        assert!(builtins::create_builtin("power", widget_config()).is_ok());
        assert!(builtins::create_builtin("firewall", widget_config()).is_ok());
        assert!(builtins::create_builtin("ipaddresses", widget_config()).is_ok());
        assert!(builtins::create_builtin("vcs", widget_config()).is_ok());
        assert!(builtins::create_builtin("nonexistent", widget_config()).is_err());
    }

    #[tokio::test]
    async fn create_scaffolds_plugin_project() {
        let dir = tempdir().unwrap();
        let name = dir.path().join("test-plugin");

        create(name.to_str().unwrap()).await.unwrap();

        let cargo_path = name.join("Cargo.toml");
        let plugin_path = name.join("plugin.toml");
        let lib_path = name.join("src").join("lib.rs");

        assert!(cargo_path.exists());
        assert!(plugin_path.exists());
        assert!(lib_path.exists());

        let cargo = std::fs::read_to_string(cargo_path).unwrap();
        assert!(cargo.contains("test-plugin"));
        assert!(cargo.contains("crate-type = [\"cdylib\"]"));
        assert!(cargo.contains("extism-pdk = \"1\""));

        let plugin = std::fs::read_to_string(plugin_path).unwrap();
        assert!(plugin.contains("[metadata]"));
        assert!(plugin.contains("test-plugin"));
        assert!(plugin.contains("description = \"A Slate plugin\""));

        let lib = std::fs::read_to_string(lib_path).unwrap();
        assert!(lib.contains("#[plugin_fn]"));
        assert!(lib.contains("pub fn metadata"));
        assert!(lib.contains("pub fn refresh"));
        assert!(lib.contains("Hello from my plugin!"));
    }

    #[tokio::test]
    async fn check_validates_builtin_widgets() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("slate.toml");
        std::fs::write(
            &config_path,
            r#"
[[widget]]
type = "builtin:resource_usage"
position = { row = 0, col = 0 }

[[widget]]
type = "builtin:power"
position = { row = 0, col = 1 }
"#,
        )
        .unwrap();

        check(Some(config_path.to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn check_reports_unknown_builtin_without_erroring() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("slate.toml");
        std::fs::write(
            &config_path,
            r#"
[[widget]]
type = "builtin:not-real"
position = { row = 0, col = 0 }
"#,
        )
        .unwrap();

        check(Some(config_path.to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn check_reports_missing_lua_script_without_erroring() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("slate.toml");
        let lua_path = dir.path().join("missing.lua");
        let config = format!(
            r#"
[[widget]]
type = "lua:{}"
position = {{ row = 0, col = 0 }}
"#,
            lua_path.display()
        );
        std::fs::write(&config_path, config).unwrap();

        check(Some(config_path.to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn migrate_does_not_error() {
        migrate("nonexistent.yml").await.unwrap();
    }
}
