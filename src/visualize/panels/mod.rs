//! Panel implementations for renacer visualize
//!
//! Each panel provides a specific view of tracing data:
//! - syscall_heatmap: Syscall category activity over time
//! - anomaly_timeline: Z-score anomaly visualization
//! - ml_scatter: ML clustering visualization (braille scatter)
//! - trace_waterfall: OTLP span Gantt chart
//! - process_syscalls: Per-process syscall breakdown
//! - stats_summary: Aggregate statistics

pub mod anomaly_timeline;
pub mod ml_scatter;
pub mod process_syscalls;
pub mod stats_summary;
pub mod syscall_heatmap;
pub mod trace_waterfall;

/// Panel identifier for type-safe operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    SyscallHeatmap,
    AnomalyTimeline,
    MlScatter,
    TraceWaterfall,
    ProcessSyscalls,
    StatsSummary,
}

impl PanelId {
    /// Get display name for panel
    pub fn name(&self) -> &'static str {
        match self {
            Self::SyscallHeatmap => "Syscalls",
            Self::AnomalyTimeline => "Anomalies",
            Self::MlScatter => "ML Clusters",
            Self::TraceWaterfall => "Trace",
            Self::ProcessSyscalls => "Processes",
            Self::StatsSummary => "Stats",
        }
    }

    /// Get keyboard shortcut
    pub fn shortcut(&self) -> char {
        match self {
            Self::SyscallHeatmap => '1',
            Self::AnomalyTimeline => '2',
            Self::MlScatter => '3',
            Self::TraceWaterfall => '4',
            Self::ProcessSyscalls => '5',
            Self::StatsSummary => '6',
        }
    }
}
