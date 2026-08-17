#[derive(serde::Deserialize, Default)]
pub struct Launch {
    pub name: Option<String>,
    pub flight_number: Option<u64>,
    pub date_utc: Option<String>,
    pub rocket: Option<String>,
    pub launchpad: Option<String>,
    pub details: Option<String>,
}

pub fn build_pairs(launch: &Launch) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    if let Some(n) = &launch.name {
        pairs.push(("Mission".into(), n.clone()));
    }
    if let Some(f) = launch.flight_number {
        pairs.push(("Flight #".into(), f.to_string()));
    }
    if let Some(d) = &launch.date_utc {
        pairs.push(("Date (UTC)".into(), d.chars().take(10).collect()));
    }
    if let Some(r) = &launch.rocket {
        pairs.push(("Rocket".into(), r.clone()));
    }
    if let Some(l) = &launch.launchpad {
        pairs.push(("Launchpad".into(), l.clone()));
    }
    if let Some(d) = &launch.details {
        pairs.push(("Details".into(), d.clone()));
    }
    pairs
}

pub fn pairs_to_content(pairs: Vec<(String, String)>) -> serde_json::Value {
    serde_json::json!({
        "type": "key_value",
        "pairs": pairs.iter().map(|(k, v)| [k, v]).collect::<Vec<_>>()
    })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use extism_pdk::*;

    #[plugin_fn]
    pub fn metadata(_input: String) -> FnResult<String> {
        Ok(serde_json::json!({
            "name": "spacex",
            "description": "Displays information about the next SpaceX launch",
            "version": "0.1.0",
            "author": "Slate"
        })
        .to_string())
    }

    #[plugin_fn]
    pub fn refresh(_input: String) -> FnResult<String> {
        let launch: Launch =
            match slate_plugin_http::get_json("https://api.spacexdata.com/v5/launches/next", &[])
            {
                Ok(l) => l,
                Err(_) => {
                    return Ok(serde_json::json!({
                        "type": "text",
                        "content": "Failed to fetch launch data"
                    })
                    .to_string());
                }
            };

        let pairs = build_pairs(&launch);
        Ok(pairs_to_content(pairs).to_string())
    }

    #[plugin_fn]
    pub fn on_key(_input: String) -> FnResult<String> {
        Ok(String::new())
    }

    #[plugin_fn]
    pub fn on_action(_input: String) -> FnResult<String> {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_launch() -> Launch {
        Launch {
            name: Some("Starlink 6-1".into()),
            flight_number: Some(200),
            date_utc: Some("2026-09-01T12:00:00.000Z".into()),
            rocket: Some("Falcon 9".into()),
            launchpad: Some("KSC LC-39A".into()),
            details: Some("Batch of Starlink satellites.".into()),
        }
    }

    #[test]
    fn build_pairs_all_fields() {
        let launch = full_launch();
        let pairs = build_pairs(&launch);
        assert_eq!(pairs.len(), 6);
        assert_eq!(pairs[0], ("Mission".into(), "Starlink 6-1".into()));
        assert_eq!(pairs[1], ("Flight #".into(), "200".into()));
        assert_eq!(pairs[2], ("Date (UTC)".into(), "2026-09-01".into()));
        assert_eq!(pairs[3], ("Rocket".into(), "Falcon 9".into()));
        assert_eq!(pairs[4], ("Launchpad".into(), "KSC LC-39A".into()));
        assert_eq!(
            pairs[5],
            ("Details".into(), "Batch of Starlink satellites.".into())
        );
    }

    #[test]
    fn build_pairs_only_name() {
        let launch = Launch {
            name: Some("Test Mission".into()),
            ..Default::default()
        };
        let pairs = build_pairs(&launch);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("Mission".into(), "Test Mission".into()));
    }

    #[test]
    fn build_pairs_date_truncated() {
        let launch = Launch {
            date_utc: Some("2026-09-01T12:00:00.000Z".into()),
            ..Default::default()
        };
        let pairs = build_pairs(&launch);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1, "2026-09-01");
    }

    #[test]
    fn pairs_to_content_structure() {
        let pairs = vec![
            ("Mission".into(), "Starlink".into()),
            ("Flight #".into(), "42".into()),
        ];
        let content = pairs_to_content(pairs);
        assert_eq!(content["type"], "key_value");
        let p = content["pairs"].as_array().unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0][0], "Mission");
        assert_eq!(p[0][1], "Starlink");
        assert_eq!(p[1][0], "Flight #");
        assert_eq!(p[1][1], "42");
    }
}
