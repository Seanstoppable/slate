use ratatui::{
    layout::Rect,
    style::{Color as RatColor, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
    Frame,
};
use slate_plugin_sdk::{Color, WidgetContent, WidgetMetadata};

use crate::layout::FocusPosition;

/// Render a widget's content into a frame area.
pub fn render_widget(
    frame: &mut Frame,
    area: Rect,
    content: &WidgetContent,
    metadata: &WidgetMetadata,
    focused: bool,
    selected: Option<usize>,
    border_color: Option<&Color>,
) {
    let border_style = if focused {
        Style::default().fg(RatColor::Cyan)
    } else if let Some(color) = border_color {
        Style::default().fg(convert_color(color))
    } else {
        Style::default().fg(RatColor::DarkGray)
    };

    let block = Block::default()
        .title(format!(" {} ", metadata.name))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    match content {
        WidgetContent::Text { content, wrap, .. } => {
            let paragraph = Paragraph::new(content.as_str());
            let paragraph = if *wrap {
                paragraph.wrap(Wrap { trim: true })
            } else {
                paragraph
            };
            frame.render_widget(paragraph, inner);
        }
        WidgetContent::Table { headers, rows, .. } => {
            let header_cells: Vec<Span> = headers
                .iter()
                .map(|h| Span::styled(h.as_str(), Style::default().add_modifier(Modifier::BOLD)))
                .collect();
            let header = Row::new(header_cells).style(Style::default().fg(RatColor::Yellow));

            let table_rows: Vec<Row> = rows
                .iter()
                .map(|row| {
                    let cells: Vec<Span> = row
                        .iter()
                        .map(|cell| Span::styled(cell.text.as_str(), convert_style(&cell.style)))
                        .collect();
                    Row::new(cells)
                })
                .collect();

            let widths: Vec<ratatui::layout::Constraint> = headers
                .iter()
                .map(|_| ratatui::layout::Constraint::Percentage(100 / headers.len() as u16))
                .collect();

            let table = Table::new(table_rows, widths).header(header);
            frame.render_widget(table, inner);
        }
        WidgetContent::KeyValue { pairs } => {
            let lines: Vec<Line> = pairs
                .iter()
                .map(|(key, cell)| {
                    Line::from(vec![
                        Span::styled(
                            format!("{}: ", key),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(cell.text.as_str(), convert_style(&cell.style)),
                    ])
                })
                .collect();
            let paragraph = Paragraph::new(lines);
            frame.render_widget(paragraph, inner);
        }
        WidgetContent::List {
            items, selectable, ..
        } => {
            let list_items: Vec<ListItem> = items
                .iter()
                .map(|item| {
                    let content = if let Some(subtitle) = &item.subtitle {
                        Line::from(vec![
                            Span::styled(
                                item.title.as_str(),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" "),
                            Span::styled(subtitle.as_str(), Style::default().fg(RatColor::Gray)),
                        ])
                    } else {
                        Line::from(Span::raw(item.title.as_str()))
                    };
                    ListItem::new(content)
                })
                .collect();
            let list = List::new(list_items)
                .highlight_style(
                    Style::default()
                        .fg(RatColor::Black)
                        .bg(RatColor::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ");

            if *selectable && selected.is_some() {
                let mut state = ListState::default().with_selected(selected);
                frame.render_stateful_widget(list, inner, &mut state);
            } else {
                frame.render_widget(list, inner);
            }
        }
        WidgetContent::Chart { data, .. } => {
            // Simple sparkline-style bar rendering
            let max = data.iter().map(|d| d.value).fold(0.0_f64, f64::max);
            let lines: Vec<Line> = data
                .iter()
                .map(|dp| {
                    let bar_len = if max > 0.0 {
                        ((dp.value / max) * 20.0) as usize
                    } else {
                        0
                    };
                    let bar: String = "█".repeat(bar_len);
                    Line::from(vec![
                        Span::styled(
                            format!("{:>8} ", dp.label),
                            Style::default().fg(RatColor::Gray),
                        ),
                        Span::styled(bar, Style::default().fg(RatColor::Green)),
                    ])
                })
                .collect();
            let paragraph = Paragraph::new(lines);
            frame.render_widget(paragraph, inner);
        }
        WidgetContent::Empty { message } => {
            let paragraph =
                Paragraph::new(message.as_str()).style(Style::default().fg(RatColor::DarkGray));
            frame.render_widget(paragraph, inner);
        }
    }
}

/// Render the status bar at the bottom of the screen.
pub fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    focus: &FocusPosition,
    widget_count: usize,
    update_msg: Option<&str>,
) {
    let update_part = update_msg.unwrap_or("");
    let status = format!(
        " Slate │ {} widgets │ Focus: ({},{}) {}│ q: quit │ Tab: next │ ←↑↓→: navigate ",
        widget_count, focus.row, focus.col, update_part
    );
    let paragraph =
        Paragraph::new(status).style(Style::default().bg(RatColor::DarkGray).fg(RatColor::White));
    frame.render_widget(paragraph, area);
}

fn convert_style(style: &slate_plugin_sdk::CellStyle) -> Style {
    let mut s = Style::default();
    if let Some(ref color) = style.fg {
        s = s.fg(convert_color(color));
    }
    if let Some(ref color) = style.bg {
        s = s.bg(convert_color(color));
    }
    if style.bold {
        s = s.add_modifier(Modifier::BOLD);
    }
    if style.italic {
        s = s.add_modifier(Modifier::ITALIC);
    }
    s
}

fn convert_color(color: &Color) -> RatColor {
    match color {
        Color::Red => RatColor::Red,
        Color::Green => RatColor::Green,
        Color::Yellow => RatColor::Yellow,
        Color::Blue => RatColor::Blue,
        Color::Magenta => RatColor::Magenta,
        Color::Cyan => RatColor::Cyan,
        Color::White => RatColor::White,
        Color::Gray => RatColor::Gray,
        Color::Rgb(r, g, b) => RatColor::Rgb(*r, *g, *b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    use slate_plugin_sdk::{Cell, CellStyle, ChartType, DataPoint, ListItem as SlateListItem};

    fn metadata() -> WidgetMetadata {
        WidgetMetadata {
            name: "Widget".to_string(),
            description: "Rendered widget".to_string(),
            version: "1.0.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn render_to_string(
        content: &WidgetContent,
        focused: bool,
        selected: Option<usize>,
        width: u16,
        height: u16,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_widget(
                    frame,
                    Rect::new(0, 0, width, height),
                    content,
                    &metadata(),
                    focused,
                    selected,
                    None,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..height {
            for x in 0..width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }
        rendered
    }

    fn render_status_to_string(
        focus: FocusPosition,
        widget_count: usize,
        update: Option<&str>,
    ) -> String {
        let backend = TestBackend::new(80, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_status_bar(frame, Rect::new(0, 1, 80, 1), &focus, widget_count, update);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for x in 0..80 {
            rendered.push_str(buffer[(x, 1)].symbol());
        }
        rendered
    }

    #[test]
    fn render_widget_renders_text_key_value_and_empty_content() {
        let text = render_to_string(
            &WidgetContent::Text {
                content: "Hello Slate".to_string(),
                scrollable: false,
                wrap: true,
            },
            true,
            None,
            40,
            6,
        );
        assert!(text.contains("Widget"));
        assert!(text.contains("Hello Slate"));

        let key_value = render_to_string(
            &WidgetContent::KeyValue {
                pairs: vec![
                    (
                        "CPU".to_string(),
                        Cell {
                            text: "12%".to_string(),
                            style: CellStyle {
                                fg: Some(Color::Green),
                                bg: Some(Color::Rgb(1, 2, 3)),
                                bold: true,
                                italic: true,
                            },
                        },
                    ),
                    ("Memory".to_string(), Cell::plain("4 GB".to_string())),
                ],
            },
            false,
            None,
            40,
            6,
        );
        assert!(key_value.contains("CPU:"));
        assert!(key_value.contains("12%"));
        assert!(key_value.contains("Memory:"));

        let empty = render_to_string(
            &WidgetContent::Empty {
                message: "Nothing here".to_string(),
            },
            false,
            None,
            40,
            6,
        );
        assert!(empty.contains("Nothing here"));
    }

    #[test]
    fn render_widget_renders_tables_lists_and_charts() {
        let table = render_to_string(
            &WidgetContent::Table {
                headers: vec!["Name".to_string(), "Value".to_string()],
                rows: vec![vec![Cell::plain("CPU"), Cell::plain("12%")]],
                selectable: false,
            },
            false,
            None,
            40,
            6,
        );
        assert!(table.contains("Name"));
        assert!(table.contains("Value"));
        assert!(table.contains("CPU"));

        let list = render_to_string(
            &WidgetContent::List {
                items: vec![
                    SlateListItem {
                        id: "1".to_string(),
                        title: "Issue 1".to_string(),
                        subtitle: Some("open".to_string()),
                        icon: None,
                        style: Default::default(),
                    },
                    SlateListItem {
                        id: "2".to_string(),
                        title: "Issue 2".to_string(),
                        subtitle: None,
                        icon: None,
                        style: Default::default(),
                    },
                ],
                selectable: true,
                actions: vec![],
            },
            false,
            Some(0),
            40,
            6,
        );
        assert!(list.contains("Issue 1"));
        assert!(list.contains("open"));
        assert!(list.contains("Issue 2"));

        let chart = render_to_string(
            &WidgetContent::Chart {
                data: vec![
                    DataPoint {
                        label: "CPU".to_string(),
                        value: 10.0,
                    },
                    DataPoint {
                        label: "Mem".to_string(),
                        value: 20.0,
                    },
                ],
                chart_type: ChartType::Bar,
            },
            false,
            None,
            40,
            6,
        );
        assert!(chart.contains("CPU"));
        assert!(chart.contains("Mem"));
        assert!(chart.contains("█"));
    }

    #[test]
    fn render_status_bar_includes_focus_widget_count_and_update_message() {
        let status = render_status_to_string(FocusPosition::new(1, 2), 5, Some("Update ready "));
        assert!(status.contains("5 widgets"));
        assert!(status.contains("Focus: (1,2)"));
        assert!(status.contains("Update ready"));
        assert!(status.contains("q: quit"));
    }

    #[test]
    fn render_widget_handles_unwrapped_text_nonselectable_lists_and_zero_value_charts() {
        let text = render_to_string(
            &WidgetContent::Text {
                content: "Long line".to_string(),
                scrollable: false,
                wrap: false,
            },
            false,
            None,
            20,
            4,
        );
        assert!(text.contains("Long line"));

        let list = render_to_string(
            &WidgetContent::List {
                items: vec![SlateListItem {
                    id: "1".to_string(),
                    title: "Only item".to_string(),
                    subtitle: None,
                    icon: None,
                    style: Default::default(),
                }],
                selectable: false,
                actions: vec![],
            },
            false,
            None,
            20,
            4,
        );
        assert!(list.contains("Only item"));

        let chart = render_to_string(
            &WidgetContent::Chart {
                data: vec![DataPoint {
                    label: "Idle".to_string(),
                    value: 0.0,
                }],
                chart_type: ChartType::Sparkline,
            },
            false,
            None,
            20,
            4,
        );
        assert!(chart.contains("Idle"));
    }

    #[test]
    fn convert_color_supports_all_palette_variants() {
        assert_eq!(convert_color(&Color::Red), RatColor::Red);
        assert_eq!(convert_color(&Color::Yellow), RatColor::Yellow);
        assert_eq!(convert_color(&Color::Blue), RatColor::Blue);
        assert_eq!(convert_color(&Color::Magenta), RatColor::Magenta);
        assert_eq!(convert_color(&Color::Cyan), RatColor::Cyan);
        assert_eq!(convert_color(&Color::White), RatColor::White);
        assert_eq!(convert_color(&Color::Gray), RatColor::Gray);
    }
}
