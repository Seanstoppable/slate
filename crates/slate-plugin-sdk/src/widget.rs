use crate::{WidgetConfig, WidgetContent, WidgetMetadata};

/// The core trait all widgets implement regardless of runtime tier.
pub trait Widget: Send {
    /// Returns metadata about this widget (name, version, etc.).
    fn metadata(&self) -> WidgetMetadata;

    /// Initialize the widget with its configuration.
    fn init(&mut self, config: WidgetConfig);

    /// Refresh the widget's content. Called on the configured interval.
    fn refresh(&mut self) -> WidgetContent;

    /// Handle a key press while the widget has focus.
    fn on_key(&mut self, _key: &str, _action: &str) {}

    /// Handle an action triggered on a specific item (e.g., a list item or
    /// selectable table row).
    /// Returns an optional action for the host to execute.
    fn on_action(&mut self, _action_id: &str, _item_id: &str) -> Option<WidgetAction> {
        None
    }

    /// Called when the widget gains focus.
    fn on_focus(&mut self) {}

    /// Called when the widget loses focus.
    fn on_blur(&mut self) {}
}

/// Actions a widget can request from the host.
#[derive(Debug, Clone, PartialEq)]
pub enum WidgetAction {
    /// Open a URL in the system browser.
    OpenUrl(String),
    /// Display a notification message.
    Notify(String),
    /// Show detail content in the widget cell (replaces list view until dismissed).
    ShowDetail(String),
    /// Request a text prompt from the user. When the user submits, the host
    /// calls `on_action(action_id, typed_text)` back into the plugin.
    PromptInput { prompt: String, action_id: String },
}

/// A boxed widget for dynamic dispatch.
pub type BoxedWidget = Box<dyn Widget>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, WidgetConfig, WidgetContent, WidgetMetadata};

    struct DefaultBehaviorWidget;

    impl Widget for DefaultBehaviorWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Default".to_string(),
                description: String::new(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            WidgetContent::Empty {
                message: "empty".to_string(),
            }
        }
    }

    #[test]
    fn prompt_input_action_compares_equal_to_itself() {
        assert_eq!(
            WidgetAction::PromptInput {
                prompt: "New todo:".to_string(),
                action_id: "add".to_string(),
            },
            WidgetAction::PromptInput {
                prompt: "New todo:".to_string(),
                action_id: "add".to_string(),
            }
        );
    }

    #[test]
    fn prompt_input_action_is_not_equal_to_notify() {
        assert_ne!(
            WidgetAction::PromptInput {
                prompt: "Enter:".to_string(),
                action_id: "add".to_string(),
            },
            WidgetAction::Notify("Enter:".to_string())
        );
    }

    #[test]
    fn show_detail_actions_compare_equal() {
        assert_eq!(
            WidgetAction::ShowDetail("details".to_string()),
            WidgetAction::ShowDetail("details".to_string())
        );
    }

    #[test]
    fn open_url_actions_compare_equal() {
        assert_eq!(
            WidgetAction::OpenUrl("https://example.com".to_string()),
            WidgetAction::OpenUrl("https://example.com".to_string())
        );
    }

    #[test]
    fn different_widget_action_variants_are_not_equal() {
        assert_ne!(
            WidgetAction::ShowDetail("details".to_string()),
            WidgetAction::OpenUrl("details".to_string())
        );
    }

    #[test]
    fn default_widget_hooks_are_noops() {
        let mut widget = DefaultBehaviorWidget;
        widget.on_key("Enter", "press");
        assert_eq!(widget.on_action("select", "item-1"), None);
        widget.on_focus();
        widget.on_blur();
        widget.init(WidgetConfig {
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: None,
        });
        assert!(matches!(widget.refresh(), WidgetContent::Empty { .. }));
    }

    #[test]
    fn boxed_widget_uses_default_trait_hooks_via_dynamic_dispatch() {
        let mut widget: BoxedWidget = Box::new(DefaultBehaviorWidget);

        assert_eq!(widget.metadata().name, "Default");
        widget.init(WidgetConfig {
            position: Position {
                row: 1,
                col: 2,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: Some(15),
        });
        widget.on_key("Space", "press");
        widget.on_focus();
        widget.on_blur();
        assert_eq!(widget.on_action("inspect", "item-7"), None);
        assert!(matches!(widget.refresh(), WidgetContent::Empty { .. }));
    }
}
