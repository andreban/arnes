// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Error type for the crate.

use std::fmt;

/// The crate's unified error type.
#[derive(Debug)]
#[non_exhaustive]
pub enum CoreError {
    /// Runtime failure inside the session loop.
    Session(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Session(msg) => write!(f, "session error: {msg}"),
        }
    }
}

impl std::error::Error for CoreError {}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, CoreError>;
