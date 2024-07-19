use clap::Parser;
use dirs::config_dir;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use which::which;

mod iwd;
mod mullvad;
mod networkmanager;

use iwd::{connect_to_iwd_wifi, disconnect_iwd_wifi, get_iwd_networks, is_iwd_connected};
use mullvad::{get_mullvad_actions, is_exit_node_active, set_mullvad_exit_node};
use networkmanager::{
    connect_to_nm_wifi, disconnect_nm_wifi, get_nm_wifi_networks, is_nm_connected,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "wlan0")]
    wifi_interface: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    #[serde(default)]
    actions: Vec<CustomAction>,
    dmenu_cmd: String,
    dmenu_args: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CustomAction {
    display: String,
    cmd: String,
}

#[derive(Debug)]
enum ActionType {
    Custom(CustomAction),
    System(SystemAction),
    Mullvad(MullvadAction),
    Wifi(WifiAction),
}

#[derive(Debug)]
enum SystemAction {
    RfkillBlock,
    RfkillUnblock,
    EditConnections,
    DisconnectWifi,
    ConnectWifi,
    DisableExitNode,
}

#[derive(Debug)]
enum MullvadAction {
    EnableTailscale,
    DisableTailscale,
    SetExitNode(String),
    SetShieldsUp(bool),
}

#[derive(Debug)]
enum WifiAction {
    Network(String),
}

pub fn format_entry(action: &str, icon: &str, text: &str) -> String {
    format!("{action:<10}- {icon} {text}")
}

fn get_default_config() -> &'static str {
    r#"
dmenu_cmd = "dmenu"
dmenu_args = "--no-multi"

[[actions]]
display = "❌ Disable tailscale"
cmd = "tailscale down"

[[actions]]
display = "✅ Enable tailscale"
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

fn get_actions(args: &Args) -> Result<Vec<ActionType>, Box<dyn Error>> {
    let config = get_config()?;
    let mut actions = config
        .actions
        .into_iter()
        .map(ActionType::Custom)
        .collect::<Vec<_>>();

    if is_command_installed("rfkill") {
        actions.push(ActionType::System(SystemAction::RfkillBlock));
        actions.push(ActionType::System(SystemAction::RfkillUnblock));
    }

    if is_command_installed("nm-connection-editor") {
        actions.push(ActionType::System(SystemAction::EditConnections));
    }

    if is_command_installed("nmcli") {
        if is_nm_connected(&RealCommandRunner, &args.wifi_interface)? {
            actions.push(ActionType::System(SystemAction::DisconnectWifi));
        } else {
            actions.push(ActionType::System(SystemAction::ConnectWifi));
        }
    } else if is_command_installed("iwctl") && is_iwd_connected(&args.wifi_interface)? {
        actions.push(ActionType::System(SystemAction::DisconnectWifi));
    }

    if is_exit_node_active()? {
        actions.push(ActionType::System(SystemAction::DisableExitNode));
    }

    if is_command_installed("tailscale") {
        actions.extend(
            get_mullvad_actions()
                .into_iter()
                .map(|m| ActionType::Mullvad(MullvadAction::SetExitNode(m))),
        );
    }

    if is_command_installed("nmcli") {
        actions.extend(get_nm_wifi_networks()?.into_iter().map(ActionType::Wifi));
    } else if is_command_installed("iwctl") {
        actions.extend(
            get_iwd_networks(&args.wifi_interface)?
                .into_iter()
                .map(ActionType::Wifi),
        );
    }

    Ok(actions)
}

fn handle_custom_action(action: &CustomAction) -> Result<bool, Box<dyn Error>> {
    let status = Command::new("sh").arg("-c").arg(&action.cmd).status()?;

    Ok(status.success())
}

fn handle_system_action(
    action: &SystemAction,
    wifi_interface: &str,
) -> Result<bool, Box<dyn Error>> {
    match action {
        SystemAction::RfkillBlock => {
            let status = Command::new("rfkill").arg("block").arg("wlan").status()?;
            Ok(status.success())
        }
        SystemAction::RfkillUnblock => {
            let status = Command::new("rfkill").arg("unblock").arg("wlan").status()?;
            Ok(status.success())
        }
        SystemAction::EditConnections => {
            let status = Command::new("nm-connection-editor").status()?;
            Ok(status.success())
        }
        SystemAction::DisconnectWifi => {
            let status = if is_command_installed("nmcli") {
                disconnect_nm_wifi(wifi_interface)?
            } else {
                disconnect_iwd_wifi(wifi_interface)?
            };
            Ok(status)
        }
        SystemAction::ConnectWifi => {
            let status = Command::new("nmcli")
                .arg("device")
                .arg("connect")
                .arg(wifi_interface)
                .status()?;
            Ok(status.success())
        }
        SystemAction::DisableExitNode => {
            let status = Command::new("tailscale")
                .arg("set")
                .arg("--exit-node=")
                .status()?;
            Ok(status.success())
        }
    }
}

