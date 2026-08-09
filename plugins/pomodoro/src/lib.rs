#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

use serde::{Deserialize, Serialize};
use serde_json::json;

const STATE_KEY: &str = "timer_state";

#[derive(Debug, Clone, Deserialize)]
struct Settings {
    #[serde(default = "default_break_minutes")]
    break_minutes: u64,
    #[serde(default = "default_work_minutes")]
    work_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimerState {
    phase: String,
    running: bool,
    remaining_secs: u64,
    last_started_at: Option<u64>,
    focused: bool,
}

#[derive(Debug, Deserialize)]
struct ActionInput {
    #[serde(default)]
    action_id: String,
}

#[derive(Debug, Deserialize)]
struct KeyInput {
    #[serde(default)]
    key: String,
}

#[derive(Debug, Deserialize)]
struct StoreGetResponse {
    #[serde(default)]
    found: bool,
    value: Option<String>,
}

fn default_work_minutes() -> u64 {
    25
}

fn default_break_minutes() -> u64 {
    5
}

fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn default_state(settings: &Settings) -> TimerState {
    TimerState {
        phase: "work".to_string(),
        running: false,
        remaining_secs: settings.work_minutes.saturating_mul(60).max(60),
        last_started_at: None,
        focused: false,
    }
}

fn duration_for_phase(settings: &Settings, phase: &str) -> u64 {
    match phase {
        "break" => settings.break_minutes.saturating_mul(60).max(60),
        _ => settings.work_minutes.saturating_mul(60).max(60),
    }
}

fn next_phase(phase: &str) -> &'static str {
    match phase {
        "break" => "work",
        _ => "break",
    }
}

fn normalize_state(settings: &Settings, state: &mut TimerState, now: u64) {
    if !state.running {
        state.last_started_at = None;
        if state.remaining_secs == 0 {
            state.remaining_secs = duration_for_phase(settings, &state.phase);
        }
        return;
    }

    let Some(last_started_at) = state.last_started_at else {
        state.last_started_at = Some(now);
        return;
    };

    let elapsed = now.saturating_sub(last_started_at);
    if elapsed == 0 {
        return;
    }

    if elapsed >= state.remaining_secs {
        let phase = next_phase(&state.phase).to_string();
        state.phase = phase.clone();
        state.remaining_secs = duration_for_phase(settings, &phase);
        state.last_started_at = Some(now);
    } else {
        state.remaining_secs -= elapsed;
        state.last_started_at = Some(now);
    }
}

fn toggle_running(state: &mut TimerState, now: u64) {
    if state.running {
        if let Some(last_started_at) = state.last_started_at {
            let elapsed = now.saturating_sub(last_started_at);
            state.remaining_secs = state.remaining_secs.saturating_sub(elapsed).max(1);
        }
        state.running = false;
        state.last_started_at = None;
    } else {
        state.running = true;
        state.last_started_at = Some(now);
    }
}

fn reset_state(settings: &Settings, state: &mut TimerState) {
    *state = default_state(settings);
}

fn skip_phase(settings: &Settings, state: &mut TimerState, now: u64) {
    state.phase = next_phase(&state.phase).to_string();
    state.remaining_secs = duration_for_phase(settings, &state.phase);
    state.last_started_at = state.running.then_some(now);
}

fn format_remaining(secs: u64) -> String {
    let minutes = secs / 60;
    let seconds = secs % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn build_content(state: &TimerState) -> serde_json::Value {
    let status = if state.running { "Running" } else { "Paused" };
    let phase = if state.phase == "break" {
        "Break"
    } else {
        "Work"
    };
    let focus = if state.focused { "Focused" } else { "Background" };
    let toggle_label = if state.running { "Pause" } else { "Start" };

    json!({
        "type": "list",
        "selectable": true,
        "items": [{
            "id": "pomodoro",
            "title": format!("⏱ {} {}", phase, format_remaining(state.remaining_secs)),
            "subtitle": format!("{} • {}", status, focus)
        }],
        "actions": [
            {"id": "toggle", "label": toggle_label, "key": "s", "confirm": false},
            {"id": "skip", "label": "Skip phase", "key": "n", "confirm": false},
            {"id": "reset", "label": "Reset", "key": "r", "confirm": false}
        ]
    })
}

fn detail_text(settings: &Settings, state: &TimerState) -> String {
    format!(
        "Pomodoro\n\nPhase: {}\nStatus: {}\nRemaining: {}\nWork minutes: {}\nBreak minutes: {}\nFocused: {}\n\nKeys: space/s=start-pause, n=skip, r=reset, Enter=help",
        if state.phase == "break" { "Break" } else { "Work" },
        if state.running { "Running" } else { "Paused" },
        format_remaining(state.remaining_secs),
        settings.work_minutes,
        settings.break_minutes,
        if state.focused { "yes" } else { "no" }
    )
}

#[cfg(target_arch = "wasm32")]
fn call_host(function: &str, request: serde_json::Value) -> Result<String, Error> {
    let request_str = request.to_string();
    let mem = Memory::from_bytes(request_str.as_bytes())?;
    let offset = unsafe { extism_pdk::extism_call(function, mem.offset()) };
    if offset != 0 {
        return Err(Error::msg(format!("{function} host function call failed")));
    }
    let output = extism_pdk::output_bytes()?;
    String::from_utf8(output).map_err(|e| Error::msg(format!("Invalid UTF-8 from {function}: {e}")))
}

#[cfg(target_arch = "wasm32")]
fn load_settings() -> Settings {
    call_host("get_config", json!({}))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Settings {
            work_minutes: default_work_minutes(),
            break_minutes: default_break_minutes(),
        })
}

