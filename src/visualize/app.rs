//! Application state for renacer visualize (ttop-identical pattern)
//!
//! Manages UI state, metric history, and collector coordination.

use super::ring_buffer::HistoryBuffer;
use super::VisualizeConfig;
use crate::tracer::VisualizerEvent;
use crossterm::event::{KeyCode, KeyModifiers};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::Instant;

/// Syscall category for grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallCategory {
    File,
    Network,
    Memory,
    Process,
    Other,
}

impl SyscallCategory {
    /// Categorize a syscall by name
    pub fn from_name(name: &str) -> Self {
        match name {
            // File operations
            "read" | "write" | "open" | "close" | "stat" | "fstat" | "lstat" | "lseek"
            | "pread64" | "pwrite64" | "readv" | "writev" | "access" | "pipe" | "dup" | "dup2"
            | "dup3" | "fcntl" | "flock" | "fsync" | "fdatasync" | "truncate" | "ftruncate"
            | "getdents" | "getdents64" | "getcwd" | "chdir" | "fchdir" | "rename" | "mkdir"
            | "rmdir" | "creat" | "link" | "unlink" | "symlink" | "readlink" | "chmod"
            | "fchmod" | "chown" | "fchown" | "lchown" | "umask" | "openat" | "mkdirat"
            | "mknodat" | "fchownat" | "futimesat" | "newfstatat" | "unlinkat" | "renameat"
            | "linkat" | "symlinkat" | "readlinkat" | "fchmodat" | "faccessat" | "pselect6"
            | "ppoll" | "splice" | "tee" | "vmsplice" | "sync_file_range" | "utimensat"
            | "fallocate" | "eventfd" | "eventfd2" | "epoll_create" | "epoll_create1"
            | "epoll_ctl" | "epoll_wait" | "epoll_pwait" | "inotify_init" | "inotify_init1"
            | "inotify_add_watch" | "inotify_rm_watch" | "fstatat64" | "statx" => Self::File,

            // Network operations
            "socket" | "connect" | "accept" | "accept4" | "sendto" | "recvfrom" | "sendmsg"
            | "recvmsg" | "shutdown" | "bind" | "listen" | "getsockname" | "getpeername"
            | "socketpair" | "setsockopt" | "getsockopt" | "sendmmsg" | "recvmmsg" | "select"
            | "poll" => Self::Network,

            // Memory operations
            "brk" | "mmap" | "munmap" | "mprotect" | "mremap" | "msync" | "mincore" | "madvise"
            | "mlock" | "munlock" | "mlockall" | "munlockall" | "shmget" | "shmat" | "shmctl"
            | "shmdt" => Self::Memory,

            // Process operations
            "fork" | "vfork" | "clone" | "clone3" | "execve" | "execveat" | "exit"
            | "exit_group" | "wait4" | "waitid" | "kill" | "tkill" | "tgkill" | "getpid"
            | "getppid" | "getuid" | "geteuid" | "getgid" | "getegid" | "setuid" | "setgid"
            | "setpgid" | "getpgid" | "getpgrp" | "setsid" | "getsid" | "setreuid" | "setregid"
            | "getgroups" | "setgroups" | "setresuid" | "getresuid" | "setresgid" | "getresgid"
            | "setfsuid" | "setfsgid" | "gettid" | "futex" | "sched_yield"
            | "sched_setaffinity" | "sched_getaffinity" | "set_tid_address" | "prctl"
            | "arch_prctl" | "ptrace" | "seccomp" | "getrandom" => Self::Process,

            _ => Self::Other,
        }
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Network => "net",
            Self::Memory => "mem",
            Self::Process => "proc",
            Self::Other => "other",
        }
    }
}

/// Panel visibility state
#[derive(Debug, Clone, Copy)]
pub struct PanelVisibility {
    pub syscall_heatmap: bool,
    pub anomaly_timeline: bool,
    pub ml_scatter: bool,
    pub trace_waterfall: bool,
    pub process_syscalls: bool,
    pub stats_summary: bool,
}

impl Default for PanelVisibility {
    fn default() -> Self {
        Self {
            syscall_heatmap: true,
            anomaly_timeline: true,
            ml_scatter: false,      // Disabled by default (requires --ml)
            trace_waterfall: false, // Disabled by default (requires --otlp)
            process_syscalls: true,
            stats_summary: true,
        }
    }
}

