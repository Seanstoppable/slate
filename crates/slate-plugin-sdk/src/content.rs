use serde::{Deserialize, Serialize};

/// Rich content types that widgets can render.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetContent {
    Text {
        content: String,
        #[serde(default)]
        scrollable: bool,
        #[serde(default)]
        wrap: bool,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<Cell>>,
        #[serde(default)]
        selectable: bool,
    },
    KeyValue {
        pairs: Vec<(String, Cell)>,
    },
    List {
        items: Vec<ListItem>,
        #[serde(default)]
        selectable: bool,
        #[serde(default)]
        actions: Vec<Action>,
    },
    Chart {
        data: Vec<DataPoint>,
        chart_type: ChartType,
    },
    Empty {
        message: String,
    },
}

/// A styled cell value for tables and key-value displays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub text: String,
    #[serde(default)]
    pub style: CellStyle,
}

impl Cell {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: CellStyle::default(),
        }
    }

    pub fn colored(text: impl Into<String>, color: Color) -> Self {
        Self {
            text: text.into(),
            style: CellStyle {
                fg: Some(color),
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CellStyle {
    #[serde(default)]
    pub fg: Option<Color>,
    #[serde(default)]
    pub bg: Option<Color>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Gray,
    Rgb(u8, u8, u8),
}

/// An item in a List widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub style: CellStyle,
}

/// An action that can be performed on a list item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub confirm: bool,
}

/// A data point for chart widgets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    Bar,
    Line,
    Sparkline,
}

impl WidgetContent {
    /// Returns true if this is a selectable list.
    pub fn is_selectable_list(&self) -> bool {
        matches!(
            self,
            WidgetContent::List {
                selectable: true,
                ..
            }
        )
    }

