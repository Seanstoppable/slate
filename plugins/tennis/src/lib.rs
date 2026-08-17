#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

const DEFAULT_MATCH_LIMIT: usize = 10;
const DEFAULT_STATUS: &str = "live";
const FREE_KEY_URL: &str = "https://livetennisapi.com/subscribe/free";
const BASE_URL: &str = "https://api.livetennisapi.com/api/public/v1";

// --- Settings ---

#[derive(Deserialize, Default)]
struct Settings {
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    tour: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    match_limit: Option<usize>,
}

// --- API response types ---

#[derive(Deserialize, Default, Clone)]
struct Player {
    #[serde(default, deserialize_with = "nullable_string")]
    name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_u32")]
    ranking: u32,
}

fn deserialize_nullable_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<u32>::deserialize(deserializer).map(|opt| opt.unwrap_or(0))
}

fn nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}

#[derive(Deserialize, Default, Clone)]
struct Players {
    #[serde(default)]
    p1: Player,
    #[serde(default)]
    p2: Player,
}

#[derive(Deserialize, Default, Clone)]
struct Score {
    #[serde(default)]
    sets: Vec<i32>,
    #[serde(default)]
    games: Vec<Vec<i32>>,
    #[serde(default)]
    points: Vec<String>,
    #[serde(default)]
    server: u8,
    #[serde(default)]
    is_tiebreak: bool,
}

#[derive(Deserialize, Default, Clone)]
struct Match {
    #[serde(default, deserialize_with = "nullable_string")]
    tournament: String,
    #[serde(default, deserialize_with = "nullable_string")]
    round: String,
    #[serde(default)]
    players: Players,
    #[serde(default)]
    score: Option<Score>,
    #[serde(default, deserialize_with = "nullable_string")]
    scheduled_time: String,
    #[serde(default, deserialize_with = "deserialize_nullable_u8")]
    winner: u8,
}

fn deserialize_nullable_u8<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<u8>::deserialize(deserializer).map(|opt| opt.unwrap_or(0))
}

#[derive(Deserialize, Default)]
struct MatchesResponse {
    #[serde(default)]
    data: Vec<Match>,
}

// --- Plugin exports ---

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "Tennis",
        "description": "Live tennis match scores from Live Tennis API",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: Settings = serde_json::from_str(&input).unwrap_or_default();

    if settings.api_key.trim().is_empty() {
        return Ok(json!({
            "type": "text",
            "content": format!(
                "No Live Tennis API key configured.\n\nSet `api_key` in the tennis widget config.\n\nGet a free key (1,000 req/day): {}",
                FREE_KEY_URL
            ),
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let status = normalize_status(&settings.status);
    let limit = settings.match_limit.unwrap_or(DEFAULT_MATCH_LIMIT);

    let url = build_api_url(status, &settings.tour, limit);
    let req = HttpRequest::new(&url)
        .with_header("Accept", "application/json")
        .with_header("x-api-key", settings.api_key.trim());

    let response = http::request::<String>(&req, None)?;
    let body = response.body();
    let status_code = response.status_code();
    let body_str = std::str::from_utf8(&body).unwrap_or("{}");

    match status_code {
        200 => {}
        401 => {
            return Ok(json!({
                "type": "text",
                "content": format!(
                    "Invalid API key (401)\n\nThe Live Tennis API rejected the configured key.\nCheck `api_key` or get a free key: {}",
                    FREE_KEY_URL
                ),
                "scrollable": false,
                "wrap": true
            })
            .to_string());
        }
        429 => {
            return Ok(json!({
                "type": "text",
                "content": "Rate limited (429)\n\nToo many requests to the Live Tennis API.\nIncrease this widget's refresh interval and try again.",
                "scrollable": false,
                "wrap": true
            })
            .to_string());
        }
        code => {
            return Ok(json!({
                "type": "text",
                "content": format!("Live Tennis API returned unexpected status {}", code),
                "scrollable": false,
                "wrap": true
            })
            .to_string());
        }
    }

    let envelope: MatchesResponse = match serde_json::from_str(body_str) {
        Ok(r) => r,
        Err(e) => {
            return Ok(json!({
                "type": "text",
                "content": format!("Failed to parse API response: {}", e),
                "scrollable": false,
                "wrap": true
            })
            .to_string());
        }
    };

    let matches: Vec<&Match> = envelope.data.iter().take(limit).collect();

    if matches.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": format!("No {} matches", status),
            "scrollable": false,
            "wrap": false
        })
        .to_string());
    }

    let items: Vec<serde_json::Value> = matches
        .iter()
        .map(|m| {
            json!({
                "id": format!("{}-{}-{}-vs-{}", m.tournament, m.round, m.players.p1.name, m.players.p2.name),
                "title": render_match_title(m),
                "subtitle": render_match_subtitle(m),
                "style": {}
            })
        })
        .collect();

    Ok(json!({
        "type": "list",
        "items": items,
        "selectable": true,
        "actions": [
            {"id": "details", "label": "Show details", "key": "enter", "confirm": false}
        ]
    })
    .to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    #[derive(Deserialize)]
    struct ActionInput {
        #[serde(default)]
        action_id: String,
        #[serde(default)]
        item_id: String,
    }

    if let Ok(action) = serde_json::from_str::<ActionInput>(&input) {
        if action.action_id == "details" || action.action_id == "select" {
            return Ok(json!({"show_detail": format!("Match: {}", action.item_id)}).to_string());
        }
    }
    Ok(String::new())
}

