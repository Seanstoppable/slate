#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
#[cfg(target_arch = "wasm32")]
use serde::Deserialize;
#[cfg(target_arch = "wasm32")]
use serde_json::json;

pub struct DiskEntry {
    pub label: String,
    pub display: String,
    pub color: &'static str,
}

/// Make a 10-char usage bar: "#" * filled + "-" * (10-filled)
pub fn make_bar(pct: u8) -> String {
    let filled = (pct / 10) as usize;
    "#".repeat(filled) + &"-".repeat(10 - filled)
}

/// Color based on percent: >=90 red, >=70 yellow, else green
pub fn bar_color(pct: u8) -> &'static str {
    if pct >= 90 {
        "red"
    } else if pct >= 70 {
        "yellow"
    } else {
        "green"
    }
}

/// Parse a single line of `df -h` output (not the header line).
/// Returns None for lines that should be skipped.
/// Line format: filesystem size used avail pct% mount
pub fn parse_df_line(line: &str) -> Option<DiskEntry> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }
    let pct_str = parts[4];
    let mount = parts[5];
    if mount.starts_with("/private/var/") || mount.starts_with("/snap/") {
        return None;
    }
    let pct: u8 = pct_str.trim_end_matches('%').parse().ok()?;
    let used = parts[2];
    let size = parts[1];
    let bar = make_bar(pct);
    let color = bar_color(pct);
    Some(DiskEntry {
        label: mount.to_string(),
        display: format!("{}  {} / {}", bar, used, size),
        color,
    })
}

/// Parse a single line of `wmic logicaldisk get name,size,freespace /format:csv` output.
/// Line format: Node,FreeSpace,Name,Size
pub fn parse_wmic_line(line: &str) -> Option<DiskEntry> {
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 4 {
        return None;
    }
    let free: u64 = parts[1].trim().parse().ok()?;
    let name = parts[2].trim();
    let total: u64 = parts[3].trim().parse().ok()?;
    if total == 0 || name.is_empty() {
        return None;
    }
    let pct = ((total - free) * 100 / total) as u8;
    let bar = make_bar(pct);
    let color = bar_color(pct);
    Some(DiskEntry {
        label: name.to_string(),
        display: format!("{}  {}%", bar, pct),
        color,
    })
}

/// Parse a single line of `powershell Get-PSDrive` output for fixed drives.
/// Line format (after header): Name  Used(GB)  Free(GB)  Provider  Root  ...
/// We use the -csv formatted output instead: Name,Used,Free
pub fn parse_psdrive_line(line: &str) -> Option<DiskEntry> {
    let parts: Vec<&str> = line.splitn(3, ',').collect();
    if parts.len() < 3 {
        return None;
    }
    let name = parts[0].trim().trim_matches('"');
    let used: f64 = parts[1].trim().trim_matches('"').parse().ok()?;
    let free: f64 = parts[2].trim().trim_matches('"').parse().ok()?;
    let total = used + free;
    if total == 0.0 || name.is_empty() {
        return None;
    }
    let pct = ((used / total) * 100.0) as u8;
    let bar = make_bar(pct);
    let color = bar_color(pct);
    Some(DiskEntry {
        label: format!("{name}:\\"),
        display: format!(
            "{}  {:.1}GB / {:.1}GB",
            bar,
            used,
            total
        ),
        color,
    })
}

