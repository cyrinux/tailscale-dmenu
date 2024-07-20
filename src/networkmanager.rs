use crate::{
    convert_network_strength, is_known_network, notify_connection, prompt_for_password,
    CommandRunner, RealCommandRunner, WifiAction,
};
use std::io::{BufRead, BufReader};
use std::process::Command;

pub fn get_nm_wifi_networks() -> Result<Vec<WifiAction>, Box<dyn std::error::Error>> {
    get_nm_wifi_networks_with_command_runner(&RealCommandRunner)
}

fn get_nm_wifi_networks_with_command_runner(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<WifiAction>, Box<dyn std::error::Error>> {
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
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    let output = command_runner.run_command(
        "nmcli",
        &[
            "--colors",
            "no",
            "-t",
            "-f",
            "IN-USE,SSID,BARS",
            "device",
            "wifi",
        ],
    )?;

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        Ok(Some(reader.lines().map_while(Result::ok).collect()))
    } else {
        Ok(None)
    }
}

fn parse_wifi_lines(actions: &mut Vec<WifiAction>, wifi_lines: Vec<String>) {
    for line in wifi_lines {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 3 {
            let in_use = parts[0].trim();
            let ssid = parts[1].trim();
            let signal = parts[2].trim();
            if !ssid.is_empty() {
                let display = format!(
                    "{} {} {}",
                    if in_use == "*" { "✅" } else { "📶" },
                    ssid,
                    convert_network_strength(signal)
                );
                actions.push(WifiAction::Network(display));
            }
        }
    }
}

pub fn connect_to_nm_wifi(action: &str) -> Result<bool, Box<dyn std::error::Error>> {
    connect_to_nm_wifi_with_command_runner(action, &RealCommandRunner)
}

fn connect_to_nm_wifi_with_command_runner(
    action: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn std::error::Error>> {
    if let Some(ssid) = action.split_whitespace().nth(1) {
        if is_known_network(ssid)? && attempt_connection(ssid, None, command_runner)? {
            Ok(true)
        } else {
            // If the first attempt fails, prompt for a password and retry
            let password = prompt_for_password(command_runner, ssid)?;
            attempt_connection(ssid, Some(password), command_runner)
        }
    } else {
        Ok(false)
    }
}

fn attempt_connection(
    ssid: &str,
    password: Option<String>,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn std::error::Error>> {
    let command = match password {
        Some(ref pwd) => format!("device wifi connect {ssid} password {pwd}"),
        None => format!("connection up {ssid}"),
    };

    let command_parts: Vec<&str> = command.split_whitespace().collect();

    let status = command_runner.run_command("nmcli", &command_parts)?.status;

    if status.success() {
        notify_connection(ssid)?;
        Ok(true)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Failed to connect to Wi-Fi network: {ssid}");
        Ok(false)
    }
}

pub fn disconnect_nm_wifi(interface: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let status = Command::new("nmcli")
        .arg("device")
        .arg("disconnect")
        .arg(interface)
        .status()?;
    Ok(status.success())
}

pub fn is_nm_connected(
    command_runner: &dyn CommandRunner,
    interface: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let output =
        command_runner.run_command("nmcli", &["-t", "-f", "DEVICE,STATE", "device", "status"])?;
    let reader = BufReader::new(output.stdout.as_slice());
    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 2 && parts[0].trim() == interface && parts[1].trim() == "connected" {
            return Ok(true);
        }
    }
    Ok(false)
}
