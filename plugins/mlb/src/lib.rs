#[cfg(target_arch = "wasm32")]
use chrono::Datelike;
#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

const API_BASE: &str = "https://statsapi.mlb.com/api/v1";

#[derive(Deserialize, Default, Debug)]
pub struct Settings {
    #[serde(default = "default_matches_from")]
    pub matches_from: i64,
    #[serde(default = "default_matches_to")]
    pub matches_to: i64,
    #[serde(default = "default_standing_count")]
    pub standing_count: usize,
}

fn default_matches_from() -> i64 {
    -1
}
fn default_matches_to() -> i64 {
    1
}
fn default_standing_count() -> usize {
    5
}

// Minimal schedule response types
#[derive(Deserialize, Debug, Default)]
pub struct ScheduleResponse {
    #[serde(default)]
    pub dates: Vec<DateItem>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct DateItem {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub games: Vec<Game>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct Game {
    #[serde(default, rename = "gamePk")]
    pub game_pk: i64,
    #[serde(default, rename = "gameDate")]
    pub game_date: Option<String>,
    #[serde(default)]
    pub status: GameStatus,
    #[serde(default)]
    pub teams: Teams,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct GameStatus {
    #[serde(default, rename = "abstractGameState")]
    pub abstract_game_state: String,
    #[serde(default, rename = "detailedState")]
    pub detailed_state: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct Teams {
    #[serde(default)]
    pub away: TeamSide,
    #[serde(default)]
    pub home: TeamSide,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct TeamSide {
    #[serde(default)]
    pub team: TeamRef,
    #[serde(default)]
    pub score: Option<i64>,
}

#[derive(Deserialize, Debug, Default, Clone, Serialize)]
pub struct GameSummary {
    pub home_team: String,
    pub away_team: String,
    pub status: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct LinescoreResponse {
    #[serde(default, rename = "currentInning")]
    pub current_inning: Option<i64>,
    #[serde(default, rename = "currentInningOrdinal")]
    pub current_inning_ordinal: Option<String>,
    #[serde(default)]
    pub teams: LinescoreTeams,
    #[serde(default)]
    pub innings: Vec<Inning>,
}

#[derive(Deserialize, Debug, Default)]
pub struct LinescoreTeams {
    #[serde(default)]
    pub away: LinescoreTeam,
    #[serde(default)]
    pub home: LinescoreTeam,
}

#[derive(Deserialize, Debug, Default)]
pub struct LinescoreTeam {
    #[serde(default)]
    pub runs: i64,
    #[serde(default)]
    pub hits: i64,
    #[serde(default)]
    pub errors: i64,
}

#[derive(Deserialize, Debug, Default)]
pub struct Inning {
    #[serde(default)]
    pub num: i64,
    #[serde(default)]
    pub away: InningTeam,
    #[serde(default)]
    pub home: InningTeam,
}

#[derive(Deserialize, Debug, Default)]
pub struct InningTeam {
    #[serde(default)]
    pub runs: Option<i64>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct TeamRef {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
}

// Minimal standings response types (records contain teamRecords)
#[derive(Deserialize, Debug, Default)]
pub struct StandingsResponse {
    #[serde(default)]
    pub records: Vec<Record>,
}

#[derive(Deserialize, Debug, Default)]
pub struct Record {
    #[serde(default, rename = "teamRecords")]
    pub team_records: Vec<TeamRecord>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct TeamRecord {
    #[serde(default)]
    pub team: TeamRef,
    #[serde(default)]
    pub wins: i64,
    #[serde(default)]
    pub losses: i64,
    #[serde(default, rename = "winPct")]
    pub win_pct: String,
    #[serde(default, rename = "divisionRank")]
    pub division_rank: String,
    #[serde(default, rename = "leagueRank")]
    pub league_rank: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct TableEntry {
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub team: TeamRef,
    #[serde(default)]
    pub played_games: Option<i64>,
    #[serde(default)]
    pub points: Option<i64>,
}

pub fn build_schedule_url(start_date: &str, end_date: &str) -> String {
    format!("{}/schedule?sportId=1&startDate={}&endDate={}", API_BASE, start_date, end_date)
}

pub fn build_standings_url(season: i32) -> String {
    format!("{}/standings?season={}", API_BASE, season)
}

pub fn format_score(game: &Game) -> String {
    let home = game
        .teams
        .home
        .score
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".to_string());
    let away = game
        .teams
        .away
        .score
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".to_string());
    format!("{} - {}", home, away)
}

pub fn format_status(status: &GameStatus) -> String {
    match status.abstract_game_state.as_str() {
        "Preview" => "SCHEDULED".to_string(),
        "In Progress" => "LIVE".to_string(),
        "Final" => "FINAL".to_string(),
        other => {
            if !status.detailed_state.trim().is_empty() {
                status.detailed_state.clone()
            } else {
                other.to_string()
            }
        }
    }
}

pub fn format_game_detail(
    summary: &GameSummary,
    linescore: &LinescoreResponse,
) -> String {
    let inning_label = linescore
        .current_inning_ordinal
        .clone()
        .or_else(|| linescore.current_inning.map(|inning| inning.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let innings = linescore
        .innings
        .iter()
        .map(|inning| {
            format!(
                "{}:{}-{}",
                inning.num,
                inning.away.runs.unwrap_or(0),
                inning.home.runs.unwrap_or(0)
            )
        })
        .collect::<Vec<_>>()
        .join("  ");

    let mut lines = vec![
        format!("{} @ {}", summary.away_team, summary.home_team),
        format!("Status: {}", summary.status),
        format!(
            "Score: {} - {}  ({} inning)",
            linescore.teams.away.runs, linescore.teams.home.runs, inning_label
        ),
        format!(
            "Away: {} R  {} H  {} E",
            linescore.teams.away.runs, linescore.teams.away.hits, linescore.teams.away.errors
        ),
        format!(
            "Home: {} R  {} H  {} E",
            linescore.teams.home.runs, linescore.teams.home.hits, linescore.teams.home.errors
        ),
    ];
    if !innings.is_empty() {
        lines.push(format!("Innings: {innings}"));
    }
    lines.join("\n")
}

pub fn render_table(games: &[Game], standings: &HashMap<String, Vec<TableEntry>>, standing_count: usize) -> serde_json::Value {
    let mut rows: Vec<serde_json::Value> = games
        .iter()
        .map(|g| {
            json!([
                {"text": "Game", "style": {"bold": true}},
                {"text": g.game_date.clone().unwrap_or_default(), "style": {}},
                {"text": g.teams.home.team.name.clone(), "style": {"bold": true}},
                {"text": format_score(g), "style": {}},
                {"text": g.teams.away.team.name.clone(), "style": {"bold": true}},
                {"text": format_status(&g.status), "style": {}}
            ])
        })
        .collect();

    for (group, table) in standings {
        for entry in table.iter().take(standing_count) {
            rows.push(json!([
                {"text": group, "style": {}},
                {"text": entry.position.to_string(), "style": {"bold": true}},
                {"text": entry.team.name.clone(), "style": {}},
                {"text": format!("{} wins", entry.points.unwrap_or_default()), "style": {}},
                {"text": format!("{} played", entry.played_games.unwrap_or_default()), "style": {}},
                {"text": "", "style": {}}
            ]));
        }
    }

    json!({
        "type": "table",
        "headers": ["Type", "Date / Rank", "Home / Team", "Score / Wins", "Away / Played", "Status"],
        "rows": rows,
        "selectable": true
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "MLB",
        "description": "MLB schedule and standings from the MLB Stats API",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    }).to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: Settings = serde_json::from_str(&input).unwrap_or_default();

    let today = chrono::Utc::now().date_naive();
    let from_date = (today + chrono::Duration::days(settings.matches_from))
        .format("%Y-%m-%d")
        .to_string();
    let to_date = (today + chrono::Duration::days(settings.matches_to))
        .format("%Y-%m-%d")
        .to_string();

    let schedule_url = build_schedule_url(&from_date, &to_date);
    let season = today.year();
    let standings_url = build_standings_url(season);

    let headers = [
        ("Accept", "application/json"),
        ("User-Agent", "slate-mlb-plugin"),
    ];

    let mut all_games: Vec<Game> = Vec::new();
    let mut standings_map: HashMap<String, Vec<TableEntry>> = HashMap::new();

    if let Ok(parsed) = slate_plugin_http::get_json::<ScheduleResponse>(&schedule_url, &headers) {
        for date in parsed.dates {
            for game in date.games {
                all_games.push(game.clone());
            }
        }
    }

    if let Ok(parsed2) = slate_plugin_http::get_json::<StandingsResponse>(&standings_url, &headers) {
        // flatten team records into a simple table
        let mut entries: Vec<TableEntry> = Vec::new();
        for record in parsed2.records {
            for tr in record.team_records {
                let played = (tr.wins + tr.losses) as i64;
                let position = tr.division_rank.parse::<i64>().unwrap_or(0);
                entries.push(TableEntry {
                    position,
                    team: tr.team.clone(),
                    played_games: Some(played),
                    points: Some(tr.wins as i64),
                });
            }
        }
        standings_map.insert("Standings".to_string(), entries);
    }

    let game_ids: Vec<i64> = all_games.iter().map(|g| g.game_pk).collect();
    let game_ids = serde_json::to_string(&game_ids).unwrap_or_default();
    #[cfg(target_arch = "wasm32")]
    var::set("mlb_game_ids", game_ids.as_str())?;
    let summaries: Vec<GameSummary> = all_games
        .iter()
        .map(|game| GameSummary {
            home_team: game.teams.home.team.name.clone(),
            away_team: game.teams.away.team.name.clone(),
            status: format_status(&game.status),
        })
        .collect();
    let summaries = serde_json::to_string(&summaries).unwrap_or_default();
    #[cfg(target_arch = "wasm32")]
    var::set("mlb_game_summaries", summaries.as_str())?;

    Ok(render_table(&all_games, &standings_map, settings.standing_count).to_string())
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

    let game_ids = var::get::<String>("mlb_game_ids").ok().flatten().and_then(|ids| serde_json::from_str::<Vec<i64>>(&ids).ok()).unwrap_or_default();
    let Some(&game_pk) = game_ids.get(index) else {
        return Ok(String::new());
    };
    if game_pk == 0 {
        return Ok(String::new());
    }

    let summaries = var::get::<String>("mlb_game_summaries")
        .ok()
        .flatten()
        .and_then(|summaries| serde_json::from_str::<Vec<GameSummary>>(&summaries).ok())
        .unwrap_or_default();
    let Some(summary) = summaries.get(index) else {
        return Ok(String::new());
    };

    let url = format!("{}/game/{}/linescore", API_BASE, game_pk);
    let headers = [("Accept", "application/json"), ("User-Agent", "slate-mlb-plugin")];
    let details = slate_plugin_http::get_json::<LinescoreResponse>(&url, &headers)
        .map(|linescore| format_game_detail(summary, &linescore))
        .map_err(|e| format!("Unable to fetch or parse game details: {}", e));

    Ok(json!({
        "show_detail": match details {
            Ok(summary) => summary,
            Err(err) => err,
        }
    }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_schedule_url_contains_dates() {
        let url = build_schedule_url("2026-08-01", "2026-08-05");
        assert!(url.contains("startDate=2026-08-01"));
        assert!(url.contains("endDate=2026-08-05"));
    }

    #[test]
    fn test_build_standings_url() {
        assert_eq!(build_standings_url(2026), "https://statsapi.mlb.com/api/v1/standings?season=2026");
    }

    #[test]
    fn test_format_score_nulls_and_values() {
        let game_none = Game::default();
        assert_eq!(format_score(&game_none), "- - -");

        let mut g = Game::default();
        g.teams.home.team.name = "Home".to_string();
        g.teams.away.team.name = "Away".to_string();
        g.teams.home.score = Some(3);
        g.teams.away.score = Some(2);
        assert_eq!(format_score(&g), "3 - 2");
    }

    #[test]
    fn test_format_status_variants() {
        let mut s = GameStatus::default();
        s.abstract_game_state = "Preview".to_string();
        assert_eq!(format_status(&s), "SCHEDULED");
        s.abstract_game_state = "In Progress".to_string();
        assert_eq!(format_status(&s), "LIVE");
        s.abstract_game_state = "Final".to_string();
        assert_eq!(format_status(&s), "FINAL");
    }

    #[test]
    fn test_format_game_detail_is_concise() {
        let summary = GameSummary {
            home_team: "Home".to_string(),
            away_team: "Away".to_string(),
            status: "FINAL".to_string(),
        };
        let linescore = LinescoreResponse {
            current_inning: Some(9),
            teams: LinescoreTeams {
                away: LinescoreTeam {
                    runs: 3,
                    hits: 8,
                    errors: 0,
                },
                home: LinescoreTeam {
                    runs: 2,
                    hits: 6,
                    errors: 1,
                },
            },
            innings: vec![Inning {
                num: 1,
                away: InningTeam { runs: Some(1) },
                home: InningTeam { runs: Some(0) },
            }],
            ..Default::default()
        };

        assert_eq!(
            format_game_detail(&summary, &linescore),
            "Away @ Home\nStatus: FINAL\nScore: 3 - 2  (9 inning)\nAway: 3 R  8 H  0 E\nHome: 2 R  6 H  1 E\nInnings: 1:1-0"
        );
    }

    #[test]
    fn test_render_table_with_standings_only() {
        let mut standings = HashMap::new();
        standings.insert(
            "Div".to_string(),
            vec![TableEntry {
                position: 1,
                team: TeamRef { id: 1, name: "Team A".to_string() },
                played_games: Some(10),
                points: Some(7),
            }],
        );
        let table = render_table(&[], &standings, 5);
        assert_eq!(table["rows"][0][0]["text"], "Div");
        assert_eq!(table["rows"][0][2]["text"], "Team A");
    }
}
