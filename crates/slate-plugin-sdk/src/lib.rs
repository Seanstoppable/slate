//! Slate Plugin SDK
//!
//! Defines the Widget trait and content types that all plugins implement,
//! whether they are built-in (native Rust), WASM, or Lua.

mod content;
mod metadata;
mod widget;

pub use content::*;
pub use metadata::*;
pub use widget::*;
