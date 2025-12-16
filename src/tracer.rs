//! System call tracing using ptrace
//!
//! Sprint 3-4: Trace all syscalls with name resolution

use anyhow::{Context, Result};
use nix::sys::ptrace;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::os::unix::process::CommandExt;
use std::process::Command;
use tracing::{info, trace, warn};

use crate::syscalls;

/// Configuration for tracer behavior
pub struct TracerConfig {
    pub enable_source: bool,
    pub filter: crate::filter::SyscallFilter,
    pub statistics_mode: bool,
    pub timing_mode: bool,
    pub output_format: crate::cli::OutputFormat,
    pub follow_forks: bool,
    pub profile_self: bool,
    pub function_time: bool,
    pub stats_extended: bool,   // Sprint 19: Extended statistics with Trueno
    pub anomaly_threshold: f32, // Sprint 19: Anomaly detection threshold (σ)
    pub anomaly_realtime: bool, // Sprint 20: Real-time anomaly detection
    pub anomaly_window_size: usize, // Sprint 20: Sliding window size
    pub hpu_analysis: bool,     // Sprint 21: HPU-accelerated analysis (GPU if available)
    pub hpu_cpu_only: bool,     // Sprint 21: Force CPU backend (disable GPU)
    pub ml_anomaly: bool,       // Sprint 23: ML-based anomaly detection using Aprender
    pub ml_clusters: usize,     // Sprint 23: Number of clusters for KMeans
    pub ml_compare: bool,       // Sprint 23: Compare ML results with z-score
    pub ml_outliers: bool,      // Sprint 22: Isolation Forest outlier detection
    pub ml_outlier_threshold: f32, // Sprint 22: Contamination threshold
    pub ml_outlier_trees: usize, // Sprint 22: Number of trees
    pub explain: bool,          // Sprint 22: Enable explainability
    pub dl_anomaly: bool,       // Sprint 23: Deep Learning Autoencoder anomaly detection
    pub dl_threshold: f32,      // Sprint 23: Reconstruction error threshold (σ multiplier)
    pub dl_hidden_size: usize,  // Sprint 23: Autoencoder hidden layer size
    pub dl_epochs: usize,       // Sprint 23: Training epochs
    pub trace_transpiler_decisions: bool, // Sprint 26: Trace transpiler compile-time decisions
    pub transpiler_map: Option<crate::transpiler_map::TranspilerMap>, // Sprint 24-28: Transpiler source mapping
    pub otlp_endpoint: Option<String>, // Sprint 30: OpenTelemetry OTLP endpoint
    pub otlp_service_name: String,     // Sprint 30: Service name for OTLP traces
    pub trace_parent: Option<String>,  // Sprint 33: W3C Trace Context for distributed tracing
    pub chaos_config: Option<crate::chaos::ChaosConfig>, // Sprint 47: Chaos engineering (Issue #17)
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self {
            enable_source: false,
            filter: crate::filter::SyscallFilter::all(),
            statistics_mode: false,
            timing_mode: false,
            output_format: crate::cli::OutputFormat::default(),
            follow_forks: false,
            profile_self: false,
            function_time: false,
            stats_extended: false,
            anomaly_threshold: 2.0,
            anomaly_realtime: false,
            anomaly_window_size: 100,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            explain: false,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            trace_transpiler_decisions: false,
            transpiler_map: None,
            otlp_endpoint: None,
            otlp_service_name: "renacer".to_string(),
            trace_parent: None,
            chaos_config: None,
        }
    }
}

/// Sprint 9-10 Scope
/// - `-p PID` flag to attach to running processes
/// - Uses `PTRACE_ATTACH` instead of `fork()` + `PTRACE_TRACEME`
pub fn attach_to_pid(pid: i32, config: TracerConfig) -> Result<i32> {
    let pid = Pid::from_raw(pid);

    // Attach to the running process
    ptrace::attach(pid).context(format!("Failed to attach to PID {pid}"))?;

    // Wait for SIGSTOP from PTRACE_ATTACH
    waitpid(pid, None).context("Failed to wait for attach signal")?;

    eprintln!("[renacer: Attached to process {pid}]");

    // Use the same tracing logic as trace_command
    trace_child(pid, config)
}

/// Sprint 9-10 Scope
/// - Syscall filtering via -e trace= expressions
/// - Statistics mode via -c flag
/// - Timing per syscall via -T flag
/// - JSON output via --format json
/// - Fork following via -f flag
pub fn trace_command(command: &[String], config: TracerConfig) -> Result<i32> {
    if command.is_empty() {
        anyhow::bail!("Command array is empty");
    }

    let program = &command[0];
    let args = &command[1..];

    // Sprint 47: Clone chaos config for child process (Issue #17)
    let chaos_config = config.chaos_config.clone();

    // Fork: parent will trace, child will exec
    // SAFETY: fork() is safe to call; we handle both parent and child cases properly
    match unsafe { fork() }.context("Failed to fork")? {
        ForkResult::Parent { child } => trace_child(child, config),
        ForkResult::Child => {
            // Child: allow tracing and exec target program
            ptrace::traceme().context("Failed to PTRACE_TRACEME")?;

            // Sprint 47: Apply chaos resource limits before exec (Issue #17)
            if let Some(ref chaos) = chaos_config {
                if let Err(e) = chaos.apply_limits() {
                    eprintln!("[renacer: Warning: Failed to apply chaos limits: {e}]");
                }
            }

            // Use std::process::Command for exec
            let err = Command::new(program).args(args).exec();

            // If we get here, exec failed
            eprintln!("Failed to exec {program}: {err}");
            std::process::exit(1);
        }
    }
}

/// Tracers and profilers used during tracing
struct Tracers {
    profiling_ctx: Option<crate::profiling::ProfilingContext>,
    function_profiler: Option<crate::function_profiler::FunctionProfiler>,
    stats_tracker: Option<crate::stats::StatsTracker>,
    json_output: Option<crate::json_output::JsonOutput>,
    csv_output: Option<crate::csv_output::CsvOutput>,
    csv_stats_output: Option<crate::csv_output::CsvStatsOutput>,
    html_output: Option<crate::html_output::HtmlOutput>, // Sprint 22
    anomaly_detector: Option<crate::anomaly::AnomalyDetector>, // Sprint 20
    #[allow(dead_code)] // Sprint 26: Will be used once stderr capture is wired
    decision_tracer: Option<crate::decision_trace::DecisionTracer>, // Sprint 26
    #[cfg(feature = "otlp")]
    otlp_exporter: Option<crate::otlp_exporter::OtlpExporter>, // Sprint 30
}

/// Initialize profiling-related tracers
fn initialize_profiling_tracers(
    config: &TracerConfig,
) -> (
    Option<crate::profiling::ProfilingContext>,
    Option<crate::function_profiler::FunctionProfiler>,
    Option<crate::anomaly::AnomalyDetector>,
) {
    let profiling_ctx = if config.profile_self {
        Some(crate::profiling::ProfilingContext::new())
    } else {
        None
    };

    let function_profiler = if config.function_time {
        Some(crate::function_profiler::FunctionProfiler::new())
    } else {
        None
    };

    let anomaly_detector = if config.anomaly_realtime {
        Some(crate::anomaly::AnomalyDetector::new(
            config.anomaly_window_size,
            config.anomaly_threshold,
        ))
    } else {
        None
    };

    (profiling_ctx, function_profiler, anomaly_detector)
}

/// Initialize output format tracers (JSON, CSV, HTML)
fn initialize_output_tracers(
    config: &TracerConfig,
) -> (
    Option<crate::json_output::JsonOutput>,
    Option<crate::csv_output::CsvOutput>,
    Option<crate::csv_output::CsvStatsOutput>,
    Option<crate::html_output::HtmlOutput>,
) {
    use crate::cli::OutputFormat;

    let json_output = if matches!(config.output_format, OutputFormat::Json) {
        Some(crate::json_output::JsonOutput::new())
    } else {
        None
    };

    let csv_output = if matches!(config.output_format, OutputFormat::Csv) && !config.statistics_mode
    {
        Some(crate::csv_output::CsvOutput::new(
            config.timing_mode,
            config.enable_source,
        ))
    } else {
        None
    };

    let csv_stats_output =
        if matches!(config.output_format, OutputFormat::Csv) && config.statistics_mode {
            Some(crate::csv_output::CsvStatsOutput::new())
        } else {
            None
        };

    let html_output = if matches!(config.output_format, OutputFormat::Html) {
        Some(crate::html_output::HtmlOutput::new(
            config.timing_mode,
            config.enable_source,
        ))
    } else {
        None
    };

    (json_output, csv_output, csv_stats_output, html_output)
}

/// Initialize all tracers and profilers based on config
fn initialize_tracers(config: &TracerConfig) -> Tracers {
    // Initialize profiling tracers
    let (profiling_ctx, function_profiler, anomaly_detector) = initialize_profiling_tracers(config);

    // Initialize output format tracers
    let (json_output, csv_output, csv_stats_output, html_output) =
        initialize_output_tracers(config);

    // Create stats_tracker if statistics mode is enabled OR if ML/DL anomaly analysis is enabled
    let stats_tracker =
        if config.statistics_mode || config.ml_anomaly || config.ml_outliers || config.dl_anomaly {
            Some(crate::stats::StatsTracker::new())
        } else {
            None
        };

    // Initialize decision tracer for transpiler decision tracking (Sprint 26)
    let decision_tracer = if config.trace_transpiler_decisions {
        Some(crate::decision_trace::DecisionTracer::new())
    } else {
        None
    };

    // Initialize OTLP exporter if endpoint is provided (Sprint 30)
    #[cfg(feature = "otlp")]
    let otlp_exporter = if let Some(ref endpoint) = config.otlp_endpoint {
        // Sprint 33: Extract trace context from CLI flag or environment
        use crate::trace_context::TraceContext;

        let trace_context = config
            .trace_parent
            .as_ref()
            .and_then(|s| TraceContext::parse(s).ok())
            .or_else(TraceContext::from_env);

        if trace_context.is_some() {
            eprintln!("[renacer: Distributed tracing enabled - joining parent trace]");
        }

        match crate::otlp_exporter::OtlpExporter::new(
            crate::otlp_exporter::OtlpConfig::new(
                endpoint.clone(),
                config.otlp_service_name.clone(),
            ),
            trace_context,
        ) {
            Ok(exporter) => {
                eprintln!("[renacer: OTLP export enabled to {endpoint}]");
                Some(exporter)
            }
            Err(e) => {
                eprintln!("[renacer: OTLP initialization failed: {e}]");
                None
            }
        }
    } else {
        None
    };

    Tracers {
        profiling_ctx,
        function_profiler,
        stats_tracker,
        json_output,
        csv_output,
        csv_stats_output,
        html_output,
        anomaly_detector,
        decision_tracer,
        #[cfg(feature = "otlp")]
        otlp_exporter,
    }
}

/// Initialize ptrace options for the child process
fn setup_ptrace_options(child: Pid, follow_forks: bool) -> Result<()> {
    setup_ptrace_options_internal(child, follow_forks, true)
}

/// Initialize ptrace options with optional initial wait
fn setup_ptrace_options_internal(child: Pid, follow_forks: bool, wait_first: bool) -> Result<()> {
    // Wait for initial SIGSTOP (from PTRACE_TRACEME or fork event)
    if wait_first {
        trace!(pid = %child, "waiting for initial SIGSTOP");
        let status = waitpid(child, None).context("Failed to wait for child")?;
        trace!(pid = %child, status = ?status, "initial wait completed");
    }

    // Set ptrace options to trace syscalls
    let mut options = ptrace::Options::PTRACE_O_TRACESYSGOOD | ptrace::Options::PTRACE_O_EXITKILL;

    // Add fork following options if enabled
    if follow_forks {
        options |= ptrace::Options::PTRACE_O_TRACEFORK
            | ptrace::Options::PTRACE_O_TRACEVFORK
            | ptrace::Options::PTRACE_O_TRACECLONE;
    }

    trace!(pid = %child, "setting ptrace options");
    ptrace::setoptions(child, options).context("Failed to set ptrace options")?;
    trace!(pid = %child, "ptrace options set");

    // Continue the child to start syscall tracing
    trace!(pid = %child, "sending initial PTRACE_SYSCALL");
    ptrace::syscall(child, None).context("Failed to continue child with PTRACE_SYSCALL")?;
    trace!(pid = %child, "initial PTRACE_SYSCALL sent");

    Ok(())
}

/// Load DWARF debug info for source correlation
fn load_dwarf_context(child: Pid) -> Option<crate::dwarf::DwarfContext> {
    if let Ok(exe_path) = std::fs::read_link(format!("/proc/{child}/exe")) {
        match crate::dwarf::DwarfContext::load(&exe_path) {
            Ok(ctx) => {
                eprintln!(
                    "[renacer: DWARF debug info loaded from {}]",
                    exe_path.display()
                );
                Some(ctx)
            }
            Err(e) => {
                eprintln!("[renacer: Warning - failed to load DWARF: {e}]");
                eprintln!("[renacer: Continuing without source correlation]");
                None
            }
        }
    } else {
        None
    }
}