/// Sort column for process table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortColumn {
    #[default]
    Pid,
    Name,
    Cpu,
    Calls,
    Errors,
}

impl SortColumn {
    /// Cycle to next sort column
    pub fn next(self) -> Self {
        match self {
            Self::Pid => Self::Name,
            Self::Name => Self::Cpu,
            Self::Cpu => Self::Calls,
            Self::Calls => Self::Errors,
            Self::Errors => Self::Pid,
        }
    }
}

/// Anomaly record for display
#[derive(Debug, Clone)]
pub struct AnomalyRecord {
    pub syscall: String,
    pub duration_us: u64,
    pub z_score: f32,
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
    pub timestamp: Instant,
}

/// Process syscall statistics
#[derive(Debug, Clone, Default)]
pub struct ProcessSyscallStats {
    pub pid: i32,
    pub name: String,
    pub cpu_percent: f64,
    pub calls_per_sec: f64,
    pub error_count: u64,
    pub top_syscall: String,
    pub history: Vec<f64>,
}

/// Main application state
pub struct VisualizeApp {
    // Configuration
    pub config: VisualizeConfig,

    // Event channel from tracer (Sprint 52-55 bridge)
    pub event_receiver: Option<Receiver<VisualizerEvent>>,

    // Syscall metrics
    pub syscall_counts: HashMap<String, u64>,
    pub syscall_rates: HashMap<String, f64>,
    pub category_rates: HashMap<SyscallCategory, f64>,
    pub total_syscalls: u64,
    pub total_errors: u64,
    pub syscall_rate: f64,

    // History buffers (300 elements, normalized 0-1)
    pub category_history: HashMap<SyscallCategory, HistoryBuffer<f64>>,
    pub latency_history: HistoryBuffer<f64>,
    pub anomaly_history: HistoryBuffer<f64>,
    pub rate_history: HistoryBuffer<f64>,

    // Anomaly tracking
    pub anomalies: Vec<AnomalyRecord>,
    pub anomaly_count: usize,

    // ML clustering (when enabled)
    pub cluster_points: Vec<(f64, f64, u8)>, // (x, y, cluster_id)
    pub outlier_count: usize,
    pub silhouette_score: f64,

    // Process tracking
    pub processes: Vec<ProcessSyscallStats>,

    // UI state
    pub panels: PanelVisibility,
    pub selected_panel: usize,
    pub process_selected: usize,
    pub process_scroll_offset: usize,
    pub sort_column: SortColumn,
    pub sort_descending: bool,
    pub filter: String,
    pub show_filter_input: bool,
    pub show_help: bool,

    // Frame timing
    pub frame_id: u64,
    pub last_collect: Instant,
    pub avg_frame_time_us: u64,
    pub max_frame_time_us: u64,

    // Exit control
    pub should_exit: bool,
    pub exit_code: i32,

    // Trace state
    pub trace_complete: bool,
}

impl VisualizeApp {
    /// Create a new application instance
    pub fn new(config: VisualizeConfig) -> Self {
        Self::with_receiver(config, None)
    }

    /// Create a new application instance with an event receiver
    pub fn with_receiver(
        config: VisualizeConfig,
        event_receiver: Option<Receiver<VisualizerEvent>>,
    ) -> Self {
        let history_size = config.history_size;

        // Initialize category history buffers
        let mut category_history = HashMap::new();
        for category in [
            SyscallCategory::File,
            SyscallCategory::Network,
            SyscallCategory::Memory,
            SyscallCategory::Process,
            SyscallCategory::Other,
        ] {
            category_history.insert(category, HistoryBuffer::new(history_size));
        }

        // Set panel visibility based on config
        let mut panels = PanelVisibility::default();
        panels.anomaly_timeline = config.enable_anomaly;
        panels.ml_scatter = config.enable_ml;
        panels.trace_waterfall = config.otlp_endpoint.is_some();

        Self {
            config,
            event_receiver,
            syscall_counts: HashMap::new(),
            syscall_rates: HashMap::new(),
            category_rates: HashMap::new(),
            total_syscalls: 0,
            total_errors: 0,
            syscall_rate: 0.0,
            category_history,
            latency_history: HistoryBuffer::new(history_size),
            anomaly_history: HistoryBuffer::new(history_size),
            rate_history: HistoryBuffer::new(history_size),
            anomalies: Vec::new(),
            anomaly_count: 0,
            cluster_points: Vec::new(),
            outlier_count: 0,
            silhouette_score: 0.0,
            processes: Vec::new(),
            panels,
            selected_panel: 0,
            process_selected: 0,
            process_scroll_offset: 0,
            sort_column: SortColumn::default(),
            sort_descending: true,
            filter: String::new(),
            show_filter_input: false,
            show_help: false,
            frame_id: 0,
            last_collect: Instant::now(),
            avg_frame_time_us: 0,
            max_frame_time_us: 0,
            should_exit: false,
            exit_code: 0,
            trace_complete: false,
        }
    }