// --- Rendering helpers ---

fn render_match_title(m: &Match) -> String {
    let p1 = format_player(&m.players.p1, m.winner == 1);
    let p2 = format_player(&m.players.p2, m.winner == 2);
    let is_live = m.score.is_some() && m.winner == 0;

    let mut parts: Vec<String> = vec![p1.clone()];

    if let Some(ref score) = m.score {
        let mut score_str = format_games(score);
        if is_live && score.server == 1 {
            score_str.push('*');
        }
        if is_live {
            if let Some(pts) = format_points(score) {
                score_str = format!("{} {}", score_str.trim(), pts);
            }
        }
        let score_str = score_str.trim().to_string();
        if !score_str.is_empty() {
            parts.push(score_str);
        }
    }

    parts.push("vs".to_string());

    if is_live && m.score.as_ref().map_or(false, |s| s.server == 2) {
        parts.push(format!("{}*", p2));
    } else {
        parts.push(p2);
    }

    parts.join(" ")
}

fn render_match_subtitle(m: &Match) -> String {
    let mut parts = Vec::new();

    let location = format_location(m);
    if !location.is_empty() {
        parts.push(location);
    }

    if m.score.is_none() && !m.scheduled_time.is_empty() {
        let time = m.scheduled_time.replace('T', " ");
        parts.push(format!("🕙 {}", time));
    }

    parts.join(" • ")
}

fn format_player(player: &Player, winner: bool) -> String {
    let name = if player.name.is_empty() {
        "TBD".to_string()
    } else {
        player.name.clone()
    };

    let display = if player.ranking > 0 {
        format!("{} ({})", name, player.ranking)
    } else {
        name
    };

    if winner {
        format!("[b]{}[/b]", display)
    } else {
        display
    }
}

fn format_games(score: &Score) -> String {
    if score.games.len() == 2 {
        let count = score.games[0].len().min(score.games[1].len());
        if count > 0 {
            let parts: Vec<String> = (0..count)
                .map(|i| format!("{}-{}", score.games[0][i], score.games[1][i]))
                .collect();
            return parts.join(" ");
        }
    }

    if score.sets.len() == 2 {
        return format!("{}-{} sets", score.sets[0], score.sets[1]);
    }

    String::new()
}

fn format_points(score: &Score) -> Option<String> {
    if score.points.len() != 2 || score.points[0].is_empty() || score.points[1].is_empty() {
        return None;
    }
    let points = format!("{}-{}", score.points[0], score.points[1]);
    if score.is_tiebreak {
        Some(format!("(TB {})", points))
    } else {
        Some(format!("({})", points))
    }
}

