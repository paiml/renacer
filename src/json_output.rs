//! JSON output format for syscall traces
//!
//! Sprint 9-10: --format json implementation

use serde::{Deserialize, Serialize};

/// Source location information for a syscall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSourceLocation {
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

/// A single syscall event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSyscall {
    /// Syscall name (e.g., "openat", "read")
    pub name: String,
    /// Arguments as formatted strings
    pub args: Vec<String>,
    /// Return value (may be negative for errors)
    pub result: i64,
    /// Duration in microseconds (0 if timing not enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_us: Option<u64>,
    /// Source location (if --source enabled and available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<JsonSourceLocation>,
}

/// ML Anomaly Analysis result (Sprint 23)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMlAnalysis {
    /// Number of clusters used
    pub clusters: usize,
    /// Silhouette score for clustering quality (-1 to 1)
    pub silhouette_score: f64,
    /// List of detected anomalies
    pub anomalies: Vec<JsonMlAnomaly>,
}

/// A detected ML anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMlAnomaly {
    /// Syscall name
    pub syscall: String,
    /// Average time in microseconds
    pub avg_time_us: f64,
    /// Cluster assignment
    pub cluster: usize,
}

/// Isolation Forest Analysis result (Sprint 22)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonIsolationForestAnalysis {
    /// Number of trees in forest
    pub num_trees: usize,
    /// Contamination threshold used
    pub contamination: f32,
    /// Total samples analyzed
    pub total_samples: usize,
    /// List of detected outliers
    pub outliers: Vec<JsonIsolationForestOutlier>,
}

/// A detected outlier from Isolation Forest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonIsolationForestOutlier {
    /// Syscall name
    pub syscall: String,
    /// Anomaly score (0.0 to 1.0, higher is more anomalous)
    pub anomaly_score: f64,
    /// Average duration in microseconds
    pub avg_duration_us: f64,
    /// Call count
    pub call_count: u64,
    /// Feature importance (if explainability enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_importance: Option<Vec<JsonFeatureImportance>>,
}

/// Feature importance for explainability (XAI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFeatureImportance {
    /// Feature name
    pub feature: String,
    /// Importance percentage (0-100)
    pub importance: f64,
}

/// Autoencoder anomaly detection analysis (Sprint 23)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonAutoencoderAnalysis {
    /// Hidden layer size
    pub hidden_size: usize,
    /// Training epochs
    pub epochs: usize,
    /// Threshold multiplier (σ)
    pub threshold: f32,
    /// Adaptive reconstruction error threshold
    pub adaptive_threshold: f64,
    /// Total samples analyzed
    pub total_samples: usize,
    /// List of detected anomalies
    pub anomalies: Vec<JsonAutoencoderAnomaly>,
}

/// A detected anomaly from Autoencoder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonAutoencoderAnomaly {
    /// Syscall name
    pub syscall: String,
    /// Reconstruction error
    pub reconstruction_error: f64,
    /// Average duration in microseconds
    pub avg_duration_us: f64,
    /// Call count
    pub call_count: u64,
    /// Feature contributions to error (if explainability enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_contributions: Option<Vec<JsonFeatureImportance>>,
}

/// Summary statistics for the trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSummary {
    /// Total number of syscalls traced
    pub total_syscalls: u64,
    /// Total time in microseconds (if timing enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_time_us: Option<u64>,
    /// Exit code of traced process
    pub exit_code: i32,
}

/// Root JSON output structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonOutput {
    /// Format version identifier
    pub version: String,
    /// Format name
    pub format: String,
    /// List of syscall events
    pub syscalls: Vec<JsonSyscall>,
    /// Summary statistics
    pub summary: JsonSummary,
    /// ML anomaly analysis results (if --ml-anomaly enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ml_analysis: Option<JsonMlAnalysis>,
    /// Isolation Forest outlier analysis (if --ml-outliers enabled) (Sprint 22)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolation_forest_analysis: Option<JsonIsolationForestAnalysis>,
    /// Autoencoder anomaly detection (if --dl-anomaly enabled) (Sprint 23)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoencoder_analysis: Option<JsonAutoencoderAnalysis>,
}

