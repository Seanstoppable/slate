#[cfg(target_arch = "wasm32")]
use extism_pdk::*;
use serde::Deserialize;
#[cfg(target_arch = "wasm32")]
use serde_json::json;



#[derive(Deserialize)]

struct Container {

    #[serde(rename = "ID", default)]

    id: String,

    #[serde(rename = "Names", default)]

    names: String,

    #[serde(rename = "Image", default)]

    image: String,

    #[serde(rename = "Status", default)]

    status: String,

    #[serde(rename = "State", default)]

    state: String,

    #[serde(rename = "Ports", default)]

    ports: String,

}



#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct ExecResult {

    #[serde(default)]

    stdout: String,

    #[serde(default)]

    stderr: String,

    #[serde(default)]

    exit_code: i32,

}



fn state_icon(state: &str) -> &'static str {

    match state {

        "running" => "\u{1F7E2}",

        "exited" => "\u{1F534}",

        "paused" => "\u{23F8}",

        "restarting" => "\u{1F504}",

        _ => "\u{26AA}",

    }

}



fn format_subtitle(image: &str, ports: &str) -> String {

    if ports.is_empty() {

        image.to_string()

    } else {

        format!("{} | {}", image, ports)

    }

}



#[cfg(target_arch = "wasm32")]

#[plugin_fn]

pub fn metadata(_input: String) -> FnResult<String> {

    let meta = json!({

        "name": "Docker",

        "description": "Shows Docker container status",

        "version": env!("CARGO_PKG_VERSION"),

        "author": "Slate Community"

    });

    Ok(meta.to_string())

}



#[cfg(target_arch = "wasm32")]

#[plugin_fn]

pub fn refresh(_input: String) -> FnResult<String> {

    let result = run_docker_exec("docker", &["ps", "-a", "--format", "json", "--no-trunc"])?;



    if result.exit_code != 0 {

        let content = json!({

            "type": "text",

            "content": format!(

                "Docker error: {}",

                if result.stderr.is_empty() {

                    "Is Docker running?"

                } else {

                    &result.stderr

                }

            ),

            "scrollable": false,

            "wrap": true

        });

        return Ok(content.to_string());

    }



    let mut items = Vec::new();



    for line in result.stdout.lines() {

        let line = line.trim();

        if line.is_empty() {

            continue;

        }



        if let Ok(container) = serde_json::from_str::<Container>(line) {

            let subtitle = format_subtitle(&container.image, &container.ports);



            items.push(json!({

                "id": container.id,

                "title": format!("{} {}", state_icon(&container.state), container.names),

                "subtitle": format!("{} - {}", container.status, subtitle),

                "style": {}

            }));

        }

    }



    if items.is_empty() {

        let content = json!({

            "type": "text",

            "content": "No containers found.",

            "scrollable": false,

            "wrap": true

        });

        return Ok(content.to_string());

    }



    let content = json!({

        "type": "list",

        "items": items,

        "selectable": true,

        "actions": [

            {"id": "start", "label": "Start", "key": "s", "confirm": false},

            {"id": "stop", "label": "Stop", "key": "x", "confirm": true},

            {"id": "restart", "label": "Restart", "key": "r", "confirm": true},

            {"id": "logs", "label": "View logs", "key": "l", "confirm": false}

        ]

    });



    Ok(content.to_string())

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

        action_id: String,

        item_id: String,

    }



    if let Ok(action) = serde_json::from_str::<ActionInput>(&input) {

        let container_id = &action.item_id;

        match action.action_id.as_str() {

            "start" => {

                let _ = run_docker_exec("docker", &["start", container_id]);

            }

            "stop" => {

                let _ = run_docker_exec("docker", &["stop", container_id]);

            }

            "restart" => {

                let _ = run_docker_exec("docker", &["restart", container_id]);

            }

            "logs" => {

                if let Ok(result) = run_docker_exec("docker", &["logs", "--tail", "50", container_id]) {

                    let logs = if result.stdout.is_empty() {

                        result.stderr

                    } else {

                        result.stdout

                    };

                    let content = json!({

                        "type": "text",

                        "content": logs,

                        "scrollable": true,

                        "wrap": true

                    });

                    return Ok(content.to_string());

                }

            }

            _ => {}

        }

    }

    Ok(String::new())

}



#[cfg(target_arch = "wasm32")]
#[host_fn]
extern "ExtismHost" {
    fn exec_command(input: String) -> String;
}

#[cfg(target_arch = "wasm32")]
fn run_docker_exec(cmd: &str, args: &[&str]) -> Result<ExecResult, Error> {
    let request = json!({"cmd": cmd, "args": args}).to_string();
    let output = unsafe { exec_command(request)? };
    serde_json::from_str(&output).map_err(|e| Error::msg(e.to_string()))
}



#[cfg(test)]

mod tests {

    use super::*;



    #[test]

    fn test_container_deserialization() {

        let json = r#"{"ID":"abc123","Names":"web","Image":"nginx:latest","Status":"Up 2 hours","State":"running","Ports":"0.0.0.0:80->80/tcp"}"#;

        let container: Container = serde_json::from_str(json).unwrap();



        assert_eq!(container.id, "abc123");

        assert_eq!(container.names, "web");

        assert_eq!(container.image, "nginx:latest");

        assert_eq!(container.status, "Up 2 hours");

        assert_eq!(container.state, "running");

        assert_eq!(container.ports, "0.0.0.0:80->80/tcp");

    }



    #[test]

    fn test_state_icon_variants() {

        assert_eq!(state_icon("running"), "\u{1F7E2}");

        assert_eq!(state_icon("exited"), "\u{1F534}");

        assert_eq!(state_icon("paused"), "\u{23F8}");

        assert_eq!(state_icon("restarting"), "\u{1F504}");

        assert_eq!(state_icon("created"), "\u{26AA}");

    }



    #[test]

    fn test_format_subtitle_without_ports() {

        assert_eq!(format_subtitle("redis:7", ""), "redis:7");

    }



    #[test]

    fn test_format_subtitle_with_ports() {

        assert_eq!(

            format_subtitle("redis:7", "6379/tcp"),

            "redis:7 | 6379/tcp"

        );

    }

}
