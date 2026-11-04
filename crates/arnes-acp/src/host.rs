// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    io,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use arnes_core::{
    ReadTextFile, Terminal, TerminalExitStatus, TerminalHandle, TerminalSnapshot, TerminalSpec,
    WriteTextFile,
};
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::types::{
    filesystem::{FsReadTextFileParams, FsWriteTextFileParams},
    jsonrpc::OutboundRequest,
    terminal::{
        EnvVariable, TerminalCreateParams, TerminalCreateResult,
        TerminalExitStatus as WireExitStatus, TerminalOutputResult, TerminalRefParams,
    },
};

pub type PendingRequests = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

/// How long a client request may take before it is abandoned, for requests
/// that are expected to answer promptly.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Removes a pending request entry when dropped so cancelled or early-returning
/// outbound calls do not leak their response sender in [`PendingRequests`].
pub(crate) struct PendingRequestGuard {
    pending: PendingRequests,
    request_id: String,
}

impl PendingRequestGuard {
    pub(crate) fn new(pending: PendingRequests, request_id: String) -> Self {
        Self {
            pending,
            request_id,
        }
    }
}

impl Drop for PendingRequestGuard {
    fn drop(&mut self) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.request_id);
    }
}

/// Sends agent-to-client JSON-RPC requests for one session and awaits
/// their responses.
#[derive(Clone)]
struct ClientRequester {
    session_id: String,
    notify_tx: mpsc::UnboundedSender<String>,
    pending: PendingRequests,
}

impl ClientRequester {
    fn new(
        session_id: String,
        notify_tx: mpsc::UnboundedSender<String>,
        pending: PendingRequests,
    ) -> Self {
        Self {
            session_id,
            notify_tx,
            pending,
        }
    }

    /// Sends `method` with `params` and returns the client's `result`.
    ///
    /// A JSON-RPC error from the client becomes an [`io::Error`]. With a
    /// `timeout` the request is abandoned once it elapses; without one it
    /// waits until the client answers or disconnects.
    async fn request(
        &self,
        method: &'static str,
        params: impl Serialize,
        timeout: Option<Duration>,
    ) -> io::Result<Value> {
        let request_id = Uuid::now_v7().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(request_id.clone(), tx);
        let _guard = PendingRequestGuard::new(Arc::clone(&self.pending), request_id.clone());

        let req = OutboundRequest::new(request_id, method, params);
        let serialized = serde_json::to_string(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if self.notify_tx.send(serialized).is_err() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "client disconnected",
            ));
        }

        let response = match timeout {
            Some(timeout) => tokio::time::timeout(timeout, rx).await.map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, format!("{method} timed out"))
            })?,
            None => rx.await,
        };
        match response {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(msg)) => Err(io::Error::other(msg)),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "response channel closed",
            )),
        }
    }

    /// Like [`request`](Self::request), then deserializes the result.
    async fn request_as<T: DeserializeOwned>(
        &self,
        method: &'static str,
        params: impl Serialize,
        timeout: Option<Duration>,
    ) -> io::Result<T> {
        let result = self.request(method, params, timeout).await?;
        serde_json::from_value(result).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid {method} result: {e}"),
            )
        })
    }
}

/// Fulfils `WriteTextFile` by delegating to the ACP client via `fs/write_text_file`.
pub struct AcpWriteTextFile {
    client: ClientRequester,
}

impl AcpWriteTextFile {
    pub fn new(
        session_id: String,
        notify_tx: mpsc::UnboundedSender<String>,
        pending: PendingRequests,
    ) -> Self {
        Self {
            client: ClientRequester::new(session_id, notify_tx, pending),
        }
    }
}

#[async_trait]
impl WriteTextFile for AcpWriteTextFile {
    async fn write_text_file(&self, path: &Path, content: &str) -> io::Result<()> {
        let params = FsWriteTextFileParams {
            session_id: self.client.session_id.clone(),
            path: path.to_string_lossy().into_owned(),
            content: content.to_owned(),
        };
        let result = self
            .client
            .request("fs/write_text_file", params, Some(REQUEST_TIMEOUT))
            .await?;
        match result["error"].as_str() {
            Some(err_msg) => Err(io::Error::other(err_msg.to_owned())),
            None => Ok(()),
        }
    }
}

/// Fulfils `ReadTextFile` by delegating to the ACP client via `fs/read_text_file`.
pub struct AcpReadTextFile {
    client: ClientRequester,
}

impl AcpReadTextFile {
    pub fn new(
        session_id: String,
        notify_tx: mpsc::UnboundedSender<String>,
        pending: PendingRequests,
    ) -> Self {
        Self {
            client: ClientRequester::new(session_id, notify_tx, pending),
        }
    }
}