#[cfg(target_arch = "wasm32")]
fn load_state(settings: &Settings) -> TimerState {
    let response = call_host("store_get", json!({ "key": STATE_KEY }))
        .ok()
        .and_then(|raw| serde_json::from_str::<StoreGetResponse>(&raw).ok());

    response
        .and_then(|response| {
            if response.found {
                response
                    .value
                    .and_then(|value| serde_json::from_str::<TimerState>(&value).ok())
            } else {
                None
            }
        })
        .unwrap_or_else(|| default_state(settings))
}

#[cfg(target_arch = "wasm32")]
fn save_state(state: &TimerState) -> Result<(), Error> {
    call_host(
        "store_set",
        json!({
            "key": STATE_KEY,
            "value": serde_json::to_string(state).unwrap_or_default()
        }),
    )?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn mutate_state<F>(mutator: F) -> Result<TimerState, Error>
where
    F: FnOnce(&Settings, &mut TimerState, u64),
{
    let settings = load_settings();
    let now = now_secs();
    let mut state = load_state(&settings);
    normalize_state(&settings, &mut state, now);
    mutator(&settings, &mut state, now);
    normalize_state(&settings, &mut state, now);
    save_state(&state)?;
    Ok(state)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "Pomodoro",
        "description": "Interactive pomodoro timer with persistent state",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    let settings = load_settings();
    let now = now_secs();
    let mut state = load_state(&settings);
    normalize_state(&settings, &mut state, now);
    save_state(&state)?;
    Ok(build_content(&state).to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(input: String) -> FnResult<String> {
    let key = serde_json::from_str::<KeyInput>(&input).unwrap_or(KeyInput {
        key: String::new(),
    });

    match key.key.as_str() {
        " " | "Space" => {
            let _ = mutate_state(|_, state, now| toggle_running(state, now));
        }
        "r" | "R" => {
            let _ = mutate_state(|settings, state, _| reset_state(settings, state));
        }
        "n" | "N" => {
            let _ = mutate_state(|settings, state, now| skip_phase(settings, state, now));
        }
        _ => {}
    }

    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    let action = serde_json::from_str::<ActionInput>(&input).unwrap_or(ActionInput {
        action_id: String::new(),
    });

    let settings = load_settings();
    let now = now_secs();
    let mut state = load_state(&settings);
    normalize_state(&settings, &mut state, now);

    match action.action_id.as_str() {
        "toggle" => {
            toggle_running(&mut state, now);
            save_state(&state)?;
            Ok(String::new())
        }
        "skip" => {
            skip_phase(&settings, &mut state, now);
            save_state(&state)?;
            Ok(json!({"notify": format!("Switched to {} phase", if state.phase == "break" { "break" } else { "work" })}).to_string())
        }
        "reset" => {
            reset_state(&settings, &mut state);
            save_state(&state)?;
            Ok(json!({"notify": "Pomodoro reset"}).to_string())
        }
        "select" => Ok(json!({"show_detail": detail_text(&settings, &state)}).to_string()),
        _ => Ok(String::new()),
    }
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_focus(_input: String) -> FnResult<String> {
    let _ = mutate_state(|_, state, _| state.focused = true);
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_blur(_input: String) -> FnResult<String> {
    let _ = mutate_state(|_, state, _| state.focused = false);
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_state_counts_down_elapsed_time() {
        let settings = Settings {
            work_minutes: 25,
            break_minutes: 5,
        };
        let mut state = TimerState {
            phase: "work".to_string(),
            running: true,
            remaining_secs: 120,
            last_started_at: Some(100),
            focused: false,
        };

        normalize_state(&settings, &mut state, 130);

        assert_eq!(state.remaining_secs, 90);
        assert_eq!(state.last_started_at, Some(130));
    }

    #[test]
    fn normalize_state_rolls_into_next_phase() {
        let settings = Settings {
            work_minutes: 25,
            break_minutes: 5,
        };
        let mut state = TimerState {
            phase: "work".to_string(),
            running: true,
            remaining_secs: 10,
            last_started_at: Some(50),
            focused: false,
        };

        normalize_state(&settings, &mut state, 75);

        assert_eq!(state.phase, "break");
        assert_eq!(state.remaining_secs, 300);
        assert_eq!(state.last_started_at, Some(75));
    }

    #[test]
    fn build_content_includes_expected_actions() {
        let content = build_content(&TimerState {
            phase: "work".to_string(),
            running: false,
            remaining_secs: 1500,
            last_started_at: None,
            focused: true,
        });

        let actions = content["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0]["id"], "toggle");
        assert_eq!(actions[1]["key"], "n");
        assert_eq!(actions[2]["label"], "Reset");
    }
}