pub fn entries_to_key_value_json(entries: &[DiskEntry]) -> serde_json::Value {
    let pairs: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| serde_json::json!([e.label, {"text": e.display, "color": e.color}]))
        .collect();
    serde_json::json!({"type": "key_value", "pairs": pairs})
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct ExecResult {
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    #[serde(default)]
    exit_code: i32,
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Disk Usage",
        "description": "Shows disk space usage for mounted filesystems",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    let mut entries: Vec<DiskEntry> = Vec::new();

    let df_result = run_exec("df", &["-h"]);
    if let Ok(result) = df_result {
        if result.exit_code == 0 && !result.stdout.is_empty() {
            entries = result
                .stdout
                .lines()
                .skip(1)
                .filter_map(parse_df_line)
                .collect();
        }
    }

    if entries.is_empty() {
        // Windows: use PowerShell Get-PSDrive (wmic was removed in Windows 11)
        if let Ok(result) = run_exec(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-PSDrive -PSProvider FileSystem | Select-Object -Property Name,@{N='Used';E={[math]::Round($_.Used/1GB,2)}},@{N='Free';E={[math]::Round($_.Free/1GB,2)}} | ConvertTo-Csv -NoTypeInformation | Select-Object -Skip 1",
            ],
        ) {
            if result.exit_code == 0 {
                entries = result
                    .stdout
                    .lines()
                    .filter_map(parse_psdrive_line)
                    .collect();
            }
        }
    }

    if entries.is_empty() {
        let content = json!({
            "type": "text",
            "content": "No disk info available",
            "scrollable": false,
            "wrap": true
        });
        return Ok(content.to_string());
    }

    Ok(entries_to_key_value_json(&entries).to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[host_fn]
extern "ExtismHost" {
    fn exec_command(input: String) -> String;
}

#[cfg(target_arch = "wasm32")]
fn run_exec(cmd: &str, args: &[&str]) -> Result<ExecResult, Error> {
    let request = json!({"cmd": cmd, "args": args}).to_string();
    let output = unsafe { exec_command(request)? };
    serde_json::from_str(&output).map_err(|e| Error::msg(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_bar_empty() {
        assert_eq!(make_bar(0), "----------");
    }

    #[test]
    fn test_make_bar_half() {
        assert_eq!(make_bar(50), "#####-----");
    }

    #[test]
    fn test_make_bar_full() {
        assert_eq!(make_bar(100), "##########");
    }

    #[test]
    fn test_bar_color() {
        assert_eq!(bar_color(69), "green");
        assert_eq!(bar_color(70), "yellow");
        assert_eq!(bar_color(89), "yellow");
        assert_eq!(bar_color(90), "red");
        assert_eq!(bar_color(100), "red");
    }

    #[test]
    fn test_parse_df_line_valid() {
        let line = "/dev/sda1        100G   45G   55G  45% /";
        let entry = parse_df_line(line).unwrap();
        assert_eq!(entry.label, "/");
        assert_eq!(entry.display, "####------  45G / 100G");
        assert_eq!(entry.color, "green");
    }

    #[test]
    fn test_parse_df_line_skips_private_var() {
        let line = "map auto_home    100G    0B  100G   0% /private/var/folders/abc";
        assert!(parse_df_line(line).is_none());
    }

    #[test]
    fn test_parse_df_line_skips_snap() {
        let line = "/dev/loop0       100M  100M     0  100% /snap/core/12345";
        assert!(parse_df_line(line).is_none());
    }

    #[test]
    fn test_parse_df_line_too_few_fields() {
        let line = "/dev/sda1 100G 45G";
        assert!(parse_df_line(line).is_none());
    }

    #[test]
    fn test_parse_wmic_line_valid() {
        // Node,FreeSpace,Name,Size  -> used = (100-40)/100 = 60%
        let line = "MYPC,40000000000,C:,100000000000";
        let entry = parse_wmic_line(line).unwrap();
        assert_eq!(entry.label, "C:");
        assert_eq!(entry.display, "######----  60%");
        assert_eq!(entry.color, "green");
    }

    #[test]
    fn test_parse_wmic_line_zero_total() {
        let line = "MYPC,0,D:,0";
        assert!(parse_wmic_line(line).is_none());
    }

    #[test]
    fn test_entries_to_key_value_json() {
        let entries = vec![DiskEntry {
            label: "/".to_string(),
            display: "#####-----  45G / 100G".to_string(),
            color: "green",
        }];
        let json = entries_to_key_value_json(&entries);
        assert_eq!(json["type"], "key_value");
        let pairs = json["pairs"].as_array().unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0][0], "/");
        assert_eq!(pairs[0][1]["text"], "#####-----  45G / 100G");
        assert_eq!(pairs[0][1]["color"], "green");
    }
}
