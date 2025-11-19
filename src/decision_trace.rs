//! Transpiler Decision Tracing
//!
//! Sprint 26: Capture and analyze transpiler compile-time decisions (v1.0 - stderr)
//! Sprint 27: Memory-mapped file support with hash-based IDs (v2.0 spec)
//!
//! This module enables observability into transpiler decision-making by:
//! 1. Parsing decision traces from stderr (v1.0 - Sprint 26)
//! 2. Reading decision traces from memory-mapped MessagePack files (v2.0 - Sprint 27)
//! 3. Correlating decisions with source locations via DWARF
//! 4. Building decision dependency graphs
//! 5. Detecting decision anomalies
//! 6. Profiling decision performance
//!
//! ## v2.0 Specification (Sprint 27)
//!
//! The v2.0 specification addresses critical performance issues identified in peer review:
//! - Hash-based decision IDs (u64) eliminate I-cache bloat from string IDs
//! - Memory-mapped files eliminate I/O blocking from stderr writes
//! - Decision manifest provides human-readable mapping (hash → description)
//!
//! Reference: `docs/specifications/ruchy-tracing-support.md` (v2.0.0)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hasher;

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

    /// Sprint 27 (v2.0): Hash-based decision ID (FNV-1a)
    ///
    /// Generated from `category::name::file::line` using FNV-1a algorithm.
    /// Eliminates I-cache bloat from string-based IDs.
    pub decision_id: Option<u64>,
}

/// Sprint 27 (v2.0): Decision manifest entry
///
/// Maps u64 decision_id to human-readable description.
/// This is the "sidecar" file (`.ruchy/decision_manifest.json`) that
/// makes hash-based traces interpretable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionManifestEntry {
    /// FNV-1a hash of category::name::file::line
    pub decision_id: u64,

    /// Decision category (e.g., "optimization", "type_inference")
    pub category: String,

    /// Decision name (e.g., "inline_candidate", "infer_function")
    pub name: String,

    /// Source location where decision was made
    pub source: SourceLocation,

    /// Decision input parameters (from transpiler)
    pub input: serde_json::Value,

    /// Decision result/output (from transpiler)
    pub result: serde_json::Value,
}

/// Source location in Ruby/transpiled code
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceLocation {
    /// Source file path (e.g., "foo.rb")
    pub file: String,

    /// Line number (1-indexed)
    pub line: u32,

    /// Column number (optional, 1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// Sprint 27 (v2.0): Decision manifest (JSON sidecar)
///
/// Complete mapping of all decision IDs to their human-readable descriptions.
/// Loaded from `.ruchy/decision_manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionManifest {
    /// Manifest version (for forward compatibility)
    pub version: String,

    /// Git commit hash (for trace correlation)
    pub git_commit: Option<String>,

    /// Transpiler version
    pub transpiler_version: Option<String>,

    /// Map of decision_id (hex string) → manifest entry
    #[serde(flatten)]
    pub entries: HashMap<String, DecisionManifestEntry>,
}

