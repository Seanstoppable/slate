use chrono::NaiveDate;
#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

const API_BASE: &str = "https://api.football-data.org/v4";
#[cfg(target_arch = "wasm32")]
const API_SIGNUP_URL: &str = "https://www.football-data.org/";

#[derive(Deserialize, Default, Debug)]
pub struct Settings {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub competitions: Vec<String>,
    #[serde(default = "default_matches_from")]
    pub matches_from: i64,
    #[serde(default = "default_matches_to")]
    pub matches_to: i64,
    #[serde(default = "default_standing_count")]
    pub standing_count: usize,
}

fn default_matches_from() -> i64 {
    2
}
fn default_matches_to() -> i64 {
    5
}
fn default_standing_count() -> usize {
    5
}

// Minimal types for parsing the v4 API responses we need
#[derive(Deserialize, Debug)]
pub struct MatchesResponse {
    #[serde(default)]
    pub matches: Vec<MatchItem>,
}

#[derive(Deserialize, Debug)]
pub struct MatchItem {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    #[serde(rename = "utcDate")]
    pub utc_date: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub competition: Option<CompetitionRef>,
    #[serde(default)]
    #[serde(rename = "homeTeam")]
    pub home_team: TeamRef,
    #[serde(default)]
    #[serde(rename = "awayTeam")]
    pub away_team: TeamRef,
    #[serde(default)]
    pub score: Score,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct CompetitionRef {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub code: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct TeamRef {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct Score {
    #[serde(default)]
    #[serde(rename = "fullTime")]
    pub full_time: Option<TeamScore>,
    #[serde(default)]
    pub winner: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct TeamScore {
    #[serde(default)]
    pub home: Option<i64>,
    #[serde(default)]
    pub away: Option<i64>,
}

#[derive(Deserialize, Debug, Default)]
pub struct MatchDetails {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    #[serde(rename = "utcDate")]
    pub utc_date: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub competition: CompetitionRef,
    #[serde(default)]
    pub season: Season,
    #[serde(default)]
    pub matchday: Option<i64>,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    #[serde(rename = "homeTeam")]
    pub home_team: TeamRef,
    #[serde(default)]
    #[serde(rename = "awayTeam")]
    pub away_team: TeamRef,
    #[serde(default)]
    pub score: Score,
    #[serde(default)]
    pub goals: Vec<Goal>,
    #[serde(default)]
    pub bookings: Vec<Booking>,
    #[serde(default)]
    pub substitutions: Vec<Substitution>,
    #[serde(default)]
    pub referees: Vec<Person>,
}

#[derive(Deserialize, Debug, Default)]
pub struct Season {
    #[serde(default)]
    pub name: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct Goal {
    #[serde(default)]
    pub minute: Option<i64>,
    #[serde(default)]
    #[serde(rename = "injuryTime")]
    pub injury_time: Option<i64>,
    #[serde(default)]
    pub scorer: Person,
    #[serde(default)]
    pub assist: Option<Person>,
    #[serde(default)]
    pub score: TeamScore,
}

#[derive(Deserialize, Debug, Default)]
pub struct Booking {
    #[serde(default)]
    pub minute: Option<i64>,
    #[serde(default)]
    pub team: TeamRef,
    #[serde(default)]
    pub player: Person,
    #[serde(default)]
    pub card: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct Substitution {
    #[serde(default)]
    pub minute: Option<i64>,
    #[serde(default)]
    pub team: TeamRef,
    #[serde(default)]
    #[serde(rename = "playerOut")]
    pub player_out: Person,
    #[serde(default)]
    #[serde(rename = "playerIn")]
    pub player_in: Person,
}

#[derive(Deserialize, Debug, Default)]
pub struct Person {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub nationality: String,
    #[serde(default)]
    pub role: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct StandingsResponse {
    #[serde(default)]
    pub standings: Vec<Standing>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct Standing {
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub table: Vec<TableEntry>,
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct TableEntry {
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub team: TeamRef,
    #[serde(default)]
    #[serde(rename = "playedGames")]
    pub played_games: Option<i64>,
    #[serde(default)]
    pub points: Option<i64>,
}

// Build a matches URL for a competition and an inclusive date window in YYYY-MM-DD.
// The v4 API's dateTo parameter is exclusive, so we add one day to the inclusive end.
pub fn build_matches_url(competition: &str, date_from: &str, date_to_inclusive: &str) -> String {
    let from = NaiveDate::parse_from_str(date_from, "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
    let to = NaiveDate::parse_from_str(date_to_inclusive, "%Y-%m-%d").unwrap_or(from);
    let to_exclusive = to + chrono::Duration::days(1);
    format!(
        "{}/competitions/{}/matches?dateFrom={}&dateTo={}",
        API_BASE,
        competition,
        from.format("%Y-%m-%d"),
        to_exclusive.format("%Y-%m-%d")
    )
}

pub fn build_standings_url(competition: &str) -> String {
    format!("{}/competitions/{}/standings", API_BASE, competition)
}

pub fn build_match_details_url(match_id: i64) -> String {
    format!("{}/matches/{}", API_BASE, match_id)
}

// Choose the TOTAL standings if present, otherwise fall back to first available standings
pub fn choose_total_standing(standings: &[Standing]) -> Option<&Standing> {
    standings
        .iter()
        .find(|s| match &s.r#type {
            Some(t) => t == "TOTAL",
            None => false,
        })
        .or_else(|| standings.first())
}

pub fn format_score(score: &Score) -> String {
    if let Some(ft) = &score.full_time {
        let home = ft
            .home
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string());
        let away = ft
            .away
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string());
        format!("{} - {}", home, away)
    } else {
        "-".to_string()
    }
}

pub fn format_status(status: &str) -> String {
    match status {
        "SCHEDULED" => "SCHEDULED".to_string(),
        "TIMED" => "TIMED".to_string(),
        "IN_PLAY" => "LIVE".to_string(),
        "PAUSED" => "LIVE (PAUSED)".to_string(),
        "FINISHED" => "FINISHED".to_string(),
        "AWARDED" => "FINISHED (AWARDED)".to_string(),
        "POSTPONED" => "POSTPONED".to_string(),
        other => other.to_string(),
    }
}

fn format_minute(minute: Option<i64>, injury_time: Option<i64>) -> String {
    match (minute, injury_time) {
        (Some(minute), Some(injury_time)) => format!("{}+{}'", minute, injury_time),
        (Some(minute), None) => format!("{}'", minute),
        _ => "?".to_string(),
    }
}

fn format_person(person: &Person) -> &str {
    if person.name.trim().is_empty() {
        "Unknown"
    } else {
        &person.name
    }
}

pub fn format_match_details(match_details: &MatchDetails) -> String {
    let competition = if match_details.competition.name.trim().is_empty() {
        "Competition"
    } else {
        &match_details.competition.name
    };
    let season = if match_details.season.name.trim().is_empty() {
        String::new()
    } else {
        format!(" ({})", match_details.season.name)
    };
    let matchday = match_details
        .matchday
        .map(|matchday| format!(" | Matchday {}", matchday))
        .unwrap_or_default();
    let stage = if match_details.stage.trim().is_empty() {
        String::new()
    } else {
        format!(" | {}", match_details.stage)
    };
    let group = match_details
        .group
        .as_deref()
        .filter(|group| !group.trim().is_empty())
        .map(|group| format!(" | {}", group))
        .unwrap_or_default();

    let mut lines = vec![
        format!("{}{}", competition, season),
        format!("{}{}{}{}", match_details.utc_date, matchday, stage, group),
        format!(
            "{} {} {}",
            match_details.home_team.name,
            format_score(&match_details.score),
            match_details.away_team.name
        ),
        format!("Status: {}", format_status(&match_details.status)),
    ];

    if !match_details.goals.is_empty() {
        lines.push(String::new());
        lines.push("Goals".to_string());
        for goal in &match_details.goals {
            let assist = goal
                .assist
                .as_ref()
                .filter(|assist| !assist.name.trim().is_empty())
                .map(|assist| format!(" (assist: {})", format_person(assist)))
                .unwrap_or_default();
            lines.push(format!(
                "{} {}{} [{} - {}]",
                format_minute(goal.minute, goal.injury_time),
                format_person(&goal.scorer),
                assist,
                goal.score
                    .home
                    .map_or_else(|| "-".to_string(), |score| score.to_string()),
                goal.score
                    .away
                    .map_or_else(|| "-".to_string(), |score| score.to_string())
            ));
        }
    }

    if !match_details.bookings.is_empty() {
        lines.push(String::new());
        lines.push("Bookings".to_string());
        for booking in &match_details.bookings {
            lines.push(format!(
                "{} {} ({}) - {}",
                format_minute(booking.minute, None),
                format_person(&booking.player),
                booking.team.name,
                booking.card
            ));
        }
    }

    if !match_details.substitutions.is_empty() {
        lines.push(String::new());
        lines.push("Substitutions".to_string());
        for substitution in &match_details.substitutions {
            lines.push(format!(
                "{} {}: {} -> {}",
                format_minute(substitution.minute, None),
                substitution.team.name,
                format_person(&substitution.player_out),
                format_person(&substitution.player_in)
            ));
        }
    }

    if !match_details.referees.is_empty() {
        lines.push(String::new());
        lines.push("Referees".to_string());
        for referee in &match_details.referees {
            let role = if referee.role.trim().is_empty() {
                String::new()
            } else {
                format!(" ({})", referee.role)
            };
            lines.push(format!("{}{}", format_person(referee), role));
        }
    }

    lines.join("\n")
}

pub fn render_table(
    matches: &[MatchItem],
    standings_map: &HashMap<String, Vec<TableEntry>>,
    standing_count: usize,
) -> serde_json::Value {
    let mut rows: Vec<serde_json::Value> = matches
        .iter()
        .map(|m| {
            let comp_code = m
                .competition
                .as_ref()
                .map(|c| c.code.clone())
                .unwrap_or_default();

            json!([
                {"text": comp_code, "style": {}},
                {"text": "Match", "style": {"bold": true}},
                {"text": m.utc_date, "style": {}},
                {"text": m.home_team.name, "style": {"bold": true}},
                {"text": format_score(&m.score), "style": {}},
                {"text": m.away_team.name, "style": {"bold": true}},
                {"text": format_status(&m.status), "style": {}}
            ])
        })
        .collect();

    for (competition, table) in standings_map {
        for entry in table.iter().take(standing_count) {
            rows.push(json!([
                {"text": competition, "style": {}},
                {"text": "Standing", "style": {"bold": true}},
                {"text": entry.position.to_string(), "style": {}},
                {"text": entry.team.name, "style": {"bold": true}},
                {"text": format!("{} pts", entry.points.unwrap_or_default()), "style": {}},
                {"text": format!("{} played", entry.played_games.unwrap_or_default()), "style": {}},
                {"text": "", "style": {}}
            ]));
        }
    }

    json!({
        "type": "table",
        "headers": ["Comp", "Type", "Date / Rank", "Home / Team", "Score / Points", "Away / Played", "Status"],
        "rows": rows,
        "selectable": true
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "Football",
        "description": "Football matches and standings from football-data.org",
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
            "content": format!("Configure `api_key` for the football plugin. Sign up for a key at {} and set it as the `api_key` secret.", API_SIGNUP_URL),
            "scrollable": false,
            "wrap": true
        }).to_string());
    }

    if settings.competitions.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "Configure `competitions` with football-data competition codes (e.g. ['PL', 'CL']).",
            "scrollable": false,
            "wrap": true
        }).to_string());
    }

    // Action callbacks only receive the selected row, so retain the API key
    // in the loaded plugin instance for match-detail requests.
    var::set("football_api_key", settings.api_key.as_str())?;

    // Fetch matches and standings per competition
    let mut all_matches: Vec<MatchItem> = Vec::new();
    let mut standings_map: HashMap<String, Vec<TableEntry>> = HashMap::new();

    for comp in &settings.competitions {
        let today = chrono::Utc::now().date_naive();
        let from_date = (today + chrono::Duration::days(settings.matches_from))
            .format("%Y-%m-%d")
            .to_string();
        let to_date = (today + chrono::Duration::days(settings.matches_to))
            .format("%Y-%m-%d")
            .to_string();

        let matches_url = build_matches_url(comp, &from_date, &to_date);
        let standings_url = build_standings_url(comp);

        let headers = [
            ("Accept", "application/json"),
            ("X-Auth-Token", settings.api_key.as_str()),
            ("User-Agent", "slate-football-plugin"),
        ];
        if let Ok(parsed) = slate_plugin_http::get_json::<MatchesResponse>(&matches_url, &headers) {
            for m in parsed.matches {
                all_matches.push(m);
            }
        }

        if let Ok(parsed2) =
            slate_plugin_http::get_json::<StandingsResponse>(&standings_url, &headers)
        {
            if let Some(s) = choose_total_standing(&parsed2.standings) {
                let table_entries = s.table.clone();
                // store by competition code
                standings_map.insert(comp.clone(), table_entries);
            }
        }
    }

    let match_ids: Vec<i64> = all_matches.iter().map(|match_item| match_item.id).collect();
    let match_ids = serde_json::to_string(&match_ids).unwrap_or_default();
    var::set("football_match_ids", match_ids)?;

    Ok(render_table(&all_matches, &standings_map, settings.standing_count).to_string())
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

    let Ok(action) = serde_json::from_str::<ActionInput>(&input) else {
        return Ok(String::new());
    };
    if action.action_id != "select" {
        return Ok(String::new());
    }

    let Ok(index) = action.item_id.parse::<usize>() else {
        return Ok(String::new());
    };
    let match_ids = var::get::<String>("football_match_ids")
        .ok()
        .flatten()
        .and_then(|ids| serde_json::from_str::<Vec<i64>>(&ids).ok())
        .unwrap_or_default();
    let Some(&match_id) = match_ids.get(index) else {
        return Ok(String::new());
    };
    if match_id == 0 {
        return Ok(String::new());
    }

    let api_key = var::get::<String>("football_api_key")
        .ok()
        .flatten()
        .unwrap_or_default();
    if api_key.trim().is_empty() {
        return Ok(json!({
            "show_detail": format!(
                "Configure `api_key` to view match details. Sign up at {}.",
                API_SIGNUP_URL
            )
        })
        .to_string());
    }

    let url = build_match_details_url(match_id);
    let headers = [
        ("Accept", "application/json"),
        ("X-Auth-Token", api_key.as_str()),
        ("User-Agent", "slate-football-plugin"),
    ];
    let details = slate_plugin_http::get_json::<MatchDetails>(&url, &headers)
        .map_err(|error| format!("Unable to fetch or parse match details: {}", error));

    Ok(json!({
        "show_detail": match details {
            Ok(details) => format_match_details(&details),
            Err(error) => error,
        }
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_matches_url_adds_one_day() {
        let url = build_matches_url("PL", "2026-08-01", "2026-08-05");
        assert!(url.contains("dateFrom=2026-08-01"));
        // dateTo should be exclusive, one day past 2026-08-05 => 2026-08-06
        assert!(url.contains("dateTo=2026-08-06"));
    }

    #[test]
    fn test_build_match_details_url() {
        assert_eq!(
            build_match_details_url(1234),
            "https://api.football-data.org/v4/matches/1234"
        );
    }

    #[test]
    fn test_choose_total_standing_prefers_total() {
        let s1 = Standing {
            stage: None,
            table: vec![],
            r#type: Some("GROUP".to_string()),
        };
        let s2 = Standing {
            stage: None,
            table: vec![],
            r#type: Some("TOTAL".to_string()),
        };
        let v = vec![s1, s2];
        let chosen = choose_total_standing(&v).unwrap();
        assert_eq!(chosen.r#type.as_ref().unwrap(), "TOTAL");
    }

    #[test]
    fn test_choose_total_standing_fallback() {
        let s1 = Standing {
            stage: None,
            table: vec![],
            r#type: None,
        };
        let v = vec![s1];
        let chosen = choose_total_standing(&v).unwrap();
        assert!(chosen.r#type.is_none());
    }

    #[test]
    fn test_format_score_nulls_and_values() {
        let score_none = Score {
            full_time: None,
            winner: None,
        };
        assert_eq!(format_score(&score_none), "-");

        let score_partial = Score {
            full_time: Some(TeamScore {
                home: None,
                away: Some(1),
            }),
            winner: None,
        };
        assert_eq!(format_score(&score_partial), "- - 1");

        let score_full = Score {
            full_time: Some(TeamScore {
                home: Some(2),
                away: Some(1),
            }),
            winner: None,
        };
        assert_eq!(format_score(&score_full), "2 - 1");
    }

    #[test]
    fn test_format_status_various() {
        assert_eq!(format_status("SCHEDULED"), "SCHEDULED");
        assert_eq!(format_status("TIMED"), "TIMED");
        assert_eq!(format_status("IN_PLAY"), "LIVE");
        assert_eq!(format_status("PAUSED"), "LIVE (PAUSED)");
        assert_eq!(format_status("FINISHED"), "FINISHED");
        assert_eq!(format_status("AWARDED"), "FINISHED (AWARDED)");
        assert_eq!(format_status("POSTPONED"), "POSTPONED");
    }

    #[test]
    fn test_parse_matches_json() {
        let data = r#"{ "matches": [ { "id": 99, "utcDate":"2026-08-10T15:00:00Z", "status":"SCHEDULED", "competition": { "id": 1, "name": "Premier League", "code": "PL" }, "homeTeam": { "id": 10, "name": "Team A" }, "awayTeam": { "id": 20, "name": "Team B" }, "score": { "fullTime": { "home": null, "away": null }, "winner": null } } ] }"#;
        let parsed: MatchesResponse = serde_json::from_str(data).unwrap();
        assert_eq!(parsed.matches.len(), 1);
        let m = &parsed.matches[0];
        assert_eq!(m.id, 99);
        assert_eq!(m.home_team.name, "Team A");
        assert_eq!(m.away_team.name, "Team B");
        assert_eq!(m.status, "SCHEDULED");
    }

    #[test]
    fn test_render_table_includes_standings_without_matches() {
        let mut standings = HashMap::new();
        standings.insert(
            "PL".to_string(),
            vec![TableEntry {
                position: 1,
                team: TeamRef {
                    id: 1,
                    name: "Team A".to_string(),
                },
                played_games: Some(2),
                points: Some(6),
            }],
        );

        let table = render_table(&[], &standings, 5);

        assert_eq!(table["rows"][0][0]["text"], "PL");
        assert_eq!(table["rows"][0][1]["text"], "Standing");
        assert_eq!(table["rows"][0][3]["text"], "Team A");
        assert_eq!(table["rows"][0][4]["text"], "6 pts");
    }

    #[test]
    fn test_format_match_details_includes_events() {
        let data = r#"{
            "id": 99,
            "utcDate": "2026-08-16T15:00:00Z",
            "status": "FINISHED",
            "competition": {"name": "Premier League"},
            "season": {"name": "2026"},
            "matchday": 1,
            "stage": "REGULAR_SEASON",
            "homeTeam": {"name": "Team A"},
            "awayTeam": {"name": "Team B"},
            "score": {"fullTime": {"home": 2, "away": 1}},
            "goals": [{
                "minute": 12,
                "scorer": {"name": "Player A"},
                "assist": {"name": "Player B"},
                "score": {"home": 1, "away": 0}
            }],
            "bookings": [{
                "minute": 45,
                "team": {"name": "Team B"},
                "player": {"name": "Player C"},
                "card": "YELLOW_CARD"
            }],
            "substitutions": [{
                "minute": 60,
                "team": {"name": "Team A"},
                "playerOut": {"name": "Player D"},
                "playerIn": {"name": "Player E"}
            }],
            "referees": [{"name": "Referee", "role": "REFEREE"}]
        }"#;
        let details: MatchDetails = serde_json::from_str(data).unwrap();
        let content = format_match_details(&details);

        assert!(content.contains("Team A 2 - 1 Team B"));
        assert!(content.contains("12' Player A (assist: Player B) [1 - 0]"));
        assert!(content.contains("45' Player C (Team B) - YELLOW_CARD"));
        assert!(content.contains("60' Team A: Player D -> Player E"));
        assert!(content.contains("Referee (REFEREE)"));
    }
}
