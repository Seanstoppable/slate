#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
struct ExecResult {
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    #[serde(default)]
    exit_code: i32,
}

#[cfg(target_arch = "wasm32")]
#[host_fn]
extern "ExtismHost" {
    fn exec_command(input: String) -> String;
}

#[cfg(target_arch = "wasm32")]
fn run_exec(cmd: &str, args: &[&str]) -> Result<ExecResult, Error> {
    let request = json!({ "cmd": cmd, "args": args }).to_string();
    let output = unsafe { exec_command(request)? };
    serde_json::from_str(&output).map_err(|e| Error::msg(e.to_string()))
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "VCS",
        "description": "Version control status (git/hg)",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    let engine = settings["engine"].as_str().unwrap_or("git");
    let repo_path = settings["repo_path"].as_str().unwrap_or(".");

    let (branch, status_entries, log_entries) = match engine {
        "hg" => get_hg_info(repo_path),
        _ => get_git_info(repo_path),
    };

    let content = build_vcs_content(engine, &branch, &status_entries, &log_entries);
    Ok(content.to_string())
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

// --- Pure logic (testable on native) ---

fn build_vcs_content(
    engine: &str,
    branch: &str,
    status_entries: &[(String, String)],
    log_entries: &[(String, String, String, String)],
) -> serde_json::Value {
    let mut modified = 0usize;
    let mut added = 0usize;
    let mut deleted = 0usize;
    let mut untracked = 0usize;

    for (state, _) in status_entries {
        match state.as_str() {
            "modified" => modified += 1,
            "added" => added += 1,
            "deleted" => deleted += 1,
            "untracked" => untracked += 1,
            _ => {}
        }
    }

    let mut summary_parts = Vec::new();
    if modified > 0 {
        summary_parts.push(format!("{modified} modified"));
    }
    if added > 0 {
        summary_parts.push(format!("{added} added"));
    }
    if deleted > 0 {
        summary_parts.push(format!("{deleted} deleted"));
    }
    if untracked > 0 {
        summary_parts.push(format!("{untracked} untracked"));
    }

    let status_summary = if summary_parts.is_empty() {
        "clean".to_string()
    } else {
        summary_parts.join(", ")
    };

    let branch_display = if branch.is_empty() {
        "(detached)"
    } else {
        branch
    };

    let mut pairs = vec![
        json!({"key": "Engine", "value": engine}),
        json!({"key": "Branch", "value": branch_display}),
        json!({"key": "Status", "value": status_summary}),
    ];

    for (i, (hash, message, author, date)) in log_entries.iter().take(5).enumerate() {
        let key = if i == 0 {
            "Last commit".to_string()
        } else {
            format!("Recent {}", i + 1)
        };
        let mut val = format!("{} {}", hash, message);
        if !author.is_empty() || !date.is_empty() {
            let extra: Vec<&str> = [author.as_str(), date.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect();
            val.push_str(&format!(" ({})", extra.join(" • ")));
        }
        pairs.push(json!({"key": key, "value": val}));
    }

    if log_entries.is_empty() {
        pairs.push(json!({"key": "Last commit", "value": "No commits available"}));
    }

    json!({
        "type": "key_value",
        "pairs": pairs
    })
}

fn parse_git_status_output(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter(|l| l.len() >= 3)
        .map(|line| {
            let state = match &line[..2] {
                " M" | "M " | "MM" => "modified",
                "A " | "AM" => "added",
                " D" | "D " => "deleted",
                "??" => "untracked",
                _ => "other",
            };
            (state.to_string(), line[3..].to_string())
        })
        .collect()
}

fn parse_commit_log_output(text: &str) -> Vec<(String, String, String, String)> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            (
                parts.first().unwrap_or(&"").to_string(),
                parts.get(1).unwrap_or(&"").to_string(),
                parts.get(2).unwrap_or(&"").to_string(),
                parts.get(3).unwrap_or(&"").to_string(),
            )
        })
        .collect()
}

