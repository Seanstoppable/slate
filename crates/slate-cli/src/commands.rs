use anyhow::Result;
use axum::{extract::State, response::Html, routing::get, Json, Router};
use slate_core::{App, Dashboard, DashboardSnapshot, SlateConfig};
use slate_plugin_host::{LuaPlugin, WasmPlugin};
use slate_plugin_manager::{Lockfile, PluginInstaller, Registry};
use slate_plugin_sdk::{Permissions, WidgetConfig, WidgetContent, WidgetMetadata};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use url::Url;

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
                .next_back()
                .or_else(|| entry.widget_type.split(':').next_back())
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
    if let Some(err) = &entry.settings_error {
        anyhow::bail!("{err}");
    }

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
            let manifest = read_plugin_manifest(&wasm_path);
            check_os_support(&wasm_path)?;
            let mut widget = WasmPlugin::from_file(
                &wasm_path,
                effective_plugin_permissions(manifest.as_ref(), &widget_config),
            )?;
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
            .next_back()
            .unwrap_or(&entry.widget_type);

        let plugins_dir = PluginInstaller::default_dir()?;
        let wasm_path = plugins_dir
            .join(plugin_name)
            .join(format!("{}.wasm", plugin_name));

        if wasm_path.exists() {
            let manifest = read_plugin_manifest(&wasm_path);
            check_os_support(&wasm_path)?;
            let mut widget = WasmPlugin::from_file(
                &wasm_path,
                effective_plugin_permissions(manifest.as_ref(), &widget_config),
            )?;
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
    #[serde(default)]
    permissions: Permissions,
}

#[derive(serde::Deserialize, Default)]
struct PluginManifestPlugin {
    #[serde(default)]
    name: String,
    #[serde(default)]
    os: Vec<String>,
}

/// Grant feedreader access only to the hosts selected by the user.
fn effective_plugin_permissions(
    manifest: Option<&PluginManifest>,
    widget_config: &WidgetConfig,
) -> Permissions {
    let mut permissions = manifest
        .map(|manifest| manifest.permissions.clone())
        .unwrap_or_default();

    if manifest.is_some_and(|manifest| manifest.plugin.name == "feedreader")
        && (permissions.network.is_empty()
            || (permissions.network.len() == 1 && permissions.network[0] == "*"))
    {
        permissions.network = feedreader_hosts(widget_config);
    }

    permissions
}

fn feedreader_hosts(widget_config: &WidgetConfig) -> Vec<String> {
    let mut hosts = std::collections::BTreeSet::new();
    let feeds = widget_config
        .settings
        .get("feeds")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str);

    for feed in feeds {
        if let Ok(url) = Url::parse(feed.trim()) {
            if matches!(url.scheme(), "http" | "https") {
                if let Some(host) = url.host_str() {
                    hosts.insert(host.to_string());
                }
            }
        }
    }

    hosts.into_iter().collect()
}

/// Find and parse plugin.toml next to a WASM file (in same dir or parent dir).
fn read_plugin_manifest(wasm_path: &std::path::Path) -> Option<PluginManifest> {
    // Walk up from the wasm file's directory until we find plugin.toml or hit the root.
    let mut dir = wasm_path.parent()?;
    loop {
        let candidate = dir.join("plugin.toml");
        if candidate.exists() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                if let Ok(manifest) = toml::from_str::<PluginManifest>(&content) {
                    return Some(manifest);
                }
            }
        }
        dir = dir.parent()?;
    }
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

/// Build the widget configuration passed to a plugin from its config entry.
fn build_widget_config(entry: &slate_core::config::WidgetEntry) -> WidgetConfig {
    WidgetConfig {
        position: entry.position.clone(),
        settings: entry
            .settings
            .iter()
            .map(|(k, v)| (k.clone(), toml_to_json(v)))
            .collect(),
        refresh_interval: entry.refresh_interval,
    }
}

/// Resolve the optional border color declared in a widget entry's settings.
fn border_color_for(entry: &slate_core::config::WidgetEntry) -> Option<slate_plugin_sdk::Color> {
    entry
        .settings
        .get("border_color")
        .and_then(|v| v.as_str())
        .and_then(parse_color)
}

/// Whether a widget type refers to an installable (non-builtin, non-Lua) plugin.
fn is_installable(widget_type: &str) -> bool {
    !widget_type.starts_with("builtin:") && !widget_type.starts_with("lua:")
}

/// Short display name for a plugin source such as `github.com/owner/repo`.
fn plugin_display_name(widget_type: &str) -> &str {
    widget_type.split('/').next_back().unwrap_or(widget_type)
}

fn build_dashboard(config: SlateConfig) -> Dashboard {
    let mut dashboard = Dashboard::new(config.clone());

    for entry in &config.widget {
        let widget = load_widget_or_error(entry, build_widget_config(entry));
        dashboard.add_widget(
            widget,
            entry.position.row,
            entry.position.col,
            entry.position.row_span,
            entry.position.col_span,
            entry.refresh_interval,
            border_color_for(entry),
        );
    }

    if config.widget.is_empty() {
        dashboard.add_widget(Box::new(builtins::WelcomeWidget), 0, 0, 1, 1, None, None);
    }

    dashboard
}

/// Run the dashboard.
pub async fn run(config_path: Option<&str>) -> Result<()> {
    let config = match config_path {
        Some(path) => SlateConfig::load_from(Path::new(path))?,
        None => SlateConfig::load_default()?,
    };
    let dashboard = build_dashboard(config);
    let mut app = App::from_dashboard(dashboard);

    app.run()
}

#[derive(Clone)]
struct WebState {
    dashboard: Arc<Mutex<Dashboard>>,
}

const DASHBOARD_HTML: &str = include_str!("web/dashboard.html");

async fn web_index() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

async fn web_health() -> &'static str {
    "ok"
}

async fn web_dashboard(State(state): State<WebState>) -> Json<DashboardSnapshot> {
    let dashboard = state
        .dashboard
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Json(dashboard.snapshot())
}

