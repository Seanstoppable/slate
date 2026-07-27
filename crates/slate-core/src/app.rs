use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend, layout::Constraint, layout::Direction, layout::Layout, Terminal,
};
use slate_plugin_sdk::{BoxedWidget, WidgetAction, WidgetContent, WidgetMetadata};

use crate::config::SlateConfig;
use crate::layout::{compute_grid, FocusPosition};
use crate::notifications::UpdateNotifications;
use crate::render::{render_status_bar, render_widget};

/// A running widget instance with its state.
struct WidgetInstance {
    widget: BoxedWidget,
    metadata: WidgetMetadata,
    content: WidgetContent,
    row: u16,
    col: u16,
    last_refresh: Instant,
    refresh_interval: Duration,
    /// Selected index for list widgets
    selected: Option<usize>,
    /// Detail view content (replaces normal rendering, suppresses refresh)
    detail_content: Option<String>,
}

/// The main Slate application.
pub struct App {
    config: SlateConfig,
    widgets: Vec<WidgetInstance>,
    focus: FocusPosition,
    running: bool,
    notifications: UpdateNotifications,
}

impl App {
    pub fn new(config: SlateConfig) -> Self {
        let notifications = UpdateNotifications::load();
        Self {
            config,
            widgets: Vec::new(),
            focus: FocusPosition::new(0, 0),
            running: true,
            notifications,
        }
    }

