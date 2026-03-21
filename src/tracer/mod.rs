//! System call tracing using ptrace
//!
//! Sprint 3-4: Trace all syscalls with name resolution

mod ml_analysis;
mod output;
mod syscall_handling;

use anyhow::{Context, Result};
use nix::sys::ptrace;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::os::unix::process::CommandExt;
use std::process::Command;
use tracing::{info, trace, warn};

/// Real-time syscall event for visualization (Sprint 52-55)
#[derive(Debug, Clone)]
pub struct VisualizerEvent {
    /// Syscall name
    pub name: String,
    /// Duration in microseconds
    pub duration_us: u64,
    /// Return value (negative for errors)
    pub result: i64,
    /// Process ID
    pub pid: i32,
}

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
    pub anomaly_threshold: f32, // Sprint 19: Anomaly detection threshold (sigma)
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
    pub dl_threshold: f32,      // Sprint 23: Reconstruction error threshold (sigma multiplier)
    pub dl_hidden_size: usize,  // Sprint 23: Autoencoder hidden layer size
    pub dl_epochs: usize,       // Sprint 23: Training epochs
    pub trace_transpiler_decisions: bool, // Sprint 26: Trace transpiler compile-time decisions
    pub transpiler_map: Option<crate::transpiler_map::TranspilerMap>, // Sprint 24-28: Transpiler source mapping
    pub otlp_endpoint: Option<String>, // Sprint 30: OpenTelemetry OTLP endpoint
    pub otlp_service_name: String,     // Sprint 30: Service name for OTLP traces
    pub trace_parent: Option<String>,  // Sprint 33: W3C Trace Context for distributed tracing
    pub chaos_config: Option<crate::chaos::ChaosConfig>, // Sprint 47: Chaos engineering (Issue #17)
    /// Sprint 52-55: Event sink for real-time visualization
    pub visualizer_sink: Option<std::sync::mpsc::Sender<VisualizerEvent>>,
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
            visualizer_sink: None,
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
#[allow(unsafe_code)]
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
    decision_tracer: Option<crate::decision_trace::DecisionTracer>, // Sprint 26
    #[cfg(feature = "otlp")]
    otlp_exporter: Option<crate::otlp_exporter::OtlpExporter>, // Sprint 30
    /// Sprint 52-55: Event sink for real-time visualization
    visualizer_sink: Option<std::sync::mpsc::Sender<VisualizerEvent>>,
}