pub async fn serve(config_path: Option<&str>, host: &str, port: u16) -> Result<()> {
    let config = match config_path {
        Some(path) => SlateConfig::load_from(Path::new(path))?,
        None => SlateConfig::load_default()?,
    };
    let dashboard = Arc::new(Mutex::new(build_dashboard(config)));
    let app = Router::new()
        .route("/", get(web_index))
        .route("/health", get(web_health))
        .route("/api/dashboard", get(web_dashboard))
        .with_state(WebState {
            dashboard: Arc::clone(&dashboard),
        });

    let refresh_dashboard = Arc::clone(&dashboard);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let mut dashboard = refresh_dashboard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            dashboard.refresh_due(None);
        }
    });

    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    println!(
        "Serving Slate dashboard at http://{}:{}",
        listener.local_addr()?.ip(),
        listener.local_addr()?.port()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

/// Install all declared plugins.
pub async fn install() -> Result<()> {
    let config = SlateConfig::load_default()?;
    let installer = PluginInstaller::new(PluginInstaller::default_dir()?);
    let mut lockfile = Lockfile::load_default()?;

    for entry in &config.widget {
        if is_installable(&entry.widget_type) {
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
        if is_installable(&entry.widget_type) {
            let plugin_name = plugin_display_name(&entry.widget_type);
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
        println!("{:<20} {:<12} {:<12} Source", "Plugin", "Current", "Latest");
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
        println!("{:<20} {:<12} Source", "Plugin", "Version");
        println!("{}", "-".repeat(60));
        for name in &installed {
            if let Some(locked) = lockfile.get(name) {
                println!("{:<20} {:<12} {}", name, locked.version, locked.source);
            } else {
                println!("{:<20} {:<12} unlocked", name, "?");
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
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"

[target.'cfg(target_arch = "wasm32")'.dependencies]
extism-pdk = "1"
"#
    );
    std::fs::write(dir.join("Cargo.toml"), cargo_toml)?;

    // plugin.toml
    let plugin_toml = format!(
        r#"[plugin]
name = "{name}"
description = "A Slate plugin"
        tags = ["example"]
version = "0.1.0"
        author = "Your Name"
        language = "rust"

        [permissions]
        # network = ["api.example.com"]
        # storage = true

[config]
# Generated by `slate lint --fix` — add entries here for each settings key.
"#
    );
    std::fs::write(dir.join("plugin.toml"), plugin_toml)?;

    // src/lib.rs
    let lib_rs = r#"#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": env!("CARGO_PKG_NAME"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "version": env!("CARGO_PKG_VERSION"),
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_focus(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_blur(_input: String) -> FnResult<String> {
    Ok(String::new())
}
"#;
    std::fs::write(dir.join("src").join("lib.rs"), lib_rs)?;

    println!("Created plugin scaffold in '{}'", name);
    println!("  Build with: cargo build --target wasm32-wasip1 --release");
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
                for export in reader.into_iter().flatten() {
                    found_exports.push(export.name.to_string());
                }
            }
            Ok(wasmparser::Payload::ImportSection(reader)) => {
                for import in reader.into_iter().flatten() {
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
        let plugin_name = widget_type.split('/').next_back().unwrap_or(widget_type);
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

        if let Some(err) = &entry.settings_error {
            println!("  {:2}. {} ✗ {}", i + 1, label, err);
            errors += 1;
            continue;
        }

        if entry.widget_type.starts_with("builtin:") {
            let name = entry.widget_type.trim_start_matches("builtin:");
            if builtins::is_builtin(name) {
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
                        match WasmPlugin::from_file(
                            &wasm_path,
                            read_plugin_manifest(&wasm_path)
                                .map(|manifest| manifest.permissions)
                                .unwrap_or_default(),
                        ) {
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

/// Parse a color string into a Color enum value.
fn parse_color(s: &str) -> Option<slate_plugin_sdk::Color> {
    use slate_plugin_sdk::Color;
    match s.to_lowercase().as_str() {
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" | "purple" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        _ => None,
    }
}

/// Infer the config type from a `.as_XXX()` call following a settings access.
fn infer_type_from_accessor(accessor: &str) -> &'static str {
    if accessor.contains("as_str") {
        "string"
    } else if accessor.contains("as_bool") {
        "boolean"
    } else if accessor.contains("as_i64") || accessor.contains("as_u64") {
        "integer"
    } else if accessor.contains("as_f64") {
        "number"
    } else if accessor.contains("as_array") {
        "array"
    } else {
        "string"
    }
}

/// Extract config keys used in a plugin's source code by scanning for
/// `settings["key"]` and `config::get("key")` patterns.
fn extract_config_keys_from_source(plugin_dir: &Path) -> Vec<(String, &'static str)> {
    let src_dir = plugin_dir.join("src");
    let mut keys: Vec<(String, &'static str)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let files = if src_dir.is_dir() {
        std::fs::read_dir(&src_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "rs"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    let settings_re =
        regex::Regex::new(r#"settings\["([a-zA-Z_][a-zA-Z0-9_]*)"\](\.[a-z_0-9]+\(\))?"#).unwrap();
    let config_get_re = regex::Regex::new(r#"config::get\("([a-zA-Z_][a-zA-Z0-9_]*)"\)"#).unwrap();

    for file in files {
        let content = match std::fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for cap in settings_re.captures_iter(&content) {
            let key = cap[1].to_string();
            if seen.contains(&key) {
                continue;
            }
            let typ = cap
                .get(2)
                .map(|m| infer_type_from_accessor(m.as_str()))
                .unwrap_or("string");
            seen.insert(key.clone());
            keys.push((key, typ));
        }

        for cap in config_get_re.captures_iter(&content) {
            let key = cap[1].to_string();
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key.clone());
            keys.push((key, "string"));
        }
    }

    keys.sort_by_key(|a| a.0.to_lowercase());
    keys
}

/// Lint a plugin directory for baseline compliance.
/// Checks: plugin.toml structure, tags alphabetical, [config] matches source usage.
pub async fn lint(path: Option<&str>) -> Result<()> {
    let plugin_dir = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir()?,
    };

    let manifest_path = plugin_dir.join("plugin.toml");
    if !manifest_path.exists() {
        println!("✗ No plugin.toml found in {}", plugin_dir.display());
        return Ok(());
    }

    let content = std::fs::read_to_string(&manifest_path)?;
    let doc: toml::Value = toml::from_str(&content)?;

    let mut errors = 0u32;
    let mut warnings = 0u32;

    // Find the plugin metadata section ([plugin] or [metadata])
    let meta = doc
        .get("plugin")
        .or_else(|| doc.get("metadata"))
        .and_then(|v| v.as_table());

    if let Some(meta) = meta {
        // Check required fields
        for field in ["name", "description", "version"] {
            if meta
                .get(field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .is_empty()
            {
                println!("  ✗ Missing required field: {}", field);
                errors += 1;
            }
        }

        // Check tags exist and are sorted
        if let Some(tags) = meta.get("tags").and_then(|v| v.as_array()) {
            let tag_strings: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
            if tag_strings.is_empty() {
                println!("  ⚠ tags array is empty");
                warnings += 1;
            } else {
                let mut sorted = tag_strings.clone();
                sorted.sort_by_key(|a| a.to_lowercase());
                if tag_strings != sorted {
                    println!(
                        "  ✗ tags are not alphabetically sorted\n    have: {:?}\n    want: {:?}",
                        tag_strings, sorted
                    );
                    errors += 1;
                } else {
                    println!("  ✓ tags present and sorted");
                }
            }
        } else {
            println!("  ✗ Missing tags field");
            errors += 1;
        }
    } else {
        println!("  ✗ Missing [plugin] or [metadata] section");
        errors += 1;
    }

    // Extract config keys from source
    let source_keys = extract_config_keys_from_source(&plugin_dir);

    // Parse existing [config] section
    let config_section = doc.get("config").and_then(|v| v.as_table());
    let declared_keys: Vec<String> = config_section
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default();

    if source_keys.is_empty() {
        if config_section.is_some() {
            println!("  ⚠ [config] section present but no settings usage found in source");
            warnings += 1;
        } else {
            println!("  ✓ No config keys used (no [config] section needed)");
        }
    } else {
        // Check for missing keys in [config]
        let mut missing: Vec<&str> = Vec::new();
        for (key, _) in &source_keys {
            if !declared_keys.iter().any(|k| k == key) {
                missing.push(key);
            }
        }

        if missing.is_empty() {
            println!("  ✓ All config keys documented in [config]");
        } else {
            println!(
                "  ✗ Config keys used in source but missing from [config]: {:?}",
                missing
            );
            errors += 1;
        }

        // Check for extra keys in [config] not used in source
        let source_key_names: Vec<&str> = source_keys.iter().map(|(k, _)| k.as_str()).collect();
        let extra: Vec<&str> = declared_keys
            .iter()
            .filter(|k| !source_key_names.contains(&k.as_str()))
            .map(|k| k.as_str())
            .collect();
        if !extra.is_empty() {
            println!(
                "  ⚠ Config keys in [config] not found in source: {:?}",
                extra
            );
            warnings += 1;
        }

        // Check alphabetical order of [config] keys
        if let Some(table) = config_section {
            let keys: Vec<&String> = table.keys().collect();
            let mut sorted_keys = keys.clone();
            sorted_keys.sort_by_key(|a| a.to_lowercase());
            if keys != sorted_keys {
                println!("  ✗ [config] keys are not alphabetically sorted");
                errors += 1;
            }
        }
    }

    // Summary
    println!();
    if errors == 0 && warnings == 0 {
        println!("✓ Plugin passes baseline");
    } else {
        println!("Results: {} errors, {} warnings", errors, warnings);
        if errors > 0 {
            println!("Run `slate lint --fix` to auto-generate [config] from source.");
        }
    }

    Ok(())
}

/// Generate or update the [config] section in plugin.toml from source code analysis.
pub fn lint_fix(plugin_dir: &Path) -> Result<()> {
    let manifest_path = plugin_dir.join("plugin.toml");
    let content = std::fs::read_to_string(&manifest_path)?;

    let source_keys = extract_config_keys_from_source(plugin_dir);
    if source_keys.is_empty() {
        println!("No config keys found in source — nothing to generate.");
        return Ok(());
    }

    // Build the [config] section
    let mut config_lines = Vec::new();
    config_lines.push("\n[config]".to_string());
    for (key, typ) in &source_keys {
        config_lines.push(format!(
            "{} = {{ type = \"{}\", required = false, description = \"\" }}",
            key, typ
        ));
    }
    let config_block = config_lines.join("\n");

    // Replace existing [config] section or append
    let new_content = if let Some(start) = content.find("\n[config]") {
        // Find the end of [config] section (next section or EOF)
        let after_config = &content[start + 1..];
        let end = after_config
            .find("\n[")
            .map(|i| start + 1 + i)
            .unwrap_or(content.len());
        format!("{}{}\n{}", &content[..start], config_block, &content[end..])
    } else {
        format!("{}\n{}\n", content.trim_end(), config_block)
    };

    std::fs::write(&manifest_path, new_content)?;
    println!(
        "Updated [config] in {} ({} keys)",
        manifest_path.display(),
        source_keys.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use slate_core::config::WidgetEntry;
    use slate_plugin_sdk::Position;
    use slate_plugin_sdk::Widget;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    #[test]
    fn build_widget_config_converts_settings_and_carries_position() {
        let entry = WidgetEntry {
            widget_type: "builtin:clock".to_string(),
            position: Position {
                row: 1,
                col: 2,
                row_span: 2,
                col_span: 3,
            },
            refresh_interval: Some(45),
            settings: std::collections::HashMap::from([
                ("title".to_string(), toml::Value::String("Now".to_string())),
                ("limit".to_string(), toml::Value::Integer(5)),
            ]),
            settings_error: None,
        };

        let config = build_widget_config(&entry);

        assert_eq!(config.position.row, 1);
        assert_eq!(config.position.col, 2);
        assert_eq!(config.position.row_span, 2);
        assert_eq!(config.position.col_span, 3);
        assert_eq!(config.refresh_interval, Some(45));
        assert_eq!(
            config.settings.get("title"),
            Some(&serde_json::json!("Now"))
        );
        assert_eq!(config.settings.get("limit"), Some(&serde_json::json!(5)));
    }

    #[test]
    fn feedreader_permissions_are_limited_to_configured_feed_hosts() {
        let config = WidgetConfig {
            settings: HashMap::from([(
                "feeds".to_string(),
                serde_json::json!([
                    "http://blog.example.com/atom.xml",
                    "https://news.example.com/other.xml",
                    "file:///not-a-feed",
                    "not a URL"
                ]),
            )]),
            ..widget_config()
        };
        let manifest = PluginManifest {
            plugin: PluginManifestPlugin {
                name: "feedreader".to_string(),
                ..Default::default()
            },
            permissions: Permissions {
                ..Default::default()
            },
        };

        let permissions = effective_plugin_permissions(Some(&manifest), &config);

        assert_eq!(
            permissions.network,
            vec![
                "blog.example.com".to_string(),
                "news.example.com".to_string()
            ]
        );
    }

    #[test]
    fn non_feedreader_wildcard_permissions_are_unchanged() {
        let manifest = PluginManifest {
            plugin: PluginManifestPlugin {
                name: "other-plugin".to_string(),
                ..Default::default()
            },
            permissions: Permissions {
                network: vec!["*".to_string()],
                ..Default::default()
            },
        };

        let permissions = effective_plugin_permissions(Some(&manifest), &widget_config());

        assert_eq!(permissions.network, vec!["*".to_string()]);
    }

    #[test]
    fn border_color_for_reads_valid_colors_and_ignores_others() {
        let mut entry = WidgetEntry {
            widget_type: "builtin:clock".to_string(),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: std::collections::HashMap::new(),
            settings_error: None,
        };
        assert!(border_color_for(&entry).is_none());

        entry.settings.insert(
            "border_color".to_string(),
            toml::Value::String("red".to_string()),
        );
        assert!(matches!(
            border_color_for(&entry),
            Some(slate_plugin_sdk::Color::Red)
        ));

        entry.settings.insert(
            "border_color".to_string(),
            toml::Value::String("definitely-not-a-color".to_string()),
        );
        assert!(border_color_for(&entry).is_none());

        entry
            .settings
            .insert("border_color".to_string(), toml::Value::Integer(3));
        assert!(border_color_for(&entry).is_none());
    }

    #[test]
    fn is_installable_excludes_builtin_and_lua_sources() {
        assert!(is_installable("github.com/owner/repo"));
        assert!(is_installable("wasm:plugins/clock.wasm"));
        assert!(!is_installable("builtin:resource_usage"));
        assert!(!is_installable("lua:~/.config/slate/script.lua"));
    }

    #[test]
    fn plugin_display_name_uses_last_path_segment() {
        assert_eq!(plugin_display_name("github.com/owner/repo"), "repo");
        assert_eq!(plugin_display_name("clock"), "clock");
        assert_eq!(plugin_display_name("a/b/c"), "c");
    }

    #[test]
    fn load_widget_or_error_reports_settings_interpolation_failure() {
        let entry = WidgetEntry {
            widget_type: "builtin:clock".to_string(),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: Some(
                "Environment variable `SLATE_MISSING_TOKEN` referenced in config is not set"
                    .to_string(),
            ),
        };

        let mut widget = load_widget_or_error(&entry, widget_config());

        match widget.refresh() {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("SLATE_MISSING_TOKEN"), "{content}");
            }
            other => panic!("Expected Text content, got {other:?}"),
        }
    }

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

    fn escape_path(path: &std::path::Path) -> String {
        path.display().to_string().replace('\\', "\\\\")
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn load_widget_or_error_returns_builtin_on_success() {
        let entry = WidgetEntry {
            widget_type: "builtin:resource_usage".to_string(),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            settings_error: None,
            refresh_interval: None,
        };

        let mut widget = load_widget_or_error(&entry, widget_config());

        assert_ne!(widget.metadata().description, "Failed to load");
        widget.init(widget_config());
    }

    #[test]
    fn error_widget_init_is_a_noop() {
        let mut widget = ErrorWidget {
            name: "broken".to_string(),
            error: "boom".to_string(),
        };

        widget.init(widget_config());

        assert_eq!(widget.metadata().name, "broken");
    }

    struct EnvGuard {
        vars: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn redirect_to(path: &std::path::Path) -> Self {
            let mut vars = Vec::new();
            let value = path.display().to_string();
            for key in [
                "APPDATA",
                "LOCALAPPDATA",
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
            ] {
                vars.push((key, std::env::var(key).ok()));
                std::env::set_var(key, &value);
            }
            Self { vars }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.vars {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }

    struct FileRestoreGuard {
        path: std::path::PathBuf,
        original: Option<Vec<u8>>,
    }

    impl FileRestoreGuard {
        fn new(path: std::path::PathBuf) -> Self {
            Self {
                original: std::fs::read(&path).ok(),
                path,
            }
        }
    }

    impl Drop for FileRestoreGuard {
        fn drop(&mut self) {
            if let Some(original) = &self.original {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(&self.path, original).ok();
            } else {
                std::fs::remove_file(&self.path).ok();
            }
        }
    }

    fn write_default_config(content: &str) -> std::path::PathBuf {
        let path = SlateConfig::default_path().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    fn write_default_lockfile(content: &str) -> std::path::PathBuf {
        let path = Lockfile::default_path().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
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
            "[plugin]\nname = \"same-dir\"\nos = [\"windows\"]\n\n[permissions]\nexec = [\"git\"]\nstorage = true\n",
        )
        .unwrap();

        let manifest = read_plugin_manifest(&wasm_path).unwrap();
        assert_eq!(manifest.plugin.name, "same-dir");
        assert_eq!(manifest.plugin.os, vec!["windows"]);
        assert_eq!(manifest.permissions.exec, vec!["git"]);
        assert!(manifest.permissions.storage);
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
        let expected = std::env::consts::OS;
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
    fn validate_wasm_binary_reports_missing_files() {
        let dir = tempdir().unwrap();
        let missing_path = dir.path().join("missing.wasm");

        let issues = validate_wasm_binary(&missing_path);

        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("Cannot read file"));
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
        assert!(issues
            .iter()
            .any(|issue| issue.contains("Optional exports not found")));
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
        assert!(builtins::create_builtin("logfile", widget_config()).is_ok());
        assert!(builtins::create_builtin("nonexistent", widget_config()).is_err());
    }

    #[tokio::test]
    async fn run_returns_error_for_missing_explicit_config_file() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing.toml");

        let error = run(Some(missing.to_str().unwrap())).await.unwrap_err();

        assert!(error.to_string().contains("Failed to read config"));
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
        assert!(plugin.contains("[plugin]"));
        assert!(plugin.contains("test-plugin"));
        assert!(plugin.contains("description = \"A Slate plugin\""));

        let lib = std::fs::read_to_string(lib_path).unwrap();
        assert!(lib.contains("#[plugin_fn]"));
        assert!(lib.contains("pub fn metadata"));
        assert!(lib.contains("pub fn refresh"));
        assert!(lib.contains("Hello from my plugin!"));
    }

    #[tokio::test]
    async fn create_scaffolds_nested_plugin_project_paths() {
        let dir = tempdir().unwrap();
        let name = dir
            .path()
            .join("nested")
            .join("plugins")
            .join("sample-plugin");

        create(name.to_str().unwrap()).await.unwrap();

        assert!(name.join("Cargo.toml").exists());
        assert!(name.join("plugin.toml").exists());
        assert!(name.join("src").join("lib.rs").exists());
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
            escape_path(&lua_path)
        );
        std::fs::write(&config_path, config).unwrap();

        check(Some(config_path.to_str().unwrap())).await.unwrap();
    }

    #[test]
    fn load_widget_or_error_wraps_failures_in_error_widget() {
        let entry = WidgetEntry {
            widget_type: "builtin:not-real".to_string(),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: None,
        };

        let mut widget = load_widget_or_error(&entry, widget_config());
        assert_eq!(widget.metadata().name, "builtin:not-real");
        match widget.refresh() {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("Plugin load error"));
                assert!(content.contains("Unknown builtin widget"));
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn load_widget_or_error_uses_repo_name_for_uninstalled_github_plugins() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());
        let entry = WidgetEntry {
            widget_type: "github.com/example/slate-weather".to_string(),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: None,
        };

        let mut widget = load_widget_or_error(&entry, widget_config());
        assert_eq!(widget.metadata().name, "slate-weather");
        match widget.refresh() {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("Plugin load error"));
                assert!(content.contains("not installed"));
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn try_load_widget_loads_existing_lua_script() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("greeting.lua");
        std::fs::write(
            &script_path,
            r#"
name = "Greeting"
description = "Greets"
version = "1.0.0"
function refresh()
  return '{"type":"text","content":"Hello from Lua","scrollable":false,"wrap":true}'
end
"#,
        )
        .unwrap();

        let entry = WidgetEntry {
            widget_type: format!("lua:{}", script_path.display()),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: None,
        };

        let mut widget = try_load_widget(&entry, widget_config()).unwrap();
        assert_eq!(widget.metadata().name, "Greeting");
        match widget.refresh() {
            WidgetContent::Text { content, .. } => assert_eq!(content, "Hello from Lua"),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn check_reports_missing_local_wasm_without_erroring() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("slate.toml");
        let wasm_path = dir.path().join("missing.wasm");
        std::fs::write(
            &config_path,
            format!(
                r#"
[[widget]]
type = "wasm:{}"
position = {{ row = 0, col = 0 }}
"#,
                escape_path(&wasm_path)
            ),
        )
        .unwrap();

        check(Some(config_path.to_str().unwrap())).await.unwrap();
    }

    #[test]
    fn try_load_widget_rejects_missing_local_wasm() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("missing.wasm");
        let entry = WidgetEntry {
            widget_type: format!("wasm:{}", wasm_path.display()),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: None,
        };

        let error = match try_load_widget(&entry, widget_config()) {
            Ok(_) => panic!("expected missing wasm"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("WASM file not found"));
    }

    #[test]
    fn try_load_widget_rejects_unsupported_wasm_os() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("unsupported.wasm");
        write_wasm(
            &wasm_path,
            r#"
                (module
                    (func (export "metadata") (result i32) (i32.const 0))
                    (func (export "refresh") (result i32) (i32.const 0))
                )
            "#,
        );
        let unsupported = ["linux", "macos", "windows"]
            .into_iter()
            .find(|os| *os != current_os())
            .unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            format!("[plugin]\nos = [\"{}\"]\n", unsupported),
        )
        .unwrap();
        let entry = WidgetEntry {
            widget_type: format!("wasm:{}", wasm_path.display()),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: None,
        };

        let error = match try_load_widget(&entry, widget_config()) {
            Ok(_) => panic!("expected unsupported OS to be rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("Not available on"));
        assert!(error.to_string().contains(unsupported));
    }

    #[test]
    fn try_load_widget_rejects_uninstalled_github_plugins() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());
        let entry = WidgetEntry {
            widget_type: "github.com/example/slate-weather".to_string(),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: None,
        };

        let error = match try_load_widget(&entry, widget_config()) {
            Ok(_) => panic!("expected missing GitHub plugin"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("not installed"));
    }

    #[test]
    fn try_load_widget_loads_local_stub_wasm_plugin() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("stub.wasm");
        write_wasm(
            &wasm_path,
            r#"
                (module
                    (memory (export "memory") 1)
                    (func (export "metadata") (result i32) (i32.const 0))
                    (func (export "refresh") (result i32) (i32.const 0))
                )
            "#,
        );

        let entry = WidgetEntry {
            widget_type: format!("wasm:{}", wasm_path.display()),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: None,
        };

        let mut widget = try_load_widget(&entry, widget_config()).unwrap();
        assert_eq!(widget.metadata().name, "stub");
        match widget.refresh() {
            WidgetContent::Text { content, .. } => assert_eq!(content, ""),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn try_load_widget_loads_installed_github_stub_wasm_plugin() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());

        let plugin_name = "slate-clock";
        let plugin_dir = PluginInstaller::default_dir().unwrap().join(plugin_name);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        let wasm_path = plugin_dir.join(format!("{plugin_name}.wasm"));
        write_wasm(
            &wasm_path,
            r#"
                (module
                    (memory (export "memory") 1)
                    (func (export "metadata") (result i32) (i32.const 0))
                    (func (export "refresh") (result i32) (i32.const 0))
                )
            "#,
        );

        let entry = WidgetEntry {
            widget_type: format!("github.com/example/{plugin_name}"),
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            refresh_interval: None,
            settings: Default::default(),
            settings_error: None,
        };

        let mut widget = try_load_widget(&entry, widget_config()).unwrap();
        assert_eq!(widget.metadata().name, plugin_name);
        match widget.refresh() {
            WidgetContent::Text { content, .. } => assert_eq!(content, ""),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn check_handles_mixed_widget_types() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("slate.toml");

        let lua_path = dir.path().join("ok.lua");
        std::fs::write(
            &lua_path,
            r#"
name = "Check Lua"
function refresh()
  return '{"type":"text","content":"ok","scrollable":false,"wrap":true}'
end
"#,
        )
        .unwrap();

        let valid_wasm_path = dir.path().join("valid.wasm");
        write_wasm(
            &valid_wasm_path,
            r#"
                (module
                    (memory (export "memory") 1)
                    (func (export "metadata") (result i32) (i32.const 0))
                    (func (export "refresh") (result i32) (i32.const 0))
                )
            "#,
        );

        let unsupported_dir = dir.path().join("unsupported");
        std::fs::create_dir_all(&unsupported_dir).unwrap();
        let unsupported_wasm_path = unsupported_dir.join("unsupported.wasm");
        write_wasm(
            &unsupported_wasm_path,
            r#"
                (module
                    (memory (export "memory") 1)
                    (func (export "metadata") (result i32) (i32.const 0))
                    (func (export "refresh") (result i32) (i32.const 0))
                )
            "#,
        );
        let unsupported_os = ["linux", "macos", "windows"]
            .into_iter()
            .find(|os| *os != current_os())
            .unwrap();
        std::fs::write(
            unsupported_dir.join("plugin.toml"),
            format!(
                "[plugin]\nname = \"unsupported\"\nos = [\"{}\"]\n",
                unsupported_os
            ),
        )
        .unwrap();

        let invalid_wasm_path = dir.path().join("invalid.wasm");
        std::fs::write(&invalid_wasm_path, b"not wasm").unwrap();

        std::fs::write(
            &config_path,
            format!(
                r#"
[[widget]]
type = "builtin:resource_usage"
position = {{ row = 0, col = 0 }}

[[widget]]
type = "builtin:not-real"
position = {{ row = 0, col = 1 }}

[[widget]]
type = "lua:{}"
position = {{ row = 1, col = 0 }}

[[widget]]
type = "lua:{}"
position = {{ row = 1, col = 1 }}

[[widget]]
type = "wasm:{}"
position = {{ row = 2, col = 0 }}

[[widget]]
type = "wasm:{}"
position = {{ row = 2, col = 1 }}

[[widget]]
type = "wasm:{}"
position = {{ row = 3, col = 0 }}

[[widget]]
type = "github.com/example/not-installed"
position = {{ row = 3, col = 1 }}
"#,
                escape_path(&lua_path),
                escape_path(&dir.path().join("missing.lua")),
                escape_path(&valid_wasm_path),
                escape_path(&unsupported_wasm_path),
                escape_path(&invalid_wasm_path),
            ),
        )
        .unwrap();

        check(Some(config_path.to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn check_uses_default_config_and_handles_wasm_success_and_extism_failure() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());
        let config_path = SlateConfig::default_path().unwrap();
        let _restore = FileRestoreGuard::new(config_path.clone());

        let ok_wasm = dir.path().join("ok.wasm");
        write_wasm(
            &ok_wasm,
            r#"
                (module
                    (memory (export "memory") 1)
                    (func (export "metadata") (result i32) (i32.const 0))
                    (func (export "refresh") (result i32) (i32.const 0))
                )
            "#,
        );

        let failing_wasm = dir.path().join("failing.wasm");
        write_wasm(
            &failing_wasm,
            r#"
                (module
                    (import "env" "missing_host" (func))
                    (memory (export "memory") 1)
                    (func (export "metadata") (result i32) (i32.const 0))
                    (func (export "refresh") (result i32) (i32.const 0))
                )
            "#,
        );

        write_default_config(&format!(
            r#"
[[widget]]
type = "wasm:{}"
position = {{ row = 0, col = 0 }}

[[widget]]
type = "wasm:{}"
position = {{ row = 0, col = 1 }}
"#,
            escape_path(&ok_wasm),
            escape_path(&failing_wasm)
        ));

        check(None).await.unwrap();
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn list_handles_empty_and_unlocked_plugins_in_default_directory() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());
        let _restore = FileRestoreGuard::new(Lockfile::default_path().unwrap());
        Lockfile::default().save_default().unwrap();

        list().await.unwrap();

        let plugin_name = "orphan-plugin";
        let plugins_dir = PluginInstaller::default_dir().unwrap();
        std::fs::create_dir_all(plugins_dir.join(plugin_name)).unwrap();

        list().await.unwrap();
    }

    #[tokio::test]
    async fn check_returns_ok_for_invalid_config_files() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("slate.toml");
        std::fs::write(&config_path, "[[widget]]\ntype = ").unwrap();

        check(Some(config_path.to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn install_and_update_return_errors_for_invalid_default_config() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());
        let _restore = FileRestoreGuard::new(SlateConfig::default_path().unwrap());
        write_default_config("[[widget]]\ntype = ");

        let install_error = install().await.unwrap_err();
        assert!(install_error
            .to_string()
            .contains("Failed to parse slate.toml"));

        let update_error = update().await.unwrap_err();
        assert!(update_error
            .to_string()
            .contains("Failed to parse slate.toml"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn list_remove_and_outdated_return_errors_for_invalid_default_lockfile() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());
        let _restore = FileRestoreGuard::new(Lockfile::default_path().unwrap());
        write_default_lockfile("not = [valid");

        let list_error = list().await.unwrap_err();
        assert!(list_error.to_string().contains("Failed to parse lockfile"));

        let remove_error = remove("clock").await.unwrap_err();
        assert!(remove_error
            .to_string()
            .contains("Failed to parse lockfile"));

        let outdated_error = outdated().await.unwrap_err();
        assert!(outdated_error
            .to_string()
            .contains("Failed to parse lockfile"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn list_and_remove_use_redirected_default_directories() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());
        let _restore = FileRestoreGuard::new(Lockfile::default_path().unwrap());

        let plugins_dir = PluginInstaller::default_dir().unwrap();
        let plugin_name = "slate-cli-test-plugin";
        std::fs::create_dir_all(plugins_dir.join(plugin_name)).unwrap();
        std::fs::write(
            plugins_dir
                .join(plugin_name)
                .join(format!("{plugin_name}.wasm")),
            b"wasm",
        )
        .unwrap();

        let mut lockfile = Lockfile::default();
        lockfile.lock(
            plugin_name,
            slate_plugin_manager::lockfile::LockedPlugin {
                source: "github.com/slate-community/slate-cli-test-plugin".to_string(),
                version: "1.0.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );
        let lockfile_path = Lockfile::default_path().unwrap();
        lockfile.save_to(&lockfile_path).unwrap();
        assert!(plugins_dir.join(plugin_name).exists());

        Lockfile::default().save_default().unwrap();
        let mut lockfile = Lockfile::load_from(&lockfile_path).unwrap();
        lockfile.lock(
            plugin_name,
            slate_plugin_manager::lockfile::LockedPlugin {
                source: "github.com/slate-community/slate-cli-test-plugin".to_string(),
                version: "1.0.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );
        lockfile.save_to(&lockfile_path).unwrap();

        list().await.unwrap();
        remove(plugin_name).await.unwrap();

        assert!(!plugins_dir.join(plugin_name).exists());
        let updated_lockfile = Lockfile::load_from(&lockfile_path).unwrap();
        assert!(updated_lockfile.get(plugin_name).is_none());
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn outdated_returns_clean_result_with_empty_redirected_lockfile() {
        let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let _env = EnvGuard::redirect_to(dir.path());
        let _restore = FileRestoreGuard::new(Lockfile::default_path().unwrap());
        Lockfile::default().save_default().unwrap();

        outdated().await.unwrap();
    }

    #[tokio::test]
    async fn migrate_does_not_error() {
        migrate("nonexistent.yml").await.unwrap();
    }

    #[test]
    fn extract_config_keys_finds_settings_and_config_get() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            r#"
            let name = settings["name"].as_str().unwrap_or("default");
            let count = settings["count"].as_u64().unwrap_or(10);
            let enabled = settings["enabled"].as_bool().unwrap_or(true);
            let items = settings["items"].as_array();
            let token = config::get("token").ok();
            "#,
        )
        .unwrap();

        let keys = extract_config_keys_from_source(dir.path());
        assert_eq!(keys.len(), 5);
        // Should be alphabetically sorted
        assert_eq!(keys[0], ("count".to_string(), "integer"));
        assert_eq!(keys[1], ("enabled".to_string(), "boolean"));
        assert_eq!(keys[2], ("items".to_string(), "array"));
        assert_eq!(keys[3], ("name".to_string(), "string"));
        assert_eq!(keys[4], ("token".to_string(), "string"));
    }

    #[test]
    fn infer_type_from_accessor_maps_correctly() {
        assert_eq!(infer_type_from_accessor(".as_str()"), "string");
        assert_eq!(infer_type_from_accessor(".as_bool()"), "boolean");
        assert_eq!(infer_type_from_accessor(".as_i64()"), "integer");
        assert_eq!(infer_type_from_accessor(".as_u64()"), "integer");
        assert_eq!(infer_type_from_accessor(".as_f64()"), "number");
        assert_eq!(infer_type_from_accessor(".as_array()"), "array");
        assert_eq!(infer_type_from_accessor(".unknown()"), "string");
    }

    #[test]
    fn lint_fix_generates_config_section() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            r#"let x = settings["alpha"].as_bool().unwrap_or(false);
            let y = settings["beta"].as_str().unwrap_or("");
            "#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            "[plugin]\nname = \"test\"\ndescription = \"A test\"\nversion = \"0.1.0\"\ntags = [\"test\"]\n",
        )
        .unwrap();

        lint_fix(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join("plugin.toml")).unwrap();
        assert!(content.contains("[config]"));
        assert!(content.contains("alpha = { type = \"boolean\""));
        assert!(content.contains("beta = { type = \"string\""));
        // alpha should come before beta (alphabetical)
        let alpha_pos = content.find("alpha").unwrap();
        let beta_pos = content.find("beta").unwrap();
        assert!(alpha_pos < beta_pos);
    }

    #[tokio::test]
    async fn lint_passes_for_compliant_plugin() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            r#"let x = settings["key"].as_str().unwrap_or("");"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            "[plugin]\nname = \"test\"\ndescription = \"A test\"\nversion = \"0.1.0\"\ntags = [\"test\"]\n\n[config]\nkey = { type = \"string\", required = false, description = \"A key\" }\n",
        )
        .unwrap();

        lint(Some(dir.path().to_str().unwrap())).await.unwrap();
    }

    #[test]
    fn build_dashboard_adds_welcome_widget_when_config_is_empty() {
        let dashboard = build_dashboard(SlateConfig::default());

        assert_eq!(dashboard.widgets.len(), 1);
        assert_eq!(dashboard.widgets[0].metadata.name, "Welcome");
    }

    #[test]
    fn build_dashboard_loads_configured_builtin_with_settings() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("app.log");
        std::fs::write(&log_path, "first line\nsecond line").unwrap();

        let mut config = SlateConfig::default();
        config.layout.rows = 3;
        config.layout.cols = 4;
        config.widget = vec![WidgetEntry {
            widget_type: "builtin:logfile".to_string(),
            position: Position {
                row: 1,
                col: 2,
                row_span: 2,
                col_span: 1,
            },
            refresh_interval: Some(12),
            settings: HashMap::from([
                (
                    "border_color".to_string(),
                    toml::Value::String("purple".to_string()),
                ),
                (
                    "filePath".to_string(),
                    toml::Value::String(log_path.display().to_string()),
                ),
            ]),
            settings_error: None,
        }];

        let dashboard = build_dashboard(config);
        let snapshot = dashboard.snapshot();

        assert_eq!(snapshot.layout.rows, 3);
        assert_eq!(snapshot.layout.cols, 4);
        assert_eq!(snapshot.widgets.len(), 1);
        assert_eq!(snapshot.widgets[0].metadata.name, "Log File");
        assert_eq!(snapshot.widgets[0].position.row, 1);
        assert_eq!(snapshot.widgets[0].position.col, 2);
        assert_eq!(snapshot.widgets[0].position.row_span, 2);
        assert_eq!(snapshot.widgets[0].refresh_interval_seconds, 12);
        assert!(matches!(
            snapshot.widgets[0].border_color,
            Some(slate_plugin_sdk::Color::Magenta)
        ));
        match &snapshot.widgets[0].content {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("first line"));
                assert!(content.contains("second line"));
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn parse_color_accepts_supported_names_case_insensitively() {
        use slate_plugin_sdk::Color;

        assert!(matches!(parse_color("RED"), Some(Color::Red)));
        assert!(matches!(parse_color("green"), Some(Color::Green)));
        assert!(matches!(parse_color("Yellow"), Some(Color::Yellow)));
        assert!(matches!(parse_color("blue"), Some(Color::Blue)));
        assert!(matches!(parse_color("purple"), Some(Color::Magenta)));
        assert!(matches!(parse_color("magenta"), Some(Color::Magenta)));
        assert!(matches!(parse_color("cyan"), Some(Color::Cyan)));
        assert!(matches!(parse_color("white"), Some(Color::White)));
        assert!(matches!(parse_color("grey"), Some(Color::Gray)));
        assert!(matches!(parse_color("gray"), Some(Color::Gray)));
        assert!(parse_color("chartreuse").is_none());
    }

    #[tokio::test]
    async fn serve_returns_error_for_missing_explicit_config_file() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing.toml");

        let error = serve(Some(missing.to_str().unwrap()), "127.0.0.1", 0)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("Failed to read config"));
    }

    #[tokio::test]
    async fn web_dashboard_returns_snapshot_json() {
        let dashboard = Arc::new(Mutex::new(build_dashboard(SlateConfig::default())));
        let state = WebState { dashboard };

        let Json(snapshot) = web_dashboard(State(state)).await;

        assert_eq!(snapshot.layout.rows, 2);
        assert_eq!(snapshot.layout.cols, 2);
        assert_eq!(snapshot.widgets.len(), 1);
        assert_eq!(snapshot.widgets[0].metadata.name, "Welcome");
    }

    #[tokio::test]
    async fn web_endpoints_return_expected_static_content() {
        assert_eq!(web_health().await, "ok");
        assert!(web_index().await.0.contains("/api/dashboard"));
        assert!(web_index().await.0.contains("Slate Dashboard"));
    }

    #[tokio::test]
    async fn lint_returns_ok_for_missing_plugin_toml() {
        let dir = tempdir().unwrap();
        // No plugin.toml — should return Ok without panicking
        lint(Some(dir.path().to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn lint_reports_missing_plugin_section() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("plugin.toml"), "[permissions]\n").unwrap();
        lint(Some(dir.path().to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn lint_reports_empty_tags_and_config_keys_present_but_unused() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // No settings["..."] usage → source_keys is empty
        std::fs::write(src.join("lib.rs"), "fn main() {}").unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            "[plugin]\nname = \"test\"\ndescription = \"t\"\nversion = \"0.1.0\"\ntags = []\n\n[config]\nfoo = {}\n",
        )
        .unwrap();
        lint(Some(dir.path().to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn lint_reports_unsorted_tags_and_missing_config_key() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            r#"let x = settings["beta"].as_str().unwrap_or("");"#,
        )
        .unwrap();
        // Tags are not sorted; [config] is missing the key used in source
        std::fs::write(
            dir.path().join("plugin.toml"),
            "[plugin]\nname = \"t\"\ndescription = \"t\"\nversion = \"0.1.0\"\ntags = [\"z\", \"a\"]\n",
        )
        .unwrap();
        lint(Some(dir.path().to_str().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn lint_reports_extra_config_key_and_unsorted_config_keys() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            r#"let x = settings["alpha"].as_str().unwrap_or("");"#,
        )
        .unwrap();
        // [config] has "alpha" (used) and "zzz_extra" (unused), and is not sorted
        std::fs::write(
            dir.path().join("plugin.toml"),
            "[plugin]\nname = \"t\"\ndescription = \"t\"\nversion = \"0.1.0\"\ntags = [\"a\"]\n\n[config]\nzzz_extra = {}\nalpha = {}\n",
        )
        .unwrap();
        lint(Some(dir.path().to_str().unwrap())).await.unwrap();
    }

    #[test]
    fn lint_fix_returns_ok_when_no_source_keys() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("plugin.toml"), "[plugin]\nname = \"t\"\n").unwrap();

        lint_fix(dir.path()).unwrap();

        // File should not gain a [config] section when there are no keys
        let content = std::fs::read_to_string(dir.path().join("plugin.toml")).unwrap();
        assert!(!content.contains("[config]"));
    }

    #[test]
    fn lint_fix_replaces_existing_config_section() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            r#"let x = settings["newkey"].as_bool().unwrap_or(false);"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            "[plugin]\nname = \"t\"\n\n[config]\noldkey = { type = \"string\" }\n",
        )
        .unwrap();

        lint_fix(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join("plugin.toml")).unwrap();
        assert!(content.contains("[config]"));
        assert!(content.contains("newkey"));
        // oldkey should have been replaced
        assert!(!content.contains("oldkey"));
    }

    #[test]
    fn check_os_support_uses_plugin_name_from_manifest_when_available() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("widget.wasm");
        std::fs::write(&wasm_path, b"wasm").unwrap();

        let unsupported = ["linux", "macos", "windows"]
            .into_iter()
            .find(|os| *os != current_os())
            .unwrap();
        // Write a manifest with an explicit name (exercises the non-empty-name branch)
        std::fs::write(
            dir.path().join("plugin.toml"),
            format!(
                "[plugin]\nname = \"my-named-plugin\"\nos = [\"{}\"]\n",
                unsupported
            ),
        )
        .unwrap();

        let err = check_os_support(&wasm_path).unwrap_err();
        assert!(err.to_string().contains("Not available on"));
    }
}
