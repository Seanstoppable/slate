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
use slate_plugin_sdk::{Action, BoxedWidget, Color, WidgetAction, WidgetContent};

use crate::config::{ConfigWarning, SlateConfig};
use crate::dashboard::{refresh_widget_content, Dashboard, WidgetInstance};
use crate::keybindings::reserved_keybinding;
use crate::layout::{compute_grid, compute_widget_area, FocusPosition};
use crate::notifications::UpdateNotifications;
use crate::render::{render_input_bar, render_status_bar, render_widget, render_widget_help_modal};

/// Active text-input prompt state.
struct InputMode {
    prompt: String,
    action_id: String,
    buffer: String,
}

/// The main Slate application.
pub struct App {
    dashboard: Dashboard,
    focus: FocusPosition,
    focus_initialized: bool,
    running: bool,
    notifications: UpdateNotifications,
    /// Active text-input prompt, if any.
    input_mode: Option<InputMode>,
    /// Whether help for the focused widget is displayed.
    help_visible: bool,
    /// Non-fatal config problems surfaced in the status bar.
    config_warnings: Vec<ConfigWarning>,
}

impl App {
    pub fn new(config: SlateConfig) -> Self {
        Self::from_dashboard(Dashboard::new(config))
    }

    pub fn from_dashboard(dashboard: Dashboard) -> Self {
        let notifications = UpdateNotifications::load();
        let config_warnings = dashboard.config.warnings();
        Self {
            dashboard,
            focus: FocusPosition::new(0, 0),
            focus_initialized: false,
            running: true,
            notifications,
            input_mode: None,
            help_visible: false,
            config_warnings,
        }
    }

    /// Non-fatal config problems detected at startup.
    pub fn config_warnings(&self) -> &[ConfigWarning] {
        &self.config_warnings
    }

    /// Register a widget into the application.
    #[allow(clippy::too_many_arguments)]
    pub fn add_widget(
        &mut self,
        widget: BoxedWidget,
        row: u16,
        col: u16,
        row_span: u16,
        col_span: u16,
        refresh_interval: Option<u64>,
        border_color: Option<Color>,
    ) {
        self.dashboard.add_widget(
            widget,
            row,
            col,
            row_span,
            col_span,
            refresh_interval,
            border_color,
        );
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
        self.ensure_focus_hook_fired();

        while self.running {
            self.dashboard.refresh_due(Some(&self.focus));

            // Draw
            terminal.draw(|frame| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)])
                    .split(frame.area());

                let main_area = chunks[0];
                let status_area = chunks[1];

                let grid = compute_grid(
                    main_area,
                    self.dashboard.config.layout.rows,
                    self.dashboard.config.layout.cols,
                );

