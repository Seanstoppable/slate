use anyhow::{Context, Result};
use mlua::prelude::*;
use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};
use std::path::Path;

/// A Lua-scripted plugin using mlua (Luau runtime).
pub struct LuaPlugin {
    lua: Lua,
    metadata: WidgetMetadata,
    script_path: String,
    /// Most recent error from a callback (`on_key`, `on_action`, `on_focus`,
    /// `on_blur`), surfaced on the next `refresh()` render.
    last_error: Option<String>,
}

impl LuaPlugin {
    /// Load a Lua plugin from a script file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let lua = Lua::new();

        // Inject the `slate` host API table before running the script
        Self::inject_host_api(&lua)?;

        let script = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read Lua script: {}", path.display()))?;

        lua.load(&script).exec().map_err(|e| {
            anyhow::anyhow!("Failed to execute Lua script {}: {}", path.display(), e)
        })?;

        // Extract metadata from Lua globals
        let name: String = lua.globals().get::<String>("name").unwrap_or_else(|_| {
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
            last_error: None,
        })
    }

    /// Inject `slate.*` host functions into the Lua environment.
    fn inject_host_api(lua: &Lua) -> Result<()> {
        let slate = lua.create_table().map_err(|e| anyhow::anyhow!("{}", e))?;

        // slate.exec(cmd, args?) -> { stdout, stderr, exit_code }
        let exec_fn = lua
            .create_function(|lua_ctx, (cmd, args): (String, Option<Vec<String>>)| {
                let args = args.unwrap_or_default();
                let output = std::process::Command::new(&cmd).args(&args).output();

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
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        slate
            .set("exec", exec_fn)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // slate.read_file(path) -> string or nil
        let read_file_fn = lua
            .create_function(|_, path: String| match std::fs::read_to_string(&path) {
                Ok(content) => Ok(Some(content)),
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "slate.read_file failed");
                    Ok(None)
                }
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        slate
            .set("read_file", read_file_fn)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // slate.time() -> { hour, min, sec, year, month, day, weekday, timestamp }
        let time_fn = lua
            .create_function(|lua_ctx, ()| {
                use chrono::Local;
                let now = Local::now();
                let tbl = lua_ctx.create_table()?;
                tbl.set(
                    "hour",
                    now.format("%H").to_string().parse::<i32>().unwrap_or(0),
                )?;
                tbl.set(
                    "min",
                    now.format("%M").to_string().parse::<i32>().unwrap_or(0),
                )?;
                tbl.set(
                    "sec",
                    now.format("%S").to_string().parse::<i32>().unwrap_or(0),
                )?;
                tbl.set(
                    "year",
                    now.format("%Y").to_string().parse::<i32>().unwrap_or(0),
                )?;
                tbl.set(
                    "month",
                    now.format("%m").to_string().parse::<i32>().unwrap_or(0),
                )?;
                tbl.set(
                    "day",
                    now.format("%d").to_string().parse::<i32>().unwrap_or(0),
                )?;
                tbl.set("weekday", now.format("%A").to_string())?;
                tbl.set("timestamp", now.timestamp())?;
                Ok(tbl)
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        slate
            .set("time", time_fn)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // slate.env(name) -> string or nil
        let env_fn = lua
            .create_function(|_, name: String| Ok(std::env::var(&name).ok()))
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        slate
            .set("env", env_fn)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        lua.globals()
            .set("slate", slate)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Inject content helper library
        lua.load(include_str!("lua_helpers.lua"))
            .exec()
            .map_err(|e| anyhow::anyhow!("Failed to load Lua helpers: {}", e))?;

        Ok(())
    }

    /// Log a Lua callback failure and stash it so the next render shows it.
    fn record_error(&mut self, callback: &str, err: &LuaError) {
        tracing::error!(
            script = %self.script_path,
            callback = %callback,
            error = %err,
            "Lua callback failed"
        );
        self.last_error = Some(format!("{}: {}", callback, err));
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
        let result: Result<String, _> = self
            .lua
            .globals()
            .get::<LuaFunction>("refresh")
            .and_then(|f| f.call(()));

        match result {
            Ok(content) => {
                if let Some(err) = self.last_error.take() {
                    return WidgetContent::Text {
                        content: format!("[Lua error] {}: {}", self.script_path, err),
                        scrollable: false,
                        wrap: true,
                    };
                }
                // Try to parse as JSON WidgetContent
                serde_json::from_str(&content).unwrap_or(WidgetContent::Text {
                    content,
                    scrollable: false,
                    wrap: true,
                })
            }
            Err(e) => {
                self.last_error = None;
                tracing::error!(script = %self.script_path, error = %e, "Lua refresh() failed");
                WidgetContent::Text {
                    content: format!("[Lua error] {}: {}", self.script_path, e),
                    scrollable: false,
                    wrap: true,
                }
            }
        }
    }

    fn on_key(&mut self, key: &str, action: &str) {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_key") {
            if let Err(e) = func.call::<()>((key.to_string(), action.to_string())) {
                self.record_error("on_key", &e);
            }
        }
    }

    fn on_action(
        &mut self,
        action_id: &str,
        item_id: &str,
    ) -> Option<slate_plugin_sdk::WidgetAction> {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_action") {
            match func.call::<Option<String>>((action_id.to_string(), item_id.to_string())) {
                Ok(Some(json_str)) => return parse_widget_action(&json_str),
                Ok(None) => {}
                Err(e) => self.record_error("on_action", &e),
            }
        }
        None
    }

    fn on_focus(&mut self) {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_focus") {
            if let Err(e) = func.call::<()>(()) {
                self.record_error("on_focus", &e);
            }
        }
    }

    fn on_blur(&mut self) {
        if let Ok(func) = self.lua.globals().get::<LuaFunction>("on_blur") {
            if let Err(e) = func.call::<()>(()) {
                self.record_error("on_blur", &e);
            }
        }
    }
}

