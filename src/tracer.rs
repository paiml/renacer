//! System call tracing using ptrace
//!
//! Sprint 3-4: Trace all syscalls with name resolution

use anyhow::{Context, Result};
use nix::sys::ptrace;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::os::unix::process::CommandExt;
use std::process::Command;

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
}

/// Attach to a running process by PID and trace syscalls
///
/// # Sprint 9-10 Scope
/// - `-p PID` flag to attach to running processes
/// - Uses PTRACE_ATTACH instead of fork() + PTRACE_TRACEME
pub fn attach_to_pid(pid: i32, config: TracerConfig) -> Result<()> {
    let pid = Pid::from_raw(pid);

    // Attach to the running process
    ptrace::attach(pid).context(format!("Failed to attach to PID {}", pid))?;

    // Wait for SIGSTOP from PTRACE_ATTACH
    waitpid(pid, None).context("Failed to wait for attach signal")?;

    eprintln!("[renacer: Attached to process {}]", pid);

    // Use the same tracing logic as trace_command
    trace_child(pid, config)?;

    Ok(())
}

/// Trace a command and print syscalls to stdout
///
/// # Sprint 3-4 Scope
/// - Intercept ALL syscalls
/// - Resolve syscall number → name
/// - Print format: `syscall_name(args...) = result`
///
/// # Sprint 5-6 Scope
/// - Optional source correlation with DWARF debug info
///
/// # Sprint 9-10 Scope
/// - Syscall filtering via -e trace= expressions
/// - Statistics mode via -c flag
/// - Timing per syscall via -T flag
/// - JSON output via --format json
/// - Fork following via -f flag
pub fn trace_command(command: &[String], config: TracerConfig) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("Command array is empty");
    }

    let program = &command[0];
    let args = &command[1..];

    // Fork: parent will trace, child will exec
    match unsafe { fork() }.context("Failed to fork")? {
        ForkResult::Parent { child } => {
            trace_child(child, config)?;
            Ok(())
        }
        ForkResult::Child => {
            // Child: allow tracing and exec target program
            ptrace::traceme().context("Failed to PTRACE_TRACEME")?;

            // Use std::process::Command for exec
            let err = Command::new(program).args(args).exec();

            // If we get here, exec failed
            eprintln!("Failed to exec {}: {}", program, err);
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
}

/// Initialize all tracers and profilers based on config
fn initialize_tracers(config: &TracerConfig) -> Tracers {
    use crate::cli::OutputFormat;

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

    let stats_tracker = if config.statistics_mode {
        Some(crate::stats::StatsTracker::new())
    } else {
        None
    };

    let json_output = if matches!(config.output_format, OutputFormat::Json) {
        Some(crate::json_output::JsonOutput::new())
    } else {
        None
    };

    Tracers {
        profiling_ctx,
        function_profiler,
        stats_tracker,
        json_output,
    }
}

/// Initialize ptrace options for the child process
fn setup_ptrace_options(child: Pid, follow_forks: bool) -> Result<()> {
    // Wait for initial SIGSTOP from PTRACE_TRACEME
    waitpid(child, None).context("Failed to wait for child")?;

    // Set ptrace options to trace syscalls
    let mut options = ptrace::Options::PTRACE_O_TRACESYSGOOD | ptrace::Options::PTRACE_O_EXITKILL;

    // Add fork following options if enabled
    if follow_forks {
        options |= ptrace::Options::PTRACE_O_TRACEFORK
            | ptrace::Options::PTRACE_O_TRACEVFORK
            | ptrace::Options::PTRACE_O_TRACECLONE;
    }

    ptrace::setoptions(child, options).context("Failed to set ptrace options")?;
    Ok(())
}

/// Load DWARF debug info for source correlation
fn load_dwarf_context(child: Pid) -> Option<crate::dwarf::DwarfContext> {
    if let Ok(exe_path) = std::fs::read_link(format!("/proc/{}/exe", child)) {
        match crate::dwarf::DwarfContext::load(&exe_path) {
            Ok(ctx) => {
                eprintln!(
                    "[renacer: DWARF debug info loaded from {}]",
                    exe_path.display()
                );
                Some(ctx)
            }
            Err(e) => {
                eprintln!("[renacer: Warning - failed to load DWARF: {}]", e);
                eprintln!("[renacer: Continuing without source correlation]");
                None
            }
        }
    } else {
        None
    }
}

/// Handle process exit or signal termination
fn handle_wait_status(status: WaitStatus) -> Option<i32> {
    match status {
        WaitStatus::Exited(_, code) => Some(code),
        WaitStatus::Signaled(_, sig, _) => {
            eprintln!("Child killed by signal: {:?}", sig);
            Some(128 + sig as i32)
        }
        _ => None,
    }
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
    let in_json_mode = tracers.json_output.is_some();

    if !*in_syscall {
        // Syscall entry - record start time if timing enabled
        if config.timing_mode || config.statistics_mode || in_json_mode {
            *syscall_entry_time = Some(std::time::Instant::now());
        }

        *current_syscall_entry = process_syscall_entry(
            child,
            dwarf_ctx,
            config,
            tracers.profiling_ctx.as_mut(),
            in_json_mode,
        )?;
        *in_syscall = true;
    } else {
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
    }
    Ok(())
}

/// Process syscall entry event
fn process_syscall_entry(
    child: Pid,
    dwarf_ctx: Option<&crate::dwarf::DwarfContext>,
    config: &TracerConfig,
    profiling_ctx: Option<&mut crate::profiling::ProfilingContext>,
    in_json_mode: bool,
) -> Result<Option<SyscallEntry>> {
    if let Some(prof) = profiling_ctx {
        prof.measure(crate::profiling::ProfilingCategory::Other, || {
            handle_syscall_entry(
                child,
                dwarf_ctx,
                &config.filter,
                config.statistics_mode,
                in_json_mode,
                config.function_time,
            )
        })
    } else {
        handle_syscall_entry(
            child,
            dwarf_ctx,
            &config.filter,
            config.statistics_mode,
            in_json_mode,
            config.function_time,
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
    if let Some(prof) = tracers.profiling_ctx.as_mut() {
        let result = prof.measure(crate::profiling::ProfilingCategory::Other, || {
            handle_syscall_exit(
                child,
                current_syscall_entry,
                tracers.stats_tracker.as_mut(),
                tracers.json_output.as_mut(),
                tracers.function_profiler.as_mut(),
                timing_mode,
                duration_us,
            )
        });
        result?;
        prof.record_syscall();
        Ok(())
    } else {
        handle_syscall_exit(
            child,
            current_syscall_entry,
            tracers.stats_tracker.as_mut(),
            tracers.json_output.as_mut(),
            tracers.function_profiler.as_mut(),
            timing_mode,
            duration_us,
        )
    }
}

/// Print all summaries at end of tracing
fn print_summaries(
    stats_tracker: Option<crate::stats::StatsTracker>,
    json_output: Option<crate::json_output::JsonOutput>,
    profiling_ctx: Option<crate::profiling::ProfilingContext>,
    function_profiler: Option<crate::function_profiler::FunctionProfiler>,
    exit_code: i32,
) {
    // Print statistics summary if in statistics mode
    if let Some(tracker) = stats_tracker {
        tracker.print_summary();
    }

    // Print JSON output if in JSON mode
    if let Some(mut output) = json_output {
        output.set_exit_code(exit_code);
        match output.to_json() {
            Ok(json) => println!("{}", json),
            Err(e) => eprintln!("Failed to serialize JSON: {}", e),
        }
    }

    // Print profiling summary if enabled
    if let Some(ctx) = profiling_ctx {
        ctx.print_summary();
    }

    // Print function profiling summary if enabled
    if let Some(profiler) = function_profiler {
        profiler.print_summary();
    }
}

/// Trace a child process, filtering syscalls based on filter
fn trace_child(child: Pid, config: TracerConfig) -> Result<i32> {
    // Initialize all tracers and profilers
    let mut tracers = initialize_tracers(&config);

    // Setup ptrace options
    setup_ptrace_options(child, config.follow_forks)?;

    let mut in_syscall = false;
    let mut current_syscall_entry: Option<SyscallEntry> = None;
    let mut syscall_entry_time: Option<std::time::Instant> = None;
    let mut dwarf_ctx: Option<crate::dwarf::DwarfContext> = None;
    let mut dwarf_loaded = false;
    let exit_code;

    loop {
        // Sprint 5-6: Load DWARF context on first syscall if --source is enabled
        // We wait until after exec() has happened to get the right binary
        if config.enable_source && !dwarf_loaded {
            dwarf_loaded = true; // Only try once
            dwarf_ctx = load_dwarf_context(child);
        }

        // Continue and wait for next syscall or exit
        ptrace::syscall(child, None).context("Failed to PTRACE_SYSCALL")?;

        let status = waitpid(child, None).context("Failed to waitpid")?;

        // Check if process exited or was signaled
        if let Some(code) = handle_wait_status(status) {
            exit_code = code;
            break;
        }

        // Handle syscall events
        if matches!(status, WaitStatus::PtraceSyscall(_)) {
            handle_syscall_event(
                child,
                &mut in_syscall,
                &mut current_syscall_entry,
                &mut syscall_entry_time,
                dwarf_ctx.as_ref(),
                &config,
                &mut tracers,
            )?;
        }
    }

    // Print all summaries
    print_summaries(
        tracers.stats_tracker,
        tracers.json_output,
        tracers.profiling_ctx,
        tracers.function_profiler,
        exit_code,
    );

    // Exit with traced program's exit code
    std::process::exit(exit_code);
}

/// Syscall entry data for JSON output
struct SyscallEntry {
    name: String,
    args: Vec<String>,
    source: Option<crate::json_output::JsonSourceLocation>,
    function_name: Option<String>,
    caller_name: Option<String>,
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
/// Returns (current_function, caller_function)
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
                    user_functions.push(func_name.to_string());
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
                read_string(child, arg2 as usize).unwrap_or_else(|_| format!("{:#x}", arg2));
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
fn print_syscall_entry(
    child: Pid,
    name: &str,
    syscall_num: i64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    source_info: &Option<crate::dwarf::SourceLocation>,
) {
    // Print source location if available
    if let Some(src) = source_info {
        print!("{}:{} ", src.file, src.line);
        if let Some(func) = &src.function {
            print!("{} ", func);
        }
    }

    // Print syscall with arguments
    match name {
        "openat" => {
            let filename =
                read_string(child, arg2 as usize).unwrap_or_else(|_| format!("{:#x}", arg2));
            print!("{}({:#x}, \"{}\", {:#x}) = ", name, arg1, filename, arg3);
        }
        "unknown" => {
            print!(
                "syscall_{}({:#x}, {:#x}, {:#x}) = ",
                syscall_num, arg1, arg2, arg3
            );
        }
        _ => {
            print!("{}({:#x}, {:#x}, {:#x}) = ", name, arg1, arg2, arg3);
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
            let func = source_info
                .as_ref()
                .and_then(|src| src.function.clone().map(|s| s.to_string()));
            (func, None)
        }
    } else {
        let func = source_info
            .as_ref()
            .and_then(|src| src.function.clone().map(|s| s.to_string()));
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
    json_mode: bool,
    function_profiling_enabled: bool,
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

    // Format arguments for JSON mode if needed
    let args = if json_mode {
        format_syscall_args_for_json(child, name, arg1, arg2, arg3)
    } else {
        Vec::new()
    };

    // Print syscall entry if not in statistics or JSON mode
    if !statistics_mode && !json_mode {
        print_syscall_entry(child, name, syscall_num, arg1, arg2, arg3, &source_info);
    }

    // Extract function names for profiling
    let (function_name, caller_name) =
        extract_function_names(child, dwarf_ctx, &source_info, function_profiling_enabled);

    let json_source = source_info
        .as_ref()
        .map(|src| crate::json_output::JsonSourceLocation {
            file: src.file.to_string(),
            line: src.line,
            function: src.function.clone().map(|s| s.to_string()),
        });

    // Return syscall entry data
    Ok(Some(SyscallEntry {
        name: name.to_string(),
        args,
        source: json_source,
        function_name,
        caller_name,
    }))
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
        println!("{}", result);
    }
}

/// Handle syscall exit - print return value and record statistics
fn handle_syscall_exit(
    child: Pid,
    syscall_entry: &Option<SyscallEntry>,
    stats_tracker: Option<&mut crate::stats::StatsTracker>,
    json_output: Option<&mut crate::json_output::JsonOutput>,
    function_profiler: Option<&mut crate::function_profiler::FunctionProfiler>,
    timing_mode: bool,
    duration_us: u64,
) -> Result<()> {
    let regs = ptrace::getregs(child).context("Failed to get registers")?;
    let result = regs.rax as i64;

    // Check modes before borrowing
    let in_stats_mode = stats_tracker.is_some();
    let in_json_mode = json_output.is_some();

    // Record statistics
    record_stats_for_syscall(syscall_entry, stats_tracker, result, duration_us);

    // Record JSON output
    record_json_for_syscall(syscall_entry, json_output, result, timing_mode, duration_us);

    // Record function profiling
    record_function_profiling(syscall_entry, function_profiler, duration_us);

    // Print result if not in statistics or JSON mode
    if syscall_entry.is_some() && !in_stats_mode && !in_json_mode {
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
        };
        let result = trace_command(&empty, config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_trace_command_not_implemented_yet() {
        // RED phase: this should fail until we implement
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
        };
        let result = trace_command(&cmd, config);
        assert!(result.is_err());
    }

    #[test]
    fn test_syscall_entry_creation() {
        let entry = SyscallEntry {
            name: "open".to_string(),
            args: vec!["arg1".to_string(), "arg2".to_string()],
            source: None,
            function_name: None,
            caller_name: None,
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
        };
        assert_eq!(entry.name, "read");
        assert!(entry.source.is_some());
        let src = entry.source.unwrap();
        assert_eq!(src.file, "test.rs");
        assert_eq!(src.line, 42);
        assert_eq!(src.function, Some("main".to_string()));
        assert_eq!(entry.function_name, Some("main".to_string()));
    }

    #[test]
    fn test_attach_to_pid_invalid_pid() {
        // Test attaching to a non-existent PID (should fail)
        let config = TracerConfig {
            enable_source: false,
            filter: crate::filter::SyscallFilter::all(),
            statistics_mode: false,
            timing_mode: false,
            output_format: crate::cli::OutputFormat::Text,
            follow_forks: false,
            profile_self: false,
            function_time: false,
        };
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
}
