//! SIMD-accelerated theme and color system for renacer visualize.
//!
//! ttop-identical btop-style dark theme with vibrant gradients.
//! Uses SIMD batch processing for color mapping via trueno-viz kernels.
//!
//! # SIMD Acceleration
//!
//! - `sparkline_batch`: Vectorized min/max finding for normalization
//! - `normalize_batch`: AVX2/NEON parallel normalization
//!
//! # Toyota Way Principle: Andon (Visual Management)
//!
//! Colors provide immediate visual feedback on system state:
//! - Green: Normal operation
//! - Yellow: Warning threshold
//! - Red: Critical/anomaly detected

use ratatui::style::Color;

/// btop-style color gradient for percentage values (0-100)
/// Uses smooth transition: cyan -> green -> yellow -> orange -> red
#[allow(clippy::many_single_char_names)]
pub fn percent_color(percent: f64) -> Color {
    // Clamp to valid range (handle NaN)
    let p = if percent.is_nan() {
        0.0
    } else {
        percent.clamp(0.0, 100.0)
    };

    // btop-style 5-stop gradient
    if p >= 90.0 {
        // Critical: bright red (255, 64, 64)
        Color::Rgb(255, 64, 64)
    } else if p >= 75.0 {
        // High: orange-red gradient
        let t = (p - 75.0) / 15.0;
        let r = 255;
        let g = (180.0 - t * 116.0) as u8;
        let b = 64;
        Color::Rgb(r, g, b)
    } else if p >= 50.0 {
        // Medium-high: yellow to orange
        let t = (p - 50.0) / 25.0;
        let r = 255;
        let g = (220.0 - t * 40.0) as u8;
        let b = 64;
        Color::Rgb(r, g, b)
    } else if p >= 25.0 {
        // Medium-low: green to yellow
        let t = (p - 25.0) / 25.0;
        let r = (100.0 + t * 155.0) as u8;
        let g = 220;
        let b = (100.0 - t * 36.0) as u8;
        Color::Rgb(r, g, b)
    } else {
        // Low: cyan to green
        let t = p / 25.0;
        let r = (64.0 + t * 36.0) as u8;
        let g = (180.0 + t * 40.0) as u8;
        let b = (220.0 - t * 120.0) as u8;
        Color::Rgb(r, g, b)
    }
}

/// Severity color gradient for anomaly Z-scores
/// Maps standard deviations to visual urgency
pub fn severity_color(z_score: f32) -> Color {
    let z = if z_score.is_nan() { 0.0 } else { z_score.abs() };

    if z >= 5.0 {
        // High severity: bright red
        Color::Rgb(255, 64, 64)
    } else if z >= 4.0 {
        // Medium severity: orange
        Color::Rgb(255, 180, 100)
    } else if z >= 3.0 {
        // Low severity: yellow
        Color::Rgb(220, 220, 80)
    } else {
        // Normal: green
        Color::Rgb(100, 220, 100)
    }
}

/// Latency color for syscall duration visualization
/// Maps microseconds to urgency colors
pub fn latency_color(duration_us: u64) -> Color {
    if duration_us > 100_000 {
        // >100ms: critical red
        Color::Rgb(255, 64, 64)
    } else if duration_us > 10_000 {
        // >10ms: orange
        Color::Rgb(255, 180, 100)
    } else if duration_us > 1_000 {
        // >1ms: yellow
        Color::Rgb(220, 220, 80)
    } else if duration_us > 100 {
        // >100us: green
        Color::Rgb(100, 220, 100)
    } else {
        // <100us: cyan (fast)
        Color::Rgb(64, 180, 220)
    }
}

/// Panel border colors - btop-style vibrant distinct colors
pub mod borders {
    use ratatui::style::Color;
    use ratatui::widgets::BorderType;

    // btop uses vibrant, saturated colors for borders
    pub const SYSCALL_HEATMAP: Color = Color::Rgb(100, 200, 255); // Bright cyan
    pub const ANOMALY_TIMELINE: Color = Color::Rgb(255, 100, 100); // Red
    pub const ML_SCATTER: Color = Color::Rgb(180, 120, 255); // Purple
    pub const TRACE_WATERFALL: Color = Color::Rgb(255, 150, 100); // Orange
    pub const PROCESS_SYSCALLS: Color = Color::Rgb(220, 180, 100); // Gold
    pub const STATS_SUMMARY: Color = Color::Rgb(100, 255, 150); // Bright green
    pub const HELP: Color = Color::Rgb(180, 180, 180); // Gray

