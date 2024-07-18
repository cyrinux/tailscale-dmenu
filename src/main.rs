use dirs::config_dir;
use notify_rust::Notification;
use reqwest::blocking::get;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use which::which;

mod iwd;
mod mullvad;
mod networkmanager;

use iwd::{connect_to_iwd_wifi, get_iwd_networks};
use mullvad::{get_mullvad_actions, set_mullvad_exit_node};
use networkmanager::{connect_to_nm_wifi, get_nm_wifi_networks};

#[derive(Deserialize)]
struct Action {
    display: String,
    cmd: String,
}

#[derive(Deserialize)]
struct Config {
    actions: Vec<Action>,
}

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
display = "🛡️ Shields up"
cmd = "tailscale set --shields-up=true"

[[actions]]
display = "🛡️ Shields down"
cmd = "tailscale set --shields-up=false"
"#
}

fn get_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let config_dir = config_dir().ok_or("Failed to find config directory")?;
    Ok(config_dir.join("tailscale-dmenu").join("config.toml"))
}

fn create_default_config_if_missing() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&config_path, get_default_config())?;
    }
    Ok(())
}

fn get_config() -> Result<Config, Box<dyn std::error::Error>> {
    let config_path = get_config_path()?;
    let config_content = fs::read_to_string(config_path)?;
    let config = toml::from_str(&config_content)?;
    Ok(config)
}

/// Retrieves the list of actions to display in the dmenu.
fn get_actions() -> Result<Vec<String>, Box<dyn Error>> {
    let config = get_config()?;
    let mut actions = config
        .actions
        .into_iter()
        .map(|action| format!("action - {}", action.display))
        .collect::<Vec<_>>();

    if is_command_installed("tailscale") {
        actions.extend(get_mullvad_actions());
    }

    if is_command_installed("nmcli") {
        actions.extend(get_nm_wifi_networks()?);
    } else if is_command_installed("iwctl") {
        actions.extend(get_iwd_networks()?);
    }

    Ok(actions)
}

fn set_action(action: &str) -> Result<bool, Box<dyn std::error::Error>> {
    if is_command_installed("nmcli") {
        connect_to_nm_wifi(action)?;
    } else if is_command_installed("iwctl") && !is_command_installed("nmcli") {
        connect_to_iwd_wifi(action)?;
    }

    if is_command_installed("tailscale") {
        set_mullvad_exit_node(action);
        check_mullvad()?;
    }

    let config = get_config()?;
    if let Some(action_config) = config
        .actions
        .iter()
        .find(|a| format!("action - {}", a.display) == action)
    {
        #[cfg(debug_assertions)]
        eprintln!("Executing command: {}", action_config.cmd);

        let status = Command::new("sh")
            .arg("-c")
            .arg(&action_config.cmd)
            .status()?;

        if status.success() {
            return Ok(true);
        }

        #[cfg(debug_assertions)]
        eprintln!("Command executed with non-zero exit status: {}", status);
    }

    Ok(false)
}

fn check_mullvad() -> Result<(), Box<dyn std::error::Error>> {
    let response = get("https://am.i.mullvad.net/connected")?.text()?;
    Notification::new()
        .summary("Connected Status")
        .body(response.trim())
        .show()?;
    Ok(())
}

fn is_command_installed(cmd: &str) -> bool {
    which(cmd).is_ok()
}

fn main() -> Result<(), Box<dyn Error>> {
    create_default_config_if_missing()?;

    let actions = get_actions()?;
    let action = {
        let mut child = Command::new("dmenu")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        {
            let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
            write!(stdin, "{}", actions.join("\n"))?;
        }

        let output = child.wait_with_output()?;
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    if !action.is_empty() {
        set_action(&action)?;
    }

    #[cfg(debug_assertions)]
    {
        Command::new("tailscale").arg("status").status()?;
    }

    Ok(())
}

fn notify_connection(ssid: &str) -> Result<(), Box<dyn std::error::Error>> {
    Notification::new()
        .summary("Wi-Fi")
        .body(&format!("Connected to {}", ssid))
        .show()?;
    Ok(())
}
