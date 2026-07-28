use chrono::Local;
use slate_plugin_sdk::{Cell, WidgetConfig, WidgetContent, WidgetMetadata};

pub(crate) struct ClockWidget;

impl ClockWidget {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl slate_plugin_sdk::Widget for ClockWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Clock".to_string(),
            description: "Current date and time".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let now = Local::now();
        let pairs = vec![
            (
                "Time".to_string(),
                Cell::plain(now.format("%H:%M:%S").to_string()),
            ),
            (
                "Date".to_string(),
                Cell::plain(now.format("%A, %B %d, %Y").to_string()),
            ),
            (
                "Timezone".to_string(),
                Cell::plain(now.format("%Z").to_string()),
            ),
            (
                "Unix".to_string(),
                Cell::plain(now.timestamp().to_string()),
            ),
        ];
        WidgetContent::KeyValue { pairs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::Widget;

    #[test]
    fn clock_widget_returns_key_value_content() {
        let mut widget = ClockWidget::new();
        let content = widget.refresh();
        match content {
            WidgetContent::KeyValue { pairs } => {
                let keys: Vec<_> = pairs.iter().map(|(k, _)| k.as_str()).collect();
                assert_eq!(keys, vec!["Time", "Date", "Timezone", "Unix"]);
            }
            other => panic!("expected KeyValue, got {other:?}"),
        }
    }
}
