use anyhow::{Context, Result};
use mlua::prelude::*;
use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};
use std::path::Path;

/// A Lua-scripted plugin using mlua (Luau runtime).
pub struct LuaPlugin {
    lua: Lua,
    metadata: WidgetMetadata,
    script_path: String,
}

impl LuaPlugin {
    /// Load a Lua plugin from a script file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let lua = Lua::new();
        let script = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read Lua script: {}", path.display()))?;

        lua.load(&script)
            .exec()
            .with_context(|| format!("Failed to execute Lua script: {}", path.display()))?;

        // Extract metadata from Lua globals
        let name: String = lua
            .globals()
            .get::<String>("name")
            .unwrap_or_else(|_| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("lua-widget")
                    .to_string()
            });

        let description: String = lua
            .globals()
            .get::<String>("description")
            .unwrap_or_default();

        let version: String = lua
            .globals()
            .get::<String>("version")
            .unwrap_or_else(|_| "0.1.0".to_string());

        Ok(Self {
            lua,
            metadata: WidgetMetadata {
                name,
                description,
                version,
                author: None,
                homepage: None,
            },
            script_path: path.display().to_string(),
        })
    }
}

impl slate_plugin_sdk::Widget for LuaPlugin {
    fn metadata(&self) -> WidgetMetadata {
        self.metadata.clone()
    }

    fn init(&mut self, config: WidgetConfig) {
        // Pass config to Lua as a global table
        if let Ok(settings_json) = serde_json::to_string(&config.settings) {
            let _ = self.lua.globals().set("config_json", settings_json);
        }
    }

    fn refresh(&mut self) -> WidgetContent {
        // Call the Lua `refresh()` function and parse its return value
        let result: Result<String, _> = self.lua.globals().get::<LuaFunction>("refresh").and_then(|f| f.call(()));

        match result {
            Ok(content) => {
                // Try to parse as JSON WidgetContent
                serde_json::from_str(&content).unwrap_or_else(|_| WidgetContent::Text {
                    content,
                    scrollable: false,
                    wrap: true,
                })
            }
            Err(e) => WidgetContent::Text {
                content: format!("[Lua error] {}: {}", self.script_path, e),
                scrollable: false,
                wrap: true,
            },
        }
    }

    fn on_key(&mut self, key: &str, action: &str) {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_key") {
            let _ = func.call::<()>((key.to_string(), action.to_string()));
        }
    }

    fn on_action(&mut self, action_id: &str, item_id: &str) {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_action") {
            let _ = func.call::<()>((action_id.to_string(), item_id.to_string()));
        }
    }

    fn on_focus(&mut self) {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_focus") {
            let _ = func.call::<()>(());
        }
    }

    fn on_blur(&mut self) {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_blur") {
            let _ = func.call::<()>(());
        }
    }
}