fn handle_mullvad_action(action: &MullvadAction) -> Result<bool, Box<dyn Error>> {
    match action {
        MullvadAction::EnableTailscale => {
            let status = Command::new("tailscale").arg("up").status()?;
            Ok(status.success())
        }
        MullvadAction::DisableTailscale => {
            let status = Command::new("tailscale").arg("down").status()?;
            Ok(status.success())
        }
        MullvadAction::SetExitNode(node) => {
            if set_mullvad_exit_node(node) {
                Ok(true)
            } else {
                Ok(false)
            }
        }
        MullvadAction::SetShieldsUp(enable) => {
            let status = Command::new("tailscale")
                .arg("set")
                .arg("--shields-up")
                .arg(if *enable { "true" } else { "false" })
                .status()?;
            Ok(status.success())
        }
    }
}

fn handle_wifi_action(action: &WifiAction, wifi_interface: &str) -> Result<bool, Box<dyn Error>> {
    match action {
        WifiAction::Network(network) => {
            if is_command_installed("nmcli") {
                connect_to_nm_wifi(network)?;
            } else if is_command_installed("iwctl") {
                connect_to_iwd_wifi(wifi_interface, network)?;
            }
            Ok(true)
        }
    }
}

fn set_action(wifi_interface: &str, action: ActionType) -> Result<bool, Box<dyn Error>> {
    match action {
        ActionType::Custom(custom_action) => handle_custom_action(&custom_action),
        ActionType::System(system_action) => handle_system_action(&system_action, wifi_interface),
        ActionType::Mullvad(mullvad_action) => handle_mullvad_action(&mullvad_action),
        ActionType::Wifi(wifi_action) => handle_wifi_action(&wifi_action, wifi_interface),
    }
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
            let actions_display = actions
                .iter()
                .map(|action| match action {
                    ActionType::Custom(custom_action) => {
                        format!("{:<10}- {}", "action", &custom_action.display)
                    }
                    ActionType::System(system_action) => match system_action {
                        SystemAction::RfkillBlock => {
                            format_entry("action", "📶", "Radio wifi rfkill block")
                        }
                        SystemAction::RfkillUnblock => {
                            format_entry("action", "📶", "Radio wifi rfkill unblock")
                        }
                        SystemAction::EditConnections => {
                            format_entry("action", "📶", "Edit connections")
                        }
                        SystemAction::DisconnectWifi => {
                            format_entry("action", "❌", "Disconnect wifi")
                        }
                        SystemAction::ConnectWifi => format_entry("action", "📶", "Connect wifi"),
                        SystemAction::DisableExitNode => {
                            format_entry("action", "❌", "Disable exit node")
                        }
                    },
                    ActionType::Mullvad(mullvad_action) => match mullvad_action {
                        MullvadAction::EnableTailscale => {
                            format_entry("mullvad", "✅", "Enable tailscale")
                        }
                        MullvadAction::DisableTailscale => {
                            format_entry("mullvad", "❌", "Disable tailscale")
                        }
                        MullvadAction::SetExitNode(node) => node.to_string(),
                        MullvadAction::SetShieldsUp(enable) => {
                            if *enable {
                                format_entry("mullvad", "🛡️", "Shields up")
                            } else {
                                format_entry("mullvad", "🛡️", "Shields down")
                            }
                        }
                    },
                    ActionType::Wifi(wifi_action) => match wifi_action {
                        WifiAction::Network(network) => format_entry("wifi", "", network),
                    },
                })
                .collect::<Vec<_>>()
                .join("\n");
            write!(stdin, "{}", actions_display)?;
        }

        let output = child.wait_with_output()?;
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    if !action.is_empty() {
        let selected_action = actions
            .into_iter()
            .find(|a| match a {
                ActionType::Custom(custom_action) => {
                    format!("{:<10}- {}", "action", &custom_action.display) == action
                }
                ActionType::System(system_action) => match system_action {
                    SystemAction::RfkillBlock => {
                        action == format_entry("action", "📶", "Radio wifi rfkill block")
                    }
                    SystemAction::RfkillUnblock => {
                        action == format_entry("action", "📶", "Radio wifi rfkill unblock")
                    }
                    SystemAction::EditConnections => {
                        action == format_entry("action", "📶", "Edit connections")
                    }
                    SystemAction::DisconnectWifi => {
                        action == format_entry("action", "❌", "Disconnect wifi")
                    }
                    SystemAction::ConnectWifi => {
                        action == format_entry("action", "📶", "Connect wifi")
                    }
                    SystemAction::DisableExitNode => {
                        action == format_entry("action", "❌", "Disable exit node")
                    }
                },
                ActionType::Mullvad(mullvad_action) => match mullvad_action {
                    MullvadAction::EnableTailscale => {
                        action == format_entry("mullvad", "✅", "Enable tailscale")
                    }
                    MullvadAction::DisableTailscale => {
                        action == format_entry("mullvad", "❌", "Disable tailscale")
                    }
                    MullvadAction::SetExitNode(node) => action == *node,
                    MullvadAction::SetShieldsUp(enable) => {
                        if *enable {
                            action == format_entry("mullvad", "🛡️", "Shields up")
                        } else {
                            action == format_entry("mullvad", "🛡️", "Shields down")
                        }
                    }
                },
                ActionType::Wifi(wifi_action) => match wifi_action {
                    WifiAction::Network(network) => action == format_entry("wifi", "", network),
                },
            })
            .ok_or("Selected action not found")?;

        set_action(&args.wifi_interface, selected_action)?;
    }

    #[cfg(debug_assertions)]
    if is_command_installed("tailscale") {
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
