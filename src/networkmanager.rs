use regex::Regex;

use crate::command::{read_output_lines, CommandRunner};
use crate::utils::{convert_network_strength, prompt_for_password};
use crate::{notify_connection, parse_wifi_action, WifiAction};
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
    let (ssid, security) = parse_wifi_action(action)?;
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
