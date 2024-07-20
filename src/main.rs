use clap::Parser;
use dirs::config_dir;
use notify_rust::Notification;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use which::which;

mod bluetooth;
mod iwd;
mod mullvad;
mod networkmanager;

use bluetooth::{
    connect_to_bluetooth_device, disconnect_bluetooth_device, get_paired_bluetooth_devices,
    BluetoothAction,
};
use iwd::{connect_to_iwd_wifi, disconnect_iwd_wifi, get_iwd_networks, is_iwd_connected};
use mullvad::{check_mullvad, get_mullvad_actions, is_exit_node_active, set_exit_node};
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
enum TailscaleAction {
    DisableExitNode,
    SetEnable(bool),
    SetExitNode(String),
    SetShields(bool),
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

    if is_command_installed("tailscale") && is_exit_node_active()? {
        actions.push(ActionType::Tailscale(TailscaleAction::DisableExitNode));
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

    if is_command_installed("nmcli") {
        if is_nm_connected(&RealCommandRunner, &args.wifi_interface)? {
            actions.push(ActionType::Wifi(WifiAction::Disconnect));
        } else {
            actions.push(ActionType::Wifi(WifiAction::Connect));
        }
    } else if is_command_installed("iwctl") && is_iwd_connected(&args.wifi_interface)? {
        actions.push(ActionType::Wifi(WifiAction::Disconnect));
    }

    if is_command_installed("rfkill") {
        actions.push(ActionType::System(SystemAction::RfkillBlock));
        actions.push(ActionType::System(SystemAction::RfkillUnblock));
    }

    if is_command_installed("nm-connection-editor") {
        actions.push(ActionType::System(SystemAction::EditConnections));
    }

    if is_command_installed("tailscale") {
        actions.push(ActionType::Tailscale(TailscaleAction::SetEnable(
            !is_tailscale_enabled()?,
        )));
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(false)));
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(true)));
        actions.extend(
            get_mullvad_actions()
                .into_iter()
                .map(|m| ActionType::Tailscale(TailscaleAction::SetExitNode(m))),
        );
    }

    if is_command_installed("bluetoothctl") {
        actions.extend(
            get_paired_bluetooth_devices()?
                .into_iter()
                .map(ActionType::Bluetooth),
        );
        actions.push(ActionType::Bluetooth(BluetoothAction::Disconnect));
    }

    Ok(actions)
}

fn handle_bluetooth_action(action: &BluetoothAction) -> Result<bool, Box<dyn Error>> {
    match action {
        BluetoothAction::Connect(device) => connect_to_bluetooth_device(device),
        BluetoothAction::Disconnect => disconnect_bluetooth_device(),
    }
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

fn handle_tailscale_action(action: &TailscaleAction) -> Result<bool, Box<dyn Error>> {
    if !is_command_installed("tailscale") {
        return Ok(false);
    }

    match action {
        TailscaleAction::DisableExitNode => {
            let status = Command::new("tailscale")
                .arg("set")
                .arg("--exit-node=")
                .status()?;
            check_mullvad()?;
            Ok(status.success())
        }
        TailscaleAction::SetEnable(enable) => {
            let status = Command::new("tailscale")
                .arg(if *enable { "up" } else { "down" })
                .status()?;
            Ok(status.success())
        }
        TailscaleAction::SetExitNode(node) => {
            if set_exit_node(node) {
                check_mullvad()?;
                Ok(true)
            } else {
                check_mullvad()?;
                Ok(false)
            }
        }
        TailscaleAction::SetShields(enable) => {
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
        WifiAction::Disconnect => {
            let status = if is_command_installed("nmcli") {
                disconnect_nm_wifi(wifi_interface)?
            } else {
                disconnect_iwd_wifi(wifi_interface)?
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
                connect_to_nm_wifi(network)?;
            } else if is_command_installed("iwctl") {
                connect_to_iwd_wifi(wifi_interface, network)?;
            }
            check_mullvad()?;
            Ok(true)
        }
    }
}

fn set_action(wifi_interface: &str, action: ActionType) -> Result<bool, Box<dyn Error>> {
    match action {
        ActionType::Custom(custom_action) => handle_custom_action(&custom_action),
        ActionType::System(system_action) => handle_system_action(&system_action),
        ActionType::Tailscale(mullvad_action) => handle_tailscale_action(&mullvad_action),
        ActionType::Wifi(wifi_action) => handle_wifi_action(&wifi_action, wifi_interface),
        ActionType::Bluetooth(bluetooth_action) => handle_bluetooth_action(&bluetooth_action),
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
                            format_entry("tailscale", "❌", "Disable exit node")
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
                        WifiAction::Network(network) => format_entry("wifi", "", network),
                        WifiAction::Disconnect => format_entry("wifi", "❌", "Disconnect"),
                        WifiAction::Connect => format_entry("wifi", "📶", "Connect"),
                    },
                    ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
                        BluetoothAction::Connect(device) => format_entry("bluetooth", "", device),
                        BluetoothAction::Disconnect => {
                            format_entry("bluetooth", "❌", "Disconnect")
                        }
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
                        action == format_entry("tailscale", "❌", "Disable exit node")
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
                    WifiAction::Network(network) => action == format_entry("wifi", "", network),
                    WifiAction::Disconnect => action == format_entry("wifi", "❌", "Disconnect"),
                    WifiAction::Connect => action == format_entry("wifi", "📶", "Connect"),
                },
                ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
                    BluetoothAction::Connect(device) => {
                        action == format_entry("bluetooth", "", device)
                    }
                    BluetoothAction::Disconnect => {
                        action == format_entry("bluetooth", "❌", "Disconnect")
                    }
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

pub fn is_known_network(ssid: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let output = Command::new("iwctl")
        .arg("known-networks")
        .arg("list")
        .output()?;

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let ssid_pattern = format!(r"\b{}\b", regex::escape(ssid));
        let re = Regex::new(&ssid_pattern)?;

        for line in reader.lines() {
            let line = line?;
            if re.is_match(&line) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn is_tailscale_enabled() -> Result<bool, Box<dyn std::error::Error>> {
    let output = Command::new("tailscale").arg("status").output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(!stdout.contains("Tailscale is stopped"));
    }
    Ok(false)
}

pub fn convert_network_strength(line: &str) -> String {
    // Define the mapping for network strength symbols
    let strength_symbols = ["_", "▂", "▄", "▆", "█"];

    // Extract the stars from the end of the line
    let stars = line.chars().rev().take_while(|&c| c == '*').count();

    // Create the network manager style representation
    let network_strength = format!(
        "{}{}{}{}",
        strength_symbols.get(1).unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 2 { 2 } else { 0 })
            .unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 3 { 3 } else { 0 })
            .unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 4 { 4 } else { 0 })
            .unwrap_or(&"_"),
    );

    network_strength
}
