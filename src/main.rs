use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use dirs::config_dir;
use reqwest::blocking::get;
use serde::Deserialize;

#[cfg(feature = "iwd")]
mod iwd;
mod mullvad;
#[cfg(feature = "networkmanager")]
mod networkmanager;

#[cfg(feature = "iwd")]
use iwd::{connect_to_iwd_wifi, get_iwd_networks};
use mullvad::{get_mullvad_actions, set_mullvad_exit_node};
#[cfg(feature = "networkmanager")]
use networkmanager::{connect_to_wifi, get_wifi_networks};

/// Represents an action that can be taken, including the display name and the command to execute.
#[derive(Deserialize)]
struct Action {
    display: String,
    cmd: String,
}

/// Represents the configuration, including a list of actions.
#[derive(Deserialize)]
struct Config {
    actions: Vec<Action>,
}

/// Returns the default configuration as a string.
fn get_default_config() -> &'static str {
    r#"
[[actions]]
display = "❌ - Disable mullvad"
cmd = "tailscale set --exit-node="

[[actions]]
display = "❌ - Disable tailscale"
cmd = "tailscale down"

[[actions]]
display = "✅ - Enable tailscale"
cmd = "tailscale up"

[[actions]]
display = "🌿 RaspberryPi"
cmd = "tailscale set --exit-node-allow-lan-access --exit-node=raspberrypi"

[[actions]]
display = "wifi scan"
cmd = "nmcli dev wifi list --rescan yes"

[[actions]]
display = "🛡️ Shields up"
cmd = "tailscale set --shields-up=true"

[[actions]]
display = "🛡️ Shields down"
cmd = "tailscale set --shields-up=false"
"#
}

/// Returns the path to the configuration file.
fn get_config_path() -> PathBuf {
    let config_dir = config_dir().expect("Failed to find config directory");
    config_dir.join("tailscale-dmenu").join("config.toml")
}

/// Creates the default configuration file if it doesn't already exist.
fn create_default_config_if_missing() {
    let config_path = get_config_path();

    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create config directory");
        }

        fs::write(&config_path, get_default_config()).expect("Failed to write default config");
    }
}

/// Reads and parses the configured actions from the configuration file.
fn get_config() -> Config {
    let config_path = get_config_path();
    let config_content = fs::read_to_string(config_path).expect("Failed to read config file");
    toml::from_str(&config_content).expect("Failed to parse config file")
}

/// Retrieves the list of actions to display in the dmenu.
fn get_actions() -> Vec<String> {
    let config = get_config();
    let mut actions = config
        .actions
        .into_iter()
        .map(|action| format!("action - {}", action.display))
        .collect::<Vec<_>>();

    actions.extend(get_mullvad_actions());

    #[cfg(feature = "networkmanager")]
    actions.extend(get_wifi_networks());
    #[cfg(feature = "iwd")]
    actions.extend(get_iwd_networks());

    actions
}

/// Executes the command associated with the selected action.
fn set_action(action: &str) {
    if set_mullvad_exit_node(action) {
        // Post-action for Mullvad
        let response = get("https://am.i.mullvad.net/connected")
            .expect("Failed to make request")
            .text()
            .expect("Failed to read response text");

        let notification = format!("notify-send 'Connected Status' '{}'", response.trim());

        Command::new("sh")
            .arg("-c")
            .arg(notification)
            .status()
            .expect("Failed to send notification");
    } else {
        let connected = {
            #[cfg(feature = "networkmanager")]
            {
                connect_to_wifi(action)
            }
            #[cfg(feature = "iwd")]
            {
                connect_to_iwd_wifi(action)
            }
        };

        if connected {
            return;
        }
    }

    // Handle configured actions
    let config = get_config();
    if let Some(action) = config
        .actions
        .iter()
        .find(|a| format!("action - {}", a.display) == action)
    {
        eprintln!("Executing command: {}", action.cmd);

        let status = Command::new("sh").arg("-c").arg(&action.cmd).status();

        match status {
            Ok(status) => {
                if !status.success() {
                    eprintln!("Command executed with non-zero exit status: {}", status);
                }
            }
            Err(err) => {
                eprintln!("Failed to execute command: {:?}", err);
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

        let output = child
            .wait_with_output()
            .expect("Failed to read dmenu output");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    if !action.is_empty() {
        set_action(&action);
    }

    // Display Tailscale status only if in debug mode
    #[cfg(debug_assertions)]
    {
        Command::new("tailscale")
            .arg("status")
            .status()
            .expect("Failed to get tailscale status");
    }
}
