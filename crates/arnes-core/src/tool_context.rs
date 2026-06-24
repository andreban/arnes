// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Per-invocation context passed to every tool's `execute`.

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::Host;

/// Everything a tool body needs from the harness, bundled so capability
/// additions don't change `Tool::execute`'s signature.
#[derive(Clone)]
pub struct ToolContext {
    pub host: Host,
    /// Working directory for resolving relative paths in tool calls.
    pub cwd: PathBuf,
    /// Paths the agent has read with approval, and may therefore edit.
    pub read_grants: Arc<Mutex<HashSet<PathBuf>>>,
}