    /// Register a widget into the application.
    pub fn add_widget(
        &mut self,
        mut widget: BoxedWidget,
        row: u16,
        col: u16,
        refresh_interval: Option<u64>,
    ) {
        let metadata = widget.metadata();
        let interval = refresh_interval.unwrap_or(self.config.global.refresh_interval);
        let content = widget.refresh();
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
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(interval),
            selected,
            detail_content: None,
        });
    }

    /// Set update notification state (called before run).
    pub fn set_notifications(&mut self, notifications: UpdateNotifications) {
        self.notifications = notifications;
    }

    /// Run the main TUI loop.
    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.main_loop(&mut terminal);

        // Restore terminal state fully
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            crossterm::cursor::Show,
            LeaveAlternateScreen
        )?;
        terminal.show_cursor()?;

        result
    }

    fn main_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        while self.running {
            // Refresh widgets that are due (skip focused list widgets to avoid disrupting navigation)
            let now = Instant::now();
            for instance in &mut self.widgets {
                if now.duration_since(instance.last_refresh) >= instance.refresh_interval {
                    // Don't auto-refresh a focused selectable list (user is navigating)
                    let is_focused =
                        instance.row == self.focus.row && instance.col == self.focus.col;
                    if is_focused && instance.content.is_selectable_list() {
                        continue;
                    }
                    // Don't auto-refresh while showing detail view
                    if instance.detail_content.is_some() {
                        continue;
                    }
                    instance.content = instance.widget.refresh();
                    instance.last_refresh = now;
                    // Initialize selection for new list content
                    if instance.content.is_selectable_list() && instance.selected.is_none() {
                        instance.selected = Some(0);
                    }
                }
            }

            // Draw
            terminal.draw(|frame| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)])
                    .split(frame.area());

                let main_area = chunks[0];
                let status_area = chunks[1];

                let grid =
                    compute_grid(main_area, self.config.layout.rows, self.config.layout.cols);

                for instance in &self.widgets {
                    let row = instance.row as usize;
                    let col = instance.col as usize;
                    if row < grid.len() && col < grid[row].len() {
                        let area = grid[row][col];
                        let focused =
                            self.focus.row == instance.row && self.focus.col == instance.col;

                        // Show detail view if set, otherwise normal content
                        if let Some(detail) = &instance.detail_content {
                            let detail_widget_content = WidgetContent::Text {
                                content: detail.clone(),
                                scrollable: true,
                                wrap: true,
                            };
                            render_widget(
                                frame,
                                area,
                                &detail_widget_content,
                                &instance.metadata,
                                focused,
                                None,
                            );
                        } else {
                            render_widget(
                                frame,
                                area,
                                &instance.content,
                                &instance.metadata,
                                focused,
                                instance.selected,
                            );
                        }
                    }
                }

                render_status_bar(
                    frame,
                    status_area,
                    &self.focus,
                    self.widgets.len(),
                    self.notifications.status_message().as_deref(),
                );
            })?;

            // Handle input (poll with timeout for refresh)
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    // Only handle key press events (ignore release/repeat)
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key);
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // If showing detail view, Escape dismisses it
        let focused_showing_detail = self
            .focused_widget()
            .map(|w| w.detail_content.is_some())
            .unwrap_or(false);

        if focused_showing_detail {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    if let Some(instance) = self.focused_widget_mut() {
                        instance.detail_content = None;
                    }
                    return;
                }
                _ => return, // Ignore all other keys while in detail view
            }
        }

        // Check if focused widget is a selectable list
        let focused_is_list = self
            .focused_widget()
            .map(|w| w.content.is_selectable_list())
            .unwrap_or(false);

        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            KeyCode::Tab => {
                // Move focus to next widget in reading order
                self.focus
                    .move_next(self.config.layout.rows, self.config.layout.cols);
            }
            KeyCode::BackTab => {
                // Move focus to previous widget
                self.focus
                    .move_prev(self.config.layout.rows, self.config.layout.cols);
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.focus.move_left(self.config.layout.cols);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.focus.move_right(self.config.layout.cols);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if focused_is_list {
                    if let Some(instance) = self.focused_widget_mut() {
                        if let Some(sel) = &mut instance.selected {
                            if *sel > 0 {
                                *sel -= 1;
                            }
                        }
                    }
                } else {
                    self.focus.move_up(self.config.layout.rows);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if focused_is_list {
                    if let Some(instance) = self.focused_widget_mut() {
                        let max = instance.content.list_len().saturating_sub(1);
                        if let Some(sel) = &mut instance.selected {
                            if *sel < max {
                                *sel += 1;
                            }
                        }
                    }
                } else {
                    self.focus.move_down(self.config.layout.rows);
                }
            }
            KeyCode::Enter => {
                // Forward to focused widget with selected item
                if let Some(instance) = self.focused_widget_mut() {
                    if let (Some(sel), WidgetContent::List { items, .. }) =
                        (instance.selected, &instance.content)
                    {
                        if let Some(item) = items.get(sel) {
                            let item_id = item.id.clone();
                            if let Some(action) = instance.widget.on_action("select", &item_id) {
                                match action {
                                    WidgetAction::ShowDetail(detail) => {
                                        instance.detail_content = Some(detail);
                                    }
                                    other => Self::handle_widget_action(other),
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Char('r') => {
                // Force refresh focused widget
                if let Some(instance) = self.focused_widget_mut() {
                    instance.content = instance.widget.refresh();
                    instance.last_refresh = Instant::now();
                    // Reset selection if list changed
                    if instance.content.is_selectable_list() {
                        instance.selected = Some(0);
                    }
                }
            }
            _ => {
                // Forward other keys to focused widget
                if let Some(instance) = self.focused_widget_mut() {
                    let key_str = format!("{:?}", key.code);
                    instance.widget.on_key(&key_str, "");
                }
            }
        }
    }

    fn handle_widget_action(action: WidgetAction) {
        match action {
            WidgetAction::OpenUrl(url) => {
                // Open URL in system browser
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", "start", &url])
                        .spawn();
                }
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg(&url).spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                }
            }
            WidgetAction::Notify(msg) => {
                // For now, just log it
                tracing::info!("Widget notification: {}", msg);
            }
            WidgetAction::ShowDetail(_) => {
                // Handled directly at the call site (sets detail_content on instance)
                unreachable!("ShowDetail should be handled before calling handle_widget_action");
            }
        }
    }

    fn focused_widget(&self) -> Option<&WidgetInstance> {
        self.widgets
            .iter()
            .find(|w| w.row == self.focus.row && w.col == self.focus.col)
    }

    fn focused_widget_mut(&mut self) -> Option<&mut WidgetInstance> {
        self.widgets
            .iter_mut()
            .find(|w| w.row == self.focus.row && w.col == self.focus.col)
    }
}