    /// Returns the number of items in a list, or 0 if not a list.
    pub fn list_len(&self) -> usize {
        match self {
            WidgetContent::List { items, .. } => items.len(),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_list_item() -> ListItem {
        ListItem {
            id: "item-1".to_string(),
            title: "Item 1".to_string(),
            subtitle: Some("Details".to_string()),
            icon: Some("•".to_string()),
            style: CellStyle {
                fg: Some(Color::Blue),
                bg: None,
                bold: true,
                italic: false,
            },
        }
    }

    #[test]
    fn list_len_returns_zero_for_non_list_content() {
        let content = WidgetContent::Text {
            content: "hello".to_string(),
            scrollable: false,
            wrap: true,
        };

        assert_eq!(content.list_len(), 0);
    }

    #[test]
    fn selectable_list_only_returns_true_for_selectable_lists() {
        let variants = vec![
            WidgetContent::Text {
                content: "text".to_string(),
                scrollable: false,
                wrap: true,
            },
            WidgetContent::Table {
                headers: vec!["name".to_string()],
                rows: vec![vec![Cell::plain("value")]],
                selectable: true,
            },
            WidgetContent::KeyValue {
                pairs: vec![("status".to_string(), Cell::plain("ok"))],
            },
            WidgetContent::List {
                items: vec![sample_list_item()],
                selectable: false,
                actions: vec![],
            },
            WidgetContent::Chart {
                data: vec![DataPoint {
                    label: "Jan".to_string(),
                    value: 42.0,
                }],
                chart_type: ChartType::Bar,
            },
            WidgetContent::Empty {
                message: "Nothing here".to_string(),
            },
        ];

        for variant in variants {
            assert!(!variant.is_selectable_list());
        }

        let selectable = WidgetContent::List {
            items: vec![sample_list_item()],
            selectable: true,
            actions: vec![],
        };
        assert!(selectable.is_selectable_list());
    }

    #[test]
    fn list_len_returns_item_count_only_for_lists() {
        let list = WidgetContent::List {
            items: vec![sample_list_item(), sample_list_item()],
            selectable: true,
            actions: vec![],
        };
        assert_eq!(list.list_len(), 2);

        let non_lists = vec![
            WidgetContent::Text {
                content: "text".to_string(),
                scrollable: false,
                wrap: true,
            },
            WidgetContent::Table {
                headers: vec!["name".to_string()],
                rows: vec![vec![Cell::plain("value")]],
                selectable: false,
            },
            WidgetContent::KeyValue {
                pairs: vec![("status".to_string(), Cell::plain("ok"))],
            },
            WidgetContent::Chart {
                data: vec![DataPoint {
                    label: "Jan".to_string(),
                    value: 1.0,
                }],
                chart_type: ChartType::Line,
            },
            WidgetContent::Empty {
                message: "empty".to_string(),
            },
        ];

        for content in non_lists {
            assert_eq!(content.list_len(), 0);
        }
    }

    #[test]
    fn cell_constructors_set_expected_styles() {
        let plain = Cell::plain("hello");
        assert_eq!(plain.text, "hello");
        assert!(plain.style.fg.is_none());
        assert!(plain.style.bg.is_none());
        assert!(!plain.style.bold);
        assert!(!plain.style.italic);

        let colored = Cell::colored("warn", Color::Yellow);
        assert_eq!(colored.text, "warn");
        assert!(matches!(colored.style.fg, Some(Color::Yellow)));
        assert!(colored.style.bg.is_none());
        assert!(!colored.style.bold);
        assert!(!colored.style.italic);
    }

    #[test]
    fn rgb_colors_round_trip_through_json() {
        let json = serde_json::to_string(&Color::Rgb(12, 34, 56)).unwrap();
        let round_trip: Color = serde_json::from_str(&json).unwrap();

        assert!(matches!(round_trip, Color::Rgb(12, 34, 56)));
    }

    #[test]
    fn widget_content_variants_round_trip_through_json() {
        let variants = vec![
            WidgetContent::Text {
                content: "hello".to_string(),
                scrollable: true,
                wrap: false,
            },
            WidgetContent::Table {
                headers: vec!["name".to_string(), "value".to_string()],
                rows: vec![vec![Cell::plain("cpu"), Cell::colored("90%", Color::Red)]],
                selectable: true,
            },
            WidgetContent::KeyValue {
                pairs: vec![("status".to_string(), Cell::colored("ok", Color::Green))],
            },
            WidgetContent::List {
                items: vec![sample_list_item()],
                selectable: true,
                actions: vec![Action {
                    id: "open".to_string(),
                    label: "Open".to_string(),
                    key: Some("enter".to_string()),
                    confirm: true,
                }],
            },
            WidgetContent::Chart {
                data: vec![DataPoint {
                    label: "point".to_string(),
                    value: 12.5,
                }],
                chart_type: ChartType::Sparkline,
            },
            WidgetContent::Empty {
                message: "No content".to_string(),
            },
        ];

        for original in variants {
            let json = serde_json::to_string(&original).unwrap();
            let round_trip: WidgetContent = serde_json::from_str(&json).unwrap();

            match (original, round_trip) {
                (
                    WidgetContent::Text {
                        content: expected_content,
                        scrollable: expected_scrollable,
                        wrap: expected_wrap,
                    },
                    WidgetContent::Text {
                        content,
                        scrollable,
                        wrap,
                    },
                ) => {
                    assert_eq!(content, expected_content);
                    assert_eq!(scrollable, expected_scrollable);
                    assert_eq!(wrap, expected_wrap);
                }
                (
                    WidgetContent::Table {
                        headers: expected_headers,
                        rows: expected_rows,
                        selectable: expected_selectable,
                    },
                    WidgetContent::Table {
                        headers,
                        rows,
                        selectable,
                    },
                ) => {
                    assert_eq!(headers, expected_headers);
                    assert_eq!(rows.len(), expected_rows.len());
                    assert_eq!(rows[0][0].text, expected_rows[0][0].text);
                    assert_eq!(rows[0][1].text, expected_rows[0][1].text);
                    assert_eq!(selectable, expected_selectable);
                }
                (
                    WidgetContent::KeyValue {
                        pairs: expected_pairs,
                    },
                    WidgetContent::KeyValue { pairs },
                ) => {
                    assert_eq!(pairs.len(), expected_pairs.len());
                    assert_eq!(pairs[0].0, expected_pairs[0].0);
                    assert_eq!(pairs[0].1.text, expected_pairs[0].1.text);
                }
                (
                    WidgetContent::List {
                        items: expected_items,
                        selectable: expected_selectable,
                        actions: expected_actions,
                    },
                    WidgetContent::List {
                        items,
                        selectable,
                        actions,
                    },
                ) => {
                    assert_eq!(items.len(), expected_items.len());
                    assert_eq!(items[0].title, expected_items[0].title);
                    assert_eq!(selectable, expected_selectable);
                    assert_eq!(actions.len(), expected_actions.len());
                    assert_eq!(actions[0].label, expected_actions[0].label);
                }
                (
                    WidgetContent::Chart {
                        data: expected_data,
                        chart_type: expected_chart_type,
                    },
                    WidgetContent::Chart { data, chart_type },
                ) => {
                    assert_eq!(data.len(), expected_data.len());
                    assert_eq!(data[0].label, expected_data[0].label);
                    assert_eq!(data[0].value, expected_data[0].value);
                    assert!(matches!(
                        (chart_type, expected_chart_type),
                        (ChartType::Sparkline, ChartType::Sparkline)
                            | (ChartType::Bar, ChartType::Bar)
                            | (ChartType::Line, ChartType::Line)
                    ));
                }
                (
                    WidgetContent::Empty {
                        message: expected_message,
                    },
                    WidgetContent::Empty { message },
                ) => assert_eq!(message, expected_message),
                _ => panic!("variant changed during round trip"),
            }
        }
    }
}