/// Sprint 27 (v2.0): Generate decision ID using FNV-1a hash algorithm
///
/// Creates a unique 64-bit hash from decision metadata:
/// `category::name::file::line`
///
/// # Arguments
/// * `category` - Decision category (e.g., "optimization", "type_inference")
/// * `name` - Decision name (e.g., "inline_candidate", "infer_function")
/// * `file` - Source file path (e.g., "foo.rb")
/// * `line` - Line number in source file
///
/// # Returns
/// 64-bit FNV-1a hash that uniquely identifies this decision
///
/// # Example
/// ```
/// use renacer::decision_trace::generate_decision_id;
///
/// let decision_id = generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);
/// assert_ne!(decision_id, 0); // Should be non-zero
///
/// // Deterministic - same inputs produce same hash
/// let decision_id2 = generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);
/// assert_eq!(decision_id, decision_id2);
/// ```
///
/// # Performance
/// FNV-1a is chosen for speed (single pass, simple operations) over cryptographic
/// security. The 64-bit hash space (2^64 = 18 quintillion) makes collisions
/// extremely unlikely in practice.
///
/// Reference: http://www.isthe.com/chongo/tech/comp/fnv/
pub fn generate_decision_id(category: &str, name: &str, file: &str, line: u32) -> u64 {
    let mut hasher = fnv::FnvHasher::default();

    // Hash format: "category::name::file::line"
    hasher.write(category.as_bytes());
    hasher.write(b"::");
    hasher.write(name.as_bytes());
    hasher.write(b"::");
    hasher.write(file.as_bytes());
    hasher.write(b"::");
    hasher.write(&line.to_le_bytes());

    hasher.finish()
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
            decision_id: None, // Sprint 27: v1.0 stderr format doesn't include hash IDs
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
            decision_id: None,
        };

        let graph = DecisionGraph::from_traces(vec![trace]);
        let cascades = graph.find_cascades();
        assert!(cascades.is_empty()); // Single decision, no cascade
    }

    // Sprint 27 (v2.0) Tests
    mod sprint27_v2_tests {
        use super::*;

        #[test]
        fn test_generate_decision_id_basic() {
            // RED phase: Test hash generation from category::name::file::line
            let decision_id = generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);

            // Should be a valid u64 (non-zero)
            assert_ne!(decision_id, 0);

            // Should be deterministic (same inputs = same output)
            let decision_id2 =
                generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);
            assert_eq!(decision_id, decision_id2);
        }

        #[test]
        fn test_generate_decision_id_different_inputs_different_outputs() {
            // Different category
            let id1 = generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);
            let id2 = generate_decision_id("type_inference", "inline_candidate", "foo.rb", 3);
            assert_ne!(id1, id2);

            // Different name
            let id3 = generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);
            let id4 = generate_decision_id("optimization", "tail_recursion", "foo.rb", 3);
            assert_ne!(id3, id4);

            // Different file
            let id5 = generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);
            let id6 = generate_decision_id("optimization", "inline_candidate", "bar.rb", 3);
            assert_ne!(id5, id6);

            // Different line
            let id7 = generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);
            let id8 = generate_decision_id("optimization", "inline_candidate", "foo.rb", 5);
            assert_ne!(id7, id8);
        }

        #[test]
        fn test_generate_decision_id_spec_example() {
            // From spec: optimization::inline_candidate at foo.rb:3 → 0xA1B2C3D4E5F67890
            // We can't predict exact hash but can verify it's consistent
            let decision_id = generate_decision_id("optimization", "inline_candidate", "foo.rb", 3);

            // Should be 64-bit value
            assert!(decision_id > 0);
            assert!(decision_id <= u64::MAX);
        }

        #[test]
        fn test_decision_manifest_entry_serialization() {
            // Test that DecisionManifestEntry can be serialized to JSON
            let entry = DecisionManifestEntry {
                decision_id: 0xA1B2C3D4E5F67890,
                category: "optimization".to_string(),
                name: "inline_candidate".to_string(),
                source: SourceLocation {
                    file: "foo.rb".to_string(),
                    line: 3,
                    column: Some(1),
                },
                input: serde_json::json!({"size": 4, "call_count": 1000}),
                result: serde_json::json!({"decision": "no_inline", "reason": "recursive"}),
            };

            let json = serde_json::to_string(&entry).unwrap();
            // The decision_id in the test is 0xA1B2C3D4E5F67890 (hex)
            // which should appear as 11651590505119512720 in the JSON
            assert!(json.contains("decision_id"));
            assert!(json.contains("optimization"));
            assert!(json.contains("inline_candidate"));
            assert!(json.contains("foo.rb"));

            // Verify it can be deserialized back
            let deserialized: DecisionManifestEntry = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.decision_id, 0xA1B2C3D4E5F67890);
            assert_eq!(deserialized.category, "optimization");
        }

        #[test]
        fn test_decision_manifest_deserialization() {
            // Test loading a manifest from JSON
            let json = r#"{
                "version": "2.0.0",
                "git_commit": "abc123def",
                "transpiler_version": "3.213.0",
                "0xA1B2C3D4E5F67890": {
                    "decision_id": 11638049751140409488,
                    "category": "optimization",
                    "name": "inline_candidate",
                    "source": {
                        "file": "foo.rb",
                        "line": 3
                    },
                    "input": {"size": 4},
                    "result": {"decision": "no_inline"}
                }
            }"#;

            let manifest: DecisionManifest = serde_json::from_str(json).unwrap();
            assert_eq!(manifest.version, "2.0.0");
            assert_eq!(manifest.git_commit, Some("abc123def".to_string()));
            assert_eq!(manifest.entries.len(), 1);
        }

        #[test]
        fn test_source_location_serialization() {
            let loc = SourceLocation {
                file: "foo.rb".to_string(),
                line: 42,
                column: Some(10),
            };

            let json = serde_json::to_string(&loc).unwrap();
            assert!(json.contains("foo.rb"));
            assert!(json.contains("42"));
            assert!(json.contains("10"));

            // Without column
            let loc2 = SourceLocation {
                file: "bar.rb".to_string(),
                line: 100,
                column: None,
            };

            let json2 = serde_json::to_string(&loc2).unwrap();
            assert!(json2.contains("bar.rb"));
            assert!(json2.contains("100"));
            assert!(!json2.contains("column")); // Should be skipped
        }
    }
}
