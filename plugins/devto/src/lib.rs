#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

use serde::Deserialize;
use serde_json::json;

/// A Dev.to article from the API response.
#[derive(Deserialize, Debug, Clone)]
struct Article {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    user: ArticleUser,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    positive_reactions_count: u32,
    #[serde(default)]
    comments_count: u32,
    #[serde(default)]
    reading_time_minutes: u32,
    #[serde(default)]
    tag_list: Vec<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct ArticleUser {
    #[serde(default)]
    username: String,
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Dev.to",
        "description": "Shows articles from dev.to",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let url = build_api_url(&settings);
    let headers = [
        ("Accept", "application/json"),
        ("User-Agent", "slate-devto/0.1.0"),
    ];

    let articles: Vec<Article> = slate_plugin_http::get_json(&url, &headers).unwrap_or_default();
    let limit = settings["numberOfArticles"]
        .as_u64()
        .unwrap_or(10) as usize;

    let content = build_article_list(&articles, limit);
    Ok(content.to_string())
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
        if action.action_id == "open" && !action.item_id.is_empty() {
            let result = json!({"open_url": action.item_id});
            return Ok(result.to_string());
        }
    }
    Ok(String::new())
}

// --- Pure logic (testable on native) ---

/// Build the Dev.to API URL from settings.
fn build_api_url(settings: &serde_json::Value) -> String {
    let mut url = "https://dev.to/api/articles?per_page=".to_string();
    let limit = settings["numberOfArticles"].as_u64().unwrap_or(10);
    url.push_str(&limit.to_string());

    if let Some(tag) = settings["contentTag"].as_str() {
        if !tag.is_empty() {
            url.push_str("&tag=");
            url.push_str(tag);
        }
    }

    if let Some(username) = settings["contentUsername"].as_str() {
        if !username.is_empty() {
            url.push_str("&username=");
            url.push_str(username);
        }
    }

    if let Some(state) = settings["contentState"].as_str() {
        if !state.is_empty() {
            url.push_str("&state=");
            url.push_str(state);
        }
    }

    if let Some(top) = settings["top"].as_u64() {
        url.push_str("&top=");
        url.push_str(&top.to_string());
    }

    url
}

/// Format an article subtitle with metadata.
fn format_subtitle(article: &Article) -> String {
    let mut parts = Vec::new();

    parts.push(format!("by {}", article.user.username));

    if article.reading_time_minutes > 0 {
        parts.push(format!("{}m read", article.reading_time_minutes));
    }

    if article.positive_reactions_count > 0 {
        parts.push(format!("❤️ {}", article.positive_reactions_count));
    }

    if article.comments_count > 0 {
        parts.push(format!("💬 {}", article.comments_count));
    }

    parts.join(" • ")
}

