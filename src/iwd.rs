use std::io::{BufRead, BufReader};
use std::process::Command;

use crate::RealCommandRunner;
use crate::{convert_network_strength, is_known_network, notify_connection, prompt_for_password};
use regex::Regex;

use crate::WifiAction;

pub fn get_iwd_networks(interface: &str) -> Result<Vec<WifiAction>, Box<dyn std::error::Error>> {
    let mut actions = Vec::new();

    if let Some(networks) = fetch_iwd_networks(interface)? {
        let has_connected = networks.iter().any(|network| network.starts_with('>'));

        if !has_connected {
            // Rescan networks
            let rescan_output = Command::new("iwctl")
                .arg("station")
                .arg(interface)
                .arg("scan")
                .output()?;

            if rescan_output.status.success() {
                if let Some(rescan_networks) = fetch_iwd_networks(interface)? {
                    let _ = parse_iwd_networks(&mut actions, rescan_networks);
                }
            }
        } else {
            let _ = parse_iwd_networks(&mut actions, networks);
        }
    }

    Ok(actions)
}

fn fetch_iwd_networks(interface: &str) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    let output = Command::new("iwctl")
        .arg("station")
        .arg(interface)
        .arg("get-networks")
        .output()?;

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let networks = reader
            .lines()
            .map_while(Result::ok)
            .skip_while(|network| !network.contains("Available networks"))
            .skip(3)
            .collect();
        Ok(Some(networks))
    } else {
        Ok(None)
    }
}

fn parse_iwd_networks(
    actions: &mut Vec<WifiAction>,
    networks: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Regex to remove ANSI color codes
    let ansi_escape = Regex::new(r"\x1B\[[0-9;]*m.*?\x1B\[0m")?;

    for network in networks {
        let line = ansi_escape.replace_all(&network, "").to_string();
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let connected = parts[0] == ">";
            let ssid_start = if connected { 1 } else { 0 };
            let ssid = parts[ssid_start..parts.len() - 2].join(" ");
            let signal = parts[parts.len() - 1].trim();
            let display = format!(
                "{} {} {}",
                if connected { "🌐" } else { "📶" },
                ssid,
                convert_network_strength(signal)
            );
            actions.push(WifiAction::Network(display));
        }
    }

    Ok(())
}

pub fn connect_to_iwd_wifi(
    interface: &str,
    action: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    if let Some(ssid) = action.split_whitespace().nth(3) {
        if !is_known_network(ssid)? {
            let passphrase = prompt_for_password(&RealCommandRunner, ssid)?;
            attempt_connection(interface, ssid, Some(passphrase))
        } else {
            attempt_connection(interface, ssid, None)
        }
    } else {
        Ok(false)
    }
}

fn attempt_connection(
    interface: &str,
    ssid: &str,
    passphrase: Option<String>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut command_args = vec![
        "station".to_string(),
        interface.to_string(),
        "connect".to_string(),
        ssid.to_string(),
    ];
    if let Some(pwd) = passphrase {
        command_args.push("--passphrase".to_string());
        command_args.push(pwd);
    }

    let status = Command::new("iwctl").args(&command_args).status()?;

    if status.success() {
        notify_connection(ssid)?;
        Ok(true)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Failed to connect to Wi-Fi network: {ssid}");
        Ok(false)
    }
}

pub fn disconnect_iwd_wifi(interface: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let status = Command::new("iwctl")
        .arg("station")
        .arg(interface)
        .arg("disconnect")
        .status()?;
    Ok(status.success())
}

pub fn is_iwd_connected(interface: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let output = Command::new("iwctl")
        .arg("station")
        .arg(interface)
        .arg("show")
        .output()?;

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        for line in reader.lines() {
            let line = line?;
            if line.contains("Connected") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
