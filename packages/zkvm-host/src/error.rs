//! Unified error type for the VERIA host.
//!
//! The host is invoked from three places — a CLI, an HTTP bridge, and the
//! integration tests — so we centralise the error variants here and let each
//! call site choose how to render them (text, JSON, anyhow chain).

use thiserror::Error;

/// Top-level error type for the host crate.
#[derive(Debug, Error)]
pub enum HostError {
    /// The caller passed a `circuit` string that does not map to a known
    /// circuit id.  See [`crate::circuits::CircuitId::from_str`].
    #[error("unknown circuit: {0}")]
    UnknownCircuit(String),

    /// Input deserialization failed.  Wraps `serde_json::Error` while keeping
    /// the original payload byte length for diagnostics.
    #[error("invalid input json ({bytes} bytes): {source}")]
    InvalidInput {
        bytes: usize,
        #[source]
        source: serde_json::Error,
    },

    /// Bounds violation. Each circuit declares a max length; submitting a
    /// larger payload is rejected at the host before reaching the guest.
    #[error("input exceeds bound: got {got}, max {max}")]
    OutOfBounds { got: usize, max: usize },

    /// The SP1 SDK returned an error (setup / execute / prove). We keep the
    /// upstream chain via `anyhow::Error` so callers can downcast if needed.
    #[error("sp1 sdk error: {0}")]
    Sp1(#[from] anyhow::Error),

    /// I/O failure when reading inputs or writing proofs.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Folding adapter failed (shape mismatch, accumulator drift).
    #[error("folding error: {0}")]
    Folding(String),

    /// The guest program produced an output that did not match the host-side
    /// expectation. Should never trigger in production; it is fired only by
    /// the determinism cross-check in `prover::ProveOptions::cross_check`.
    #[error("cross-check mismatch: host={host_hex}, guest={guest_hex}")]
    CrossCheck { host_hex: String, guest_hex: String },

    /// HTTP layer (axum) error.
    #[error("http error: {0}")]
    Http(String),
}

impl HostError {
    /// Map this error onto a stable string code suitable for the JSON
    /// `error.code` field on the HTTP response.
    pub fn code(&self) -> &'static str {
        match self {
            HostError::UnknownCircuit(_) => "unknown_circuit",
            HostError::InvalidInput { .. } => "invalid_input",
            HostError::OutOfBounds { .. } => "out_of_bounds",
            HostError::Sp1(_) => "sp1_error",
            HostError::Io(_) => "io_error",
            HostError::Folding(_) => "folding_error",
            HostError::CrossCheck { .. } => "cross_check_mismatch",
            HostError::Http(_) => "http_error",
        }
    }
}

/// Convenience `Result` alias.
pub type HostResult<T> = std::result::Result<T, HostError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        assert_eq!(HostError::UnknownCircuit("x".into()).code(), "unknown_circuit");
        assert_eq!(
            HostError::OutOfBounds { got: 9, max: 8 }.code(),
            "out_of_bounds"
        );
    }

    #[test]
    fn out_of_bounds_renders_both_sides() {
        let e = HostError::OutOfBounds { got: 5000, max: 4096 };
        let msg = format!("{e}");
        assert!(msg.contains("5000"));
        assert!(msg.contains("4096"));
    }
}