/// Initialize profiling-related tracers
fn initialize_profiling_tracers(
    config: &TracerConfig,
) -> (
    Option<crate::profiling::ProfilingContext>,
    Option<crate::function_profiler::FunctionProfiler>,
    Option<crate::anomaly::AnomalyDetector>,
) {
    let profiling_ctx =
        if config.profile_self { Some(crate::profiling::ProfilingContext::new()) } else { None };

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
        Some(crate::csv_output::CsvOutput::new(config.timing_mode, config.enable_source))
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
        Some(crate::html_output::HtmlOutput::new(config.timing_mode, config.enable_source))
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
        visualizer_sink: config.visualizer_sink.clone(),
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
                eprintln!("[renacer: DWARF debug info loaded from {}]", exe_path.display());
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
                        warn!("Failed to setup ptrace options for child {}: {}", new_pid, e);
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
        let duration_us =
            syscall_entry_time.map(|start| start.elapsed().as_micros() as u64).unwrap_or(0);

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
            syscall_handling::handle_syscall_entry(
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
        syscall_handling::handle_syscall_entry(
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
            syscall_handling::handle_syscall_exit(
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
        syscall_handling::handle_syscall_exit(
            child,
            current_syscall_entry,
            tracers,
            timing_mode,
            duration_us,
        )
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
    dl_threshold: f32,         // Sprint 23: Reconstruction error threshold (sigma multiplier)
    dl_hidden_size: usize,     // Sprint 23: Hidden layer size
    dl_epochs: usize,          // Sprint 23: Training epochs
    explain: bool,             // Sprint 22/23: Enable explainability
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

/// Start the OTLP root span if the feature is enabled and exporter is configured.
#[cfg(feature = "otlp")]
fn start_otlp_root_span(tracers: &mut Tracers, child: Pid) {
    if let Some(ref mut exporter) = tracers.otlp_exporter {
        let program_name = std::fs::read_to_string(format!("/proc/{child}/cmdline"))
            .ok()
            .and_then(|s| s.split('\0').next().map(std::string::ToString::to_string))
            .unwrap_or_else(|| format!("pid:{child}"));
        exporter.start_root_span(&program_name, child.as_raw());
    }
}

/// Build analysis configuration from tracer configuration.
fn build_analysis_config(config: &TracerConfig) -> AnalysisConfig {
    AnalysisConfig {
        stats_extended: config.stats_extended,
        anomaly_threshold: config.anomaly_threshold,
        hpu_analysis: config.hpu_analysis,
        hpu_cpu_only: config.hpu_cpu_only,
        ml_anomaly: config.ml_anomaly,
        ml_clusters: config.ml_clusters,
        ml_compare: config.ml_compare,
        ml_outliers: config.ml_outliers,
        ml_outlier_threshold: config.ml_outlier_threshold,
        ml_outlier_trees: config.ml_outlier_trees,
        dl_anomaly: config.dl_anomaly,
        dl_threshold: config.dl_threshold,
        dl_hidden_size: config.dl_hidden_size,
        dl_epochs: config.dl_epochs,
        explain: config.explain,
    }
}

/// Wait for next process event, returning the wait status.
/// Returns `Ok(None)` when all processes have exited.
fn wait_for_event(
    config: &TracerConfig,
    child: Pid,
    processes: &std::collections::HashMap<Pid, ProcessState>,
) -> Result<Option<nix::sys::wait::WaitStatus>> {
    let wait_result =
        if config.follow_forks { waitpid(Pid::from_raw(-1), None) } else { waitpid(child, None) };

    match wait_result {
        Ok(s) => {
            trace!(status = ?s, "waitpid returned");
            Ok(Some(s))
        }
        Err(_) if processes.is_empty() => {
            trace!("waitpid error but processes empty, breaking");
            Ok(None)
        }
        Err(e) => {
            warn!(error = %e, "waitpid failed");
            Err(e).context("Failed to waitpid")
        }
    }
}

/// Trace a child process, filtering syscalls based on filter
fn trace_child(child: Pid, config: TracerConfig) -> Result<i32> {
    info!(pid = %child, "starting trace_child");

    let mut tracers = initialize_tracers(&config);
    trace!("tracers initialized");

    #[cfg(feature = "otlp")]
    start_otlp_root_span(&mut tracers, child);

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
        let Some(status) = wait_for_event(&config, child, &processes)? else {
            break;
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

    output::print_summaries(
        tracers,
        config.timing_mode,
        main_exit_code,
        &build_analysis_config(&config),
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
    _raw_arg3: Option<u64>,
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
            visualizer_sink: None,                    // Sprint 52-55
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
            visualizer_sink: None,                    // Sprint 52-55
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
            _raw_arg3: Some(3),
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
            _raw_arg3: Some(0),
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
        assert!(err_msg.contains("attach") || err_msg.contains("Failed"), "Error: {}", err_msg);
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
            _raw_arg3: Some(10),
        };
        assert_eq!(entry.raw_arg1, Some(1));
        assert_eq!(entry.raw_arg2, Some(0x7fff_0000_0000));
        assert_eq!(entry._raw_arg3, Some(10));
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
            _raw_arg3: None,
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
            visualizer_sink: None, // Sprint 52-55
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
        output::print_text_stats(&stats_tracker, false, 2.0, None);
        #[cfg(not(feature = "otlp"))]
        output::print_text_stats(&stats_tracker, false, 2.0, None);
        // Should not panic with None
    }

    #[test]
    fn test_print_text_stats_with_tracker() {
        let stats_tracker = Some(crate::stats::StatsTracker::new());
        #[cfg(feature = "otlp")]
        output::print_text_stats(&stats_tracker, false, 2.0, None);
        #[cfg(not(feature = "otlp"))]
        output::print_text_stats(&stats_tracker, false, 2.0, None);
    }

    #[test]
    fn test_print_text_stats_extended() {
        let stats_tracker = Some(crate::stats::StatsTracker::new());
        #[cfg(feature = "otlp")]
        output::print_text_stats(&stats_tracker, true, 3.0, None);
        #[cfg(not(feature = "otlp"))]
        output::print_text_stats(&stats_tracker, true, 3.0, None);
    }

    #[test]
    fn test_print_json_output() {
        let json_out = crate::json_output::JsonOutput::new();
        // Should not panic
        output::print_json_output(json_out, 0);
    }

    #[test]
    fn test_print_json_output_nonzero_exit() {
        let json_out = crate::json_output::JsonOutput::new();
        output::print_json_output(json_out, 1);
    }

    #[test]
    fn test_generate_ml_analysis_none() {
        let result = ml_analysis::generate_ml_analysis_for_json(&None, 5);
        assert!(result.is_none());
    }

    #[test]
    fn test_generate_ml_analysis_empty_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result = ml_analysis::generate_ml_analysis_for_json(&tracker, 3);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_isolation_forest_none() {
        let result =
            ml_analysis::generate_isolation_forest_analysis_for_json(&None, 100, 0.1, false);
        assert!(result.is_none());
    }

    #[test]
    fn test_generate_isolation_forest_empty_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result =
            ml_analysis::generate_isolation_forest_analysis_for_json(&tracker, 50, 0.1, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_isolation_forest_with_explain() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result =
            ml_analysis::generate_isolation_forest_analysis_for_json(&tracker, 50, 0.1, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_autoencoder_none() {
        let result = ml_analysis::generate_autoencoder_analysis_for_json(&None, 8, 50, 2.0, false);
        assert!(result.is_none());
    }

    #[test]
    fn test_generate_autoencoder_empty_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result =
            ml_analysis::generate_autoencoder_analysis_for_json(&tracker, 8, 50, 2.0, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_autoencoder_with_explain() {
        let tracker = Some(crate::stats::StatsTracker::new());
        let result =
            ml_analysis::generate_autoencoder_analysis_for_json(&tracker, 8, 50, 2.0, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_print_csv_stats_none() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        output::print_csv_stats(csv_stats, &None, false, false, 2.0);
    }

    #[test]
    fn test_print_csv_stats_with_tracker() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        output::print_csv_stats(csv_stats, &tracker, false, false, 2.0);
    }

    #[test]
    fn test_print_csv_stats_with_timing() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        output::print_csv_stats(csv_stats, &tracker, true, false, 2.0);
    }

    #[test]
    fn test_print_csv_stats_extended() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(crate::stats::StatsTracker::new());
        output::print_csv_stats(csv_stats, &tracker, true, true, 3.0);
    }

    #[test]
    fn test_print_hpu_analysis_none() {
        output::print_hpu_analysis(&None, false);
    }

    #[test]
    fn test_print_hpu_analysis_cpu_only() {
        let tracker = Some(crate::stats::StatsTracker::new());
        output::print_hpu_analysis(&tracker, true);
    }

    #[test]
    fn test_print_hpu_analysis_with_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        output::print_hpu_analysis(&tracker, false);
    }

    #[test]
    fn test_print_ml_analysis_none() {
        ml_analysis::print_ml_analysis(&None, 5, false, 2.0);
    }

    #[test]
    fn test_print_ml_analysis_with_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        ml_analysis::print_ml_analysis(&tracker, 5, false, 2.0);
    }

    #[test]
    fn test_print_ml_analysis_compare() {
        let tracker = Some(crate::stats::StatsTracker::new());
        ml_analysis::print_ml_analysis(&tracker, 5, true, 2.0);
    }

    #[test]
    fn test_print_isolation_forest_none() {
        ml_analysis::print_isolation_forest_analysis(&None, 100, 0.1, false);
    }

    #[test]
    fn test_print_isolation_forest_with_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        ml_analysis::print_isolation_forest_analysis(&tracker, 100, 0.1, false);
    }

    #[test]
    fn test_print_isolation_forest_with_explain() {
        let tracker = Some(crate::stats::StatsTracker::new());
        ml_analysis::print_isolation_forest_analysis(&tracker, 100, 0.1, true);
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
        ml_analysis::print_autoencoder_analysis(&None, 8, 50, 2.0, false);
    }

    #[test]
    fn test_print_autoencoder_with_tracker() {
        let tracker = Some(crate::stats::StatsTracker::new());
        ml_analysis::print_autoencoder_analysis(&tracker, 8, 50, 2.0, false);
    }

    #[test]
    fn test_print_autoencoder_with_explain() {
        let tracker = Some(crate::stats::StatsTracker::new());
        ml_analysis::print_autoencoder_analysis(&tracker, 8, 50, 2.0, true);
    }

    #[test]
    fn test_print_optional_summaries_all_none() {
        output::print_optional_summaries(None, None, None);
    }

    #[test]
    fn test_print_optional_summaries_with_profiling() {
        let ctx = Some(crate::profiling::ProfilingContext::new());
        output::print_optional_summaries(ctx, None, None);
    }

    #[test]
    fn test_print_optional_summaries_with_function_profiler() {
        let profiler = Some(crate::function_profiler::FunctionProfiler::new());
        output::print_optional_summaries(None, profiler, None);
    }

    #[test]
    fn test_print_optional_summaries_with_anomaly_detector() {
        let detector = Some(crate::anomaly::AnomalyDetector::new(100, 2.0));
        output::print_optional_summaries(None, None, detector);
    }

    #[test]
    fn test_print_optional_summaries_all_some() {
        let ctx = Some(crate::profiling::ProfilingContext::new());
        let profiler = Some(crate::function_profiler::FunctionProfiler::new());
        let detector = Some(crate::anomaly::AnomalyDetector::new(100, 2.0));
        output::print_optional_summaries(ctx, profiler, detector);
    }

    #[test]
    fn test_print_decision_trace_summary_none() {
        output::print_decision_trace_summary(None);
    }

    #[test]
    fn test_print_decision_trace_summary_empty() {
        let tracer = Some(crate::decision_trace::DecisionTracer::new());
        output::print_decision_trace_summary(tracer);
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
        output::print_analysis_summaries(&None, &analysis);
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
        output::print_analysis_summaries(&tracker, &analysis);
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
        output::print_analysis_summaries(&tracker, &analysis);
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
        output::print_analysis_summaries(&tracker, &analysis);
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
        output::print_analysis_summaries(&tracker, &analysis);
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
        output::print_analysis_summaries(&tracker, &analysis);
    }

    #[test]
    fn test_handle_json_output_basic() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &None, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_with_ml() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_with_outliers() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_with_dl() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_all_analysis() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 1);
    }

    // should_print_result tests
    #[test]
    fn test_should_print_result_none_entry() {
        assert!(!output::should_print_result(&None, false, false, false, false));
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
            _raw_arg3: None,
        });
        assert!(output::should_print_result(&entry, false, false, false, false));
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
            _raw_arg3: None,
        });
        assert!(!output::should_print_result(&entry, true, false, false, false));
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
            _raw_arg3: None,
        });
        assert!(!output::should_print_result(&entry, false, true, false, false));
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
            _raw_arg3: None,
        });
        assert!(!output::should_print_result(&entry, false, false, true, false));
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
            _raw_arg3: None,
        });
        assert!(!output::should_print_result(&entry, false, false, false, true));
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
            _raw_arg3: None,
        });
        assert!(!output::should_print_result(&entry, true, true, true, true));
    }

    // print_syscall_result tests
    #[test]
    fn test_print_syscall_result_success() {
        output::print_syscall_result(0, false, 0);
    }

    #[test]
    fn test_print_syscall_result_error() {
        output::print_syscall_result(-1, false, 0);
    }

    #[test]
    fn test_print_syscall_result_with_timing() {
        output::print_syscall_result(42, true, 1234);
    }

    #[test]
    fn test_print_syscall_result_negative_with_timing() {
        output::print_syscall_result(-22, true, 5678);
    }

    // record_stats_for_syscall tests
    #[test]
    fn test_record_stats_for_syscall_none_entry() {
        let mut tracker = Some(crate::stats::StatsTracker::new());
        syscall_handling::record_stats_for_syscall_test(&None, tracker.as_mut(), 0, 100);
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
            _raw_arg3: None,
        });
        syscall_handling::record_stats_for_syscall_test(&entry, None, 0, 100);
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
            _raw_arg3: None,
        });
        let mut tracker = Some(crate::stats::StatsTracker::new());
        syscall_handling::record_stats_for_syscall_test(&entry, tracker.as_mut(), 100, 500);
    }

    // record_function_profiling tests
    #[test]
    fn test_record_function_profiling_none_entry() {
        let mut profiler = Some(crate::function_profiler::FunctionProfiler::new());
        syscall_handling::record_function_profiling_test(&None, profiler.as_mut(), 100);
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
            _raw_arg3: None,
        });
        syscall_handling::record_function_profiling_test(&entry, None, 100);
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
            _raw_arg3: None,
        });
        let mut profiler = Some(crate::function_profiler::FunctionProfiler::new());
        syscall_handling::record_function_profiling_test(&entry, profiler.as_mut(), 100);
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
            _raw_arg3: None,
        });
        let mut profiler = Some(crate::function_profiler::FunctionProfiler::new());
        syscall_handling::record_function_profiling_test(&entry, profiler.as_mut(), 100);
    }

    // handle_anomaly_detection tests
    #[test]
    fn test_handle_anomaly_detection_none_entry() {
        let mut detector = Some(crate::anomaly::AnomalyDetector::new(100, 2.0));
        syscall_handling::handle_anomaly_detection_test(&None, detector.as_mut(), 100);
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
            _raw_arg3: None,
        });
        syscall_handling::handle_anomaly_detection_test(&entry, None, 100);
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
            _raw_arg3: None,
        });
        let mut detector = Some(crate::anomaly::AnomalyDetector::new(100, 2.0));
        syscall_handling::handle_anomaly_detection_test(&entry, detector.as_mut(), 100);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, false, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, true, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, false, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, true, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, false, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, false, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, false, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, true, 0, &analysis);
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
        let result = ml_analysis::generate_ml_analysis_for_json(&tracker, 3);
        assert!(result.is_some());
        let report = result.expect("test");
        assert!(report.total_samples > 0);
    }

    #[test]
    fn test_generate_isolation_forest_populated() {
        let tracker = Some(create_populated_stats_tracker());
        let result =
            ml_analysis::generate_isolation_forest_analysis_for_json(&tracker, 50, 0.1, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_isolation_forest_populated_with_explain() {
        let tracker = Some(create_populated_stats_tracker());
        let result =
            ml_analysis::generate_isolation_forest_analysis_for_json(&tracker, 50, 0.1, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_autoencoder_populated() {
        let tracker = Some(create_populated_stats_tracker());
        let result =
            ml_analysis::generate_autoencoder_analysis_for_json(&tracker, 8, 10, 2.0, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_generate_autoencoder_populated_with_explain() {
        let tracker = Some(create_populated_stats_tracker());
        let result =
            ml_analysis::generate_autoencoder_analysis_for_json(&tracker, 8, 10, 2.0, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_print_text_stats_populated() {
        let tracker = Some(create_populated_stats_tracker());
        #[cfg(feature = "otlp")]
        output::print_text_stats(&tracker, false, 2.0, None);
        #[cfg(not(feature = "otlp"))]
        output::print_text_stats(&tracker, false, 2.0, None);
    }

    #[test]
    fn test_print_text_stats_populated_extended() {
        let tracker = Some(create_populated_stats_tracker());
        #[cfg(feature = "otlp")]
        output::print_text_stats(&tracker, true, 2.0, None);
        #[cfg(not(feature = "otlp"))]
        output::print_text_stats(&tracker, true, 2.0, None);
    }

    #[test]
    fn test_print_csv_stats_populated() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        output::print_csv_stats(csv_stats, &tracker, false, false, 2.0);
    }

    #[test]
    fn test_print_csv_stats_populated_extended() {
        let csv_stats = crate::csv_output::CsvStatsOutput::new();
        let tracker = Some(create_populated_stats_tracker());
        output::print_csv_stats(csv_stats, &tracker, true, true, 2.0);
    }

    #[test]
    fn test_print_hpu_analysis_populated() {
        let tracker = Some(create_populated_stats_tracker());
        output::print_hpu_analysis(&tracker, false);
    }

    #[test]
    fn test_print_hpu_analysis_populated_cpu_only() {
        let tracker = Some(create_populated_stats_tracker());
        output::print_hpu_analysis(&tracker, true);
    }

    #[test]
    fn test_print_ml_analysis_populated() {
        let tracker = Some(create_populated_stats_tracker());
        ml_analysis::print_ml_analysis(&tracker, 3, false, 2.0);
    }

    #[test]
    fn test_print_ml_analysis_populated_compare() {
        let tracker = Some(create_populated_stats_tracker());
        ml_analysis::print_ml_analysis(&tracker, 3, true, 2.0);
    }

    #[test]
    fn test_print_isolation_forest_populated() {
        let tracker = Some(create_populated_stats_tracker());
        ml_analysis::print_isolation_forest_analysis(&tracker, 50, 0.1, false);
    }

    #[test]
    fn test_print_isolation_forest_populated_explain() {
        let tracker = Some(create_populated_stats_tracker());
        ml_analysis::print_isolation_forest_analysis(&tracker, 50, 0.1, true);
    }

    #[test]
    fn test_print_autoencoder_populated() {
        let tracker = Some(create_populated_stats_tracker());
        ml_analysis::print_autoencoder_analysis(&tracker, 8, 10, 2.0, false);
    }

    #[test]
    fn test_print_autoencoder_populated_explain() {
        let tracker = Some(create_populated_stats_tracker());
        ml_analysis::print_autoencoder_analysis(&tracker, 8, 10, 2.0, true);
    }

    #[test]
    fn test_handle_json_output_populated() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_populated_ml() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_populated_outliers() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_populated_dl() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 0);
    }

    #[test]
    fn test_handle_json_output_populated_all() {
        let json_out = crate::json_output::JsonOutput::new();
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
        output::handle_json_output(json_out, &tracker, &analysis, 1);
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
        output::print_analysis_summaries(&tracker, &analysis);
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
        output::print_analysis_summaries(&tracker, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, true, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, true, 0, &analysis);
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
            visualizer_sink: None,
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
        output::print_summaries(tracers, true, 0, &analysis);
    }
}