/// Handle ptrace fork/vfork/clone events (Sprint 18: Multi-process tracing)
fn handle_ptrace_event(
    pid: Pid,
    event: i32,
    processes: &mut std::collections::HashMap<Pid, ProcessState>,
    config: &TracerConfig,
) -> Result<()> {
    use nix::libc;

    // Check if this is a fork/vfork/clone event
    match event {
        libc::PTRACE_EVENT_FORK | libc::PTRACE_EVENT_VFORK | libc::PTRACE_EVENT_CLONE => {
            // Extract the new child PID
            let new_pid_raw = ptrace::getevent(pid)
                .context("Failed to get event message for fork/vfork/clone")?;
            let new_pid = Pid::from_raw(new_pid_raw as i32);

            // Wait for the new child to stop
            let wait_status = waitpid(new_pid, None).context("Failed to wait for new child")?;

            // Check if child is still alive and can be continued
            match wait_status {
                WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _) => {
                    // Child already exited, nothing to continue
                    eprintln!("[renacer: Process {pid} forked child {new_pid} (already exited)]");
                }
                _ => {
                    // Setup ptrace options for the new child (already waited)
                    if let Err(e) =
                        setup_ptrace_options_internal(new_pid, config.follow_forks, false)
                    {
                        // Child may have exited between waitpid and setoptions
                        warn!(
                            "Failed to setup ptrace options for child {}: {}",
                            new_pid, e
                        );
                        return Ok(());
                    }

                    // Add to tracking
                    processes.insert(new_pid, ProcessState::new());

                    // Continue the new child process
                    // Handle ESRCH gracefully - child may have exited between waitpid and syscall
                    match ptrace::syscall(new_pid, None) {
                        Ok(()) => {
                            eprintln!("[renacer: Process {pid} forked child {new_pid}]");
                        }
                        Err(nix::errno::Errno::ESRCH) => {
                            // Child already exited, remove from tracking
                            processes.remove(&new_pid);
                            eprintln!(
                                "[renacer: Process {pid} forked child {new_pid} (exited immediately)]"
                            );
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!("Failed to continue new child: {e}"));
                        }
                    }
                }
            }
        }
        _ => {
            // Unknown ptrace event, ignore
        }
    }

    Ok(())
}

/// Handle syscall event (entry or exit)
fn handle_syscall_event(
    child: Pid,
    in_syscall: &mut bool,
    current_syscall_entry: &mut Option<SyscallEntry>,
    syscall_entry_time: &mut Option<std::time::Instant>,
    dwarf_ctx: Option<&crate::dwarf::DwarfContext>,
    config: &TracerConfig,
    tracers: &mut Tracers,
) -> Result<()> {
    // Check if we're in a structured output mode (JSON, CSV, HTML) to suppress text output
    let in_json_mode = tracers.json_output.is_some();
    let in_csv_mode = tracers.csv_output.is_some() || tracers.csv_stats_output.is_some();
    let in_html_mode = tracers.html_output.is_some();
    let structured_output = in_json_mode || in_csv_mode || in_html_mode;

    if *in_syscall {
        // Syscall exit - calculate duration
        let duration_us = syscall_entry_time
            .map(|start| start.elapsed().as_micros() as u64)
            .unwrap_or(0);

        process_syscall_exit(
            child,
            current_syscall_entry,
            tracers,
            config.timing_mode,
            duration_us,
        )?;

        *current_syscall_entry = None;
        *syscall_entry_time = None;
        *in_syscall = false;
    } else {
        // Syscall entry - record start time if timing enabled
        if config.timing_mode || config.statistics_mode || structured_output {
            *syscall_entry_time = Some(std::time::Instant::now());
        }

        *current_syscall_entry = process_syscall_entry(
            child,
            dwarf_ctx,
            config,
            tracers.profiling_ctx.as_mut(),
            structured_output,
        )?;
        *in_syscall = true;
    }
    Ok(())
}

/// Process syscall entry event
fn process_syscall_entry(
    child: Pid,
    dwarf_ctx: Option<&crate::dwarf::DwarfContext>,
    config: &TracerConfig,
    profiling_ctx: Option<&mut crate::profiling::ProfilingContext>,
    structured_output: bool,
) -> Result<Option<SyscallEntry>> {
    if let Some(prof) = profiling_ctx {
        prof.measure(crate::profiling::ProfilingCategory::Other, || {
            handle_syscall_entry(
                child,
                dwarf_ctx,
                &config.filter,
                config.statistics_mode,
                structured_output,
                config.function_time,
                config.transpiler_map.as_ref(),
            )
        })
    } else {
        handle_syscall_entry(
            child,
            dwarf_ctx,
            &config.filter,
            config.statistics_mode,
            structured_output,
            config.function_time,
            config.transpiler_map.as_ref(),
        )
    }
}

/// Process syscall exit event
fn process_syscall_exit(
    child: Pid,
    current_syscall_entry: &Option<SyscallEntry>,
    tracers: &mut Tracers,
    timing_mode: bool,
    duration_us: u64,
) -> Result<()> {
    // Check if profiling is enabled and handle accordingly
    if let Some(mut prof) = tracers.profiling_ctx.take() {
        // Temporarily take profiling_ctx out to avoid borrow conflict
        let result = prof.measure(crate::profiling::ProfilingCategory::Other, || {
            handle_syscall_exit(
                child,
                current_syscall_entry,
                tracers,
                timing_mode,
                duration_us,
            )
        });
        prof.record_syscall();
        // Put profiling_ctx back
        tracers.profiling_ctx = Some(prof);
        result
    } else {
        handle_syscall_exit(
            child,
            current_syscall_entry,
            tracers,
            timing_mode,
            duration_us,
        )
    }
}

/// Print text statistics summary
///
/// Sprint 32: Now accepts optional `OtlpExporter` for compute block tracing
fn print_text_stats(
    stats_tracker: &Option<crate::stats::StatsTracker>,
    stats_extended: bool,
    anomaly_threshold: f32,
    #[cfg(feature = "otlp")] otlp_exporter: Option<&crate::otlp_exporter::OtlpExporter>,
    #[cfg(not(feature = "otlp"))] _otlp_exporter: Option<&()>,
) {
    if let Some(ref tracker) = stats_tracker {
        tracker.print_summary();
        if stats_extended {
            #[cfg(feature = "otlp")]
            tracker.print_extended_summary(anomaly_threshold, otlp_exporter);
            #[cfg(not(feature = "otlp"))]
            tracker.print_extended_summary(anomaly_threshold, _otlp_exporter);
        }
    }
}

/// Print JSON output
fn print_json_output(mut output: crate::json_output::JsonOutput, exit_code: i32) {
    output.set_exit_code(exit_code);
    match output.to_json() {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("Failed to serialize JSON: {e}"),
    }
}

/// Generate and populate ML analysis for JSON output
fn generate_ml_analysis_for_json(
    stats_tracker: &Option<crate::stats::StatsTracker>,
    ml_clusters: usize,
) -> Option<crate::ml_anomaly::MlAnomalyReport> {
    if let Some(ref tracker) = stats_tracker {
        let mut ml_data = std::collections::HashMap::new();
        for (syscall_name, stats) in tracker.stats_map() {
            let total_time_ns = stats.total_time_us * 1000;
            ml_data.insert(syscall_name.clone(), (stats.count, total_time_ns));
        }
        let analyzer = crate::ml_anomaly::MlAnomalyAnalyzer::new(ml_clusters);
        Some(analyzer.analyze(&ml_data))
    } else {
        None
    }
}

/// Generate Isolation Forest analysis for JSON output (Sprint 22)
fn generate_isolation_forest_analysis_for_json(
    stats_tracker: &Option<crate::stats::StatsTracker>,
    num_trees: usize,
    contamination: f32,
    explain: bool,
) -> Option<crate::isolation_forest::OutlierReport> {
    if let Some(ref tracker) = stats_tracker {
        let mut data = std::collections::HashMap::new();
        for (syscall_name, stats) in tracker.stats_map() {
            let total_time_ns = stats.total_time_us * 1000;
            data.insert(syscall_name.clone(), (stats.count, total_time_ns));
        }
        Some(crate::isolation_forest::analyze_outliers(
            &data,
            num_trees,
            contamination,
            explain,
        ))
    } else {
        None
    }
}

/// Generate Autoencoder analysis for JSON output (Sprint 23)
fn generate_autoencoder_analysis_for_json(
    stats_tracker: &Option<crate::stats::StatsTracker>,
    hidden_size: usize,
    epochs: usize,
    threshold: f64,
    explain: bool,
) -> Option<crate::autoencoder::AutoencoderReport> {
    if let Some(ref tracker) = stats_tracker {
        let mut data = std::collections::HashMap::new();
        for (syscall_name, stats) in tracker.stats_map() {
            let total_time_ns = stats.total_time_us * 1000;
            data.insert(syscall_name.clone(), (stats.count, total_time_ns));
        }
        Some(crate::autoencoder::analyze_anomalies(
            &data,
            hidden_size,
            epochs,
            threshold,
            explain,
        ))
    } else {
        None
    }
}

/// Print CSV statistics output
fn print_csv_stats(
    mut csv_stats: crate::csv_output::CsvStatsOutput,
    stats_tracker: &Option<crate::stats::StatsTracker>,
    timing_mode: bool,
    stats_extended: bool,
    anomaly_threshold: f32,
) {
    if let Some(ref tracker) = stats_tracker {
        for (syscall_name, stats) in tracker.stats_map() {
            let total_time_us = if timing_mode {
                Some(stats.total_time_us)
            } else {
                None
            };
            csv_stats.add_stat(crate::csv_output::CsvStat {
                syscall: syscall_name.clone(),
                calls: stats.count,
                errors: stats.errors,
                total_time_us,
            });
        }
        if stats_extended {
            // Note: CSV output doesn't get OTLP tracing - pass None
            #[cfg(feature = "otlp")]
            tracker.print_extended_summary(anomaly_threshold, None);
            #[cfg(not(feature = "otlp"))]
            tracker.print_extended_summary(anomaly_threshold, None);
        }
    }
    print!("{}", csv_stats.to_csv(timing_mode));
}

/// Print HPU analysis report
fn print_hpu_analysis(stats_tracker: &Option<crate::stats::StatsTracker>, hpu_cpu_only: bool) {
    if let Some(ref tracker) = stats_tracker {
        let mut hpu_data = std::collections::HashMap::new();
        for (syscall_name, stats) in tracker.stats_map() {
            let total_time_ns = stats.total_time_us * 1000;
            hpu_data.insert(syscall_name.clone(), (stats.count, total_time_ns));
        }
        let profiler = crate::hpu::HPUProfiler::new(hpu_cpu_only);
        let report = profiler.analyze(&hpu_data);
        print!("{}", report.format());
    }
}

/// Print ML anomaly analysis report (Sprint 23)
fn print_ml_analysis(
    stats_tracker: &Option<crate::stats::StatsTracker>,
    ml_clusters: usize,
    ml_compare: bool,
    anomaly_threshold: f32,
) {
    if let Some(ref tracker) = stats_tracker {
        let mut ml_data = std::collections::HashMap::new();
        for (syscall_name, stats) in tracker.stats_map() {
            let total_time_ns = stats.total_time_us * 1000;
            ml_data.insert(syscall_name.clone(), (stats.count, total_time_ns));
        }
        let analyzer = crate::ml_anomaly::MlAnomalyAnalyzer::new(ml_clusters);
        let report = analyzer.analyze(&ml_data);

        if ml_compare {
            // Compare with z-score anomaly detection
            let mut zscore_anomalies = Vec::new();
            for syscall_name in tracker.stats_map().keys() {
                // Note: Pass None - this is for comparison, not primary compute
                #[cfg(feature = "otlp")]
                let extended = tracker.calculate_extended_statistics(syscall_name, None);
                #[cfg(not(feature = "otlp"))]
                let extended = tracker.calculate_extended_statistics(syscall_name, None);

                if let Some(extended) = extended {
                    if extended.stddev > 0.0 {
                        let z_score = (extended.max - extended.mean) / extended.stddev;
                        if z_score > anomaly_threshold {
                            zscore_anomalies.push((syscall_name.clone(), f64::from(z_score)));
                        }
                    }
                }
            }
            eprint!("{}", report.format_comparison(&zscore_anomalies));
        } else {
            eprint!("{}", report.format());
        }
    }
}

