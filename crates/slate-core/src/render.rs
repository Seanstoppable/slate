use ratatui::{
    backend::TestBackend,
    layout::Rect,
    style::{Color as RatColor, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table,
        TableState, Wrap,
    },
    Frame, Terminal,
};
use slate_plugin_sdk::{Action, Color, WidgetContent, WidgetMetadata};

use crate::config::ConfigWarning;
use crate::keybindings::{action_has_reserved_key, HOST_KEYBINDINGS};
use crate::layout::FocusPosition;

/// Render a widget's content to a self-contained HTML snippet, preserving the same
/// colors/styles the widget would show in the real terminal UI. Used to generate
/// "live" widget snapshots for the plugin registry documentation.
pub fn render_snapshot_html(
    content: &WidgetContent,
    metadata: &WidgetMetadata,
    width: u16,
    height: u16,
) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend terminal");
    terminal
        .draw(|frame| {
            render_widget(
                frame,
                Rect::new(0, 0, width, height),
                content,
                metadata,
                false,
                None,
                None,
            );
        })
        .expect("draw snapshot");

    let buffer = terminal.backend().buffer();
    let mut html = String::from("<pre class=\"snapshot\">");
    for y in 0..height {
        if y > 0 {
            html.push('\n');
        }
        for x in 0..width {
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            let escaped = html_escape_char(symbol);
            let mut style_parts = Vec::new();
            if let Some(css) = rat_color_to_css_fg(cell.fg) {
                style_parts.push(format!("color:{}", css));
            }
            if let Some(css) = rat_color_to_css_bg(cell.bg) {
                style_parts.push(format!("background:{}", css));
            }
            if cell.modifier.contains(Modifier::BOLD) {
                style_parts.push("font-weight:bold".to_string());
            }
            if cell.modifier.contains(Modifier::ITALIC) {
                style_parts.push("font-style:italic".to_string());
            }
            if style_parts.is_empty() {
                html.push_str(&escaped);
            } else {
                html.push_str(&format!(
                    "<span style=\"{}\">{}</span>",
                    style_parts.join(";"),
                    escaped
                ));
            }
        }
    }
    html.push_str("</pre>");
    html
}

fn html_escape_char(s: &str) -> String {
    match s {
        "&" => "&amp;".to_string(),
        "<" => "&lt;".to_string(),
        ">" => "&gt;".to_string(),
        " " => "&nbsp;".to_string(),
        "" => "&nbsp;".to_string(),
        other => other.to_string(),
    }
}

fn rat_color_to_css_fg(color: RatColor) -> Option<String> {
    rat_color_to_css(color, "#e6edf3")
}

fn rat_color_to_css_bg(color: RatColor) -> Option<String> {
    rat_color_to_css(color, "#0d1117")
}

/// Maps a ratatui color to a CSS color string, skipping the given default so we
/// don't emit redundant inline styles for the common case.
fn rat_color_to_css(color: RatColor, default: &str) -> Option<String> {
    let css = match color {
        RatColor::Reset => return None,
        RatColor::Black => "#484f58",
        RatColor::Red => "#f85149",
        RatColor::Green => "#3fb950",
        RatColor::Yellow => "#d29922",
        RatColor::Blue => "#58a6ff",
        RatColor::Magenta => "#bc8cff",
        RatColor::Cyan => "#39c5cf",
        RatColor::Gray | RatColor::White => "#e6edf3",
        RatColor::DarkGray => "#8b949e",
        RatColor::LightRed => "#ff7b72",
        RatColor::LightGreen => "#56d364",
        RatColor::LightYellow => "#e3b341",
        RatColor::LightBlue => "#79c0ff",
        RatColor::LightMagenta => "#d2a8ff",
        RatColor::LightCyan => "#76e3ea",
        RatColor::Rgb(r, g, b) => return Some(format!("#{:02x}{:02x}{:02x}", r, g, b)),
        RatColor::Indexed(_) => return None,
    };
    if css == default {
        None
    } else {
        Some(css.to_string())
    }
}

const SLATE_SURFACE: RatColor = RatColor::Rgb(15, 23, 42);
const SLATE_STATUS_BG: RatColor = RatColor::Rgb(2, 6, 23);
const SLATE_BORDER: RatColor = RatColor::Rgb(71, 85, 105);
const SLATE_BORDER_FOCUSED: RatColor = RatColor::Rgb(203, 213, 225);
const SLATE_TEXT: RatColor = RatColor::Rgb(226, 232, 240);
const SLATE_MUTED: RatColor = RatColor::Rgb(148, 163, 184);
const SLATE_SELECTION_BG: RatColor = RatColor::Rgb(51, 65, 85);
const SLATE_CHART: RatColor = RatColor::Rgb(125, 211, 252);

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
    render_widget_with_scroll(
        frame,
        area,
        content,
        metadata,
        focused,
        selected,
        border_color,
        0,
    );
}