    /// Create a new app in deterministic mode for testing
    pub fn new_deterministic(seed: u64) -> Self {
        let config = VisualizeConfig {
            deterministic: true,
            ..Default::default()
        };
        let _ = seed; // Would be used for seeded RNG if needed
        Self::new(config)
    }

    /// Collect metrics from all collectors
    pub fn collect_metrics(&mut self) {
        // Sprint 52-55: Drain events from tracer channel
        // Take receiver out temporarily to avoid borrow conflict
        if let Some(receiver) = self.event_receiver.take() {
            // Non-blocking drain of all pending events
            while let Ok(event) = receiver.try_recv() {
                self.record_syscall(&event.name, event.duration_us, event.result);
            }
            // Put it back
            self.event_receiver = Some(receiver);
        }

        // Update history buffers
        self.rate_history.push(self.syscall_rate);

        for (category, rate) in &self.category_rates {
            if let Some(buf) = self.category_history.get_mut(category) {
                // Normalize rate to 0-1 (assuming max 10K/s)
                let normalized = (rate / 10000.0).min(1.0);
                buf.push(normalized);
            }
        }

        // Update anomaly history (average z-score of recent anomalies)
        let avg_z = if self.anomalies.is_empty() {
            0.0
        } else {
            let recent: Vec<_> = self
                .anomalies
                .iter()
                .filter(|a| a.timestamp.elapsed().as_secs() < 60)
                .collect();
            if recent.is_empty() {
                0.0
            } else {
                recent.iter().map(|a| a.z_score as f64).sum::<f64>() / recent.len() as f64
            }
        };
        self.anomaly_history.push(avg_z);

        self.last_collect = Instant::now();
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        // Handle filter input mode
        if self.show_filter_input {
            match code {
                KeyCode::Esc => {
                    self.show_filter_input = false;
                }
                KeyCode::Enter => {
                    self.show_filter_input = false;
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                }
                _ => {}
            }
            return;
        }

        // Normal mode key handling
        match code {
            // Help
            KeyCode::Char('?') | KeyCode::F(1) => {
                self.show_help = !self.show_help;
            }

            // Panel toggles (1-6)
            KeyCode::Char('1') => {
                self.panels.syscall_heatmap = !self.panels.syscall_heatmap;
            }
            KeyCode::Char('2') => {
                self.panels.anomaly_timeline = !self.panels.anomaly_timeline;
            }
            KeyCode::Char('3') => {
                self.panels.ml_scatter = !self.panels.ml_scatter;
            }
            KeyCode::Char('4') => {
                self.panels.trace_waterfall = !self.panels.trace_waterfall;
            }
            KeyCode::Char('5') => {
                self.panels.process_syscalls = !self.panels.process_syscalls;
            }
            KeyCode::Char('6') => {
                self.panels.stats_summary = !self.panels.stats_summary;
            }

            // Reset all panels
            KeyCode::Char('0') => {
                self.panels = PanelVisibility::default();
                self.panels.anomaly_timeline = self.config.enable_anomaly;
                self.panels.ml_scatter = self.config.enable_ml;
                self.panels.trace_waterfall = self.config.otlp_endpoint.is_some();
            }

            // Navigation
            KeyCode::Char('j') | KeyCode::Down => {
                if self.process_selected < self.processes.len().saturating_sub(1) {
                    self.process_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.process_selected > 0 {
                    self.process_selected -= 1;
                }
            }
            KeyCode::Char('g') => {
                self.process_selected = 0;
            }
            KeyCode::Char('G') => {
                self.process_selected = self.processes.len().saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.process_selected =
                    (self.process_selected + 10).min(self.processes.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.process_selected = self.process_selected.saturating_sub(10);
            }

            // Sorting
            KeyCode::Char('s') | KeyCode::Tab => {
                self.sort_column = self.sort_column.next();
            }
            KeyCode::Char('r') => {
                self.sort_descending = !self.sort_descending;
            }

            // Filter
            KeyCode::Char('f' | '/') => {
                self.show_filter_input = true;
            }
            KeyCode::Delete => {
                self.filter.clear();
            }

            // Escape closes overlays
            KeyCode::Esc => {
                self.show_help = false;
            }

            _ => {}
        }

        // Handle Ctrl combinations
        if modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char('l') = code {
                // Clear screen / refresh (would be handled by terminal)
            }
        }
    }

    /// Record a syscall event
    pub fn record_syscall(&mut self, name: &str, duration_us: u64, result: i64) {
        // Update counts
        *self.syscall_counts.entry(name.to_string()).or_default() += 1;
        self.total_syscalls += 1;

        if result < 0 {
            self.total_errors += 1;
        }

        // Update category
        let category = SyscallCategory::from_name(name);
        *self.category_rates.entry(category).or_default() += 1.0;

        // Update latency history
        self.latency_history.push(duration_us as f64);
    }

    /// Record an anomaly
    pub fn record_anomaly(
        &mut self,
        syscall: String,
        duration_us: u64,
        z_score: f32,
        source_file: Option<String>,
        source_line: Option<u32>,
    ) {
        self.anomalies.push(AnomalyRecord {
            syscall,
            duration_us,
            z_score,
            source_file,
            source_line,
            timestamp: Instant::now(),
        });

        // Keep only last 100 anomalies
        if self.anomalies.len() > 100 {
            self.anomalies.remove(0);
        }

        self.anomaly_count = self.anomalies.len();
    }

    /// Update ML cluster points
    pub fn update_clusters(&mut self, points: Vec<(f64, f64, u8)>, silhouette: f64) {
        self.outlier_count = points.iter().filter(|(_, _, c)| *c == 255).count();
        self.cluster_points = points;
        self.silhouette_score = silhouette;
    }

    /// Get sorted and filtered processes
    pub fn sorted_processes(&self) -> Vec<&ProcessSyscallStats> {
        let mut processes: Vec<_> = self
            .processes
            .iter()
            .filter(|p| {
                if self.filter.is_empty() {
                    true
                } else {
                    p.name.to_lowercase().contains(&self.filter.to_lowercase())
                        || p.pid.to_string().contains(&self.filter)
                }
            })
            .collect();

        processes.sort_by(|a, b| {
            let cmp = match self.sort_column {
                SortColumn::Pid => a.pid.cmp(&b.pid),
                SortColumn::Name => a.name.cmp(&b.name),
                SortColumn::Cpu => a.cpu_percent.partial_cmp(&b.cpu_percent).unwrap(),
                SortColumn::Calls => a.calls_per_sec.partial_cmp(&b.calls_per_sec).unwrap(),
                SortColumn::Errors => a.error_count.cmp(&b.error_count),
            };
            if self.sort_descending {
                cmp.reverse()
            } else {
                cmp
            }
        });

        processes
    }

    /// Count visible top panels
    pub fn count_top_panels(&self) -> usize {
        let mut count = 0;
        if self.panels.syscall_heatmap {
            count += 1;
        }
        if self.panels.anomaly_timeline {
            count += 1;
        }
        if self.panels.ml_scatter {
            count += 1;
        }
        if self.panels.trace_waterfall {
            count += 1;
        }
        if self.panels.stats_summary {
            count += 1;
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syscall_category() {
        assert_eq!(SyscallCategory::from_name("read"), SyscallCategory::File);
        assert_eq!(
            SyscallCategory::from_name("socket"),
            SyscallCategory::Network
        );
        assert_eq!(SyscallCategory::from_name("mmap"), SyscallCategory::Memory);
        assert_eq!(SyscallCategory::from_name("fork"), SyscallCategory::Process);
        assert_eq!(
            SyscallCategory::from_name("unknown"),
            SyscallCategory::Other
        );
    }

    #[test]
    fn test_app_new() {
        let config = VisualizeConfig::default();
        let app = VisualizeApp::new(config);
        assert_eq!(app.total_syscalls, 0);
        assert!(app.panels.syscall_heatmap);
    }

    #[test]
    fn test_record_syscall() {
        let mut app = VisualizeApp::new(VisualizeConfig::default());
        app.record_syscall("read", 100, 0);
        app.record_syscall("read", 200, 0);
        app.record_syscall("write", 150, -1);

        assert_eq!(app.total_syscalls, 3);
        assert_eq!(app.total_errors, 1);
        assert_eq!(*app.syscall_counts.get("read").unwrap(), 2);
    }

    #[test]
    fn test_record_anomaly() {
        let mut app = VisualizeApp::new(VisualizeConfig::default());
        app.record_anomaly(
            "read".to_string(),
            50000,
            4.5,
            Some("test.rs".to_string()),
            Some(42),
        );

        assert_eq!(app.anomaly_count, 1);
        assert_eq!(app.anomalies[0].syscall, "read");
        assert_eq!(app.anomalies[0].z_score, 4.5);
    }

    #[test]
    fn test_panel_toggles() {
        let mut app = VisualizeApp::new(VisualizeConfig::default());
        assert!(app.panels.syscall_heatmap);

        app.handle_key(KeyCode::Char('1'), KeyModifiers::empty());
        assert!(!app.panels.syscall_heatmap);

        app.handle_key(KeyCode::Char('1'), KeyModifiers::empty());
        assert!(app.panels.syscall_heatmap);
    }

    #[test]
    fn test_navigation() {
        let mut app = VisualizeApp::new(VisualizeConfig::default());
        app.processes = vec![
            ProcessSyscallStats {
                pid: 1,
                name: "init".to_string(),
                ..Default::default()
            },
            ProcessSyscallStats {
                pid: 2,
                name: "bash".to_string(),
                ..Default::default()
            },
        ];

        assert_eq!(app.process_selected, 0);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::empty());
        assert_eq!(app.process_selected, 1);
        app.handle_key(KeyCode::Char('k'), KeyModifiers::empty());
        assert_eq!(app.process_selected, 0);
    }

    #[test]
    fn test_sort_column_cycle() {
        let mut col = SortColumn::Pid;
        col = col.next();
        assert_eq!(col, SortColumn::Name);
        col = col.next();
        assert_eq!(col, SortColumn::Cpu);
        col = col.next();
        assert_eq!(col, SortColumn::Calls);
        col = col.next();
        assert_eq!(col, SortColumn::Errors);
        col = col.next();
        assert_eq!(col, SortColumn::Pid);
    }

    #[test]
    fn test_help_toggle() {
        let mut app = VisualizeApp::new(VisualizeConfig::default());
        assert!(!app.show_help);

        app.handle_key(KeyCode::Char('?'), KeyModifiers::empty());
        assert!(app.show_help);

        app.handle_key(KeyCode::Esc, KeyModifiers::empty());
        assert!(!app.show_help);
    }

    #[test]
    fn test_filter_input() {
        let mut app = VisualizeApp::new(VisualizeConfig::default());

        // Enter filter mode
        app.handle_key(KeyCode::Char('f'), KeyModifiers::empty());
        assert!(app.show_filter_input);

        // Type filter
        app.handle_key(KeyCode::Char('t'), KeyModifiers::empty());
        app.handle_key(KeyCode::Char('e'), KeyModifiers::empty());
        app.handle_key(KeyCode::Char('s'), KeyModifiers::empty());
        app.handle_key(KeyCode::Char('t'), KeyModifiers::empty());
        assert_eq!(app.filter, "test");

        // Exit filter mode
        app.handle_key(KeyCode::Enter, KeyModifiers::empty());
        assert!(!app.show_filter_input);
    }

    #[test]
    fn test_event_receiver_drains_channel() {
        use std::sync::mpsc;

        // Create channel
        let (tx, rx) = mpsc::channel::<VisualizerEvent>();

        // Create app with receiver
        let mut app = VisualizeApp::with_receiver(VisualizeConfig::default(), Some(rx));
        assert_eq!(app.total_syscalls, 0);

        // Send some events
        tx.send(VisualizerEvent {
            name: "read".to_string(),
            duration_us: 100,
            result: 0,
            pid: 1234,
        })
        .unwrap();
        tx.send(VisualizerEvent {
            name: "write".to_string(),
            duration_us: 200,
            result: -1,
            pid: 1234,
        })
        .unwrap();

        // Collect metrics should drain the channel
        app.collect_metrics();

        // Verify events were processed
        assert_eq!(app.total_syscalls, 2);
        assert_eq!(app.total_errors, 1);
        assert_eq!(*app.syscall_counts.get("read").unwrap(), 1);
        assert_eq!(*app.syscall_counts.get("write").unwrap(), 1);
    }
}
