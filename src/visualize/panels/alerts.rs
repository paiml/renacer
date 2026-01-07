//! Alerts panel - real-time alerting visualization (Sprint 56)
//!
//! Displays active alerts and alert history:
//! - Threshold alerts: metric exceeds/falls below threshold
//! - Rate alerts: rate of change exceeds threshold
//! - Absence alerts: expected events missing
//! - Anomaly alerts: statistical deviation detected
//!
//! # Toyota Way Principle: Andon
//!
//! Visual alert system - problems are immediately visible.
//! Red: firing, Yellow: pending, Green: resolved.

use crate::visualize::app::VisualizeApp;
use crate::visualize::theme::{borders, severity_color};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Alert severity level for display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl AlertSeverity {
    fn color(&self) -> Color {
        match self {
            AlertSeverity::Info => Color::Cyan,
            AlertSeverity::Warning => Color::Yellow,
            AlertSeverity::Critical => Color::Rgb(255, 80, 80),
        }
    }

    fn indicator(&self) -> &'static str {
        match self {
            AlertSeverity::Info => "ℹ",
            AlertSeverity::Warning => "⚠",
            AlertSeverity::Critical => "🔴",
        }
    }
}

/// Alert state for display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertState {
    Inactive,
    Pending,
    Firing,
    Resolved,
}

impl AlertState {
    fn color(&self) -> Color {
        match self {
            AlertState::Inactive => Color::DarkGray,
            AlertState::Pending => Color::Yellow,
            AlertState::Firing => Color::Rgb(255, 80, 80),
            AlertState::Resolved => Color::Green,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            AlertState::Inactive => "INACTIVE",
            AlertState::Pending => "PENDING ",
            AlertState::Firing => "FIRING  ",
            AlertState::Resolved => "RESOLVED",
        }
    }
}

