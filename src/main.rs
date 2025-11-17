use anyhow::Result;
use clap::Parser;
use renacer::{cli::Cli, filter, tracer};

fn main() -> Result<()> {
    let args = Cli::parse();

    // Parse filter expression if provided
    let filter = if let Some(expr) = args.filter {
        filter::SyscallFilter::from_expr(&expr)?
    } else {
        filter::SyscallFilter::all()
    };

    // Create tracer configuration
    let config = tracer::TracerConfig {
        enable_source: args.source,
        filter,
        statistics_mode: args.statistics,
        timing_mode: args.timing,
        output_format: args.format,
        follow_forks: args.follow_forks,
        profile_self: args.profile_self,
        function_time: args.function_time,
    };

    // Either attach to PID or trace command (mutually exclusive)
    match (args.pid, args.command) {
        (Some(pid), None) => {
            // Attach to running process
            tracer::attach_to_pid(pid, config)?;
        }
        (None, Some(command)) => {
            // Trace command
            tracer::trace_command(&command, config)?;
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