    // Sprint 56: Metrics and Alerting panels
    pub const METRICS: Color = Color::Rgb(100, 180, 255); // Light blue
    pub const ALERTS: Color = Color::Rgb(255, 80, 80); // Bright red

    /// Rounded border style for btop-like appearance
    pub const STYLE: BorderType = BorderType::Rounded;
}

/// Graph colors - high contrast for visibility
pub mod graph {
    use ratatui::style::Color;

    // btop-style graph colors: bright and distinct
    pub const SYSCALL_FILE: Color = Color::Rgb(100, 200, 255); // Cyan
    pub const SYSCALL_NET: Color = Color::Rgb(180, 120, 255); // Purple
    pub const SYSCALL_MEM: Color = Color::Rgb(100, 255, 150); // Green
    pub const SYSCALL_PROC: Color = Color::Rgb(255, 180, 100); // Orange
    pub const SYSCALL_OTHER: Color = Color::Rgb(180, 180, 180); // Gray

    pub const ANOMALY: Color = Color::Rgb(255, 100, 100); // Red
    pub const LATENCY: Color = Color::Rgb(220, 220, 80); // Yellow
    pub const CLUSTER_0: Color = Color::Rgb(100, 200, 255); // Cyan
    pub const CLUSTER_1: Color = Color::Rgb(180, 120, 255); // Purple
    pub const CLUSTER_2: Color = Color::Rgb(100, 255, 150); // Green
    pub const CLUSTER_3: Color = Color::Rgb(255, 180, 100); // Orange
    pub const OUTLIER: Color = Color::Rgb(255, 64, 64); // Red
}

/// Process state colors
pub mod process_state {
    use ratatui::style::Color;

    pub const RUNNING: Color = Color::Rgb(100, 255, 100); // Bright green
    pub const SLEEPING: Color = Color::Rgb(120, 120, 140); // Gray
    pub const DISK_WAIT: Color = Color::Rgb(255, 200, 100); // Yellow-orange
    pub const ZOMBIE: Color = Color::Rgb(255, 80, 80); // Red
    pub const STOPPED: Color = Color::Rgb(255, 150, 100); // Orange
    pub const UNKNOWN: Color = Color::Rgb(180, 180, 180); // Light gray
}

/// Sparkline characters (8-level Unicode blocks)
pub const SPARKLINE_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Generate sparkline string from values using SIMD-accelerated min/max
///
/// Uses trueno-viz `simd_min`/`simd_max` for AVX2/NEON vectorized range finding.
pub fn sparkline(values: &[f64], max_width: usize) -> String {
    if values.is_empty() {
        return String::new();
    }

    // Use SIMD for min/max finding
    let min = trueno_viz::monitor::simd::kernels::simd_min(values);
    let max = trueno_viz::monitor::simd::kernels::simd_max(values);
    let range = max - min;

    values
        .iter()
        .take(max_width)
        .map(|v| {
            if range == 0.0 || v.is_nan() {
                SPARKLINE_CHARS[0]
            } else {
                let normalized = ((v - min) / range).clamp(0.0, 1.0);
                let idx = (normalized * 7.0).round() as usize;
                SPARKLINE_CHARS[idx.min(7)]
            }
        })
        .collect()
}

/// Normalize values to 0.0-1.0 range using SIMD acceleration
///
/// Uses trueno-viz `simd_normalize` for AVX2/NEON parallel normalization.
pub fn normalize_batch(values: &[f64]) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }
    let max = trueno_viz::monitor::simd::kernels::simd_max(values);
    if max == 0.0 {
        return vec![0.0; values.len()];
    }
    trueno_viz::monitor::simd::kernels::simd_normalize(values, max)
}

/// Format bytes to human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1}T", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{bytes}B")
    }
}

/// Format duration in microseconds to human-readable string
pub fn format_duration_us(us: u64) -> String {
    if us >= 1_000_000 {
        format!("{:.1}s", us as f64 / 1_000_000.0)
    } else if us >= 1_000 {
        format!("{:.1}ms", us as f64 / 1_000.0)
    } else {
        format!("{us}us")
    }
}

