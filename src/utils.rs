use std::io::Write;
use std::process::{Command, Stdio};

/// Converts network strength to a visual representation.
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

/// Prompts the user for a password using `pinentry-gnome3`.
pub fn prompt_for_password(ssid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut child = Command::new("pinentry-gnome3")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        write!(stdin, "SETDESC Enter {ssid} password\nGETPIN\n")?;
    }

    let output = child.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let password_line = stdout
        .lines()
        .find(|line| line.starts_with("D "))
        .ok_or("Password not found")?;
    let password = password_line.trim_start_matches("D ").trim().to_string();

    Ok(password)
}
