use crate::format_entry;
use notify_rust::Notification;
use regex::Regex;
use reqwest::blocking::get;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

pub fn get_mullvad_actions() -> Vec<String> {
    get_mullvad_actions_with_command_runner(&RealCommandRunner)
}

pub fn check_mullvad() -> Result<(), Box<dyn std::error::Error>> {
    let response = get("https://am.i.mullvad.net/connected")?.text()?;
    Notification::new()
        .summary("Connected Status")
        .body(response.trim())
        .show()?;
    Ok(())
}

fn get_mullvad_actions_with_command_runner(command_runner: &dyn CommandRunner) -> Vec<String> {
    let output = command_runner
        .run_command("tailscale", &["exit-node", "list"])
        .expect("Failed to execute command");

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let regex = Regex::new(r"\s{2,}").unwrap();

        let mut actions: Vec<String> = reader
            .lines()
            .map_while(Result::ok)
            .filter(|line| line.contains("mullvad.ts.net"))
            .map(|line| parse_mullvad_line(&line, &regex))
            .collect();

        let reader = BufReader::new(output.stdout.as_slice());
        actions.extend(
            reader
                .lines()
                .map_while(Result::ok)
                .filter(|line| line.contains("ts.net") && !line.contains("mullvad.ts.net"))
                .map(|line| parse_exit_node_line(&line, &regex)),
        );

        actions.sort_by(|a, b| {
            a.split_whitespace()
                .next()
                .cmp(&b.split_whitespace().next())
        });
        actions
    } else {
        Vec::new()
    }
}

fn parse_mullvad_line(line: &str, regex: &Regex) -> String {
    let parts: Vec<&str> = regex.split(line).collect();
    let country = parts.get(2).unwrap_or(&"");
    let node_name = parts.get(1).unwrap_or(&"");
    format_entry(
        "mullvad",
        get_flag(country),
        &format!("{country} - {node_name}"),
    )
}

fn parse_exit_node_line(line: &str, regex: &Regex) -> String {
    let parts: Vec<&str> = regex.split(line).collect();
    let node_ip = parts.first().unwrap_or(&"").trim();
    let node_name = parts.get(1).unwrap_or(&"");
    format_entry("exit-node", "🌿", &format!("{node_name} - {node_ip}"))
}

pub fn set_exit_node(action: &str) -> bool {
    let node_name = match extract_node_name(action) {
        Some(name) => name,
        None => return false,
    };

    if !execute_command("tailscale", &["up"]) {
        return false;
    }

    execute_command(
        "tailscale",
        &[
            "set",
            "--exit-node",
            node_name,
            "--exit-node-allow-lan-access=true",
        ],
    )
}

fn extract_node_name(action: &str) -> Option<&str> {
    let regex = Regex::new(r" ([\w_.-]+)$").ok()?;
    regex
        .captures(action)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
}

fn execute_command(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_or(false, |status| status.success())
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

pub fn is_exit_node_active() -> Result<bool, Box<dyn std::error::Error>> {
    let output = Command::new("tailscale").arg("status").output()?;

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        for line in reader.lines() {
            let line = line?;
            if line.contains("active; exit node;") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub trait CommandRunner {
    fn run_command(
        &self,
        command: &str,
        args: &[&str],
    ) -> Result<std::process::Output, std::io::Error>;
}

struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run_command(
        &self,
        command: &str,
        args: &[&str],
    ) -> Result<std::process::Output, std::io::Error> {
        Command::new(command).args(args).output()
    }
}
