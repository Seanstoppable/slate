use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
struct Settings {
    #[serde(default)]
    feed_url: String,
}

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

#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: Settings = serde_json::from_str(&input).unwrap_or_default();
    if settings.feed_url.trim().is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "Configure `feed_url` to fetch an RSS or Atom feed.",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let req = HttpRequest::new(settings.feed_url.trim())
        .with_header("Accept", "application/rss+xml, application/atom+xml, application/xml, text/xml");
    let response = http::request::<String>(&req, None)?;
    let body = response.body();
    let xml = std::str::from_utf8(&body).unwrap_or("");

    let items = parse_feed_items(xml);
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

#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

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
