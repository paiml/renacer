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

/// Attach to a running process by PID and trace syscalls
///
/// # Sprint 9-10 Scope
/// - `-p PID` flag to attach to running processes
/// - Uses PTRACE_ATTACH instead of fork() + PTRACE_TRACEME
pub fn attach_to_pid(pid: i32, enable_source: bool, filter: crate::filter::SyscallFilter, statistics_mode: bool, timing_mode: bool, output_format: crate::cli::OutputFormat, follow_forks: bool) -> Result<()> {
    let pid = Pid::from_raw(pid);

    // Attach to the running process
    ptrace::attach(pid).context(format!("Failed to attach to PID {}", pid))?;

    // Wait for SIGSTOP from PTRACE_ATTACH
    waitpid(pid, None).context("Failed to wait for attach signal")?;

    eprintln!("[renacer: Attached to process {}]", pid);

    // Use the same tracing logic as trace_command
    trace_child(pid, enable_source, filter, statistics_mode, timing_mode, output_format, follow_forks)?;

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
pub fn trace_command(command: &[String], enable_source: bool, filter: crate::filter::SyscallFilter, statistics_mode: bool, timing_mode: bool, output_format: crate::cli::OutputFormat, follow_forks: bool) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("Command array is empty");
    }

    let program = &command[0];
    let args = &command[1..];

    // Fork: parent will trace, child will exec
    match unsafe { fork() }.context("Failed to fork")? {
        ForkResult::Parent { child } => {
            trace_child(child, enable_source, filter, statistics_mode, timing_mode, output_format, follow_forks)?;
            Ok(())
        }
        ForkResult::Child => {
            // Child: allow tracing and exec target program
            ptrace::traceme().context("Failed to PTRACE_TRACEME")?;

            // Use std::process::Command for exec
            let err = Command::new(program)
                .args(args)
                .exec();

            // If we get here, exec failed
            eprintln!("Failed to exec {}: {}", program, err);
            std::process::exit(1);
        }
    }
}

/// Trace a child process, filtering syscalls based on filter
fn trace_child(child: Pid, enable_source: bool, filter: crate::filter::SyscallFilter, statistics_mode: bool, timing_mode: bool, output_format: crate::cli::OutputFormat, follow_forks: bool) -> Result<i32> {
    use crate::cli::OutputFormat;
    // Wait for initial SIGSTOP from PTRACE_TRACEME
    waitpid(child, None).context("Failed to wait for child")?;

    // Set ptrace options to trace syscalls
    let mut options = ptrace::Options::PTRACE_O_TRACESYSGOOD
        | ptrace::Options::PTRACE_O_EXITKILL;

    // Add fork following options if enabled
    if follow_forks {
        options |= ptrace::Options::PTRACE_O_TRACEFORK
            | ptrace::Options::PTRACE_O_TRACEVFORK
            | ptrace::Options::PTRACE_O_TRACECLONE;
    }

    ptrace::setoptions(child, options)
        .context("Failed to set ptrace options")?;

    let mut in_syscall = false;
    let mut current_syscall_entry: Option<SyscallEntry> = None;
    let mut syscall_entry_time: Option<std::time::Instant> = None;
    let mut dwarf_ctx: Option<crate::dwarf::DwarfContext> = None;
    let mut dwarf_loaded = false;
    let mut stats_tracker = if statistics_mode {
        Some(crate::stats::StatsTracker::new())
    } else {
        None
    };
    let mut json_output = if matches!(output_format, OutputFormat::Json) {
        Some(crate::json_output::JsonOutput::new())
    } else {
        None
    };
    let exit_code;

    loop {
        // Sprint 5-6: Load DWARF context on first syscall if --source is enabled
        // We wait until after exec() has happened to get the right binary
        if enable_source && !dwarf_loaded {
            dwarf_loaded = true; // Only try once
            // Read /proc/PID/exe to get binary path
            if let Ok(exe_path) = std::fs::read_link(format!("/proc/{}/exe", child)) {
                match crate::dwarf::DwarfContext::load(&exe_path) {
                    Ok(ctx) => {
                        eprintln!("[renacer: DWARF debug info loaded from {}]", exe_path.display());
                        dwarf_ctx = Some(ctx);
                    }
                    Err(e) => {
                        eprintln!("[renacer: Warning - failed to load DWARF: {}]", e);
                        eprintln!("[renacer: Continuing without source correlation]");
                    }
                }
            }
        }

        // Continue and wait for next syscall or exit
        ptrace::syscall(child, None).context("Failed to PTRACE_SYSCALL")?;

        match waitpid(child, None).context("Failed to waitpid")? {
            WaitStatus::Exited(_, code) => {
                exit_code = code;
                break;
            }
            WaitStatus::Signaled(_, sig, _) => {
                eprintln!("Child killed by signal: {:?}", sig);
                exit_code = 128 + sig as i32;
                break;
            }
            WaitStatus::PtraceEvent(_, _, _) => {
                // Ptrace event, continue
                continue;
            }
            WaitStatus::PtraceSyscall(_) => {
                // Syscall entry or exit
                let in_json_mode = json_output.is_some();
                if !in_syscall {
                    // Syscall entry - record start time if timing enabled
                    if timing_mode || statistics_mode || in_json_mode {
                        syscall_entry_time = Some(std::time::Instant::now());
                    }
                    current_syscall_entry = handle_syscall_entry(child, dwarf_ctx.as_ref(), &filter, statistics_mode, in_json_mode)?;
                    in_syscall = true;
                } else {
                    // Syscall exit - calculate duration
                    let duration_us = if let Some(start) = syscall_entry_time {
                        start.elapsed().as_micros() as u64
                    } else {
                        0
                    };
                    handle_syscall_exit(child, &current_syscall_entry, stats_tracker.as_mut(), json_output.as_mut(), timing_mode, duration_us)?;
                    current_syscall_entry = None;
                    syscall_entry_time = None;
                    in_syscall = false;
                }
            }
            _ => {
                // Other status, continue
                continue;
            }
        }
    }

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

    // Exit with traced program's exit code
    std::process::exit(exit_code);
}

