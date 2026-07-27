//! Slate Core
//!
//! TUI engine (ratatui), grid layout, focus management, config parsing,
//! and the main application loop.

pub mod app;
pub mod config;
pub mod layout;
pub mod notifications;
pub mod render;

pub use app::App;
pub use config::SlateConfig;
pub use notifications::UpdateNotifications;