fn format_location(m: &Match) -> String {
    let parts: Vec<&str> = [m.tournament.as_str(), m.round.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect();
    parts.join(" ")
}

fn normalize_status(status: &str) -> &str {
    match status.trim().to_lowercase().as_str() {
        "live" => "live",
        "upcoming" => "upcoming",
        "completed" => "completed",
        _ => DEFAULT_STATUS,
    }
}

fn build_api_url(status: &str, tour: &str, limit: usize) -> String {
    let mut url = format!("{}/matches?status={}", BASE_URL, encode_component(status));
    if !tour.trim().is_empty() {
        url.push_str(&format!("&tour={}", encode_component(tour.trim())));
    }
    if limit > 0 {
        url.push_str(&format!("&limit={}", limit));
    }
    url
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_player_ranked() {
        let p = Player { name: "Sinner".into(), ranking: 1 };
        assert_eq!(format_player(&p, false), "Sinner (1)");
    }

    #[test]
    fn test_format_player_unranked() {
        let p = Player { name: "Qualifier".into(), ranking: 0 };
        assert_eq!(format_player(&p, false), "Qualifier");
    }

    #[test]
    fn test_format_player_empty_name() {
        let p = Player::default();
        assert_eq!(format_player(&p, false), "TBD");
    }

    #[test]
    fn test_format_player_winner() {
        let p = Player { name: "Alcaraz".into(), ranking: 2 };
        assert_eq!(format_player(&p, true), "[b]Alcaraz (2)[/b]");
    }

    #[test]
    fn test_format_games_per_set() {
        let score = Score {
            games: vec![vec![6, 4, 2], vec![3, 6, 1]],
            ..Default::default()
        };
        assert_eq!(format_games(&score), "6-3 4-6 2-1");
    }

    #[test]
    fn test_format_games_sets_fallback() {
        let score = Score {
            sets: vec![2, 1],
            ..Default::default()
        };
        assert_eq!(format_games(&score), "2-1 sets");
    }

    #[test]
    fn test_format_games_uneven_arrays() {
        let score = Score {
            games: vec![vec![6, 4, 2], vec![3, 6]],
            ..Default::default()
        };
        assert_eq!(format_games(&score), "6-3 4-6");
    }

    #[test]
    fn test_format_games_empty() {
        let score = Score::default();
        assert_eq!(format_games(&score), "");
    }

    #[test]
    fn test_format_points_regular() {
        let score = Score {
            points: vec!["40".into(), "AD".into()],
            ..Default::default()
        };
        assert_eq!(format_points(&score), Some("(40-AD)".into()));
    }

    #[test]
    fn test_format_points_tiebreak() {
        let score = Score {
            points: vec!["5".into(), "3".into()],
            is_tiebreak: true,
            ..Default::default()
        };
        assert_eq!(format_points(&score), Some("(TB 5-3)".into()));
    }

    #[test]
    fn test_format_points_empty() {
        let score = Score::default();
        assert_eq!(format_points(&score), None);
    }

    #[test]
    fn test_format_points_partial() {
        let score = Score {
            points: vec!["40".into(), "".into()],
            ..Default::default()
        };
        assert_eq!(format_points(&score), None);
    }

    #[test]
    fn test_normalize_status() {
        assert_eq!(normalize_status("live"), "live");
        assert_eq!(normalize_status("upcoming"), "upcoming");
        assert_eq!(normalize_status("completed"), "completed");
        assert_eq!(normalize_status("LIVE"), "live");
        assert_eq!(normalize_status("bogus"), "live");
        assert_eq!(normalize_status(""), "live");
    }

    #[test]
    fn test_build_api_url_full() {
        let url = build_api_url("live", "atp", 5);
        assert_eq!(
            url,
            "https://api.livetennisapi.com/api/public/v1/matches?status=live&tour=atp&limit=5"
        );
    }

    #[test]
    fn test_build_api_url_no_tour() {
        let url = build_api_url("upcoming", "", 10);
        assert_eq!(
            url,
            "https://api.livetennisapi.com/api/public/v1/matches?status=upcoming&limit=10"
        );
    }

    #[test]
    fn test_format_location() {
        let m = Match {
            tournament: "Tampere".into(),
            round: "QF".into(),
            ..Default::default()
        };
        assert_eq!(format_location(&m), "Tampere QF");
    }

    #[test]
    fn test_format_location_empty() {
        let m = Match::default();
        assert_eq!(format_location(&m), "");
    }

    #[test]
    fn test_render_match_title_live_p1_serving() {
        let m = Match {
            tournament: "Tampere".into(),
            round: "QF".into(),
            players: Players {
                p1: Player { name: "Sinner".into(), ranking: 1 },
                p2: Player { name: "Alcaraz".into(), ranking: 2 },
            },
            score: Some(Score {
                sets: vec![1, 1],
                games: vec![vec![6, 4, 2], vec![3, 6, 1]],
                points: vec!["40".into(), "AD".into()],
                server: 1,
                is_tiebreak: false,
            }),
            winner: 0,
            ..Default::default()
        };
        assert_eq!(
            render_match_title(&m),
            "Sinner (1) 6-3 4-6 2-1* (40-AD) vs Alcaraz (2)"
        );
    }

    #[test]
    fn test_render_match_title_live_p2_serving() {
        let m = Match {
            players: Players {
                p1: Player { name: "Sinner".into(), ranking: 1 },
                p2: Player { name: "Alcaraz".into(), ranking: 2 },
            },
            score: Some(Score {
                games: vec![vec![6, 2], vec![3, 2]],
                points: vec!["15".into(), "30".into()],
                server: 2,
                ..Default::default()
            }),
            winner: 0,
            ..Default::default()
        };
        assert_eq!(
            render_match_title(&m),
            "Sinner (1) 6-3 2-2 (15-30) vs Alcaraz (2)*"
        );
    }

    #[test]
    fn test_render_match_title_completed() {
        let m = Match {
            players: Players {
                p1: Player { name: "Sinner".into(), ranking: 1 },
                p2: Player { name: "Alcaraz".into(), ranking: 2 },
            },
            score: Some(Score {
                games: vec![vec![6, 4, 4, 4], vec![4, 6, 6, 6]],
                ..Default::default()
            }),
            winner: 2,
            ..Default::default()
        };
        assert_eq!(
            render_match_title(&m),
            "Sinner (1) 6-4 4-6 4-6 4-6 vs [b]Alcaraz (2)[/b]"
        );
    }

    #[test]
    fn test_render_match_subtitle_upcoming() {
        let m = Match {
            tournament: "Umag".into(),
            round: "R16".into(),
            scheduled_time: "2026-07-24T18:30:00Z".into(),
            ..Default::default()
        };
        assert_eq!(
            render_match_subtitle(&m),
            "Umag R16 • 🕙 2026-07-24 18:30:00Z"
        );
    }

    #[test]
    fn test_render_match_subtitle_live() {
        let m = Match {
            tournament: "Tampere".into(),
            round: "QF".into(),
            score: Some(Score::default()),
            ..Default::default()
        };
        assert_eq!(render_match_subtitle(&m), "Tampere QF");
    }
}
