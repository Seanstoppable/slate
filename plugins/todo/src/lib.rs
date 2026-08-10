#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

#[cfg(target_arch = "wasm32")]
host_fn!(get_data_dir() -> String);

/// A parsed todo.txt item.
pub struct TodoItem {
    pub id: usize,
    pub title: String,
    pub subtitle: String,
    pub done: bool,
}

/// Parse a single todo.txt line into a `TodoItem`.
/// Returns `None` for blank lines.
pub fn parse_todo_line(line: &str) -> Option<TodoItem> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Lines starting with "x " are completed tasks
    let (done, rest) = if let Some(remainder) = trimmed.strip_prefix("x ") {
        (true, remainder.trim())
    } else {
        (false, trimmed)
    };

    // Parse optional priority like "(A) " at the start
    let (priority, text) = if rest.starts_with('(')
        && rest.len() >= 4
        && rest.as_bytes().get(2) == Some(&b')')
        && rest.as_bytes().get(3) == Some(&b' ')
    {
        let p = &rest[1..2];
        let t = rest[4..].trim();
        (Some(p.to_string()), t)
    } else {
        (None, rest)
    };

    let title = if done {
        format!("✓ {text}")
    } else {
        text.to_string()
    };

    let subtitle = match &priority {
        Some(p) if !done => format!("Priority: {p}"),
        _ => String::new(),
    };

    Some(TodoItem {
        id: 0, // filled in by caller
        title,
        subtitle,
        done,
    })
}

/// Render a slice of `TodoItem`s into the list JSON value.
pub fn render_items(items: &[TodoItem]) -> serde_json::Value {
    let list: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            serde_json::json!({
                "id": item.id.to_string(),
                "title": item.title,
                "subtitle": item.subtitle,
            })
        })
        .collect();
    serde_json::json!({
        "type": "list",
        "selectable": true,
        "items": list,
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(serde_json::json!({
        "name": "Todo",
        "description": "Displays tasks from a todo.txt file in your plugin data directory",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    let data_dir_json = unsafe { get_data_dir()? };
    let parsed: serde_json::Value = serde_json::from_str(&data_dir_json).unwrap_or_default();
    let dir = parsed["path"].as_str().unwrap_or("").to_string();

    let todo_path = format!("{dir}/todo.txt");
    let content = match std::fs::read_to_string(&todo_path) {
        Ok(text) => text,
        Err(_) => {
            return Ok(serde_json::json!({
                "type": "text",
                "content": format!(
                    "No todo.txt found.\nCreate one at:\n  {todo_path}"
                ),
                "scrollable": false,
                "wrap": true,
            })
            .to_string());
        }
    };

    let items: Vec<TodoItem> = content
        .lines()
        .filter_map(parse_todo_line)
        .enumerate()
        .map(|(i, mut item)| {
            item.id = i + 1;
            item
        })
        .collect();

    Ok(render_items(&items).to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_todo_line_skips_blank_lines() {
        assert!(parse_todo_line("").is_none());
        assert!(parse_todo_line("   ").is_none());
        assert!(parse_todo_line("\t").is_none());
    }

    #[test]
    fn parse_todo_line_marks_done_items() {
        let item = parse_todo_line("x finish the report").unwrap();
        assert!(item.done);
        assert!(item.title.contains('✓'));
        assert!(item.title.contains("finish the report"));
    }

    #[test]
    fn parse_todo_line_parses_priority() {
        let item = parse_todo_line("(A) call dentist").unwrap();
        assert!(!item.done);
        assert_eq!(item.title, "call dentist");
        assert_eq!(item.subtitle, "Priority: A");
    }

    #[test]
    fn parse_todo_line_parses_plain_items() {
        let item = parse_todo_line("buy milk").unwrap();
        assert!(!item.done);
        assert_eq!(item.title, "buy milk");
        assert!(item.subtitle.is_empty());
    }

    #[test]
    fn parse_todo_line_done_item_has_no_priority_subtitle() {
        let item = parse_todo_line("x (B) old priority task").unwrap();
        assert!(item.done);
        // When done, priority is not shown in subtitle
        assert!(item.subtitle.is_empty());
    }

    #[test]
    fn render_items_returns_empty_list_for_no_items() {
        let val = render_items(&[]);
        assert_eq!(val["type"], "list");
        assert!(val["selectable"].as_bool().unwrap());
        assert_eq!(val["items"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn render_items_includes_all_fields() {
        let items = vec![
            TodoItem {
                id: 1,
                title: "✓ done".to_string(),
                subtitle: String::new(),
                done: true,
            },
            TodoItem {
                id: 2,
                title: "pending".to_string(),
                subtitle: "Priority: A".to_string(),
                done: false,
            },
        ];
        let val = render_items(&items);
        let arr = val["items"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["id"], "1");
        assert_eq!(arr[0]["title"], "✓ done");
        assert_eq!(arr[1]["subtitle"], "Priority: A");
    }

    #[test]
    fn mixed_content_parsed_correctly() {
        let input = "x task done\n(B) medium priority\n\nnormal task\n(A) top priority";
        let items: Vec<TodoItem> = input
            .lines()
            .filter_map(parse_todo_line)
            .enumerate()
            .map(|(i, mut item)| {
                item.id = i + 1;
                item
            })
            .collect();
        assert_eq!(items.len(), 4);
        assert!(items[0].done);
        assert_eq!(items[1].subtitle, "Priority: B");
        assert_eq!(items[2].title, "normal task");
        assert_eq!(items[3].subtitle, "Priority: A");
    }
}