/// Print Isolation Forest outlier analysis report (Sprint 22)
fn print_isolation_forest_analysis(
    stats_tracker: &Option<crate::stats::StatsTracker>,
    num_trees: usize,
    contamination: f32,
    explain: bool,
) {
    if let Some(ref tracker) = stats_tracker {
        let mut data = std::collections::HashMap::new();
        for (syscall_name, stats) in tracker.stats_map() {
            let total_time_ns = stats.total_time_us * 1000;
            data.insert(syscall_name.clone(), (stats.count, total_time_ns));
        }

        let report =
            crate::isolation_forest::analyze_outliers(&data, num_trees, contamination, explain);

        // Print report
        eprintln!("\n=== Isolation Forest Anomaly Detection ===");
        eprintln!(
            "Trees: {}, Contamination: {:.1}%, Samples: {}\n",
            report.num_trees,
            report.contamination * 100.0,
            report.total_samples
        );

        if report.outliers.is_empty() {
            eprintln!("No outliers detected.");
        } else {
            eprintln!("Detected {} outlier(s):\n", report.outliers.len());
            for outlier in &report.outliers {
                eprintln!(
                    "  {} (anomaly score: {:.3})",
                    outlier.syscall, outlier.anomaly_score
                );
                eprintln!(
                    "    Avg duration: {:.2} μs, Calls: {}",
                    outlier.avg_duration_us, outlier.call_count
                );

                if explain && !outlier.feature_importance.is_empty() {
                    eprintln!("    Feature Importance:");
                    for (feature, importance) in &outlier.feature_importance {
                        eprintln!("      {feature}: {importance:.1}%");
                    }
                }
                eprintln!();
            }
        }
        eprintln!("=========================================\n");
    }
}

/// Print Autoencoder anomaly detection report (Sprint 23)
fn print_autoencoder_analysis(
    stats_tracker: &Option<crate::stats::StatsTracker>,
    hidden_size: usize,
    epochs: usize,
    threshold: f32,
    explain: bool,
) {
    if let Some(ref tracker) = stats_tracker {
        let mut data = std::collections::HashMap::new();
        for (syscall_name, stats) in tracker.stats_map() {
            let total_time_ns = stats.total_time_us * 1000;
            data.insert(syscall_name.clone(), (stats.count, total_time_ns));
        }

        let report = crate::autoencoder::analyze_anomalies(
            &data,
            hidden_size,
            epochs,
            f64::from(threshold),
            explain,
        );

        // Print report
        eprintln!("\n=== Autoencoder Anomaly Detection ===");
        eprintln!(
            "Hidden Size: {}, Epochs: {}, Threshold: {:.2}σ",
            report.hidden_size, report.epochs, threshold
        );
        eprintln!(
            "Samples: {}, Adaptive Threshold: {:.4}\n",
            report.total_samples, report.threshold
        );

        if report.anomalies.is_empty() {
            eprintln!("No anomalies detected.");
        } else {
            eprintln!("Detected {} anomal(y/ies):\n", report.anomalies.len());
            for anomaly in &report.anomalies {
                eprintln!(
                    "  {} (reconstruction error: {:.4})",
                    anomaly.syscall, anomaly.reconstruction_error
                );
                eprintln!(
                    "    Avg duration: {:.2} μs, Calls: {}",
                    anomaly.avg_duration_us, anomaly.call_count
                );

                if explain && !anomaly.feature_contributions.is_empty() {
                    eprintln!("    Feature Contributions to Error:");
                    for (feature, contribution) in &anomaly.feature_contributions {
                        eprintln!("      {feature}: {contribution:.1}%");
                    }
                }
                eprintln!();
            }
        }
        eprintln!("======================================\n");
    }
}

/// Analysis configuration for `print_summaries`
struct AnalysisConfig {
    stats_extended: bool,
    anomaly_threshold: f32,
    hpu_analysis: bool,
    hpu_cpu_only: bool,
    ml_anomaly: bool,
    ml_clusters: usize,
    ml_compare: bool,
    ml_outliers: bool,         // Sprint 22: Isolation Forest outlier detection
    ml_outlier_threshold: f32, // Sprint 22: Contamination threshold
    ml_outlier_trees: usize,   // Sprint 22: Number of trees
    dl_anomaly: bool,          // Sprint 23: Deep Learning Autoencoder anomaly detection
    dl_threshold: f32,         // Sprint 23: Reconstruction error threshold (σ multiplier)
    dl_hidden_size: usize,     // Sprint 23: Hidden layer size
    dl_epochs: usize,          // Sprint 23: Training epochs
    explain: bool,             // Sprint 22/23: Enable explainability
}

/// Print optional profiling and tracing summaries
fn print_optional_summaries(
    profiling_ctx: Option<crate::profiling::ProfilingContext>,
    function_profiler: Option<crate::function_profiler::FunctionProfiler>,
    anomaly_detector: Option<crate::anomaly::AnomalyDetector>,
) {
    if let Some(ctx) = profiling_ctx {
        ctx.print_summary();
    }
    if let Some(profiler) = function_profiler {
        profiler.print_summary();
    }
    if let Some(detector) = anomaly_detector {
        detector.print_summary();
    }
}
/// Sprint 26: Print decision trace summary
fn print_decision_trace_summary(decision_tracer: Option<crate::decision_trace::DecisionTracer>) {
    if let Some(tracer) = decision_tracer {
        if tracer.count() == 0 {
            return;
        }

        // Sprint 27 Phase 3: Write to memory-mapped file (.ruchy/decisions.msgpack)
        let mmap_path = std::path::Path::new(".ruchy/decisions.msgpack");
        let manifest_path = std::path::Path::new(".ruchy/decision_manifest.json");

        // Write MessagePack file
        match tracer.write_to_msgpack(mmap_path) {
            Ok(()) => {
                println!("\n✅ Decision traces written to: {}", mmap_path.display());
            }
            Err(e) => {
                eprintln!(
                    "⚠️  Failed to write decision traces to {}: {}",
                    mmap_path.display(),
                    e
                );
            }
        }

        // Write manifest file
        match tracer.write_manifest(
            manifest_path,
            "2.0.0",
            None, // git_commit (could add via git2 crate)
            Some(env!("CARGO_PKG_VERSION")),
        ) {
            Ok(()) => {
                println!(
                    "✅ Decision manifest written to: {}",
                    manifest_path.display()
                );
            }
            Err(e) => {
                eprintln!(
                    "⚠️  Failed to write decision manifest to {}: {}",
                    manifest_path.display(),
                    e
                );
            }
        }

        // Also print summary to stdout for convenience
        println!("\n=== Transpiler Decision Traces ===\n");

        for trace in tracer.traces() {
            // Format: category::name with input and result
            print!("[{}::{}] ", trace.category, trace.name);

            // Print input (compact JSON)
            print!("input={}", trace.input);

            // Print result if available
            if let Some(ref result) = trace.result {
                print!(" result={result}");
            }

            // Print decision_id if available (Sprint 27)
            if let Some(decision_id) = trace.decision_id {
                print!(" id=0x{decision_id:X}");
            }

            println!();
        }

        println!("\nTotal decision traces: {}", tracer.count());
        println!("Decision manifest: {}", manifest_path.display());
        println!("Binary traces: {}", mmap_path.display());
    }
}

/// Print analysis summaries (HPU, ML, Isolation Forest, Autoencoder)
fn print_analysis_summaries(
    stats_tracker: &Option<crate::stats::StatsTracker>,
    analysis: &AnalysisConfig,
) {
    if analysis.hpu_analysis {
        print_hpu_analysis(stats_tracker, analysis.hpu_cpu_only);
    }
    if analysis.ml_anomaly {
        print_ml_analysis(
            stats_tracker,
            analysis.ml_clusters,
            analysis.ml_compare,
            analysis.anomaly_threshold,
        );
    }
    if analysis.ml_outliers {
        print_isolation_forest_analysis(
            stats_tracker,
            analysis.ml_outlier_trees,
            analysis.ml_outlier_threshold,
            analysis.explain,
        );
    }
    if analysis.dl_anomaly {
        print_autoencoder_analysis(
            stats_tracker,
            analysis.dl_hidden_size,
            analysis.dl_epochs,
            analysis.dl_threshold,
            analysis.explain,
        );
    }
}

/// Handle JSON output with ML analysis additions
fn handle_json_output(
    mut output: crate::json_output::JsonOutput,
    stats_tracker: &Option<crate::stats::StatsTracker>,
    analysis: &AnalysisConfig,
    exit_code: i32,
) {
    if analysis.ml_anomaly {
        if let Some(report) = generate_ml_analysis_for_json(stats_tracker, analysis.ml_clusters) {
            output.set_ml_analysis(report);
        }
    }
    if analysis.ml_outliers {
        if let Some(report) = generate_isolation_forest_analysis_for_json(
            stats_tracker,
            analysis.ml_outlier_trees,
            analysis.ml_outlier_threshold,
            analysis.explain,
        ) {
            output.set_isolation_forest_analysis(report, analysis.explain);
        }
    }
    if analysis.dl_anomaly {
        if let Some(report) = generate_autoencoder_analysis_for_json(
            stats_tracker,
            analysis.dl_hidden_size,
            analysis.dl_epochs,
            f64::from(analysis.dl_threshold),
            analysis.explain,
        ) {
            output.set_autoencoder_analysis(report, analysis.dl_threshold, analysis.explain);
        }
    }
    print_json_output(output, exit_code);
}

/// Print all summaries at end of tracing
fn print_summaries(tracers: Tracers, timing_mode: bool, exit_code: i32, analysis: &AnalysisConfig) {
    let Tracers {
        stats_tracker,
        json_output,
        csv_output,
        csv_stats_output,
        html_output,
        profiling_ctx,
        function_profiler,
        anomaly_detector,
        decision_tracer,
        #[cfg(feature = "otlp")]
        mut otlp_exporter,
    } = tracers;

    // Sprint 31: Export decision traces to OTLP (before ending root span)
    #[cfg(feature = "otlp")]
    if let (Some(ref mut exporter), Some(ref tracer)) = (&mut otlp_exporter, &decision_tracer) {
        for trace in tracer.traces() {
            exporter.record_decision(
                &trace.category,
                &trace.name,
                trace.result.as_ref().and_then(|v| v.as_str()),
                trace.timestamp_us,
            );
        }
    }

    // Print statistics summary if in statistics mode (text format)
    if stats_tracker.is_some() && csv_stats_output.is_none() {
        #[cfg(feature = "otlp")]
        print_text_stats(
            &stats_tracker,
            analysis.stats_extended,
            analysis.anomaly_threshold,
            otlp_exporter.as_ref(),
        );
        #[cfg(not(feature = "otlp"))]
        print_text_stats(
            &stats_tracker,
            analysis.stats_extended,
            analysis.anomaly_threshold,
            None,
        );
    }

    // Sprint 30: End root span and shutdown OTLP exporter
    #[cfg(feature = "otlp")]
    if let Some(ref mut exporter) = otlp_exporter {
        exporter.end_root_span(exit_code);
        exporter.shutdown();
    }

    // Handle output formats
    if let Some(output) = json_output {
        handle_json_output(output, &stats_tracker, analysis, exit_code);
    }
    if let Some(output) = csv_output {
        print!("{}", output.to_csv());
    }
    if let Some(csv_stats) = csv_stats_output {
        print_csv_stats(
            csv_stats,
            &stats_tracker,
            timing_mode,
            analysis.stats_extended,
            analysis.anomaly_threshold,
        );
    }
    if let Some(output) = html_output {
        print!("{}", output.to_html(stats_tracker.as_ref()));
    }

    // Print profiling and tracing summaries
    print_optional_summaries(profiling_ctx, function_profiler, anomaly_detector);
    print_analysis_summaries(&stats_tracker, analysis);
    print_decision_trace_summary(decision_tracer);
}

/// Per-process state for multi-process tracing
#[derive(Debug)]
struct ProcessState {
    in_syscall: bool,
    current_syscall_entry: Option<SyscallEntry>,
    syscall_entry_time: Option<std::time::Instant>,
    dwarf_ctx: Option<crate::dwarf::DwarfContext>,
    dwarf_loaded: bool,
}

impl ProcessState {
    fn new() -> Self {
        Self {
            in_syscall: false,
            current_syscall_entry: None,
            syscall_entry_time: None,
            dwarf_ctx: None,
            dwarf_loaded: false,
        }
    }
}

/// Handle wait status and update process tracking
fn handle_traced_process_status(
    status: WaitStatus,
    processes: &mut std::collections::HashMap<Pid, ProcessState>,
    main_pid: Pid,
    main_exit_code: &mut i32,
    config: &TracerConfig,
) -> Result<Option<Pid>> {
    match status {
        WaitStatus::Exited(p, code) => {
            processes.remove(&p);
            if p == main_pid {
                *main_exit_code = code;
            }
            Ok(None)
        }
        WaitStatus::Signaled(p, sig, _) => {
            eprintln!("Process {p} killed by signal: {sig:?}");
            processes.remove(&p);
            if p == main_pid {
                *main_exit_code = 128 + sig as i32;
            }
            Ok(None)
        }
        WaitStatus::PtraceSyscall(p) => Ok(Some(p)),
        WaitStatus::PtraceEvent(p, _sig, event) => {
            handle_ptrace_event(p, event, processes, config)?;
            ptrace::syscall(p, None).context("Failed to PTRACE_SYSCALL after event")?;
            Ok(None)
        }
        _ => {
            if let Some(p) = status.pid() {
                ptrace::syscall(p, None).ok();
            }
            Ok(None)
        }
    }
}

