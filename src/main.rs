use clap::Parser;
use dirs::config_dir;
use notify_rust::Notification;
use reqwest::blocking::get;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use which::which;

mod iwd;
mod mullvad;
mod networkmanager;

use iwd::{connect_to_iwd_wifi, disconnect_iwd_wifi, get_iwd_networks};
use mullvad::{get_mullvad_actions, set_mullvad_exit_node};
use networkmanager::{connect_to_nm_wifi, get_nm_wifi_networks};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "wlan0")]
    wifi_interface: String,
}

#[derive(Deserialize)]
struct Action {
    display: String,
    cmd: String,
}

#[derive(Deserialize)]
struct Config {
    actions: Vec<Action>,
    dmenu_cmd: String,
    dmenu_args: String,
}

fn get_default_config() -> &'static str {
    r#"
dmenu_cmd = "dmenu"
dmenu_args = "--no-multi"

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
display = "🌿 - RaspberryPi"
cmd = "tailscale set --exit-node-allow-lan-access --exit-node=raspberrypi"

[[actions]]
display = "🛡️ - Shields up"
cmd = "tailscale set --shields-up=true"

[[actions]]
display = "🛡️ - Shields down"
cmd = "tailscale set --shields-up=false"
"#
}

fn get_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let config_dir = config_dir().ok_or("Failed to find config directory")?;
    Ok(config_dir.join("network-dmenu").join("config.toml"))
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

fn get_actions(args: &Args) -> Result<Vec<String>, Box<dyn Error>> {
    let config = get_config()?;
    let mut actions = config
        .actions
        .into_iter()
        .map(|action| format!("{:<8}- {}", "action", action.display))
        .collect::<Vec<_>>();

    if is_command_installed("rfkill") {
        actions.push(format!("{:<8}- 📶 - Radio wifi rfkill ON", "action"));
        actions.push(format!("{:<8}- 📶 - Radio wifi rfkill OFF", "action"));
    }

    if is_command_installed("nm-connection-editor") {
        actions.push(format!("{:<8}- 📶 - Edit connections", "action"));
    }

    if is_command_installed("nmcli") {
        actions.push(format!("{:<8}- 📶 - Disconnect wifi", "action"));
        actions.push(format!("{:<8}- 📶 - Connect wifi", "action"));
    } else if is_command_installed("iwctl") {
        actions.push(format!("{:<8}- 📶 - Disconnect wifi", "action"));
    }

    if is_command_installed("rfkill") {
        actions.extend(get_mullvad_actions());
    }

    if is_command_installed("tailscale") {
        actions.extend(get_mullvad_actions());
    }

    if is_command_installed("nmcli") {
        actions.extend(get_nm_wifi_networks()?);
    } else if is_command_installed("iwctl") {
        actions.extend(get_iwd_networks(&args.wifi_interface)?);
    }

    Ok(actions)
}

fn handle_custom_action(
    action: &str,
    config: &Config,
    wifi_interface: &str,
) -> Result<bool, Box<dyn Error>> {
    if let Some(action_config) = config
        .actions
        .iter()
        .find(|a| format!("{:<8}- {}", "action", a.display) == action)
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

    if action.contains("📶 - Radio wifi rfkill ON") {
        let status = Command::new("rfkill").arg("unblock").arg("wlan").status()?;
        return Ok(status.success());
    }

    if action.contains("📶 - Radio wifi rfkill OFF") {
        let status = Command::new("rfkill").arg("block").arg("wlan").status()?;
        return Ok(status.success());
    }

    if action.contains("📶 - Edit connections") {
        let status = Command::new("nm-connection-editor").status()?;
        return Ok(status.success());
    }

    if action.contains("📶 - Disconnect wifi") {
        let status = if is_command_installed("nmcli") {
            disconnect_nm_wifi(wifi_interface)?
        } else {
            disconnect_iwd_wifi(wifi_interface)?
        };
        return Ok(status);
    }

    if action.contains("📶 - Connect wifi") {
        let status = Command::new("nmcli")
            .arg("device")
            .arg("connect")
            .arg(wifi_interface)
            .status()?;
        return Ok(status.success());
    }

    Ok(false)
}

fn set_action(wifi_interface: &str, action: &str) -> Result<bool, Box<dyn Error>> {
    let config = get_config()?;

    if handle_custom_action(action, &config, wifi_interface)? {
        return Ok(true);
    }

    if is_command_installed("nmcli") {
        connect_to_nm_wifi(action)?;
    } else if is_command_installed("iwctl") && !is_command_installed("nmcli") {
        connect_to_iwd_wifi(wifi_interface, action)?;
    }

    if is_command_installed("tailscale") {
        set_mullvad_exit_node(action);
        check_mullvad()?;
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
    let args = Args::parse();

    create_default_config_if_missing()?;

    let config = get_config()?;

    if !is_command_installed("pinentry-gnome3") || !is_command_installed(&config.dmenu_cmd) {
        panic!("pinentry-gnome3 or dmenu command missing");
    }

    let actions = get_actions(&args)?;
    let action = {
        let mut child = Command::new(&config.dmenu_cmd)
            .args(config.dmenu_args.split_whitespace())
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
        set_action(&args.wifi_interface, &action)?;
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
        .body(&format!("Connected to {ssid}"))
        .show()?;
    Ok(())
}

pub trait CommandRunner {
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error>;
}

pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
        Command::new(command).args(args).output()
    }
}

pub fn prompt_for_password(
    command_runner: &dyn CommandRunner,
    ssid: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let output = command_runner.run_command(
        "sh",
        &[
            "-c",
            &format!("echo 'SETDESC Enter '{ssid}' password\nGETPIN' | pinentry-gnome3"),
        ],
    )?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let password_line = stdout
        .lines()
        .find(|line| line.starts_with("D "))
        .ok_or("Password not found")?;
    let password = password_line.trim_start_matches("D ").trim().to_string();

    Ok(password)
}

fn disconnect_nm_wifi(interface: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let status = Command::new("nmcli")
        .arg("device")
        .arg("disconnect")
        .arg(interface)
        .status()?;
    Ok(status.success())
}
