//! Terminal capability — long-running child processes the agent can
//! observe and control.

use std::{collections::HashMap, io, path::PathBuf};

use async_trait::async_trait;
use uuid::Uuid;

/// Manages child processes the agent can spawn, inspect, and stop.
///
/// `terminal_kill` stops the process but leaves the handle valid for a
/// final snapshot; `terminal_release` discards the handle and frees its
/// resources.
#[async_trait]
pub trait Terminal: Send + Sync {
    async fn terminal_create(&self, _spec: TerminalSpec) -> io::Result<TerminalHandle>;

    async fn terminal_snapshot(&self, _handle: &TerminalHandle) -> io::Result<TerminalSnapshot>;

    async fn terminal_kill(&self, _handle: &TerminalHandle) -> io::Result<()>;

    async fn terminal_release(&self, _handle: TerminalHandle) -> io::Result<()>;
}

#[derive(Clone, Debug)]
pub struct TerminalSpec {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct TerminalHandle {
    pub id: Uuid,
}

#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    pub output: String,
    pub exit_code: Option<i32>,
}