/// Process a single syscall event for a traced PID
fn process_syscall_for_pid(
    pid: Pid,
    processes: &mut std::collections::HashMap<Pid, ProcessState>,
    config: &TracerConfig,
    tracers: &mut Tracers,
) -> Result<()> {
    let state = if let Some(s) = processes.get_mut(&pid) {
        s
    } else {
        ptrace::syscall(pid, None).ok();
        return Ok(());
    };

    // Load DWARF context on first syscall if needed
    if config.enable_source && !state.dwarf_loaded {
        state.dwarf_loaded = true;
        state.dwarf_ctx = load_dwarf_context(pid);
    }

    // Handle syscall entry/exit
    handle_syscall_event(
        pid,
        &mut state.in_syscall,
        &mut state.current_syscall_entry,
        &mut state.syscall_entry_time,
        state.dwarf_ctx.as_ref(),
        config,
        tracers,
    )?;

    ptrace::syscall(pid, None).context("Failed to PTRACE_SYSCALL")
}

/// Trace a child process, filtering syscalls based on filter
fn trace_child(child: Pid, config: TracerConfig) -> Result<i32> {
    info!(pid = %child, "starting trace_child");

    let mut tracers = initialize_tracers(&config);
    trace!("tracers initialized");

    // Sprint 30: Start root span for OTLP exporter
    #[cfg(feature = "otlp")]
    if let Some(ref mut exporter) = tracers.otlp_exporter {
        // Get program name from /proc/{pid}/cmdline
        let program_name = std::fs::read_to_string(format!("/proc/{child}/cmdline"))
            .ok()
            .and_then(|s| s.split('\0').next().map(std::string::ToString::to_string))
            .unwrap_or_else(|| format!("pid:{child}"));

        exporter.start_root_span(&program_name, child.as_raw());
    }

    trace!("calling setup_ptrace_options");
    setup_ptrace_options(child, config.follow_forks)?;
    trace!("ptrace options set successfully");

    use std::collections::HashMap;
    let mut processes: HashMap<Pid, ProcessState> = HashMap::new();
    processes.insert(child, ProcessState::new());

    let main_pid = child;
    let mut main_exit_code = 0;

    info!("entering main wait loop");
    while !processes.is_empty() {
        trace!(num_processes = processes.len(), "calling waitpid");
        let wait_result = if config.follow_forks {
            waitpid(Pid::from_raw(-1), None)
        } else {
            waitpid(child, None)
        };

        let status = match wait_result {
            Ok(s) => {
                trace!(status = ?s, "waitpid returned");
                s
            }
            Err(_) if processes.is_empty() => {
                trace!("waitpid error but processes empty, breaking");
                break;
            }
            Err(e) => {
                warn!(error = %e, "waitpid failed");
                return Err(e).context("Failed to waitpid");
            }
        };

        let pid = if let Some(p) = handle_traced_process_status(
            status,
            &mut processes,
            main_pid,
            &mut main_exit_code,
            &config,
        )? {
            trace!(pid = %p, "handle_traced_process_status returned pid");
            p
        } else {
            trace!("handle_traced_process_status returned None, continuing");
            continue;
        };

        trace!(pid = %pid, "calling process_syscall_for_pid");
        process_syscall_for_pid(pid, &mut processes, &config, &mut tracers)?;
        trace!(pid = %pid, "process_syscall_for_pid completed");
    }

    info!("exited main wait loop");

    print_summaries(
        tracers,
        config.timing_mode,
        main_exit_code,
        &AnalysisConfig {
            stats_extended: config.stats_extended,
            anomaly_threshold: config.anomaly_threshold,
            hpu_analysis: config.hpu_analysis,
            hpu_cpu_only: config.hpu_cpu_only,
            ml_anomaly: config.ml_anomaly,
            ml_clusters: config.ml_clusters,
            ml_compare: config.ml_compare,
            ml_outliers: config.ml_outliers, // Sprint 22
            ml_outlier_threshold: config.ml_outlier_threshold, // Sprint 22
            ml_outlier_trees: config.ml_outlier_trees, // Sprint 22
            dl_anomaly: config.dl_anomaly,   // Sprint 23
            dl_threshold: config.dl_threshold, // Sprint 23
            dl_hidden_size: config.dl_hidden_size, // Sprint 23
            dl_epochs: config.dl_epochs,     // Sprint 23
            explain: config.explain,         // Sprint 22/23
        },
    );
    Ok(main_exit_code)
}

/// Syscall entry data for JSON output
#[derive(Debug)]
struct SyscallEntry {
    name: String,
    args: Vec<String>,
    source: Option<crate::json_output::JsonSourceLocation>,
    function_name: Option<String>,
    caller_name: Option<String>,
    // Sprint 26: Raw args for decision trace capture (write syscall interception)
    raw_arg1: Option<u64>,
    raw_arg2: Option<u64>,
    #[allow(dead_code)] // May be used in future for more complex decision trace patterns
    raw_arg3: Option<u64>,
}

/// Find the user function that triggered a syscall by unwinding the stack
///
/// Syscalls execute in libc, not user code. To attribute syscalls to the
/// user function that triggered them, we need to unwind the stack and find
/// the first non-libc function.
///
/// Returns the function name if found, None otherwise.
#[allow(dead_code)] // Reserved for future use (available as helper function)
fn find_user_function_via_unwinding(
    child: Pid,
    dwarf_ctx: &crate::dwarf::DwarfContext,
) -> Option<String> {
    find_user_function_with_caller(child, dwarf_ctx).map(|(func, _)| func)
}

/// Find user function and its caller from stack unwinding
/// Returns (`current_function`, `caller_function`)
fn find_user_function_with_caller(
    child: Pid,
    dwarf_ctx: &crate::dwarf::DwarfContext,
) -> Option<(String, Option<String>)> {
    // Unwind the stack to get all frames
    let frames = match crate::stack_unwind::unwind_stack(child) {
        Ok(frames) => frames,
        Err(_) => return None, // Stack unwinding failed
    };

    let mut user_functions = Vec::new();

    // Walk through frames and collect user functions
    for frame in frames {
        // Look up this address in DWARF
        if let Some(source_info) = dwarf_ctx.lookup(frame.rip).ok().flatten() {
            if let Some(func_name) = source_info.function {
                // Filter out libc/system functions
                let is_libc = func_name.starts_with("__")
                    || func_name.contains("libc")
                    || func_name.contains("@plt")
                    || func_name.contains("@@GLIBC");

                if !is_libc {
                    user_functions.push(func_name.clone());
                }
            }
        }
    }

    // Return the first user function and its caller (if available)
    match user_functions.len() {
        0 => None,
        1 => Some((user_functions[0].clone(), None)),
        _ => Some((user_functions[0].clone(), Some(user_functions[1].clone()))),
    }
}

/// Format syscall arguments for JSON output
fn format_syscall_args_for_json(
    child: Pid,
    name: &str,
    arg1: u64,
    arg2: u64,
    arg3: u64,
) -> Vec<String> {
    match name {
        "openat" => {
            let filename =
                read_string(child, arg2 as usize).unwrap_or_else(|_| format!("{arg2:#x}"));
            vec![
                format!("{:#x}", arg1),
                format!("\"{}\"", filename),
                format!("{:#x}", arg3),
            ]
        }
        _ => vec![
            format!("{:#x}", arg1),
            format!("{:#x}", arg2),
            format!("{:#x}", arg3),
        ],
    }
}

/// Print syscall entry with optional source location
#[allow(clippy::too_many_arguments)]
fn print_syscall_entry(
    child: Pid,
    name: &str,
    syscall_num: i64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    source_info: &Option<crate::dwarf::SourceLocation>,
    transpiler_map: Option<&crate::transpiler_map::TranspilerMap>,
) {
    // Print source location if available
    if let Some(src) = source_info {
        // Try to map to transpiler source first
        if let Some(transpiled_source) = map_to_transpiler_source(src, transpiler_map) {
            // Show both Rust and original source
            print!("{transpiled_source} ");
        } else {
            // Show just Rust source from DWARF
            print!("{}:{} ", src.file, src.line);
            if let Some(func) = &src.function {
                print!("{func} ");
            }
        }
    }

    // Print syscall with arguments
    match name {
        "openat" => {
            let filename =
                read_string(child, arg2 as usize).unwrap_or_else(|_| format!("{arg2:#x}"));
            print!("{name}({arg1:#x}, \"{filename}\", {arg3:#x}) = ");
        }
        "unknown" => {
            print!("syscall_{syscall_num}({arg1:#x}, {arg2:#x}, {arg3:#x}) = ");
        }
        _ => {
            print!("{name}({arg1:#x}, {arg2:#x}, {arg3:#x}) = ");
        }
    }
    std::io::Write::flush(&mut std::io::stdout()).ok();
}

/// Extract function name and caller from DWARF context
fn extract_function_names(
    child: Pid,
    dwarf_ctx: Option<&crate::dwarf::DwarfContext>,
    source_info: &Option<crate::dwarf::SourceLocation>,
    function_profiling_enabled: bool,
) -> (Option<String>, Option<String>) {
    if function_profiling_enabled {
        if let Some(ctx) = dwarf_ctx {
            find_user_function_with_caller(child, ctx)
                .map_or((None, None), |(func, caller)| (Some(func), caller))
        } else {
            let func = source_info.as_ref().and_then(|src| src.function.clone());
            (func, None)
        }
    } else {
        let func = source_info.as_ref().and_then(|src| src.function.clone());
        (func, None)
    }
}

/// Handle syscall entry - record syscall number and arguments
/// Returns the syscall entry data if it should be traced, None otherwise
fn handle_syscall_entry(
    child: Pid,
    dwarf_ctx: Option<&crate::dwarf::DwarfContext>,
    filter: &crate::filter::SyscallFilter,
    statistics_mode: bool,
    structured_output: bool,
    function_profiling_enabled: bool,
    transpiler_map: Option<&crate::transpiler_map::TranspilerMap>,
) -> Result<Option<SyscallEntry>> {
    let regs = ptrace::getregs(child).context("Failed to get registers")?;

    // On x86_64: syscall number in orig_rax
    let syscall_num = regs.orig_rax as i64;

    // Get syscall name
    let name = syscalls::syscall_name(syscall_num);

    // Sprint 9-10: Filter syscalls based on -e trace= expression
    if !filter.should_trace(name) {
        // Don't print or track this syscall
        return Ok(None);
    }

    // Arguments in rdi, rsi, rdx, r10, r8, r9 for x86_64
    let arg1 = regs.rdi;
    let arg2 = regs.rsi;
    let arg3 = regs.rdx;

    // Sprint 5-6: Look up source location using instruction pointer if DWARF is available
    let source_info = if let Some(ctx) = dwarf_ctx {
        let ip = regs.rip;
        ctx.lookup(ip).ok().flatten()
    } else {
        None
    };

    // Format arguments for structured output modes (JSON, CSV, HTML) if needed
    let args = if structured_output {
        format_syscall_args_for_json(child, name, arg1, arg2, arg3)
    } else {
        Vec::new()
    };

    // Print syscall entry if not in statistics or structured output mode
    if !statistics_mode && !structured_output {
        print_syscall_entry(
            child,
            name,
            syscall_num,
            arg1,
            arg2,
            arg3,
            &source_info,
            transpiler_map,
        );
    }

    // Extract function names for profiling
    let (function_name, caller_name) =
        extract_function_names(child, dwarf_ctx, &source_info, function_profiling_enabled);

    let json_source = source_info
        .as_ref()
        .map(|src| crate::json_output::JsonSourceLocation {
            file: src.file.clone(),
            line: src.line,
            function: src.function.clone(),
        });

    // Return syscall entry data
    Ok(Some(SyscallEntry {
        name: name.to_string(),
        args,
        source: json_source,
        function_name,
        caller_name,
        // Sprint 26: Store raw args for decision trace capture
        raw_arg1: Some(arg1),
        raw_arg2: Some(arg2),
        raw_arg3: Some(arg3),
    }))
}

