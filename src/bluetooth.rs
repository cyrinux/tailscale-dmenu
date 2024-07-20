use crate::format_entry;
use regex::Regex;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[derive(Debug)]
pub enum BluetoothAction {
    ToggleConnect(String),
}

pub fn get_paired_bluetooth_devices() -> Result<Vec<BluetoothAction>, Box<dyn Error>> {
    let output = Command::new("bluetoothctl").arg("devices").output()?;

    let connected_devices = get_connected_devices();

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let devices: Vec<BluetoothAction> = reader
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| parse_bluetooth_device(line, &connected_devices))
            .collect();
        Ok(devices)
    } else {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to fetch paired Bluetooth devices",
        )))
    }
}

fn parse_bluetooth_device(line: String, connected_devices: &[String]) -> Option<BluetoothAction> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        let address = parts[1].to_string();
        let name = parts[2..].join(" ");
        let is_active = connected_devices.contains(&address);
        Some(BluetoothAction::ToggleConnect(format_entry(
            "bluetooth",
            if is_active { "✅" } else { " " },
            &format!("{name} - {address}"),
        )))
    } else {
        None
    }
}

pub fn handle_bluetooth_action(
    action: &BluetoothAction,
    connected_devices: &[String],
) -> Result<bool, Box<dyn Error>> {
    match action {
        BluetoothAction::ToggleConnect(device) => {
            connect_to_bluetooth_device(device, connected_devices)
        }
    }
}

fn connect_to_bluetooth_device(
    device: &str,
    connected_devices: &[String],
) -> Result<bool, Box<dyn Error>> {
    if let Some(address) = extract_device_address(device) {
        let is_active = connected_devices.contains(&address);
        let action = if is_active { "disconnect" } else { "connect" };
        let status = Command::new("bluetoothctl")
            .arg(action)
            .arg(&address)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        if status.success() {
            Ok(true)
        } else {
            #[cfg(debug_assertions)]
            eprintln!("Failed to connect to Bluetooth device: {address}");
            Ok(false)
        }
    } else {
        Ok(false)
    }
}

fn extract_device_address(device: &str) -> Option<String> {
    let re = Regex::new(r" ([\w:]+)$").ok()?;
    re.captures(device)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

pub fn get_connected_devices() -> Vec<String> {
    let output = Command::new("bluetoothctl")
        .arg("info")
        .output()
        .expect("Failed to execute bluetoothctl command");

    let output_str =
        std::str::from_utf8(&output.stdout).expect("Failed to convert output to string");

    let mut mac_addresses = Vec::new();

    for line in output_str.lines() {
        if line.starts_with("Device ") {
            if let Some(mac) = line.split_whitespace().nth(1) {
                mac_addresses.push(mac.to_string());
            }
        }
    }

    mac_addresses
}
