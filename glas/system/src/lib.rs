//! Real OS primitives for Glas (`ADR-0010`).
//!
//! `ProcessSystem` implements the core's [`System`] seam by running OS helper
//! programs (`timedclient-qt5`, `pactl`, …) via `std::process`. It is the only
//! place that touches the machine; the core stays pure and host-testable.

use std::process::Command;

use glas_core::system::System;

/// Runs OS helper programs and returns their stdout.
pub struct ProcessSystem;

impl System for ProcessSystem {
    fn run(&self, program: &str, args: &[&str]) -> Result<String, String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("failed to run {program}: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "{program} exited with {}: {}",
                output.status,
                stderr.trim()
            ));
        }
        Ok(stdout.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_a_program_and_captures_stdout() {
        let out = ProcessSystem.run("echo", &["hello"]).unwrap();
        assert_eq!(out.trim(), "hello");
    }

    #[test]
    fn missing_program_is_an_error() {
        let err = ProcessSystem
            .run("definitely-not-a-real-program-xyz", &[])
            .unwrap_err();
        assert!(err.contains("failed to run"), "got: {err}");
    }
}
