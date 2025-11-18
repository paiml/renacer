//! HTML output format for syscall trace reports
//!
//! Sprint 22: Rich visual reports with styled tables and embedded CSS

use crate::stats::StatsTracker;

/// HTML record for a single syscall event
#[derive(Debug, Clone)]
pub struct HtmlSyscall {
    pub name: String,
    pub arguments: String,
    pub result: i64,
    pub duration_us: Option<u64>,
    pub source_location: Option<String>,
}

/// HTML output formatter
#[derive(Debug)]
pub struct HtmlOutput {
    syscalls: Vec<HtmlSyscall>,
    include_timing: bool,
    include_source: bool,
}

impl HtmlOutput {
    /// Create a new HTML output formatter
    pub fn new(include_timing: bool, include_source: bool) -> Self {
        Self {
            syscalls: Vec::new(),
            include_timing,
            include_source,
        }
    }

    /// Add a syscall to the output
    pub fn add_syscall(&mut self, syscall: HtmlSyscall) {
        self.syscalls.push(syscall);
    }

    /// Escape HTML special characters to prevent XSS
    fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    /// Generate embedded CSS styles
    fn generate_styles() -> &'static str {
        r#"
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 20px;
            background-color: #f5f5f5;
        }
        h1, h2 {
            color: #333;
        }
        table {
            border-collapse: collapse;
            width: 100%;
            background-color: white;
            box-shadow: 0 1px 3px rgba(0,0,0,0.1);
            margin-bottom: 20px;
        }
        th, td {
            border: 1px solid #ddd;
            padding: 8px;
            text-align: left;
        }
        th {
            background-color: #4a90d9;
            color: white;
            font-weight: bold;
        }
        tr:nth-child(even) {
            background-color: #f9f9f9;
        }
        tr:hover {
            background-color: #f0f0f0;
        }
        .syscall {
            color: #0066cc;
            font-weight: bold;
            font-family: monospace;
        }
        .args {
            font-family: monospace;
            font-size: 0.9em;
            color: #555;
        }
        .result {
            font-family: monospace;
        }
        .result-error {
            color: #cc0000;
        }
        .duration {
            font-family: monospace;
            color: #666;
        }
        .source {
            font-size: 0.85em;
            color: #888;
        }
        .stats-table {
            margin-top: 20px;
        }
        .stats-table th {
            background-color: #5cb85c;
        }
        .footer {
            margin-top: 20px;
            font-size: 0.8em;
            color: #888;
            text-align: center;
        }
        "#
    }

    /// Generate HTML table header
    fn generate_header(&self) -> String {
        let mut headers = vec!["Syscall", "Arguments", "Result"];

        if self.include_timing {
            headers.push("Duration");
        }

        if self.include_source {
            headers.push("Source");
        }

        let header_cells: Vec<String> = headers.iter().map(|h| format!("<th>{}</th>", h)).collect();

        format!("<tr>{}</tr>", header_cells.join(""))
    }

    /// Format a syscall as HTML table row
    fn format_syscall_row(&self, syscall: &HtmlSyscall) -> String {
        let result_class = if syscall.result < 0 {
            "result result-error"
        } else {
            "result"
        };

        let mut cells = vec![
            format!(
                r#"<td class="syscall">{}</td>"#,
                Self::escape_html(&syscall.name)
            ),
            format!(
                r#"<td class="args">{}</td>"#,
                Self::escape_html(&syscall.arguments)
            ),
            format!(r#"<td class="{}">{}</td>"#, result_class, syscall.result),
        ];

        if self.include_timing {
            let duration_text = match syscall.duration_us {
                Some(d) => format!("{} us", d),
                None => String::new(),
            };
            cells.push(format!(
                r#"<td class="duration">{}</td>"#,
                Self::escape_html(&duration_text)
            ));
        }

        if self.include_source {
            let source_text = syscall.source_location.as_deref().unwrap_or("");
            cells.push(format!(
                r#"<td class="source">{}</td>"#,
                Self::escape_html(source_text)
            ));
        }

        format!("<tr>{}</tr>", cells.join(""))
    }

    /// Generate complete HTML document
    pub fn to_html(&self, stats: Option<&StatsTracker>) -> String {
        let mut html = String::new();

        // DOCTYPE and HTML start
        html.push_str("<!DOCTYPE html>\n");
        html.push_str("<html lang=\"en\">\n");

        // Head section
        html.push_str("<head>\n");
        html.push_str("    <meta charset=\"UTF-8\">\n");
        html.push_str(
            "    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n",
        );
        html.push_str("    <title>Renacer Trace Report</title>\n");
        html.push_str("    <style>");
        html.push_str(Self::generate_styles());
        html.push_str("</style>\n");
        html.push_str("</head>\n");

        // Body section
        html.push_str("<body>\n");
        html.push_str("    <h1>Syscall Trace Report</h1>\n");

        // Syscall trace table
        html.push_str("    <table>\n");
        html.push_str("        ");
        html.push_str(&self.generate_header());
        html.push('\n');

        for syscall in &self.syscalls {
            html.push_str("        ");
            html.push_str(&self.format_syscall_row(syscall));
            html.push('\n');
        }

        html.push_str("    </table>\n");

        // Statistics section (if provided)
        if let Some(tracker) = stats {
            html.push_str(&self.render_statistics(tracker));
        }

        // Footer
        html.push_str("    <div class=\"footer\">\n");
        html.push_str("        Generated by Renacer - System Call Tracer\n");
        html.push_str("    </div>\n");

        html.push_str("</body>\n");
        html.push_str("</html>\n");

        html
    }

    /// Render statistics as HTML table
    fn render_statistics(&self, tracker: &StatsTracker) -> String {
        let mut html = String::new();

        html.push_str("    <h2>Statistics Summary</h2>\n");
        html.push_str("    <table class=\"stats-table\">\n");
        html.push_str("        <tr><th>% time</th><th>seconds</th><th>usecs/call</th><th>calls</th><th>errors</th><th>syscall</th></tr>\n");

        let stats = tracker.stats_map();
        let total_time: u64 = stats.values().map(|s| s.total_time_us).sum();

        // Sort by total time (descending)
        let mut sorted_stats: Vec<_> = stats.iter().collect();
        sorted_stats.sort_by(|a, b| b.1.total_time_us.cmp(&a.1.total_time_us));

        for (name, stat) in sorted_stats {
            let pct = if total_time > 0 {
                (stat.total_time_us as f64 / total_time as f64) * 100.0
            } else {
                0.0
            };

            let usecs_per_call = if stat.count > 0 {
                stat.total_time_us / stat.count
            } else {
                0
            };

            let seconds = stat.total_time_us as f64 / 1_000_000.0;

            html.push_str(&format!(
                "        <tr><td>{:.2}</td><td>{:.6}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"syscall\">{}</td></tr>\n",
                pct,
                seconds,
                usecs_per_call,
                stat.count,
                stat.errors,
                Self::escape_html(name)
            ));
        }

        html.push_str("    </table>\n");

        html
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape() {
        assert_eq!(HtmlOutput::escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(HtmlOutput::escape_html("a&b"), "a&amp;b");
        assert_eq!(HtmlOutput::escape_html("\"test\""), "&quot;test&quot;");
        assert_eq!(HtmlOutput::escape_html("'test'"), "&#39;test&#39;");
    }

    #[test]
    fn test_html_output_new() {
        let output = HtmlOutput::new(true, true);
        assert!(output.include_timing);
        assert!(output.include_source);
        assert!(output.syscalls.is_empty());
    }

    #[test]
    fn test_html_output_add_syscall() {
        let mut output = HtmlOutput::new(false, false);
        output.add_syscall(HtmlSyscall {
            name: "write".to_string(),
            arguments: "1, \"test\", 4".to_string(),
            result: 4,
            duration_us: None,
            source_location: None,
        });
        assert_eq!(output.syscalls.len(), 1);
        assert_eq!(output.syscalls[0].name, "write");
    }

    #[test]
    fn test_html_output_basic_structure() {
        let output = HtmlOutput::new(false, false);
        let html = output.to_html(None);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<html"));
        assert!(html.contains("</html>"));
        assert!(html.contains("<head>"));
        assert!(html.contains("<body>"));
        assert!(html.contains("<style>"));
        assert!(html.contains("<table"));
    }

    #[test]
    fn test_html_output_with_syscall() {
        let mut output = HtmlOutput::new(false, false);
        output.add_syscall(HtmlSyscall {
            name: "write".to_string(),
            arguments: "1, \"hello\", 5".to_string(),
            result: 5,
            duration_us: None,
            source_location: None,
        });

        let html = output.to_html(None);
        assert!(html.contains("write"));
        assert!(html.contains("<tr"));
        assert!(html.contains("<td"));
    }

    #[test]
    fn test_html_output_escape_xss() {
        let mut output = HtmlOutput::new(false, false);
        output.add_syscall(HtmlSyscall {
            name: "write".to_string(),
            arguments: "<script>alert('xss')</script>".to_string(),
            result: 0,
            duration_us: None,
            source_location: None,
        });

        let html = output.to_html(None);
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_html_output_with_timing() {
        let mut output = HtmlOutput::new(true, false);
        output.add_syscall(HtmlSyscall {
            name: "write".to_string(),
            arguments: "1, \"test\", 4".to_string(),
            result: 4,
            duration_us: Some(1234),
            source_location: None,
        });

        let html = output.to_html(None);
        assert!(html.contains("Duration"));
        assert!(html.contains("1234 us"));
    }

    #[test]
    fn test_html_output_with_source() {
        let mut output = HtmlOutput::new(false, true);
        output.add_syscall(HtmlSyscall {
            name: "write".to_string(),
            arguments: "1, \"test\", 4".to_string(),
            result: 4,
            duration_us: None,
            source_location: Some("src/main.rs:42".to_string()),
        });

        let html = output.to_html(None);
        assert!(html.contains("Source"));
        assert!(html.contains("src/main.rs:42"));
    }

    #[test]
    fn test_html_output_error_result() {
        let mut output = HtmlOutput::new(false, false);
        output.add_syscall(HtmlSyscall {
            name: "open".to_string(),
            arguments: "\"/nonexistent\"".to_string(),
            result: -2,
            duration_us: None,
            source_location: None,
        });

        let html = output.to_html(None);
        assert!(html.contains("result-error"));
        assert!(html.contains("-2"));
    }

    #[test]
    fn test_html_output_header_columns() {
        let output = HtmlOutput::new(true, true);
        let header = output.generate_header();

        assert!(header.contains("Syscall"));
        assert!(header.contains("Arguments"));
        assert!(header.contains("Result"));
        assert!(header.contains("Duration"));
        assert!(header.contains("Source"));
    }
}
