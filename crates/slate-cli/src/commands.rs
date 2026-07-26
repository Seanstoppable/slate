use anyhow::Result;
use std::path::Path;

use slate_core::{App, SlateConfig};
use slate_plugin_host::{LuaPlugin, WasmPlugin};
use slate_plugin_manager::{Lockfile, PluginInstaller, Registry};
use slate_plugin_sdk::{Permissions, WidgetConfig, WidgetMetadata, WidgetContent};

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
        } else if entry.widget_type.starts_with("wasm:") {
            // Local WASM file path
            let path = entry.widget_type.trim_start_matches("wasm:");
            let path = shellexpand::tilde(path);
            let wasm_path = std::path::PathBuf::from(path.as_ref());

            if wasm_path.exists() {
                let mut widget = WasmPlugin::from_file(&wasm_path, Permissions::default())?;
                slate_plugin_sdk::Widget::init(&mut widget, widget_config);
                app.add_widget(Box::new(widget), entry.position.row, entry.position.col, entry.refresh_interval);
            } else {
                eprintln!(
                    "Warning: WASM plugin not found at '{}'. Build it first.",
                    wasm_path.display()
                );
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
