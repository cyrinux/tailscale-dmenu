use crate::notify_connection;
use regex::Regex;
use std::io::{BufRead, BufReader};
use std::process::Command;

pub fn get_iwd_networks() -> Vec<String> {
    let mut actions = Vec::new();

    if let Some(networks) = fetch_iwd_networks() {
        let mut has_connected = false;
        for network in &networks {
            if network.starts_with(">") {
                has_connected = true;
                break;
            }
        }

        if !has_connected {
            // Rescan networks
            let rescan_output = Command::new("iwctl")
                .arg("station")
                .arg("wlan0")
                .arg("scan")
                .output()
                .expect("Failed to execute rescan command");

            if rescan_output.status.success() {
                if let Some(rescan_networks) = fetch_iwd_networks() {
                    parse_iwd_networks(&mut actions, rescan_networks);
                }
            }
        } else {
            parse_iwd_networks(&mut actions, networks);
        }
    }

    actions
}

fn fetch_iwd_networks() -> Option<Vec<String>> {
    let output = Command::new("iwctl")
        .arg("station")
        .arg("wlan0")
        .arg("get-networks")
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let networks = reader
            .lines()
            .map_while(Result::ok)
            .skip_while(|network| !network.contains("Available networks"))
            .skip(3)
            .collect();
        Some(networks)
    } else {
        None
    }
}

fn parse_iwd_networks(actions: &mut Vec<String>, networks: Vec<String>) {
    // Regex to remove ANSI color codes
    let ansi_escape = Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]").unwrap();

    for network in networks {
        let line = ansi_escape.replace_all(&network, "").to_string();
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let connected = parts[0] == ">";
            let ssid_start = if connected { 1 } else { 0 };
            let ssid = parts[ssid_start..parts.len() - 2].join(" ");
            let signal = parts[parts.len() - 1].trim();
            let display = format!(
                "wifi - {} {} - {}",
                if connected { "🌐" } else { "📶" },
                ssid,
                signal
            );
            actions.push(display);
        }
    }
}

pub fn connect_to_iwd_wifi(action: &str) -> bool {
    println!("connect test iwd");
    if action.starts_with("wifi - ") {
        let ssid = action.split_whitespace().nth(3).unwrap_or("");
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("iwctl station wlan0 connect '{ssid}'"))
            .status();

        match status {
            Ok(status) => {
                if !status.success() {
                    eprintln!("Failed to connect to Wi-Fi network: {ssid}");
                    false
                } else {
                    notify_connection(ssid);

                    true
                }
            }
            Err(err) => {
                eprintln!("Failed to execute Wi-Fi connection command: {err:?}");
                false
            }
        }
    } else {
        false
    }
}