/// Map DWARF source location to transpiler source (Sprint 24-28)
/// Returns a formatted string with the original source location if mapping is available
fn map_to_transpiler_source(
    dwarf_source: &crate::dwarf::SourceLocation,
    transpiler_map: Option<&crate::transpiler_map::TranspilerMap>,
) -> Option<String> {
    if let Some(map) = transpiler_map {
        // Extract line number from DWARF source location
        let rust_line = dwarf_source.line as usize;

        // Look up in transpiler map
        if let Some(mapping) = map.lookup_line(rust_line) {
            // Format as "python_file:line in python_function"
            return Some(format!(
                "{}:{} in {} [{}]",
                map.source_file().display(),
                mapping.python_line,
                mapping.python_function,
                map.source_language()
            ));
        }
    }
    None
}

/// Read a null-terminated string from the tracee's memory
fn read_string(child: Pid, addr: usize) -> Result<String> {
    use nix::sys::uio::{process_vm_readv, RemoteIoVec};
    use std::io::IoSliceMut;

    // Read up to 4096 bytes (max path length)
    let mut buf = vec![0u8; 4096];
    let mut local_iov = [IoSliceMut::new(&mut buf)];
    let remote_iov = [RemoteIoVec {
        base: addr,
        len: 4096,
    }];

    let bytes_read = process_vm_readv(child, &mut local_iov, &remote_iov)
        .context("Failed to read string from tracee memory")?;

    if bytes_read == 0 {
        anyhow::bail!("Read 0 bytes from tracee");
    }

    // Find null terminator
    let null_pos = buf.iter().position(|&b| b == 0).unwrap_or(bytes_read);

    // Convert to UTF-8 string (lossy - invalid UTF-8 will be replaced with �)
    Ok(String::from_utf8_lossy(&buf[..null_pos]).to_string())
}

/// Record statistics for a syscall
fn record_stats_for_syscall(
    syscall_entry: &Option<SyscallEntry>,
    stats_tracker: Option<&mut crate::stats::StatsTracker>,
    result: i64,
    duration_us: u64,
) {
    if let (Some(entry), Some(tracker)) = (syscall_entry, stats_tracker) {
        tracker.record(&entry.name, result, duration_us);
    }
}

/// Record JSON output for a syscall
fn record_json_for_syscall(
    syscall_entry: &Option<SyscallEntry>,
    json_output: Option<&mut crate::json_output::JsonOutput>,
    result: i64,
    timing_mode: bool,
    duration_us: u64,
) {
    if let (Some(entry), Some(output)) = (syscall_entry, json_output) {
        let duration = if timing_mode && duration_us > 0 {
            Some(duration_us)
        } else {
            None
        };

        output.add_syscall(crate::json_output::JsonSyscall {
            name: entry.name.clone(),
            args: entry.args.clone(),
            result,
            duration_us: duration,
            source: entry.source.clone(),
        });
    }
}

/// Record CSV output for a syscall
fn record_csv_for_syscall(
    syscall_entry: &Option<SyscallEntry>,
    csv_output: Option<&mut crate::csv_output::CsvOutput>,
    result: i64,
    timing_mode: bool,
    duration_us: u64,
) {
    if let (Some(entry), Some(output)) = (syscall_entry, csv_output) {
        let duration = if timing_mode && duration_us > 0 {
            Some(duration_us)
        } else {
            None
        };

        // Format source location as "file:line" string
        let source_location = entry.source.as_ref().map(|src| {
            if let Some(func) = &src.function {
                format!("{}:{} in {}", src.file, src.line, func)
            } else {
                format!("{}:{}", src.file, src.line)
            }
        });

        // Format arguments as comma-separated string
        let arguments = entry.args.join(", ");

        output.add_syscall(crate::csv_output::CsvSyscall {
            name: entry.name.clone(),
            arguments,
            result,
            duration_us: duration,
            source_location,
        });
    }
}

/// Record HTML output for a syscall
fn record_html_for_syscall(
    syscall_entry: &Option<SyscallEntry>,
    html_output: Option<&mut crate::html_output::HtmlOutput>,
    result: i64,
    timing_mode: bool,
    duration_us: u64,
) {
    if let (Some(entry), Some(output)) = (syscall_entry, html_output) {
        let duration = if timing_mode && duration_us > 0 {
            Some(duration_us)
        } else {
            None
        };

        // Format source location as "file:line" string
        let source_location = entry.source.as_ref().map(|src| {
            if let Some(func) = &src.function {
                format!("{}:{} in {}", src.file, src.line, func)
            } else {
                format!("{}:{}", src.file, src.line)
            }
        });

        // Format arguments as comma-separated string
        let arguments = entry.args.join(", ");

        output.add_syscall(crate::html_output::HtmlSyscall {
            name: entry.name.clone(),
            arguments,
            result,
            duration_us: duration,
            source_location,
        });
    }
}

/// Record function profiling data
fn record_function_profiling(
    syscall_entry: &Option<SyscallEntry>,
    function_profiler: Option<&mut crate::function_profiler::FunctionProfiler>,
    duration_us: u64,
) {
    if let (Some(entry), Some(profiler)) = (syscall_entry, function_profiler) {
        if let Some(function_name) = &entry.function_name {
            profiler.record(
                function_name,
                &entry.name,
                duration_us,
                entry.caller_name.as_deref(),
            );
        }
    }
}

/// Print syscall result
fn print_syscall_result(result: i64, timing_mode: bool, duration_us: u64) {
    if timing_mode && duration_us > 0 {
        println!("{} <{:.6}>", result, duration_us as f64 / 1_000_000.0);
    } else {
        println!("{result}");
    }
}

/// Handle syscall exit - print return value and record statistics
/// Handle real-time anomaly detection and alerts
fn handle_anomaly_detection(
    syscall_entry: &Option<SyscallEntry>,
    anomaly_detector: Option<&mut crate::anomaly::AnomalyDetector>,
    duration_us: u64,
) {
    if let (Some(entry), Some(detector)) = (syscall_entry, anomaly_detector) {
        if let Some(anomaly) = detector.record_and_check(&entry.name, duration_us) {
            // Print real-time anomaly alert to stderr
            let severity_label = match anomaly.severity {
                crate::anomaly::AnomalySeverity::Low => "🟢 Low",
                crate::anomaly::AnomalySeverity::Medium => "🟡 Medium",
                crate::anomaly::AnomalySeverity::High => "🔴 High",
            };
            eprintln!(
                "⚠️  ANOMALY: {} took {} μs ({:.1}σ from baseline {:.1} μs) - {}",
                anomaly.syscall_name,
                anomaly.duration_us,
                anomaly.z_score.abs(),
                anomaly.baseline_mean,
                severity_label
            );
        }
    }
}

/// Check if syscall result should be printed to stdout
fn should_print_result(
    syscall_entry: &Option<SyscallEntry>,
    in_stats_mode: bool,
    in_json_mode: bool,
    in_csv_mode: bool,
    in_html_mode: bool,
) -> bool {
    syscall_entry.is_some() && !in_stats_mode && !in_json_mode && !in_csv_mode && !in_html_mode
}

/// Sprint 26: Capture decision traces from `write()` syscalls to stderr
///
/// Intercepts write(2, buffer, count) calls and parses [DECISION] and [RESULT] lines
fn capture_decision_trace(
    child: Pid,
    syscall_entry: &Option<SyscallEntry>,
    decision_tracer: Option<&mut crate::decision_trace::DecisionTracer>,
    bytes_written: i64,
) {
    // Only process if decision tracing is enabled
    let Some(tracer) = decision_tracer else {
        return;
    };

    // Only process if we have a syscall entry
    let Some(entry) = syscall_entry else {
        return;
    };

    // Only intercept write() syscalls
    if entry.name != "write" {
        return;
    }

    // Check if writing to stderr (fd = 2)
    if entry.raw_arg1 != Some(2) {
        return;
    }

    // Only process successful writes
    if bytes_written <= 0 {
        return;
    }

    // Get buffer address and size
    let buffer_addr = entry.raw_arg2.unwrap_or(0);
    let buffer_size = bytes_written as usize;

    // Read buffer from child process memory
    use nix::sys::uio::{process_vm_readv, RemoteIoVec};
    use std::io::IoSliceMut;

    let mut buffer = vec![0u8; buffer_size];
    let mut local_iov = [IoSliceMut::new(&mut buffer)];
    let remote_iov = [RemoteIoVec {
        base: buffer_addr as usize,
        len: buffer_size,
    }];

    // Try to read; silently ignore errors (child may have exited, etc.)
    if process_vm_readv(child, &mut local_iov, &remote_iov).is_err() {
        return;
    }

    // Convert to string, replacing invalid UTF-8 with replacement character
    let content = String::from_utf8_lossy(&buffer);

    // Parse each line through DecisionTracer
    for line in content.lines() {
        // Ignore parse errors - not all stderr lines are decision traces
        let _ = tracer.parse_line(line);
    }
}

