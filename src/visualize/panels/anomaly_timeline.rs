//! Anomaly timeline panel - Z-score visualization with source correlation

use crate::visualize::app::VisualizeApp;
use crate::visualize::theme::{
    borders, format_duration_us, format_zscore, severity_color, sparkline,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

/// Draw the anomaly timeline panel
pub fn draw(f: &mut Frame, app: &VisualizeApp, area: Rect) {
    // Build header with stats
    let header = format!(
        " Anomalies {} │ threshold: {}σ ",
        app.anomaly_count, app.config.anomaly_threshold
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(borders::STYLE)
        .border_style(Style::default().fg(borders::ANOMALY_TIMELINE))
        .title(Span::styled(
            header,
            Style::default()
                .fg(borders::ANOMALY_TIMELINE)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    // Split into sparkline and table
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);

    // Draw Z-score sparkline
    let z_history: Vec<f64> = app.anomaly_history.to_vec();
    let spark = sparkline(&z_history, chunks[0].width.saturating_sub(12) as usize);

    let spark_line = Line::from(vec![
        Span::styled("Z-score ", Style::default().fg(Color::DarkGray)),
        Span::styled(spark, Style::default().fg(borders::ANOMALY_TIMELINE)),
    ]);
    let spark_para = Paragraph::new(spark_line);
    f.render_widget(spark_para, chunks[0]);

    // Draw anomaly table
    if chunks[1].height < 2 {
        return;
    }

    let header_cells = ["Syscall", "Duration", "Z-Score", "Source"]
        .iter()
        .map(|h| Span::styled(*h, Style::default().add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .anomalies
        .iter()
        .rev()
        .take(chunks[1].height.saturating_sub(1) as usize)
        .map(|anomaly| {
            let z_color = severity_color(anomaly.z_score);
            let source = anomaly.source_file.as_ref().map_or_else(
                || "-".to_string(),
                |f| {
                    let line = anomaly.source_line.unwrap_or(0);
                    format!("{}:{}", f.rsplit('/').next().unwrap_or(f), line)
                },
            );

            Row::new(vec![
                Span::styled(anomaly.syscall.clone(), Style::default().fg(Color::White)),
                Span::styled(
                    format_duration_us(anomaly.duration_us),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(format_zscore(anomaly.z_score), Style::default().fg(z_color)),
                Span::styled(source, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Min(10),
    ];

    let table = Table::new(rows, widths).header(header).column_spacing(1);

    f.render_widget(table, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visualize::VisualizeConfig;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_draw_anomaly_timeline() {
        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = VisualizeApp::new(VisualizeConfig::default());

        terminal
            .draw(|f| {
                draw(f, &app, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);
        assert!(content.contains("Anomalies"));
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