#[async_trait]
impl ReadTextFile for AcpReadTextFile {
    async fn read_text_file(
        &self,
        path: &Path,
        line: Option<usize>,
        limit: Option<usize>,
    ) -> io::Result<String> {
        let params = FsReadTextFileParams {
            session_id: self.client.session_id.clone(),
            path: path.to_string_lossy().into_owned(),
            line: line.map(|n| n as u64),
            limit: limit.map(|n| n as u64),
        };
        let result = self
            .client
            .request("fs/read_text_file", params, Some(REQUEST_TIMEOUT))
            .await?;
        result["content"]
            .as_str()
            .map(|s| s.to_owned())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing content"))
    }
}

/// Fulfils `Terminal` by delegating to the ACP client via its `terminal/*`
/// methods.
///
/// The client picks each terminal's id; this host hands out its own
/// [`TerminalHandle`] ids and maps them to the client's until the
/// terminal is released.
pub struct AcpTerminal {
    client: ClientRequester,
    terminals: Mutex<HashMap<Uuid, String>>,
}

impl AcpTerminal {
    pub fn new(
        session_id: String,
        notify_tx: mpsc::UnboundedSender<String>,
        pending: PendingRequests,
    ) -> Self {
        Self {
            client: ClientRequester::new(session_id, notify_tx, pending),
            terminals: Mutex::new(HashMap::new()),
        }
    }

    /// Builds the params that address `handle`'s terminal on the client.
    fn ref_params(&self, handle: &TerminalHandle) -> io::Result<TerminalRefParams> {
        let terminal_id = self
            .terminals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| unknown_terminal(handle))?;
        Ok(TerminalRefParams {
            session_id: self.client.session_id.clone(),
            terminal_id,
        })
    }
}

#[async_trait]
impl Terminal for AcpTerminal {
    async fn terminal_create(&self, spec: TerminalSpec) -> io::Result<TerminalHandle> {
        let mut env: Vec<EnvVariable> = spec
            .env
            .into_iter()
            .map(|(name, value)| EnvVariable { name, value })
            .collect();
        // HashMap order is arbitrary; keep the wire form deterministic.
        env.sort_by(|a, b| a.name.cmp(&b.name));
        let params = TerminalCreateParams {
            session_id: self.client.session_id.clone(),
            command: spec.command,
            args: spec.args,
            env,
            cwd: spec.cwd.map(|p| p.to_string_lossy().into_owned()),
            output_byte_limit: spec.output_byte_limit.map(|n| n as u64),
        };
        let result: TerminalCreateResult = self
            .client
            .request_as("terminal/create", params, Some(REQUEST_TIMEOUT))
            .await?;
        let handle = TerminalHandle { id: Uuid::now_v7() };
        tracing::debug!(id = %handle.id, terminal_id = %result.terminal_id, "terminal_create");
        self.terminals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(handle.id, result.terminal_id);
        Ok(handle)
    }

    async fn terminal_snapshot(&self, handle: &TerminalHandle) -> io::Result<TerminalSnapshot> {
        let params = self.ref_params(handle)?;
        let result: TerminalOutputResult = self
            .client
            .request_as("terminal/output", params, Some(REQUEST_TIMEOUT))
            .await?;
        Ok(TerminalSnapshot {
            output: result.output,
            truncated: result.truncated,
            exit_status: result.exit_status.map(Into::into),
        })
    }

    async fn terminal_wait_for_exit(
        &self,
        handle: &TerminalHandle,
    ) -> io::Result<TerminalExitStatus> {
        let params = self.ref_params(handle)?;
        // No timeout: the command may legitimately run for a long time, and
        // callers that need a deadline impose their own.
        let status: WireExitStatus = self
            .client
            .request_as("terminal/wait_for_exit", params, None)
            .await?;
        Ok(status.into())
    }

    async fn terminal_kill(&self, handle: &TerminalHandle) -> io::Result<()> {
        tracing::debug!(id = %handle.id, "terminal_kill");
        let params = self.ref_params(handle)?;
        self.client
            .request("terminal/kill", params, Some(REQUEST_TIMEOUT))
            .await?;
        Ok(())
    }

    async fn terminal_release(&self, handle: TerminalHandle) -> io::Result<()> {
        tracing::debug!(id = %handle.id, "terminal_release");
        // Forget the mapping up front: the handle is invalid from here on,
        // even if the client fails to answer.
        let terminal_id = self
            .terminals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&handle.id)
            .ok_or_else(|| unknown_terminal(&handle))?;
        let params = TerminalRefParams {
            session_id: self.client.session_id.clone(),
            terminal_id,
        };
        self.client
            .request("terminal/release", params, Some(REQUEST_TIMEOUT))
            .await?;
        Ok(())
    }
}

impl From<WireExitStatus> for TerminalExitStatus {
    fn from(status: WireExitStatus) -> Self {
        Self {
            exit_code: status.exit_code,
            signal: status.signal,
        }
    }
}

fn unknown_terminal(handle: &TerminalHandle) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("unknown terminal: {}", handle.id),
    )
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