fn handle_syscall_exit(
    child: Pid,
    syscall_entry: &Option<SyscallEntry>,
    tracers: &mut Tracers,
    timing_mode: bool,
    duration_us: u64,
) -> Result<()> {
    let regs = ptrace::getregs(child).context("Failed to get registers")?;
    let result = regs.rax as i64;

    // Check modes before borrowing
    let in_stats_mode = tracers.stats_tracker.is_some();
    let in_json_mode = tracers.json_output.is_some();
    let in_csv_mode = tracers.csv_output.is_some() || tracers.csv_stats_output.is_some();
    let in_html_mode = tracers.html_output.is_some();

    // Record statistics
    record_stats_for_syscall(
        syscall_entry,
        tracers.stats_tracker.as_mut(),
        result,
        duration_us,
    );

    // Record JSON output
    record_json_for_syscall(
        syscall_entry,
        tracers.json_output.as_mut(),
        result,
        timing_mode,
        duration_us,
    );

    // Record CSV output
    record_csv_for_syscall(
        syscall_entry,
        tracers.csv_output.as_mut(),
        result,
        timing_mode,
        duration_us,
    );

    // Record HTML output
    record_html_for_syscall(
        syscall_entry,
        tracers.html_output.as_mut(),
        result,
        timing_mode,
        duration_us,
    );

    // Sprint 26: Capture decision traces from stderr writes
    capture_decision_trace(
        child,
        syscall_entry,
        tracers.decision_tracer.as_mut(),
        result,
    );

    // Record CSV stats (we'll handle this in print_summaries)
    if let (Some(entry), Some(stats)) = (syscall_entry, tracers.csv_stats_output.as_mut()) {
        // CSV stats are accumulated in stats_tracker and printed at the end
        let _ = (entry, stats); // Suppress unused warning for now
    }

    // Record function profiling
    record_function_profiling(
        syscall_entry,
        tracers.function_profiler.as_mut(),
        duration_us,
    );

    // Sprint 20: Real-time anomaly detection
    handle_anomaly_detection(
        syscall_entry,
        tracers.anomaly_detector.as_mut(),
        duration_us,
    );

    // Sprint 30: Record syscall to OTLP exporter
    #[cfg(feature = "otlp")]
    if let (Some(entry), Some(exporter)) = (syscall_entry, tracers.otlp_exporter.as_ref()) {
        let source_file = entry.source.as_ref().map(|s| s.file.as_str());
        let source_line = entry.source.as_ref().map(|s| s.line);

        exporter.record_syscall(
            &entry.name,
            if duration_us > 0 {
                Some(duration_us)
            } else {
                None
            },
            result,
            source_file,
            source_line,
        );
    }

    // Print result if not in statistics, JSON, CSV, or HTML mode
    if should_print_result(
        syscall_entry,
        in_stats_mode,
        in_json_mode,
        in_csv_mode,
        in_html_mode,
    ) {
        print_syscall_result(result, timing_mode, duration_us);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_command_requires_nonempty_array() {
        let empty: Vec<String> = vec![];
        let config = TracerConfig {
            enable_source: false,
            filter: crate::filter::SyscallFilter::all(),
            statistics_mode: false,
            timing_mode: false,
            output_format: crate::cli::OutputFormat::Text,
            follow_forks: false,
            profile_self: false,
            function_time: false,
            stats_extended: false,                    // Sprint 19
            anomaly_threshold: 3.0,                   // Sprint 19
            anomaly_realtime: false,                  // Sprint 20
            anomaly_window_size: 100,                 // Sprint 20
            hpu_analysis: false,                      // Sprint 21
            hpu_cpu_only: false,                      // Sprint 21
            ml_anomaly: false,                        // Sprint 23
            ml_clusters: 3,                           // Sprint 23
            ml_compare: false,                        // Sprint 23
            ml_outliers: false,                       // Sprint 22
            ml_outlier_threshold: 0.1,                // Sprint 22
            ml_outlier_trees: 100,                    // Sprint 22
            explain: false,                           // Sprint 22/23
            dl_anomaly: false,                        // Sprint 23
            dl_threshold: 2.0,                        // Sprint 23
            dl_hidden_size: 3,                        // Sprint 23
            dl_epochs: 100,                           // Sprint 23
            trace_transpiler_decisions: false,        // Sprint 26
            transpiler_map: None,                     // Sprint 24-28
            otlp_endpoint: None,                      // Sprint 30
            otlp_service_name: "renacer".to_string(), // Sprint 30
            trace_parent: None,                       // Sprint 33
            chaos_config: None,                       // Sprint 47
        };
        let result = trace_command(&empty, config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    #[ignore = "requires ptrace permissions - run with --ignored"]
    fn test_trace_command_basic() {
        // GREEN: trace_command now works - verify it succeeds for basic commands
        let cmd = vec!["echo".to_string(), "test".to_string()];
        let config = TracerConfig {
            enable_source: false,
            filter: crate::filter::SyscallFilter::all(),
            statistics_mode: false,
            timing_mode: false,
            output_format: crate::cli::OutputFormat::Text,
            follow_forks: false,
            profile_self: false,
            function_time: false,
            stats_extended: false,                    // Sprint 19
            anomaly_threshold: 3.0,                   // Sprint 19
            anomaly_realtime: false,                  // Sprint 20
            anomaly_window_size: 100,                 // Sprint 20
            hpu_analysis: false,                      // Sprint 21
            hpu_cpu_only: false,                      // Sprint 21
            ml_anomaly: false,                        // Sprint 23
            ml_clusters: 3,                           // Sprint 23
            ml_compare: false,                        // Sprint 23
            ml_outliers: false,                       // Sprint 22
            ml_outlier_threshold: 0.1,                // Sprint 22
            ml_outlier_trees: 100,                    // Sprint 22
            explain: false,                           // Sprint 22/23
            dl_anomaly: false,                        // Sprint 23
            dl_threshold: 2.0,                        // Sprint 23
            dl_hidden_size: 3,                        // Sprint 23
            dl_epochs: 100,                           // Sprint 23
            trace_transpiler_decisions: false,        // Sprint 26
            transpiler_map: None,                     // Sprint 24-28
            otlp_endpoint: None,                      // Sprint 30
            otlp_service_name: "renacer".to_string(), // Sprint 30
            trace_parent: None,                       // Sprint 33
            chaos_config: None,                       // Sprint 47
        };
        let result = trace_command(&cmd, config);
        assert!(result.is_ok(), "trace_command failed: {:?}", result);
    }

    #[test]
    fn test_syscall_entry_creation() {
        let entry = SyscallEntry {
            name: "open".to_string(),
            args: vec!["arg1".to_string(), "arg2".to_string()],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: Some(1),
            raw_arg2: Some(2),
            raw_arg3: Some(3),
        };
        assert_eq!(entry.name, "open");
        assert_eq!(entry.args.len(), 2);
        assert!(entry.source.is_none());
        assert!(entry.function_name.is_none());
    }

    #[test]
    fn test_syscall_entry_with_source() {
        let source = crate::json_output::JsonSourceLocation {
            file: "test.rs".to_string(),
            line: 42,
            function: Some("main".to_string()),
        };
        let entry = SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: Some(source),
            function_name: Some("main".to_string()),
            caller_name: None,
            raw_arg1: Some(0),
            raw_arg2: Some(0),
            raw_arg3: Some(0),
        };
        assert_eq!(entry.name, "read");
        assert!(entry.source.is_some());
        let src = entry.source.expect("test");
        assert_eq!(src.file, "test.rs");
        assert_eq!(src.line, 42);
        assert_eq!(src.function, Some("main".to_string()));
        assert_eq!(entry.function_name, Some("main".to_string()));
    }

    #[test]
    fn test_attach_to_pid_invalid_pid() {
        // Test attaching to a non-existent PID (should fail)
        let config = TracerConfig::default();
        let result = attach_to_pid(999999, config);
        assert!(result.is_err());
        // Error message should mention attach failure
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("attach") || err_msg.contains("Failed"),
            "Error: {}",
            err_msg
        );
    }

    // TracerConfig default tests
    #[test]
    fn test_tracer_config_default() {
        let config = TracerConfig::default();
        assert!(!config.enable_source);
        assert!(!config.statistics_mode);
        assert!(!config.timing_mode);
        assert!(!config.follow_forks);
        assert!(!config.profile_self);
        assert!(!config.function_time);
        assert!(!config.stats_extended);
        assert!((config.anomaly_threshold - 2.0).abs() < f32::EPSILON);
        assert!(!config.anomaly_realtime);
        assert_eq!(config.anomaly_window_size, 100);
        assert!(!config.hpu_analysis);
        assert!(!config.hpu_cpu_only);
        assert!(!config.ml_anomaly);
        assert_eq!(config.ml_clusters, 5);
        assert!(!config.ml_compare);
        assert!(!config.ml_outliers);
        assert!((config.ml_outlier_threshold - 0.1).abs() < f32::EPSILON);
        assert_eq!(config.ml_outlier_trees, 100);
        assert!(!config.explain);
        assert!(!config.dl_anomaly);
        assert!((config.dl_threshold - 2.0).abs() < f32::EPSILON);
        assert_eq!(config.dl_hidden_size, 8);
        assert_eq!(config.dl_epochs, 50);
        assert!(!config.trace_transpiler_decisions);
        assert!(config.transpiler_map.is_none());
        assert!(config.otlp_endpoint.is_none());
        assert_eq!(config.otlp_service_name, "renacer");
        assert!(config.trace_parent.is_none());
        assert!(config.chaos_config.is_none());
    }

    // ProcessState tests
    #[test]
    fn test_process_state_new() {
        let state = ProcessState::new();
        assert!(!state.in_syscall);
        assert!(state.current_syscall_entry.is_none());
        assert!(state.syscall_entry_time.is_none());
        assert!(state.dwarf_ctx.is_none());
        assert!(!state.dwarf_loaded);
    }

    // initialize_profiling_tracers tests
    #[test]
    fn test_initialize_profiling_tracers_all_disabled() {
        let config = TracerConfig::default();
        let (profiling_ctx, function_profiler, anomaly_detector) =
            initialize_profiling_tracers(&config);
        assert!(profiling_ctx.is_none());
        assert!(function_profiler.is_none());
        assert!(anomaly_detector.is_none());
    }

    #[test]
    fn test_initialize_profiling_tracers_profile_self() {
        let mut config = TracerConfig::default();
        config.profile_self = true;
        let (profiling_ctx, function_profiler, anomaly_detector) =
            initialize_profiling_tracers(&config);
        assert!(profiling_ctx.is_some());
        assert!(function_profiler.is_none());
        assert!(anomaly_detector.is_none());
    }

    #[test]
    fn test_initialize_profiling_tracers_function_time() {
        let mut config = TracerConfig::default();
        config.function_time = true;
        let (profiling_ctx, function_profiler, anomaly_detector) =
            initialize_profiling_tracers(&config);
        assert!(profiling_ctx.is_none());
        assert!(function_profiler.is_some());
        assert!(anomaly_detector.is_none());
    }

    #[test]
    fn test_initialize_profiling_tracers_anomaly_realtime() {
        let mut config = TracerConfig::default();
        config.anomaly_realtime = true;
        let (profiling_ctx, function_profiler, anomaly_detector) =
            initialize_profiling_tracers(&config);
        assert!(profiling_ctx.is_none());
        assert!(function_profiler.is_none());
        assert!(anomaly_detector.is_some());
    }

    #[test]
    fn test_initialize_profiling_tracers_all_enabled() {
        let mut config = TracerConfig::default();
        config.profile_self = true;
        config.function_time = true;
        config.anomaly_realtime = true;
        let (profiling_ctx, function_profiler, anomaly_detector) =
            initialize_profiling_tracers(&config);
        assert!(profiling_ctx.is_some());
        assert!(function_profiler.is_some());
        assert!(anomaly_detector.is_some());
    }

    // initialize_output_tracers tests
    #[test]
    fn test_initialize_output_tracers_text_format() {
        let config = TracerConfig::default();
        let (json, csv, csv_stats, html) = initialize_output_tracers(&config);
        assert!(json.is_none());
        assert!(csv.is_none());
        assert!(csv_stats.is_none());
        assert!(html.is_none());
    }

    #[test]
    fn test_initialize_output_tracers_json_format() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Json;
        let (json, csv, csv_stats, html) = initialize_output_tracers(&config);
        assert!(json.is_some());
        assert!(csv.is_none());
        assert!(csv_stats.is_none());
        assert!(html.is_none());
    }

    #[test]
    fn test_initialize_output_tracers_csv_format() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Csv;
        let (json, csv, csv_stats, html) = initialize_output_tracers(&config);
        assert!(json.is_none());
        assert!(csv.is_some());
        assert!(csv_stats.is_none());
        assert!(html.is_none());
    }

    #[test]
    fn test_initialize_output_tracers_csv_stats_format() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Csv;
        config.statistics_mode = true;
        let (json, csv, csv_stats, html) = initialize_output_tracers(&config);
        assert!(json.is_none());
        assert!(csv.is_none());
        assert!(csv_stats.is_some());
        assert!(html.is_none());
    }

    #[test]
    fn test_initialize_output_tracers_html_format() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Html;
        let (json, csv, csv_stats, html) = initialize_output_tracers(&config);
        assert!(json.is_none());
        assert!(csv.is_none());
        assert!(csv_stats.is_none());
        assert!(html.is_some());
    }

    // initialize_tracers tests
    #[test]
    fn test_initialize_tracers_default() {
        let config = TracerConfig::default();
        let tracers = initialize_tracers(&config);
        assert!(tracers.profiling_ctx.is_none());
        assert!(tracers.function_profiler.is_none());
        assert!(tracers.stats_tracker.is_none());
        assert!(tracers.json_output.is_none());
        assert!(tracers.csv_output.is_none());
        assert!(tracers.csv_stats_output.is_none());
        assert!(tracers.html_output.is_none());
        assert!(tracers.anomaly_detector.is_none());
        assert!(tracers.decision_tracer.is_none());
    }

    #[test]
    fn test_initialize_tracers_with_statistics() {
        let mut config = TracerConfig::default();
        config.statistics_mode = true;
        let tracers = initialize_tracers(&config);
        assert!(tracers.stats_tracker.is_some());
    }

    #[test]
    fn test_initialize_tracers_with_ml_anomaly() {
        let mut config = TracerConfig::default();
        config.ml_anomaly = true;
        let tracers = initialize_tracers(&config);
        assert!(tracers.stats_tracker.is_some());
    }

    #[test]
    fn test_initialize_tracers_with_ml_outliers() {
        let mut config = TracerConfig::default();
        config.ml_outliers = true;
        let tracers = initialize_tracers(&config);
        assert!(tracers.stats_tracker.is_some());
    }

    #[test]
    fn test_initialize_tracers_with_dl_anomaly() {
        let mut config = TracerConfig::default();
        config.dl_anomaly = true;
        let tracers = initialize_tracers(&config);
        assert!(tracers.stats_tracker.is_some());
    }

    #[test]
    fn test_initialize_tracers_with_decision_tracer() {
        let mut config = TracerConfig::default();
        config.trace_transpiler_decisions = true;
        let tracers = initialize_tracers(&config);
        assert!(tracers.decision_tracer.is_some());
    }

    #[test]
    fn test_initialize_tracers_json_output() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Json;
        let tracers = initialize_tracers(&config);
        assert!(tracers.json_output.is_some());
    }

    #[test]
    fn test_initialize_tracers_html_output() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Html;
        let tracers = initialize_tracers(&config);
        assert!(tracers.html_output.is_some());
    }

    // SyscallEntry tests
    #[test]
    fn test_syscall_entry_with_raw_args() {
        let entry = SyscallEntry {
            name: "write".to_string(),
            args: vec!["1".to_string(), "buf".to_string(), "10".to_string()],
            source: None,
            function_name: None,
            caller_name: Some("caller_fn".to_string()),
            raw_arg1: Some(1),
            raw_arg2: Some(0x7fff_0000_0000),
            raw_arg3: Some(10),
        };
        assert_eq!(entry.raw_arg1, Some(1));
        assert_eq!(entry.raw_arg2, Some(0x7fff_0000_0000));
        assert_eq!(entry.raw_arg3, Some(10));
        assert_eq!(entry.caller_name, Some("caller_fn".to_string()));
    }

    #[test]
    fn test_syscall_entry_empty_args() {
        let entry = SyscallEntry {
            name: "getpid".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        };
        assert!(entry.args.is_empty());
        assert!(entry.raw_arg1.is_none());
    }

    // Test TraceChild helper functions implicitly
    #[test]
    fn test_tracer_config_with_custom_values() {
        let config = TracerConfig {
            enable_source: true,
            filter: crate::filter::SyscallFilter::all(),
            statistics_mode: true,
            timing_mode: true,
            output_format: crate::cli::OutputFormat::Json,
            follow_forks: true,
            profile_self: true,
            function_time: true,
            stats_extended: true,
            anomaly_threshold: 5.0,
            anomaly_realtime: true,
            anomaly_window_size: 200,
            hpu_analysis: true,
            hpu_cpu_only: true,
            ml_anomaly: true,
            ml_clusters: 10,
            ml_compare: true,
            ml_outliers: true,
            ml_outlier_threshold: 0.2,
            ml_outlier_trees: 200,
            explain: true,
            dl_anomaly: true,
            dl_threshold: 3.0,
            dl_hidden_size: 16,
            dl_epochs: 100,
            trace_transpiler_decisions: true,
            transpiler_map: None,
            otlp_endpoint: Some("http://localhost:4317".to_string()),
            otlp_service_name: "test-service".to_string(),
            trace_parent: Some("00-trace-parent-01".to_string()),
            chaos_config: None,
        };

        assert!(config.enable_source);
        assert!(config.statistics_mode);
        assert!(config.timing_mode);
        assert!(config.follow_forks);
        assert!(config.profile_self);
        assert!(config.function_time);
        assert!(config.stats_extended);
        assert!((config.anomaly_threshold - 5.0).abs() < f32::EPSILON);
        assert!(config.anomaly_realtime);
        assert_eq!(config.anomaly_window_size, 200);
        assert!(config.hpu_analysis);
        assert!(config.hpu_cpu_only);
        assert!(config.ml_anomaly);
        assert_eq!(config.ml_clusters, 10);
        assert!(config.ml_compare);
        assert!(config.ml_outliers);
        assert!((config.ml_outlier_threshold - 0.2).abs() < f32::EPSILON);
        assert_eq!(config.ml_outlier_trees, 200);
        assert!(config.explain);
        assert!(config.dl_anomaly);
        assert!((config.dl_threshold - 3.0).abs() < f32::EPSILON);
        assert_eq!(config.dl_hidden_size, 16);
        assert_eq!(config.dl_epochs, 100);
        assert!(config.trace_transpiler_decisions);
        assert!(config.otlp_endpoint.is_some());
        assert_eq!(config.otlp_service_name, "test-service");
        assert!(config.trace_parent.is_some());
    }

    #[test]
    fn test_initialize_tracers_full_config() {
        let mut config = TracerConfig::default();
        config.profile_self = true;
        config.function_time = true;
        config.anomaly_realtime = true;
        config.statistics_mode = true;
        config.ml_anomaly = true;
        config.ml_outliers = true;
        config.dl_anomaly = true;
        config.trace_transpiler_decisions = true;
        config.output_format = crate::cli::OutputFormat::Json;

        let tracers = initialize_tracers(&config);
        assert!(tracers.profiling_ctx.is_some());
        assert!(tracers.function_profiler.is_some());
        assert!(tracers.anomaly_detector.is_some());
        assert!(tracers.stats_tracker.is_some());
        assert!(tracers.json_output.is_some());
        assert!(tracers.decision_tracer.is_some());
    }

    // Tests for analysis and output functions

    #[test]
    fn test_print_text_stats_none() {
        let stats_tracker: Option<crate::stats::StatsTracker> = None;
        #[cfg(feature = "otlp")]
        print_text_stats(&stats_tracker, false, 2.0, None);
        #[cfg(not(feature = "otlp"))]
        print_text_stats(&stats_tracker, false, 2.0, None);
        // Should not panic with None
    }

    #[test]
    fn test_print_text_stats_with_tracker() {
        let stats_tracker = Some(crate::stats::StatsTracker::new());
        #[cfg(feature = "otlp")]
        print_text_stats(&stats_tracker, false, 2.0, None);
        #[cfg(not(feature = "otlp"))]
        print_text_stats(&stats_tracker, false, 2.0, None);
    }

    #[test]
    fn test_print_text_stats_extended() {
        let stats_tracker = Some(crate::stats::StatsTracker::new());
        #[cfg(feature = "otlp")]
        print_text_stats(&stats_tracker, true, 3.0, None);
        #[cfg(not(feature = "otlp"))]
        print_text_stats(&stats_tracker, true, 3.0, None);
    }

    #[test]
    fn test_print_json_output() {
        let output = crate::json_output::JsonOutput::new();
        // Should not panic
        print_json_output(output, 0);
    }

    #[test]
    fn test_print_json_output_nonzero_exit() {
        let output = crate::json_output::JsonOutput::new();
        print_json_output(output, 1);
    }

    #[test]
    fn test_generate_ml_analysis_none() {
        let result = generate_ml_analysis_for_json(&None, 5);
        assert!(result.is_none());
    }

    #[test]
    fn test_generate_ml_analysis_empty_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result = generate_ml_analysis_for_json(&tracker, 3);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_isolation_forest_none() {
        let result = generate_isolation_forest_analysis_for_json(&None, 100, 0.1, false);
        assert!(result.is_none());
    }

    #[test]
    fn test_generate_isolation_forest_empty_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result = generate_isolation_forest_analysis_for_json(&tracker, 50, 0.1, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_isolation_forest_with_explain() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result = generate_isolation_forest_analysis_for_json(&tracker, 50, 0.1, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_autoencoder_none() {
        let result = generate_autoencoder_analysis_for_json(&None, 8, 50, 2.0, false);
        assert!(result.is_none());
    }

    #[test]
    fn test_generate_autoencoder_empty_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result = generate_autoencoder_analysis_for_json(&tracker, 8, 50, 2.0, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_autoencoder_with_explain() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result = generate_autoencoder_analysis_for_json(&tracker, 8, 50, 2.0, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_print_csv_stats_none() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        print_csv_stats(csv_stats, &None, false, false, 2.0);
    }

    #[test]
    fn test_print_csv_stats_with_tracker() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        print_csv_stats(csv_stats, &tracker, false, false, 2.0);
    }

    #[test]
    fn test_print_csv_stats_with_timing() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        print_csv_stats(csv_stats, &tracker, true, false, 2.0);
    }

    #[test]
    fn test_print_csv_stats_extended() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        print_csv_stats(csv_stats, &tracker, true, true, 3.0);
    }

    #[test]
    fn test_print_hpu_analysis_none() {
        print_hpu_analysis(&None, false);
    }

    #[test]
    fn test_print_hpu_analysis_cpu_only() {
        let tracker = Some(crate::stats::StatsTracker::new());
        print_hpu_analysis(&tracker, true);
    }

    #[test]
    fn test_print_hpu_analysis_with_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        print_hpu_analysis(&tracker, false);
    }

    #[test]
    fn test_print_ml_analysis_none() {
        print_ml_analysis(&None, 5, false, 2.0);
    }

    #[test]
    fn test_print_ml_analysis_with_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        print_ml_analysis(&tracker, 5, false, 2.0);
    }

    #[test]
    fn test_print_ml_analysis_compare() {
        let tracker = Some(crate::stats::StatsTracker::new());
        print_ml_analysis(&tracker, 5, true, 2.0);
    }

    #[test]
    fn test_print_isolation_forest_none() {
        print_isolation_forest_analysis(&None, 100, 0.1, false);
    }

    #[test]
    fn test_print_isolation_forest_with_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        print_isolation_forest_analysis(&tracker, 100, 0.1, false);
    }

    #[test]
    fn test_print_isolation_forest_with_explain() {
        let tracker = Some(crate::stats::StatsTracker::new());
        print_isolation_forest_analysis(&tracker, 100, 0.1, true);
    }

    #[test]
    fn test_initialize_output_tracers_csv_with_timing() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Csv;
        config.timing_mode = true;
        let (json, csv, csv_stats, html) = initialize_output_tracers(&config);
        assert!(json.is_none());
        assert!(csv.is_some());
        assert!(csv_stats.is_none());
        assert!(html.is_none());
    }

    #[test]
    fn test_initialize_output_tracers_csv_with_source() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Csv;
        config.enable_source = true;
        let (_, csv, _, _) = initialize_output_tracers(&config);
        assert!(csv.is_some());
    }

    #[test]
    fn test_initialize_output_tracers_html_with_timing_and_source() {
        let mut config = TracerConfig::default();
        config.output_format = crate::cli::OutputFormat::Html;
        config.timing_mode = true;
        config.enable_source = true;
        let (_, _, _, html) = initialize_output_tracers(&config);
        assert!(html.is_some());
    }

    // More analysis function tests

    #[test]
    fn test_print_autoencoder_none() {
        print_autoencoder_analysis(&None, 8, 50, 2.0, false);
    }

    #[test]
    fn test_print_autoencoder_with_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        print_autoencoder_analysis(&tracker, 8, 50, 2.0, false);
    }

    #[test]
    fn test_print_autoencoder_with_explain() {
        let tracker = Some(crate::stats::StatsTracker::new());
        print_autoencoder_analysis(&tracker, 8, 50, 2.0, true);
    }

    #[test]
    fn test_print_optional_summaries_all_none() {
        print_optional_summaries(None, None, None);
    }

    #[test]
    fn test_print_optional_summaries_with_profiling() {
        let ctx = Some(crate::profiling::ProfilingContext::new());
        print_optional_summaries(ctx, None, None);
    }

    #[test]
    fn test_print_optional_summaries_with_function_profiler() {
        let profiler = Some(crate::function_profiler::FunctionProfiler::new());
        print_optional_summaries(None, profiler, None);
    }

    #[test]
    fn test_print_optional_summaries_with_anomaly_detector() {
        let detector = Some(crate::anomaly::AnomalyDetector::new(100, 2.0));
        print_optional_summaries(None, None, detector);
    }

    #[test]
    fn test_print_optional_summaries_all_some() {
        let ctx = Some(crate::profiling::ProfilingContext::new());
        let profiler = Some(crate::function_profiler::FunctionProfiler::new());
        let detector = Some(crate::anomaly::AnomalyDetector::new(100, 2.0));
        print_optional_summaries(ctx, profiler, detector);
    }

    #[test]
    fn test_print_decision_trace_summary_none() {
        print_decision_trace_summary(None);
    }

    #[test]
    fn test_print_decision_trace_summary_empty() {
        let tracer = Some(crate::decision_trace::DecisionTracer::new());
        print_decision_trace_summary(tracer);
    }

    #[test]
    fn test_analysis_config_fields() {
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 3.0,
            hpu_analysis: true,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 10,
            ml_compare: true,
            ml_outliers: true,
            ml_outlier_threshold: 0.15,
            ml_outlier_trees: 200,
            dl_anomaly: true,
            dl_threshold: 2.5,
            dl_hidden_size: 16,
            dl_epochs: 100,
            explain: true,
        };
        assert!(analysis.stats_extended);
        assert!((analysis.anomaly_threshold - 3.0).abs() < f32::EPSILON);
        assert!(analysis.hpu_analysis);
        assert!(!analysis.hpu_cpu_only);
        assert!(analysis.ml_anomaly);
        assert_eq!(analysis.ml_clusters, 10);
    }

    #[test]
    fn test_print_analysis_summaries_none() {
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_analysis_summaries(&None, &analysis);
    }

    #[test]
    fn test_print_analysis_summaries_hpu() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: true,
            hpu_cpu_only: true,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_analysis_summaries(&tracker, &analysis);
    }

    #[test]
    fn test_print_analysis_summaries_ml() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_analysis_summaries(&tracker, &analysis);
    }

    #[test]
    fn test_print_analysis_summaries_outliers() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_analysis_summaries(&tracker, &analysis);
    }

    #[test]
    fn test_print_analysis_summaries_dl() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_analysis_summaries(&tracker, &analysis);
    }

    #[test]
    fn test_print_analysis_summaries_all() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 2.0,
            hpu_analysis: true,
            hpu_cpu_only: true,
            ml_anomaly: true,
            ml_clusters: 5,
            ml_compare: true,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: true,
        };
        print_analysis_summaries(&tracker, &analysis);
    }

    #[test]
    fn test_handle_json_output_basic() {
        let output = crate::json_output::JsonOutput::new();
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        handle_json_output(output, &None, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_with_ml() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        handle_json_output(output, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_with_outliers() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: true,
        };
        handle_json_output(output, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_with_dl() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        handle_json_output(output, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_all_analysis() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 2.0,
            hpu_analysis: true,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 5,
            ml_compare: true,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: true,
        };
        handle_json_output(output, &tracker, &analysis, 1);
    }

    // should_print_result tests
    #[test]
    fn test_should_print_result_none_entry() {
        assert!(!should_print_result(&None, false, false, false, false));
    }

    #[test]
    fn test_should_print_result_with_entry_no_modes() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        assert!(should_print_result(&entry, false, false, false, false));
    }

    #[test]
    fn test_should_print_result_stats_mode() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        assert!(!should_print_result(&entry, true, false, false, false));
    }

    #[test]
    fn test_should_print_result_json_mode() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        assert!(!should_print_result(&entry, false, true, false, false));
    }

    #[test]
    fn test_should_print_result_csv_mode() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        assert!(!should_print_result(&entry, false, false, true, false));
    }

    #[test]
    fn test_should_print_result_html_mode() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        assert!(!should_print_result(&entry, false, false, false, true));
    }

    #[test]
    fn test_should_print_result_all_modes() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        assert!(!should_print_result(&entry, true, true, true, true));
    }

    // print_syscall_result tests
    #[test]
    fn test_print_syscall_result_success() {
        print_syscall_result(0, false, 0);
    }

    #[test]
    fn test_print_syscall_result_error() {
        print_syscall_result(-1, false, 0);
    }

    #[test]
    fn test_print_syscall_result_with_timing() {
        print_syscall_result(42, true, 1234);
    }

    #[test]
    fn test_print_syscall_result_negative_with_timing() {
        print_syscall_result(-22, true, 5678);
    }

    // record_stats_for_syscall tests
    #[test]
    fn test_record_stats_for_syscall_none_entry() {
        let mut tracker = Some(crate::stats::StatsTracker::new());
        record_stats_for_syscall(&None, tracker.as_mut(), 0, 100);
    }

    #[test]
    fn test_record_stats_for_syscall_none_tracker() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        record_stats_for_syscall(&entry, None, 0, 100);
    }

    #[test]
    fn test_record_stats_for_syscall_with_data() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        let mut tracker = Some(crate::stats::StatsTracker::new());
        record_stats_for_syscall(&entry, tracker.as_mut(), 100, 500);
    }

    // record_function_profiling tests
    #[test]
    fn test_record_function_profiling_none_entry() {
        let mut profiler = Some(crate::function_profiler::FunctionProfiler::new());
        record_function_profiling(&None, profiler.as_mut(), 100);
    }

    #[test]
    fn test_record_function_profiling_none_profiler() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: Some("test_fn".to_string()),
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        record_function_profiling(&entry, None, 100);
    }

    #[test]
    fn test_record_function_profiling_with_data() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: Some("test_fn".to_string()),
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        let mut profiler = Some(crate::function_profiler::FunctionProfiler::new());
        record_function_profiling(&entry, profiler.as_mut(), 100);
    }

    #[test]
    fn test_record_function_profiling_no_function_name() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        let mut profiler = Some(crate::function_profiler::FunctionProfiler::new());
        record_function_profiling(&entry, profiler.as_mut(), 100);
    }

    // handle_anomaly_detection tests
    #[test]
    fn test_handle_anomaly_detection_none_entry() {
        let mut detector = Some(crate::anomaly::AnomalyDetector::new(100, 2.0));
        handle_anomaly_detection(&None, detector.as_mut(), 100);
    }

    #[test]
    fn test_handle_anomaly_detection_none_detector() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        handle_anomaly_detection(&entry, None, 100);
    }

    #[test]
    fn test_handle_anomaly_detection_with_data() {
        let entry = Some(SyscallEntry {
            name: "read".to_string(),
            args: vec![],
            source: None,
            function_name: None,
            caller_name: None,
            raw_arg1: None,
            raw_arg2: None,
            raw_arg3: None,
        });
        let mut detector = Some(crate::anomaly::AnomalyDetector::new(100, 2.0));
        handle_anomaly_detection(&entry, detector.as_mut(), 100);
    }

    // print_summaries tests with Tracers struct
    #[test]
    fn test_print_summaries_minimal() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: None,
            json_output: None,
            csv_output: None,
            csv_stats_output: None,
            html_output: None,
            anomaly_detector: None,
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_summaries(tracers, false, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_with_stats() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: Some(crate::stats::StatsTracker::new()),
            json_output: None,
            csv_output: None,
            csv_stats_output: None,
            html_output: None,
            anomaly_detector: None,
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_summaries(tracers, true, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_with_json() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: None,
            json_output: Some(crate::json_output::JsonOutput::new()),
            csv_output: None,
            csv_stats_output: None,
            html_output: None,
            anomaly_detector: None,
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_summaries(tracers, false, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_with_csv_stats() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: Some(crate::stats::StatsTracker::new()),
            json_output: None,
            csv_output: None,
            csv_stats_output: Some(crate::csv_output::CsvStatsOutput::new()),
            html_output: None,
            anomaly_detector: None,
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_summaries(tracers, true, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_with_html() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: None,
            json_output: None,
            csv_output: None,
            csv_stats_output: None,
            html_output: Some(crate::html_output::HtmlOutput::new(true, true)),
            anomaly_detector: None,
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_summaries(tracers, false, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_with_profiling() {
        let tracers = Tracers {
            profiling_ctx: Some(crate::profiling::ProfilingContext::new()),
            function_profiler: Some(crate::function_profiler::FunctionProfiler::new()),
            stats_tracker: None,
            json_output: None,
            csv_output: None,
            csv_stats_output: None,
            html_output: None,
            anomaly_detector: Some(crate::anomaly::AnomalyDetector::new(100, 2.0)),
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_summaries(tracers, false, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_with_decision_tracer() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: None,
            json_output: None,
            csv_output: None,
            csv_stats_output: None,
            html_output: None,
            anomaly_detector: None,
            decision_tracer: Some(crate::decision_trace::DecisionTracer::new()),
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_summaries(tracers, false, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_all_enabled() {
        let tracers = Tracers {
            profiling_ctx: Some(crate::profiling::ProfilingContext::new()),
            function_profiler: Some(crate::function_profiler::FunctionProfiler::new()),
            stats_tracker: Some(crate::stats::StatsTracker::new()),
            json_output: None,
            csv_output: None,
            csv_stats_output: None,
            html_output: None,
            anomaly_detector: Some(crate::anomaly::AnomalyDetector::new(100, 2.0)),
            decision_tracer: Some(crate::decision_trace::DecisionTracer::new()),
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 3.0,
            hpu_analysis: true,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 5,
            ml_compare: true,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: true,
        };
        print_summaries(tracers, true, 0, &analysis);
    }

    // Tests with populated stats tracker to cover iteration paths
    fn create_populated_stats_tracker() -> crate::stats::StatsTracker {
        let mut tracker = crate::stats::StatsTracker::new();
        // Record some syscalls to populate the tracker
        tracker.record("read", 0, 1000);
        tracker.record("write", 0, 2000);
        tracker.record("open", 0, 500);
        tracker.record("close", 0, 100);
        tracker.record("read", 0, 1500);
        tracker.record("write", -1, 3000);
        tracker
    }

    #[test]
    fn test_generate_ml_analysis_populated() {
        let tracker = Some(create_populated_stats_tracker());
        let result = generate_ml_analysis_for_json(&tracker, 3);
        assert!(result.is_some());
        let report = result.expect("test");
        assert!(report.total_samples > 0);
    }

    #[test]
    fn test_generate_isolation_forest_populated() {
        let tracker = Some(create_populated_stats_tracker());
        let result = generate_isolation_forest_analysis_for_json(&tracker, 50, 0.1, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_isolation_forest_populated_with_explain() {
        let tracker = Some(create_populated_stats_tracker());
        let result = generate_isolation_forest_analysis_for_json(&tracker, 50, 0.1, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_autoencoder_populated() {
        let tracker = Some(create_populated_stats_tracker());
        let result = generate_autoencoder_analysis_for_json(&tracker, 8, 10, 2.0, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_autoencoder_populated_with_explain() {
        let tracker = Some(create_populated_stats_tracker());
        let result = generate_autoencoder_analysis_for_json(&tracker, 8, 10, 2.0, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_print_text_stats_populated() {
        let tracker = Some(create_populated_stats_tracker());
        #[cfg(feature = "otlp")]
        print_text_stats(&tracker, false, 2.0, None);
        #[cfg(not(feature = "otlp"))]
        print_text_stats(&tracker, false, 2.0, None);
    }

    #[test]
    fn test_print_text_stats_populated_extended() {
        let tracker = Some(create_populated_stats_tracker());
        #[cfg(feature = "otlp")]
        print_text_stats(&tracker, true, 2.0, None);
        #[cfg(not(feature = "otlp"))]
        print_text_stats(&tracker, true, 2.0, None);
    }

    #[test]
    fn test_print_csv_stats_populated() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        print_csv_stats(csv_stats, &tracker, false, false, 2.0);
    }

    #[test]
    fn test_print_csv_stats_populated_extended() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        print_csv_stats(csv_stats, &tracker, true, true, 2.0);
    }

    #[test]
    fn test_print_hpu_analysis_populated() {
        let tracker = Some(create_populated_stats_tracker());
        print_hpu_analysis(&tracker, false);
    }

    #[test]
    fn test_print_hpu_analysis_populated_cpu_only() {
        let tracker = Some(create_populated_stats_tracker());
        print_hpu_analysis(&tracker, true);
    }

    #[test]
    fn test_print_ml_analysis_populated() {
        let tracker = Some(create_populated_stats_tracker());
        print_ml_analysis(&tracker, 3, false, 2.0);
    }

    #[test]
    fn test_print_ml_analysis_populated_compare() {
        let tracker = Some(create_populated_stats_tracker());
        print_ml_analysis(&tracker, 3, true, 2.0);
    }

    #[test]
    fn test_print_isolation_forest_populated() {
        let tracker = Some(create_populated_stats_tracker());
        print_isolation_forest_analysis(&tracker, 50, 0.1, false);
    }

    #[test]
    fn test_print_isolation_forest_populated_explain() {
        let tracker = Some(create_populated_stats_tracker());
        print_isolation_forest_analysis(&tracker, 50, 0.1, true);
    }

    #[test]
    fn test_print_autoencoder_populated() {
        let tracker = Some(create_populated_stats_tracker());
        print_autoencoder_analysis(&tracker, 8, 10, 2.0, false);
    }

    #[test]
    fn test_print_autoencoder_populated_explain() {
        let tracker = Some(create_populated_stats_tracker());
        print_autoencoder_analysis(&tracker, 8, 10, 2.0, true);
    }

    #[test]
    fn test_handle_json_output_populated() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        handle_json_output(output, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_populated_ml() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 3,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        handle_json_output(output, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_populated_outliers() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 50,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: true,
        };
        handle_json_output(output, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_populated_dl() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 10,
            explain: false,
        };
        handle_json_output(output, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_populated_all() {
        let output = crate::json_output::JsonOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 2.0,
            hpu_analysis: true,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 3,
            ml_compare: true,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 50,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 10,
            explain: true,
        };
        handle_json_output(output, &tracker, &analysis, 1);
    }

    #[test]
    fn test_print_analysis_summaries_populated() {
        let tracker = Some(create_populated_stats_tracker());
        let analysis = AnalysisConfig {
            stats_extended: false,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_analysis_summaries(&tracker, &analysis);
    }

    #[test]
    fn test_print_analysis_summaries_populated_all() {
        let tracker = Some(create_populated_stats_tracker());
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 2.0,
            hpu_analysis: true,
            hpu_cpu_only: true,
            ml_anomaly: true,
            ml_clusters: 3,
            ml_compare: true,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 50,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 10,
            explain: true,
        };
        print_analysis_summaries(&tracker, &analysis);
    }

    #[test]
    fn test_print_summaries_populated_stats() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: Some(create_populated_stats_tracker()),
            json_output: None,
            csv_output: None,
            csv_stats_output: None,
            html_output: None,
            anomaly_detector: None,
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 2.0,
            hpu_analysis: true,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 3,
            ml_compare: true,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 50,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 10,
            explain: true,
        };
        print_summaries(tracers, true, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_populated_csv_stats() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: Some(create_populated_stats_tracker()),
            json_output: None,
            csv_output: None,
            csv_stats_output: Some(crate::csv_output::CsvStatsOutput::new()),
            html_output: None,
            anomaly_detector: None,
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: false,
            ml_clusters: 5,
            ml_compare: false,
            ml_outliers: false,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 100,
            dl_anomaly: false,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 50,
            explain: false,
        };
        print_summaries(tracers, true, 0, &analysis);
    }

    #[test]
    fn test_print_summaries_populated_json() {
        let tracers = Tracers {
            profiling_ctx: None,
            function_profiler: None,
            stats_tracker: Some(create_populated_stats_tracker()),
            json_output: Some(crate::json_output::JsonOutput::new()),
            csv_output: None,
            csv_stats_output: None,
            html_output: None,
            anomaly_detector: None,
            decision_tracer: None,
            #[cfg(feature = "otlp")]
            otlp_exporter: None,
        };
        let analysis = AnalysisConfig {
            stats_extended: true,
            anomaly_threshold: 2.0,
            hpu_analysis: false,
            hpu_cpu_only: false,
            ml_anomaly: true,
            ml_clusters: 3,
            ml_compare: false,
            ml_outliers: true,
            ml_outlier_threshold: 0.1,
            ml_outlier_trees: 50,
            dl_anomaly: true,
            dl_threshold: 2.0,
            dl_hidden_size: 8,
            dl_epochs: 10,
            explain: true,
        };
        print_summaries(tracers, true, 0, &analysis);
    }
}