impl JsonOutput {
    /// Create a new JSON output structure
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            format: "renacer-json-v1".to_string(),
            syscalls: Vec::new(),
            summary: JsonSummary {
                total_syscalls: 0,
                total_time_us: None,
                exit_code: 0,
            },
            ml_analysis: None,
            isolation_forest_analysis: None,
            autoencoder_analysis: None,
        }
    }

    /// Add a syscall to the output
    pub fn add_syscall(&mut self, syscall: JsonSyscall) {
        self.summary.total_syscalls += 1;
        if let Some(duration) = syscall.duration_us {
            *self.summary.total_time_us.get_or_insert(0) += duration;
        }
        self.syscalls.push(syscall);
    }

    /// Set the exit code
    pub fn set_exit_code(&mut self, code: i32) {
        self.summary.exit_code = code;
    }

    /// Set ML analysis results (Sprint 23)
    pub fn set_ml_analysis(&mut self, report: crate::ml_anomaly::MlAnomalyReport) {
        let anomalies = report
            .anomalies
            .iter()
            .map(|a| JsonMlAnomaly {
                syscall: a.syscall.clone(),
                avg_time_us: a.avg_time_us,
                cluster: a.cluster,
            })
            .collect();

        self.ml_analysis = Some(JsonMlAnalysis {
            clusters: report.num_clusters,
            silhouette_score: report.silhouette_score,
            anomalies,
        });
    }

    /// Set Isolation Forest analysis results (Sprint 22)
    pub fn set_isolation_forest_analysis(
        &mut self,
        report: crate::isolation_forest::OutlierReport,
        explain: bool,
    ) {
        let outliers = report
            .outliers
            .iter()
            .map(|o| {
                let feature_importance = if explain && !o.feature_importance.is_empty() {
                    Some(
                        o.feature_importance
                            .iter()
                            .map(|(feature, importance)| JsonFeatureImportance {
                                feature: feature.clone(),
                                importance: *importance,
                            })
                            .collect(),
                    )
                } else {
                    None
                };

                JsonIsolationForestOutlier {
                    syscall: o.syscall.clone(),
                    anomaly_score: o.anomaly_score,
                    avg_duration_us: o.avg_duration_us,
                    call_count: o.call_count,
                    feature_importance,
                }
            })
            .collect();

        self.isolation_forest_analysis = Some(JsonIsolationForestAnalysis {
            num_trees: report.num_trees,
            contamination: report.contamination,
            total_samples: report.total_samples,
            outliers,
        });
    }

    /// Set Autoencoder analysis results (Sprint 23)
    pub fn set_autoencoder_analysis(
        &mut self,
        report: crate::autoencoder::AutoencoderReport,
        threshold: f32,
        explain: bool,
    ) {
        let anomalies = report
            .anomalies
            .iter()
            .map(|a| {
                let feature_contributions = if explain && !a.feature_contributions.is_empty() {
                    Some(
                        a.feature_contributions
                            .iter()
                            .map(|(feature, contribution)| JsonFeatureImportance {
                                feature: feature.clone(),
                                importance: *contribution,
                            })
                            .collect(),
                    )
                } else {
                    None
                };

                JsonAutoencoderAnomaly {
                    syscall: a.syscall.clone(),
                    reconstruction_error: a.reconstruction_error,
                    avg_duration_us: a.avg_duration_us,
                    call_count: a.call_count,
                    feature_contributions,
                }
            })
            .collect();

        self.autoencoder_analysis = Some(JsonAutoencoderAnalysis {
            hidden_size: report.hidden_size,
            epochs: report.epochs,
            threshold,
            adaptive_threshold: report.threshold,
            total_samples: report.total_samples,
            anomalies,
        });
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

impl Default for JsonOutput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_output_creation() {
        let output = JsonOutput::new();
        assert_eq!(output.format, "renacer-json-v1");
        assert_eq!(output.syscalls.len(), 0);
        assert_eq!(output.summary.total_syscalls, 0);
    }

    #[test]
    fn test_add_syscall() {
        let mut output = JsonOutput::new();
        let syscall = JsonSyscall {
            name: "write".to_string(),
            args: vec!["1".to_string(), "\"hello\"".to_string(), "5".to_string()],
            result: 5,
            duration_us: Some(100),
            source: None,
        };

        output.add_syscall(syscall);
        assert_eq!(output.summary.total_syscalls, 1);
        assert_eq!(output.summary.total_time_us, Some(100));
    }

    #[test]
    fn test_json_serialization() {
        let mut output = JsonOutput::new();
        output.add_syscall(JsonSyscall {
            name: "openat".to_string(),
            args: vec![
                "0xffffff9c".to_string(),
                "\"/tmp/test\"".to_string(),
                "0x2".to_string(),
            ],
            result: 3,
            duration_us: None,
            source: Some(JsonSourceLocation {
                file: "main.rs".to_string(),
                line: 42,
                function: Some("main".to_string()),
            }),
        });
        output.set_exit_code(0);

        let json = output.to_json().unwrap();
        assert!(json.contains("\"name\": \"openat\""));
        assert!(json.contains("\"format\": \"renacer-json-v1\""));
        assert!(json.contains("\"file\": \"main.rs\""));
        assert!(json.contains("\"line\": 42"));
    }

    #[test]
    fn test_optional_fields_omitted() {
        let syscall = JsonSyscall {
            name: "read".to_string(),
            args: vec!["3".to_string()],
            result: 10,
            duration_us: None,
            source: None,
        };

        let json = serde_json::to_string(&syscall).unwrap();
        // Optional None fields should be omitted
        assert!(!json.contains("duration_us"));
        assert!(!json.contains("source"));
    }
}
