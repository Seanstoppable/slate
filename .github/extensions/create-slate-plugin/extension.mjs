import { joinSession } from "@github/copilot-sdk/extension";
import { writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";

const session = await joinSession({
    skills: [
        {
            name: "create-slate-plugin",
            description:
                "Scaffold a new Slate WASM plugin module. Creates the directory structure, Cargo.toml, plugin.toml, and boilerplate src/lib.rs with extism-pdk exports. Use when the user wants to create a new Slate widget/module/plugin. IMPORTANT: Only use WASM plugins for widgets that fetch their own data via HTTP APIs or capability-gated host functions. If the widget needs direct OS/system access (power, network interfaces, firewall rules, VCS status, hardware sensors), it should be a builtin widget in crates/slate-cli/src/commands.rs instead — not a WASM plugin.",
        },
    ],
    tools: [
        {
            name: "scaffold_slate_plugin",
            description:
                "Create a new Slate WASM plugin with the correct project structure, Cargo.toml, plugin.toml, and starter code. The plugin will compile to wasm32-wasip1 and export metadata, refresh, on_key, and on_action functions.",
            parameters: {
                type: "object",
                properties: {
                    name: {
                        type: "string",
                        description:
                            "Plugin name in kebab-case (e.g., 'my-widget'). Will be prefixed with 'slate-' for the crate name.",
                    },
                    description: {
                        type: "string",
                        description: "Short description of what the plugin displays",
                    },
                    content_type: {
                        type: "string",
                        enum: ["text", "key_value", "list"],
                        description:
                            "Primary content type the plugin will render: text (simple display), key_value (labeled pairs), or list (selectable items)",
                    },
                    permissions: {
                        type: "array",
                        items: { type: "string" },
                        description:
                            "Required permissions: 'network' (HTTP requests), 'exec' (run commands), 'storage' (persistent KV), 'filesystem_read', 'raw_network' (ping/ICMP)",
                    },
                    network_hosts: {
                        type: "array",
                        items: { type: "string" },
                        description:
                            "Allowed network hosts (e.g., ['api.github.com', 'api.example.com']). Required if 'network' is in permissions.",
                    },
                    author: {
                        type: "string",
                        description: "Plugin author name",
                    },
                },
                required: ["name", "description", "content_type"],
            },
            handler: async (args) => {
                const {
                    name,
                    description,
                    content_type,
                    permissions = [],
                    network_hosts = [],
                    author = "Slate Community",
                } = args;

                const crateName = `slate-${name}`;
                const pluginDir = join("plugins", name);

                // Generate Cargo.toml
                const cargoToml = `[package]
name = "${crateName}"
version = "0.1.0"
edition = "2021"
description = "${description}"

[lib]
crate-type = ["cdylib"]

[dependencies]
extism-pdk = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
`;

                // Generate plugin.toml
                let permSection = "[permissions]\n";
                if (permissions.includes("network") && network_hosts.length > 0) {
                    permSection += `network = [${network_hosts.map((h) => `"${h}"`).join(", ")}]\n`;
                }
                if (permissions.includes("exec")) {
                    permSection += `exec = []\n`;
                }
                if (permissions.includes("storage")) {
                    permSection += `storage = true\n`;
                }
                if (permissions.includes("filesystem_read")) {
                    permSection += `filesystem_read = []\n`;
                }
                if (permissions.includes("raw_network")) {
                    permSection += `raw_network = true\n`;
                }

                const pluginToml = `[plugin]
name = "${name}"
description = "${description}"
version = "0.1.0"
author = "${author}"

${permSection}`;

                // Generate src/lib.rs based on content_type
                const libRs = generateLibRs(name, description, content_type, permissions);

                // Create files
                await mkdir(join(pluginDir, "src"), { recursive: true });
                await writeFile(join(pluginDir, "Cargo.toml"), cargoToml);
                await writeFile(join(pluginDir, "plugin.toml"), pluginToml);
                await writeFile(join(pluginDir, "src", "lib.rs"), libRs);

                return `Created Slate plugin "${name}" at ${pluginDir}/

Files created:
  ${pluginDir}/Cargo.toml
  ${pluginDir}/plugin.toml
  ${pluginDir}/src/lib.rs

To build:
  cd ${pluginDir}
  cargo build --release --target wasm32-wasip1

To use in slate.toml:
  [[widget]]
  type = "wasm:path/to/${pluginDir}/target/wasm32-wasip1/release/${crateName.replace(/-/g, "_")}.wasm"
  position = { row = 0, col = 0 }

Content type: ${content_type}
Permissions: ${permissions.length > 0 ? permissions.join(", ") : "none"}

NOTE: This creates a WASM plugin (sandboxed, portable). If this widget needs
direct OS access (system commands, hardware sensors, file reads), consider
making it a builtin widget in crates/slate-cli/src/commands.rs instead.
Builtins use native Rust and implement the same Widget trait.`;
            },
        },
    ],
});

function generateLibRs(name, description, contentType, permissions) {
    const hasNetwork = permissions.includes("network");

    const metadataFn = `/// Return plugin metadata.
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "${titleCase(name)}",
        "description": "${description}",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}`;

    let refreshFn;
    switch (contentType) {
        case "text":
            refreshFn = `/// Render widget content.
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    ${hasNetwork ? httpExample() : ""}
    let content = json!({
        "type": "text",
        "content": "Hello from ${titleCase(name)}!",
        "scrollable": false,
        "wrap": true
    });
    Ok(content.to_string())
}`;
            break;
        case "key_value":
            refreshFn = `/// Render widget content.
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    ${hasNetwork ? httpExample() : ""}
    let content = json!({
        "type": "key_value",
        "pairs": [
            {"key": "Status", "value": "OK"},
            {"key": "Info", "value": "Configure in slate.toml"}
        ]
    });
    Ok(content.to_string())
}`;
            break;
        case "list":
            refreshFn = `/// Render widget content.
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    ${hasNetwork ? httpExample() : ""}
    let content = json!({
        "type": "list",
        "items": [
            {"id": "1", "title": "Item 1", "subtitle": "Description"},
            {"id": "2", "title": "Item 2", "subtitle": "Description"}
        ],
        "selectable": true
    });
    Ok(content.to_string())
}`;
            break;
    }

    const onKeyFn = `/// Handle key events.
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}`;

    const onActionFn =
        contentType === "list"
            ? `
/// Handle actions on list items.
#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    #[derive(Deserialize)]
    struct ActionInput {
        action_id: String,
        item_id: String,
    }

    if let Ok(action) = serde_json::from_str::<ActionInput>(&input) {
        match action.action_id.as_str() {
            "select" => {
                // Handle item selection
                // Return {"open_url": "..."} to open a URL in the browser
            }
            _ => {}
        }
    }
    Ok(String::new())
}`
            : "";

    return `use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

${metadataFn}

${refreshFn}

${onKeyFn}
${onActionFn}
`;
}

function httpExample() {
    return `// Example HTTP request:
    // let req = HttpRequest::new("https://api.example.com/data")
    //     .with_header("Accept", "application/json");
    // let resp = http::request::<String>(&req, None)?;
    // let body = resp.body();
    // let data = std::str::from_utf8(&body).unwrap_or("{}");
`;
}

function titleCase(name) {
    return name
        .split("-")
        .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
        .join(" ");
}
