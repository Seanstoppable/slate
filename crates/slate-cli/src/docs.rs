use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, Default, Clone)]
struct DocsManifest {
    #[serde(default)]
    plugin: DocsManifestPlugin,
    #[serde(default)]
    metadata: DocsManifestPlugin,
    #[serde(default)]
    permissions: DocsManifestPermissions,
    #[serde(default)]
    config: HashMap<String, DocsConfigField>,
}

#[derive(serde::Deserialize, Default, Clone)]
struct DocsManifestPlugin {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    os: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(serde::Deserialize, Default, Clone)]
struct DocsManifestPermissions {
    #[serde(default)]
    network: Vec<String>,
    #[serde(default)]
    exec: Vec<String>,
    #[serde(default)]
    secrets: Vec<String>,
    #[serde(default)]
    storage: Option<bool>,
    #[serde(default)]
    filesystem_read: Vec<String>,
    #[serde(default)]
    raw_network: Option<bool>,
}

#[derive(serde::Deserialize, Default, Clone)]
struct DocsConfigField {
    #[serde(default, rename = "type")]
    field_type: String,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    description: String,
    #[allow(dead_code)]
    #[serde(default)]
    default: Option<String>,
}

struct PluginInfo {
    name: String,
    description: String,
    version: String,
    author: String,
    language: String,
    os: Vec<String>,
    tags: Vec<String>,
    permissions: Vec<String>,
    kind: &'static str,
    config_example: String,
    install_hint: String,
    /// A live-rendered HTML snapshot of the widget's actual terminal output, generated at
    /// docs-build time from real widget content (builtins and Lua scripts only).
    snapshot: Option<String>,
}

