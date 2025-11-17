//! CLI argument parsing for Renacer

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "renacer")]
#[command(version)]
#[command(about = "Pure Rust system call tracer with source correlation", long_about = None)]
pub struct Cli {
    /// Enable source code correlation using DWARF debug info
    #[arg(short, long)]
    pub source: bool,

    /// Filter syscalls to trace (e.g., -e trace=open,read,write or -e trace=file)
    #[arg(short = 'e', long = "expr", value_name = "EXPR")]
    pub filter: Option<String>,

    /// Show statistics summary (syscall counts and timing) instead of individual calls
    #[arg(short = 'c', long = "summary")]
    pub statistics: bool,

    /// Command to trace (everything after --)
    #[arg(last = true)]
    pub command: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parses_command() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "hello"]);
        assert!(cli.command.is_some());
        let cmd = cli.command.unwrap();
        assert_eq!(cmd[0], "echo");
        assert_eq!(cmd[1], "hello");
    }

    #[test]
    fn test_cli_empty_without_command() {
        let cli = Cli::parse_from(["renacer"]);
        assert!(cli.command.is_none());
    }
}
