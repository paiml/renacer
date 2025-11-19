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

/// Print function mappings from transpiler source map (Sprint 25)
fn print_function_mappings(map: &transpiler_map::TranspilerMap, show_context: bool) {
    if show_context {
        println!("=== Transpiler Source Map ===");
        println!("Source Language: {} -> Rust", map.source_language());
        println!("Source File: {}", map.source_file().display());
        println!();
    }

    if !map.function_map.is_empty() {
        if show_context {
            println!("Function Mappings (Rust -> {}):", map.source_language());
            println!("─────────────────────────────────────────");
        }
        for (rust_fn, python_fn) in &map.function_map {
            println!("{} -> {}", rust_fn, python_fn);
        }
        if show_context {
            println!("─────────────────────────────────────────");
            println!();
        }
    }
}

/// Print stack trace mappings from transpiler source map (Sprint 26)
fn print_stack_trace_mappings(map: &transpiler_map::TranspilerMap, show_context: bool) {
    if show_context {
        println!("=== Stack Trace Mapping ===");
        println!(
            "Source: {} -> {}",
            map.source_file().display(),
            map.generated_file().display()
        );
        println!();
    }

    if !map.mappings.is_empty() {
        if show_context {
            println!("Line Mappings (Rust -> {}):", map.source_language());
            println!("─────────────────────────────────────────");
        }
        for mapping in &map.mappings {
            println!(
                "{} ({}:{}) -> {}:{}",
                mapping.rust_function,
                map.generated_file().display(),
                mapping.rust_line,
                map.source_file().display(),
                mapping.python_line
            );
        }
        if show_context {
            println!("─────────────────────────────────────────");
            println!();
        }
    }
}

/// Print error correlation mappings from transpiler source map (Sprint 27)
fn print_error_correlation_mappings(map: &transpiler_map::TranspilerMap, show_context: bool) {
    if show_context {
        println!("=== Error Correlation Mapping ===");
        println!(
            "Errors in {} will map to {}",
            map.generated_file().display(),
            map.source_file().display()
        );
        println!();
    }

    if !map.mappings.is_empty() {
        if show_context {
            println!("Available Line Mappings ({} entries):", map.mappings.len());
            println!("─────────────────────────────────────────");
        }
        for mapping in &map.mappings {
            println!(
                "  {}:{} -> {}:{} ({})",
                map.generated_file().display(),
                mapping.rust_line,
                map.source_file().display(),
                mapping.python_line,
                mapping.python_function
            );
        }
        if show_context {
            println!("─────────────────────────────────────────");
            println!();
        }
    }
}

/// Execute the tracer based on PID or command arguments
fn run_tracer(
    pid: Option<i32>,
    command: Option<Vec<String>>,
    config: tracer::TracerConfig,
) -> Result<()> {
    match (pid, command) {
        (Some(pid), None) => {
            tracer::attach_to_pid(pid, config)?;
        }
        (None, Some(command)) => {
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
    let source_map = if let Some(map_path) = &args.transpiler_map {
        Some(transpiler_map::TranspilerMap::from_file(map_path)?)
    } else {
        None
    };

    // Sprint 25: Print function name correlations when using --function-time with source map
    if let (true, Some(ref map)) = (args.function_time, &source_map) {
        print_function_mappings(map, args.show_transpiler_context);
    }

    // Sprint 26: Print stack trace mapping info when using --rewrite-stacktrace with source map
    if let (true, Some(ref map)) = (args.rewrite_stacktrace, &source_map) {
        print_stack_trace_mappings(map, args.show_transpiler_context);
    }

    // Sprint 27: Print error correlation info when using --rewrite-errors with source map
    if let (true, Some(ref map)) = (args.rewrite_errors, &source_map) {
        print_error_correlation_mappings(map, args.show_transpiler_context);
    }

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
        ml_outliers: args.ml_outliers,             // Sprint 22
        ml_outlier_threshold: args.ml_outlier_threshold, // Sprint 22
        ml_outlier_trees: args.ml_outlier_trees,   // Sprint 22
        explain: args.explain,                     // Sprint 22/23
        dl_anomaly: args.dl_anomaly,               // Sprint 23
        dl_threshold: args.dl_threshold,           // Sprint 23
        dl_hidden_size: args.dl_hidden_size,       // Sprint 23
        dl_epochs: args.dl_epochs,                 // Sprint 23
        transpiler_map: source_map,                // Sprint 24-28
    };

    // Either attach to PID or trace command (mutually exclusive)
    run_tracer(args.pid, args.command, config)?;

    Ok(())
}
