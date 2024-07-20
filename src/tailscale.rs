use crate::format_entry;
use crate::is_command_installed;
use notify_rust::Notification;
use regex::Regex;
use reqwest::blocking::get;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[derive(Debug)]
pub enum TailscaleAction {
    DisableExitNode,
    SetEnable(bool),
    SetExitNode(String),
    SetShields(bool),
}

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

    let active_exit_node = get_active_exit_node();

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let regex = Regex::new(r"\s{2,}").unwrap();

        let mut actions: Vec<String> = reader
            .lines()
            .map_while(Result::ok)
            .filter(|line| line.contains("mullvad.ts.net"))
            .map(|line| parse_mullvad_line(&line, &regex, &active_exit_node))
            .collect();

        let reader = BufReader::new(output.stdout.as_slice());
        actions.extend(
            reader
                .lines()
                .map_while(Result::ok)
                .filter(|line| line.contains("ts.net") && !line.contains("mullvad.ts.net"))
                .map(|line| parse_exit_node_line(&line, &regex, &active_exit_node)),
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

fn parse_mullvad_line(line: &str, regex: &Regex, active_exit_node: &str) -> String {
    let parts: Vec<&str> = regex.split(line).collect();
    let country = parts.get(2).unwrap_or(&"");
    let node_name = parts.get(1).unwrap_or(&"");
    let is_active = active_exit_node == *node_name;
    format_entry(
        "mullvad",
        if is_active { "✅" } else { get_flag(country) },
        &format!("{country} - {node_name}"),
    )
}

fn parse_exit_node_line(line: &str, regex: &Regex, active_exit_node: &str) -> String {
    let parts: Vec<&str> = regex.split(line).collect();
    let node_ip = parts.first().unwrap_or(&"").trim();
    let node_name = parts.get(1).unwrap_or(&"");
    let is_active = active_exit_node == *node_name;
    format_entry(
        "exit-node",
        if is_active { "✅" } else { "🌿" },
        &format!("{node_name} - {node_ip}"),
    )
}

fn get_active_exit_node() -> String {
    let output = Command::new("tailscale")
        .arg("status")
        .arg("--json")
        .output()
        .expect("failed to execute process");

    let json: Value = serde_json::from_slice(&output.stdout).expect("failed to parse JSON");

    if let Some(peers) = json.get("Peer") {
        if let Some(peers_map) = peers.as_object() {
            for peer in peers_map.values() {
                if peer["Active"].as_bool() == Some(true)
                    && peer["ExitNode"].as_bool() == Some(true)
                {
                    if let Some(dns_name) = peer["DNSName"].as_str() {
                        return dns_name.trim_end_matches('.').to_string();
                    }
                }
            }
        }
    }

    String::new()
}

fn set_exit_node(action: &str) -> bool {
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

pub fn handle_tailscale_action(action: &TailscaleAction) -> Result<bool, Box<dyn Error>> {
    if !is_command_installed("tailscale") {
        return Ok(false);
    }

    match action {
        TailscaleAction::DisableExitNode => {
            let status = Command::new("tailscale")
                .arg("set")
                .arg("--exit-node=")
                .status()?;
            check_mullvad()?;
            Ok(status.success())
        }
        TailscaleAction::SetEnable(enable) => {
            let status = Command::new("tailscale")
                .arg(if *enable { "up" } else { "down" })
                .status()?;
            Ok(status.success())
        }
        TailscaleAction::SetExitNode(node) => {
            if set_exit_node(node) {
                check_mullvad()?;
                Ok(true)
            } else {
                check_mullvad()?;
                Ok(false)
            }
        }
        TailscaleAction::SetShields(enable) => {
            let status = Command::new("tailscale")
                .arg("set")
                .arg("--shields-up")
                .arg(if *enable { "true" } else { "false" })
                .status()?;
            Ok(status.success())
        }
    }
}

pub fn is_tailscale_enabled() -> Result<bool, Box<dyn std::error::Error>> {
    let output = Command::new("tailscale").arg("status").output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(!stdout.contains("Tailscale is stopped"));
    }
    Ok(false)
}