                for instance in &self.dashboard.widgets {
                    let area = match compute_widget_area(
                        &grid,
                        instance.row,
                        instance.col,
                        instance.row_span,
                        instance.col_span,
                    ) {
                        Some(a) => a,
                        None => continue,
                    };
                    let focused = self.focus.row == instance.row && self.focus.col == instance.col;

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
                            instance.border_color.as_ref(),
                        );
                    } else {
                        render_widget(
                            frame,
                            area,
                            &instance.content,
                            &instance.metadata,
                            focused,
                            instance.selected,
                            instance.border_color.as_ref(),
                        );
                    }
                }

                if let Some(ref mode) = self.input_mode {
                    render_input_bar(frame, status_area, &mode.prompt, &mode.buffer);
                } else {
                    render_status_bar(
                        frame,
                        status_area,
                        &self.focus,
                        self.dashboard.widget_count(),
                        self.notifications.status_message().as_deref(),
                        self.config_warnings.len(),
                    );
                }

                if self.help_visible {
                    if let Some(instance) = self.focused_widget() {
                        let actions: &[Action] = match &instance.content {
                            WidgetContent::List { actions, .. } => actions,
                            _ => &[],
                        };
                        render_widget_help_modal(frame, frame.area(), &instance.metadata, actions);
                    }
                }
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
        // If a text-input prompt is active, intercept all keys
        if self.input_mode.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.input_mode = None;
                    return;
                }
                KeyCode::Enter => {
                    let (text, action_id) = {
                        let mode = self.input_mode.as_ref().unwrap();
                        (mode.buffer.clone(), mode.action_id.clone())
                    };
                    self.input_mode = None;
                    let mut pending_action: Option<WidgetAction> = None;
                    if let Some(instance) = self.focused_widget_mut() {
                        if let Some(action) = instance.widget.on_action(&action_id, &text) {
                            match action {
                                WidgetAction::ShowDetail(d) => {
                                    instance.detail_content = Some(d);
                                }
                                other => pending_action = Some(other),
                            }
                        }
                        instance.content = refresh_widget_content(instance.widget.as_mut());
                        instance.last_refresh = Instant::now();
                    }
                    if let Some(action) = pending_action {
                        self.handle_widget_action(action);
                    }
                    return;
                }
                KeyCode::Backspace => {
                    if let Some(ref mut mode) = self.input_mode {
                        mode.buffer.pop();
                    }
                    return;
                }
                KeyCode::Char(ch) => {
                    if let Some(ref mut mode) = self.input_mode {
                        mode.buffer.push(ch);
                    }
                    return;
                }
                _ => return,
            }
        }

        if self.help_visible {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                    self.help_visible = false;
                }
                _ => {}
            }
            return;
        }

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

        let key_name = key_to_action_name(&key);
        if focused_is_list
            && reserved_keybinding(&key_name).is_none()
            && self.try_trigger_list_action(&key)
        {
            return;
        }

        match key.code {
            KeyCode::Char('?') if self.focused_widget().is_some() => {
                self.help_visible = true;
            }
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            KeyCode::Tab => {
                // Move focus to next widget in reading order
                let mut next = self.focus;
                next.move_next(
                    self.dashboard.config.layout.rows,
                    self.dashboard.config.layout.cols,
                );
                self.set_focus(next);
            }
            KeyCode::BackTab => {
                // Move focus to previous widget
                let mut next = self.focus;
                next.move_prev(
                    self.dashboard.config.layout.rows,
                    self.dashboard.config.layout.cols,
                );
                self.set_focus(next);
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let mut next = self.focus;
                next.move_left(self.dashboard.config.layout.cols);
                self.set_focus(next);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let mut next = self.focus;
                next.move_right(self.dashboard.config.layout.cols);
                self.set_focus(next);
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
                    let mut next = self.focus;
                    next.move_up(self.dashboard.config.layout.rows);
                    self.set_focus(next);
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
                    let mut next = self.focus;
                    next.move_down(self.dashboard.config.layout.rows);
                    self.set_focus(next);
                }
            }
            KeyCode::Enter => {
                // Forward to focused widget with selected item
                let mut pending_action: Option<WidgetAction> = None;
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
                                    other => pending_action = Some(other),
                                }
                            }
                        }
                    }
                }
                if let Some(action) = pending_action {
                    self.handle_widget_action(action);
                }
            }
            KeyCode::Char('r') => {
                // Force refresh focused widget
                if let Some(instance) = self.focused_widget_mut() {
                    instance.content = refresh_widget_content(instance.widget.as_mut());
                    instance.last_refresh = Instant::now();
                    // Reset selection if list changed
                    if instance.content.is_selectable_list() {
                        instance.selected = Some(0);
                    }
                }
            }
            _ => {
                // Forward other keys to focused widget, then immediately
                // re-render so any state mutated by on_key (e.g. a Lua
                // widget's interactive state machine) shows up right away
                // instead of waiting for the next scheduled refresh.
                if let Some(instance) = self.focused_widget_mut() {
                    let key_str = key_to_action_name(&key);
                    instance.widget.on_key(&key_str, "");
                    instance.content = refresh_widget_content(instance.widget.as_mut());
                    instance.last_refresh = Instant::now();
                    if instance.content.is_selectable_list() && instance.selected.is_none() {
                        instance.selected = Some(0);
                    }
                }
            }
        }
    }

    fn handle_widget_action(&mut self, action: WidgetAction) {
        match action {
            WidgetAction::PromptInput { prompt, action_id } => {
                self.input_mode = Some(InputMode {
                    prompt,
                    action_id,
                    buffer: String::new(),
                });
            }
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
        self.dashboard
            .widgets
            .iter()
            .find(|w| w.row == self.focus.row && w.col == self.focus.col)
    }

    fn focused_widget_mut(&mut self) -> Option<&mut WidgetInstance> {
        self.dashboard
            .widgets
            .iter_mut()
            .find(|w| w.row == self.focus.row && w.col == self.focus.col)
    }

    fn ensure_focus_hook_fired(&mut self) {
        if self.focus_initialized {
            return;
        }

        if let Some(widget) = self.focused_widget_mut() {
            widget.widget.on_focus();
            self.focus_initialized = true;
        }
    }

    fn set_focus(&mut self, next: FocusPosition) {
        if self.focus == next {
            self.ensure_focus_hook_fired();
            return;
        }

        if let Some(widget) = self.focused_widget_mut() {
            widget.widget.on_blur();
        }

        self.focus = next;
        self.focus_initialized = true;

        if let Some(widget) = self.focused_widget_mut() {
            widget.widget.on_focus();
        }
    }

    fn try_trigger_list_action(&mut self, key: &KeyEvent) -> bool {
        let Some(instance) = self.focused_widget() else {
            return false;
        };
        let WidgetContent::List { items, actions, .. } = &instance.content else {
            return false;
        };

        let key_name = key_to_action_name(key);
        let Some(action_id) = actions
            .iter()
            .find(|action| {
                action
                    .key
                    .as_deref()
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(&key_name))
            })
            .map(|action| action.id.clone())
        else {
            return false;
        };

        // Use the selected item's id, or empty string when the list has no items.
        let item_id = instance
            .selected
            .and_then(|sel| items.get(sel))
            .map(|item| item.id.clone())
            .unwrap_or_default();
        let mut pending_action: Option<WidgetAction> = None;
        if let Some(instance) = self.focused_widget_mut() {
            if let Some(action) = instance.widget.on_action(&action_id, &item_id) {
                match action {
                    WidgetAction::ShowDetail(detail) => instance.detail_content = Some(detail),
                    other => pending_action = Some(other),
                }
            }
        }
        if let Some(action) = pending_action {
            self.handle_widget_action(action);
        }
        true
    }
}