pub async fn docs(output_dir: Option<&str>) -> Result<()> {
    let out = PathBuf::from(output_dir.unwrap_or("docs/plugins"));
    std::fs::create_dir_all(&out)?;

    let mut plugins: Vec<PluginInfo> = Vec::new();

    let plugins_path = Path::new("plugins");
    if plugins_path.exists() {
        for entry in std::fs::read_dir(plugins_path)? {
            let entry = entry?;
            let toml_path = entry.path().join("plugin.toml");
            if toml_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&toml_path) {
                    if let Ok(manifest) = toml::from_str::<DocsManifest>(&content) {
                        let p = if !manifest.plugin.name.is_empty() {
                            &manifest.plugin
                        } else {
                            &manifest.metadata
                        };
                        let mut perms = Vec::new();
                        if !manifest.permissions.network.is_empty() {
                            perms.push(format!(
                                "network: {}",
                                manifest.permissions.network.join(", ")
                            ));
                        }
                        if !manifest.permissions.exec.is_empty() {
                            perms.push(format!("exec: {}", manifest.permissions.exec.join(", ")));
                        }
                        if !manifest.permissions.secrets.is_empty() {
                            perms.push(format!(
                                "secrets: {}",
                                manifest.permissions.secrets.join(", ")
                            ));
                        }
                        if manifest.permissions.storage == Some(true) {
                            perms.push("storage".to_string());
                        }
                        if !manifest.permissions.filesystem_read.is_empty() {
                            perms.push(format!(
                                "filesystem_read: {}",
                                manifest.permissions.filesystem_read.join(", ")
                            ));
                        }
                        if manifest.permissions.raw_network == Some(true) {
                            perms.push("raw_network".to_string());
                        }

                        let config_example = generate_config_example(&p.name, "plugin", &manifest);
                        let install_hint = if p.language.is_empty() || p.language == "rust" {
                            format!(
                                "Add to slate.toml:\n  type = \"github.com/slate-community/slate-{}\"",
                                p.name
                            )
                        } else {
                            format!(
                                "Build: see plugins/{}/README.md\nOr add pre-built: type = \"wasm:path/to/plugin.wasm\"",
                                p.name
                            )
                        };

                        plugins.push(PluginInfo {
                            name: p.name.clone(),
                            description: p.description.clone(),
                            version: p.version.clone(),
                            author: p.author.clone(),
                            language: if p.language.is_empty() {
                                "rust".to_string()
                            } else {
                                p.language.clone()
                            },
                            os: p.os.clone(),
                            tags: p.tags.clone(),
                            permissions: perms,
                            kind: "plugin",
                            config_example,
                            install_hint,
                            snapshot: snapshot_manifest_plugin(&entry.path(), &p.name),
                        });
                    }
                }
            }
        }
    }

    let builtins: &[(&str, &str, &str, &str, &[&str])] = &[
        ("resource_usage", "CPU, memory, swap, and temperature monitoring", "Real-time system resource usage with configurable refresh rates. Shows CPU percentage, memory used/total, swap usage, CPU count, and hottest temperature sensor.", "[[widget]]\ntype = \"builtin:resource_usage\"\nposition = { row = 0, col = 0 }", &["monitoring", "system"]),
        ("power", "Battery status and power source", "Shows charge level, charging state, and power source. On desktops without a battery, displays 'AC Power (100%)'. Supports Windows (WMI), macOS (pmset), and Linux (sysfs).", "[[widget]]\ntype = \"builtin:power\"\nposition = { row = 0, col = 1 }", &["hardware", "system"]),
        ("firewall", "Firewall rules and status", "Displays active firewall rules. Uses netsh on Windows, pfctl on macOS, and iptables/nftables on Linux.", "[[widget]]\ntype = \"builtin:firewall\"\nposition = { row = 0, col = 2 }", &["network", "security", "system"]),
        ("ipaddresses", "Network interface IP addresses", "Lists all network interfaces with their IPv4/IPv6 addresses. Shows interface name, IP, and status. Defaults to all interfaces if none specified.", "[[widget]]\ntype = \"builtin:ipaddresses\"\nposition = { row = 1, col = 0 }", &["network", "utility"]),
        ("logfile", "Display and tail text files", "Shows the last N lines of a file with auto-refresh. Supports ~ expansion and environment variables in paths.", "[[widget]]\ntype = \"builtin:logfile\"\nposition = { row = 1, col = 1 }\nfilePath = \"~/app.log\"", &["monitoring", "utility"]),
    ];
    for (name, desc, _long_desc, config, tags) in builtins {
        plugins.push(PluginInfo {
            name: name.to_string(),
            description: desc.to_string(),
            version: "built-in".to_string(),
            author: "Slate".to_string(),
            language: "rust (native)".to_string(),
            os: vec![
                "macos".to_string(),
                "linux".to_string(),
                "windows".to_string(),
            ],
            tags: tags.iter().map(|t| t.to_string()).collect(),
            permissions: vec!["system (native access)".to_string()],
            kind: "builtin",
            config_example: config.to_string(),
            install_hint: "Built-in - no installation needed. Just add to slate.toml.".to_string(),
            snapshot: snapshot_builtin(name),
        });
    }

    let scripts_path = Path::new("scripts");
    if scripts_path.exists() {
        for entry in std::fs::read_dir(scripts_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let name = extract_lua_field(&content, "name").unwrap_or_else(|| {
                        path.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    });
                    let description =
                        extract_lua_field(&content, "description").unwrap_or_default();
                    let version = extract_lua_field(&content, "version")
                        .unwrap_or_else(|| "0.1.0".to_string());

                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let config_example = format!(
                        "[[widget]]\ntype = \"lua:scripts/{}\"\nposition = {{ row = 0, col = 0 }}",
                        filename
                    );

                    plugins.push(PluginInfo {
                        name: name.to_lowercase().replace(' ', "-"),
                        description,
                        version,
                        author: "Slate Community".to_string(),
                        language: "lua".to_string(),
                        os: vec![
                            "macos".to_string(),
                            "linux".to_string(),
                            "windows".to_string(),
                        ],
                        tags: vec!["script".to_string()],
                        permissions: vec!["unsandboxed (full io access)".to_string()],
                        kind: "script",
                        config_example,
                        install_hint: format!(
                            "Copy {} to your scripts/ folder and add to slate.toml.",
                            filename
                        ),
                        snapshot: snapshot_lua_script(&path),
                    });
                }
            }
        }
    }

    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let html = generate_docs_html(&plugins)?;
    let out_file = out.join("index.html");
    std::fs::write(&out_file, &html)?;

    println!("Generated plugin docs: {}", out_file.display());
    println!(
        "  {} plugins, {} builtins, {} scripts",
        plugins.iter().filter(|p| p.kind == "plugin").count(),
        plugins.iter().filter(|p| p.kind == "builtin").count(),
        plugins.iter().filter(|p| p.kind == "script").count(),
    );

    Ok(())
}

