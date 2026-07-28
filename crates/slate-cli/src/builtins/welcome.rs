use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};

pub(crate) struct WelcomeWidget;

impl slate_plugin_sdk::Widget for WelcomeWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Welcome".to_string(),
            description: "Welcome screen".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        WidgetContent::Text {
            content: concat!(
                "Welcome to Slate!\n\n",
                "Edit %APPDATA%\\slate\\slate.toml to add widgets.\n",
                "Run `slate search` to find plugins.\n",
                "Run `slate install` to install declared plugins.\n\n",
                "Press 'q' to quit."
            )
            .to_string(),
            scrollable: false,
            wrap: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WelcomeWidget;
    use slate_plugin_sdk::{Position, Widget, WidgetConfig, WidgetContent};

    #[test]
    fn welcome_widget_returns_expected_metadata_and_content() {
        let mut widget = WelcomeWidget;
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
        let metadata = widget.metadata();
        assert_eq!(metadata.name, "Welcome");
        assert_eq!(metadata.description, "Welcome screen");

        match widget.refresh() {
            WidgetContent::Text { content, wrap, .. } => {
                assert!(content.contains("Welcome to Slate!"));
                assert!(content.contains("slate search"));
                assert!(wrap);
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }
}
