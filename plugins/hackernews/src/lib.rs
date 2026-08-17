#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    // Fetch top story IDs
    let ids: Vec<u64> =
        slate_plugin_http::get_json("https://hacker-news.firebaseio.com/v0/topstories.json", &[])?;

    let mut items = Vec::new();

    // Fetch details for top N stories
    for &id in ids.iter().take(MAX_STORIES) {
        let story_url = format!(
            "https://hacker-news.firebaseio.com/v0/item/{}.json",
            id
        );
        if let Ok(story) = slate_plugin_http::get_json::<Story>(&story_url, &[]) {
            items.push(json!({
                "id": story.id.to_string(),
                "title": format!("▲{} {}", story.score, story.title),
                "subtitle": format!("by {} | {} comments", story.by, story.descendants),
                "style": {}
            }));
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

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(input: String) -> FnResult<String> {
    // Input is JSON: {"key": "...", "action": "..."}
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    // Input is JSON: {"action_id": "open", "item_id": "12345"}
    #[derive(Deserialize)]
    struct ActionInput {
        action_id: String,
        item_id: String,
    }

    if let Ok(action) = serde_json::from_str::<ActionInput>(&input) {
        if let Some(url) = build_hn_url(&action.action_id, &action.item_id) {
            return Ok(json!({"open_url": url}).to_string());
        }
    }
    Ok(String::new())
}

fn build_hn_url(action_id: &str, item_id: &str) -> Option<String> {
    match action_id {
        "open" | "select" | "comments" => {
            Some(format!("https://news.ycombinator.com/item?id={}", item_id))
        }
        _ => None,
    }
}

fn format_story_title(score: u32, title: &str) -> String {
    format!("▲{} {}", score, title)
}

fn format_story_subtitle(by: &str, descendants: u32) -> String {
    format!("by {} | {} comments", by, descendants)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_hn_url_open() {
        assert_eq!(
            build_hn_url("open", "12345"),
            Some("https://news.ycombinator.com/item?id=12345".to_string())
        );
    }

    #[test]
    fn test_build_hn_url_select() {
        assert_eq!(
            build_hn_url("select", "99999"),
            Some("https://news.ycombinator.com/item?id=99999".to_string())
        );
    }

    #[test]
    fn test_build_hn_url_comments() {
        assert_eq!(
            build_hn_url("comments", "42"),
            Some("https://news.ycombinator.com/item?id=42".to_string())
        );
    }

    #[test]
    fn test_build_hn_url_unknown_action() {
        assert_eq!(build_hn_url("delete", "123"), None);
    }

    #[test]
    fn test_format_story_title() {
        assert_eq!(format_story_title(150, "Rust is great"), "▲150 Rust is great");
        assert_eq!(format_story_title(0, "New post"), "▲0 New post");
    }

    #[test]
    fn test_format_story_subtitle() {
        assert_eq!(format_story_subtitle("dang", 42), "by dang | 42 comments");
        assert_eq!(format_story_subtitle("user", 0), "by user | 0 comments");
    }

    #[test]
    fn test_story_deserialization() {
        let json = r#"{"id":123,"title":"Test","url":"https://t.co","score":10,"by":"user","descendants":5}"#;
        let story: Story = serde_json::from_str(json).unwrap();
        assert_eq!(story.id, 123);
        assert_eq!(story.title, "Test");
        assert_eq!(story.score, 10);
        assert_eq!(story.by, "user");
    }

    #[test]
    fn test_story_deserialization_defaults() {
        let json = r#"{"id":1}"#;
        let story: Story = serde_json::from_str(json).unwrap();
        assert_eq!(story.title, "");
        assert_eq!(story.score, 0);
        assert_eq!(story.by, "");
    }
}
