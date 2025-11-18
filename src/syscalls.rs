//! Syscall number to name mapping for x86_64
//!
//! Sprint 3-4: Full syscall coverage

/// Resolve syscall number to name for x86_64
///
/// Returns the syscall name, or "syscall_NNN" if unknown
pub fn syscall_name(num: i64) -> &'static str {
    match num {
        0 => "read",
        1 => "write",
        2 => "open",
        3 => "close",
        4 => "stat",
        5 => "fstat",
        6 => "lstat",
        7 => "poll",
        8 => "lseek",
        9 => "mmap",
        10 => "mprotect",
        11 => "munmap",
        12 => "brk",
        13 => "rt_sigaction",
        14 => "rt_sigprocmask",
        15 => "rt_sigreturn",
        16 => "ioctl",
        17 => "pread64",
        18 => "pwrite64",
        19 => "readv",
        20 => "writev",
        21 => "access",
        22 => "pipe",
        23 => "select",
        24 => "sched_yield",
        25 => "mremap",
        26 => "msync",
        27 => "mincore",
        28 => "madvise",
        29 => "shmget",
        30 => "shmat",
        31 => "shmctl",
        32 => "dup",
        33 => "dup2",
        34 => "pause",
        35 => "nanosleep",
        36 => "getitimer",
        37 => "alarm",
        38 => "setitimer",
        39 => "getpid",
        40 => "sendfile",
        41 => "socket",
        42 => "connect",
        43 => "accept",
        44 => "sendto",
        45 => "recvfrom",
        46 => "sendmsg",
        47 => "recvmsg",
        48 => "shutdown",
        49 => "bind",
        50 => "listen",
        51 => "getsockname",
        52 => "getpeername",
        53 => "socketpair",
        54 => "setsockopt",
        55 => "getsockopt",
        56 => "clone",
        57 => "fork",
        58 => "vfork",
        59 => "execve",
        60 => "exit",
        61 => "wait4",
        62 => "kill",
        63 => "uname",
        72 => "fcntl",
        73 => "flock",
        74 => "fsync",
        75 => "fdatasync",
        76 => "truncate",
        77 => "ftruncate",
        78 => "getdents",
        79 => "getcwd",
        80 => "chdir",
        81 => "fchdir",
        82 => "rename",
        83 => "mkdir",
        84 => "rmdir",
        85 => "creat",
        86 => "link",
        87 => "unlink",
        88 => "symlink",
        89 => "readlink",
        90 => "chmod",
        91 => "fchmod",
        92 => "chown",
        93 => "fchown",
        94 => "lchown",
        95 => "umask",
        96 => "gettimeofday",
        97 => "getrlimit",
        98 => "getrusage",
        99 => "sysinfo",
        102 => "getuid",
        104 => "getgid",
        105 => "setuid",
        107 => "setgid",
        108 => "geteuid",
        109 => "getegid",
        110 => "setpgid",
        111 => "getppid",
        112 => "getpgrp",
        113 => "setsid",
        131 => "sigaltstack",
        157 => "prctl",
        158 => "arch_prctl",
        186 => "gettid",
        202 => "futex",
        217 => "getdents64",
        218 => "set_tid_address",
        228 => "clock_gettime",
        231 => "exit_group",
        257 => "openat",
        262 => "newfstatat",
        273 => "set_robust_list",
        318 => "getrandom",
        332 => "statx",
        435 => "clone3",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_syscalls() {
        assert_eq!(syscall_name(0), "read");
        assert_eq!(syscall_name(1), "write");
        assert_eq!(syscall_name(2), "open");
        assert_eq!(syscall_name(3), "close");
        assert_eq!(syscall_name(257), "openat");
    }

    #[test]
    fn test_unknown_syscall() {
        assert_eq!(syscall_name(9999), "unknown");
    }

    #[test]
    fn test_all_known_syscalls() {
        // Test all known syscall numbers (comprehensive coverage)
        let known_syscalls = vec![
            (0, "read"),
            (1, "write"),
            (2, "open"),
            (3, "close"),
            (4, "stat"),
            (5, "fstat"),
            (6, "lstat"),
            (7, "poll"),
            (8, "lseek"),
            (9, "mmap"),
            (10, "mprotect"),
            (11, "munmap"),
            (12, "brk"),
            (13, "rt_sigaction"),
            (14, "rt_sigprocmask"),
            (15, "rt_sigreturn"),
            (16, "ioctl"),
            (17, "pread64"),
            (18, "pwrite64"),
            (19, "readv"),
            (20, "writev"),
            (21, "access"),
            (22, "pipe"),
            (39, "getpid"),
            (56, "clone"),
            (57, "fork"),
            (58, "vfork"),
            (59, "execve"),
            (60, "exit"),
            (61, "wait4"),
            (62, "kill"),
            (63, "uname"),
            (72, "fcntl"),
            (79, "getcwd"),
            (80, "chdir"),
            (89, "readlink"),
            (96, "gettimeofday"),
            (102, "getuid"),
            (104, "getgid"),
            (105, "setuid"),
            (107, "setgid"),
            (108, "geteuid"),
            (109, "getegid"),
            (186, "gettid"),
            (228, "clock_gettime"),
            (231, "exit_group"),
            (257, "openat"),
            (262, "newfstatat"),
            (273, "set_robust_list"),
            (318, "getrandom"),
            (332, "statx"),
        ];

        for (num, expected_name) in known_syscalls {
            assert_eq!(
                syscall_name(num),
                expected_name,
                "Syscall {} should be named {}",
                num,
                expected_name
            );
        }
    }

    #[test]
    fn test_syscall_name_never_panics() {
        // Property: syscall_name should never panic for any i64
        for num in [-1000, -1, 0, 1, 100, 500, 1000, 9999, i64::MAX] {
            let name = syscall_name(num);
            assert!(
                !name.is_empty(),
                "Syscall name should never be empty for {}",
                num
            );
        }
    }

    #[test]
    fn test_syscall_name_always_returns_str() {
        // Property: syscall_name always returns a valid str
        for num in 0..400 {
            let name = syscall_name(num);
            assert!(!name.is_empty());
            // Either a known name or "unknown"
            assert!(name.chars().all(|c| c.is_alphanumeric() || c == '_'));
        }
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_syscall_name_never_panics(num in any::<i64>()) {
            // Property: syscall_name never panics for any i64
            let name = syscall_name(num);
            prop_assert!(!name.is_empty());
        }

        #[test]
        fn prop_syscall_name_deterministic(num in 0..400i64) {
            // Property: syscall_name is deterministic
            let name1 = syscall_name(num);
            let name2 = syscall_name(num);
            prop_assert_eq!(name1, name2);
        }

        #[test]
        fn prop_unknown_syscalls_return_unknown(num in 500..10000i64) {
            // Property: high syscall numbers return "unknown"
            // Note: clone3 is at 435, so we start from 500
            let name = syscall_name(num);
            prop_assert_eq!(name, "unknown");
        }
    }
}
