#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
#[cfg(target_arch = "wasm32")]
use serde::Deserialize;
#[cfg(target_arch = "wasm32")]
use serde_json::json;

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct IpInfoResponse {

    ip: String,

    #[serde(default)]

    city: String,

    #[serde(default)]

    region: String,

    #[serde(default)]

    country: String,

    #[serde(default)]

    org: String,

    #[serde(default)]

    timezone: String,

}



#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct IpApiResponse {

    #[serde(default)]

    query: String,

    #[serde(default)]

    city: String,

    #[serde(default)]

    district: String,

    #[serde(default)]

    region: String,

    #[serde(rename = "regionName", default)]

    region_name: String,

    #[serde(default)]

    country: String,

    #[serde(rename = "countryCode", default)]

    country_code: String,

    #[serde(default)]

    continent: String,

    #[serde(rename = "continentCode", default)]

    continent_code: String,

    #[serde(default)]

    lat: f64,

    #[serde(default)]

    lon: f64,

    #[serde(default)]

    isp: String,

    #[serde(default)]

    org: String,

    #[serde(rename = "as", default)]

    as_info: String,

    #[serde(rename = "asname", default)]

    as_name: String,

    #[serde(default)]

    timezone: String,

    #[serde(default)]

    currency: String,

    #[serde(default)]

    reverse: String,

    #[serde(default)]

    zip: String,

}



#[cfg(target_arch = "wasm32")]
const IPAPI_DEFAULT_ARGS: &[&str] = &[

    "ip",

    "city",

    "regionName",

    "country",

    "isp",

    "timezone",

];



#[cfg(target_arch = "wasm32")]

#[plugin_fn]

pub fn metadata(_input: String) -> FnResult<String> {

    let meta = json!({

        "name": "IP Info",

        "description": "Shows public IP and geolocation (ipinfo.io or ip-api.com)",

        "version": env!("CARGO_PKG_VERSION"),

        "author": "Slate Community"

    });

    Ok(meta.to_string())

}



#[cfg(target_arch = "wasm32")]

#[plugin_fn]

pub fn refresh(input: String) -> FnResult<String> {

    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let backend = settings["backend"].as_str().unwrap_or("ipinfo");



    let content = match backend {

        "ipapi" => {

            let args: Vec<String> = settings["args"]

                .as_array()

                .map(|arr| {

                    arr.iter()

                        .filter_map(|v| v.as_str().map(String::from))

                        .collect()

                })

                .unwrap_or_else(|| IPAPI_DEFAULT_ARGS.iter().map(|s| s.to_string()).collect());

            match fetch_from_ipapi(&args) {

                Ok(pairs) => json!({ "type": "key_value", "pairs": pairs }),

                Err(e) => json!({

                    "type": "text",

                    "content": format!("Error: {}", e),

                    "scrollable": false,

                    "wrap": true

                }),

            }

        }

        _ => match fetch_from_ipinfo() {

            Ok(pairs) => json!({ "type": "key_value", "pairs": pairs }),

            Err(e) => json!({

                "type": "text",

                "content": format!("Error: {}", e),

                "scrollable": false,

                "wrap": true

            }),

        },

    };



    Ok(content.to_string())

}



#[cfg(target_arch = "wasm32")]
fn fetch_from_ipinfo() -> Result<Vec<serde_json::Value>, Error> {
    let info: IpInfoResponse =
        slate_plugin_http::get_json("https://ipinfo.io/json", &[("Accept", "application/json")])
            .map_err(|e| Error::msg(format!("Failed to parse ipinfo.io response: {}", e)))?;

    Ok(vec![
        json!({"key": "IP", "value": info.ip}),
        json!({"key": "Location", "value": format!("{}, {}, {}", info.city, info.region, info.country)}),
        json!({"key": "Org", "value": info.org}),
        json!({"key": "Timezone", "value": info.timezone}),
    ])
}

#[cfg(target_arch = "wasm32")]
fn fetch_from_ipapi(args: &[String]) -> Result<Vec<serde_json::Value>, Error> {
    let api_fields = args
        .iter()
        .map(|a| arg_to_api_field(a))
        .collect::<Vec<_>>()
        .join(",");
    let url = format!("http://ip-api.com/json/?fields={}", api_fields);

    let info: IpApiResponse =
        slate_plugin_http::get_json(&url, &[("Accept", "application/json")])
            .map_err(|e| Error::msg(format!("Failed to parse ip-api.com response: {}", e)))?;



    let pairs: Vec<serde_json::Value> = args

        .iter()

        .filter_map(|arg| {

            let (label, value) = match arg.as_str() {

                "ip" => ("IP", info.query.clone()),

                "isp" => ("ISP", info.isp.clone()),

                "as" => ("AS", info.as_info.clone()),

                "asName" => ("AS Name", info.as_name.clone()),

                "city" => ("City", info.city.clone()),

                "district" => ("District", info.district.clone()),

                "region" => ("Region", info.region.clone()),

                "regionName" => ("Region", info.region_name.clone()),

                "country" => ("Country", info.country.clone()),

                "countryCode" => ("Country Code", info.country_code.clone()),

                "continent" => ("Continent", info.continent.clone()),

                "continentCode" => ("Continent Code", info.continent_code.clone()),

                "coordinates" => ("Coordinates", format!("{}, {}", info.lat, info.lon)),

                "postalCode" => ("Postal Code", info.zip.clone()),

                "currency" => ("Currency", info.currency.clone()),

                "organization" => ("Organization", info.org.clone()),

                "timezone" => ("Timezone", info.timezone.clone()),

                "reverseDNS" => ("Reverse DNS", info.reverse.clone()),

                _ => return None,

            };

            if value.is_empty() {

                return None;

            }

            Some(json!({"key": label, "value": value}))

        })

        .collect();



    Ok(pairs)

}



fn arg_to_api_field(arg: &str) -> &str {

    match arg {

        "ip" => "query",

        "coordinates" => "lat,lon",

        "postalCode" => "zip",

        "organization" => "org",

        "reverseDNS" => "reverse",

        other => other,

    }

}



#[cfg(target_arch = "wasm32")]

#[plugin_fn]

pub fn on_key(_input: String) -> FnResult<String> {

    Ok(String::new())

}



#[cfg(test)]

mod tests {

    use super::*;



    #[test]

    fn test_arg_to_api_field_mappings() {

        assert_eq!(arg_to_api_field("ip"), "query");

        assert_eq!(arg_to_api_field("coordinates"), "lat,lon");

        assert_eq!(arg_to_api_field("postalCode"), "zip");

        assert_eq!(arg_to_api_field("organization"), "org");

        assert_eq!(arg_to_api_field("reverseDNS"), "reverse");

    }



    #[test]

    fn test_arg_to_api_field_passthrough() {

        assert_eq!(arg_to_api_field("timezone"), "timezone");

        assert_eq!(arg_to_api_field("countryCode"), "countryCode");

    }

}
