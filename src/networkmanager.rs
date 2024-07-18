use crate::notify_connection;
use std::io::{BufRead, BufReader};
use std::process::{Command, Output};

pub fn get_nm_wifi_networks() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    get_nm_wifi_networks_with_command_runner(&RealCommandRunner)
}

fn get_nm_wifi_networks_with_command_runner(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut actions = Vec::new();

    if let Some(lines) = fetch_wifi_lines(command_runner)? {
        let has_in_use = lines.iter().any(|line| line.starts_with('*'));

        if !has_in_use {
            let rescan_output = command_runner
                .run_command("nmcli", &["dev", "wifi", "list", "--rescan", "auto"])?;

            if rescan_output.status.success() {
                if let Some(rescan_lines) = fetch_wifi_lines(command_runner)? {
                    parse_wifi_lines(&mut actions, rescan_lines);
                }
            }
        } else {
            parse_wifi_lines(&mut actions, lines);
        }
    }

    Ok(actions)
}

fn fetch_wifi_lines(
    command_runner: &dyn CommandRunner,
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    let output =
        command_runner.run_command("nmcli", &["-t", "-f", "IN-USE,SSID,BARS", "device", "wifi"])?;

    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        Ok(Some(reader.lines().map_while(Result::ok).collect()))
    } else {
        Ok(None)
    }
}

fn parse_wifi_lines(actions: &mut Vec<String>, wifi_lines: Vec<String>) {
    for line in wifi_lines {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 3 {
            let in_use = parts[0].trim();
            let ssid = parts[1].trim();
            let bars = parts[2].trim();
            if !ssid.is_empty() {
                let display = format!(
                    "wifi - {} {} - {}",
                    if in_use == "*" { "🌐" } else { "📶" },
                    ssid,
                    bars
                );
                actions.push(display);
            }
        }
    }
}

pub fn connect_to_nm_wifi(action: &str) -> Result<bool, Box<dyn std::error::Error>> {
    connect_to_nm_wifi_with_command_runner(action, &RealCommandRunner)
}

fn connect_to_nm_wifi_with_command_runner(
    action: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn std::error::Error>> {
    if action.starts_with("wifi - ") {
        let ssid = action.split_whitespace().nth(3).unwrap_or("");
        println!("{ssid}");
        println!("{action}");
        if attempt_connection(ssid, None, command_runner)? {
            Ok(true)
        } else {
            // If the first attempt fails, prompt for a password using dmenu and retry
            let password = prompt_for_password(command_runner)?;
            attempt_connection(ssid, Some(password), command_runner)
        }
    } else {
        Ok(false)
    }
}

fn attempt_connection(
    ssid: &str,
    password: Option<String>,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn std::error::Error>> {
    let command = match password {
        Some(ref pwd) => format!("nmcli device wifi connect '{}' password '{}'", ssid, pwd),
        None => format!("nmcli connection up '{}'", ssid),
    };

    let status = command_runner.run_command("sh", &["-c", &command])?.status;

    if status.success() {
        notify_connection(ssid)?;
        Ok(true)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Failed to connect to Wi-Fi network: {}", ssid);
        Ok(false)
    }
}

fn prompt_for_password(
    command_runner: &dyn CommandRunner,
) -> Result<String, Box<dyn std::error::Error>> {
    let output = command_runner.run_command("dmenu", &["-p", "Enter Wi-Fi password:"])?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub trait CommandRunner {
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error>;
}

struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
        Command::new(command).args(args).output()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockCommandRunner {
        output: Output,
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, _command: &str, _args: &[&str]) -> Result<Output, std::io::Error> {
            Ok(self.output.clone())
        }
    }

    #[test]
    fn test_parse_wifi_lines() {
        let mut actions = Vec::new();
        let wifi_lines = vec!["*:Network1:70%".to_string(), ":Network2:60%".to_string()];
        parse_wifi_lines(&mut actions, wifi_lines);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], "wifi - 🌐 Network1 - 70%");
        assert_eq!(actions[1], "wifi - 📶 Network2 - 60%");
    }

    #[test]
    fn test_get_nm_wifi_networks() {
        let mock_output = Output {
            status: std::os::unix::process::ExitStatusExt::from_raw(0),
            stdout: b"\
            *:Network1:70%
            :Network2:60%"
                .to_vec(),
            stderr: vec![],
        };

        let mock_command_runner = MockCommandRunner {
            output: mock_output,
        };
        let actions = get_nm_wifi_networks_with_command_runner(&mock_command_runner).unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], "wifi - 🌐 Network1 - 70%");
        assert_eq!(actions[1], "wifi - 📶 Network2 - 60%");
    }

    #[test]
    fn test_connect_to_nm_wifi_successful() {
        let mock_output = Output {
            status: std::os::unix::process::ExitStatusExt::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_command_runner = MockCommandRunner {
            output: mock_output,
        };
        let result = connect_to_nm_wifi_with_command_runner(
            "wifi - 🌐 Network1 - 70%",
            &mock_command_runner,
        )
        .unwrap();
        assert!(result);
    }

    #[test]
    fn test_connect_to_nm_wifi_failure() {
        let mock_output = Output {
            status: std::os::unix::process::ExitStatusExt::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_command_runner = MockCommandRunner {
            output: mock_output,
        };
        let result = connect_to_nm_wifi_with_command_runner(
            "wifi - 🌐 Network1 - 70%",
            &mock_command_runner,
        )
        .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_prompt_for_password() {
        let mock_output = Output {
            status: std::os::unix::process::ExitStatusExt::from_raw(0),
            stdout: b"password".to_vec(),
            stderr: vec![],
        };

        let mock_command_runner = MockCommandRunner {
            output: mock_output,
        };
        let password = prompt_for_password(&mock_command_runner).unwrap();
        assert_eq!(password, "password");
    }
}
