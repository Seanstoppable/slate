#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

const CHART_API_BASE: &str = "https://query1.finance.yahoo.com/v8/finance/chart/";

#[derive(Deserialize, Default)]
struct Settings {
    #[serde(default)]
    symbols: Vec<String>,
    #[serde(default)]
    sort: bool,
}

#[derive(Deserialize, Default)]
struct ChartResponse {
    #[serde(default)]
    chart: Chart,
}

#[derive(Deserialize, Default)]
struct Chart {
    #[serde(default)]
    result: Vec<ChartResult>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct ChartResult {
    #[serde(default)]
    meta: ChartMeta,
}

#[derive(Deserialize, Default)]
struct ChartMeta {
    #[serde(default)]
    currency: String,
    #[serde(default)]
    symbol: String,
    #[serde(default, rename = "regularMarketPrice")]
    regular_market_price: f64,
    #[serde(default, rename = "chartPreviousClose")]
    chart_previous_close: f64,
    #[serde(default, rename = "previousClose")]
    previous_close: f64,
    #[serde(default, rename = "currentTradingPeriod")]
    current_trading_period: TradingPeriods,
}

#[derive(Deserialize, Default)]
struct TradingPeriods {
    #[serde(default)]
    pre: TradingWindow,
    #[serde(default)]
    regular: TradingWindow,
    #[serde(default)]
    post: TradingWindow,
}

#[derive(Deserialize, Default)]
struct TradingWindow {
    #[serde(default)]
    start: i64,
    #[serde(default)]
    end: i64,
}

/// A resolved quote for a single symbol, or an error placeholder if the
/// symbol could not be fetched/parsed.
struct Quote {
    symbol: String,
    currency: String,
    market_state: String,
    market_price: f64,
    market_change: f64,
    market_change_pct: f64,
    trend: String,
    error: Option<String>,
}

impl Quote {
    fn errored(symbol: &str, error: impl Into<String>) -> Self {
        Self {
            symbol: symbol.to_string(),
            currency: String::new(),
            market_state: "?".to_string(),
            market_price: 0.0,
            market_change: 0.0,
            market_change_pct: 0.0,
            trend: "?".to_string(),
            error: Some(error.into()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "Yahoo Finance",
        "description": "Stock, ETF, and futures quotes via Yahoo Finance",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[cfg(target_arch = "wasm32")]
fn get_unix_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(target_arch = "wasm32")]
fn fetch_chart_meta(symbol: &str) -> Result<ChartMeta, String> {
    let url = format!(
        "{}{}?range=1d&interval=1d",
        CHART_API_BASE,
        encode_component(symbol)
    );
    let req = HttpRequest::new(&url)
        .with_header("Accept", "application/json")
        .with_header("User-Agent", "Mozilla/5.0 (compatible; slate-yfinance/1.0)");
    let response = http::request::<String>(&req, None).map_err(|e| e.to_string())?;
    let body = response.body();
    let body_str = std::str::from_utf8(&body).unwrap_or("{}");

    let parsed: ChartResponse =
        serde_json::from_str(body_str).map_err(|e| format!("parse error: {}", e))?;

    if let Some(err) = parsed.chart.error {
        if !err.is_null() {
            return Err(format!("API error: {}", err));
        }
    }

    parsed
        .chart
        .result
        .into_iter()
        .next()
        .map(|r| r.meta)
        .ok_or_else(|| "no results for symbol".to_string())
}

#[cfg(target_arch = "wasm32")]
fn quote_for_symbol(symbol: &str, now: i64) -> Quote {
    match fetch_chart_meta(symbol) {
        Ok(meta) => build_quote(&meta, symbol, now),
        Err(e) => Quote::errored(symbol, e),
    }
}

fn build_quote(meta: &ChartMeta, requested_symbol: &str, now: i64) -> Quote {
    let previous_close = if meta.chart_previous_close != 0.0 {
        meta.chart_previous_close
    } else {
        meta.previous_close
    };

    let market_price = meta.regular_market_price;
    let (market_change, market_change_pct) = if previous_close != 0.0 {
        let change = market_price - previous_close;
        (change, (change / previous_close) * 100.0)
    } else {
        (0.0, 0.0)
    };

    let symbol = if meta.symbol.trim().is_empty() {
        requested_symbol.to_string()
    } else {
        meta.symbol.clone()
    };

    Quote {
        symbol,
        currency: meta.currency.clone(),
        market_state: market_state(&meta.current_trading_period, now),
        market_price,
        market_change,
        market_change_pct,
        trend: trend_for_pct(market_change_pct),
        error: None,
    }
}

fn market_state(periods: &TradingPeriods, now: i64) -> String {
    if now < periods.pre.start {
        "CLOSED".to_string()
    } else if now < periods.regular.start {
        "PRE".to_string()
    } else if now <= periods.regular.end {
        "REGULAR".to_string()
    } else if now <= periods.post.end {
        "POST".to_string()
    } else {
        "CLOSED".to_string()
    }
}

fn trend_for_pct(pct: f64) -> String {
    if pct > 3.0 {
        "bigup".to_string()
    } else if pct > 0.0 {
        "up".to_string()
    } else if pct > -3.0 {
        "drop".to_string()
    } else {
        "bigdrop".to_string()
    }
}

fn market_icon(state: &str) -> &'static str {
    match state {
        "PRE" => "⏭",
        "REGULAR" => "▶",
        "POST" => "⏮",
        "CLOSED" => "⏹",
        _ => "?",
    }
}

fn trend_icon(trend: &str) -> &'static str {
    match trend {
        "bigup" => "⬆",
        "up" => "↗",
        "drop" => "↘",
        "bigdrop" => "⬇",
        _ => "?",
    }
}

fn trend_color(trend: &str) -> &'static str {
    match trend {
        "bigup" | "up" => "green",
        "drop" | "bigdrop" => "red",
        _ => "gray",
    }
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

/// Build the {"type": "table", ...} JSON payload from a list of quotes.
fn render_table(mut quotes: Vec<Quote>, sort: bool) -> serde_json::Value {
    if sort {
        quotes.sort_by(|a, b| {
            b.market_change_pct
                .partial_cmp(&a.market_change_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let rows: Vec<serde_json::Value> = quotes
        .iter()
        .map(|q| {
            if let Some(err) = &q.error {
                json!([
                    {"text": "?", "style": {}},
                    {"text": q.symbol, "style": {"bold": true}},
                    {"text": "—", "style": {}},
                    {"text": "?", "style": {}},
                    {"text": err, "style": {"fg": "gray"}}
                ])
            } else {
                json!([
                    {"text": market_icon(&q.market_state), "style": {}},
                    {"text": q.symbol, "style": {"bold": true}},
                    {"text": format!("{:.2} {}", q.market_price, q.currency), "style": {}},
                    {"text": trend_icon(&q.trend), "style": {"fg": trend_color(&q.trend)}},
                    {
                        "text": format!("{:+.2} ({:+.2}%)", q.market_change, q.market_change_pct),
                        "style": {"fg": trend_color(&q.trend)}
                    }
                ])
            }
        })
        .collect();

    json!({
        "type": "table",
        "headers": ["", "Symbol", "Price", "", "Change"],
        "rows": rows,
        "selectable": false
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: Settings = serde_json::from_str(&input).unwrap_or_default();

    if settings.symbols.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "Configure `symbols` for the yfinance plugin. Example: symbols = [\"AAPL\", \"MSFT\", \"GC=F\"].",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let now = get_unix_secs();
    let quotes: Vec<Quote> = settings
        .symbols
        .iter()
        .map(|s| quote_for_symbol(s, now))
        .collect();

    Ok(render_table(quotes, settings.sort).to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn periods(pre_start: i64, reg_start: i64, reg_end: i64, post_end: i64) -> TradingPeriods {
        TradingPeriods {
            pre: TradingWindow {
                start: pre_start,
                end: reg_start,
            },
            regular: TradingWindow {
                start: reg_start,
                end: reg_end,
            },
            post: TradingWindow {
                start: reg_end,
                end: post_end,
            },
        }
    }

    #[test]
    fn test_market_state_before_pre() {
        let p = periods(1000, 2000, 3000, 4000);
        assert_eq!(market_state(&p, 500), "CLOSED");
    }

    #[test]
    fn test_market_state_pre_market() {
        let p = periods(1000, 2000, 3000, 4000);
        assert_eq!(market_state(&p, 1500), "PRE");
    }

    #[test]
    fn test_market_state_regular() {
        let p = periods(1000, 2000, 3000, 4000);
        assert_eq!(market_state(&p, 2500), "REGULAR");
        assert_eq!(market_state(&p, 2000), "REGULAR");
        assert_eq!(market_state(&p, 3000), "REGULAR");
    }

    #[test]
    fn test_market_state_post_market() {
        let p = periods(1000, 2000, 3000, 4000);
        assert_eq!(market_state(&p, 3500), "POST");
        assert_eq!(market_state(&p, 4000), "POST");
    }

    #[test]
    fn test_market_state_after_post() {
        let p = periods(1000, 2000, 3000, 4000);
        assert_eq!(market_state(&p, 5000), "CLOSED");
    }

    #[test]
    fn test_trend_for_pct() {
        assert_eq!(trend_for_pct(5.0), "bigup");
        assert_eq!(trend_for_pct(3.1), "bigup");
        assert_eq!(trend_for_pct(1.0), "up");
        assert_eq!(trend_for_pct(0.01), "up");
        assert_eq!(trend_for_pct(0.0), "drop");
        assert_eq!(trend_for_pct(-1.0), "drop");
        assert_eq!(trend_for_pct(-2.99), "drop");
        assert_eq!(trend_for_pct(-3.5), "bigdrop");
    }

    #[test]
    fn test_market_icon() {
        assert_eq!(market_icon("PRE"), "⏭");
        assert_eq!(market_icon("REGULAR"), "▶");
        assert_eq!(market_icon("POST"), "⏮");
        assert_eq!(market_icon("CLOSED"), "⏹");
        assert_eq!(market_icon("?"), "?");
        assert_eq!(market_icon("unknown"), "?");
    }

    #[test]
    fn test_trend_icon() {
        assert_eq!(trend_icon("bigup"), "⬆");
        assert_eq!(trend_icon("up"), "↗");
        assert_eq!(trend_icon("drop"), "↘");
        assert_eq!(trend_icon("bigdrop"), "⬇");
        assert_eq!(trend_icon("?"), "?");
    }

    #[test]
    fn test_trend_color() {
        assert_eq!(trend_color("bigup"), "green");
        assert_eq!(trend_color("up"), "green");
        assert_eq!(trend_color("drop"), "red");
        assert_eq!(trend_color("bigdrop"), "red");
        assert_eq!(trend_color("?"), "gray");
    }

    #[test]
    fn test_encode_component() {
        assert_eq!(encode_component("AAPL"), "AAPL");
        assert_eq!(encode_component("GC=F"), "GC%3DF");
        assert_eq!(encode_component("BRK.B"), "BRK.B");
    }

    #[test]
    fn test_build_quote_positive_change() {
        let meta = ChartMeta {
            currency: "USD".to_string(),
            symbol: "AAPL".to_string(),
            regular_market_price: 110.0,
            chart_previous_close: 100.0,
            previous_close: 0.0,
            current_trading_period: periods(0, 100, 200, 300),
        };
        let quote = build_quote(&meta, "aapl", 150);
        assert_eq!(quote.symbol, "AAPL");
        assert_eq!(quote.currency, "USD");
        assert_eq!(quote.market_state, "REGULAR");
        assert_eq!(quote.market_price, 110.0);
        assert_eq!(quote.market_change, 10.0);
        assert!((quote.market_change_pct - 10.0).abs() < 1e-9);
        assert_eq!(quote.trend, "bigup");
        assert!(quote.error.is_none());
    }

    #[test]
    fn test_build_quote_falls_back_to_previous_close() {
        let meta = ChartMeta {
            currency: "USD".to_string(),
            symbol: "MSFT".to_string(),
            regular_market_price: 95.0,
            chart_previous_close: 0.0,
            previous_close: 100.0,
            current_trading_period: periods(0, 100, 200, 300),
        };
        let quote = build_quote(&meta, "msft", 150);
        assert_eq!(quote.market_change, -5.0);
        assert_eq!(quote.trend, "bigdrop");
    }

    #[test]
    fn test_build_quote_zero_previous_close_defaults_to_no_change() {
        let meta = ChartMeta {
            currency: "USD".to_string(),
            symbol: "GME".to_string(),
            regular_market_price: 20.0,
            chart_previous_close: 0.0,
            previous_close: 0.0,
            current_trading_period: periods(0, 100, 200, 300),
        };
        let quote = build_quote(&meta, "gme", 150);
        assert_eq!(quote.market_change, 0.0);
        assert_eq!(quote.market_change_pct, 0.0);
        assert_eq!(quote.trend, "drop");
    }

    #[test]
    fn test_build_quote_uses_requested_symbol_when_meta_symbol_missing() {
        let meta = ChartMeta {
            currency: "USD".to_string(),
            symbol: String::new(),
            regular_market_price: 50.0,
            chart_previous_close: 50.0,
            previous_close: 0.0,
            current_trading_period: periods(0, 100, 200, 300),
        };
        let quote = build_quote(&meta, "unknown-sym", 150);
        assert_eq!(quote.symbol, "unknown-sym");
    }

    #[test]
    fn test_quote_errored() {
        let quote = Quote::errored("BADSYM", "no results for symbol");
        assert_eq!(quote.symbol, "BADSYM");
        assert_eq!(quote.market_state, "?");
        assert_eq!(quote.trend, "?");
        assert_eq!(quote.error, Some("no results for symbol".to_string()));
    }

    #[test]
    fn test_render_table_success_row() {
        let quotes = vec![Quote {
            symbol: "AAPL".to_string(),
            currency: "USD".to_string(),
            market_state: "REGULAR".to_string(),
            market_price: 110.0,
            market_change: 10.0,
            market_change_pct: 10.0,
            trend: "bigup".to_string(),
            error: None,
        }];
        let value = render_table(quotes, false);
        assert_eq!(value["type"], "table");
        assert_eq!(value["rows"][0][1]["text"], "AAPL");
        assert_eq!(value["rows"][0][2]["text"], "110.00 USD");
    }

    #[test]
    fn test_render_table_error_row() {
        let quotes = vec![Quote::errored("BADSYM", "no results for symbol")];
        let value = render_table(quotes, false);
        assert_eq!(value["rows"][0][1]["text"], "BADSYM");
        assert_eq!(value["rows"][0][4]["text"], "no results for symbol");
    }

    #[test]
    fn test_render_table_sort_by_change_pct_descending() {
        let quotes = vec![
            Quote {
                symbol: "LOW".to_string(),
                currency: "USD".to_string(),
                market_state: "REGULAR".to_string(),
                market_price: 10.0,
                market_change: -1.0,
                market_change_pct: -5.0,
                trend: "bigdrop".to_string(),
                error: None,
            },
            Quote {
                symbol: "HIGH".to_string(),
                currency: "USD".to_string(),
                market_state: "REGULAR".to_string(),
                market_price: 10.0,
                market_change: 1.0,
                market_change_pct: 5.0,
                trend: "bigup".to_string(),
                error: None,
            },
        ];
        let value = render_table(quotes, true);
        assert_eq!(value["rows"][0][1]["text"], "HIGH");
        assert_eq!(value["rows"][1][1]["text"], "LOW");
    }
}
