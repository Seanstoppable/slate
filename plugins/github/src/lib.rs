use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct PullRequest {
    number: u64,
    title: String,
    #[serde(default)]
    user: User,
    state: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    html_url: String,
}

#[derive(Deserialize, Default)]
struct User {
    #[serde(default)]
    login: String,
}

#[derive(Deserialize)]
struct Issue {
    number: u64,
    title: String,
    #[serde(default)]
    user: User,
    state: String,
    #[serde(default)]
    labels: Vec<Label>,
    #[serde(default)]
    html_url: String,
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

#[derive(Deserialize)]
struct Notification {
    #[serde(default)]
    id: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    subject: NotificationSubject,
    #[serde(default)]
    unread: bool,
}

#[derive(Deserialize, Default)]
struct NotificationSubject {
    #[serde(default)]
    title: String,
    #[serde(rename = "type", default)]
    subject_type: String,
}

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "GitHub",
        "description": "GitHub PRs, issues, and notifications",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    let token = config::get("token").ok().flatten().unwrap_or_default();
    let repos_json = config::get("repos").ok().flatten().unwrap_or_else(|| "[]".to_string());
    let view = config::get("view").ok().flatten().unwrap_or_else(|| "prs".to_string());

    if token.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "⚠️  No GitHub token configured.\nSet 'token' in widget config or GITHUB_TOKEN env var.",
            "scrollable": false,
            "wrap": true
        }).to_string());
    }

    let repos: Vec<String> = serde_json::from_str(&repos_json).unwrap_or_default();
    if repos.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "⚠️  No repos configured.\nAdd 'repos = [\"owner/repo\"]' to widget config.",
            "scrollable": false,
            "wrap": true
        }).to_string());
    }

    match view.as_str() {
        "prs" => fetch_pull_requests(&token, &repos),
        "issues" => fetch_issues(&token, &repos),
        "notifications" => fetch_notifications(&token),
        _ => fetch_pull_requests(&token, &repos),
    }
}

fn fetch_pull_requests(token: &str, repos: &[String]) -> FnResult<String> {
    let mut items = Vec::new();

    for repo in repos {
        let url = format!("https://api.github.com/repos/{}/pulls?state=open&per_page=10", repo);
        let req = HttpRequest::new(&url)
            .with_header("Authorization", &format!("Bearer {}", token))
            .with_header("Accept", "application/vnd.github.v3+json")
            .with_header("User-Agent", "slate-github-plugin");

        if let Ok(response) = http::request::<String>(&req, None) {
            let body = response.body();
            let body_str = std::str::from_utf8(&body).unwrap_or("[]");
            if let Ok(prs) = serde_json::from_str::<Vec<PullRequest>>(body_str) {
                for pr in prs {
                    let icon = if pr.draft { "📝" } else { "🟢" };
                    items.push(json!({
                        "id": format!("{}#{}", repo, pr.number),
                        "title": format!("{} #{} {}", icon, pr.number, pr.title),
                        "subtitle": format!("by {} in {}", pr.user.login, repo),
                        "style": {}
                    }));
                }
            }
        }
    }

    let content = json!({
        "type": "list",
        "items": items,
        "selectable": true,
        "actions": [
            {"id": "open", "label": "Open in browser", "key": "o", "confirm": false},
            {"id": "approve", "label": "Approve", "key": "a", "confirm": true},
            {"id": "merge", "label": "Merge", "key": "m", "confirm": true}
        ]
    });

    Ok(content.to_string())
}

