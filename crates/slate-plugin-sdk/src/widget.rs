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

    /// Handle an action triggered on a specific item (e.g., list item action).
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
#[derive(Debug, Clone)]
pub enum WidgetAction {
    /// Open a URL in the system browser.
    OpenUrl(String),
    /// Display a notification message.
    Notify(String),
}

/// A boxed widget for dynamic dispatch.
pub type BoxedWidget = Box<dyn Widget>;
