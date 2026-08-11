#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
#[cfg(target_arch = "wasm32")]
use serde::Deserialize;
#[cfg(target_arch = "wasm32")]
use serde_json::json;

pub struct CommitItem {
    pub hash: String,
    pub subject: String,
    pub time_ago: String,
    pub author: String,
}

/// Parse a single line of git log output in format: hash|subject|time_ago|author
pub fn parse_commit_line(line: &str) -> Option<CommitItem> {
    let mut parts = line.splitn(4, '|');
    let hash = parts.next()?.trim().to_string();
    let subject = parts.next()?.trim().to_string();
    let time_ago = parts.next()?.trim().to_string();
    let author = parts.next()?.trim().to_string();
    if hash.is_empty() {
        return None;
    }
    Some(CommitItem {
        hash,
        subject,
        time_ago,
        author,
    })
}

pub fn commits_to_list_json(items: &[CommitItem]) -> serde_json::Value {
    let json_items: Vec<serde_json::Value> = items
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.hash,
                "title": c.subject,
                "subtitle": format!("{} \u{2022} {} \u{2022} {}", c.hash, c.author, c.time_ago),
                "style": {}
            })
        })
        .collect();
    serde_json::json!({
        "type": "list",
        "items": json_items,
        "selectable": true,
        "actions": [
            {"id": "detail", "label": "Show details", "key": "d", "confirm": false}
        ]
    })
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
        "name": "Git Recent",
        "description": "Shows recent git commits in a repository",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let config: serde_json::Value = serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
    let count = config
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(8);
    let path = config
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".")
        .to_string();

    let count_str = count.to_string();
    let git_dir = format!("{path}/.git");
    let result = run_exec(
        "git",
        &[
            "--git-dir",
            &git_dir,
            "--work-tree",
            &path,
            "log",
            "--oneline",
            "--no-decorate",
            "-n",
            &count_str,
            "--format=%h|%s|%ar|%an",
        ],
    )?;

    if result.exit_code != 0 {
        let message = if result.stderr.contains("not a git repository") {
            "Not a git repository".to_string()
        } else if result.stderr.is_empty() {
            "git not available".to_string()
        } else {
            format!("git error: {}", result.stderr.trim())
        };
        let content = json!({
            "type": "text",
            "content": message,
            "scrollable": false,
            "wrap": true
        });
        return Ok(content.to_string());
    }

    let commits: Vec<CommitItem> = result
        .stdout
        .lines()
        .filter_map(|line| parse_commit_line(line.trim()))
        .collect();

    if commits.is_empty() {
        let content = json!({
            "type": "text",
            "content": "No commits found",
            "scrollable": false,
            "wrap": true
        });
        return Ok(content.to_string());
    }

    Ok(commits_to_list_json(&commits).to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    #[derive(Deserialize)]
    struct ActionInput {
        action_id: String,
        item_id: String,
    }

    if let Ok(action) = serde_json::from_str::<ActionInput>(&input) {
        if action.action_id == "detail" {
            let hash = &action.item_id;
            if let Ok(result) = run_exec(
                "git",
                &[
                    "-C",
                    ".",
                    "show",
                    "--stat",
                    "--format=commit %H%nAuthor: %an <%ae>%nDate:   %ad%n%n%s%n%n%b",
                    hash,
                ],
            ) {
                let content = json!({
                    "type": "action",
                    "action": "show_detail",
                    "content": result.stdout
                });
                return Ok(content.to_string());
            }
        }
    }

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
    fn test_parse_commit_line_valid() {
        let item = parse_commit_line("abc1234|Fix the bug|2 hours ago|Alice Smith").unwrap();
        assert_eq!(item.hash, "abc1234");
        assert_eq!(item.subject, "Fix the bug");
        assert_eq!(item.time_ago, "2 hours ago");
        assert_eq!(item.author, "Alice Smith");
    }

    #[test]
    fn test_parse_commit_line_missing_parts() {
        assert!(parse_commit_line("abc1234|Fix the bug").is_none());
        assert!(parse_commit_line("abc1234").is_none());
        assert!(parse_commit_line("").is_none());
    }

    #[test]
    fn test_parse_commit_line_empty_hash() {
        assert!(parse_commit_line("|Fix the bug|2 hours ago|Alice").is_none());
    }

    #[test]
    fn test_commits_to_list_json() {
        let items = vec![CommitItem {
            hash: "abc1234".to_string(),
            subject: "Fix the bug".to_string(),
            time_ago: "2 hours ago".to_string(),
            author: "Alice Smith".to_string(),
        }];
        let json = commits_to_list_json(&items);
        assert_eq!(json["type"], "list");
        assert_eq!(json["selectable"], true);
        let list_items = json["items"].as_array().unwrap();
        assert_eq!(list_items.len(), 1);
        assert_eq!(list_items[0]["id"], "abc1234");
        assert_eq!(list_items[0]["title"], "Fix the bug");
        assert_eq!(
            list_items[0]["subtitle"],
            "abc1234 \u{2022} Alice Smith \u{2022} 2 hours ago"
        );
    }
}
