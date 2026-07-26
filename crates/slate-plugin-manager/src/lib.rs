//! Slate Plugin Manager
//!
//! Handles plugin installation from GitHub, version resolution,
//! lockfile management, update checking, and registry search.

pub mod install;
pub mod lockfile;
pub mod registry;
pub mod update;

pub use install::PluginInstaller;
pub use lockfile::Lockfile;
pub use registry::Registry;
