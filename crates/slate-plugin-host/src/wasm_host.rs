use anyhow::{Context, Result};
use slate_plugin_sdk::{
    Permissions, WidgetConfig, WidgetContent, WidgetMetadata,
};
use std::path::Path;

use crate::permissions::PermissionGuard;

/// A WASM plugin loaded via Extism.
pub struct WasmPlugin {
    metadata: WidgetMetadata,
    permissions: PermissionGuard,
    // In a full implementation, this would hold an extism::Plugin instance.
    // For now we store the WASM bytes and create the plugin on demand.
    wasm_bytes: Vec<u8>,
    config: Option<WidgetConfig>,
}

impl WasmPlugin {
    /// Load a WASM plugin from a file path.
    pub fn from_file(path: &Path, permissions: Permissions) -> Result<Self> {
        let wasm_bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read WASM file: {}", path.display()))?;

        // Extract metadata from the WASM module's exported function
        // For now, use filename-based metadata
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Self {
            metadata: WidgetMetadata {
                name,
                description: String::new(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            },
            permissions: PermissionGuard::new(permissions),
            wasm_bytes,
            config: None,
        })
    }

    /// Load a WASM plugin from raw bytes.
    pub fn from_bytes(
        bytes: Vec<u8>,
        metadata: WidgetMetadata,
        permissions: Permissions,
    ) -> Self {
        Self {
            metadata,
            permissions: PermissionGuard::new(permissions),
            wasm_bytes: bytes,
            config: None,
        }
    }
}

impl slate_plugin_sdk::Widget for WasmPlugin {
    fn metadata(&self) -> WidgetMetadata {
        self.metadata.clone()
    }

    fn init(&mut self, config: WidgetConfig) {
        self.config = Some(config);
    }

    fn refresh(&mut self) -> WidgetContent {
        // In full implementation, this calls the WASM module's refresh export.
        // The WASM module communicates via JSON serialization through Extism's
        // input/output mechanism and can call host functions for HTTP, storage, etc.
        WidgetContent::Text {
            content: format!("[WASM] {} - loaded ({} bytes)", self.metadata.name, self.wasm_bytes.len()),
            scrollable: false,
            wrap: true,
        }
    }

    fn on_key(&mut self, key: &str, action: &str) {
        // Forward to WASM module's on_key export
        let _ = (key, action);
    }

    fn on_action(&mut self, action_id: &str, item_id: &str) {
        // Forward to WASM module's on_action export
        let _ = (action_id, item_id);
    }

    fn on_focus(&mut self) {}
    fn on_blur(&mut self) {}
}
