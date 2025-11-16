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
pub fn trace_command(command: &[String]) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("Command array is empty");
    }

    let program = &command[0];
    let args = &command[1..];

    // Fork: parent will trace, child will exec
    match unsafe { fork() }.context("Failed to fork")? {
        ForkResult::Parent { child } => {
            trace_child(child)?;
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

/// Trace a child process, printing write syscalls only
fn trace_child(child: Pid) -> Result<i32> {
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
    let exit_code;

    loop {
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
                    handle_syscall_entry(child)?;
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
fn handle_syscall_entry(child: Pid) -> Result<()> {
    let regs = ptrace::getregs(child).context("Failed to get registers")?;

    // On x86_64: syscall number in orig_rax
    let syscall_num = regs.orig_rax as i64;

    // Get syscall name
    let name = syscalls::syscall_name(syscall_num);

    // Arguments in rdi, rsi, rdx, r10, r8, r9 for x86_64
    let arg1 = regs.rdi;
    let arg2 = regs.rsi;
    let arg3 = regs.rdx;

    // Sprint 3-4: Trace all syscalls with basic argument display
    if name == "unknown" {
        print!("syscall_{}({:#x}, {:#x}, {:#x}) = ", syscall_num, arg1, arg2, arg3);
    } else {
        print!("{}({:#x}, {:#x}, {:#x}) = ", name, arg1, arg2, arg3);
    }

    std::io::Write::flush(&mut std::io::stdout()).ok();

    Ok(())
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
        let result = trace_command(&empty);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_trace_command_not_implemented_yet() {
        // RED phase: this should fail until we implement
        let cmd = vec!["echo".to_string(), "test".to_string()];
        let result = trace_command(&cmd);
        assert!(result.is_err());
    }
}
