#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

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
        "actions": [
            {"id": "add", "label": "Add item", "key": "a", "confirm": false},
            {"id": "delete", "label": "Delete", "key": "x", "confirm": true},
            {"id": "toggle", "label": "Toggle done", "key": "d", "confirm": false},
        ]
    })
}

/// Toggle a line between done (prefixed "x ") and pending. line_num is 1-based.
pub fn toggle_line(content: &str, line_num: usize) -> String {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let mut result = if i + 1 == line_num {
                if line.starts_with("x ") {
                    line[2..].to_string()
                } else {
                    format!("x {line}")
                }
            } else {
                line.to_string()
            };
            result.push('\n');
            result
        })
        .collect()
}

/// Delete a line by 1-based line number.
pub fn delete_line(content: &str, line_num: usize) -> String {
    content
        .lines()
        .enumerate()
        .filter(|(i, _)| i + 1 != line_num)
        .map(|(_, line)| format!("{line}\n"))
        .collect()
}

#[cfg(target_arch = "wasm32")]
#[host_fn]
extern "ExtismHost" {
    fn get_data_dir(input: String) -> String;
    fn store_get(input: String) -> String;
    fn store_set(input: String) -> String;
}

#[cfg(target_arch = "wasm32")]
fn call_get_data_dir() -> Result<String, Error> {
    let json = unsafe { get_data_dir(String::new())? };
    let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    Ok(v["path"].as_str().unwrap_or("").to_string())
}

#[cfg(target_arch = "wasm32")]
fn read_todo_file(dir: &str) -> String {
    std::fs::read_to_string(format!("{dir}/todo.txt")).unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
fn write_todo_file(dir: &str, content: &str) {
    let _ = std::fs::write(format!("{dir}/todo.txt"), content);
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
    let dir = call_get_data_dir()?;

    let todo_path = format!("{dir}/todo.txt");
    // Create an empty todo.txt if it doesn't exist yet
    if !std::path::Path::new(&todo_path).exists() {
        let _ = std::fs::write(&todo_path, "");
    }
    let content = std::fs::read_to_string(&todo_path).unwrap_or_default();

    let items: Vec<TodoItem> = content
        .lines()
        .filter_map(parse_todo_line)
        .enumerate()
        .map(|(i, mut item)| {
            item.id = i + 1;
            item
        })
        .collect();

    if items.is_empty() {
        return Ok(serde_json::json!({
            "type": "list",
            "selectable": true,
            "items": [],
            "actions": [
                {"id": "add", "label": "Add item", "key": "a", "confirm": false},
            ],
            "empty_message": "No tasks. Press 'a' to add one.",
        })
        .to_string());
    }

    Ok(render_items(&items).to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    #[derive(serde::Deserialize)]
    struct ActionInput {
        action_id: String,
        item_id: String,
    }

    let Ok(action) = serde_json::from_str::<ActionInput>(&input) else {
        return Ok(String::new());
    };

    // "add" key on a list action: always show the prompt to get new text.
    // We distinguish the "show prompt" call from the "write text" callback
    // by using "add_confirm" as the action_id for the second call.
    if action.action_id == "add" {
        return Ok(serde_json::json!({
            "action": "prompt_input",
            "prompt": "New todo",
            "action_id": "add_confirm",
        })
        .to_string());
    }

    let dir = call_get_data_dir()?;

    match action.action_id.as_str() {
        "toggle" => {
            let line_num: usize = action.item_id.parse().unwrap_or(0);
            let content = read_todo_file(&dir);
            let new_content = toggle_line(&content, line_num);
            write_todo_file(&dir, &new_content);
        }
        "delete" => {
            let line_num: usize = action.item_id.parse().unwrap_or(0);
            let content = read_todo_file(&dir);
            let new_content = delete_line(&content, line_num);
            write_todo_file(&dir, &new_content);
        }
        "add_confirm" => {
            // item_id is the new text typed by user (via PromptInput)
            if !action.item_id.trim().is_empty() {
                let mut content = read_todo_file(&dir);
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(action.item_id.trim());
                content.push('\n');
                write_todo_file(&dir, &content);
            }
        }
        _ => {}
    }

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
    fn render_items_includes_actions() {
        let val = render_items(&[]);
        let actions = val["actions"].as_array().unwrap();
        let ids: Vec<&str> = actions
            .iter()
            .map(|a| a["id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"toggle"));
        assert!(ids.contains(&"delete"));
        assert!(ids.contains(&"add"));
    }

    #[test]
    fn toggle_line_marks_pending_item_as_done() {
        let content = "buy milk\ndo laundry\n";
        let result = toggle_line(content, 1);
        assert!(result.starts_with("x buy milk\n"));
        assert!(result.contains("do laundry\n"));
    }

    #[test]
    fn toggle_line_marks_done_item_as_pending() {
        let content = "x buy milk\ndo laundry\n";
        let result = toggle_line(content, 1);
        assert!(result.starts_with("buy milk\n"));
    }

    #[test]
    fn toggle_line_out_of_range_leaves_content_unchanged() {
        let content = "buy milk\ndo laundry\n";
        let result = toggle_line(content, 99);
        assert!(result.contains("buy milk\n"));
        assert!(result.contains("do laundry\n"));
    }

    #[test]
    fn delete_line_removes_correct_line() {
        let content = "buy milk\ndo laundry\nread book\n";
        let result = delete_line(content, 2);
        assert!(result.contains("buy milk\n"));
        assert!(!result.contains("do laundry"));
        assert!(result.contains("read book\n"));
    }

    #[test]
    fn delete_line_out_of_range_leaves_content_unchanged() {
        let content = "buy milk\ndo laundry\n";
        let result = delete_line(content, 99);
        assert!(result.contains("buy milk\n"));
        assert!(result.contains("do laundry\n"));
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
