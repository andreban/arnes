// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Per-invocation context passed to every tool's `execute`.

use std::{path::PathBuf, sync::Arc};

use crate::{AgentId, Host};
use serde_json::Value;

/// Everything a tool body needs from the harness, bundled so capability
/// additions don't change `Tool::execute`'s signature.
pub struct ToolContext {
    pub host: Host,
    pub progress: Option<Arc<dyn ProgressSink>>,
    pub agent_id: AgentId,
    /// Working directory for resolving relative paths in tool calls.
    pub cwd: PathBuf,
}

/// One progress payload from a running tool.
#[derive(Clone, Debug)]
pub enum ProgressUpdate {
    Text(String),
    /// Structured payload, tool-defined. Reserved for future use.
    Structured(Value),
}

/// Sink for `ProgressUpdate`s.
pub trait ProgressSink: Send + Sync {
    fn send(&self, update: ProgressUpdate);
}
