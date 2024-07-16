use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::io::{Write, BufRead, BufReader};
use regex::Regex;

fn get_flag(country: &str) -> &'static str {
    let country_flags: HashMap<&str, &str> = [
        ("Albania", "🇦🇱"), ("Australia", "🇦🇺"), ("Austria", "🇦🇹"),
        ("Belgium", "🇧🇪"), ("Brazil", "🇧🇷"), ("Bulgaria", "🇧🇬"),
        ("Canada", "🇨🇦"), ("Chile", "🇨🇱"), ("Colombia", "🇨🇴"),
        ("Croatia", "🇭🇷"), ("Czech Republic", "🇨🇿"), ("Denmark", "🇩🇰"),
        ("Estonia", "🇪🇪"), ("Finland", "🇫🇮"), ("France", "🇫🇷"),
        ("Germany", "🇩🇪"), ("Greece", "🇬🇷"), ("Hong Kong", "🇭🇰"),
        ("Hungary", "🇭🇺"), ("Indonesia", "🇮🇩"), ("Ireland", "🇮🇪"),
        ("Israel", "🇮🇱"), ("Italy", "🇮🇹"), ("Japan", "🇯🇵"),
        ("Latvia", "🇱🇻"), ("Mexico", "🇲🇽"), ("Netherlands", "🇳🇱"),
        ("New Zealand", "🇳🇿"), ("Norway", "🇳🇴"), ("Poland", "🇵🇱"),
        ("Portugal", "🇵🇹"), ("Romania", "🇷🇴"), ("Serbia", "🇷🇸"),
        ("Singapore", "🇸🇬"), ("Slovakia", "🇸🇰"), ("Slovenia", "🇸🇮"),
        ("South Africa", "🇿🇦"), ("Spain", "🇪🇸"), ("Sweden", "🇸🇪"),
        ("Switzerland", "🇨🇭"), ("Thailand", "🇹🇭"), ("Turkey", "🇹🇷"),
        ("UK", "🇬🇧"), ("Ukraine", "🇺🇦"), ("USA", "🇺🇸")
    ].iter().cloned().collect();

    *country_flags.get(country).unwrap_or(&"❓")
}

fn get_actions() -> Option<String> {
    let mut actions = String::from(
        "❌ - Disable mullvad - disable_mullvad\n\
         ❌ - Disable tailscale - disable_tailscale\n\
         ✅ - Enable tailscale - enable_tailscale\n\
         RaspberryPi - raspberrypi - raspberrypi\n"
    );

    let output = Command::new("tailscale")
        .arg("exit-node")
        .arg("list")
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let regex = Regex::new(r"\s{2,}").unwrap();
        let mut lines: Vec<String> = reader.lines()
            .filter_map(Result::ok)
            .filter(|line| line.contains("mullvad.ts.net"))
            .map(|line| {
                let parts: Vec<&str> = regex.split(&line).collect();
                let country = parts.get(2).unwrap_or(&"");
                let rest = parts.get(3).unwrap_or(&"");
                let node_name = parts.get(1).unwrap_or(&"");
                format!("{} {} - {} - {}", get_flag(country), country, rest, node_name)
            })
            .collect();

        lines.sort_by(|a, b| a.split_whitespace().next().cmp(&b.split_whitespace().next()));
        actions.push_str(&lines.join("\n"));
    }

    let mut child = Command::new("dmenu")
        .arg("-f")
        .arg("--no-multi")
        .arg("-p")
        .arg("Select action:")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to execute dmenu");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin.write_all(actions.as_bytes()).expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to read dmenu output");

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn set_action() {
    if let Some(action) = get_actions() {
        let regex = Regex::new(r" - ([\w_.-]+)$").unwrap();
        if let Some(caps) = regex.captures(&action) {
            let action_str = caps.get(1).map_or("", |m| m.as_str());
            match action_str {
                "disable_mullvad" => {
                    Command::new("tailscale")
                        .arg("set")
                        .arg("--exit-node=")
                        .arg("--exit-node-allow-lan-access=false")
                        .status()
                        .expect("Failed to disable mullvad");
                }
                "disable_tailscale" => {
                    Command::new("tailscale")
                        .arg("down")
                        .status()
                        .expect("Failed to disable tailscale");
                }
                "enable_tailscale" => {
                    Command::new("tailscale")
                        .arg("up")
                        .status()
                        .expect("Failed to enable tailscale");
                }
                _ => {
                    if !action_str.is_empty() && !action.contains("❌") && !action.contains("✅") {
                        let node_name = action_str;
                        Command::new("tailscale")
                            .arg("up")
                            .status()
                            .expect("Failed to enable tailscale");

                        Command::new("tailscale")
                            .arg("set")
                            .arg("--exit-node")
                            .arg(node_name)
                            .arg("--exit-node-allow-lan-access=true")
                            .status()
                            .expect("Failed to set exit node");
                    }
                }
            }
        }
    }
}

fn main() {
    set_action();

    Command::new("tailscale")
        .arg("status")
        .status()
        .expect("Failed to get tailscale status");
}
