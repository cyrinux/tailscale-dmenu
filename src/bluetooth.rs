use regex::Regex;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::process::Command;

#[derive(Debug)]
pub enum BluetoothAction {
    Connect(String),
    Disconnect,
}

pub fn get_paired_bluetooth_devices() -> Result<Vec<BluetoothAction>, Box<dyn Error>> {
    let output = Command::new("bluetoothctl").arg("devices").output()?;

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let devices: Vec<BluetoothAction> = reader
            .lines()
            .map_while(Result::ok)
            .filter_map(parse_bluetooth_device)
            .collect();
        Ok(devices)
    } else {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to fetch paired Bluetooth devices",
        )))
    }
}

fn parse_bluetooth_device(line: String) -> Option<BluetoothAction> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        let address = parts[1].to_string();
        let name = parts[2..].join(" ");
        Some(BluetoothAction::Connect(format!("{name} - {address}")))
    } else {
        None
    }
}

pub fn connect_to_bluetooth_device(device: &str) -> Result<bool, Box<dyn Error>> {
    if let Some(address) = extract_device_address(device) {
        let status = Command::new("bluetoothctl")
            .arg("connect")
            .arg(&address)
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
    let re = Regex::new(r"\(([^)]+)\)").ok()?;
    re.captures(device)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

pub fn disconnect_bluetooth_device() -> Result<bool, Box<dyn Error>> {
    let status = Command::new("bluetoothctl").arg("disconnect").status()?;
    Ok(status.success())
}
