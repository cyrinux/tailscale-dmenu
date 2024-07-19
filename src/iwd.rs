use crate::RealCommandRunner;
use crate::{notify_connection, prompt_for_password};
use regex::Regex;
use std::io::{BufRead, BufReader};
use std::process::Command;

pub fn get_iwd_networks(interface: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
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
    actions: &mut Vec<String>,
    networks: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Regex to remove ANSI color codes
    let ansi_escape = Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]")?;

    for network in networks {
        let line = ansi_escape.replace_all(&network, "").to_string();
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let connected = parts[0] == ">";
            let ssid_start = if connected { 1 } else { 0 };
            let ssid = parts[ssid_start..parts.len() - 2].join(" ");
            let signal = parts[parts.len() - 1].trim();
            let display = format!(
                "{:<8}- {} {} - {}",
                "wifi",
                if connected { "🌐" } else { "📶" },
                ssid,
                signal
            );
            actions.push(display);
        }
    }

    Ok(())
}

pub fn connect_to_iwd_wifi(
    interface: &str,
    action: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    if action.starts_with("wifi") {
        let ssid = action.split_whitespace().nth(3).unwrap_or("");
        if attempt_connection(interface, ssid, None)? {
            Ok(true)
        } else {
            // If the first attempt fails, prompt for a passphrase using dmenu and retry
            let passphrase = prompt_for_password(&RealCommandRunner, ssid)?;
            attempt_connection(interface, ssid, Some(passphrase))
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
    let command = match passphrase {
        Some(ref pwd) => format!("--passphrase '{pwd}' station {interface} connect '{ssid}'"),
        None => format!("station {interface} connect '{ssid}'"),
    };

    let status = Command::new("iwctl").args(command.split(' ')).status()?;

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
