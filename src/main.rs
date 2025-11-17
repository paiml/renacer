use anyhow::Result;
use clap::Parser;

mod cli;
mod dwarf;
mod filter;
mod stats;
mod syscalls;
mod tracer;

use cli::Cli;

fn main() -> Result<()> {
    let args = Cli::parse();

    if let Some(command) = args.command {
        // Parse filter expression if provided
        let filter = if let Some(expr) = args.filter {
            filter::SyscallFilter::from_expr(&expr)?
        } else {
            filter::SyscallFilter::all()
        };

        tracer::trace_command(&command, args.source, filter, args.statistics)?;
    } else {
        anyhow::bail!("No command specified. Usage: renacer -- COMMAND [ARGS...]");
    }

    Ok(())
}
