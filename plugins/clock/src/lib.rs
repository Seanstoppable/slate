#[cfg(target_arch = "wasm32")]

#[cfg(target_arch = "wasm32")]
use serde_json::json;

#[cfg(target_arch = "wasm32")]
const CLOCK_ICON: &str = "\u{1F550}";



#[cfg(target_arch = "wasm32")]

#[plugin_fn]

pub fn metadata(_input: String) -> FnResult<String> {

    let meta = json!({

        "name": "Clocks",

        "description": "Displays multiple world clocks as a location list (wtfutil-style)",

        "version": env!("CARGO_PKG_VERSION"),

        "author": "Slate Community"

    });

    Ok(meta.to_string())

}



#[cfg(target_arch = "wasm32")]
fn get_local_time() -> (String, String) {

    use std::time::{SystemTime, UNIX_EPOCH};



    let secs = SystemTime::now()

        .duration_since(UNIX_EPOCH)

        .unwrap_or_default()

        .as_secs();



    format_time_and_date_from_unix_secs(secs)

}



fn format_time_and_date_from_unix_secs(secs: u64) -> (String, String) {

    let s = (secs % 60) as u8;

    let m = ((secs / 60) % 60) as u8;

    let h = ((secs / 3600) % 24) as u8;

    let time_str = format!("{:02}:{:02}:{:02}", h, m, s);



    let days = (secs / 86_400) as i64;

    let (year, month, day) = civil_from_days(days);

    let weekday = weekday_from_days(days);

    let date_str = format!("{}, {} {:02}, {}", weekday, month_name(month), day, year);



    (time_str, date_str)

}



fn civil_from_days(days: i64) -> (i64, u32, u32) {

    let z = days + 719468;

    let era = if z >= 0 { z } else { z - 146096 } / 146097;

    let doe = (z - era * 146097) as u64;

    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;

    let y = yoe as i64 + era * 400;

    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);

    let mp = (5 * doy + 2) / 153;

    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;

    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;

    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d)

}



fn weekday_from_days(days: i64) -> &'static str {

    let w = ((days % 7) + 4) % 7;

    match w {

        0 => "Sun",

        1 => "Mon",

        2 => "Tue",

        3 => "Wed",

        4 => "Thu",

        5 => "Fri",

        6 => "Sat",

        _ => "???",

    }

}



fn month_name(m: u32) -> &'static str {

    match m {

        1 => "Jan",

        2 => "Feb",

        3 => "Mar",

        4 => "Apr",

        5 => "May",

        6 => "Jun",

        7 => "Jul",

        8 => "Aug",

        9 => "Sep",

        10 => "Oct",

        11 => "Nov",

        12 => "Dec",

        _ => "???",

    }

}



#[cfg(target_arch = "wasm32")]

#[plugin_fn]

pub fn refresh(input: String) -> FnResult<String> {

    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();



    let clocks = settings["clocks"].as_array();



    let content = if let Some(clocks) = clocks {

        if clocks.is_empty() {

            let (time, date) = get_local_time();

            json!({

                "type": "text",

                "content": format!("  {}  {}\n  {}\n  UTC", CLOCK_ICON, time, date),

                "scrollable": false,

                "wrap": false

            })

        } else {

            let label_width = clocks

                .iter()

                .filter_map(|c| c["label"].as_str())

                .map(|l| l.len())

                .max()

                .unwrap_or(12)

                .max(12);



            let mut lines = Vec::new();

            for clock in clocks {

                let label = clock["label"].as_str().unwrap_or("???");

                let time = clock["time"].as_str().unwrap_or("--:--");

                let date = clock["date"].as_str().unwrap_or("---");

                lines.push(format!(

                    "  {:<width$}  {}  {}",

                    label,

                    time,

                    date,

                    width = label_width

                ));

            }



            json!({

                "type": "text",

                "content": lines.join("\n"),

                "scrollable": false,

                "wrap": false

            })

        }

    } else {

        let (time, date) = get_local_time();

        let tz_display = settings["timezone"].as_str().unwrap_or("UTC");



        json!({

            "type": "text",

            "content": format!("  {}  {}\n  {}\n  {}", CLOCK_ICON, time, date, tz_display),

            "scrollable": false,

            "wrap": false

        })

    };



    Ok(content.to_string())

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



    #[test]

    fn test_civil_from_days_known_dates() {

        assert_eq!(civil_from_days(0), (1970, 1, 1));

        assert_eq!(civil_from_days(18_628), (2021, 1, 1));

        assert_eq!(civil_from_days(-1), (1969, 12, 31));

        assert_eq!(civil_from_days(-365), (1969, 1, 1));

    }



    #[test]

    fn test_weekday_from_days_known_values() {

        assert_eq!(weekday_from_days(0), "Thu");

        assert_eq!(weekday_from_days(1), "Fri");

        assert_eq!(weekday_from_days(2), "Sat");

        assert_eq!(weekday_from_days(3), "Sun");

        assert_eq!(weekday_from_days(-1), "Wed");

    }



    #[test]

    fn test_month_name_all_values() {

        let months = [

            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",

        ];



        for (index, expected) in months.iter().enumerate() {

            assert_eq!(month_name(index as u32 + 1), *expected);

        }



        assert_eq!(month_name(0), "???");

        assert_eq!(month_name(13), "???");

    }



    #[test]

    fn test_format_time_and_date_from_unix_secs_epoch() {

        let (time, date) = format_time_and_date_from_unix_secs(0);

        assert_eq!(time, "00:00:00");

        assert_eq!(date, "Thu, Jan 01, 1970");

    }



    #[test]

    fn test_format_time_and_date_from_unix_secs_rollover() {

        let (time, date) = format_time_and_date_from_unix_secs(86_399);

        assert_eq!(time, "23:59:59");

        assert_eq!(date, "Thu, Jan 01, 1970");



        let (time, date) = format_time_and_date_from_unix_secs(86_400 + 3_661);

        assert_eq!(time, "01:01:01");

        assert_eq!(date, "Fri, Jan 02, 1970");

    }

}
