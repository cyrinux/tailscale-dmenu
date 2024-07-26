use std::error::Error;
use std::io::{BufRead, BufReader};
use std::process::{Command, Output, Stdio};

/// Trait for running shell commands.
pub trait CommandRunner {
    /// Runs a shell command with the specified arguments.
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error>;
}

/// Struct for running real shell commands.
pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
        Command::new(command).args(args).env("LC_ALL", "C").output()
    }
}

/// Checks if a command is installed on the system.
pub fn is_command_installed(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Reads the output of a command and returns it as a vector of lines.
pub fn read_output_lines(output: &Output) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(BufReader::new(output.stdout.as_slice())
        .lines()
        .collect::<Result<Vec<String>, _>>()?)
}

/// Executes a command and returns whether it was successful.
pub fn execute_command(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .env("LC_ALL", "C")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_or(false, |status| status.success())
}
