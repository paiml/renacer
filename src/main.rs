use anyhow::Result;
use clap::Parser;

mod cli;
mod dwarf;
mod filter;
mod json_output;
mod stats;
mod syscalls;
mod tracer;

use cli::Cli;

fn main() -> Result<()> {
    let args = Cli::parse();

    // Parse filter expression if provided
    let filter = if let Some(expr) = args.filter {
        filter::SyscallFilter::from_expr(&expr)?
    } else {
        filter::SyscallFilter::all()
    };

    // Either attach to PID or trace command (mutually exclusive)
    match (args.pid, args.command) {
        (Some(pid), None) => {
            // Attach to running process
            tracer::attach_to_pid(pid, args.source, filter, args.statistics, args.timing, args.format, args.follow_forks)?;
        }
        (None, Some(command)) => {
            // Trace command
            tracer::trace_command(&command, args.source, filter, args.statistics, args.timing, args.format, args.follow_forks)?;
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("Cannot specify both -p PID and command. Choose one.");
        }
        (None, None) => {
            anyhow::bail!("Must specify either -p PID or command. Usage: renacer -p PID or renacer -- COMMAND [ARGS...]");
        }
    }

    Ok(())
}
