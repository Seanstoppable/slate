//! Slate Core
//!
//! TUI engine (ratatui), grid layout, focus management, config parsing,
//! and the main application loop.

pub mod config;
pub mod layout;
pub mod render;
pub mod app;

pub use app::App;
pub use config::SlateConfig;