fn parse_hg_status_output(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter(|l| l.len() >= 2)
        .map(|line| {
            let state = match line.chars().next().unwrap_or(' ') {
                'M' => "modified",
                'A' => "added",
                'R' => "deleted",
                '?' => "untracked",
                _ => "other",
            };
            (state.to_string(), line.get(2..).unwrap_or("").to_string())
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn get_git_info(repo_path: &str) -> (String, Vec<(String, String)>, Vec<(String, String, String, String)>) {
    let branch = run_exec("git", &["-C", repo_path, "rev-parse", "--abbrev-ref", "HEAD"])
        .map(|r| r.stdout.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let status = run_exec("git", &["-C", repo_path, "status", "--porcelain"])
        .map(|r| parse_git_status_output(&r.stdout))
        .unwrap_or_default();

    let log = run_exec("git", &["-C", repo_path, "log", "--oneline", "-10", "--format=%h|%s|%an|%ar"])
        .map(|r| parse_commit_log_output(&r.stdout))
        .unwrap_or_default();

    (branch, status, log)
}

#[cfg(target_arch = "wasm32")]
fn get_hg_info(repo_path: &str) -> (String, Vec<(String, String)>, Vec<(String, String, String, String)>) {
    let branch = run_exec("hg", &["branch", "-R", repo_path])
        .map(|r| r.stdout.trim().to_string())
        .unwrap_or_else(|_| "default".to_string());

    let status = run_exec("hg", &["status", "-R", repo_path])
        .map(|r| parse_hg_status_output(&r.stdout))
        .unwrap_or_default();

    let log = run_exec("hg", &["log", "-R", repo_path, "-l", "10", "--template", "{short(node)}|{desc|firstline}|{author|user}|{date|age}\n"])
        .map(|r| parse_commit_log_output(&r.stdout))
        .unwrap_or_default();

    (branch, status, log)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_status() {
        let output = " M src/main.rs\nA  new_file.rs\n?? untracked.txt\n D deleted.rs\n";
        let entries = parse_git_status_output(output);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0], ("modified".to_string(), "src/main.rs".to_string()));
        assert_eq!(entries[1], ("added".to_string(), "new_file.rs".to_string()));
        assert_eq!(entries[2], ("untracked".to_string(), "untracked.txt".to_string()));
        assert_eq!(entries[3], ("deleted".to_string(), "deleted.rs".to_string()));
    }

    #[test]
    fn test_parse_git_status_empty() {
        let entries = parse_git_status_output("");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_hg_status() {
        let output = "M modified.py\nA added.py\nR removed.py\n? untracked.py\n";
        let entries = parse_hg_status_output(output);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].0, "modified");
        assert_eq!(entries[1].0, "added");
        assert_eq!(entries[2].0, "deleted");
        assert_eq!(entries[3].0, "untracked");
    }

    #[test]
    fn test_parse_commit_log() {
        let output = "abc1234|Fix bug|Alice|2 hours ago\ndef5678|Add feature|Bob|1 day ago\n";
        let entries = parse_commit_log_output(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], ("abc1234".to_string(), "Fix bug".to_string(), "Alice".to_string(), "2 hours ago".to_string()));
        assert_eq!(entries[1].2, "Bob");
    }

    #[test]
    fn test_parse_commit_log_missing_fields() {
        let output = "abc1234|Fix bug\n";
        let entries = parse_commit_log_output(output);
        assert_eq!(entries[0].0, "abc1234");
        assert_eq!(entries[0].1, "Fix bug");
        assert_eq!(entries[0].2, "");
        assert_eq!(entries[0].3, "");
    }

    #[test]
    fn test_build_vcs_content_clean() {
        let content = build_vcs_content("git", "main", &[], &[]);
        let pairs = content["pairs"].as_array().unwrap();
        assert_eq!(pairs[2]["value"], "clean");
    }

    #[test]
    fn test_build_vcs_content_dirty() {
        let status = vec![
            ("modified".to_string(), "a.rs".to_string()),
            ("modified".to_string(), "b.rs".to_string()),
            ("untracked".to_string(), "c.rs".to_string()),
        ];
        let content = build_vcs_content("git", "feature", &status, &[]);
        let pairs = content["pairs"].as_array().unwrap();
        assert_eq!(pairs[2]["value"], "2 modified, 1 untracked");
    }

    #[test]
    fn test_build_vcs_content_with_log() {
        let log = vec![
            ("abc1234".to_string(), "Fix bug".to_string(), "Alice".to_string(), "2h ago".to_string()),
        ];
        let content = build_vcs_content("git", "main", &[], &log);
        let pairs = content["pairs"].as_array().unwrap();
        assert_eq!(pairs[3]["key"], "Last commit");
        assert!(pairs[3]["value"].as_str().unwrap().contains("abc1234"));
        assert!(pairs[3]["value"].as_str().unwrap().contains("Alice"));
    }

    #[test]
    fn test_build_vcs_content_detached() {
        let content = build_vcs_content("git", "", &[], &[]);
        let pairs = content["pairs"].as_array().unwrap();
        assert_eq!(pairs[1]["value"], "(detached)");
    }

    #[test]
    fn test_build_vcs_content_hg_engine() {
        let content = build_vcs_content("hg", "default", &[], &[]);
        let pairs = content["pairs"].as_array().unwrap();
        assert_eq!(pairs[0]["value"], "hg");
    }
}
