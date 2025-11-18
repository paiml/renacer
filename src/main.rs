use anyhow::Result;
use clap::Parser;
use renacer::{cli::Cli, filter, tracer, transpiler_map};
use tracing_subscriber::EnvFilter;

/// Initialize tracing subscriber for debug output
fn init_tracing(debug: bool) {
    if debug {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::from_default_env().add_directive(tracing::Level::TRACE.into()),
            )
            .with_writer(std::io::stderr)
            .init();
    }
}

fn main() -> Result<()> {
    let args = Cli::parse();

    // Validate ml_clusters range (must be >= 2)
    if args.ml_clusters < 2 {
        anyhow::bail!(
            "Invalid value for --ml-clusters: {} (must be >= 2)",
            args.ml_clusters
        );
    }

    // Initialize tracing if --debug flag is set
    init_tracing(args.debug);

    // Load transpiler source map if provided (Sprint 24)
    let _source_map = if let Some(map_path) = &args.transpiler_map {
        Some(transpiler_map::TranspilerMap::from_file(map_path)?)
    } else {
        None
    };

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
        stats_extended: args.stats_extended,       // Sprint 19
        anomaly_threshold: args.anomaly_threshold, // Sprint 19
        anomaly_realtime: args.anomaly_realtime,   // Sprint 20
        anomaly_window_size: args.anomaly_window_size, // Sprint 20
        hpu_analysis: args.hpu_analysis,           // Sprint 21
        hpu_cpu_only: args.hpu_cpu_only,           // Sprint 21
        ml_anomaly: args.ml_anomaly,               // Sprint 23
        ml_clusters: args.ml_clusters,             // Sprint 23
        ml_compare: args.ml_compare,               // Sprint 23
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
