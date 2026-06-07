// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! OS-resource abstraction.

use std::sync::Arc;

mod filesystem;
mod terminal;

pub use filesystem::{ReadTextFile, WriteTextFile};
pub use terminal::{Terminal, TerminalHandle, TerminalSnapshot, TerminalSpec};

/// Optional OS-resource capabilities the embedder grants to a session.
///
/// Each field is `Some` when the embedder provides that capability and
/// `None` otherwise. Defaults to all-`None`; tools must handle the
/// missing case rather than assume any field is set.
#[derive(Default, Clone)]
pub struct Host {
    pub read_text_file: Option<Arc<dyn ReadTextFile>>,
    pub write_text_file: Option<Arc<dyn WriteTextFile>>,
    pub terminal: Option<Arc<dyn Terminal>>,
}
