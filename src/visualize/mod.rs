//! Real-Time Tracing Visualization Module (Sprint 52-55)
//!
//! Provides terminal-based visualization of syscall traces, anomalies, ML clustering,
//! and OpenTelemetry spans using ttop-identical architecture and performance characteristics.
//!
//! # Architecture
//!
//! ```text
//! ptrace syscall intercept → Collectors → VisualizeApp → Panel Router → ratatui → Terminal
//! ```
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
pub mod panels;
pub mod ring_buffer;
pub mod theme;
pub mod ui;
pub mod widgets;

// Re-exports for convenience
pub use app::VisualizeApp;
pub use ring_buffer::HistoryBuffer;
pub use theme::{percent_color, severity_color};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
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
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = VisualizeApp::new(config.clone());

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
                    || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    break Ok(0);
                }

                // Handle other keys
                app.handle_key(key.code, key.modifiers);
            }
        }

        // Periodic collection
        if last_tick.elapsed() >= tick_rate {
            app.collect_metrics();
            last_tick = Instant::now();
        }

        // Check if we should exit
        if app.should_exit {
            break Ok(app.exit_code);
        }
    };

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
