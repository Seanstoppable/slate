use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

const MAX_STORIES: usize = 10;

#[derive(Deserialize)]
struct Story {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    score: u32,
    #[serde(default)]
    by: String,
    #[serde(default)]
    descendants: u32,
}

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Hacker News",
        "description": "Top stories from Hacker News",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    // Fetch top story IDs
    let req = HttpRequest::new("https://hacker-news.firebaseio.com/v0/topstories.json");
    let response = http::request::<String>(&req, None)?;
    let resp_body = response.body();
    let body_str = std::str::from_utf8(&resp_body).unwrap_or("[]");
    let ids: Vec<u64> = serde_json::from_str(body_str).unwrap_or_default();

    let mut items = Vec::new();

    // Fetch details for top N stories
    for &id in ids.iter().take(MAX_STORIES) {
        let story_url = format!(
            "https://hacker-news.firebaseio.com/v0/item/{}.json",
            id
        );
        let req = HttpRequest::new(&story_url);
        if let Ok(resp) = http::request::<String>(&req, None) {
            let body = resp.body();
            let resp_str = std::str::from_utf8(&body).unwrap_or("{}");
            if let Ok(story) = serde_json::from_str::<Story>(resp_str) {
                items.push(json!({
                    "id": story.id.to_string(),
                    "title": format!("▲{} {}", story.score, story.title),
                    "subtitle": format!("by {} | {} comments", story.by, story.descendants),
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
            {"id": "open", "label": "Open in browser", "key": "o", "confirm": false},
            {"id": "comments", "label": "View comments", "key": "c", "confirm": false}
        ]
    });

    Ok(content.to_string())
}

#[plugin_fn]
pub fn on_key(input: String) -> FnResult<String> {
    // Input is JSON: {"key": "...", "action": "..."}
    Ok(String::new())
}

#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    // Input is JSON: {"action_id": "open", "item_id": "12345"}
    #[derive(Deserialize)]
    struct ActionInput {
        action_id: String,
        item_id: String,
    }

    if let Ok(action) = serde_json::from_str::<ActionInput>(&input) {
        match action.action_id.as_str() {
            "open" => {
                let url = format!("https://news.ycombinator.com/item?id={}", action.item_id);
                // Request host to open URL
                let result = json!({"open_url": url});
                return Ok(result.to_string());
            }
            "comments" => {
                let url = format!("https://news.ycombinator.com/item?id={}", action.item_id);
                let result = json!({"open_url": url});
                return Ok(result.to_string());
            }
            _ => {}
        }
    }
    Ok(String::new())
}
