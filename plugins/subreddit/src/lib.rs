#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

use serde::Deserialize;
use serde_json::json;

/// Reddit API listing response.
#[derive(Deserialize, Debug, Clone, Default)]
struct RedditListing {
    #[serde(default)]
    data: RedditListingData,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct RedditListingData {
    #[serde(default)]
    children: Vec<RedditChild>,
}

#[derive(Deserialize, Debug, Clone)]
struct RedditChild {
    data: RedditPost,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct RedditPost {
    #[serde(default)]
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    score: i64,
    #[serde(default)]
    num_comments: u64,
    #[serde(default)]
    permalink: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    is_self: bool,
    #[serde(default)]
    stickied: bool,
    #[serde(default)]
    over_18: bool,
    #[serde(default)]
    subreddit: String,
    #[serde(default)]
    created_utc: f64,
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Subreddit",
        "description": "Shows posts from Reddit subreddits",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let subreddit = settings["subreddit"]
        .as_str()
        .unwrap_or("all");
    let sort = settings["sort"]
        .as_str()
        .unwrap_or("hot");
    let limit = settings["numberOfPosts"]
        .as_u64()
        .unwrap_or(15) as usize;

    let url = build_reddit_url(subreddit, sort, limit);
    let headers = [("User-Agent", "slate-subreddit/0.1.0")];

    let listing: RedditListing = slate_plugin_http::get_json(&url, &headers).unwrap_or_default();
    let posts: Vec<&RedditPost> = listing.data.children.iter().map(|c| &c.data).collect();

    let show_nsfw = settings["showNsfw"].as_bool().unwrap_or(false);
    let content = build_post_list(&posts, limit, show_nsfw);
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
            let url = if action.item_id.starts_with("http") {
                action.item_id
            } else {
                format!("https://www.reddit.com{}", action.item_id)
            };
            let result = json!({"open_url": url});
            return Ok(result.to_string());
        }
    }
    Ok(String::new())
}

// --- Pure logic (testable on native) ---

/// Build the Reddit JSON API URL.
fn build_reddit_url(subreddit: &str, sort: &str, limit: usize) -> String {
    format!(
        "https://www.reddit.com/r/{}/{}.json?limit={}&raw_json=1",
        subreddit, sort, limit
    )
}

/// Format score compactly (1234 → "1.2k").
fn format_score(score: i64) -> String {
    if score.abs() >= 10000 {
        format!("{:.0}k", score as f64 / 1000.0)
    } else if score.abs() >= 1000 {
        format!("{:.1}k", score as f64 / 1000.0)
    } else {
        score.to_string()
    }
}

/// Build subtitle for a post.
fn format_post_subtitle(post: &RedditPost) -> String {
    let mut parts = Vec::new();
    parts.push(format!("↑{}", format_score(post.score)));
    parts.push(format!("💬{}", post.num_comments));
    parts.push(format!("u/{}", post.author));
    if post.stickied {
        parts.push("📌".to_string());
    }
    if post.over_18 {
        parts.push("🔞".to_string());
    }
    parts.join(" • ")
}

