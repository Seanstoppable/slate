use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
struct Settings {
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    location: String,
    #[serde(default = "default_provider")]
    provider: String,
}

#[derive(Deserialize, Default)]
struct WeatherResponse {
    #[serde(default)]
    name: String,
    #[serde(default)]
    sys: Sys,
    #[serde(default)]
    main: Main,
    #[serde(default)]
    weather: Vec<Weather>,
    #[serde(default)]
    wind: Wind,
    #[serde(default)]
    message: String,
}

#[derive(Deserialize, Default)]
struct Sys {
    #[serde(default)]
    country: String,
}

#[derive(Deserialize, Default)]
struct Main {
    #[serde(default)]
    temp: f64,
    #[serde(default)]
    feels_like: f64,
    #[serde(default)]
    humidity: u64,
}

#[derive(Deserialize, Default)]
struct Weather {
    #[serde(default)]
    description: String,
}

#[derive(Deserialize, Default)]
struct Wind {
    #[serde(default)]
    speed: f64,
}

fn default_provider() -> String {
    "openweathermap".to_string()
}

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "Weather",
        "description": "Current weather from OpenWeatherMap",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: Settings = serde_json::from_str(&input).unwrap_or_default();

    if settings.api_key.trim().is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "Configure `api_key` and `location` for the weather plugin. Example: api_key = \"...\", location = \"Seattle\".",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    if settings.location.trim().is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "Configure `location` with a city name or lat,lon coordinates.",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    if settings.provider.trim() != "openweathermap" {
        return Ok(json!({
            "type": "text",
            "content": format!("Unsupported provider: {}. Only `openweathermap` is currently implemented.", settings.provider),
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let url = build_weather_url(settings.location.trim(), settings.api_key.trim());
    let req = HttpRequest::new(&url)
        .with_header("Accept", "application/json")
        .with_header("User-Agent", "slate-weather-plugin");
    let response = http::request::<String>(&req, None)?;
    let body = response.body();
    let body_str = std::str::from_utf8(&body).unwrap_or("{}");

    match serde_json::from_str::<WeatherResponse>(body_str) {
        Ok(weather) if !weather.message.trim().is_empty() && weather.name.trim().is_empty() => Ok(json!({
            "type": "text",
            "content": format!("Weather API error: {}", weather.message),
            "scrollable": false,
            "wrap": true
        })
        .to_string()),
        Ok(weather) => {
            let location = if weather.sys.country.trim().is_empty() {
                weather.name
            } else {
                format!("{}, {}", weather.name, weather.sys.country)
            };
            let conditions = weather
                .weather
                .first()
                .map(|w| title_case(&w.description))
                .unwrap_or_else(|| "Unknown".to_string());

            Ok(json!({
                "type": "key_value",
                "pairs": [
                    {"key": "Location", "value": if location.trim().is_empty() { settings.location.trim() } else { location.as_str() }},
                    {"key": "Temperature", "value": format!("{:.1} °C", weather.main.temp)},
                    {"key": "Feels Like", "value": format!("{:.1} °C", weather.main.feels_like)},
                    {"key": "Humidity", "value": format!("{}%", weather.main.humidity)},
                    {"key": "Conditions", "value": conditions},
                    {"key": "Wind", "value": format!("{:.1} m/s", weather.wind.speed)}
                ]
            })
            .to_string())
        }
        Err(err) => Ok(json!({
            "type": "text",
            "content": format!("Unable to parse weather response: {}", err),
            "scrollable": false,
            "wrap": true
        })
        .to_string()),
    }
}

#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

fn build_weather_url(location: &str, api_key: &str) -> String {
    if let Some((lat, lon)) = parse_lat_lon(location) {
        format!(
            "https://api.openweathermap.org/data/2.5/weather?lat={}&lon={}&appid={}&units=metric",
            lat,
            lon,
            encode_component(api_key)
        )
    } else {
        format!(
            "https://api.openweathermap.org/data/2.5/weather?q={}&appid={}&units=metric",
            encode_component(location),
            encode_component(api_key)
        )
    }
}

fn parse_lat_lon(location: &str) -> Option<(f64, f64)> {
    let mut parts = location.split(',');
    let lat = parts.next()?.trim().parse::<f64>().ok()?;
    let lon = parts.next()?.trim().parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((lat, lon))
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

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
