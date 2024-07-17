use std::io::{BufRead, BufReader};
use std::process::Command;

pub fn get_wifi_networks() -> Vec<String> {
    let mut actions = Vec::new();

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
        let wifi_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

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

    actions
}

pub fn connect_to_wifi(action: &str) -> bool {
    if action.starts_with("wifi - ") {
        let ssid = action.split_whitespace().nth(2).unwrap_or("");
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("nmcli connection up '{}'", ssid))
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
