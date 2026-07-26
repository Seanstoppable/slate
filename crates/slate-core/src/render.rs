use ratatui::{
    layout::Rect,
    style::{Color as RatColor, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table, Wrap},
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
) {
    let border_style = if focused {
        Style::default().fg(RatColor::Cyan)
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
        WidgetContent::Text {
            content, wrap, ..
        } => {
            let paragraph = Paragraph::new(content.as_str());
            let paragraph = if *wrap {
                paragraph.wrap(Wrap { trim: true })
            } else {
                paragraph
            };
            frame.render_widget(paragraph, inner);
        }
        WidgetContent::Table {
            headers, rows, ..
        } => {
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
        WidgetContent::List { items, .. } => {
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
            let list = List::new(list_items);
            frame.render_widget(list, inner);
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
            let paragraph = Paragraph::new(message.as_str())
                .style(Style::default().fg(RatColor::DarkGray));
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
    let paragraph = Paragraph::new(status)
        .style(Style::default().bg(RatColor::DarkGray).fg(RatColor::White));
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
