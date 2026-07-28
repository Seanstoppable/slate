use chrono::Local;
use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};

pub(crate) struct DigitalClockWidget;

impl DigitalClockWidget {
    pub(crate) fn new() -> Self {
        Self
    }
}

const DIGITS: [&[&str]; 10] = [
    &["┌─┐", "│ │", "│ │", "│ │", "└─┘"], // 0
    &["  ╷", "  │", "  │", "  │", "  ╵"], // 1
    &["┌─┐", "  │", "┌─┘", "│  ", "└─┘"], // 2
    &["┌─┐", "  │", " ─┤", "  │", "└─┘"], // 3
    &["╷ ╷", "│ │", "└─┤", "  │", "  ╵"], // 4
    &["┌─┐", "│  ", "└─┐", "  │", "└─┘"], // 5
    &["┌─┐", "│  ", "├─┐", "│ │", "└─┘"], // 6
    &["┌─┐", "  │", "  │", "  │", "  ╵"], // 7
    &["┌─┐", "│ │", "├─┤", "│ │", "└─┘"], // 8
    &["┌─┐", "│ │", "└─┤", "  │", "└─┘"], // 9
];

const COLON: &[&str] = &["   ", " ● ", "   ", " ● ", "   "];

fn render_digital_time(hour: u32, min: u32, sec: u32) -> String {
    let parts: Vec<&[&str]> = vec![
        DIGITS[(hour / 10) as usize],
        DIGITS[(hour % 10) as usize],
        COLON,
        DIGITS[(min / 10) as usize],
        DIGITS[(min % 10) as usize],
        COLON,
        DIGITS[(sec / 10) as usize],
        DIGITS[(sec % 10) as usize],
    ];

    let mut lines = Vec::new();
    for row in 0..5 {
        let line: Vec<&str> = parts.iter().map(|p| p[row]).collect();
        lines.push(line.join(" "));
    }
    lines.join("\n")
}

impl slate_plugin_sdk::Widget for DigitalClockWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Digital Clock".to_string(),
            description: "Large ASCII digit clock".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let now = Local::now();
        let time_art = render_digital_time(
            now.format("%H").to_string().parse().unwrap_or(0),
            now.format("%M").to_string().parse().unwrap_or(0),
            now.format("%S").to_string().parse().unwrap_or(0),
        );
        let date_line = now.format("  %A, %B %d, %Y").to_string();

        WidgetContent::Text {
            content: format!("{}\n\n{}", time_art, date_line),
            scrollable: false,
            wrap: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::Widget;

    #[test]
    fn digital_clock_renders_text_content() {
        let mut widget = DigitalClockWidget::new();
        let content = widget.refresh();
        match content {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains('●')); // colon separators
                assert!(content.lines().count() >= 5); // at least 5 lines for digits
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn render_digital_time_formats_correctly() {
        let output = render_digital_time(12, 30, 45);
        let lines: Vec<_> = output.lines().collect();
        assert_eq!(lines.len(), 5);
        // Each line should have content for 8 parts (6 digits + 2 colons)
        assert!(lines[0].len() > 20);
    }
}
