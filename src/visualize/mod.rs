//! Real-Time Tracing Visualization Module (Sprint 52-55)
//!
//! Provides terminal-based visualization of syscall traces, anomalies, ML clustering,
//! and OpenTelemetry spans using ttop-identical architecture and performance characteristics.
//!
//! # Architecture
//!
//! ```text
//! ptrace syscall intercept → Tracer Thread → mpsc channel → VisualizeApp → ratatui → Terminal
//!                                   ↓
//!                           stdout → /dev/null (suppressed)
//! ```
//!
//! # Bridge Design (QA Falsification Protocol Findings)
//!
//! The tracer runs in a background thread and communicates via unbounded mpsc channel:
//!
//! - **Stdout Suppression**: Tracer thread redirects stdout to /dev/null before tracing.
//!   Without this, tracer output corrupts the TUI (print_syscall_entry, stats summaries).
//! - **Channel Draining**: UI thread drains all pending events every 50ms tick via try_recv().
//! - **Lifecycle**: When traced process exits, tracer thread finishes, UI shows "TRACE COMPLETE"
//!   banner and waits for 'q' to exit.
//! - **Panic Safety**: TerminalGuard ensures raw mode cleanup even on panic.
//!
//! # Performance Targets (ttop-identical)
//!
//! - Event tick rate: 50ms
//! - Average frame time: 8ms
//! - P99 frame time: <16ms
//! - Memory: <10MB RSS
//!
//! # Toyota Way Principles
//!
//! - **Genchi Genbutsu**: Direct syscall observation, no intermediaries
//! - **Jidoka**: Automatic anomaly detection with visual alerts (Andon)
//! - **Muda**: Zero-copy ring buffers, no allocation in hot path
//! - **Poka-Yoke**: Type-safe panel IDs prevent configuration errors

pub mod app;
pub mod collectors;
pub mod panels;
pub mod ring_buffer;
pub mod theme;
pub mod ui;
pub mod widgets;

// Re-exports for convenience
pub use app::VisualizeApp;
pub use collectors::{AnomalyCollector, Collector, SpanReceiver, SyscallCollector};
pub use ring_buffer::HistoryBuffer;
pub use theme::{percent_color, severity_color};

use crate::tracer::{self, TracerConfig, VisualizerEvent};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Configuration for the visualize subcommand
#[derive(Debug, Clone)]
pub struct VisualizeConfig {
    /// Tick rate in milliseconds (default: 50, matches ttop)
    pub tick_rate_ms: u64,

    /// Enable anomaly detection panel
    pub enable_anomaly: bool,

    /// Enable ML clustering panel
    pub enable_ml: bool,

    /// Number of ML clusters (default: 3)
    pub ml_clusters: usize,

    /// Anomaly Z-score threshold (default: 3.0)
    pub anomaly_threshold: f32,

    /// History buffer size (default: 300, matches ttop)
    pub history_size: usize,

    /// Enable deterministic mode for testing
    pub deterministic: bool,

    /// Show FPS overlay
    pub show_fps: bool,

    /// Process ID to attach to (None = trace command)
    pub pid: Option<i32>,

    /// Enable source correlation
    pub enable_source: bool,

    /// Filter expression
    pub filter: Option<String>,

    /// OTLP endpoint for span export
    pub otlp_endpoint: Option<String>,
}

impl Default for VisualizeConfig {
    fn default() -> Self {
        Self {
            tick_rate_ms: 50,
            enable_anomaly: true,
            enable_ml: false,
            ml_clusters: 3,
            anomaly_threshold: 3.0,
            history_size: 300,
            deterministic: false,
            show_fps: false,
            pid: None,
            enable_source: false,
            filter: None,
            otlp_endpoint: None,
        }
    }
}

