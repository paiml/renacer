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
pub fn trace_command(command: &[String], enable_source: bool, filter: crate::filter::SyscallFilter) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("Command array is empty");
    }

    let program = &command[0];
    let args = &command[1..];

    // Fork: parent will trace, child will exec
    match unsafe { fork() }.context("Failed to fork")? {
        ForkResult::Parent { child } => {
            trace_child(child, enable_source, filter)?;
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
fn trace_child(child: Pid, enable_source: bool, filter: crate::filter::SyscallFilter) -> Result<i32> {
    // Wait for initial SIGSTOP from PTRACE_TRACEME
    waitpid(child, None).context("Failed to wait for child")?;

    // Set ptrace options to trace syscalls
    ptrace::setoptions(
        child,
        ptrace::Options::PTRACE_O_TRACESYSGOOD
            | ptrace::Options::PTRACE_O_EXITKILL,
    )
    .context("Failed to set ptrace options")?;

    let mut in_syscall = false;
    let mut dwarf_ctx: Option<crate::dwarf::DwarfContext> = None;
    let mut dwarf_loaded = false;
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
                if !in_syscall {
                    // Syscall entry
                    handle_syscall_entry(child, dwarf_ctx.as_ref(), &filter)?;
                    in_syscall = true;
                } else {
                    // Syscall exit
                    handle_syscall_exit(child)?;
                    in_syscall = false;
                }
            }
            _ => {
                // Other status, continue
                continue;
            }
        }
    }

    // Exit with traced program's exit code
    std::process::exit(exit_code);
}

/// Handle syscall entry - record syscall number and arguments
fn handle_syscall_entry(child: Pid, dwarf_ctx: Option<&crate::dwarf::DwarfContext>, filter: &crate::filter::SyscallFilter) -> Result<()> {
    let regs = ptrace::getregs(child).context("Failed to get registers")?;

    // On x86_64: syscall number in orig_rax
    let syscall_num = regs.orig_rax as i64;

    // Get syscall name
    let name = syscalls::syscall_name(syscall_num);

    // Sprint 9-10: Filter syscalls based on -e trace= expression
    if !filter.should_trace(name) {
        // Don't print this syscall
        return Ok(());
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
    let formatted = match name {
        "openat" => {
            // openat(dfd, filename, flags, mode)
            let filename = read_string(child, arg2 as usize).unwrap_or_else(|_| format!("{:#x}", arg2));
            format!("{}({:#x}, \"{}\", {:#x}) = ", name, arg1, filename, arg3)
        }
        "unknown" => {
            format!("syscall_{}({:#x}, {:#x}, {:#x}) = ", syscall_num, arg1, arg2, arg3)
        }
        _ => {
            format!("{}({:#x}, {:#x}, {:#x}) = ", name, arg1, arg2, arg3)
        }
    };

    // Print syscall with optional source location
    if let Some(src) = source_info {
        print!("{}:{} ", src.file, src.line);
        if let Some(func) = src.function {
            print!("{} ", func);
        }
    }
    print!("{}", formatted);
    std::io::Write::flush(&mut std::io::stdout()).ok();

    Ok(())
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

/// Handle syscall exit - print return value
fn handle_syscall_exit(child: Pid) -> Result<()> {
    let regs = ptrace::getregs(child).context("Failed to get registers")?;

    // Sprint 3-4: Print result for all syscalls
    // Return value in rax (may be negative for errors)
    let result = regs.rax as i64;
    println!("{}", result);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_command_requires_nonempty_array() {
        let empty: Vec<String> = vec![];
        let filter = crate::filter::SyscallFilter::all();
        let result = trace_command(&empty, false, filter);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_trace_command_not_implemented_yet() {
        // RED phase: this should fail until we implement
        let cmd = vec!["echo".to_string(), "test".to_string()];
        let filter = crate::filter::SyscallFilter::all();
        let result = trace_command(&cmd, false, filter);
        assert!(result.is_err());
    }
}
