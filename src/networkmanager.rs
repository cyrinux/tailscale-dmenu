use regex::Regex;

use crate::command::{read_output_lines, CommandRunner};
use crate::utils::{convert_network_strength, prompt_for_password};
use crate::{notify_connection, WifiAction};
use std::error::Error;
use std::io::{BufRead, BufReader};

pub fn get_nm_wifi_networks(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<WifiAction>, Box<dyn Error>> {
    let mut actions = Vec::new();

    if let Some(lines) = fetch_wifi_lines(command_runner)? {
        let has_in_use = lines.iter().any(|line| line.starts_with('*'));

        if !has_in_use {
            let rescan_output = command_runner.run_command(
                "nmcli",
                &["--colors", "no", "dev", "wifi", "list", "--rescan", "auto"],
            )?;

            if rescan_output.status.success() {
                if let Some(rescan_lines) = fetch_wifi_lines(command_runner)? {
                    parse_wifi_lines(&mut actions, rescan_lines);
                }
            }
        } else {
            parse_wifi_lines(&mut actions, lines);
        }
    }

    Ok(actions)
}

fn fetch_wifi_lines(
    command_runner: &dyn CommandRunner,
) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    let output = command_runner.run_command(
        "nmcli",
        &[
            "--colors",
            "no",
            "-t",
            "-f",
            "IN-USE,SSID,BARS,SECURITY",
            "device",
            "wifi",
        ],
    )?;

    if output.status.success() {
        let reader = read_output_lines(&output)?;
        Ok(Some(reader))
    } else {
        Ok(None)
    }
}

fn parse_wifi_lines(actions: &mut Vec<WifiAction>, wifi_lines: Vec<String>) {
    wifi_lines.into_iter().for_each(|line| {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 4 {
            let in_use = parts[0].trim();
            let ssid = parts[1].trim();
            let signal = parts[2].trim();
            let security = parts[3].trim();
            if !ssid.is_empty() {
                let display = format!(
                    "{} {:<25}\t{:<11}\t{}",
                    if in_use == "*" { "✅" } else { "📶" },
                    ssid,
                    security.to_uppercase(),
                    convert_network_strength(signal),
                );
                actions.push(WifiAction::Network(display));
            }
        }
    });
}

pub fn connect_to_nm_wifi(
    action: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    // Find the position of the first emoji character (either ✅ or 📶)
    let emoji_pos = action
        .char_indices()
        .find(|(_, c)| *c == '✅' || *c == '📶')
        .map(|(i, _)| i)
        .ok_or("Emoji not found in action")?;

    // Find the position of the first tab character after the emoji
    let tab_pos = action[emoji_pos..]
        .char_indices()
        .find(|(_, c)| *c == '\t')
        .map(|(i, _)| i + emoji_pos)
        .ok_or("Tab character not found in action")?;

    // Extract the SSID between the emoji and the tab
    let ssid = action[emoji_pos + 4..tab_pos].trim(); // 4 bytes for the emoji

    // Split the rest of the action to extract security information
    let parts: Vec<&str> = action[tab_pos + 1..].split('\t').collect();
    if parts.len() < 2 {
        return Err("Action format is incorrect".into());
    }

    let security = parts[0].trim();

    #[cfg(debug_assertions)]
    println!("Connecting to Wi-Fi network: {ssid} with security {security}");

    if is_known_network(ssid, command_runner)? || security.is_empty() {
        attempt_connection(ssid, None, command_runner)
    } else {
        let password = prompt_for_password(ssid)?;
        attempt_connection(ssid, Some(password), command_runner)
    }
}

fn attempt_connection(
    ssid: &str,
    password: Option<String>,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let command = match password {
        Some(ref pwd) => vec!["device", "wifi", "connect", ssid, "password", pwd],
        None => vec!["device", "wifi", "connect", ssid],
    };

    let status = command_runner.run_command("nmcli", &command)?.status;

    if status.success() {
        notify_connection(ssid)?;
        Ok(true)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Failed to connect to Wi-Fi network: {ssid}");
        Ok(false)
    }
}

pub fn disconnect_nm_wifi(
    interface: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let status = command_runner
        .run_command("nmcli", &["device", "disconnect", interface])?
        .status;
    Ok(status.success())
}

pub fn is_nm_connected(
    command_runner: &dyn CommandRunner,
    interface: &str,
) -> Result<bool, Box<dyn Error>> {
    let output = command_runner.run_command(
        "nmcli",
        &[
            "--colors",
            "no",
            "-t",
            "-f",
            "DEVICE,STATE",
            "device",
            "status",
        ],
    )?;
    let reader = read_output_lines(&output)?;
    for line in reader {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 2 && parts[0].trim() == interface && parts[1].trim() == "connected" {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn is_known_network(
    ssid: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    // Run the `nmcli connection show` command
    let output = command_runner.run_command("nmcli", &["--colors", "no", "connection", "show"])?;

    // Check if the command executed successfully
    if output.status.success() {
        // Create a buffered reader for the command output
        let reader = BufReader::new(output.stdout.as_slice());

        // Create a regex pattern to match the SSID exactly
        let ssid_pattern = format!(r"^\s*{}\s+", regex::escape(ssid));
        let re = Regex::new(&ssid_pattern)?;

        // Iterate over each line in the output
        for line in reader.lines() {
            let line = line?;

            // Check if the line matches the SSID pattern
            if re.is_match(&line) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::MockCommandRunner;
    use std::collections::HashMap;
    use std::process::Output;

    struct MockCommandRunner {
        outputs: HashMap<String, Output>,
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
            let key = format!("{} {:?}", command, args);
            self.outputs.get(&key).cloned().ok_or(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Command not found",
            ))
        }
    }

    #[test]
    fn test_parse_wifi_lines() {
        let mut actions = Vec::new();
        let wifi_lines = vec![
            "*:SSID1:78:WPA2".to_string(),
            " :SSID2:98:OPEN".to_string(),
            " :SSID3:65:WPA3".to_string(),
        ];
        parse_wifi_lines(&mut actions, wifi_lines);
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn test_is_nm_connected() {
        let command_runner = MockCommandRunner {
            outputs: HashMap::new(),
        };
        command_runner.outputs.insert(
            "nmcli [\"--colors\", \"no\", \"-t\", \"-f\", \"DEVICE,STATE\", \"device\", \"status\"]".to_string(),
            Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"wlan0:connected".to_vec(),
                stderr: Vec::new(),
            },
        );

        let result = is_nm_connected(&command_runner, "wlan0");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