/// Build the widget content from posts.
fn build_post_list(posts: &[&RedditPost], limit: usize, show_nsfw: bool) -> serde_json::Value {
    let items: Vec<serde_json::Value> = posts
        .iter()
        .filter(|p| show_nsfw || !p.over_18)
        .take(limit)
        .map(|post| {
            json!({
                "id": post.permalink,
                "title": post.title,
                "subtitle": format_post_subtitle(post),
            })
        })
        .collect();

    if items.is_empty() {
        json!({
            "type": "text",
            "content": "No posts found.",
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
    fn test_build_reddit_url() {
        let url = build_reddit_url("rust", "hot", 10);
        assert_eq!(url, "https://www.reddit.com/r/rust/hot.json?limit=10&raw_json=1");
    }

    #[test]
    fn test_build_reddit_url_sort_new() {
        let url = build_reddit_url("programming", "new", 25);
        assert!(url.contains("/r/programming/new.json"));
        assert!(url.contains("limit=25"));
    }

    #[test]
    fn test_format_score_small() {
        assert_eq!(format_score(42), "42");
        assert_eq!(format_score(-5), "-5");
        assert_eq!(format_score(999), "999");
    }

    #[test]
    fn test_format_score_thousands() {
        assert_eq!(format_score(1500), "1.5k");
        assert_eq!(format_score(9999), "10.0k");
    }

    #[test]
    fn test_format_score_large() {
        assert_eq!(format_score(15000), "15k");
        assert_eq!(format_score(123456), "123k");
    }

    #[test]
    fn test_format_post_subtitle() {
        let post = RedditPost {
            title: "Test".to_string(),
            author: "alice".to_string(),
            score: 1500,
            num_comments: 42,
            stickied: false,
            over_18: false,
            ..Default::default()
        };
        let subtitle = format_post_subtitle(&post);
        assert_eq!(subtitle, "↑1.5k • 💬42 • u/alice");
    }

    #[test]
    fn test_format_post_subtitle_stickied() {
        let post = RedditPost {
            author: "mod".to_string(),
            score: 100,
            num_comments: 5,
            stickied: true,
            ..Default::default()
        };
        let subtitle = format_post_subtitle(&post);
        assert!(subtitle.contains("📌"));
    }

    #[test]
    fn test_format_post_subtitle_nsfw() {
        let post = RedditPost {
            author: "user".to_string(),
            score: 50,
            num_comments: 3,
            over_18: true,
            ..Default::default()
        };
        let subtitle = format_post_subtitle(&post);
        assert!(subtitle.contains("🔞"));
    }

    #[test]
    fn test_build_post_list_empty() {
        let content = build_post_list(&[], 10, false);
        assert_eq!(content["type"], "text");
        assert!(content["content"].as_str().unwrap().contains("No posts"));
    }

    #[test]
    fn test_build_post_list_filters_nsfw() {
        let nsfw_post = RedditPost {
            title: "NSFW post".to_string(),
            permalink: "/r/test/1".to_string(),
            author: "user".to_string(),
            over_18: true,
            ..Default::default()
        };
        let safe_post = RedditPost {
            title: "Safe post".to_string(),
            permalink: "/r/test/2".to_string(),
            author: "user2".to_string(),
            over_18: false,
            ..Default::default()
        };
        let posts: Vec<&RedditPost> = vec![&nsfw_post, &safe_post];

        // NSFW hidden
        let content = build_post_list(&posts, 10, false);
        let items = content["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["title"], "Safe post");

        // NSFW shown
        let content = build_post_list(&posts, 10, true);
        let items = content["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_build_post_list_respects_limit() {
        let posts: Vec<RedditPost> = (0..5).map(|i| RedditPost {
            title: format!("Post {}", i),
            permalink: format!("/r/test/{}", i),
            author: "u".to_string(),
            ..Default::default()
        }).collect();
        let refs: Vec<&RedditPost> = posts.iter().collect();
        let content = build_post_list(&refs, 3, false);
        let items = content["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_parse_reddit_json() {
        let json_str = r#"{
            "data": {
                "children": [
                    {
                        "data": {
                            "title": "Hello Rust",
                            "author": "rustacean",
                            "score": 256,
                            "num_comments": 30,
                            "permalink": "/r/rust/comments/abc/hello_rust/",
                            "url": "https://example.com",
                            "is_self": false,
                            "stickied": false,
                            "over_18": false,
                            "subreddit": "rust",
                            "created_utc": 1700000000.0
                        }
                    }
                ]
            }
        }"#;
        let listing: RedditListing = serde_json::from_str(json_str).unwrap();
        assert_eq!(listing.data.children.len(), 1);
        let post = &listing.data.children[0].data;
        assert_eq!(post.title, "Hello Rust");
        assert_eq!(post.author, "rustacean");
        assert_eq!(post.score, 256);
        assert_eq!(post.subreddit, "rust");
    }
}