/// Render a widget's content with a vertical text scroll offset.
pub fn render_widget_with_scroll(
    frame: &mut Frame,
    area: Rect,
    content: &WidgetContent,
    metadata: &WidgetMetadata,
    focused: bool,
    selected: Option<usize>,
    border_color: Option<&Color>,
    text_scroll: u16,
) {
    let border_style = if focused {
        Style::default().fg(SLATE_BORDER_FOCUSED)
    } else if let Some(color) = border_color {
        Style::default().fg(convert_color(color))
    } else {
        Style::default().fg(SLATE_BORDER)
    };

    // Focused widgets get a double-line border so it's unmistakable which
    // widget is currently receiving keypresses, even at a glance.
    let border_type = if focused {
        BorderType::Double
    } else {
        BorderType::Rounded
    };

    let block = Block::default()
        .title(format!(" {} ", metadata.name))
        .title_style(Style::default().fg(SLATE_TEXT).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(border_style)
        .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    match content {
        WidgetContent::Text { content, wrap, .. } => {
            let paragraph = Paragraph::new(content.as_str())
                .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT));
            let paragraph = if *wrap {
                paragraph.wrap(Wrap { trim: true })
            } else {
                paragraph
            };
            frame.render_widget(paragraph.scroll((text_scroll, 0)), inner);
        }
        WidgetContent::Table {
            headers,
            rows,
            selectable,
        } => {
            let header_cells: Vec<Span> = headers
                .iter()
                .map(|h| {
                    Span::styled(
                        h.as_str(),
                        Style::default().fg(SLATE_TEXT).add_modifier(Modifier::BOLD),
                    )
                })
                .collect();
            let header = Row::new(header_cells).style(
                Style::default()
                    .fg(SLATE_TEXT)
                    .add_modifier(Modifier::UNDERLINED),
            );

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

            let table = Table::new(table_rows, widths)
                .header(header)
                .column_spacing(2)
                .row_highlight_style(
                    Style::default()
                        .fg(SLATE_TEXT)
                        .bg(SLATE_SELECTION_BG)
                        .add_modifier(Modifier::BOLD),
                )
                .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT));

            if *selectable && selected.is_some() {
                let mut state = TableState::default().with_selected(selected);
                frame.render_stateful_widget(table, inner, &mut state);
            } else {
                frame.render_widget(table, inner);
            }
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
            let paragraph =
                Paragraph::new(lines).style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT));
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
                                Style::default().fg(SLATE_TEXT).add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" "),
                            Span::styled(subtitle.as_str(), Style::default().fg(SLATE_MUTED)),
                        ])
                    } else {
                        Line::from(Span::styled(
                            item.title.as_str(),
                            Style::default().fg(SLATE_TEXT),
                        ))
                    };
                    ListItem::new(content)
                })
                .collect();
            let list = List::new(list_items)
                .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT))
                .highlight_style(
                    Style::default()
                        .fg(SLATE_TEXT)
                        .bg(SLATE_SELECTION_BG)
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
                            Style::default().fg(SLATE_MUTED),
                        ),
                        Span::styled(bar, Style::default().fg(SLATE_CHART)),
                    ])
                })
                .collect();
            let paragraph =
                Paragraph::new(lines).style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT));
            frame.render_widget(paragraph, inner);
        }
        WidgetContent::Empty { message } => {
            let paragraph = Paragraph::new(message.as_str())
                .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_MUTED));
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
    config_warnings: usize,
) {
    let update_part = update_msg.unwrap_or("");
    let warning_part = if config_warnings > 0 {
        format!(
            "w: ⚠ {} config warning{} │ ",
            config_warnings,
            if config_warnings == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };
    let status = format!(
        " Slate │ {} widgets │ Focus: ({},{}) │ {}{}?: help │ q: quit │ Tab: next │ ←↑↓→: navigate ",
        widget_count, focus.row, focus.col, warning_part, update_part
    );
    let paragraph =
        Paragraph::new(status).style(Style::default().bg(SLATE_STATUS_BG).fg(SLATE_TEXT));
    frame.render_widget(paragraph, area);
}

pub fn render_input_bar(frame: &mut Frame, area: Rect, prompt: &str, buffer: &str) {
    let text = format!("{}: {}_", prompt, buffer);
    let paragraph =
        Paragraph::new(text).style(Style::default().fg(RatColor::Yellow).bg(RatColor::DarkGray));
    frame.render_widget(paragraph, area);
}

/// Render an overlay describing the focused widget and its available list actions.
pub fn render_widget_help_modal(
    frame: &mut Frame,
    area: Rect,
    metadata: &WidgetMetadata,
    actions: &[Action],
) {
    let mut lines = vec![
        Line::styled(
            "Description",
            Style::default().fg(SLATE_TEXT).add_modifier(Modifier::BOLD),
        ),
        Line::from(if metadata.description.trim().is_empty() {
            "No description available."
        } else {
            metadata.description.as_str()
        }),
        Line::default(),
        Line::styled(
            "Keybindings",
            Style::default().fg(SLATE_TEXT).add_modifier(Modifier::BOLD),
        ),
    ];
    lines.extend(
        HOST_KEYBINDINGS
            .iter()
            .map(|(key, label)| keybinding_line(key, label)),
    );

    let keybindings: Vec<&Action> = actions
        .iter()
        .filter(|action| action.key.is_some() && !action_has_reserved_key(action))
        .collect();
    if keybindings.is_empty() {
        lines.push(Line::styled(
            "No widget-specific keybindings available.",
            Style::default().fg(SLATE_MUTED),
        ));
    } else {
        lines.extend(keybindings.into_iter().map(|action| {
            keybinding_line(action.key.as_deref().unwrap_or_default(), &action.label)
        }));
    }
    lines.push(Line::default());
    lines.push(Line::styled(
        "Esc, ?, or q to close",
        Style::default().fg(SLATE_MUTED),
    ));

    let width = area.width.min(72);
    let height = area.height.min((lines.len() as u16).saturating_add(2));
    let popup_area = Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    );
    let block = Block::default()
        .title(format!(" {} Help ", metadata.name))
        .title_style(Style::default().fg(SLATE_TEXT).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SLATE_BORDER_FOCUSED))
        .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT));
    let inner = block.inner(popup_area);

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