/// Syscall entry data for JSON output
struct SyscallEntry {
    name: String,
    args: Vec<String>,
    source: Option<crate::json_output::JsonSourceLocation>,
}

/// Handle syscall entry - record syscall number and arguments
/// Returns the syscall entry data if it should be traced, None otherwise
fn handle_syscall_entry(child: Pid, dwarf_ctx: Option<&crate::dwarf::DwarfContext>, filter: &crate::filter::SyscallFilter, statistics_mode: bool, json_mode: bool) -> Result<Option<SyscallEntry>> {
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
        // Instruction pointer in rip register
        let ip = regs.rip;
        ctx.lookup(ip).ok().flatten()
    } else {
        None
    };

    // Sprint 3-4: Decode arguments based on syscall type
    let (formatted, args) = match name {
        "openat" => {
            // openat(dfd, filename, flags, mode)
            let filename = read_string(child, arg2 as usize).unwrap_or_else(|_| format!("{:#x}", arg2));
            let args = vec![format!("{:#x}", arg1), format!("\"{}\"", filename), format!("{:#x}", arg3)];
            (format!("{}({:#x}, \"{}\", {:#x}) = ", name, arg1, filename, arg3), args)
        }
        "unknown" => {
            let args = vec![format!("{:#x}", arg1), format!("{:#x}", arg2), format!("{:#x}", arg3)];
            (format!("syscall_{}({:#x}, {:#x}, {:#x}) = ", syscall_num, arg1, arg2, arg3), args)
        }
        _ => {
            let args = vec![format!("{:#x}", arg1), format!("{:#x}", arg2), format!("{:#x}", arg3)];
            (format!("{}({:#x}, {:#x}, {:#x}) = ", name, arg1, arg2, arg3), args)
        }
    };

    // Only print if not in statistics or JSON mode
    if !statistics_mode && !json_mode {
        // Print syscall with optional source location
        if let Some(src) = &source_info {
            print!("{}:{} ", src.file, src.line);
            if let Some(func) = &src.function {
                print!("{} ", func);
            }
        }
        print!("{}", formatted);
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }

    // Convert source_info to JsonSourceLocation if needed
    let json_source = source_info.map(|src| crate::json_output::JsonSourceLocation {
        file: src.file.to_string(),
        line: src.line,
        function: src.function.map(|s| s.to_string()),
    });

    // Return syscall entry data
    Ok(Some(SyscallEntry {
        name: name.to_string(),
        args,
        source: json_source,
    }))
}

/// Read a null-terminated string from the tracee's memory
fn read_string(child: Pid, addr: usize) -> Result<String> {
    use nix::sys::uio::{process_vm_readv, RemoteIoVec};
    use std::io::IoSliceMut;

    // Read up to 4096 bytes (max path length)
    let mut buf = vec![0u8; 4096];
    let mut local_iov = [IoSliceMut::new(&mut buf)];
    let remote_iov = [RemoteIoVec { base: addr, len: 4096 }];

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

/// Handle syscall exit - print return value and record statistics
fn handle_syscall_exit(child: Pid, syscall_entry: &Option<SyscallEntry>, stats_tracker: Option<&mut crate::stats::StatsTracker>, json_output: Option<&mut crate::json_output::JsonOutput>, timing_mode: bool, duration_us: u64) -> Result<()> {
    let regs = ptrace::getregs(child).context("Failed to get registers")?;

    // Return value in rax (may be negative for errors)
    let result = regs.rax as i64;

    // Check modes before borrowing
    let in_stats_mode = stats_tracker.is_some();
    let in_json_mode = json_output.is_some();

    // Record statistics if in statistics mode
    if let (Some(entry), Some(tracker)) = (syscall_entry, stats_tracker) {
        tracker.record(&entry.name, result, duration_us);
    }

    // Record JSON if in JSON mode
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

    // Print result only if not in statistics or JSON mode
    if syscall_entry.is_some() && !in_stats_mode && !in_json_mode {
        if timing_mode && duration_us > 0 {
            println!("{} <{:.6}>", result, duration_us as f64 / 1_000_000.0);
        } else {
            println!("{}", result);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_command_requires_nonempty_array() {
        let empty: Vec<String> = vec![];
        let filter = crate::filter::SyscallFilter::all();
        let result = trace_command(&empty, false, filter, false, false, crate::cli::OutputFormat::Text, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_trace_command_not_implemented_yet() {
        // RED phase: this should fail until we implement
        let cmd = vec!["echo".to_string(), "test".to_string()];
        let filter = crate::filter::SyscallFilter::all();
        let result = trace_command(&cmd, false, filter, false, false, crate::cli::OutputFormat::Text, false);
        assert!(result.is_err());
    }
}
