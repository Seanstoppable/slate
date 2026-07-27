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

impl WidgetInstance {
    fn should_refresh(&self, now: Instant, focus: &FocusPosition) -> bool {
        if now.duration_since(self.last_refresh) < self.refresh_interval {
            return false;
        }

        let is_focused = self.row == focus.row && self.col == focus.col;
        if is_focused && self.content.is_selectable_list() {
            return false;
        }

        self.detail_content.is_none()
    }
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
                if instance.should_refresh(now, &self.focus) {
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
                // Open URL in system browser (skip during tests)
                #[cfg(not(test))]
                {
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
                #[cfg(test)]
                {
                    let _ = url; // suppress unused warning
                    tracing::debug!("OpenUrl suppressed in test: {}", "...");
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

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::{Widget, WidgetConfig, WidgetContent, WidgetMetadata};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    /// A mock widget that returns a selectable list and responds to on_action.
    struct MockListWidget {
        action_response: Option<WidgetAction>,
        refresh_count: std::cell::Cell<u32>,
    }

    impl MockListWidget {
        fn new(action_response: Option<WidgetAction>) -> Self {
            Self {
                action_response,
                refresh_count: std::cell::Cell::new(0),
            }
        }
    }

    impl Widget for MockListWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Mock List".to_string(),
                description: "Test widget".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            self.refresh_count.set(self.refresh_count.get() + 1);
            WidgetContent::List {
                items: vec![
                    slate_plugin_sdk::ListItem {
                        id: "item-1".to_string(),
                        title: "First Item".to_string(),
                        subtitle: Some("subtitle".to_string()),
                        icon: None,
                        style: Default::default(),
                    },
                    slate_plugin_sdk::ListItem {
                        id: "item-2".to_string(),
                        title: "Second Item".to_string(),
                        subtitle: None,
                        icon: None,
                        style: Default::default(),
                    },
                ],
                selectable: true,
                actions: vec![],
            }
        }

        fn on_action(&mut self, _action_id: &str, _item_id: &str) -> Option<WidgetAction> {
            self.action_response.clone()
        }
    }

    /// A simple text widget that never responds to actions.
    struct MockTextWidget;

    struct CounterTextWidget {
        refresh_count: Arc<AtomicUsize>,
    }

    impl CounterTextWidget {
        fn new(refresh_count: Arc<AtomicUsize>) -> Self {
            Self { refresh_count }
        }
    }

    impl Widget for CounterTextWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Counter Text".to_string(),
                description: "Counts refreshes".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            let count = self.refresh_count.fetch_add(1, Ordering::SeqCst) + 1;
            WidgetContent::Text {
                content: format!("Refresh #{count}"),
                scrollable: false,
                wrap: true,
            }
        }
    }

    impl Widget for MockTextWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Mock Text".to_string(),
                description: "".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            WidgetContent::Text {
                content: "Hello".to_string(),
                scrollable: false,
                wrap: true,
            }
        }
    }

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn test_app_with_list_widget(action: Option<WidgetAction>) -> App {
        let config = SlateConfig::default();
        let mut app = App::new(config);
        let widget = MockListWidget::new(action);
        app.add_widget(Box::new(widget), 0, 0, Some(300));
        app
    }

    #[test]
    fn add_widget_adds_widget_and_sets_initial_content() {
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(CounterTextWidget::new(refresh_count.clone())),
            1,
            1,
            Some(60),
        );

        assert_eq!(app.widgets.len(), 1);
        assert_eq!(refresh_count.load(Ordering::SeqCst), 1);
        assert_eq!(app.widgets[0].row, 1);
        assert_eq!(app.widgets[0].col, 1);
        match &app.widgets[0].content {
            WidgetContent::Text { content, .. } => assert_eq!(content, "Refresh #1"),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn focused_widget_returns_widget_at_focus_position() {
        let mut config = SlateConfig::default();
        config.layout.rows = 1;
        config.layout.cols = 2;
        let mut app = App::new(config);
        app.add_widget(Box::new(MockTextWidget), 0, 0, Some(60));
        app.add_widget(Box::new(MockListWidget::new(None)), 0, 1, Some(60));

        assert_eq!(app.focused_widget().map(|widget| widget.metadata.name.as_str()), Some("Mock Text"));

        app.focus.col = 1;
        assert_eq!(app.focused_widget().map(|widget| widget.metadata.name.as_str()), Some("Mock List"));
    }

    #[test]
    fn should_refresh_returns_true_after_enough_time_has_passed() {
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(CounterTextWidget::new(refresh_count)),
            0,
            0,
            Some(1),
        );

        let now = Instant::now();
        app.widgets[0].last_refresh = now - Duration::from_secs(2);

        assert!(app.widgets[0].should_refresh(now, &app.focus));
    }

    #[test]
    fn should_refresh_returns_false_during_detail_view() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));
        app.widgets[0].detail_content = Some("Details".to_string());
        let now = Instant::now();
        app.widgets[0].last_refresh = now - Duration::from_secs(600);

        assert!(!app.widgets[0].should_refresh(now, &app.focus));
    }

    #[test]
    fn enter_on_list_with_show_detail_sets_detail_content() {
        let mut app = test_app_with_list_widget(Some(WidgetAction::ShowDetail(
            "Detailed info here".to_string(),
        )));

        // Widget starts with no detail
        assert!(app.widgets[0].detail_content.is_none());

        // Press Enter to select
        app.handle_key(make_key(KeyCode::Enter));

        // Detail should now be set
        assert_eq!(
            app.widgets[0].detail_content,
            Some("Detailed info here".to_string())
        );
    }

    #[test]
    fn escape_dismisses_detail_view() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail view
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.widgets[0].detail_content.is_some());

        // Escape should dismiss
        app.handle_key(make_key(KeyCode::Esc));
        assert!(app.widgets[0].detail_content.is_none());
    }

    #[test]
    fn q_dismisses_detail_view_without_quitting() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail view
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.widgets[0].detail_content.is_some());

        // 'q' should dismiss detail, NOT quit the app
        app.handle_key(make_key(KeyCode::Char('q')));
        assert!(app.widgets[0].detail_content.is_none());
        assert!(app.running); // still running
    }

    #[test]
    fn detail_view_is_reported_by_focused_widget() {
        let mut app = test_app_with_list_widget(Some(WidgetAction::ShowDetail(
            "Detailed info here".to_string(),
        )));

        app.handle_key(make_key(KeyCode::Enter));

        assert_eq!(
            app.focused_widget()
                .and_then(|widget| widget.detail_content.as_deref()),
            Some("Detailed info here")
        );
    }

    #[test]
    fn keys_are_ignored_during_detail_view() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail view
        app.handle_key(make_key(KeyCode::Enter));

        // j/k/Tab should be ignored — selection should not change
        let selected_before = app.widgets[0].selected;
        app.handle_key(make_key(KeyCode::Char('j')));
        app.handle_key(make_key(KeyCode::Char('k')));
        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.widgets[0].selected, selected_before);
    }

    #[test]
    fn enter_with_no_action_response_does_not_set_detail() {
        let mut app = test_app_with_list_widget(None);

        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.widgets[0].detail_content.is_none());
    }

    #[test]
    fn enter_with_open_url_does_not_set_detail() {
        let mut app = test_app_with_list_widget(Some(WidgetAction::OpenUrl(
            "https://example.com".to_string(),
        )));

        app.handle_key(make_key(KeyCode::Enter));
        // OpenUrl should NOT set detail_content
        assert!(app.widgets[0].detail_content.is_none());
    }

    #[test]
    fn j_k_navigate_list_selection() {
        let mut app = test_app_with_list_widget(None);

        // Starts at 0
        assert_eq!(app.widgets[0].selected, Some(0));

        // j moves down
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.widgets[0].selected, Some(1));

        // Can't go past end
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.widgets[0].selected, Some(1));

        // k moves up
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.widgets[0].selected, Some(0));

        // Can't go before 0
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.widgets[0].selected, Some(0));
    }

    #[test]
    fn q_quits_when_not_in_detail_view() {
        let mut app = test_app_with_list_widget(None);
        assert!(app.running);

        app.handle_key(make_key(KeyCode::Char('q')));
        assert!(!app.running);
    }

    #[test]
    fn tab_moves_focus_between_widgets() {
        let mut config = SlateConfig::default();
        config.layout.rows = 1;
        config.layout.cols = 2;
        let mut app = App::new(config);
        app.add_widget(Box::new(MockTextWidget), 0, 0, Some(300));
        app.add_widget(Box::new(MockTextWidget), 0, 1, Some(300));

        assert_eq!(app.focus.row, 0);
        assert_eq!(app.focus.col, 0);

        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.focus.col, 1);

        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.focus.col, 0); // wraps around
    }

    #[test]
    fn refresh_is_suppressed_when_detail_is_showing() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.widgets[0].detail_content.is_some());

        // Manually set last_refresh to the past to trigger refresh
        app.widgets[0].last_refresh = Instant::now() - Duration::from_secs(600);

        // The main loop refresh logic checks detail_content — simulate it here
        let instance = &mut app.widgets[0];
        let now = Instant::now();
        assert!(!instance.should_refresh(now, &app.focus));
    }

    #[test]
    fn forced_refresh_clears_detail_and_refreshes() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.widgets[0].detail_content.is_some());

        // Escape to dismiss, then 'r' to refresh
        app.handle_key(make_key(KeyCode::Esc));
        assert!(app.widgets[0].detail_content.is_none());

        // 'r' forces refresh
        app.handle_key(make_key(KeyCode::Char('r')));
        // Widget content should be refreshed (still a list)
        assert!(app.widgets[0].content.is_selectable_list());
    }

    #[test]
    fn forced_refresh_updates_content_and_resets_timer() {
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(CounterTextWidget::new(refresh_count.clone())),
            0,
            0,
            Some(60),
        );

        let previous_refresh = Instant::now() - Duration::from_secs(600);
        app.widgets[0].last_refresh = previous_refresh;

        app.handle_key(make_key(KeyCode::Char('r')));

        assert_eq!(refresh_count.load(Ordering::SeqCst), 2);
        assert!(app.widgets[0].last_refresh > previous_refresh);
        match &app.widgets[0].content {
            WidgetContent::Text { content, .. } => assert_eq!(content, "Refresh #2"),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn handle_key_with_no_widgets_does_not_panic() {
        let mut app = App::new(SlateConfig::default());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            app.handle_key(make_key(KeyCode::Enter));
            app.handle_key(make_key(KeyCode::Char('r')));
            app.handle_key(make_key(KeyCode::Left));
            app.handle_key(make_key(KeyCode::Esc));
        }));

        assert!(result.is_ok());
    }

    #[test]
    fn navigation_stays_within_grid_bounds() {
        let mut config = SlateConfig::default();
        config.layout.rows = 2;
        config.layout.cols = 2;
        let mut app = App::new(config);

        app.handle_key(make_key(KeyCode::Left));
        app.handle_key(make_key(KeyCode::Up));
        assert_eq!((app.focus.row, app.focus.col), (0, 0));

        app.focus.row = 1;
        app.focus.col = 1;
        app.handle_key(make_key(KeyCode::Right));
        app.handle_key(make_key(KeyCode::Down));
        assert_eq!((app.focus.row, app.focus.col), (1, 1));
    }
}