fn generate_config_example(name: &str, _kind: &str, manifest: &DocsManifest) -> String {
    let mut lines = vec![
        "[[widget]]".to_string(),
        format!("type = \"github.com/slate-community/slate-{}\"", name),
        "position = { row = 0, col = 0 }".to_string(),
    ];

    if !manifest.config.is_empty() {
        lines.push(String::new());
        lines.push("# Configuration".to_string());
        for (key, field) in &manifest.config {
            let required = field.required.unwrap_or(false);
            let example_value = match field.field_type.as_str() {
                "array" => "[\"example1\", \"example2\"]".to_string(),
                "bool" | "boolean" => "true".to_string(),
                "int" | "integer" | "number" => "1".to_string(),
                _ => "\"...\"".to_string(),
            };
            let comment = if !field.description.is_empty() {
                format!(
                    "  # {}{}",
                    field.description,
                    if required { " (required)" } else { "" }
                )
            } else if required {
                "  # (required)".to_string()
            } else {
                String::new()
            };
            lines.push(format!("{} = {}{}", key, example_value, comment));
        }
        return lines.join("\n");
    }

    if !manifest.permissions.secrets.is_empty() {
        for secret in &manifest.permissions.secrets {
            lines.push(format!("{} = \"${{{}}}\"", secret, secret.to_uppercase()));
        }
    }
    if !manifest.permissions.network.is_empty() && manifest.permissions.network[0] != "*" {
        if name == "weather" {
            lines.push("provider = \"openweathermap\"".to_string());
            lines.push("location = \"San Francisco\"".to_string());
        }
    }
    if !manifest.permissions.exec.is_empty() && name == "wego" {
        lines.push("days = \"1\"".to_string());
    }

    lines.join("\n")
}

/// Default snapshot canvas size: wide enough for typical widget content, tall enough
/// to show a handful of rows without the docs page HTML getting unwieldy.
const SNAPSHOT_WIDTH: u16 = 42;
const SNAPSHOT_HEIGHT: u16 = 8;

/// Render a real, live snapshot of a built-in widget by instantiating it and calling its
/// actual `refresh()` — the same code path used in the running dashboard — then rasterizing
/// the resulting content through the real ratatui renderer into HTML. Returns `None` if the
/// widget can't be constructed on this machine (e.g. unsupported OS API).
fn snapshot_builtin(name: &str) -> Option<String> {
    use slate_plugin_sdk::{Position, WidgetConfig};

    let config = WidgetConfig {
        position: Position {
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
        },
        settings: Default::default(),
        refresh_interval: None,
    };

    let mut widget = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::builtins::create_builtin(name, config)
    }))
    .ok()?
    .ok()?;

    let (metadata, content) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (widget.metadata(), widget.refresh())
    }))
    .ok()?;

    Some(slate_core::render::render_snapshot_html(
        &content,
        &metadata,
        SNAPSHOT_WIDTH,
        SNAPSHOT_HEIGHT,
    ))
}

/// Render a real, live snapshot of a Lua script widget by loading and executing it exactly
/// as the dashboard does, then rasterizing its output into HTML. Runs the script's actual
/// `refresh` logic (e.g. shelling out to `git`, `docker`, etc.), so the snapshot reflects
/// real output on the machine generating the docs. Returns `None` on any load/execution
/// failure so a single broken script can't break the whole docs build.
fn snapshot_lua_script(path: &Path) -> Option<String> {
    use slate_plugin_host::LuaPlugin;
    use slate_plugin_sdk::Widget;

    let path = path.to_path_buf();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let mut widget = LuaPlugin::from_file(&path).ok()?;
        let metadata = widget.metadata();
        let content = widget.refresh();
        Some(slate_core::render::render_snapshot_html(
            &content,
            &metadata,
            SNAPSHOT_WIDTH,
            SNAPSHOT_HEIGHT,
        ))
    }))
    .ok()?
}

/// Render a mock snapshot for a manifest-based (WASM/Go/Zig/etc.) plugin from a static
/// fixture file, `<plugin_dir>/docs_fixture.json`. Most of these plugins require live
/// credentials or network access (API tokens, `docker`/`brew` on PATH, etc.) that aren't
/// available in the docs-build environment, so instead of executing them we decode a
/// realistic sample of their actual `refresh()` JSON output through the exact same
/// wire-format parser (`slate_plugin_host::parse_widget_content`) the runtime host uses,
/// then rasterize it with the real renderer. Returns `None` if no fixture is present or it
/// fails to parse, in which case the docs page falls back to a "no live preview" hint.
fn snapshot_manifest_plugin(plugin_dir: &Path, name: &str) -> Option<String> {
    let fixture_path = plugin_dir.join("docs_fixture.json");
    let raw = std::fs::read_to_string(fixture_path).ok()?;

    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let content = slate_plugin_host::parse_widget_content(&raw);
        let metadata = slate_plugin_sdk::WidgetMetadata {
            name: name.to_string(),
            description: String::new(),
            version: String::new(),
            author: None,
            homepage: None,
        };
        slate_core::render::render_snapshot_html(
            &content,
            &metadata,
            SNAPSHOT_WIDTH,
            SNAPSHOT_HEIGHT,
        )
    }))
    .ok()
}

