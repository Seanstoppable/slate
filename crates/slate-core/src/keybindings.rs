use slate_plugin_sdk::{Action, WidgetContent};

pub(crate) const HOST_KEYBINDINGS: &[(&str, &str)] = &[
    ("?", "Show widget help"),
    ("r", "Refresh widget"),
    ("q", "Quit Slate"),
];

pub(crate) fn reserved_keybinding(key: &str) -> Option<&'static str> {
    HOST_KEYBINDINGS
        .iter()
        .find(|(reserved, _)| reserved.eq_ignore_ascii_case(key))
        .map(|(_, label)| *label)
}

pub(crate) fn reserved_keybinding_error(content: &WidgetContent) -> Option<String> {
    let WidgetContent::List { actions, .. } = content else {
        return None;
    };

    actions.iter().find_map(|action| {
        let key = action.key.as_deref()?;
        let reserved_for = reserved_keybinding(key)?;
        Some(format!(
            "Action '{}' binds reserved key '{}', which Slate uses to {}.",
            action.label, key, reserved_for
        ))
    })
}

pub(crate) fn action_has_reserved_key(action: &Action) -> bool {
    action
        .key
        .as_deref()
        .is_some_and(|key| reserved_keybinding(key).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_reserved_list_action_keys() {
        let content = WidgetContent::List {
            items: vec![],
            selectable: true,
            actions: vec![Action {
                id: "refresh".to_string(),
                label: "Refresh remotely".to_string(),
                key: Some("R".to_string()),
                confirm: false,
            }],
        };

        assert_eq!(
            reserved_keybinding_error(&content).as_deref(),
            Some("Action 'Refresh remotely' binds reserved key 'R', which Slate uses to Refresh widget.")
        );
    }

    #[test]
    fn permits_unreserved_list_action_keys() {
        let action = Action {
            id: "open".to_string(),
            label: "Open".to_string(),
            key: Some("o".to_string()),
            confirm: false,
        };
        let content = WidgetContent::List {
            items: vec![],
            selectable: true,
            actions: vec![action.clone()],
        };

        assert!(!action_has_reserved_key(&action));
        assert!(reserved_keybinding_error(&content).is_none());
    }
}
