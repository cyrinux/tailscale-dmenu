use crate::command::CommandRunner;
use clap::Parser;
use dirs::config_dir;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

mod bluetooth;
mod command;
mod iwd;
mod networkmanager;
mod tailscale;
mod utils;

use bluetooth::{
    get_connected_devices, get_paired_bluetooth_devices, handle_bluetooth_action, BluetoothAction,
};
use command::{is_command_installed, RealCommandRunner};
use iwd::{connect_to_iwd_wifi, disconnect_iwd_wifi, get_iwd_networks, is_iwd_connected};
use networkmanager::{
    connect_to_nm_wifi, disconnect_nm_wifi, get_nm_wifi_networks, is_nm_connected,
};
use tailscale::{
    check_mullvad, get_mullvad_actions, handle_tailscale_action, is_exit_node_active,
    is_tailscale_enabled, TailscaleAction,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "wlan0")]
    wifi_interface: String,
    #[arg(long)]
    no_wifi: bool,
    #[arg(long)]
    no_bluetooth: bool,
    #[arg(long)]
    no_tailscale: bool,
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
    Bluetooth(BluetoothAction),
    Custom(CustomAction),
    System(SystemAction),
    Tailscale(TailscaleAction),
    Wifi(WifiAction),
}

#[derive(Debug)]
enum SystemAction {
    EditConnections,
    RfkillBlock,
    RfkillUnblock,
}

#[derive(Debug)]
enum WifiAction {
    Connect,
    Disconnect,
    Network(String),
}

pub fn format_entry(action: &str, icon: &str, text: &str) -> String {
    if icon.is_empty() {
        format!("{action:<10}- {text}")
    } else {
        format!("{action:<10}- {icon} {text}")
    }
}

fn get_default_config() -> &'static str {
    r#"
dmenu_cmd = "dmenu"
dmenu_args = "--no-multi"

[[actions]]
display = "🛡️ Example"
cmd = "notify-send 'hello' 'world'"
"#
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    create_default_config_if_missing()?;

    let config = get_config()?;

    if !is_command_installed("pinentry-gnome3") || !is_command_installed(&config.dmenu_cmd) {
        panic!("pinentry-gnome3 or dmenu command missing");
    }

    let command_runner = RealCommandRunner;
    let actions = get_actions(&args, &command_runner)?;
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
                        format_entry("action", "", &custom_action.display)
                    }
                    ActionType::System(system_action) => match system_action {
                        SystemAction::RfkillBlock => {
                            format_entry("system", "❌", "Radio wifi rfkill block")
                        }
                        SystemAction::RfkillUnblock => {
                            format_entry("system", "📶", "Radio wifi rfkill unblock")
                        }
                        SystemAction::EditConnections => {
                            format_entry("system", "📶", "Edit connections")
                        }
                    },
                    ActionType::Tailscale(mullvad_action) => match mullvad_action {
                        TailscaleAction::SetExitNode(node) => node.to_string(),
                        TailscaleAction::DisableExitNode => {
                            format_entry("tailscale", "❌", "Disable exit-node")
                        }
                        TailscaleAction::SetEnable(enable) => {
                            if *enable {
                                format_entry("tailscale", "✅", "Enable tailscale")
                            } else {
                                format_entry("tailscale", "❌", "Disable tailscale")
                            }
                        }
                        TailscaleAction::SetShields(enable) => {
                            if *enable {
                                format_entry("tailscale", "🛡️", "Shields up")
                            } else {
                                format_entry("tailscale", "🛡️", "Shields down")
                            }
                        }
                    },
                    ActionType::Wifi(wifi_action) => match wifi_action {
                        WifiAction::Network(network) => {
                            format_entry(&args.wifi_interface.to_string(), "", network)
                        }
                        WifiAction::Disconnect => {
                            format_entry(&args.wifi_interface.to_string(), "❌", "Disconnect")
                        }
                        WifiAction::Connect => {
                            format_entry(&args.wifi_interface.to_string(), "📶", "Connect")
                        }
                    },
                    ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
                        BluetoothAction::ToggleConnect(device) => device.to_string(),
                    },
                })
                .collect::<Vec<_>>()
                .join("\n");
            write!(stdin, "{actions_display}")?;
        }

        let output = child.wait_with_output()?;
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    if !action.is_empty() {
        let selected_action = actions
            .into_iter()
            .find(|a| match a {
                ActionType::Custom(custom_action) => {
                    format_entry("action", "", &custom_action.display) == action
                }
                ActionType::System(system_action) => match system_action {
                    SystemAction::RfkillBlock => {
                        action == format_entry("system", "❌", "Radio wifi rfkill block")
                    }
                    SystemAction::RfkillUnblock => {
                        action == format_entry("system", "📶", "Radio wifi rfkill unblock")
                    }
                    SystemAction::EditConnections => {
                        action == format_entry("system", "📶", "Edit connections")
                    }
                },
                ActionType::Tailscale(mullvad_action) => match mullvad_action {
                    TailscaleAction::SetExitNode(node) => action == *node,
                    TailscaleAction::DisableExitNode => {
                        action == format_entry("tailscale", "❌", "Disable exit-node")
                    }
                    TailscaleAction::SetEnable(enable) => {
                        if *enable {
                            action == format_entry("tailscale", "✅", "Enable tailscale")
                        } else {
                            action == format_entry("tailscale", "❌", "Disable tailscale")
                        }
                    }
                    TailscaleAction::SetShields(enable) => {
                        if *enable {
                            action == format_entry("tailscale", "🛡️", "Shields up")
                        } else {
                            action == format_entry("tailscale", "🛡️", "Shields down")
                        }
                    }
                },
                ActionType::Wifi(wifi_action) => match wifi_action {
                    WifiAction::Network(network) => {
                        action == format_entry(&args.wifi_interface.to_string(), "", network)
                    }
                    WifiAction::Disconnect => {
                        action == format_entry(&args.wifi_interface.to_string(), "❌", "Disconnect")
                    }
                    WifiAction::Connect => {
                        action == format_entry(&args.wifi_interface.to_string(), "📶", "Connect")
                    }
                },
                ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
                    BluetoothAction::ToggleConnect(device) => &action == device,
                },
            })
            .ok_or("Selected action not found")?;

        let connected_devices = get_connected_devices(&command_runner)?;

        set_action(
            &args.wifi_interface,
            selected_action,
            &connected_devices,
            &command_runner,
        )?;
    }

    #[cfg(debug_assertions)]
    if is_command_installed("tailscale") {
        Command::new("tailscale").arg("status").status()?;
    }

    Ok(())
}