/// Build the widget content JSON from a list of articles.
fn build_article_list(articles: &[Article], limit: usize) -> serde_json::Value {
    let items: Vec<serde_json::Value> = articles
        .iter()
        .take(limit)
        .map(|article| {
            json!({
                "id": article.url,
                "title": article.title,
                "subtitle": format_subtitle(article),
            })
        })
        .collect();

    if items.is_empty() {
        json!({
            "type": "text",
            "content": "No articles found.",
            "scrollable": false,
            "wrap": true
        })
    } else {
        json!({
            "type": "list",
            "items": items,
            "selectable": true,
            "actions": [
                {"id": "open", "label": "Open in browser", "key": "o", "confirm": false}
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_api_url_defaults() {
        let settings = json!({});
        let url = build_api_url(&settings);
        assert_eq!(url, "https://dev.to/api/articles?per_page=10");
    }

    #[test]
    fn test_build_api_url_with_tag() {
        let settings = json!({"contentTag": "rust", "numberOfArticles": 5});
        let url = build_api_url(&settings);
        assert_eq!(url, "https://dev.to/api/articles?per_page=5&tag=rust");
    }

    #[test]
    fn test_build_api_url_with_username() {
        let settings = json!({"contentUsername": "alice"});
        let url = build_api_url(&settings);
        assert!(url.contains("&username=alice"));
    }

    #[test]
    fn test_build_api_url_with_state() {
        let settings = json!({"contentState": "rising"});
        let url = build_api_url(&settings);
        assert!(url.contains("&state=rising"));
    }

    #[test]
    fn test_build_api_url_with_top() {
        let settings = json!({"top": 7});
        let url = build_api_url(&settings);
        assert!(url.contains("&top=7"));
    }

    #[test]
    fn test_build_api_url_all_params() {
        let settings = json!({
            "contentTag": "webdev",
            "contentUsername": "bob",
            "contentState": "fresh",
            "numberOfArticles": 3,
            "top": 30
        });
        let url = build_api_url(&settings);
        assert!(url.starts_with("https://dev.to/api/articles?per_page=3"));
        assert!(url.contains("&tag=webdev"));
        assert!(url.contains("&username=bob"));
        assert!(url.contains("&state=fresh"));
        assert!(url.contains("&top=30"));
    }

    #[test]
    fn test_build_api_url_empty_strings_ignored() {
        let settings = json!({"contentTag": "", "contentUsername": ""});
        let url = build_api_url(&settings);
        assert_eq!(url, "https://dev.to/api/articles?per_page=10");
    }

    #[test]
    fn test_format_subtitle_full() {
        let article = Article {
            id: 1,
            title: "Test".to_string(),
            url: "https://dev.to/test".to_string(),
            user: ArticleUser { username: "alice".to_string() },
            published_at: "2026-01-01".to_string(),
            positive_reactions_count: 42,
            comments_count: 7,
            reading_time_minutes: 5,
            tag_list: vec![],
        };
        let subtitle = format_subtitle(&article);
        assert_eq!(subtitle, "by alice • 5m read • ❤️ 42 • 💬 7");
    }

    #[test]
    fn test_format_subtitle_minimal() {
        let article = Article {
            id: 1,
            title: "Test".to_string(),
            url: String::new(),
            user: ArticleUser { username: "bob".to_string() },
            published_at: String::new(),
            positive_reactions_count: 0,
            comments_count: 0,
            reading_time_minutes: 0,
            tag_list: vec![],
        };
        let subtitle = format_subtitle(&article);
        assert_eq!(subtitle, "by bob");
    }

    #[test]
    fn test_build_article_list_empty() {
        let content = build_article_list(&[], 10);
        assert_eq!(content["type"], "text");
        assert!(content["content"].as_str().unwrap().contains("No articles"));
    }

    #[test]
    fn test_build_article_list_with_articles() {
        let articles = vec![
            Article {
                id: 1,
                title: "First Post".to_string(),
                url: "https://dev.to/first".to_string(),
                user: ArticleUser { username: "alice".to_string() },
                published_at: String::new(),
                positive_reactions_count: 10,
                comments_count: 2,
                reading_time_minutes: 3,
                tag_list: vec![],
            },
            Article {
                id: 2,
                title: "Second Post".to_string(),
                url: "https://dev.to/second".to_string(),
                user: ArticleUser { username: "bob".to_string() },
                published_at: String::new(),
                positive_reactions_count: 0,
                comments_count: 0,
                reading_time_minutes: 1,
                tag_list: vec![],
            },
        ];
        let content = build_article_list(&articles, 10);
        assert_eq!(content["type"], "list");
        let items = content["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["title"], "First Post");
        assert_eq!(items[0]["id"], "https://dev.to/first");
        assert_eq!(items[1]["title"], "Second Post");
    }

    #[test]
    fn test_build_article_list_respects_limit() {
        let articles = vec![
            Article { id: 1, title: "A".to_string(), url: "u1".to_string(), user: ArticleUser { username: "x".to_string() }, published_at: String::new(), positive_reactions_count: 0, comments_count: 0, reading_time_minutes: 0, tag_list: vec![] },
            Article { id: 2, title: "B".to_string(), url: "u2".to_string(), user: ArticleUser { username: "x".to_string() }, published_at: String::new(), positive_reactions_count: 0, comments_count: 0, reading_time_minutes: 0, tag_list: vec![] },
            Article { id: 3, title: "C".to_string(), url: "u3".to_string(), user: ArticleUser { username: "x".to_string() }, published_at: String::new(), positive_reactions_count: 0, comments_count: 0, reading_time_minutes: 0, tag_list: vec![] },
        ];
        let content = build_article_list(&articles, 2);
        let items = content["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_parse_article_json() {
        let json_str = r#"[{
            "id": 12345,
            "title": "Building with Rust",
            "url": "https://dev.to/alice/building-with-rust",
            "user": {"username": "alice"},
            "published_at": "2026-07-01T10:00:00Z",
            "positive_reactions_count": 100,
            "comments_count": 15,
            "reading_time_minutes": 8,
            "tag_list": ["rust", "webdev"]
        }]"#;
        let articles: Vec<Article> = serde_json::from_str(json_str).unwrap();
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].title, "Building with Rust");
        assert_eq!(articles[0].user.username, "alice");
        assert_eq!(articles[0].positive_reactions_count, 100);
        assert_eq!(articles[0].tag_list, vec!["rust", "webdev"]);
    }
}
