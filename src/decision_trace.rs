//! Transpiler Decision Tracing
//!
//! Sprint 26: Capture and analyze transpiler compile-time decisions
//!
//! This module enables observability into transpiler decision-making by:
//! 1. Parsing decision traces from stderr (emitted by transpilers)
//! 2. Correlating decisions with source locations via DWARF
//! 3. Building decision dependency graphs
//! 4. Detecting decision anomalies
//! 5. Profiling decision performance

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single transpiler decision trace point
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionTrace {
    /// Timestamp (relative to trace start)
    pub timestamp_us: u64,

    /// Decision category (e.g., "exception_flow_analysis", "type_inference")
    pub category: String,

    /// Decision name (e.g., "try_body", "infer_return_type")
    pub name: String,

    /// Decision input (structured data)
    pub input: serde_json::Value,

    /// Decision result/output (if available)
    pub result: Option<serde_json::Value>,

    /// Source location where decision was made (file:line function)
    pub source_location: Option<String>,
}

/// Decision trace collector
#[derive(Debug)]
pub struct DecisionTracer {
    traces: Vec<DecisionTrace>,
    start_time: std::time::Instant,
}

impl DecisionTracer {
    /// Create a new decision tracer
    pub fn new() -> Self {
        Self {
            traces: Vec::new(),
            start_time: std::time::Instant::now(),
        }
    }

    /// Parse a decision trace line from stderr
    ///
    /// Expected format: `[DECISION] category::name input={"key":"value"}`
    /// Or: `[RESULT] name = {"result":"value"}`
    pub fn parse_line(&mut self, line: &str) -> Result<(), String> {
        let timestamp_us = self.start_time.elapsed().as_micros() as u64;

        if line.starts_with("[DECISION]") {
            self.parse_decision_line(line, timestamp_us)
        } else if line.starts_with("[RESULT]") {
            self.parse_result_line(line, timestamp_us)
        } else {
            // Not a decision trace line, ignore
            Ok(())
        }
    }

    /// Parse decision line: `[DECISION] category::name input={"key":"value"}`
    fn parse_decision_line(&mut self, line: &str, timestamp_us: u64) -> Result<(), String> {
        // Strip [DECISION] prefix
        let content = line
            .strip_prefix("[DECISION]")
            .ok_or("Missing [DECISION] prefix")?
            .trim();

        // Split into "category::name" and "input=..."
        let parts: Vec<&str> = content.splitn(2, " input=").collect();
        if parts.len() != 2 {
            return Err(format!("Invalid DECISION format: {}", line));
        }

        // Parse category::name
        let category_name = parts[0];
        let cat_name_parts: Vec<&str> = category_name.split("::").collect();
        if cat_name_parts.len() != 2 {
            return Err(format!("Invalid category::name format: {}", category_name));
        }

        let category = cat_name_parts[0].to_string();
        let name = cat_name_parts[1].to_string();

        // Parse JSON input
        let input: serde_json::Value =
            serde_json::from_str(parts[1]).map_err(|e| format!("Invalid JSON input: {}", e))?;

        self.traces.push(DecisionTrace {
            timestamp_us,
            category,
            name,
            input,
            result: None,
            source_location: None,
        });

        Ok(())
    }

    /// Parse result line: `[RESULT] name = {"result":"value"}`
    fn parse_result_line(&mut self, line: &str, _timestamp_us: u64) -> Result<(), String> {
        // Strip [RESULT] prefix
        let content = line
            .strip_prefix("[RESULT]")
            .ok_or("Missing [RESULT] prefix")?
            .trim();

        // Split into "name" and "= {...}"
        let parts: Vec<&str> = content.splitn(2, " = ").collect();
        if parts.len() != 2 {
            return Err(format!("Invalid RESULT format: {}", line));
        }

        let name = parts[0].trim();

        // Parse JSON result
        let result: serde_json::Value =
            serde_json::from_str(parts[1]).map_err(|e| format!("Invalid JSON result: {}", e))?;

        // Find the most recent decision with this name and attach result
        for trace in self.traces.iter_mut().rev() {
            if trace.name == name && trace.result.is_none() {
                trace.result = Some(result);
                return Ok(());
            }
        }

        Err(format!("No matching DECISION found for RESULT: {}", name))
    }

    /// Get all collected traces
    pub fn traces(&self) -> &[DecisionTrace] {
        &self.traces
    }

    /// Get trace count
    pub fn count(&self) -> usize {
        self.traces.len()
    }
}

impl Default for DecisionTracer {
    fn default() -> Self {
        Self::new()
    }
}

