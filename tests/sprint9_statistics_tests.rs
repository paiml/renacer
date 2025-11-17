//! Sprint 9-10: Statistics mode tests
#![allow(deprecated)] // suppress assert_cmd::Command::cargo_bin deprecation in tests
//!
//! Test -c flag for syscall statistics summary

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_statistics_mode_shows_summary() {
    // Test that -c shows statistics table
    let mut cmd = Command::cargo_bin("renacer").unwrap();
    cmd.arg("-c")
        .arg("--")
        .arg("echo")
        .arg("test")
        .assert()
        .success()
        .stdout(predicate::str::contains("% time"))
        .stdout(predicate::str::contains("syscall"))
        .stdout(predicate::str::contains("total"));
}

#[test]
fn test_statistics_mode_suppresses_individual_calls() {
    // Test that -c does not show individual syscall lines
    let mut cmd = Command::cargo_bin("renacer").unwrap();
    cmd.arg("-c")
        .arg("--")
        .arg("echo")
        .arg("test")
        .assert()
        .success()
        .stdout(predicate::str::contains("write(").not()); // Should NOT show individual calls
}

#[test]
fn test_statistics_with_filter() {
    // Test that -c works with -e trace= filtering
    let mut cmd = Command::cargo_bin("renacer").unwrap();
    cmd.arg("-c")
        .arg("-e")
        .arg("trace=write,brk")
        .arg("--")
        .arg("echo")
        .arg("test")
        .assert()
        .success()
        .stdout(predicate::str::contains("write"))
        .stdout(predicate::str::contains("brk"));
}

#[test]
fn test_statistics_shows_call_counts() {
    // Test that statistics show call counts
    let mut cmd = Command::cargo_bin("renacer").unwrap();
    cmd.arg("-c")
        .arg("--")
        .arg("echo")
        .arg("test")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"\d+\s+write").unwrap()); // Number before "write"
}