/// Inject synthetic syscall data for demo mode
fn inject_demo_data(app: &mut app::VisualizeApp, tick: u64) {
    // Simple LCG for deterministic pseudo-random numbers
    let mut seed = tick.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut next_rand = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        seed
    };

    // Syscall names by category for realistic distribution
    const FILE_SYSCALLS: &[&str] = &["read", "write", "open", "close", "stat", "fstat", "lseek"];
    const NET_SYSCALLS: &[&str] = &["socket", "connect", "sendto", "recvfrom", "bind", "listen"];
    const MEM_SYSCALLS: &[&str] = &["mmap", "munmap", "brk", "mprotect"];
    const PROC_SYSCALLS: &[&str] = &["fork", "clone", "execve", "wait4", "getpid"];

    // Inject 5-20 syscalls per tick for realistic rate
    let count = 5 + (next_rand() % 16) as usize;

    for _ in 0..count {
        let r = next_rand();
        let (syscalls, base_duration) = match r % 100 {
            0..=49 => (FILE_SYSCALLS, 50),  // 50% file I/O
            50..=74 => (NET_SYSCALLS, 200), // 25% network
            75..=89 => (MEM_SYSCALLS, 20),  // 15% memory
            _ => (PROC_SYSCALLS, 500),      // 10% process
        };

        let name = syscalls[(next_rand() as usize) % syscalls.len()];

        // Duration with variance (some outliers for anomaly detection)
        let duration = if next_rand() % 100 < 3 {
            // 3% chance of anomaly (10-100x normal)
            base_duration * (10 + (next_rand() % 90))
        } else {
            base_duration + (next_rand() % (base_duration * 2))
        };

        // 2% error rate
        let result = if next_rand() % 100 < 2 { -1 } else { 0 };

        app.record_syscall(name, duration, result);

        // Occasionally record anomalies for high z-scores
        if duration > base_duration * 5 {
            let z_score = (duration as f32 / base_duration as f32).min(10.0);
            app.record_anomaly(
                name.to_string(),
                duration,
                z_score,
                Some("demo.rs".to_string()),
                Some((tick % 500) as u32 + 1),
            );
        }
    }
}

/// Redirect stdout and stderr to /dev/null to prevent tracer/child output from corrupting TUI
///
/// This is called in the tracer thread before running trace_command/attach_to_pid.
/// Without this, print_syscall_entry(), stats summaries, and child process output
/// would write to the same terminal that ratatui is using for TUI rendering.
fn suppress_stdio() {
    if let Ok(devnull) = File::open("/dev/null") {
        let devnull_fd = devnull.as_raw_fd();
        unsafe {
            // Duplicate /dev/null to stdout (fd 1) and stderr (fd 2)
            libc::dup2(devnull_fd, libc::STDOUT_FILENO);
            libc::dup2(devnull_fd, libc::STDERR_FILENO);
        }
        // devnull File is dropped here, but the dup'd fds stay open
    }
}

