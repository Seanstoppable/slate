use anyhow::Result;
use std::path::Path;

use slate_core::{App, SlateConfig};
use slate_plugin_host::{LuaPlugin, WasmPlugin};
use slate_plugin_manager::{Lockfile, PluginInstaller, Registry};
use slate_plugin_sdk::{Permissions, WidgetConfig, WidgetMetadata, WidgetContent, Position};

/// Run the dashboard.
pub async fn run(config_path: Option<&str>) -> Result<()> {
    let config = match config_path {
        Some(path) => SlateConfig::load_from(Path::new(path))?,
        None => SlateConfig::load_default()?,
    };

    let mut app = App::new(config.clone());

    // Load widgets based on config
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

        if entry.widget_type.starts_with("builtin:") {
            let name = entry.widget_type.trim_start_matches("builtin:");
            let widget = create_builtin(name, widget_config)?;
            app.add_widget(widget, entry.position.row, entry.position.col, entry.refresh_interval);
        } else if entry.widget_type.starts_with("lua:") {
            let path = entry.widget_type.trim_start_matches("lua:");
            let path = shellexpand::tilde(path);
            let mut widget = LuaPlugin::from_file(Path::new(path.as_ref()))?;
            slate_plugin_sdk::Widget::init(&mut widget, widget_config);
            app.add_widget(Box::new(widget), entry.position.row, entry.position.col, entry.refresh_interval);
        } else {
            // GitHub-sourced WASM plugin
            let installer = PluginInstaller::new(PluginInstaller::default_dir()?);
            let lockfile = Lockfile::load_default()?;
            // Look for installed WASM file
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
                app.add_widget(Box::new(widget), entry.position.row, entry.position.col, entry.refresh_interval);
            } else {
                eprintln!(
                    "Warning: Plugin '{}' not installed. Run `slate install` first.",
                    entry.widget_type
                );
            }
        }
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

/// Create a built-in widget by name.
fn create_builtin(name: &str, config: WidgetConfig) -> Result<Box<dyn slate_plugin_sdk::Widget>> {
    match name {
        "resource_usage" => Ok(Box::new(ResourceUsageWidget::new(config))),
        "clock" => Ok(Box::new(ClockWidget)),
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
                "Edit ~/.config/slate/slate.toml to add widgets.\n",
                "Run `slate search` to find plugins.\n",
                "Run `slate install` to install declared plugins.\n\n",
                "Press 'q' to quit."
            ).to_string(),
            scrollable: false,
            wrap: true,
        }
    }
}

struct ClockWidget;

impl slate_plugin_sdk::Widget for ClockWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Clock".to_string(),
            description: "Current time".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Simple time formatting (hours:minutes:seconds)
        let hours = (now % 86400) / 3600;
        let minutes = (now % 3600) / 60;
        let seconds = now % 60;
        WidgetContent::Text {
            content: format!("{:02}:{:02}:{:02} UTC", hours, minutes, seconds),
            scrollable: false,
            wrap: false,
        }
    }
}

struct ResourceUsageWidget {
    _config: WidgetConfig,
}

impl ResourceUsageWidget {
    fn new(config: WidgetConfig) -> Self {
        Self { _config: config }
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
        // Placeholder - real implementation would use sysinfo crate
        WidgetContent::KeyValue {
            pairs: vec![
                ("CPU".to_string(), slate_plugin_sdk::Cell::plain("---%")),
                ("Memory".to_string(), slate_plugin_sdk::Cell::plain("---/--- MB")),
                ("Disk".to_string(), slate_plugin_sdk::Cell::plain("---/--- GB")),
            ],
        }
    }
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
