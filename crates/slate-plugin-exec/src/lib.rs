use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ExecResult {
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub exit_code: i32,
}

#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

#[cfg(target_arch = "wasm32")]
#[host_fn]
extern "ExtismHost" {
    fn exec_command(input: String) -> String;
}

#[cfg(target_arch = "wasm32")]
pub fn run_exec(cmd: &str, args: &[&str]) -> Result<ExecResult, Error> {
    let request = serde_json::json!({"cmd": cmd, "args": args}).to_string();
    let output = unsafe { exec_command(request)? };
    serde_json::from_str(&output).map_err(|e| Error::msg(e.to_string()))
}
