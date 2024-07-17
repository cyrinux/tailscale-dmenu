use crate::notify_connection;
use std::io::{BufRead, BufReader};
use std::process::Command;

pub fn get_wifi_networks() -> Vec<String> {
    let mut actions = Vec::new();

    let wifi_lines = fetch_wifi_lines();
    if let Some(lines) = wifi_lines {
        let mut has_in_use = false;
        for line in &lines {
            if line.starts_with("*") {
                has_in_use = true;
                break;
            }
        }

        if !has_in_use {
            let rescan_output = Command::new("nmcli")
                .arg("dev")
                .arg("wifi")
                .arg("list")
                .arg("--rescan")
                .arg("auto")
                .output()
                .expect("Failed to execute rescan command");

            if rescan_output.status.success() {
                if let Some(rescan_lines) = fetch_wifi_lines() {
                    parse_wifi_lines(&mut actions, rescan_lines);
                }
            }
        } else {
            parse_wifi_lines(&mut actions, lines);
        }
    }

    actions
}

fn fetch_wifi_lines() -> Option<Vec<String>> {
    let output = Command::new("nmcli")
        .arg("-t")
        .arg("-f")
        .arg("IN-USE,SSID,BARS")
        .arg("device")
        .arg("wifi")
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        Some(reader.lines().map_while(Result::ok).collect())
    } else {
        None
    }
}

fn parse_wifi_lines(actions: &mut Vec<String>, wifi_lines: Vec<String>) {
    for line in wifi_lines {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 3 {
            let in_use = parts[0].trim();
            let ssid = parts[1].trim();
            let bars = parts[2].trim();
            if !ssid.is_empty() {
                let display = format!(
                    "wifi - {} {} - {}",
                    if in_use == "*" { "🌐" } else { "📶" },
                    ssid,
                    bars
                );
                actions.push(display);
            }
        }
    }
}

pub fn connect_to_wifi(action: &str) -> bool {
    if action.starts_with("wifi - ") {
        let ssid = action.split_whitespace().nth(3).unwrap_or("");
        if attempt_connection(ssid, None) {
            true
        } else {
            // If the first attempt fails, prompt for a password using dmenu and retry
            let password = prompt_for_password();
            attempt_connection(ssid, Some(password))
        }
    } else {
        false
    }
}

fn attempt_connection(ssid: &str, password: Option<String>) -> bool {
    let command = match password {
        Some(ref pwd) => format!("nmcli device wifi connect '{ssid}' password '{pwd}'"),
        None => format!("nmcli connection up '{ssid}'"),
    };

    let status = Command::new("sh").arg("-c").arg(&command).status();

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
}

fn prompt_for_password() -> String {
    let output = Command::new("dmenu")
        .arg("-p")
        .arg("Enter Wi-Fi password:")
        .output()
        .expect("Failed to execute dmenu");

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
