//! Stack unwinding for remote process via ptrace
//!
//! GitHub Issue #1: Function-level profiling with stack unwinding
//!
//! This module implements stack unwinding for processes being traced via ptrace.
//! Unlike traditional stack unwinding (backtrace crate), we need to read the
//! remote process's memory and registers to reconstruct the call stack.

use anyhow::{Context, Result};
use nix::sys::ptrace;
use nix::unistd::Pid;
use nix::sys::uio::{RemoteIoVec, process_vm_readv};
use std::io::IoSliceMut;

/// Maximum stack depth to unwind (prevent infinite loops)
const MAX_STACK_DEPTH: usize = 64;

/// A single stack frame
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Instruction pointer (return address)
    pub rip: u64,
    /// Base pointer - Reserved for future use in advanced stack analysis
    #[allow(dead_code)]
    pub rbp: u64,
}

/// Unwind the stack of a traced process
///
/// Returns a list of stack frames, with the first frame being the current
/// instruction pointer (where the syscall was made).
///
/// # Algorithm
///
/// 1. Get current RIP and RBP from registers
/// 2. Walk the frame pointer chain (RBP) to find return addresses
/// 3. Stop when RBP is 0, points to invalid memory, or exceeds max depth
///
/// # Note
///
/// This uses the traditional x86_64 frame pointer convention. It may not
/// work correctly with binaries compiled with `-fomit-frame-pointer`.
pub fn unwind_stack(pid: Pid) -> Result<Vec<StackFrame>> {
    let mut frames = Vec::with_capacity(16);

    // Get current registers
    let regs = ptrace::getregs(pid)
        .context("Failed to get registers for stack unwinding")?;

    let rip = regs.rip;
    let mut rbp = regs.rbp;

    // Add current frame
    frames.push(StackFrame {
        rip,
        rbp,
    });

    // Walk the stack using frame pointers
    for _ in 0..MAX_STACK_DEPTH {
        if rbp == 0 {
            break; // End of stack
        }

        // Read the saved RBP and return address from the stack
        // Stack layout at RBP:
        //   [rbp+0]: saved RBP (previous frame)
        //   [rbp+8]: return address (RIP)
        match read_u64_from_process(pid, rbp) {
            Ok(saved_rbp) => {
                match read_u64_from_process(pid, rbp + 8) {
                    Ok(return_address) => {
                        if return_address == 0 {
                            break; // Invalid return address
                        }

                        frames.push(StackFrame {
                            rip: return_address,
                            rbp: saved_rbp,
                        });

                        rbp = saved_rbp;
                    }
                    Err(_) => break, // Can't read return address
                }
            }
            Err(_) => break, // Can't read saved RBP
        }
    }

    Ok(frames)
}

/// Read a u64 value from the remote process's memory
fn read_u64_from_process(pid: Pid, addr: u64) -> Result<u64> {
    let mut buffer = [0u8; 8];
    let mut local_iov = [IoSliceMut::new(&mut buffer)];
    let remote_iov = [RemoteIoVec {
        base: addr as usize,
        len: 8,
    }];

    process_vm_readv(pid, &mut local_iov, &remote_iov)
        .context(format!("Failed to read memory at address 0x{:x}", addr))?;

    Ok(u64::from_ne_bytes(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_frame_creation() {
        let frame = StackFrame {
            rip: 0x12345678,
            rbp: 0x87654321,
        };
        assert_eq!(frame.rip, 0x12345678);
        assert_eq!(frame.rbp, 0x87654321);
    }

    #[test]
    fn test_stack_frame_clone() {
        let frame = StackFrame {
            rip: 0xDEADBEEF,
            rbp: 0xCAFEBABE,
        };
        let cloned = frame.clone();
        assert_eq!(cloned.rip, 0xDEADBEEF);
        assert_eq!(cloned.rbp, 0xCAFEBABE);
    }

    #[test]
    fn test_stack_frame_debug() {
        let frame = StackFrame {
            rip: 0x1000,
            rbp: 0x2000,
        };
        let debug_str = format!("{:?}", frame);
        assert!(debug_str.contains("StackFrame"));
        assert!(debug_str.contains("rip"));
        assert!(debug_str.contains("rbp"));
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]  // Testing constant invariants
    fn test_max_stack_depth_constant() {
        assert_eq!(MAX_STACK_DEPTH, 64);
        assert!(MAX_STACK_DEPTH > 0);
        assert!(MAX_STACK_DEPTH < 1000); // Reasonable limit
    }

    #[test]
    fn test_stack_frame_zero_addresses() {
        let frame = StackFrame {
            rip: 0,
            rbp: 0,
        };
        assert_eq!(frame.rip, 0);
        assert_eq!(frame.rbp, 0);
    }

    #[test]
    fn test_stack_frame_high_addresses() {
        let frame = StackFrame {
            rip: 0xFFFFFFFFFFFFFFFF,
            rbp: 0xFFFFFFFFFFFFFFFF,
        };
        assert_eq!(frame.rip, 0xFFFFFFFFFFFFFFFF);
        assert_eq!(frame.rbp, 0xFFFFFFFFFFFFFFFF);
    }

    // Note: Testing unwind_stack() and read_u64_from_process() requires
    // a real traced process, which is covered by integration tests
    // (tests/sprint13_stack_unwinding_tests.rs)
}