fn get_config_path() -> Result<PathBuf, Box<dyn Error>> {
    let config_dir = config_dir().ok_or("Failed to find config directory")?;
    Ok(config_dir.join("network-dmenu").join("config.toml"))
}

fn create_default_config_if_missing() -> Result<(), Box<dyn Error>> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&config_path, get_default_config())?;
    }
    Ok(())
}

fn get_config() -> Result<Config, Box<dyn Error>> {
    let config_path = get_config_path()?;
    let config_content = fs::read_to_string(config_path)?;
    let config = toml::from_str(&config_content)?;
    Ok(config)
}

fn get_actions(
    args: &Args,
    command_runner: &dyn CommandRunner,
) -> Result<Vec<ActionType>, Box<dyn Error>> {
    let config = get_config()?;
    let mut actions = config
        .actions
        .into_iter()
        .map(ActionType::Custom)
        .collect::<Vec<_>>();

    if !args.no_tailscale
        && is_command_installed("tailscale")
        && is_exit_node_active(command_runner)?
    {
        actions.push(ActionType::Tailscale(TailscaleAction::DisableExitNode));
    }

    if !args.no_wifi && is_command_installed("nmcli") {
        actions.extend(
            get_nm_wifi_networks(command_runner)?
                .into_iter()
                .map(ActionType::Wifi),
        );
    } else if !args.no_wifi && is_command_installed("iwctl") {
        actions.extend(
            get_iwd_networks(&args.wifi_interface, command_runner)?
                .into_iter()
                .map(ActionType::Wifi),
        );
    }

    if !args.no_wifi && is_command_installed("nmcli") {
        if is_nm_connected(command_runner, &args.wifi_interface)? {
            actions.push(ActionType::Wifi(WifiAction::Disconnect));
        } else {
            actions.push(ActionType::Wifi(WifiAction::Connect));
        }
    } else if !args.no_wifi && is_command_installed("iwctl") {
        if is_iwd_connected(command_runner, &args.wifi_interface)? {
            actions.push(ActionType::Wifi(WifiAction::Disconnect));
        } else {
            actions.push(ActionType::Wifi(WifiAction::Connect));
        }
    }

    if !args.no_wifi && is_command_installed("rfkill") {
        actions.push(ActionType::System(SystemAction::RfkillBlock));
        actions.push(ActionType::System(SystemAction::RfkillUnblock));
    }

    if !args.no_wifi && is_command_installed("nm-connection-editor") {
        actions.push(ActionType::System(SystemAction::EditConnections));
    }

    if !args.no_tailscale && is_command_installed("tailscale") {
        actions.push(ActionType::Tailscale(TailscaleAction::SetEnable(
            !is_tailscale_enabled(command_runner)?,
        )));
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(false)));
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(true)));
        actions.extend(
            get_mullvad_actions(command_runner)
                .into_iter()
                .map(|m| ActionType::Tailscale(TailscaleAction::SetExitNode(m))),
        );
    }

    if !args.no_bluetooth && is_command_installed("bluetoothctl") {
        actions.extend(
            get_paired_bluetooth_devices(command_runner)?
                .into_iter()
                .map(ActionType::Bluetooth),
        );
    }

    Ok(actions)
}

