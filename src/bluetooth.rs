use crate::command::{read_output_lines, CommandRunner};
use crate::format_entry;
use regex::Regex;
use std::error::Error;
use std::process::Output;

#[derive(Debug)]
pub enum BluetoothAction {
    ToggleConnect(String),
}

pub fn get_paired_bluetooth_devices(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<BluetoothAction>, Box<dyn Error>> {
    let output = command_runner.run_command("bluetoothctl", &["devices"])?;
    let connected_devices = get_connected_devices(command_runner)?;

    if output.status.success() {
        let devices = parse_bluetooth_devices(&output, &connected_devices)?;
        Ok(devices)
    } else {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to fetch paired Bluetooth devices",
        )))
    }
}

fn parse_bluetooth_devices(
    output: &Output,
    connected_devices: &[String],
) -> Result<Vec<BluetoothAction>, Box<dyn Error>> {
    let reader = read_output_lines(output)?;
    let devices = reader
        .into_iter()
        .filter_map(|line| parse_bluetooth_device(line, connected_devices))
        .collect();
    Ok(devices)
}

fn parse_bluetooth_device(line: String, connected_devices: &[String]) -> Option<BluetoothAction> {
    // Define a regex pattern for matching MAC addresses and device names
    // Check if the line matches the pattern and extract captures
    Regex::new(r"([0-9A-Fa-f]{2}(:[0-9A-Fa-f]{2}){5})\s+(.*)")
        .ok()?
        .captures(&line)
        .and_then(|caps| {
            // Extract the MAC address and device name from the captures
            let address = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(3).map(|m| m.as_str().to_string());

            // Check if we successfully extracted both the address and the name
            address.and_then(|addr| {
                name.map(|nm| {
                    // Check if the device is active
                    let is_active = connected_devices.contains(&addr);

                    // Return the appropriate BluetoothAction
                    BluetoothAction::ToggleConnect(format_entry(
                        "bluetooth",
                        if is_active { "✅" } else { " " },
                        &format!("{nm:<25} - {addr}"),
                    ))
                })
            })
        })
}

pub fn handle_bluetooth_action(
    action: &BluetoothAction,
    connected_devices: &[String],
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    match action {
        BluetoothAction::ToggleConnect(device) => {
            connect_to_bluetooth_device(device, connected_devices, command_runner)
        }
    }
}

fn connect_to_bluetooth_device(
    device: &str,
    connected_devices: &[String],
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    if let Some(address) = extract_device_address(device) {
        let is_active = connected_devices.contains(&address);
        let action = if is_active { "disconnect" } else { "connect" };
        #[cfg(debug_assertions)]
        println!("Connect to Bluetooth device: {address}");
        let status = command_runner
            .run_command("bluetoothctl", &[action, &address])?
            .status;

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
    Regex::new(r"([0-9A-Fa-f]{2}(:[0-9A-Fa-f]{2}){5})$")
        .ok()?
        .captures(device)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

pub fn get_connected_devices(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<String>, Box<dyn Error>> {
    let output = command_runner.run_command("bluetoothctl", &["info"])?;
    let mac_addresses = read_output_lines(&output)?
        .into_iter()
        .filter(|line| line.starts_with("Device "))
        .filter_map(|line| line.split_whitespace().nth(1).map(|s| s.to_string()))
        .collect();
    Ok(mac_addresses)
}
