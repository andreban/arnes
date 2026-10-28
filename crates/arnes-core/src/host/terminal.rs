// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Terminal capability — long-running child processes the agent can
//! observe and control.

use std::{collections::HashMap, io, path::PathBuf};

use async_trait::async_trait;
use uuid::Uuid;

/// Manages child processes the agent can spawn, inspect, and stop.
///
/// The methods map one-to-one onto ACP's `terminal/*` client methods, so
/// an ACP host can forward each call verbatim and a local host can mirror
/// the same semantics.
///
/// `terminal_kill` stops the process but leaves the handle valid for a
/// final snapshot; `terminal_release` discards the handle and frees its
/// resources.
#[async_trait]
pub trait Terminal: Send + Sync {
    /// Spawns `spec` and returns a handle to it. Returns as soon as the
    /// process has started; it does not wait for it to exit.
    async fn terminal_create(&self, spec: TerminalSpec) -> io::Result<TerminalHandle>;

    /// Returns the output captured so far and, if the process has
    /// exited, its exit status.
    async fn terminal_snapshot(&self, handle: &TerminalHandle) -> io::Result<TerminalSnapshot>;

    /// Waits until the process exits and returns its exit status.
    ///
    /// Callers that need a deadline wrap this in their own timeout and
    /// call [`terminal_kill`](Self::terminal_kill) when it elapses.
    async fn terminal_wait_for_exit(
        &self,
        handle: &TerminalHandle,
    ) -> io::Result<TerminalExitStatus>;

    /// Stops the process if it is still running. The handle stays valid.
    async fn terminal_kill(&self, handle: &TerminalHandle) -> io::Result<()>;

    /// Kills the process if it is still running and invalidates the handle.
    async fn terminal_release(&self, handle: TerminalHandle) -> io::Result<()>;
}

/// What to spawn in a new terminal.
#[derive(Clone, Debug, Default)]
pub struct TerminalSpec {
    /// The program to run.
    pub command: String,
    /// Arguments passed to `command`.
    pub args: Vec<String>,
    /// Working directory; the host's current directory when `None`.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables, added to the inherited environment.
    pub env: HashMap<String, String>,
    /// Maximum number of output bytes to retain. When exceeded, the
    /// oldest output is discarded so the most recent output is kept.
    /// `None` means the host's default.
    pub output_byte_limit: Option<usize>,
}

/// Identifies a terminal created by [`Terminal::terminal_create`].
#[derive(Clone, Debug)]
pub struct TerminalHandle {
    /// Host-assigned identifier.
    pub id: Uuid,
}

/// Output captured from a terminal at one point in time.
#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    /// Combined stdout and stderr, in the order the host received it.
    pub output: String,
    /// Whether older output was discarded to honour
    /// [`TerminalSpec::output_byte_limit`].
    pub truncated: bool,
    /// The exit status, or `None` while the process is still running.
    pub exit_status: Option<TerminalExitStatus>,
}

/// How a terminal process ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalExitStatus {
    /// The process exit code, or `None` if it was terminated by a signal.
    pub exit_code: Option<i32>,
    /// The signal that terminated the process (e.g. `"SIGKILL"`), if any.
    pub signal: Option<String>,
}
