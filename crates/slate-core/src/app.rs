use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, layout::Constraint, layout::Direction, layout::Layout, Terminal};
use slate_plugin_sdk::{BoxedWidget, WidgetContent, WidgetMetadata};

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
    pub fn add_widget(&mut self, mut widget: BoxedWidget, row: u16, col: u16, refresh_interval: Option<u64>) {
        let metadata = widget.metadata();
        let interval = refresh_interval.unwrap_or(self.config.global.refresh_interval);
        let content = widget.refresh();
        self.widgets.push(WidgetInstance {
            widget,
            metadata,
            content,
            row,
            col,
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(interval),
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
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.main_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    fn main_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        while self.running {
            // Refresh widgets that are due
            let now = Instant::now();
            for instance in &mut self.widgets {
                if now.duration_since(instance.last_refresh) >= instance.refresh_interval {
                    instance.content = instance.widget.refresh();
                    instance.last_refresh = now;
                }
            }

            // Draw
            terminal.draw(|frame| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(1),
                        Constraint::Length(1),
                    ])
                    .split(frame.area());

                let main_area = chunks[0];
                let status_area = chunks[1];

                let grid = compute_grid(main_area, self.config.layout.rows, self.config.layout.cols);

                for instance in &self.widgets {
                    let row = instance.row as usize;
                    let col = instance.col as usize;
                    if row < grid.len() && col < grid[row].len() {
                        let area = grid[row][col];
                        let focused = self.focus.row == instance.row && self.focus.col == instance.col;
                        render_widget(frame, area, &instance.content, &instance.metadata, focused);
                    }
                }

                render_status_bar(frame, status_area, &self.focus, self.widgets.len(), self.notifications.status_message().as_deref());
            })?;

            // Handle input (poll with timeout for refresh)
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            KeyCode::Tab => {
                // Move focus to next widget
                self.focus.move_right(self.config.layout.cols);
                if self.focus.col == 0 {
                    self.focus.move_down(self.config.layout.rows);
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.focus.move_left(self.config.layout.cols);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.focus.move_right(self.config.layout.cols);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.focus.move_up(self.config.layout.rows);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.focus.move_down(self.config.layout.rows);
            }
            KeyCode::Enter => {
                // Forward to focused widget
                if let Some(instance) = self.focused_widget_mut() {
                    instance.widget.on_key("enter", "select");
                    instance.content = instance.widget.refresh();
                }
            }
            KeyCode::Char('r') => {
                // Force refresh focused widget
                if let Some(instance) = self.focused_widget_mut() {
                    instance.content = instance.widget.refresh();
                    instance.last_refresh = Instant::now();
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

    fn focused_widget_mut(&mut self) -> Option<&mut WidgetInstance> {
        self.widgets
            .iter_mut()
            .find(|w| w.row == self.focus.row && w.col == self.focus.col)
    }
}
