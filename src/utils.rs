use crate::command::CommandRunner;
use std::error::Error;

pub fn convert_network_strength(line: &str) -> String {
    let strength_symbols = ["_", "▂", "▄", "▆", "█"];
    let stars = line.chars().rev().take_while(|&c| c == '*').count();
    let network_strength = format!(
        "{}{}{}{}",
        strength_symbols.get(1).unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 2 { 2 } else { 0 })
            .unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 3 { 3 } else { 0 })
            .unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 4 { 4 } else { 0 })
            .unwrap_or(&"_"),
    );
    network_strength
}

pub fn prompt_for_password(
    command_runner: &dyn CommandRunner,
    ssid: &str,
) -> Result<String, Box<dyn Error>> {
    let output = command_runner.run_command(
        "sh",
        &[
            "-c",
            &format!("echo 'SETDESC Enter '{ssid}' password\nGETPIN' | pinentry-gnome3"),
        ],
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let password_line = stdout
        .lines()
        .find(|line| line.starts_with("D "))
        .ok_or("Password not found")?;
    let password = password_line.trim_start_matches("D ").trim().to_string();
    Ok(password)
}