/// Run the visualization TUI
///
/// # Arguments
///
/// * `command` - Command to trace (with arguments)
/// * `config` - Visualization configuration
///
/// # Returns
///
/// Exit code from the traced process (0 if successful)
pub fn run_visualize(command: Option<&[String]>, config: VisualizeConfig) -> Result<i32> {
    // Determine if we should run in demo mode (no command or pid specified)
    let demo_mode = command.is_none() && config.pid.is_none();

    // Sprint 52-55: Create channel for tracer-to-visualizer bridge
    let (tx, rx) = mpsc::channel::<VisualizerEvent>();

    // Spawn tracer in background thread if we have a command or PID
    let mut tracer_handle = if let Some(cmd) = command {
        let cmd_owned: Vec<String> = cmd.to_vec();
        let tracer_config = TracerConfig {
            visualizer_sink: Some(tx),
            ..Default::default()
        };
        Some(thread::spawn(move || {
            // Redirect stdout/stderr to /dev/null to prevent tracer/child output corrupting TUI
            suppress_stdio();
            tracer::trace_command(&cmd_owned, tracer_config)
        }))
    } else if let Some(pid) = config.pid {
        let tracer_config = TracerConfig {
            visualizer_sink: Some(tx),
            ..Default::default()
        };
        Some(thread::spawn(move || {
            // Redirect stdout/stderr to /dev/null to prevent tracer/child output corrupting TUI
            suppress_stdio();
            tracer::attach_to_pid(pid, tracer_config)
        }))
    } else {
        // Demo mode - no tracer, drop tx so rx doesn't block
        drop(tx);
        None
    };

    // Setup terminal with panic guard for cleanup
    // CRITICAL: Use /dev/tty directly instead of stdout because suppress_stdio()
    // in the tracer thread redirects stdout to /dev/null. /dev/tty bypasses this.
    enable_raw_mode()?;

    // Open /dev/tty for direct terminal access (immune to stdout redirection)
    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .unwrap_or_else(|_| {
            // Fallback to stdout if /dev/tty not available (e.g., in CI)
            // This uses a trick: we get a File from /proc/self/fd/1
            OpenOptions::new()
                .write(true)
                .open("/proc/self/fd/1")
                .expect("Failed to open terminal")
        });

    // Create a cleanup guard to ensure terminal is restored on panic
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            // Use /dev/tty for cleanup too
            if let Ok(mut tty) = OpenOptions::new().write(true).open("/dev/tty") {
                let _ = execute!(tty, LeaveAlternateScreen, DisableMouseCapture);
            }
        }
    }
    let _guard = TerminalGuard;

    execute!(tty, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(tty);
    let mut terminal = Terminal::new(backend)?;

    // Create app state with receiver (None in demo mode since tx was dropped)
    let receiver = if tracer_handle.is_some() {
        Some(rx)
    } else {
        None
    };
    let mut app = app::VisualizeApp::with_receiver(config.clone(), receiver);

    // Demo mode RNG state
    let mut demo_tick: u64 = 0;

    // Main event loop
    let tick_rate = Duration::from_millis(config.tick_rate_ms);
    let mut last_tick = Instant::now();
    let mut frame_times: Vec<u64> = Vec::with_capacity(60);

    let result = loop {
        // Draw UI
        let frame_start = Instant::now();
        terminal.draw(|f| ui::draw(f, &mut app))?;
        let frame_time = frame_start.elapsed().as_micros() as u64;

        // Track frame timing
        frame_times.push(frame_time);
        if frame_times.len() > 60 {
            frame_times.remove(0);
        }
        app.avg_frame_time_us = frame_times.iter().sum::<u64>() / frame_times.len() as u64;
        app.max_frame_time_us = *frame_times.iter().max().unwrap_or(&0);
        app.frame_id += 1;

        // Handle events
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Check for quit
                if key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    break Ok(0);
                }

                // Handle other keys
                app.handle_key(key.code, key.modifiers);
            }
        }

        // Periodic collection
        if last_tick.elapsed() >= tick_rate {
            // Demo mode: inject synthetic syscall data
            if demo_mode {
                inject_demo_data(&mut app, demo_tick);
                demo_tick = demo_tick.wrapping_add(1);
            }

            app.collect_metrics();
            last_tick = Instant::now();
        }

        // Check if we should exit
        if app.should_exit {
            break Ok(app.exit_code);
        }

        // Check if tracer thread finished (traced process exited)
        // Keep UI open to show final results - user must press 'q' to exit
        if tracer_handle
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            // Drain remaining events
            app.collect_metrics();

            // Mark trace as complete
            app.trace_complete = true;

            // Drop the receiver to stop polling dead channel
            app.event_receiver = None;

            // Join the tracer thread
            if let Some(handle) = tracer_handle.take() {
                let _ = handle.join();
            }

            // Re-enable raw mode in case child process affected terminal
            let _ = enable_raw_mode();
        }
    };

    // Clean up tracer thread if still running
    if let Some(handle) = tracer_handle {
        // Don't block indefinitely - the traced process may have exited
        let _ = handle.join();
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = VisualizeConfig::default();
        assert_eq!(config.tick_rate_ms, 50);
        assert_eq!(config.history_size, 300);
        assert!(config.enable_anomaly);
        assert!(!config.enable_ml);
    }

    #[test]
    fn test_config_clone() {
        let config = VisualizeConfig {
            tick_rate_ms: 100,
            enable_ml: true,
            ml_clusters: 5,
            ..Default::default()
        };
        let cloned = config.clone();
        assert_eq!(cloned.tick_rate_ms, 100);
        assert!(cloned.enable_ml);
        assert_eq!(cloned.ml_clusters, 5);
    }
}
