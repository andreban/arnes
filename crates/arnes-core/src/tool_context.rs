// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Per-invocation context passed to every tool's `execute`.

use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{AgentId, Host};

/// Everything a tool body needs from the harness, bundled so capability
/// additions don't change `Tool::execute`'s signature.
pub struct ToolContext {
    pub host: Host,
    pub progress: Arc<dyn ProgressSink>,
    pub cancellation: CancellationToken,
    pub agent_id: AgentId,
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