/// Format rate (calls per second)
pub fn format_rate(rate: f64) -> String {
    if rate >= 1_000_000.0 {
        format!("{:.1}M/s", rate / 1_000_000.0)
    } else if rate >= 1_000.0 {
        format!("{:.1}K/s", rate / 1_000.0)
    } else {
        format!("{:.0}/s", rate)
    }
}

/// Format Z-score with severity indicator
pub fn format_zscore(z: f32) -> String {
    let indicator = if z >= 5.0 {
        "!!!"
    } else if z >= 4.0 {
        "!!"
    } else if z >= 3.0 {
        "!"
    } else {
        ""
    };
    format!("{z:.1}σ{indicator}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_color_ranges() {
        // Should not panic for any valid percentage
        for p in 0..=100 {
            let _ = percent_color(p as f64);
        }
    }

    #[test]
    fn test_percent_color_gradient() {
        // Low values should have high blue component
        if let Color::Rgb(_, _, b) = percent_color(10.0) {
            assert!(b > 150, "Low percent should have blue tint");
        }

        // High values should have high red component
        if let Color::Rgb(r, _, _) = percent_color(95.0) {
            assert_eq!(r, 255, "High percent should be red");
        }
    }

    #[test]
    fn test_percent_color_handles_edge_cases() {
        // Should not panic for edge cases
        let _ = percent_color(-10.0);
        let _ = percent_color(150.0);
        let _ = percent_color(0.0);
        let _ = percent_color(100.0);
        let _ = percent_color(f64::NAN);
        let _ = percent_color(f64::INFINITY);
    }

    #[test]
    fn test_severity_color_thresholds() {
        // Normal (z < 3)
        if let Color::Rgb(_, g, _) = severity_color(2.0) {
            assert!(g > 200, "Normal should be green");
        }

        // High severity (z >= 5)
        if let Color::Rgb(r, _, _) = severity_color(5.5) {
            assert_eq!(r, 255, "High severity should be red");
        }
    }

    #[test]
    fn test_sparkline() {
        let values = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        let spark = sparkline(&values, 10);
        assert_eq!(spark.chars().count(), 5);
        assert!(spark.contains('▁'));
        assert!(spark.contains('█'));
    }

    #[test]
    fn test_sparkline_empty() {
        let spark = sparkline(&[], 10);
        assert!(spark.is_empty());
    }

    #[test]
    fn test_sparkline_constant() {
        let values = vec![5.0, 5.0, 5.0];
        let spark = sparkline(&values, 10);
        // All same value should use lowest bar
        assert_eq!(spark, "▁▁▁");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500B");
        assert_eq!(format_bytes(1024), "1.0K");
        assert_eq!(format_bytes(1024 * 1024), "1.0M");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0G");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_us(500), "500us");
        assert_eq!(format_duration_us(1500), "1.5ms");
        assert_eq!(format_duration_us(1_500_000), "1.5s");
    }

    #[test]
    fn test_format_rate() {
        assert_eq!(format_rate(100.0), "100/s");
        assert_eq!(format_rate(1500.0), "1.5K/s");
        assert_eq!(format_rate(1_500_000.0), "1.5M/s");
    }

    #[test]
    fn test_format_zscore() {
        assert_eq!(format_zscore(2.0), "2.0σ");
        assert_eq!(format_zscore(3.5), "3.5σ!");
        assert_eq!(format_zscore(4.5), "4.5σ!!");
        assert_eq!(format_zscore(5.5), "5.5σ!!!");
    }

    #[test]
    fn test_normalize_batch() {
        let values = vec![0.0, 25.0, 50.0, 75.0, 100.0];
        let normalized = normalize_batch(&values);
        assert_eq!(normalized.len(), 5);
        assert!((normalized[0] - 0.0).abs() < 0.001);
        assert!((normalized[4] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_normalize_batch_empty() {
        let normalized = normalize_batch(&[]);
        assert!(normalized.is_empty());
    }

    #[test]
    fn test_normalize_batch_zeros() {
        let values = vec![0.0, 0.0, 0.0];
        let normalized = normalize_batch(&values);
        assert!(normalized.iter().all(|&v| v == 0.0));
    }
}
