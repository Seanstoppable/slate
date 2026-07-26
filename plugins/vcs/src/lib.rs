use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

const MAX_COMMITS: usize = 5;

#[derive(Deserialize, Default)]
struct RefreshInput {
    #[serde(default)]
    engine: String,
    #[serde(default)]
    repo_path: String,
    #[serde(default)]
    branch: String,
    #[serde(default)]
    status: Vec<StatusEntry>,
    #[serde(default)]
    log: Vec<CommitEntry>,
}

#[derive(Deserialize)]
struct StatusEntry {
    #[serde(default)]
    file: String,
    #[serde(default)]
    state: String,
}

#[derive(Deserialize)]
struct CommitEntry {
    #[serde(default)]
    hash: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    date: String,
}

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "VCS",
        "description": "Version control status and recent commits",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: RefreshInput = serde_json::from_str(&input).unwrap_or_default();

    if settings.repo_path.trim().is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "Configure repo_path in settings",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let mut modified = 0usize;
    let mut added = 0usize;
    let mut deleted = 0usize;
    let mut untracked = 0usize;

    for entry in &settings.status {
        let _ = &entry.file;
        match entry.state.as_str() {
            "modified" => modified += 1,
            "added" => added += 1,
            "deleted" => deleted += 1,
            "untracked" => untracked += 1,
            _ => {}
        }
    }

    let total_changed = settings.status.len();
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

    let mut pairs = vec![
        json!({"key": "Engine", "value": display_or_default(&settings.engine, "unknown")}),
        json!({"key": "Branch", "value": display_or_default(&settings.branch, "(detached)")}),
        json!({"key": "Changed files", "value": total_changed.to_string()}),
        json!({"key": "Status", "value": status_summary}),
    ];

    if settings.log.is_empty() {
        pairs.push(json!({"key": "Last commit", "value": "No commits available"}));
    } else {
        for (index, commit) in settings.log.iter().take(MAX_COMMITS).enumerate() {
            let key = if index == 0 {
                "Last commit".to_string()
            } else {
                format!("Recent {}", index + 1)
            };
            let value = format_commit(commit);
            pairs.push(json!({"key": key, "value": value}));
        }
    }

    Ok(json!({
        "type": "key_value",
        "pairs": pairs
    })
    .to_string())
}

fn display_or_default(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn format_commit(commit: &CommitEntry) -> String {
    let hash = display_or_default(&commit.hash, "unknown");
    let message = display_or_default(&commit.message, "No message");
    let mut extra = Vec::new();

    if !commit.author.trim().is_empty() {
        extra.push(commit.author.trim().to_string());
    }
    if !commit.date.trim().is_empty() {
        extra.push(commit.date.trim().to_string());
    }

    if extra.is_empty() {
        format!("{hash} {message}")
    } else {
        format!("{hash} {message} ({})", extra.join(" • "))
    }
}

#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}
