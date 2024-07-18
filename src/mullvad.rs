use regex::Regex;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

pub fn get_mullvad_actions() -> Vec<String> {
    get_mullvad_actions_with_command_runner(&RealCommandRunner)
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
    format!(
        "mullvad - {} {} - {}",
        get_flag(country),
        country,
        node_name
    )
}

pub fn set_mullvad_exit_node(action: &str) -> bool {
    if !action.starts_with("mullvad - ") {
        return false;
    }

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
    let regex = Regex::new(r" - ([\w_.-]+)$").ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    struct MockCommandRunner {
        output: std::process::Output,
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(
            &self,
            _command: &str,
            _args: &[&str],
        ) -> Result<std::process::Output, std::io::Error> {
            Ok(self.output.clone())
        }
    }

    #[test]
    fn test_parse_mullvad_line() {
        let regex = Regex::new(r"\s{2,}").unwrap();
        let line = " 100.91.198.95       al-tia-wg-001.mullvad.ts.net               Albania            Tirana                 -";
        let result = parse_mullvad_line(line, &regex);
        assert_eq!(
            result,
            "mullvad - 🇦🇱 Albania - al-tia-wg-001.mullvad.ts.net"
        );
    }

    #[test]
    fn test_extract_node_name() {
        let action = "mullvad - 🇦🇱 Albania - al-tia-wg-001.mullvad.ts.net";
        let result = extract_node_name(action);
        assert_eq!(result, Some("al-tia-wg-001.mullvad.ts.net"));
    }

    #[test]
    fn test_get_flag() {
        assert_eq!(get_flag("Germany"), "🇩🇪");
        assert_eq!(get_flag("Unknown"), "❓");
    }

    #[test]
    fn test_execute_command() {
        let result = execute_command("echo", &["Hello, world!"]);
        assert!(result);
    }

    #[test]
    fn test_get_mullvad_actions() {
        let mock_output = std::process::Output {
            status: std::os::unix::process::ExitStatusExt::from_raw(0),
            stdout: b"\
IP                  HOSTNAME                                   COUNTRY            CITY                   STATUS
100.91.198.95       al-tia-wg-001.mullvad.ts.net               Albania            Tirana                 -
100.65.216.68       au-adl-wg-301.mullvad.ts.net               Australia          Any                    selected
100.79.65.118       at-vie-wg-001.mullvad.ts.net               Austria            Vienna                 -
            "
                .to_vec(),
            stderr: vec![],
        };

        let mock_command_runner = MockCommandRunner {
            output: mock_output,
        };
        let actions = get_mullvad_actions_with_command_runner(&mock_command_runner);
        assert_eq!(actions.len(), 3);
        assert_eq!(
            actions[0],
            "mullvad - 🇦🇱 Albania - al-tia-wg-001.mullvad.ts.net"
        );
        assert_eq!(
            actions[1],
            "mullvad - 🇦🇺 Australia - au-adl-wg-301.mullvad.ts.net"
        );
        assert_eq!(
            actions[2],
            "mullvad - 🇦🇹 Austria - at-vie-wg-001.mullvad.ts.net"
        );
    }
}
