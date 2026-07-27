//! Slate Plugin Host
//!
//! Manages WASM (Extism) and Lua (mlua) plugin runtimes.
//! Enforces the permissions model on host functions.

pub mod host_functions;
pub mod lua_host;
pub mod permissions;
pub mod wasm_host;

pub use lua_host::LuaPlugin;
pub use permissions::PermissionGuard;
pub use wasm_host::WasmPlugin;