fn handle_custom_action(action: &CustomAction) -> Result<bool, Box<dyn Error>> {
    let status = Command::new("sh").arg("-c").arg(&action.cmd).status()?;
    Ok(status.success())
}

fn handle_system_action(action: &SystemAction) -> Result<bool, Box<dyn Error>> {
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
    }
}

fn parse_wifi_action(action: &str) -> Result<(&str, &str), Box<dyn Error>> {
    let emoji_pos = action
        .char_indices()
        .find(|(_, c)| *c == '✅' || *c == '📶')
        .map(|(i, _)| i)
        .ok_or("Emoji not found in action")?;
    let tab_pos = action[emoji_pos..]
        .char_indices()
        .find(|(_, c)| *c == '\t')
        .map(|(i, _)| i + emoji_pos)
        .ok_or("Tab character not found in action")?;
    let ssid = action[emoji_pos + 4..tab_pos].trim();
    let parts: Vec<&str> = action[tab_pos + 1..].split('\t').collect();
    if parts.len() < 2 {
        return Err("Action format is incorrect".into());
    }
    let security = parts[0].trim();
    Ok((ssid, security))
}

fn handle_wifi_action(
    action: &WifiAction,
    wifi_interface: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    match action {
        WifiAction::Disconnect => {
            let status = if is_command_installed("nmcli") {
                disconnect_nm_wifi(wifi_interface, command_runner)?
            } else {
                disconnect_iwd_wifi(wifi_interface, command_runner)?
            };
            Ok(status)
        }
        WifiAction::Connect => {
            let status = Command::new("nmcli")
                .arg("device")
                .arg("connect")
                .arg(wifi_interface)
                .status()?;
            check_mullvad()?;
            Ok(status.success())
        }
        WifiAction::Network(network) => {
            if is_command_installed("nmcli") {
                connect_to_nm_wifi(network, command_runner)?;
            } else if is_command_installed("iwctl") {
                connect_to_iwd_wifi(wifi_interface, network, command_runner)?;
            }
            check_mullvad()?;
            Ok(true)
        }
    }
}

fn set_action(
    wifi_interface: &str,
    action: ActionType,
    connected_devices: &[String],
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    match action {
        ActionType::Custom(custom_action) => handle_custom_action(&custom_action),
        ActionType::System(system_action) => handle_system_action(&system_action),
        ActionType::Tailscale(mullvad_action) => {
            handle_tailscale_action(&mullvad_action, command_runner)
        }
        ActionType::Wifi(wifi_action) => {
            handle_wifi_action(&wifi_action, wifi_interface, command_runner)
        }
        ActionType::Bluetooth(bluetooth_action) => {
            handle_bluetooth_action(&bluetooth_action, connected_devices, command_runner)
        }
    }
}

fn notify_connection(ssid: &str) -> Result<(), Box<dyn Error>> {
    Notification::new()
        .summary("Wi-Fi")
        .body(&format!("Connected to {ssid}"))
        .show()?;
    Ok(())
}