fn fetch_issues(token: &str, repos: &[String]) -> FnResult<String> {
    let mut items = Vec::new();

    for repo in repos {
        let url = format!("https://api.github.com/repos/{}/issues?state=open&per_page=10", repo);
        let req = HttpRequest::new(&url)
            .with_header("Authorization", &format!("Bearer {}", token))
            .with_header("Accept", "application/vnd.github.v3+json")
            .with_header("User-Agent", "slate-github-plugin");

        if let Ok(response) = http::request::<String>(&req, None) {
            let body = response.body();
            let body_str = std::str::from_utf8(&body).unwrap_or("[]");
            if let Ok(issues) = serde_json::from_str::<Vec<Issue>>(body_str) {
                for issue in issues {
                    let labels: String = issue.labels.iter()
                        .map(|l| format!("[{}]", l.name))
                        .collect::<Vec<_>>()
                        .join(" ");
                    items.push(json!({
                        "id": format!("{}#{}", repo, issue.number),
                        "title": format!("#{} {}", issue.number, issue.title),
                        "subtitle": format!("by {} {} ", issue.user.login, labels),
                        "style": {}
                    }));
                }
            }
        }
    }

    let content = json!({
        "type": "list",
        "items": items,
        "selectable": true,
        "actions": [
            {"id": "open", "label": "Open in browser", "key": "o", "confirm": false},
            {"id": "close", "label": "Close issue", "key": "x", "confirm": true}
        ]
    });

    Ok(content.to_string())
}

fn fetch_notifications(token: &str) -> FnResult<String> {
    let url = "https://api.github.com/notifications?per_page=15";
    let req = HttpRequest::new(url)
        .with_header("Authorization", &format!("Bearer {}", token))
        .with_header("Accept", "application/vnd.github.v3+json")
        .with_header("User-Agent", "slate-github-plugin");

    let mut items = Vec::new();

    if let Ok(response) = http::request::<String>(&req, None) {
        let body = response.body();
        if let Ok(notifications) = serde_json::from_str::<Vec<Notification>>(std::str::from_utf8(&body).unwrap_or("[]")) {
            for notif in notifications {
                let icon = match notif.subject.subject_type.as_str() {
                    "PullRequest" => "🔀",
                    "Issue" => "🐛",
                    "Release" => "🏷️",
                    _ => "📬",
                };
                let unread_marker = if notif.unread { "●" } else { "○" };
                items.push(json!({
                    "id": notif.id,
                    "title": format!("{} {} {}", unread_marker, icon, notif.subject.title),
                    "subtitle": format!("{}", notif.reason),
                    "style": {}
                }));
            }
        }
    }

    let content = json!({
        "type": "list",
        "items": items,
        "selectable": true,
        "actions": [
            {"id": "open", "label": "Open", "key": "o", "confirm": false},
            {"id": "mark_read", "label": "Mark read", "key": "r", "confirm": false}
        ]
    });

    Ok(content.to_string())
}

#[plugin_fn]
pub fn on_key(input: String) -> FnResult<String> {
    // Handle view switching: 1=PRs, 2=Issues, 3=Notifications
    #[derive(Deserialize)]
    struct KeyInput {
        #[serde(default)]
        key: String,
    }

    if let Ok(ki) = serde_json::from_str::<KeyInput>(&input) {
        match ki.key.as_str() {
            "1" => {
                let _ = var::set("view", "prs");
            }
            "2" => {
                let _ = var::set("view", "issues");
            }
            "3" => {
                let _ = var::set("view", "notifications");
            }
            _ => {}
        }
    }
    Ok(String::new())
}

#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    #[derive(Deserialize)]
    struct ActionInput {
        action_id: String,
        item_id: String,
    }

    if let Ok(action) = serde_json::from_str::<ActionInput>(&input) {
        match action.action_id.as_str() {
            "open" => {
                // Parse "owner/repo#number" to construct URL
                if let Some((repo, num)) = action.item_id.rsplit_once('#') {
                    let url = format!("https://github.com/{}/pull/{}", repo, num);
                    return Ok(json!({"open_url": url}).to_string());
                }
            }
            "approve" | "merge" | "close" | "mark_read" => {
                // These would call GitHub API mutations via host functions
                // For PoC, just acknowledge
                return Ok(json!({"status": "acknowledged", "action": action.action_id}).to_string());
            }
            _ => {}
        }
    }
    Ok(String::new())
}
