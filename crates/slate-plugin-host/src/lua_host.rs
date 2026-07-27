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

        // Inject the `slate` host API table before running the script
        Self::inject_host_api(&lua)?;

        let script = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read Lua script: {}", path.display()))?;

        lua.load(&script)
            .exec()
            .map_err(|e| anyhow::anyhow!("Failed to execute Lua script {}: {}", path.display(), e))?;

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

    /// Inject `slate.*` host functions into the Lua environment.
    fn inject_host_api(lua: &Lua) -> Result<()> {
        let slate = lua.create_table().map_err(|e| anyhow::anyhow!("{}", e))?;

        // slate.exec(cmd, args?) -> { stdout, stderr, exit_code }
        let exec_fn = lua.create_function(|lua_ctx, (cmd, args): (String, Option<Vec<String>>)| {
            let args = args.unwrap_or_default();
            let output = std::process::Command::new(&cmd)
                .args(&args)
                .output();

            match output {
                Ok(out) => {
                    let tbl = lua_ctx.create_table()?;
                    tbl.set("stdout", String::from_utf8_lossy(&out.stdout).to_string())?;
                    tbl.set("stderr", String::from_utf8_lossy(&out.stderr).to_string())?;
                    tbl.set("exit_code", out.status.code().unwrap_or(-1))?;
                    Ok(tbl)
                }
                Err(e) => {
                    let tbl = lua_ctx.create_table()?;
                    tbl.set("stdout", "")?;
                    tbl.set("stderr", e.to_string())?;
                    tbl.set("exit_code", -1)?;
                    Ok(tbl)
                }
            }
        }).map_err(|e| anyhow::anyhow!("{}", e))?;
        slate.set("exec", exec_fn).map_err(|e| anyhow::anyhow!("{}", e))?;

        // slate.read_file(path) -> string or nil
        let read_file_fn = lua.create_function(|_, path: String| {
            match std::fs::read_to_string(&path) {
                Ok(content) => Ok(Some(content)),
                Err(_) => Ok(None),
            }
        }).map_err(|e| anyhow::anyhow!("{}", e))?;
        slate.set("read_file", read_file_fn).map_err(|e| anyhow::anyhow!("{}", e))?;

        // slate.time() -> { hour, min, sec, year, month, day, weekday, timestamp }
        let time_fn = lua.create_function(|lua_ctx, ()| {
            use chrono::Local;
            let now = Local::now();
            let tbl = lua_ctx.create_table()?;
            tbl.set("hour", now.format("%H").to_string().parse::<i32>().unwrap_or(0))?;
            tbl.set("min", now.format("%M").to_string().parse::<i32>().unwrap_or(0))?;
            tbl.set("sec", now.format("%S").to_string().parse::<i32>().unwrap_or(0))?;
            tbl.set("year", now.format("%Y").to_string().parse::<i32>().unwrap_or(0))?;
            tbl.set("month", now.format("%m").to_string().parse::<i32>().unwrap_or(0))?;
            tbl.set("day", now.format("%d").to_string().parse::<i32>().unwrap_or(0))?;
            tbl.set("weekday", now.format("%A").to_string())?;
            tbl.set("timestamp", now.timestamp())?;
            Ok(tbl)
        }).map_err(|e| anyhow::anyhow!("{}", e))?;
        slate.set("time", time_fn).map_err(|e| anyhow::anyhow!("{}", e))?;

        // slate.env(name) -> string or nil
        let env_fn = lua.create_function(|_, name: String| {
            Ok(std::env::var(&name).ok())
        }).map_err(|e| anyhow::anyhow!("{}", e))?;
        slate.set("env", env_fn).map_err(|e| anyhow::anyhow!("{}", e))?;

        lua.globals().set("slate", slate).map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(())
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

    fn on_action(&mut self, action_id: &str, item_id: &str) -> Option<slate_plugin_sdk::WidgetAction> {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_action") {
            let _ = func.call::<()>((action_id.to_string(), item_id.to_string()));
        }
        None
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