fn key_to_action_name(key: &KeyEvent) -> String {
    match key.code {
        KeyCode::Char(ch) => ch.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "shift+tab".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        _ => format!("{:?}", key.code).to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::{Widget, WidgetConfig, WidgetContent, WidgetMetadata};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use std::time::{Duration, Instant};

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

    struct RecordingWidget {
        last_key: Arc<Mutex<Option<String>>>,
    }

    struct FocusTrackingWidget {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    struct ActionKeyWidget {
        last_action: Arc<Mutex<Option<(String, String)>>>,
        key: String,
    }

    impl RecordingWidget {
        fn new(last_key: Arc<Mutex<Option<String>>>) -> Self {
            Self { last_key }
        }
    }

    impl FocusTrackingWidget {
        fn new(events: Arc<Mutex<Vec<&'static str>>>) -> Self {
            Self { events }
        }
    }

    impl ActionKeyWidget {
        fn new(last_action: Arc<Mutex<Option<(String, String)>>>) -> Self {
            Self {
                last_action,
                key: "o".to_string(),
            }
        }

        fn with_key(last_action: Arc<Mutex<Option<(String, String)>>>, key: &str) -> Self {
            Self {
                last_action,
                key: key.to_string(),
            }
        }
    }

    impl Widget for RecordingWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Recorder".to_string(),
                description: "Records keys".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            WidgetContent::Text {
                content: "Recorder".to_string(),
                scrollable: false,
                wrap: true,
            }
        }

        fn on_key(&mut self, key: &str, _action: &str) {
            *self.last_key.lock().unwrap() = Some(key.to_string());
        }
    }

    struct StatefulWidget {
        toggled: bool,
    }

    impl Widget for StatefulWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Stateful".to_string(),
                description: "Tracks toggled state".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            WidgetContent::Text {
                content: if self.toggled {
                    "on".to_string()
                } else {
                    "off".to_string()
                },
                scrollable: false,
                wrap: true,
            }
        }

        fn on_key(&mut self, key: &str, _action: &str) {
            if key == "s" {
                self.toggled = true;
            }
        }
    }

    impl Widget for FocusTrackingWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Focus Tracker".to_string(),
                description: "Records focus events".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            WidgetContent::Text {
                content: "Focus".to_string(),
                scrollable: false,
                wrap: true,
            }
        }

        fn on_focus(&mut self) {
            self.events.lock().unwrap().push("focus");
        }

        fn on_blur(&mut self) {
            self.events.lock().unwrap().push("blur");
        }
    }

    impl Widget for ActionKeyWidget {
        fn metadata(&self) -> WidgetMetadata {
            WidgetMetadata {
                name: "Action Key".to_string(),
                description: "Handles list action hotkeys".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                homepage: None,
            }
        }

        fn init(&mut self, _config: WidgetConfig) {}

        fn refresh(&mut self) -> WidgetContent {
            WidgetContent::List {
                items: vec![slate_plugin_sdk::ListItem {
                    id: "item-1".to_string(),
                    title: "Actionable".to_string(),
                    subtitle: None,
                    icon: None,
                    style: Default::default(),
                }],
                selectable: true,
                actions: vec![slate_plugin_sdk::Action {
                    id: "open".to_string(),
                    label: "Open".to_string(),
                    key: Some(self.key.clone()),
                    confirm: false,
                }],
            }
        }

        fn on_action(&mut self, action_id: &str, item_id: &str) -> Option<WidgetAction> {
            *self.last_action.lock().unwrap() = Some((action_id.to_string(), item_id.to_string()));
            Some(WidgetAction::Notify("triggered".to_string()))
        }
    }

    #[test]
    fn forwarded_key_immediately_refreshes_widget_content() {
        // Interactive widgets (e.g. Lua scripts using on_key) mutate their own
        // state on keypress; the host must re-render right away instead of
        // waiting for the next scheduled refresh tick.
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(StatefulWidget { toggled: false }),
            0,
            0,
            1,
            1,
            Some(6000),
            None,
        );

        assert!(matches!(
            &app.dashboard.widgets[0].content,
            WidgetContent::Text { content, .. } if content == "off"
        ));

        app.handle_key(make_key(KeyCode::Char('s')));

        assert!(matches!(
            &app.dashboard.widgets[0].content,
            WidgetContent::Text { content, .. } if content == "on"
        ));
    }

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn test_app_with_list_widget(action: Option<WidgetAction>) -> App {
        let config = SlateConfig::default();
        let mut app = App::new(config);
        let widget = MockListWidget::new(action);
        app.add_widget(Box::new(widget), 0, 0, 1, 1, Some(300), None);
        app
    }

    #[test]
    fn app_collects_config_warnings_from_dashboard_config() {
        let config = SlateConfig::parse(
            "[layout]\nrows = 2\ncols = 2\n\n[[widget]]\ntype = \"builtin:clock\"\nposition = { row = 7, col = 0 }\n",
        )
        .unwrap();
        let app = App::new(config);
        assert_eq!(app.config_warnings().len(), 1);
        assert!(matches!(
            app.config_warnings()[0],
            ConfigWarning::OutOfBounds { .. }
        ));
    }

    #[test]
    fn app_has_no_config_warnings_for_default_config() {
        let app = App::new(SlateConfig::default());
        assert!(app.config_warnings().is_empty());
    }

    #[test]
    fn add_widget_adds_widget_and_sets_initial_content() {
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(CounterTextWidget::new(refresh_count.clone())),
            1,
            1,
            1,
            1,
            Some(60),
            None,
        );

        assert_eq!(app.dashboard.widgets.len(), 1);
        assert_eq!(refresh_count.load(Ordering::SeqCst), 1);
        assert_eq!(app.dashboard.widgets[0].row, 1);
        assert_eq!(app.dashboard.widgets[0].col, 1);
        match &app.dashboard.widgets[0].content {
            WidgetContent::Text { content, .. } => assert_eq!(content, "Refresh #1"),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn add_widget_uses_global_refresh_interval_and_initializes_list_selection() {
        let mut config = SlateConfig::default();
        config.global.refresh_interval = 42;
        let mut app = App::new(config);

        app.add_widget(Box::new(MockListWidget::new(None)), 0, 0, 1, 1, None, None);

        assert_eq!(
            app.dashboard.widgets[0].refresh_interval,
            Duration::from_secs(42)
        );
        assert_eq!(app.dashboard.widgets[0].selected, Some(0));
    }

    #[test]
    fn focused_widget_returns_widget_at_focus_position() {
        let mut config = SlateConfig::default();
        config.layout.rows = 1;
        config.layout.cols = 2;
        let mut app = App::new(config);
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);
        app.add_widget(
            Box::new(MockListWidget::new(None)),
            0,
            1,
            1,
            1,
            Some(60),
            None,
        );

        assert_eq!(
            app.focused_widget()
                .map(|widget| widget.metadata.name.as_str()),
            Some("Mock Text")
        );

        app.focus.col = 1;
        assert_eq!(
            app.focused_widget()
                .map(|widget| widget.metadata.name.as_str()),
            Some("Mock List")
        );
    }

    #[test]
    fn ensure_focus_hook_fires_once_for_initial_widget() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(FocusTrackingWidget::new(events.clone())),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );

        app.ensure_focus_hook_fired();
        app.ensure_focus_hook_fired();

        assert_eq!(*events.lock().unwrap(), vec!["focus"]);
    }

    #[test]
    fn moving_focus_triggers_blur_then_focus_hooks() {
        let first_events = Arc::new(Mutex::new(Vec::new()));
        let second_events = Arc::new(Mutex::new(Vec::new()));
        let mut config = SlateConfig::default();
        config.layout.rows = 1;
        config.layout.cols = 2;
        let mut app = App::new(config);
        app.add_widget(
            Box::new(FocusTrackingWidget::new(first_events.clone())),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );
        app.add_widget(
            Box::new(FocusTrackingWidget::new(second_events.clone())),
            0,
            1,
            1,
            1,
            Some(60),
            None,
        );

        app.ensure_focus_hook_fired();
        app.handle_key(make_key(KeyCode::Tab));

        assert_eq!(*first_events.lock().unwrap(), vec!["focus", "blur"]);
        assert_eq!(*second_events.lock().unwrap(), vec!["focus"]);
    }

    #[test]
    fn list_action_hotkey_triggers_widget_action() {
        let last_action = Arc::new(Mutex::new(None));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(ActionKeyWidget::new(last_action.clone())),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );

        app.handle_key(make_key(KeyCode::Char('o')));

        assert_eq!(
            *last_action.lock().unwrap(),
            Some(("open".to_string(), "item-1".to_string()))
        );
    }

    #[test]
    fn reserved_refresh_action_renders_error_and_does_not_run() {
        let last_action = Arc::new(Mutex::new(None));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(ActionKeyWidget::with_key(last_action.clone(), "r")),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );
        let previous_refresh = Instant::now() - Duration::from_secs(60);
        app.dashboard.widgets[0].last_refresh = previous_refresh;

        assert!(matches!(
            &app.dashboard.widgets[0].content,
            WidgetContent::Text { content, .. }
                if content.contains("Open") && content.contains("reserved key 'r'")
        ));

        app.handle_key(make_key(KeyCode::Char('r')));

        assert_eq!(*last_action.lock().unwrap(), None);
        assert!(app.dashboard.widgets[0].last_refresh > previous_refresh);
    }

    #[test]
    fn help_modal_captures_input_until_dismissed() {
        let last_action = Arc::new(Mutex::new(None));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(ActionKeyWidget::new(last_action.clone())),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );

        app.handle_key(make_key(KeyCode::Char('?')));
        assert!(app.help_visible);

        app.handle_key(make_key(KeyCode::Char('o')));
        assert!(app.help_visible);
        assert_eq!(*last_action.lock().unwrap(), None);

        app.handle_key(make_key(KeyCode::Esc));
        assert!(!app.help_visible);

        app.handle_key(make_key(KeyCode::Char('o')));
        assert_eq!(
            *last_action.lock().unwrap(),
            Some(("open".to_string(), "item-1".to_string()))
        );
    }

    #[test]
    fn list_action_hotkey_returns_false_for_non_action_states() {
        let mut empty_app = App::new(SlateConfig::default());
        assert!(!empty_app.try_trigger_list_action(&make_key(KeyCode::Char('o'))));

        let mut text_app = App::new(SlateConfig::default());
        text_app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);
        text_app.dashboard.widgets[0].selected = Some(0);
        assert!(!text_app.try_trigger_list_action(&make_key(KeyCode::Char('o'))));

        let mut list_app = test_app_with_list_widget(None);
        list_app.dashboard.widgets[0].selected = None;
        assert!(!list_app.try_trigger_list_action(&make_key(KeyCode::Char('o'))));

        list_app.dashboard.widgets[0].selected = Some(99);
        assert!(!list_app.try_trigger_list_action(&make_key(KeyCode::Char('o'))));

        list_app.dashboard.widgets[0].selected = Some(0);
        assert!(!list_app.try_trigger_list_action(&make_key(KeyCode::Char('z'))));
    }

    #[test]
    fn list_action_hotkey_can_set_detail_content() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Hotkey details".to_string())));
        if let WidgetContent::List { actions, .. } = &mut app.dashboard.widgets[0].content {
            actions.push(slate_plugin_sdk::Action {
                id: "details".to_string(),
                label: "Details".to_string(),
                key: Some("d".to_string()),
                confirm: false,
            });
        }

        assert!(app.try_trigger_list_action(&make_key(KeyCode::Char('d'))));

        assert_eq!(
            app.dashboard.widgets[0].detail_content.as_deref(),
            Some("Hotkey details")
        );
    }

    #[test]
    fn should_refresh_returns_true_after_enough_time_has_passed() {
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(CounterTextWidget::new(refresh_count)),
            0,
            0,
            1,
            1,
            Some(1),
            None,
        );

        let now = Instant::now();
        app.dashboard.widgets[0].last_refresh = now - Duration::from_secs(2);

        assert!(app.dashboard.widgets[0].should_refresh(now, &app.focus));
    }

    #[test]
    fn should_refresh_returns_false_before_interval_expires() {
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(CounterTextWidget::new(refresh_count)),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );

        let now = Instant::now();
        app.dashboard.widgets[0].last_refresh = now - Duration::from_secs(10);

        assert!(!app.dashboard.widgets[0].should_refresh(now, &app.focus));
    }

    #[test]
    fn should_refresh_returns_false_during_detail_view() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));
        app.dashboard.widgets[0].detail_content = Some("Details".to_string());
        let now = Instant::now();
        app.dashboard.widgets[0].last_refresh = now - Duration::from_secs(600);

        assert!(!app.dashboard.widgets[0].should_refresh(now, &app.focus));
    }

    #[test]
    fn selectable_list_refresh_is_suppressed_only_while_focused() {
        let mut app = test_app_with_list_widget(None);
        let now = Instant::now();
        app.dashboard.widgets[0].last_refresh = now - Duration::from_secs(600);

        assert!(!app.dashboard.widgets[0].should_refresh(now, &app.focus));

        app.focus = FocusPosition::new(1, 1);
        assert!(app.dashboard.widgets[0].should_refresh(now, &app.focus));
    }

    #[test]
    fn enter_on_list_with_show_detail_sets_detail_content() {
        let mut app = test_app_with_list_widget(Some(WidgetAction::ShowDetail(
            "Detailed info here".to_string(),
        )));

        // Widget starts with no detail
        assert!(app.dashboard.widgets[0].detail_content.is_none());

        // Press Enter to select
        app.handle_key(make_key(KeyCode::Enter));

        // Detail should now be set
        assert_eq!(
            app.dashboard.widgets[0].detail_content,
            Some("Detailed info here".to_string())
        );
    }

    #[test]
    fn escape_dismisses_detail_view() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail view
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.dashboard.widgets[0].detail_content.is_some());

        // Escape should dismiss
        app.handle_key(make_key(KeyCode::Esc));
        assert!(app.dashboard.widgets[0].detail_content.is_none());
    }

    #[test]
    fn q_dismisses_detail_view_without_quitting() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail view
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.dashboard.widgets[0].detail_content.is_some());

        // 'q' should dismiss detail, NOT quit the app
        app.handle_key(make_key(KeyCode::Char('q')));
        assert!(app.dashboard.widgets[0].detail_content.is_none());
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

        // j/k/Tab should be ignored â€” selection should not change
        let selected_before = app.dashboard.widgets[0].selected;
        app.handle_key(make_key(KeyCode::Char('j')));
        app.handle_key(make_key(KeyCode::Char('k')));
        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.dashboard.widgets[0].selected, selected_before);
    }

    #[test]
    fn enter_with_no_action_response_does_not_set_detail() {
        let mut app = test_app_with_list_widget(None);

        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.dashboard.widgets[0].detail_content.is_none());
    }

    #[test]
    fn enter_with_open_url_does_not_set_detail() {
        let mut app = test_app_with_list_widget(Some(WidgetAction::OpenUrl(
            "https://example.com".to_string(),
        )));

        app.handle_key(make_key(KeyCode::Enter));
        // OpenUrl should NOT set detail_content
        assert!(app.dashboard.widgets[0].detail_content.is_none());
    }

    #[test]
    fn j_k_navigate_list_selection() {
        let mut app = test_app_with_list_widget(None);

        // Starts at 0
        assert_eq!(app.dashboard.widgets[0].selected, Some(0));

        // j moves down
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.dashboard.widgets[0].selected, Some(1));

        // Can't go past end
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.dashboard.widgets[0].selected, Some(1));

        // k moves up
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.dashboard.widgets[0].selected, Some(0));

        // Can't go before 0
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.dashboard.widgets[0].selected, Some(0));
    }

    #[test]
    fn q_quits_when_not_in_detail_view() {
        let mut app = test_app_with_list_widget(None);
        assert!(app.running);

        app.handle_key(make_key(KeyCode::Char('q')));
        assert!(!app.running);
    }

    #[test]
    fn enter_on_non_list_widget_is_a_noop() {
        let last_key = Arc::new(Mutex::new(None));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(RecordingWidget::new(last_key.clone())),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );

        app.handle_key(make_key(KeyCode::Enter));

        assert_eq!(last_key.lock().unwrap().as_deref(), None);
        assert!(app.dashboard.widgets[0].detail_content.is_none());
        assert!(app.running);
    }

    #[test]
    fn tab_moves_focus_between_widgets() {
        let mut config = SlateConfig::default();
        config.layout.rows = 1;
        config.layout.cols = 2;
        let mut app = App::new(config);
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(300), None);
        app.add_widget(Box::new(MockTextWidget), 0, 1, 1, 1, Some(300), None);

        assert_eq!(app.focus.row, 0);
        assert_eq!(app.focus.col, 0);

        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.focus.col, 1);

        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.focus.col, 0); // wraps around
    }

    #[test]
    fn tab_and_backtab_keep_focus_stable_with_single_widget() {
        let mut config = SlateConfig::default();
        config.layout.rows = 1;
        config.layout.cols = 1;
        let mut app = App::new(config);
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);

        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!((app.focus.row, app.focus.col), (0, 0));

        app.handle_key(make_key(KeyCode::BackTab));
        assert_eq!((app.focus.row, app.focus.col), (0, 0));
    }

    #[test]
    fn refresh_is_suppressed_when_detail_is_showing() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.dashboard.widgets[0].detail_content.is_some());

        // Manually set last_refresh to the past to trigger refresh
        app.dashboard.widgets[0].last_refresh = Instant::now() - Duration::from_secs(600);

        // The main loop refresh logic checks detail_content â€” simulate it here
        let focus = app.focus;
        let instance = &mut app.dashboard.widgets[0];
        let now = Instant::now();
        assert!(!instance.should_refresh(now, &focus));
    }

    #[test]
    fn forced_refresh_clears_detail_and_refreshes() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("Details".to_string())));

        // Enter detail
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.dashboard.widgets[0].detail_content.is_some());

        // Escape to dismiss, then 'r' to refresh
        app.handle_key(make_key(KeyCode::Esc));
        assert!(app.dashboard.widgets[0].detail_content.is_none());

        // 'r' forces refresh
        app.handle_key(make_key(KeyCode::Char('r')));
        // Widget content should be refreshed (still a list)
        assert!(app.dashboard.widgets[0].content.is_selectable_list());
    }

    #[test]
    fn forced_refresh_resets_list_selection_to_first_item() {
        let mut app = test_app_with_list_widget(None);
        app.dashboard.widgets[0].selected = Some(1);

        app.handle_key(make_key(KeyCode::Char('r')));

        assert_eq!(app.dashboard.widgets[0].selected, Some(0));
    }

    #[test]
    fn forced_refresh_updates_content_and_resets_timer() {
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(CounterTextWidget::new(refresh_count.clone())),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );

        let previous_refresh = Instant::now() - Duration::from_secs(600);
        app.dashboard.widgets[0].last_refresh = previous_refresh;

        app.handle_key(make_key(KeyCode::Char('r')));

        assert_eq!(refresh_count.load(Ordering::SeqCst), 2);
        assert!(app.dashboard.widgets[0].last_refresh > previous_refresh);
        match &app.dashboard.widgets[0].content {
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
            app.handle_key(make_key(KeyCode::Char('?')));
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

    #[test]
    fn arrow_keys_move_focus_for_non_list_widgets() {
        let mut config = SlateConfig::default();
        config.layout.rows = 2;
        config.layout.cols = 2;
        let mut app = App::new(config);
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);

        app.handle_key(make_key(KeyCode::Right));
        assert_eq!((app.focus.row, app.focus.col), (0, 1));

        app.handle_key(make_key(KeyCode::Down));
        assert_eq!((app.focus.row, app.focus.col), (1, 1));

        app.handle_key(make_key(KeyCode::Left));
        assert_eq!((app.focus.row, app.focus.col), (1, 0));

        app.handle_key(make_key(KeyCode::Up));
        assert_eq!((app.focus.row, app.focus.col), (0, 0));
    }

    #[test]
    fn arrow_keys_change_list_selection_without_moving_focus() {
        let mut app = test_app_with_list_widget(None);

        app.handle_key(make_key(KeyCode::Down));
        assert_eq!(app.dashboard.widgets[0].selected, Some(1));
        assert_eq!((app.focus.row, app.focus.col), (0, 0));

        app.handle_key(make_key(KeyCode::Up));
        assert_eq!(app.dashboard.widgets[0].selected, Some(0));
        assert_eq!((app.focus.row, app.focus.col), (0, 0));
    }

    #[test]
    fn ctrl_c_and_backtab_update_running_and_focus() {
        let mut config = SlateConfig::default();
        config.layout.rows = 1;
        config.layout.cols = 2;
        let mut app = App::new(config);
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);
        app.add_widget(Box::new(MockTextWidget), 0, 1, 1, 1, Some(60), None);
        app.focus.col = 1;

        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.running);

        app.running = true;
        app.handle_key(make_key(KeyCode::BackTab));
        assert_eq!(app.focus.col, 0);
    }

    #[test]
    fn set_notifications_and_forwarded_keys_update_state() {
        let mut app = App::new(SlateConfig::default());
        app.set_notifications(UpdateNotifications {
            available_updates: vec![crate::notifications::UpdateInfo {
                name: "clock".to_string(),
                current_version: "1.0.0".to_string(),
                latest_version: "1.2.3".to_string(),
            }],
            last_check: None,
            dismissed: false,
        });
        assert!(app
            .notifications
            .status_message()
            .as_deref()
            .unwrap_or_default()
            .contains("1 update available"));

        let mut app = App::new(SlateConfig::default());
        let last_key = Arc::new(Mutex::new(None));
        let widget = RecordingWidget::new(last_key.clone());
        app.add_widget(Box::new(widget), 0, 0, 1, 1, Some(60), None);
        app.handle_key(make_key(KeyCode::Char('x')));
        assert_eq!(last_key.lock().unwrap().as_deref(), Some("x"));
    }

    #[test]
    fn escape_is_forwarded_when_not_showing_detail() {
        let last_key = Arc::new(Mutex::new(None));
        let mut app = App::new(SlateConfig::default());
        app.add_widget(
            Box::new(RecordingWidget::new(last_key.clone())),
            0,
            0,
            1,
            1,
            Some(60),
            None,
        );

        app.handle_key(make_key(KeyCode::Esc));

        assert_eq!(last_key.lock().unwrap().as_deref(), Some("esc"));
    }

    #[test]
    fn enter_with_notify_action_keeps_running_without_detail() {
        let mut app = test_app_with_list_widget(Some(WidgetAction::Notify("hello".to_string())));

        app.handle_key(make_key(KeyCode::Enter));

        assert!(app.running);
        assert!(app.dashboard.widgets[0].detail_content.is_none());
    }

    #[test]
    fn handle_widget_action_covers_notify_and_show_detail_branches() {
        let mut app = App::new(SlateConfig::default());
        app.handle_widget_action(WidgetAction::Notify("hello".to_string()));

        let result = std::panic::catch_unwind(|| {
            let mut app2 = App::new(SlateConfig::default());
            app2.handle_widget_action(WidgetAction::ShowDetail("detail".to_string()));
        });
        assert!(result.is_err());
    }

    #[test]
    fn handle_widget_action_prompt_input_sets_input_mode() {
        let mut app = App::new(SlateConfig::default());
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);

        app.handle_widget_action(WidgetAction::PromptInput {
            prompt: "New todo".to_string(),
            action_id: "add".to_string(),
        });

        assert!(app.input_mode.is_some());
        let mode = app.input_mode.as_ref().unwrap();
        assert_eq!(mode.prompt, "New todo");
        assert_eq!(mode.action_id, "add");
        assert!(mode.buffer.is_empty());
    }

    #[test]
    fn input_mode_esc_clears_input_mode() {
        let mut app = App::new(SlateConfig::default());
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);
        app.input_mode = Some(InputMode {
            prompt: "Enter:".to_string(),
            action_id: "add".to_string(),
            buffer: "partial".to_string(),
        });

        app.handle_key(make_key(KeyCode::Esc));

        assert!(app.input_mode.is_none());
    }

    #[test]
    fn input_mode_char_appends_to_buffer() {
        let mut app = App::new(SlateConfig::default());
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);
        app.input_mode = Some(InputMode {
            prompt: "Enter:".to_string(),
            action_id: "add".to_string(),
            buffer: String::new(),
        });

        app.handle_key(make_key(KeyCode::Char('h')));
        app.handle_key(make_key(KeyCode::Char('i')));

        let buffer = app.input_mode.as_ref().map(|m| m.buffer.as_str());
        assert_eq!(buffer, Some("hi"));
    }

    #[test]
    fn input_mode_backspace_removes_last_char() {
        let mut app = App::new(SlateConfig::default());
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);
        app.input_mode = Some(InputMode {
            prompt: "Enter:".to_string(),
            action_id: "add".to_string(),
            buffer: "hello".to_string(),
        });

        app.handle_key(make_key(KeyCode::Backspace));

        let buffer = app.input_mode.as_ref().map(|m| m.buffer.as_str());
        assert_eq!(buffer, Some("hell"));
    }

    #[test]
    fn input_mode_enter_calls_on_action_and_clears_mode() {
        // MockListWidget returns Some(Notify) from on_action
        let mut app = test_app_with_list_widget(Some(WidgetAction::Notify("done".to_string())));
        app.input_mode = Some(InputMode {
            prompt: "Enter:".to_string(),
            action_id: "add".to_string(),
            buffer: "new task".to_string(),
        });

        app.handle_key(make_key(KeyCode::Enter));

        // input_mode should be cleared after Enter
        assert!(app.input_mode.is_none());
    }

    #[test]
    fn input_mode_enter_with_show_detail_response_sets_detail_content() {
        let mut app =
            test_app_with_list_widget(Some(WidgetAction::ShowDetail("detail".to_string())));
        app.input_mode = Some(InputMode {
            prompt: "Enter:".to_string(),
            action_id: "add".to_string(),
            buffer: "task text".to_string(),
        });

        app.handle_key(make_key(KeyCode::Enter));

        assert!(app.input_mode.is_none());
        assert_eq!(
            app.dashboard.widgets[0].detail_content.as_deref(),
            Some("detail")
        );
    }

    #[test]
    fn input_mode_enter_with_no_focused_widget_is_noop() {
        let mut app = App::new(SlateConfig::default());
        // No widgets — focused_widget_mut returns None
        app.input_mode = Some(InputMode {
            prompt: "Enter:".to_string(),
            action_id: "add".to_string(),
            buffer: "text".to_string(),
        });

        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.input_mode.is_none());
    }

    #[test]
    fn input_mode_ignores_unrecognized_keys() {
        let mut app = App::new(SlateConfig::default());
        app.add_widget(Box::new(MockTextWidget), 0, 0, 1, 1, Some(60), None);
        app.input_mode = Some(InputMode {
            prompt: "Enter:".to_string(),
            action_id: "add".to_string(),
            buffer: "hello".to_string(),
        });

        // Home key is unrecognized in input mode — should return early with no change
        app.handle_key(make_key(KeyCode::Home));
        assert!(app.input_mode.is_some());
        assert_eq!(
            app.input_mode.as_ref().map(|m| m.buffer.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn key_to_action_name_handles_named_and_fallback_keys() {
        let cases = [
            (KeyCode::Char('x'), "x"),
            (KeyCode::Enter, "enter"),
            (KeyCode::Esc, "esc"),
            (KeyCode::Tab, "tab"),
            (KeyCode::BackTab, "shift+tab"),
            (KeyCode::Up, "up"),
            (KeyCode::Down, "down"),
            (KeyCode::Left, "left"),
            (KeyCode::Right, "right"),
            (KeyCode::Home, "home"),
        ];

        for (code, expected) in cases {
            assert_eq!(key_to_action_name(&make_key(code)), expected);
        }
    }
}
