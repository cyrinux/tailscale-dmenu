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
        let mut lines: Vec<String> = reader
            .lines()
            .map_while(Result::ok)
            .skip_while(|line| !line.contains("Available networks"))
            .skip(1)
            .take_while(|line| !line.contains("--------------------------------------------------------------------------------"))
            .collect();

        for line in lines.iter_mut() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let ssid = parts[0].trim();
                let bars = parts[2].trim();
                let display = format!("wifi - {} {}", ssid, bars);
                actions.push(display);
            }
        }
    }

    actions
}

pub fn connect_to_iwd_wifi(action: &str) -> bool {
    if action.starts_with("wifi - ") {
        let ssid = action.split_whitespace().nth(2).unwrap_or("");
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