/// Draw the alerts panel
pub fn draw(f: &mut Frame, app: &VisualizeApp, area: Rect) {
    let header = " Alerts ";

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(borders::STYLE)
        .border_style(Style::default().fg(borders::ALERTS))
        .title(Span::styled(
            header,
            Style::default()
                .fg(borders::ALERTS)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 4 {
        return;
    }

    // Build alerts display
    let mut lines: Vec<Line> = Vec::new();

    // Section: Active Alerts
    lines.push(Line::from(Span::styled(
        "─── Active Alerts ───",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )));

    // Check for high latency alert
    let avg_latency = app.latency_history.avg();
    let latency_alert = if avg_latency > 10000.0 {
        Some((
            AlertState::Firing,
            AlertSeverity::Critical,
            "HighLatency",
            format!("avg {:.0}μs > 10ms", avg_latency),
        ))
    } else if avg_latency > 5000.0 {
        Some((
            AlertState::Pending,
            AlertSeverity::Warning,
            "HighLatency",
            format!("avg {:.0}μs > 5ms", avg_latency),
        ))
    } else {
        None
    };

    // Check for error rate alert
    let error_rate = if app.total_syscalls > 0 {
        (app.total_errors as f64 / app.total_syscalls as f64) * 100.0
    } else {
        0.0
    };

    let error_alert = if error_rate > 10.0 {
        Some((
            AlertState::Firing,
            AlertSeverity::Critical,
            "HighErrorRate",
            format!("{:.1}% > 10%", error_rate),
        ))
    } else if error_rate > 5.0 {
        Some((
            AlertState::Pending,
            AlertSeverity::Warning,
            "HighErrorRate",
            format!("{:.1}% > 5%", error_rate),
        ))
    } else {
        None
    };

    // Check for anomaly alert
    let anomaly_alert = if app.anomaly_count > 10 {
        Some((
            AlertState::Firing,
            AlertSeverity::Critical,
            "AnomalyBurst",
            format!("{} anomalies", app.anomaly_count),
        ))
    } else if app.anomaly_count > 5 {
        Some((
            AlertState::Pending,
            AlertSeverity::Warning,
            "AnomalyBurst",
            format!("{} anomalies", app.anomaly_count),
        ))
    } else {
        None
    };

    // Check for high Z-score alert
    let max_zscore = app.anomaly_history.max();
    let zscore_alert = if max_zscore > 5.0 {
        Some((
            AlertState::Firing,
            AlertSeverity::Critical,
            "HighZScore",
            format!("{:.1}σ", max_zscore),
        ))
    } else if max_zscore > 4.0 {
        Some((
            AlertState::Pending,
            AlertSeverity::Warning,
            "HighZScore",
            format!("{:.1}σ", max_zscore),
        ))
    } else {
        None
    };

    let alerts = [latency_alert, error_alert, anomaly_alert, zscore_alert];
    let active_alerts: Vec<_> = alerts.iter().filter_map(|a| a.as_ref()).collect();

    if active_alerts.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ✓ No active alerts",
            Style::default().fg(Color::Green),
        )));
    } else {
        for (state, severity, name, detail) in &active_alerts {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", severity.indicator()),
                    Style::default().fg(severity.color()),
                ),
                Span::styled(
                    format!("[{}] ", state.label()),
                    Style::default()
                        .fg(state.color())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*name, Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(detail.clone(), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Section: Alert Summary
    lines.push(Line::from(Span::styled(
        "─── Summary ───",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    let firing_count = active_alerts
        .iter()
        .filter(|(s, _, _, _)| *s == AlertState::Firing)
        .count();
    let pending_count = active_alerts
        .iter()
        .filter(|(s, _, _, _)| *s == AlertState::Pending)
        .count();

    lines.push(Line::from(vec![
        Span::styled("Firing:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", firing_count),
            Style::default().fg(if firing_count > 0 {
                Color::Red
            } else {
                Color::Green
            }),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Pending:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", pending_count),
            Style::default().fg(if pending_count > 0 {
                Color::Yellow
            } else {
                Color::Green
            }),
        ),
    ]));

    lines.push(Line::from(""));

    // Section: Alert Thresholds (info)
    lines.push(Line::from(Span::styled(
        "─── Thresholds ───",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));

    lines.push(Line::from(vec![
        Span::styled("Latency:  ", Style::default().fg(Color::DarkGray)),
        Span::styled("5ms warn / 10ms crit", Style::default().fg(Color::White)),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Error %:  ", Style::default().fg(Color::DarkGray)),
        Span::styled("5% warn / 10% crit", Style::default().fg(Color::White)),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Anomaly:  ", Style::default().fg(Color::DarkGray)),
        Span::styled("5 warn / 10 crit", Style::default().fg(Color::White)),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Z-Score:  ", Style::default().fg(Color::DarkGray)),
        Span::styled("4σ warn / 5σ crit", Style::default().fg(Color::White)),
    ]));

    // Visual severity indicator bar
    lines.push(Line::from(""));
    let z = app.anomaly_history.max() as f32;
    let color = severity_color(z);
    let bar_len = ((z / 6.0).clamp(0.0, 1.0) * (inner.width.saturating_sub(4) as f32)) as usize;
    let bar = "█".repeat(bar_len);
    let empty = "░".repeat(inner.width.saturating_sub(4) as usize - bar_len);
    lines.push(Line::from(vec![
        Span::styled(bar, Style::default().fg(color)),
        Span::styled(empty, Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visualize::VisualizeConfig;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_draw_alerts_panel() {
        let backend = TestBackend::new(50, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = VisualizeApp::new(VisualizeConfig::default());

        terminal
            .draw(|f| {
                draw(f, &app, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);
        assert!(content.contains("Alerts"));
        assert!(content.contains("Active"));
    }

    #[test]
    fn test_draw_alerts_small_area() {
        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = VisualizeApp::new(VisualizeConfig::default());

        // Should not panic with small area
        terminal
            .draw(|f| {
                draw(f, &app, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_alert_severity_colors() {
        assert_eq!(AlertSeverity::Info.color(), Color::Cyan);
        assert_eq!(AlertSeverity::Warning.color(), Color::Yellow);
        assert!(matches!(
            AlertSeverity::Critical.color(),
            Color::Rgb(255, 80, 80)
        ));
    }

    #[test]
    fn test_alert_state_labels() {
        assert_eq!(AlertState::Inactive.label(), "INACTIVE");
        assert_eq!(AlertState::Pending.label(), "PENDING ");
        assert_eq!(AlertState::Firing.label(), "FIRING  ");
        assert_eq!(AlertState::Resolved.label(), "RESOLVED");
    }

    fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
        let mut s = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                s.push(buffer[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            s.push('\n');
        }
        s
    }
}
