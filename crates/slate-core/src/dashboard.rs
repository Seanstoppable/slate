use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use slate_plugin_sdk::{BoxedWidget, Color, Position, Widget, WidgetContent, WidgetMetadata};

use crate::keybindings::reserved_keybinding_error;
use crate::{config::SlateConfig, layout::FocusPosition};

pub(crate) fn refresh_widget_content(widget: &mut dyn Widget) -> WidgetContent {
    let content = widget.refresh();
    if let Some(error) = reserved_keybinding_error(&content) {
        tracing::error!("Widget keybinding error: {}", error);
        WidgetContent::Text {
            content: format!("[Widget error] {error}"),
            scrollable: false,
            wrap: true,
        }
    } else {
        content
    }
}

/// A running widget instance with its state.
pub struct WidgetInstance {
    pub widget: BoxedWidget,
    pub metadata: WidgetMetadata,
    pub content: WidgetContent,
    pub row: u16,
    pub col: u16,
    pub row_span: u16,
    pub col_span: u16,
    pub last_refresh: Instant,
    pub refresh_interval: Duration,
    /// Selected index for list widgets
    pub selected: Option<usize>,
    /// Detail view content (replaces normal rendering, suppresses refresh)
    pub detail_content: Option<String>,
    /// Per-widget border color from config
    pub border_color: Option<Color>,
}

impl WidgetInstance {
    pub fn should_refresh(&self, now: Instant, focus: &FocusPosition) -> bool {
        self.should_refresh_with_optional_focus(now, Some(focus))
    }

    fn should_refresh_with_optional_focus(
        &self,
        now: Instant,
        focus: Option<&FocusPosition>,
    ) -> bool {
        if now.duration_since(self.last_refresh) < self.refresh_interval {
            return false;
        }

        if let Some(focus) = focus {
            let is_focused = self.row == focus.row && self.col == focus.col;
            if is_focused && self.content.is_selectable_list() {
                return false;
            }
        }

        self.detail_content.is_none()
    }
}

/// Shared dashboard state used by both TUI and web rendering.
pub struct Dashboard {
    pub config: SlateConfig,
    pub widgets: Vec<WidgetInstance>,
}

