use anyhow::Result;
use clap::Parser;

mod cli;
mod syscalls;
mod tracer;

use cli::Cli;

fn main() -> Result<()> {
    let args = Cli::parse();

    if let Some(command) = args.command {
        tracer::trace_command(&command)?;
    } else {
        anyhow::bail!("No command specified. Usage: renacer -- COMMAND [ARGS...]");
    }

    Ok(())
}
