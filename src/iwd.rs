use regex::Regex;
use std::io::{BufRead, BufReader};
use std::process::Command;

pub fn get_iwd_networks() -> Vec<String> {
    let mut actions = Vec::new();

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
            .skip(3);

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

    actions
}

pub fn connect_to_iwd_wifi(action: &str) -> bool {
    if action.starts_with("wifi - ") {
        let ssid = action.split_whitespace().nth(3).unwrap_or("");
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("iwctl station wlan0 connect '{}'", ssid))
            .status();

        match status {
            Ok(status) => {
                if !status.success() {
                    eprintln!("Failed to connect to Wi-Fi network: {}", ssid);
                    false
                } else {
                    let notification = format!("notify-send 'Wi-Fi' 'Connected to {}'", ssid);

                    Command::new("sh")
                        .arg("-c")
                        .arg(notification)
                        .status()
                        .expect("Failed to send notification");

                    true
                }
            }
            Err(err) => {
                eprintln!("Failed to execute Wi-Fi connection command: {:?}", err);
                false
            }
        }
    } else {
        false
    }
}