/// Decision dependency graph
#[derive(Debug)]
pub struct DecisionGraph {
    /// Adjacency list: decision_id -> Vec<dependent_decision_id>
    dependencies: HashMap<usize, Vec<usize>>,
    traces: Vec<DecisionTrace>,
}

impl DecisionGraph {
    /// Build dependency graph from traces
    pub fn from_traces(traces: Vec<DecisionTrace>) -> Self {
        let mut graph = Self {
            dependencies: HashMap::new(),
            traces,
        };

        graph.analyze_dependencies();
        graph
    }

    /// Analyze decision dependencies based on data flow
    fn analyze_dependencies(&mut self) {
        // Simple heuristic: if decision B's input contains values from decision A's result,
        // then B depends on A
        for i in 0..self.traces.len() {
            for j in (i + 1)..self.traces.len() {
                if self.has_dependency(i, j) {
                    self.dependencies.entry(i).or_default().push(j);
                }
            }
        }
    }

    /// Check if decision j depends on decision i
    fn has_dependency(&self, i: usize, j: usize) -> bool {
        // Check if any result value from i appears in input of j
        if let Some(ref result_i) = self.traces[i].result {
            let input_j = &self.traces[j].input;

            // Simple check: convert to strings and look for substring
            let result_str = serde_json::to_string(result_i).unwrap_or_default();
            let input_str = serde_json::to_string(input_j).unwrap_or_default();

            // Look for matching values (simplified heuristic)
            if input_str.contains(&result_str[1..result_str.len() - 1]) {
                return true;
            }
        }

        false
    }

    /// Find decision cascades (chains of dependent decisions)
    pub fn find_cascades(&self) -> Vec<Vec<usize>> {
        let mut cascades = Vec::new();

        for start_idx in 0..self.traces.len() {
            let mut cascade = vec![start_idx];
            let mut current = start_idx;

            while let Some(deps) = self.dependencies.get(&current) {
                if let Some(&next) = deps.first() {
                    cascade.push(next);
                    current = next;
                } else {
                    break;
                }
            }

            if cascade.len() > 1 {
                cascades.push(cascade);
            }
        }

        cascades
    }

    /// Get dependencies for a decision
    pub fn dependencies(&self, decision_idx: usize) -> Option<&Vec<usize>> {
        self.dependencies.get(&decision_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_tracer_new() {
        let tracer = DecisionTracer::new();
        assert_eq!(tracer.count(), 0);
    }

    #[test]
    fn test_parse_decision_line_basic() {
        let mut tracer = DecisionTracer::new();
        let result =
            tracer.parse_line(r#"[DECISION] exception_flow::try_body input={"handlers":2}"#);
        assert!(result.is_ok(), "Parse should succeed: {:?}", result);
        assert_eq!(tracer.count(), 1);

        let trace = &tracer.traces()[0];
        assert_eq!(trace.category, "exception_flow");
        assert_eq!(trace.name, "try_body");
        assert_eq!(trace.input["handlers"], 2);
    }

    #[test]
    fn test_parse_result_line() {
        let mut tracer = DecisionTracer::new();
        tracer
            .parse_line(r#"[DECISION] type_inference::infer_return input={"func":"foo"}"#)
            .unwrap();
        tracer
            .parse_line(r#"[RESULT] infer_return = {"type":"i32"}"#)
            .unwrap();

        assert_eq!(tracer.count(), 1);
        let trace = &tracer.traces()[0];
        assert!(trace.result.is_some());
        assert_eq!(trace.result.as_ref().unwrap()["type"], "i32");
    }

    #[test]
    fn test_parse_invalid_decision_format() {
        let mut tracer = DecisionTracer::new();
        let result = tracer.parse_line("[DECISION] invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_json() {
        let mut tracer = DecisionTracer::new();
        let result = tracer.parse_line(r#"[DECISION] cat::name input={invalid}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_ignore_non_decision_lines() {
        let mut tracer = DecisionTracer::new();
        let result = tracer.parse_line("Normal stderr output");
        assert!(result.is_ok());
        assert_eq!(tracer.count(), 0);
    }

    #[test]
    fn test_decision_graph_empty() {
        let graph = DecisionGraph::from_traces(vec![]);
        let cascades = graph.find_cascades();
        assert!(cascades.is_empty());
    }

    #[test]
    fn test_decision_graph_single_decision() {
        let trace = DecisionTrace {
            timestamp_us: 100,
            category: "test".to_string(),
            name: "decision1".to_string(),
            input: serde_json::json!({"x": 1}),
            result: Some(serde_json::json!({"y": 2})),
            source_location: None,
        };

        let graph = DecisionGraph::from_traces(vec![trace]);
        let cascades = graph.find_cascades();
        assert!(cascades.is_empty()); // Single decision, no cascade
    }
}