/// Parse a JSON string returned from on_action into a WidgetAction.
/// Supported formats:
///   {"open_url": "https://..."}
///   {"notify": "message"}
///   {"show_detail": "detail text content"}
fn parse_widget_action(json_str: &str) -> Option<slate_plugin_sdk::WidgetAction> {
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    if let Some(url) = value.get("open_url").and_then(|v| v.as_str()) {
        Some(slate_plugin_sdk::WidgetAction::OpenUrl(url.to_string()))
    } else if let Some(msg) = value.get("notify").and_then(|v| v.as_str()) {
        Some(slate_plugin_sdk::WidgetAction::Notify(msg.to_string()))
    } else {
        value
            .get("show_detail")
            .and_then(|v| v.as_str())
            .map(|detail| slate_plugin_sdk::WidgetAction::ShowDetail(detail.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::Widget;

    fn write_script(dir_name: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("s.lua");
        std::fs::write(&script_path, body).unwrap();
        (dir, script_path)
    }

    const OK_REFRESH: &str =
        "function refresh() return '{\"type\":\"text\",\"content\":\"ok\",\"scrollable\":false,\"wrap\":true}' end\n";

    fn error_text(content: WidgetContent) -> String {
        match content {
            WidgetContent::Text { content, .. } => content,
            other => panic!("expected text, got {:?}", other),
        }
    }

    #[test]
    fn on_key_error_surfaces_in_next_refresh() {
        let (dir, path) = write_script(
            "slate_test_lua_on_key_err",
            &format!("{}function on_key(k, a) error('boom') end\n", OK_REFRESH),
        );
        let mut plugin = LuaPlugin::from_file(&path).unwrap();
        plugin.on_key("j", "down");
        let text = error_text(plugin.refresh());
        assert!(text.contains("[Lua error]"), "{}", text);
        assert!(text.contains("on_key"), "{}", text);
        assert!(text.contains("boom"), "{}", text);
        // Error is cleared after being shown once.
        assert_eq!(error_text(plugin.refresh()), "ok");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn on_action_error_surfaces_and_returns_none() {
        let (dir, path) = write_script(
            "slate_test_lua_on_action_err",
            &format!(
                "{}function on_action(a, i) error('bad action') end\n",
                OK_REFRESH
            ),
        );
        let mut plugin = LuaPlugin::from_file(&path).unwrap();
        assert!(plugin.on_action("open", "1").is_none());
        let text = error_text(plugin.refresh());
        assert!(
            text.contains("on_action") && text.contains("bad action"),
            "{}",
            text
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn on_action_returning_nil_records_no_error() {
        let (dir, path) = write_script(
            "slate_test_lua_on_action_nil",
            &format!("{}function on_action(a, i) return nil end\n", OK_REFRESH),
        );
        let mut plugin = LuaPlugin::from_file(&path).unwrap();
        assert!(plugin.on_action("open", "1").is_none());
        assert_eq!(error_text(plugin.refresh()), "ok");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn on_focus_and_on_blur_errors_surface() {
        let (dir, path) = write_script(
            "slate_test_lua_focus_err",
            &format!(
                "{}function on_focus() error('focus fail') end\nfunction on_blur() error('blur fail') end\n",
                OK_REFRESH
            ),
        );
        let mut plugin = LuaPlugin::from_file(&path).unwrap();
        plugin.on_focus();
        let text = error_text(plugin.refresh());
        assert!(
            text.contains("on_focus") && text.contains("focus fail"),
            "{}",
            text
        );

        plugin.on_blur();
        let text = error_text(plugin.refresh());
        assert!(
            text.contains("on_blur") && text.contains("blur fail"),
            "{}",
            text
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refresh_error_clears_pending_callback_error() {
        let (dir, path) = write_script(
            "slate_test_lua_refresh_err",
            "function refresh() error('refresh fail') end\nfunction on_key(k, a) error('key fail') end\n",
        );
        let mut plugin = LuaPlugin::from_file(&path).unwrap();
        plugin.on_key("j", "down");
        let text = error_text(plugin.refresh());
        assert!(text.contains("refresh fail"), "{}", text);
        assert!(plugin.last_error.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_file_returns_nil_for_missing_path() {
        let (dir, path) = write_script(
            "slate_test_lua_read_file",
            "function refresh()\n  local c = slate.read_file('/definitely/not/a/real/path/xyz')\n  if c == nil then return 'missing' end\n  return 'found'\nend\n",
        );
        let mut plugin = LuaPlugin::from_file(&path).unwrap();
        assert_eq!(error_text(plugin.refresh()), "missing");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_plugin_loads_metadata_from_globals() {
        let dir = std::env::temp_dir().join("slate_test_lua_meta");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("test_meta.lua");
        std::fs::write(&script_path, r#"
            name = "Test Widget"
            description = "A test"
            version = "1.2.3"
            function refresh() return '{"type":"text","content":"hi","scrollable":false,"wrap":true}' end
        "#).unwrap();

        let plugin = LuaPlugin::from_file(&script_path).unwrap();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "Test Widget");
        assert_eq!(meta.description, "A test");
        assert_eq!(meta.version, "1.2.3");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_plugin_defaults_metadata_when_missing() {
        let dir = std::env::temp_dir().join("slate_test_lua_defaults");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("my_widget.lua");
        std::fs::write(&script_path, r#"
            function refresh() return '{"type":"text","content":"ok","scrollable":false,"wrap":true}' end
        "#).unwrap();

        let plugin = LuaPlugin::from_file(&script_path).unwrap();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "my_widget");
        assert_eq!(meta.version, "0.1.0");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_plugin_refresh_returns_parsed_content() {
        let dir = std::env::temp_dir().join("slate_test_lua_refresh");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("test_refresh.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Refresher"
            function refresh()
                return '{"type":"key_value","pairs":[["CPU",{"text":"42%"}]]}'
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::KeyValue { pairs } => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0].0, "CPU");
            }
            other => panic!("expected key_value, got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_plugin_refresh_returns_error_on_bad_script() {
        let dir = std::env::temp_dir().join("slate_test_lua_err");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("test_err.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Broken"
            function refresh()
                error("something went wrong")
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("[Lua error]"));
                assert!(content.contains("something went wrong"));
            }
            other => panic!("expected text error, got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_plugin_refresh_wraps_plain_text_when_json_parsing_fails() {
        let dir = std::env::temp_dir().join("slate_test_lua_plain_text");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("plain_text.lua");
        std::fs::write(
            &script_path,
            r#"
            function refresh()
                return "plain text"
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        match plugin.refresh() {
            WidgetContent::Text {
                content,
                scrollable,
                wrap,
            } => {
                assert_eq!(content, "plain text");
                assert!(!scrollable);
                assert!(wrap);
            }
            other => panic!("expected text content, got {:?}", other),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_host_slate_exec_runs_command() {
        let dir = std::env::temp_dir().join("slate_test_lua_exec");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("test_exec.lua");

        #[cfg(windows)]
        let script = r#"
            name = "Exec Test"
            function refresh()
                local r = slate.exec("cmd", {"/c", "echo hello"})
                if r.exit_code == 0 then
                    return '{"type":"text","content":"' .. r.stdout:gsub("%s+$","") .. '","scrollable":false,"wrap":true}'
                end
                return '{"type":"text","content":"failed","scrollable":false,"wrap":true}'
            end
        "#;
        #[cfg(not(windows))]
        let script = r#"
            name = "Exec Test"
            function refresh()
                local r = slate.exec("echo", {"hello"})
                if r.exit_code == 0 then
                    return '{"type":"text","content":"' .. r.stdout:gsub("%s+$","") .. '","scrollable":false,"wrap":true}'
                end
                return '{"type":"text","content":"failed","scrollable":false,"wrap":true}'
            end
        "#;

        std::fs::write(&script_path, script).unwrap();
        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text { content, .. } => {
                assert_eq!(content.trim(), "hello");
            }
            other => panic!("expected text with 'hello', got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_host_slate_exec_returns_error_for_missing_command() {
        let dir = std::env::temp_dir().join("slate_test_lua_exec_miss");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("test_exec_miss.lua");
        std::fs::write(&script_path, r#"
            name = "Exec Miss"
            function refresh()
                local r = slate.exec("nonexistent_command_xyz123", {})
                return '{"type":"text","content":"exit:' .. tostring(r.exit_code) .. '","scrollable":false,"wrap":true}'
            end
        "#).unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("exit:-1"));
            }
            other => panic!("expected text with exit:-1, got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_host_slate_time_returns_valid_fields() {
        let dir = std::env::temp_dir().join("slate_test_lua_time");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("test_time.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Time Test"
            function refresh()
                local t = slate.time()
                local valid = t.hour >= 0 and t.hour <= 23 and t.min >= 0 and t.min <= 59
                local has_weekday = type(t.weekday) == "string" and #t.weekday > 0
                local has_ts = t.timestamp > 0
                if valid and has_weekday and has_ts then
                    return '{"type":"text","content":"ok","scrollable":false,"wrap":true}'
                end
                return '{"type":"text","content":"bad","scrollable":false,"wrap":true}'
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text { content, .. } => assert_eq!(content, "ok"),
            other => panic!("expected 'ok', got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_host_slate_read_file_reads_existing() {
        let dir = std::env::temp_dir().join("slate_test_lua_read");
        std::fs::create_dir_all(&dir).unwrap();
        let data_path = dir.join("data.txt");
        std::fs::write(&data_path, "file contents here").unwrap();
        let escaped_path = data_path.to_string_lossy().replace('\\', "/");
        let script_path = dir.join("test_read.lua");
        std::fs::write(&script_path, format!(
            "name = \"Read Test\"\nfunction refresh()\n  local c = slate.read_file(\"{}\")\n  if c then return '{{\"type\":\"text\",\"content\":\"' .. c .. '\",\"scrollable\":false,\"wrap\":true}}' end\n  return '{{\"type\":\"text\",\"content\":\"nil\",\"scrollable\":false,\"wrap\":true}}'\nend",
            escaped_path
        )).unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text { content, .. } => assert_eq!(content, "file contents here"),
            other => panic!("expected file contents, got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_host_slate_read_file_returns_nil_for_missing() {
        let dir = std::env::temp_dir().join("slate_test_lua_read_miss");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("test_read_miss.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Read Miss"
            function refresh()
                local content = slate.read_file("/nonexistent/path/xyz.txt")
                if content == nil then
                    return '{"type":"text","content":"nil","scrollable":false,"wrap":true}'
                end
                return '{"type":"text","content":"unexpected","scrollable":false,"wrap":true}'
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text { content, .. } => assert_eq!(content, "nil"),
            other => panic!("expected 'nil', got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_host_slate_env_reads_variable() {
        let dir = std::env::temp_dir().join("slate_test_lua_env");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("test_env.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Env Test"
            function refresh()
                local path = slate.env("PATH")
                local missing = slate.env("SLATE_NONEXISTENT_VAR_XYZ")
                if path and not missing then
                    return '{"type":"text","content":"ok","scrollable":false,"wrap":true}'
                end
                return '{"type":"text","content":"bad","scrollable":false,"wrap":true}'
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text { content, .. } => assert_eq!(content, "ok"),
            other => panic!("expected 'ok', got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_plugin_on_key_invokes_script_handler() {
        let dir = std::env::temp_dir().join("slate_test_lua_on_key");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("on_key.lua");
        std::fs::write(
            &script_path,
            r#"
            function on_key(key, action)
                last = key .. ":" .. action
            end

            function refresh()
                return '{"type":"text","content":"' .. (last or "none") .. '","scrollable":false,"wrap":true}'
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        plugin.on_key("Enter", "press");

        match plugin.refresh() {
            WidgetContent::Text { content, .. } => assert_eq!(content, "Enter:press"),
            other => panic!("expected text content, got {:?}", other),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_plugin_focus_and_blur_invoke_optional_hooks() {
        let dir = std::env::temp_dir().join("slate_test_lua_focus_blur");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("focus_blur.lua");
        std::fs::write(
            &script_path,
            r#"
            state = "idle"

            function on_focus()
                state = "focused"
            end

            function on_blur()
                state = "blurred"
            end

            function refresh()
                return '{"type":"text","content":"' .. state .. '","scrollable":false,"wrap":true}'
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        plugin.on_focus();
        match plugin.refresh() {
            WidgetContent::Text { content, .. } => assert_eq!(content, "focused"),
            other => panic!("expected text content, got {:?}", other),
        }

        plugin.on_blur();
        match plugin.refresh() {
            WidgetContent::Text { content, .. } => assert_eq!(content, "blurred"),
            other => panic!("expected text content, got {:?}", other),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_script_state_persists_across_on_key_and_refresh() {
        // A Lua widget can be fully interactive -- state machine, keybindings,
        // rendering -- without touching any Rust code or recompiling the host,
        // because the interpreter lives as long as the widget does.
        let dir = std::env::temp_dir().join("slate_test_lua_stateful");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("stateful.lua");
        std::fs::write(
            &script_path,
            r#"
local running = false
local ticks = 0

function refresh()
    if running then ticks = ticks + 1 end
    return slate.text((running and "running" or "paused") .. " ticks=" .. ticks)
end

function on_key(key, _action)
    if key == "s" then running = true
    elseif key == "p" then running = false
    elseif key == "x" then running = false; ticks = 0
    end
end
"#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();

        let text = |content: WidgetContent| match content {
            WidgetContent::Text { content, .. } => content,
            other => panic!("expected text content, got {:?}", other),
        };

        assert!(text(plugin.refresh()).contains("paused ticks=0"));

        // 's' starts it -- state mutated purely via on_key.
        plugin.on_key("s", "");
        assert!(text(plugin.refresh()).contains("running ticks=1"));

        // State accumulates across refresh ticks.
        assert!(text(plugin.refresh()).contains("running ticks=2"));

        // 'p' pauses, so the counter stops advancing.
        plugin.on_key("p", "");
        assert!(text(plugin.refresh()).contains("paused ticks=2"));

        // 'x' resets.
        plugin.on_key("x", "");
        assert!(text(plugin.refresh()).contains("paused ticks=0"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_plugin_from_file_errors_on_missing_file() {
        let result = LuaPlugin::from_file(Path::new("/nonexistent/script.lua"));
        assert!(result.is_err());
    }

    #[test]
    fn lua_plugin_from_file_errors_on_syntax_error() {
        let dir = std::env::temp_dir().join("slate_test_lua_syntax");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("bad_syntax.lua");
        std::fs::write(&script_path, "function refresh( end").unwrap();

        let result = LuaPlugin::from_file(&script_path);
        assert!(result.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_widget_action_open_url() {
        let action = parse_widget_action(r#"{"open_url":"https://example.com"}"#);
        assert_eq!(
            action,
            Some(slate_plugin_sdk::WidgetAction::OpenUrl(
                "https://example.com".to_string()
            ))
        );
    }

    #[test]
    fn parse_widget_action_notify() {
        let action = parse_widget_action(r#"{"notify":"hello world"}"#);
        assert_eq!(
            action,
            Some(slate_plugin_sdk::WidgetAction::Notify(
                "hello world".to_string()
            ))
        );
    }

    #[test]
    fn parse_widget_action_show_detail() {
        let action = parse_widget_action(r#"{"show_detail":"commit abc123\nAuthor: dev"}"#);
        assert_eq!(
            action,
            Some(slate_plugin_sdk::WidgetAction::ShowDetail(
                "commit abc123\nAuthor: dev".to_string()
            ))
        );
    }

    #[test]
    fn parse_widget_action_empty_json() {
        let action = parse_widget_action("{}");
        assert_eq!(action, None);
    }

    #[test]
    fn parse_widget_action_invalid_json() {
        let action = parse_widget_action("not json");
        assert_eq!(action, None);
    }

    #[test]
    fn parse_widget_action_empty_string() {
        let action = parse_widget_action("");
        assert_eq!(action, None);
    }

    #[test]
    fn lua_on_action_returns_show_detail() {
        let dir = std::env::temp_dir().join("slate_test_lua_on_action_detail");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("action_detail.lua");
        std::fs::write(&script_path, r#"
            name = "Action Test"
            function refresh() return '{"type":"text","content":"hi","scrollable":false,"wrap":true}' end
            function on_action(action_id, item_id)
                return '{"show_detail":"Details for ' .. item_id .. '"}'
            end
        "#).unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let result = plugin.on_action("select", "abc123");
        assert_eq!(
            result,
            Some(slate_plugin_sdk::WidgetAction::ShowDetail(
                "Details for abc123".to_string()
            ))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_on_action_returns_open_url() {
        let dir = std::env::temp_dir().join("slate_test_lua_on_action_url");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("action_url.lua");
        std::fs::write(&script_path, r#"
            name = "URL Test"
            function refresh() return '{"type":"text","content":"hi","scrollable":false,"wrap":true}' end
            function on_action(action_id, item_id)
                return '{"open_url":"https://github.com/' .. item_id .. '"}'
            end
        "#).unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let result = plugin.on_action("open", "user/repo");
        assert_eq!(
            result,
            Some(slate_plugin_sdk::WidgetAction::OpenUrl(
                "https://github.com/user/repo".to_string()
            ))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_on_action_returns_none_when_nil() {
        let dir = std::env::temp_dir().join("slate_test_lua_on_action_nil");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("action_nil.lua");
        std::fs::write(&script_path, r#"
            name = "Nil Test"
            function refresh() return '{"type":"text","content":"hi","scrollable":false,"wrap":true}' end
            function on_action(action_id, item_id)
                return nil
            end
        "#).unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let result = plugin.on_action("select", "item1");
        assert_eq!(result, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_on_action_returns_none_when_not_defined() {
        let dir = std::env::temp_dir().join("slate_test_lua_on_action_missing");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("no_action.lua");
        std::fs::write(&script_path, r#"
            name = "No Action"
            function refresh() return '{"type":"text","content":"hi","scrollable":false,"wrap":true}' end
        "#).unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let result = plugin.on_action("select", "item1");
        assert_eq!(result, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_helpers_text_builds_valid_json() {
        let dir = std::env::temp_dir().join("slate_test_lua_helpers_text");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("helpers_text.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Helpers Text"
            function refresh()
                return slate.text("Hello\nWorld", {scrollable = true, wrap = false})
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text {
                content,
                scrollable,
                wrap,
            } => {
                assert_eq!(content, "Hello\nWorld");
                assert!(scrollable);
                assert!(!wrap);
            }
            _ => panic!("Expected Text content"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_helpers_list_builds_valid_json() {
        let dir = std::env::temp_dir().join("slate_test_lua_helpers_list");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("helpers_list.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Helpers List"
            function refresh()
                local items = {
                    {id = "a", title = "First", subtitle = "sub1"},
                    {id = "b", title = "Second", subtitle = "sub2"},
                }
                return slate.list(items, {selectable = true})
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::List {
                items, selectable, ..
            } => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].id, "a");
                assert_eq!(items[0].title, "First");
                assert_eq!(items[1].subtitle, Some("sub2".to_string()));
                assert!(selectable);
            }
            _ => panic!("Expected List content"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_helpers_key_value_builds_valid_json() {
        let dir = std::env::temp_dir().join("slate_test_lua_helpers_kv");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("helpers_kv.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Helpers KV"
            function refresh()
                local pairs = {
                    {"CPU", "42%"},
                    {"Memory", {text = "8GB", color = "green"}},
                }
                return slate.key_value(pairs)
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::KeyValue { pairs } => {
                assert_eq!(pairs.len(), 2);
                assert_eq!(pairs[0].0, "CPU");
                assert_eq!(pairs[1].0, "Memory");
            }
            _ => panic!("Expected KeyValue content, got {:?}", content),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_helpers_escape_handles_special_chars() {
        let dir = std::env::temp_dir().join("slate_test_lua_helpers_esc");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("helpers_esc.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Helpers Escape"
            function refresh()
                return slate.text('He said "hello"\nand left')
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let content = plugin.refresh();
        match content {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("\"hello\""));
                assert!(content.contains("\n"));
            }
            _ => panic!("Expected Text content"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lua_helpers_notify_returns_action() {
        let dir = std::env::temp_dir().join("slate_test_lua_helpers_notify");
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("helpers_notify.lua");
        std::fs::write(
            &script_path,
            r#"
            name = "Helpers Notify"
            function refresh() return slate.text("hi") end
            function on_action(action_id, item_id)
                return slate.notify("Something happened!")
            end
        "#,
        )
        .unwrap();

        let mut plugin = LuaPlugin::from_file(&script_path).unwrap();
        let result = plugin.on_action("select", "item1");
        assert_eq!(
            result,
            Some(slate_plugin_sdk::WidgetAction::Notify(
                "Something happened!".to_string()
            ))
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