impl Dashboard {
    pub fn new(config: SlateConfig) -> Self {
        Self {
            config,
            widgets: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_widget(
        &mut self,
        mut widget: BoxedWidget,
        row: u16,
        col: u16,
        row_span: u16,
        col_span: u16,
        refresh_interval: Option<u64>,
        border_color: Option<Color>,
    ) {
        let metadata = widget.metadata();
        let interval = refresh_interval.unwrap_or(self.config.global.refresh_interval);
        let content = refresh_widget_content(widget.as_mut());
        let selected = if content.is_selectable_list() {
            Some(0)
        } else {
            None
        };
        self.widgets.push(WidgetInstance {
            widget,
            metadata,
            content,
            row,
            col,
            row_span: row_span.max(1),
            col_span: col_span.max(1),
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(interval),
            selected,
            detail_content: None,
            border_color,
        });
    }

    pub fn refresh_due(&mut self, focus: Option<&FocusPosition>) {
        let now = Instant::now();
        for instance in &mut self.widgets {
            if instance.should_refresh_with_optional_focus(now, focus) {
                instance.content = refresh_widget_content(instance.widget.as_mut());
                instance.last_refresh = now;
                if instance.content.is_selectable_list() && instance.selected.is_none() {
                    instance.selected = Some(0);
                }
            }
        }
    }

    pub fn widget_count(&self) -> usize {
        self.widgets.len()
    }

    pub fn snapshot(&self) -> DashboardSnapshot {
        DashboardSnapshot {
            generated_at_epoch_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            layout: DashboardLayoutSnapshot {
                rows: self.config.layout.rows,
                cols: self.config.layout.cols,
            },
            widgets: self
                .widgets
                .iter()
                .enumerate()
                .map(|(index, widget)| WidgetSnapshot {
                    id: format!("widget-{}", index),
                    metadata: widget.metadata.clone(),
                    content: widget.content.clone(),
                    position: Position {
                        row: widget.row,
                        col: widget.col,
                        row_span: widget.row_span,
                        col_span: widget.col_span,
                    },
                    border_color: widget.border_color.clone(),
                    refresh_interval_seconds: widget.refresh_interval.as_secs(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardSnapshot {
    pub generated_at_epoch_seconds: u64,
    pub layout: DashboardLayoutSnapshot,
    pub widgets: Vec<WidgetSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardLayoutSnapshot {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct WidgetSnapshot {
    pub id: String,
    pub metadata: WidgetMetadata,
    pub content: WidgetContent,
    pub position: Position,
    pub border_color: Option<Color>,
    pub refresh_interval_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::{Widget, WidgetConfig};

    struct CountingWidget {
        count: u32,
        list: bool,
    }

    struct ReservedKeyWidget;

    impl Widget for CountingWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Counter".to_string(),
                description: String::new(),
                version: "1.0.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            self.count += 1;
            if self.list {
                WidgetContent::List {
                    items: vec![],
                    selectable: true,
                    actions: vec![],
                }
            } else {
                WidgetContent::Text {
                    content: format!("Refresh #{}", self.count),
                    scrollable: false,
                    wrap: true,
                }
            }
        }
    }

    impl Widget for ReservedKeyWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Reserved key".to_string(),
                description: String::new(),
                version: "1.0.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            WidgetContent::List {
                items: vec![],
                selectable: true,
                actions: vec![slate_plugin_sdk::Action {
                    id: "refresh".to_string(),
                    label: "Custom refresh".to_string(),
                    key: Some("r".to_string()),
                    confirm: false,
                }],
            }
        }
    }

    fn config() -> SlateConfig {
        SlateConfig::default()
    }

    #[test]
    fn refresh_due_updates_non_focused_widgets() {
        let mut dashboard = Dashboard::new(config());
        dashboard.add_widget(
            Box::new(CountingWidget {
                count: 0,
                list: false,
            }),
            0,
            0,
            1,
            1,
            Some(1),
            None,
        );
        dashboard.widgets[0].last_refresh = Instant::now() - Duration::from_secs(2);

        dashboard.refresh_due(None);

        match &dashboard.widgets[0].content {
            WidgetContent::Text { content, .. } => assert_eq!(content, "Refresh #2"),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn refresh_due_skips_focused_selectable_lists() {
        let mut dashboard = Dashboard::new(config());
        dashboard.add_widget(
            Box::new(CountingWidget {
                count: 0,
                list: true,
            }),
            0,
            0,
            1,
            1,
            Some(1),
            None,
        );
        dashboard.widgets[0].last_refresh = Instant::now() - Duration::from_secs(2);
        let previous_refresh = dashboard.widgets[0].last_refresh;

        dashboard.refresh_due(Some(&FocusPosition::new(0, 0)));

        assert_eq!(dashboard.widgets[0].last_refresh, previous_refresh);
    }

    #[test]
    fn snapshot_includes_layout_and_widget_positions() {
        let mut dashboard = Dashboard::new(config());
        dashboard.config.layout.rows = 3;
        dashboard.config.layout.cols = 4;
        dashboard.add_widget(
            Box::new(CountingWidget {
                count: 0,
                list: false,
            }),
            1,
            2,
            2,
            1,
            Some(15),
            Some(Color::Blue),
        );

        let snapshot = dashboard.snapshot();

        assert_eq!(snapshot.layout.rows, 3);
        assert_eq!(snapshot.layout.cols, 4);
        assert_eq!(snapshot.widgets.len(), 1);
        assert_eq!(snapshot.widgets[0].position.row, 1);
        assert_eq!(snapshot.widgets[0].position.col, 2);
        assert_eq!(snapshot.widgets[0].position.row_span, 2);
        assert_eq!(snapshot.widgets[0].refresh_interval_seconds, 15);
        assert!(matches!(
            snapshot.widgets[0].border_color,
            Some(Color::Blue)
        ));
    }

    #[test]
    fn refresh_replaces_reserved_keybindings_with_visible_widget_error() {
        let mut widget = ReservedKeyWidget;

        let content = refresh_widget_content(&mut widget);

        assert!(matches!(
            content,
            WidgetContent::Text { content, .. }
                if content.contains("Custom refresh")
                    && content.contains("reserved key 'r'")
        ));
    }
}
