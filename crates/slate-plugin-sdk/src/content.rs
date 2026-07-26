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
