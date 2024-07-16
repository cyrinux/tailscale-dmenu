use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use dirs::config_dir;
use regex::Regex;
use serde::Deserialize;

#[derive(Deserialize)]
struct Action {
    display: String,
    value: String,
    cmd: String,
}

#[derive(Deserialize)]
struct Config {
    actions: Vec<Action>,
}

fn get_flag(country: &str) -> &'static str {
    let country_flags: HashMap<&str, &str> = [
        ("Albania", "🇦🇱"), ("Australia", "🇦🇺"), ("Austria", "🇦🇹"),
        ("Belgium", "🇧🇪"), ("Brazil", "🇧🇷"), ("Bulgaria", "🇧🇬"),
        ("Canada", "🇨🇦"), ("Chile", "🇨🇱"), ("Colombia", "🇨🇴"),
        ("Croatia", "🇭🇷"), ("Czech Republic", "🇨🇿"), ("Denmark", "🇩🇰"),
        ("Estonia", "🇪🇪"), ("Finland", "🇫🇮"), ("France", "🇫🇷"),
        ("Germany", "🇩🇪"), ("Greece", "🇬🇷"), ("Hong Kong", "🇭🇰"),
        ("Hungary", "🇭🇺"), ("Indonesia", "🇮🇩"), ("Ireland", "🇮🇪"),
        ("Israel", "🇮🇱"), ("Italy", "🇮🇹"), ("Japan", "🇯🇵"),
        ("Latvia", "🇱🇻"), ("Mexico", "🇲🇽"), ("Netherlands", "🇳🇱"),
        ("New Zealand", "🇳🇿"), ("Norway", "🇳🇴"), ("Poland", "🇵🇱"),
        ("Portugal", "🇵🇹"), ("Romania", "🇷🇴"), ("Serbia", "🇷🇸"),
        ("Singapore", "🇸🇬"), ("Slovakia", "🇸🇰"), ("Slovenia", "🇸🇮"),
        ("South Africa", "🇿🇦"), ("Spain", "🇪🇸"), ("Sweden", "🇸🇪"),
        ("Switzerland", "🇨🇭"), ("Thailand", "🇹🇭"), ("Turkey", "🇹🇷"),
        ("UK", "🇬🇧"), ("Ukraine", "🇺🇦"), ("USA", "🇺🇸")
    ].iter().cloned().collect();
    *country_flags.get(country).unwrap_or(&"❓")
}

fn get_default_config() -> &'static str {
    r#"
[[actions]]
display = "❌ - Disable mullvad"
value = "disable_mullvad"
cmd = "tailscale set --exit-node= --exit-node-allow-lan-access=false"

[[actions]]
display = "❌ - Disable tailscale"
value = "disable_tailscale"
cmd = "tailscale down"

[[actions]]
display = "✅ - Enable tailscale"
value = "enable_tailscale"
cmd = "tailscale up"
"#
}

fn get_config_path() -> PathBuf {
    let config_dir = config_dir().expect("Failed to find config directory");
    config_dir.join("tailscale-dmenu").join("config.toml")
}

fn create_default_config_if_missing() {
    let config_path = get_config_path();

    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create config directory");
        }

        fs::write(&config_path, get_default_config()).expect("Failed to write default config");
    }
}

fn get_configured_actions() -> Vec<Action> {
    let config_path = get_config_path();
    let config_content = fs::read_to_string(config_path).expect("Failed to read config file");
    let config: Config = toml::from_str(&config_content).expect("Failed to parse config file");
    config.actions
}

fn get_actions() -> Vec<String> {
    let mut actions = get_configured_actions()
        .into_iter()
        .map(|action| format!("{} - {}", action.display, action.value))
        .collect::<Vec<_>>();

    let output = Command::new("tailscale")
        .arg("exit-node")
        .arg("list")
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let regex = Regex::new(r"\s{2,}").unwrap();
        let mut lines: Vec<String> = reader.lines()
            .filter_map(Result::ok)
            .filter(|line| line.contains("mullvad.ts.net"))
            .map(|line| {
                let parts: Vec<&str> = regex.split(&line).collect();
                let country = parts.get(2).unwrap_or(&"");
                let node_name = parts.get(1).unwrap_or(&"");
                format!("{} {} - {}", get_flag(country), country, node_name)
            })
            .collect();

        lines.sort_by(|a, b| a.split_whitespace().next().cmp(&b.split_whitespace().next()));
        actions.extend(lines);
    }

    actions
}

fn set_action(action: &str) {
    let regex = Regex::new(r" - ([\w_.-]+)$").unwrap();
    if let Some(caps) = regex.captures(action) {
        let action_value = caps.get(1).map_or("", |m| m.as_str());

        let configured_actions = get_configured_actions();
        if let Some(action) = configured_actions.iter().find(|a| a.value == action_value) {
            let cmd = &action.cmd;
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            let (cmd, args) = parts.split_first().expect("Failed to parse command");

            // Debug log the command and its arguments
            eprintln!("Executing command: {} {:?}", cmd, args);

            let status = Command::new(cmd)
                .args(args)
                .status();

            match status {
                Ok(status) => {
                    if !status.success() {
                        eprintln!("Command executed with non-zero exit status: {}", status);
                    }
                },
                Err(err) => {
                    eprintln!("Failed to execute command: {:?}", err);
                }
            }
        } else {
            // Handle exit node selection
            let node_name = action_value;
            let status = Command::new("tailscale")
                .arg("set")
                .arg("--exit-node")
                .arg(node_name)
                .arg("--exit-node-allow-lan-access=true")
                .status();

            match status {
                Ok(status) => {
                    if !status.success() {
                        eprintln!("Command executed with non-zero exit status: {}", status);
                    }
                },
                Err(err) => {
                    eprintln!("Failed to execute command: {:?}", err);
                }
            }
        }
    }
}

fn main() {
    create_default_config_if_missing();

    let actions = get_actions();
    let action = {
        let mut child = Command::new("dmenu")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to execute dmenu");

        {
            let stdin = child.stdin.as_mut().expect("Failed to open stdin");
            write!(stdin, "{}", actions.join("\n")).expect("Failed to write to stdin");
        }

        let output = child.wait_with_output().expect("Failed to read dmenu output");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    if !action.is_empty() {
        set_action(&action);
    }

    Command::new("tailscale")
        .arg("status")
        .status()
        .expect("Failed to get tailscale status");
}
