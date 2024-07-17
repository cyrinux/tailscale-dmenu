use regex::Regex;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::Command;

pub fn get_mullvad_actions() -> Vec<String> {
    let mut actions = Vec::new();

    let output = Command::new("tailscale")
        .arg("exit-node")
        .arg("list")
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let regex = Regex::new(r"\s{2,}").unwrap();
        let mut lines: Vec<String> = reader
            .lines()
            .map_while(Result::ok)
            .filter(|line| line.contains("mullvad.ts.net"))
            .map(|line| {
                let parts: Vec<&str> = regex.split(&line).collect();
                let country = parts.get(2).unwrap_or(&"");
                let node_name = parts.get(1).unwrap_or(&"");
                format!(
                    "mullvad - {} {} - {}",
                    get_flag(country),
                    country,
                    node_name
                )
            })
            .collect();

        lines.sort_by(|a, b| {
            a.split_whitespace()
                .next()
                .cmp(&b.split_whitespace().next())
        });
        actions.extend(lines);
    }

    actions
}

pub fn set_mullvad_exit_node(action: &str) -> bool {
    let regex = Regex::new(r" - ([\w_.-]+)$").unwrap();
    if let Some(caps) = regex.captures(action) {
        let node_name = caps.get(1).map_or("", |m| m.as_str());

        let status = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "tailscale up && tailscale set --exit-node {node_name} --exit-node-allow-lan-access=true",
            ))
            .status();

        match status {
            Ok(status) => {
                if !status.success() {
                    eprintln!("Command executed with non-zero exit status: {}", status);
                }
                true
            }
            Err(err) => {
                eprintln!("Failed to execute command: {:?}", err);
                false
            }
        }
    } else {
        false
    }
}

fn get_flag(country: &str) -> &'static str {
    let country_flags: HashMap<&str, &str> = [
        ("Albania", "🇦🇱"),
        ("Australia", "🇦🇺"),
        ("Austria", "🇦🇹"),
        ("Belgium", "🇧🇪"),
        ("Brazil", "🇧🇷"),
        ("Bulgaria", "🇧🇬"),
        ("Canada", "🇨🇦"),
        ("Chile", "🇨🇱"),
        ("Colombia", "🇨🇴"),
        ("Croatia", "🇭🇷"),
        ("Czech Republic", "🇨🇿"),
        ("Denmark", "🇩🇰"),
        ("Estonia", "🇪🇪"),
        ("Finland", "🇫🇮"),
        ("France", "🇫🇷"),
        ("Germany", "🇩🇪"),
        ("Greece", "🇬🇷"),
        ("Hong Kong", "🇭🇰"),
        ("Hungary", "🇭🇺"),
        ("Indonesia", "🇮🇩"),
        ("Ireland", "🇮🇪"),
        ("Israel", "🇮🇱"),
        ("Italy", "🇮🇹"),
        ("Japan", "🇯🇵"),
        ("Latvia", "🇱🇻"),
        ("Mexico", "🇲🇽"),
        ("Netherlands", "🇳🇱"),
        ("New Zealand", "🇳🇿"),
        ("Norway", "🇳🇴"),
        ("Poland", "🇵🇱"),
        ("Portugal", "🇵🇹"),
        ("Romania", "🇷🇴"),
        ("Serbia", "🇷🇸"),
        ("Singapore", "🇸🇬"),
        ("Slovakia", "🇸🇰"),
        ("Slovenia", "🇸🇮"),
        ("South Africa", "🇿🇦"),
        ("Spain", "🇪🇸"),
        ("Sweden", "🇸🇪"),
        ("Switzerland", "🇨🇭"),
        ("Thailand", "🇹🇭"),
        ("Turkey", "🇹🇷"),
        ("UK", "🇬🇧"),
        ("Ukraine", "🇺🇦"),
        ("USA", "🇺🇸"),
    ]
    .iter()
    .cloned()
    .collect();
    country_flags.get(country).unwrap_or(&"❓")
}
