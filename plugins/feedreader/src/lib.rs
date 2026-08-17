#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
use serde::Deserialize;
#[cfg(target_arch = "wasm32")]
use serde_json::json;

#[derive(Deserialize, Default)]
struct Settings {
    #[serde(default)]
    feeds: Vec<String>,
}

impl Settings {
    fn feed_urls(&self) -> Vec<&str> {
        self.feeds
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .collect()
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct ActionInput {
    #[serde(default)]
    action_id: String,
    #[serde(default)]
    item_id: String,
}

#[derive(Clone)]
struct FeedItem {
    title: String,
    link: String,
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "Feed Reader",
        "description": "Fetches RSS and Atom feeds into a selectable list",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: Settings = serde_json::from_str(&input).unwrap_or_default();
    let feed_urls = settings.feed_urls();
    if feed_urls.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "Configure `feeds` to fetch RSS or Atom feeds.",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let mut items = Vec::new();
    for feed in feed_urls {
        let headers = [(
            "Accept",
            "application/rss+xml, application/atom+xml, application/xml, text/xml",
        )];
        let xml = slate_plugin_http::get_text(feed, &headers)?;
        items.extend(parse_feed_items(&xml));
    }

    if items.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "No feed items found in response.",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let rendered_items: Vec<_> = items
        .into_iter()
        .map(|item| {
            json!({
                "id": item.link,
                "title": item.title,
                "subtitle": item.link
            })
        })
        .collect();

    Ok(json!({
        "type": "list",
        "items": rendered_items,
        "selectable": true
    })
    .to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    let action: ActionInput = serde_json::from_str(&input).unwrap_or(ActionInput {
        action_id: String::new(),
        item_id: String::new(),
    });

    if matches!(action.action_id.as_str(), "select" | "open") && !action.item_id.trim().is_empty() {
        return Ok(json!({ "open_url": action.item_id.trim() }).to_string());
    }

    Ok(String::new())
}

fn parse_feed_items(xml: &str) -> Vec<FeedItem> {
    let mut items = parse_blocks(xml, "item")
        .into_iter()
        .filter_map(parse_rss_item)
        .collect::<Vec<_>>();

    if items.is_empty() {
        items = parse_blocks(xml, "entry")
            .into_iter()
            .filter_map(parse_atom_entry)
            .collect();
    }

    items
}

fn parse_blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let mut blocks = Vec::new();
    let start_marker = format!("<{}", tag);
    let end_marker = format!("</{}>", tag);
    let mut rest = xml;

    while let Some(start) = rest.find(&start_marker) {
        let candidate = &rest[start..];
        let Some(start_end) = candidate.find('>') else {
            break;
        };
        let content_start = start + start_end + 1;
        let Some(end_offset) = rest[content_start..].find(&end_marker) else {
            break;
        };
        let content_end = content_start + end_offset;
        blocks.push(&rest[content_start..content_end]);
        let after_end = content_end + end_marker.len();
        rest = &rest[after_end..];
    }

    blocks
}

fn parse_rss_item(block: &str) -> Option<FeedItem> {
    let title = extract_tag_text(block, "title")?;
    let link = extract_tag_text(block, "link").unwrap_or_default();
    Some(FeedItem {
        title: decode_xml_entities(&title),
        link: decode_xml_entities(&link),
    })
}

fn parse_atom_entry(block: &str) -> Option<FeedItem> {
    let title = extract_tag_text(block, "title")?;
    let link = extract_atom_link(block).unwrap_or_default();
    Some(FeedItem {
        title: decode_xml_entities(&title),
        link: decode_xml_entities(&link),
    })
}

fn extract_tag_text(block: &str, tag: &str) -> Option<String> {
    let start_marker = format!("<{}", tag);
    let start = block.find(&start_marker)?;
    let after_start = &block[start..];
    let start_end = after_start.find('>')?;
    let content_start = start + start_end + 1;
    let end_marker = format!("</{}>", tag);
    let end = block[content_start..].find(&end_marker)?;
    let raw = block[content_start..content_start + end].trim();
    Some(strip_cdata(raw).to_string())
}

fn extract_atom_link(block: &str) -> Option<String> {
    let mut rest = block;
    while let Some(start) = rest.find("<link") {
        let candidate = &rest[start..];
        let end = candidate.find('>')?;
        let tag = &candidate[..=end];
        if let Some(href) = extract_attribute(tag, "href") {
            let rel = extract_attribute(tag, "rel").unwrap_or_default();
            if rel.is_empty() || rel == "alternate" {
                return Some(href);
            }
        }
        rest = &candidate[end + 1..];
    }
    None
}

fn extract_attribute(tag: &str, attr: &str) -> Option<String> {
    let needle = format!(r#"{}=""#, attr);
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

fn strip_cdata(value: &str) -> &str {
    value
        .strip_prefix("<![CDATA[")
        .and_then(|v| v.strip_suffix("]]>"))
        .unwrap_or(value)
}

fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_xml_entities() {
        assert_eq!(decode_xml_entities("&amp;"), "&");
        assert_eq!(decode_xml_entities("&lt;b&gt;"), "<b>");
        assert_eq!(decode_xml_entities("&quot;hi&quot;"), "\"hi\"");
        assert_eq!(decode_xml_entities("plain text"), "plain text");
        assert_eq!(decode_xml_entities("a &amp; b &lt; c"), "a & b < c");
    }

    #[test]
    fn feed_urls_ignores_empty_entries() {
        let settings = Settings {
            feeds: vec![
                " https://one.example.com/rss ".to_string(),
                String::new(),
                "https://two.example.com/atom".to_string(),
            ],
        };

        assert_eq!(
            settings.feed_urls(),
            vec![
                "https://one.example.com/rss",
                "https://two.example.com/atom"
            ]
        );
    }

    #[test]
    fn test_strip_cdata() {
        assert_eq!(strip_cdata("<![CDATA[hello]]>"), "hello");
        assert_eq!(strip_cdata("no cdata"), "no cdata");
        assert_eq!(strip_cdata("<![CDATA[<b>bold</b>]]>"), "<b>bold</b>");
    }

    #[test]
    fn test_extract_attribute() {
        assert_eq!(
            extract_attribute(r#"<link rel="alternate" href="https://example.com"/>"#, "href"),
            Some("https://example.com".to_string())
        );
        assert_eq!(
            extract_attribute(r#"<link rel="alternate" href="https://example.com"/>"#, "rel"),
            Some("alternate".to_string())
        );
        assert_eq!(
            extract_attribute(r#"<link href="https://example.com"/>"#, "rel"),
            None
        );
    }

    #[test]
    fn test_extract_tag_text() {
        let block = "<title>Hello World</title><link>https://example.com</link>";
        assert_eq!(extract_tag_text(block, "title"), Some("Hello World".to_string()));
        assert_eq!(extract_tag_text(block, "link"), Some("https://example.com".to_string()));
        assert_eq!(extract_tag_text(block, "missing"), None);
    }

    #[test]
    fn test_extract_tag_text_cdata() {
        let block = "<title><![CDATA[Breaking & News]]></title>";
        assert_eq!(extract_tag_text(block, "title"), Some("Breaking & News".to_string()));
    }

    #[test]
    fn test_parse_blocks() {
        let xml = "<item><title>One</title></item><item><title>Two</title></item>";
        let blocks = parse_blocks(xml, "item");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("One"));
        assert!(blocks[1].contains("Two"));
    }

    #[test]
    fn test_parse_blocks_with_attributes() {
        let xml = r#"<entry xml:lang="en"><title>Test</title></entry>"#;
        let blocks = parse_blocks(xml, "entry");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].contains("Test"));
    }

    #[test]
    fn test_parse_blocks_stops_on_malformed_or_unclosed_tags() {
        assert!(parse_blocks("<item<title>missing close", "item").is_empty());
        assert!(parse_blocks("<item><title>oops</title>", "item").is_empty());
    }

    #[test]
    fn test_parse_rss_item() {
        let block = "<title>My Article</title><link>https://example.com/article</link>";
        let item = parse_rss_item(block).unwrap();
        assert_eq!(item.title, "My Article");
        assert_eq!(item.link, "https://example.com/article");
    }

    #[test]
    fn test_parse_rss_item_with_entities() {
        let block = "<title>Tom &amp; Jerry</title><link>https://example.com</link>";
        let item = parse_rss_item(block).unwrap();
        assert_eq!(item.title, "Tom & Jerry");
    }

    #[test]
    fn test_parse_atom_entry() {
        let block = r#"<title>Atom Post</title><link rel="alternate" href="https://blog.example.com/post"/>"#;
        let item = parse_atom_entry(block).unwrap();
        assert_eq!(item.title, "Atom Post");
        assert_eq!(item.link, "https://blog.example.com/post");
    }

    #[test]
    fn test_parse_feed_items_rss() {
        let xml = r#"<?xml version="1.0"?>
<rss><channel>
<item><title>First</title><link>https://a.com/1</link></item>
<item><title>Second</title><link>https://a.com/2</link></item>
</channel></rss>"#;
        let items = parse_feed_items(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "First");
        assert_eq!(items[1].link, "https://a.com/2");
    }

    #[test]
    fn test_parse_feed_items_atom() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<entry><title>Atom One</title><link rel="alternate" href="https://b.com/1"/></entry>
</feed>"#;
        let items = parse_feed_items(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Atom One");
        assert_eq!(items[0].link, "https://b.com/1");
    }

    #[test]
    fn test_parse_feed_items_empty() {
        let xml = "<html><body>Not a feed</body></html>";
        let items = parse_feed_items(xml);
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_feed_items_ignores_invalid_rss_before_valid_atom_entries() {
        let xml = r#"
<feed>
  <item><link>https://invalid.example.com</link></item>
  <entry><title>Atom Title</title><link rel="alternate" href="https://valid.example.com"/></entry>
</feed>
"#;
        let items = parse_feed_items(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Atom Title");
        assert_eq!(items[0].link, "https://valid.example.com");
    }
}