fn extract_lua_field(source: &str, field: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(field) {
            if let Some(rest) = trimmed.strip_prefix(field) {
                let rest = rest.trim();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rest = rest.trim();
                    if let Some(rest) = rest.strip_prefix('"') {
                        if let Some(end) = rest.find('"') {
                            return Some(rest[..end].to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn generate_docs_html(plugins: &[PluginInfo]) -> Result<String> {
    let mut cards = String::new();
    for (idx, p) in plugins.iter().enumerate() {
        let os_badges = if p.os.is_empty() {
            r#"<span class="os-badge all" title="All platforms">&#x1F310; All</span>"#.to_string()
        } else {
            p.os.iter()
                .map(|os| {
                    match os.as_str() {
                        "macos" => {
                            r#"<span class="os-badge macos" title="macOS">&#x1F34E; macOS</span>"#
                        }
                        "linux" => {
                            r#"<span class="os-badge linux" title="Linux">&#x1F427; Linux</span>"#
                        }
                        "windows" => {
                            r#"<span class="os-badge windows" title="Windows">&#x1FA9F; Windows</span>"#
                        }
                        other => return format!(r#"<span class="os-badge">{}</span>"#, other),
                    }
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join(" ")
        };

        let lang_class = match p.language.as_str() {
            "rust" | "rust (native)" => "lang-rust",
            "go" => "lang-go",
            "zig" => "lang-zig",
            "typescript" | "assemblyscript" => "lang-ts",
            _ => "lang-other",
        };

        let kind_badge = if p.kind == "builtin" {
            r#"<span class="kind-badge builtin">built-in</span>"#
        } else if p.kind == "script" {
            r#"<span class="kind-badge script">lua script</span>"#
        } else {
            r#"<span class="kind-badge plugin">plugin</span>"#
        };

        let perms_html = if p.permissions.is_empty() {
            "<em>None required</em>".to_string()
        } else {
            p.permissions
                .iter()
                .map(|perm| format!(r#"<span class="perm-tag">{}</span>"#, html_escape(perm)))
                .collect::<Vec<_>>()
                .join(" ")
        };

        let os_data = if p.os.is_empty() {
            "macos linux windows".to_string()
        } else {
            p.os.join(" ")
        };

        let tags_html = if p.tags.is_empty() {
            String::new()
        } else {
            p.tags
                .iter()
                .map(|tag| format!(r#"<span class="tag-badge">{}</span>"#, html_escape(tag)))
                .collect::<Vec<_>>()
                .join(" ")
        };

        cards.push_str(&format!(
            r#"<div class="plugin-card" data-os="{os_data}" data-lang="{lang}" data-kind="{kind}" data-tags="{tags}" data-name="{name}" data-desc="{desc}" onclick="showDetail({idx})">
  <div class="card-header">
    <h3>{name}</h3>
    <div class="badges">{kind_badge} <span class="lang-badge {lang_class}">{lang}</span></div>
  </div>
  <p class="description">{description}</p>
  <div class="tags-row">{tags_html}</div>
  <div class="card-meta">
    <div class="os-row">{os_badges}</div>
    <div class="perms-row"><strong>Permissions:</strong> {perms_html}</div>
    <div class="version-row">v{version} &middot; {author}</div>
  </div>
</div>
"#,
            idx = idx,
            os_data = os_data,
            lang = p.language,
            kind = p.kind,
            tags = p.tags.join(" "),
            name = html_escape(&p.name),
            desc = html_escape(&p.description),
            description = html_escape(&p.description),
            kind_badge = kind_badge,
            lang_class = lang_class,
            tags_html = tags_html,
            os_badges = os_badges,
            perms_html = perms_html,
            version = html_escape(&p.version),
            author = html_escape(&p.author),
        ));
    }

    let mut plugin_data = String::from("[\n");
    for (i, p) in plugins.iter().enumerate() {
        if i > 0 {
            plugin_data.push_str(",\n");
        }
        let os_list = if p.os.is_empty() {
            "\"All platforms\"".to_string()
        } else {
            p.os.iter()
                .map(|o| format!("\"{}\"", o))
                .collect::<Vec<_>>()
                .join(",")
        };
        let perms_list = p
            .permissions
            .iter()
            .map(|perm| format!("\"{}\"", js_escape(perm)))
            .collect::<Vec<_>>()
            .join(",");
        plugin_data.push_str(&format!(
            r#"  {{name:"{}",desc:"{}",version:"{}",author:"{}",lang:"{}",kind:"{}",os:[{}],perms:[{}],tags:[{}],config:"{}",install:"{}",snapshot:{}}}"#,
            js_escape(&p.name),
            js_escape(&p.description),
            js_escape(&p.version),
            js_escape(&p.author),
            js_escape(&p.language),
            p.kind,
            os_list,
            perms_list,
            p.tags.iter().map(|t| format!("\"{}\"", js_escape(t))).collect::<Vec<_>>().join(","),
            js_escape(&p.config_example),
            js_escape(&p.install_hint),
            match &p.snapshot {
                Some(html) => format!("\"{}\"", js_escape(html)),
                None => "null".to_string(),
            },
        ));
    }
    plugin_data.push_str("\n]");

    let template = load_template()?;
    Ok(template.replace("{{CARDS}}", &cards).replace(
        "{{PLUGIN_DATA}}",
        &plugin_data.replace("</script>", "<\\/script>"),
    ))
}

fn load_template() -> Result<String> {
    let template_path = resolve_template_path()?;
    Ok(std::fs::read_to_string(template_path)?)
}

fn resolve_template_path() -> Result<PathBuf> {
    let relative = PathBuf::from("docs").join("template.html");
    if relative.exists() {
        return Ok(relative);
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for ancestor in exe_dir.ancestors() {
                let candidate = ancestor.join("docs").join("template.html");
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }
    }

    Err(anyhow!("Template not found: docs/template.html"))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct CwdReset(PathBuf);

    impl CwdReset {
        fn change_to(path: &Path) -> Self {
            let old = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self(old)
        }
    }

    impl Drop for CwdReset {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    fn sample_plugin() -> PluginInfo {
        PluginInfo {
            name: "weather".to_string(),
            description: "Shows weather".to_string(),
            version: "1.2.3".to_string(),
            author: "Slate".to_string(),
            language: "rust".to_string(),
            os: vec!["macos".to_string(), "linux".to_string()],
            tags: vec!["weather".to_string(), "utility".to_string()],
            permissions: vec!["network: api.example.com".to_string()],
            kind: "plugin",
            config_example: "[[widget]]\ntype = \"github.com/slate-community/slate-weather\""
                .to_string(),
            install_hint: "Add to slate.toml".to_string(),
            snapshot: None,
        }
    }

    fn sample_builtin() -> PluginInfo {
        PluginInfo {
            name: "power".to_string(),
            description: "Shows battery state".to_string(),
            version: "built-in".to_string(),
            author: "Slate".to_string(),
            language: "rust (native)".to_string(),
            os: vec![],
            tags: vec!["system".to_string()],
            permissions: vec![],
            kind: "builtin",
            config_example: "[[widget]]\ntype = \"builtin:power\"".to_string(),
            install_hint: "Built-in".to_string(),
            snapshot: None,
        }
    }

    fn sample_script() -> PluginInfo {
        PluginInfo {
            name: "local-script".to_string(),
            description: "Scripted widget".to_string(),
            version: "0.1.0".to_string(),
            author: "Slate Community".to_string(),
            language: "lua".to_string(),
            os: vec!["windows".to_string()],
            tags: vec!["script".to_string()],
            permissions: vec!["unsandboxed (full io access)".to_string()],
            kind: "script",
            config_example: "[[widget]]\ntype = \"lua:scripts/local.lua\"".to_string(),
            install_hint: "Copy to scripts/".to_string(),
            snapshot: None,
        }
    }

    #[test]
    fn extract_lua_field_finds_string_values() {
        let source = "name = \"My Widget\"\ndescription = \"Does stuff\"\nversion = \"2.0.0\"";
        assert_eq!(
            extract_lua_field(source, "name"),
            Some("My Widget".to_string())
        );
        assert_eq!(
            extract_lua_field(source, "description"),
            Some("Does stuff".to_string())
        );
        assert_eq!(
            extract_lua_field(source, "version"),
            Some("2.0.0".to_string())
        );
    }

    #[test]
    fn extract_lua_field_returns_none_for_missing() {
        let source = "name = \"Hello\"\nfunction refresh() end";
        assert_eq!(extract_lua_field(source, "version"), None);
        assert_eq!(extract_lua_field(source, "missing"), None);
    }

    #[test]
    fn extract_lua_field_handles_whitespace_and_indentation() {
        let source = "    name   =   \"Indented Widget\"\n  description = \"With spaces\"";
        assert_eq!(
            extract_lua_field(source, "name"),
            Some("Indented Widget".to_string())
        );
        assert_eq!(
            extract_lua_field(source, "description"),
            Some("With spaces".to_string())
        );
    }

    #[test]
    fn generate_config_example_uses_config_section() {
        let manifest = DocsManifest {
            plugin: DocsManifestPlugin {
                name: "test".to_string(),
                ..Default::default()
            },
            metadata: DocsManifestPlugin::default(),
            permissions: DocsManifestPermissions::default(),
            config: [
                (
                    "url".to_string(),
                    DocsConfigField {
                        field_type: "string".to_string(),
                        required: Some(true),
                        description: "The API URL".to_string(),
                        default: None,
                    },
                ),
                (
                    "count".to_string(),
                    DocsConfigField {
                        field_type: "integer".to_string(),
                        required: Some(false),
                        description: "Number of items".to_string(),
                        default: None,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        };

        let example = generate_config_example("test", "plugin", &manifest);
        assert!(example.contains("[[widget]]"));
        assert!(example.contains("type = \"github.com/slate-community/slate-test\""));
        assert!(example.contains("# Configuration"));
        assert!(example.contains("(required)"));
        assert!(example.contains("The API URL"));
    }

    #[test]
    fn generate_config_example_falls_back_to_permissions() {
        let manifest = DocsManifest {
            plugin: DocsManifestPlugin::default(),
            metadata: DocsManifestPlugin::default(),
            permissions: DocsManifestPermissions {
                secrets: vec!["token".to_string()],
                ..Default::default()
            },
            config: HashMap::new(),
        };

        let example = generate_config_example("github", "plugin", &manifest);
        assert!(example.contains("token = \"${TOKEN}\""));
    }

    #[test]
    fn generate_config_example_covers_type_specific_and_special_case_defaults() {
        let manifest = DocsManifest {
            plugin: DocsManifestPlugin::default(),
            metadata: DocsManifestPlugin::default(),
            permissions: DocsManifestPermissions {
                network: vec!["api.openweathermap.org".to_string()],
                exec: vec!["wego".to_string()],
                ..Default::default()
            },
            config: [
                (
                    "items".to_string(),
                    DocsConfigField {
                        field_type: "array".to_string(),
                        required: Some(false),
                        description: String::new(),
                        default: None,
                    },
                ),
                (
                    "enabled".to_string(),
                    DocsConfigField {
                        field_type: "boolean".to_string(),
                        required: Some(false),
                        description: "Turn on the widget".to_string(),
                        default: None,
                    },
                ),
                (
                    "count".to_string(),
                    DocsConfigField {
                        field_type: "number".to_string(),
                        required: Some(true),
                        description: String::new(),
                        default: None,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        };

        let config_example = generate_config_example("numbers", "plugin", &manifest);
        assert!(config_example.contains("items = [\"example1\", \"example2\"]"));
        assert!(config_example.contains("enabled = true  # Turn on the widget"));
        assert!(config_example.contains("count = 1  # (required)"));

        let weather = generate_config_example(
            "weather",
            "plugin",
            &DocsManifest {
                permissions: DocsManifestPermissions {
                    network: vec!["api.openweathermap.org".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert!(weather.contains("provider = \"openweathermap\""));
        assert!(weather.contains("location = \"San Francisco\""));

        let wego = generate_config_example(
            "wego",
            "plugin",
            &DocsManifest {
                permissions: DocsManifestPermissions {
                    exec: vec!["wego".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert!(wego.contains("days = \"1\""));
    }

    #[test]
    fn html_and_js_escape_special_characters() {
        assert_eq!(
            html_escape("<tag attr=\"a&b\">'x'</tag>"),
            "&lt;tag attr=&quot;a&amp;b&quot;&gt;'x'&lt;/tag&gt;"
        );
        assert_eq!(
            js_escape("\\\"line1\nline2\r\t'"),
            "\\\\\\\"line1\\nline2\\t'"
        );
    }

    #[test]
    fn generate_docs_html_with_empty_plugins_fills_template_markers() {
        let html = generate_docs_html(&[]).unwrap();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<div class=\"grid\" id=\"grid\">"));
        assert!(html.contains("const pluginData = ["));
        assert!(!html.contains("{{CARDS}}"));
        assert!(!html.contains("{{PLUGIN_DATA}}"));
    }

    #[test]
    fn generate_docs_html_with_sample_plugins_contains_card_content() {
        let html = generate_docs_html(&[sample_plugin()]).unwrap();
        assert!(html.contains("<h3>weather</h3>"));
        assert!(html.contains("Shows weather"));
        assert!(html.contains("network: api.example.com"));
        assert!(html.contains("onclick=\"showDetail(0)\""));
    }

    #[test]
    fn generate_docs_html_renders_multiple_plugin_cards_and_badges() {
        let html =
            generate_docs_html(&[sample_plugin(), sample_builtin(), sample_script()]).unwrap();
        assert!(html.contains("kind-badge plugin"));
        assert!(html.contains("kind-badge builtin"));
        assert!(html.contains("kind-badge script"));
        assert!(html.contains("lang-badge lang-rust"));
        assert!(html.contains("lang-badge lang-other\">lua</span>"));
        assert!(html.contains("&#x1F34E; macOS"));
        assert!(html.contains("&#x1F310; All"));
        assert!(html.contains("<em>None required</em>"));
    }

    #[test]
    fn generate_docs_html_embeds_plugin_data_javascript() {
        let html = generate_docs_html(&[sample_plugin(), sample_builtin()]).unwrap();
        assert!(html.contains("const pluginData = ["));
        assert!(html.contains("{name:\"weather\",desc:\"Shows weather\""));
        assert!(html.contains("kind:\"plugin\""));
        assert!(html.contains("os:[\"macos\",\"linux\"]"));
        assert!(html.contains("perms:[\"network: api.example.com\"]"));
        assert!(html.contains("{name:\"power\",desc:\"Shows battery state\""));
        assert!(html.contains("os:[\"All platforms\"]"));
        assert!(html.contains("perms:[]"));
    }

    #[test]
    fn generate_docs_html_covers_additional_language_and_os_badges() {
        let html = generate_docs_html(&[
            PluginInfo {
                name: "go-tool".to_string(),
                description: "Go plugin".to_string(),
                version: "1.0.0".to_string(),
                author: "Slate".to_string(),
                language: "go".to_string(),
                os: vec!["freebsd".to_string()],
                tags: vec![],
                permissions: vec!["exec: tool".to_string()],
                kind: "plugin",
                config_example: String::new(),
                install_hint: String::new(),
                snapshot: None,
            },
            PluginInfo {
                name: "zig-tool".to_string(),
                description: "Zig plugin".to_string(),
                version: "1.0.0".to_string(),
                author: "Slate".to_string(),
                language: "zig".to_string(),
                os: vec!["windows".to_string()],
                tags: vec![],
                permissions: vec![],
                kind: "plugin",
                config_example: String::new(),
                install_hint: String::new(),
                snapshot: None,
            },
            PluginInfo {
                name: "ts-tool".to_string(),
                description: "TS plugin".to_string(),
                version: "1.0.0".to_string(),
                author: "Slate".to_string(),
                language: "typescript".to_string(),
                os: vec!["linux".to_string()],
                tags: vec![],
                permissions: vec![],
                kind: "plugin",
                config_example: String::new(),
                install_hint: String::new(),
                snapshot: None,
            },
        ])
        .unwrap();

        assert!(html.contains("lang-badge lang-go"));
        assert!(html.contains("lang-badge lang-zig"));
        assert!(html.contains("lang-badge lang-ts"));
        assert!(html.contains("&#x1FA9F; Windows"));
        assert!(html.contains("<span class=\"os-badge\">freebsd</span>"));
    }

    #[test]
    fn template_file_exists_and_contains_placeholders() {
        let template_path = resolve_template_path().unwrap();
        let template = std::fs::read_to_string(template_path).unwrap();
        assert!(template.contains("{{CARDS}}"));
        assert!(template.contains("{{PLUGIN_DATA}}"));
    }

    #[test]
    fn load_template_returns_html_shell_with_placeholders() {
        let template = load_template().unwrap();
        assert!(template.contains("<!DOCTYPE html>"));
        assert!(template.contains("{{CARDS}}"));
        assert!(template.contains("{{PLUGIN_DATA}}"));
    }

    #[test]
    fn resolve_template_path_falls_back_to_executable_ancestors() {
        let _lock = cwd_lock().lock().unwrap();
        let dir = tempdir().unwrap();
        let _cwd = CwdReset::change_to(dir.path());

        let path = resolve_template_path().unwrap();

        assert!(path.ends_with(Path::new("docs").join("template.html")));
        assert!(!path.starts_with(dir.path()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn docs_generates_from_plugin_builtin_and_script_directories() {
        let _lock = cwd_lock().lock().unwrap();
        let dir = tempdir().unwrap();

        let plugin_dir = dir.path().join("plugins").join("clock");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
[plugin]
name = "clock"
description = "World clocks"
version = "0.1.0"
author = "Test"
language = "rust"
os = ["macos", "linux", "windows"]

[permissions]
network = ["worldtimeapi.org"]
storage = true
raw_network = true
filesystem_read = ["C:\\Users\\Public"]

[config]
timezone = { type = "string", required = true, description = "Timezone" }
enabled = { type = "boolean", description = "Whether the clock is enabled" }
"#,
        )
        .unwrap();

        let legacy_dir = dir.path().join("plugins").join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(
            legacy_dir.join("plugin.toml"),
            r#"
[metadata]
name = "legacy"
description = "Legacy metadata plugin"
version = "2.0.0"
author = "Legacy"
language = "go"
"#,
        )
        .unwrap();

        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::write(
            scripts_dir.join("greeting.lua"),
            r#"
name = "Greeting"
description = "A greeting widget"
version = "1.0.0"
function refresh() return '{"type":"text","content":"Hello","scrollable":false,"wrap":true}' end
"#,
        )
        .unwrap();

        let template_dir = dir.path().join("docs");
        std::fs::create_dir_all(&template_dir).unwrap();
        let template = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("template.html"),
        )
        .unwrap();
        std::fs::write(template_dir.join("template.html"), template).unwrap();

        let output_dir = dir.path().join("output");
        let _cwd = CwdReset::change_to(dir.path());

        docs(Some(output_dir.to_str().unwrap())).await.unwrap();

        let html = std::fs::read_to_string(output_dir.join("index.html")).unwrap();
        assert!(html.contains("clock"));
        assert!(html.contains("legacy"));
        assert!(html.contains("greeting"));
        assert!(html.contains("worldtimeapi.org"));
        assert!(html.contains("filesystem_read"));
        assert!(html.contains("raw_network"));
        assert!(html.contains("builtin:logfile"));
        assert!(html.contains("github.com/slate-community/slate-clock"));
        assert!(html.contains("lua:scripts/greeting.lua"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn docs_uses_default_output_and_skips_invalid_entries() {
        let _lock = cwd_lock().lock().unwrap();
        let dir = tempdir().unwrap();

        let valid_plugin_dir = dir.path().join("plugins").join("alpha");
        std::fs::create_dir_all(&valid_plugin_dir).unwrap();
        std::fs::write(
            valid_plugin_dir.join("plugin.toml"),
            r#"
[plugin]
name = "alpha"
description = "Alpha plugin"
version = "1.0.0"

[permissions]
exec = ["echo"]
secrets = ["api_token"]
"#,
        )
        .unwrap();

        let invalid_manifest_dir = dir.path().join("plugins").join("broken");
        std::fs::create_dir_all(&invalid_manifest_dir).unwrap();
        std::fs::write(invalid_manifest_dir.join("plugin.toml"), "not = [valid").unwrap();

        let unreadable_manifest_dir = dir.path().join("plugins").join("directory-manifest");
        std::fs::create_dir_all(unreadable_manifest_dir.join("plugin.toml")).unwrap();

        let ignored_dir = dir.path().join("plugins").join("missing-manifest");
        std::fs::create_dir_all(&ignored_dir).unwrap();

        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::write(
            scripts_dir.join("fallback.lua"),
            r#"
function refresh() return '{"type":"text","content":"fallback","scrollable":false,"wrap":true}' end
"#,
        )
        .unwrap();
        std::fs::write(scripts_dir.join("ignore.txt"), "not lua").unwrap();

        let template_dir = dir.path().join("docs");
        std::fs::create_dir_all(&template_dir).unwrap();
        let template = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("template.html"),
        )
        .unwrap();
        std::fs::write(template_dir.join("template.html"), template).unwrap();

        let _cwd = CwdReset::change_to(dir.path());
        docs(None).await.unwrap();

        let output = dir.path().join("docs").join("plugins").join("index.html");
        let html = std::fs::read_to_string(output).unwrap();
        assert!(html.contains("alpha"));
        assert!(html.contains("fallback"));
        assert!(html.contains("exec: echo"));
        assert!(html.contains("secrets: api_token"));
        assert!(html.contains("lang:\"rust\""));
        assert!(!html.contains("Build: see plugins/alpha/README.md"));
        assert!(!html.contains("broken"));
        assert!(!html.contains("missing-manifest"));
        assert!(!html.contains("ignore.txt"));
        assert!(html.contains("lua:scripts/fallback.lua"));
    }
}