/// Render an overlay listing non-fatal configuration problems.
pub fn render_config_warnings_modal(frame: &mut Frame, area: Rect, warnings: &[ConfigWarning]) {
    let mut lines = vec![Line::styled(
        "These widgets will not render as configured.",
        Style::default().fg(SLATE_MUTED),
    )];
    lines.push(Line::default());

    if warnings.is_empty() {
        lines.push(Line::styled(
            "No configuration warnings.",
            Style::default().fg(SLATE_MUTED),
        ));
    } else {
        for warning in warnings {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("widget #{}", warning.widget_index() + 1),
                    Style::default()
                        .fg(RatColor::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(": {}", warning), Style::default().fg(SLATE_TEXT)),
            ]));
        }
    }

    lines.push(Line::default());
    lines.push(Line::styled(
        "Run `slate check` for full validation. Esc, w, or q to close",
        Style::default().fg(SLATE_MUTED),
    ));

    let width = area.width.min(72);
    // Warnings wrap, so reserve two rendered rows per entry.
    let estimated_rows = lines.len().saturating_add(warnings.len()) as u16;
    let height = area.height.min(estimated_rows.saturating_add(2));
    let popup_area = Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    );
    let block = Block::default()
        .title(" Config Warnings ")
        .title_style(
            Style::default()
                .fg(RatColor::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(RatColor::Yellow))
        .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT));
    let inner = block.inner(popup_area);

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(SLATE_SURFACE).fg(SLATE_TEXT))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn keybinding_line(key: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{key:<12}"),
            Style::default()
                .fg(SLATE_CHART)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(label.to_string()),
    ])
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

    fn render_warnings_modal_to_string(warnings: &[ConfigWarning]) -> String {
        let (width, height) = (80u16, 20u16);
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_config_warnings_modal(frame, Rect::new(0, 0, width, height), warnings);
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

    #[test]
    fn render_config_warnings_modal_lists_each_warning() {
        let warnings = vec![
            ConfigWarning::OutOfBounds {
                index: 2,
                widget_type: "builtin:power".to_string(),
                row: 9,
                col: 0,
                rows: 5,
                cols: 3,
            },
            ConfigWarning::ZeroSpan {
                index: 4,
                widget_type: "builtin:clock".to_string(),
                row_span: 0,
                col_span: 1,
            },
        ];
        let rendered = render_warnings_modal_to_string(&warnings);

        assert!(rendered.contains("Config Warnings"), "got: {rendered}");
        // 1-based numbering matches `slate check` output.
        assert!(rendered.contains("widget #3"), "got: {rendered}");
        assert!(rendered.contains("widget #5"), "got: {rendered}");
        assert!(rendered.contains("builtin:power"), "got: {rendered}");
        assert!(rendered.contains("slate check"), "got: {rendered}");
    }

    #[test]
    fn render_config_warnings_modal_handles_empty_list() {
        let rendered = render_warnings_modal_to_string(&[]);
        assert!(
            rendered.contains("No configuration warnings"),
            "got: {rendered}"
        );
    }

    fn render_status_to_string(
        focus: FocusPosition,
        widget_count: usize,
        update: Option<&str>,
    ) -> String {
        render_status_to_string_with_warnings(focus, widget_count, update, 0)
    }

    fn render_status_to_string_with_warnings(
        focus: FocusPosition,
        widget_count: usize,
        update: Option<&str>,
        config_warnings: usize,
    ) -> String {
        let backend = TestBackend::new(80, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_status_bar(
                    frame,
                    Rect::new(0, 1, 80, 1),
                    &focus,
                    widget_count,
                    update,
                    config_warnings,
                );
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
    fn render_widget_keeps_selected_table_row_visible() {
        let rendered = render_to_string(
            &WidgetContent::Table {
                headers: vec!["Name".to_string()],
                rows: (0..8)
                    .map(|index| vec![Cell::plain(format!("Row {index}"))])
                    .collect(),
                selectable: true,
            },
            false,
            Some(6),
            30,
            6,
        );

        assert!(rendered.contains("Row 6"), "got: {rendered}");
        assert!(!rendered.contains("Row 0"), "got: {rendered}");
    }

    #[test]
    fn render_status_bar_shows_config_warning_count() {
        let one = render_status_to_string_with_warnings(FocusPosition::new(0, 0), 2, None, 1);
        assert!(one.contains("w: ⚠ 1 config warning"), "got: {one}");
        assert!(!one.contains("warnings"), "should be singular, got: {one}");

        let many = render_status_to_string_with_warnings(FocusPosition::new(0, 0), 2, None, 3);
        assert!(many.contains("w: ⚠ 3 config warnings"), "got: {many}");

        let none = render_status_to_string_with_warnings(FocusPosition::new(0, 0), 2, None, 0);
        assert!(!none.contains("config warning"), "got: {none}");
    }

    #[test]
    fn render_status_bar_includes_focus_widget_count_and_update_message() {
        let status = render_status_to_string(FocusPosition::new(1, 2), 5, Some("Update ready "));
        assert!(status.contains("5 widgets"));
        assert!(status.contains("Focus: (1,2)"));
        assert!(status.contains("Update ready"));
        assert!(status.contains("?: help"));
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

    #[test]
    fn render_input_bar_shows_prompt_and_buffer_with_cursor() {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_input_bar(frame, Rect::new(0, 0, 40, 1), "Add todo", "my task");
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for x in 0..40 {
            rendered.push_str(buffer[(x, 0)].symbol());
        }
        assert!(rendered.contains("Add todo"));
        assert!(rendered.contains("my task"));
        assert!(rendered.contains('_'));
    }

    #[test]
    fn render_widget_help_modal_shows_description_and_list_action_keys() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let actions = vec![
            Action {
                id: "refresh".to_string(),
                label: "Override refresh".to_string(),
                key: Some("r".to_string()),
                confirm: false,
            },
            Action {
                id: "open".to_string(),
                label: "Open selected item".to_string(),
                key: Some("o".to_string()),
                confirm: false,
            },
        ];

        terminal
            .draw(|frame| {
                render_widget_help_modal(frame, frame.area(), &metadata(), &actions);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..20 {
            for x in 0..80 {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(rendered.contains("Widget Help"));
        assert!(rendered.contains("Description"));
        assert!(rendered.contains("Rendered widget"));
        assert!(rendered.contains("Keybindings"));
        assert!(rendered.contains("Refresh widget"));
        assert!(rendered.contains("Open selected item"));
        assert!(!rendered.contains("Override refresh"));
        assert!(rendered.contains("Esc, ?, or q to close"));
    }
}
