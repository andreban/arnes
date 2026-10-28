// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! [`Terminal`] backed by local child processes.

use std::{
    collections::{HashMap, VecDeque},
    io,
    process::{ExitStatus, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

use arnes_core::{Terminal, TerminalExitStatus, TerminalHandle, TerminalSnapshot, TerminalSpec};
use async_trait::async_trait;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::watch,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Output retained per terminal when the spec sets no limit.
const DEFAULT_OUTPUT_BYTE_LIMIT: usize = 1024 * 1024;

/// How long to keep draining output after the process exits. A background
/// grandchild (`cmd &`) can hold the pipes open indefinitely, so the
/// readers are abandoned after this grace period.
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_millis(250);

/// Spawns each terminal as a direct child process with stdout and stderr
/// captured into a shared, size-bounded buffer.
#[derive(Default)]
pub struct LocalTerminal {
    processes: Mutex<HashMap<Uuid, LocalProcess>>,
}

struct LocalProcess {
    output: Arc<Mutex<OutputBuffer>>,
    exit: watch::Receiver<Option<TerminalExitStatus>>,
    kill: CancellationToken,
}

impl Drop for LocalProcess {
    fn drop(&mut self) {
        // Releasing a handle, or dropping the whole `LocalTerminal`, must
        // not leave the child running.
        self.kill.cancel();
    }
}

impl LocalTerminal {
    fn process<T>(
        &self,
        handle: &TerminalHandle,
        f: impl FnOnce(&LocalProcess) -> T,
    ) -> io::Result<T> {
        let processes = self.processes.lock().unwrap_or_else(|e| e.into_inner());
        processes
            .get(&handle.id)
            .map(f)
            .ok_or_else(|| unknown(handle))
    }
}

#[async_trait]
impl Terminal for LocalTerminal {
    async fn terminal_create(&self, spec: TerminalSpec) -> io::Result<TerminalHandle> {
        tracing::debug!(command = %spec.command, args = ?spec.args, "terminal_create: starting");
        let mut command = Command::new(&spec.command);
        command
            .args(&spec.args)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        let mut child = command.spawn().inspect_err(|e| {
            tracing::error!(command = %spec.command, error = %e, "terminal_create: spawn failed");
        })?;

        let output = Arc::new(Mutex::new(OutputBuffer::new(
            spec.output_byte_limit.unwrap_or(DEFAULT_OUTPUT_BYTE_LIMIT),
        )));
        let readers = [
            child
                .stdout
                .take()
                .map(|r| spawn_reader(r, Arc::clone(&output))),
            child
                .stderr
                .take()
                .map(|r| spawn_reader(r, Arc::clone(&output))),
        ];

        let (exit_tx, exit_rx) = watch::channel(None);
        let kill = CancellationToken::new();
        let id = Uuid::now_v7();
        let kill_requested = kill.clone();
        tokio::spawn(async move {
            let status = tokio::select! {
                status = child.wait() => status,
                () = kill_requested.cancelled() => {
                    let _ = child.start_kill();
                    child.wait().await
                }
            };
            for reader in readers.into_iter().flatten() {
                let _ = tokio::time::timeout(OUTPUT_DRAIN_TIMEOUT, reader).await;
            }
            let status = match status {
                Ok(status) => exit_status(status),
                Err(e) => {
                    tracing::error!(%id, error = %e, "terminal: wait failed");
                    TerminalExitStatus::default()
                }
            };
            tracing::debug!(%id, ?status, "terminal: process exited");
            let _ = exit_tx.send(Some(status));
        });

        self.processes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                id,
                LocalProcess {
                    output,
                    exit: exit_rx,
                    kill,
                },
            );
        tracing::debug!(%id, "terminal_create: spawned");
        Ok(TerminalHandle { id })
    }

    async fn terminal_snapshot(&self, handle: &TerminalHandle) -> io::Result<TerminalSnapshot> {
        self.process(handle, |process| {
            let (output, truncated) = process
                .output
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contents();
            TerminalSnapshot {
                output,
                truncated,
                exit_status: process.exit.borrow().clone(),
            }
        })
    }

    async fn terminal_wait_for_exit(
        &self,
        handle: &TerminalHandle,
    ) -> io::Result<TerminalExitStatus> {
        let mut exit = self.process(handle, |process| process.exit.clone())?;
        let status = exit
            .wait_for(Option::is_some)
            .await
            .map_err(|_| io::Error::other("terminal exited without reporting a status"))?;
        Ok(status.clone().unwrap_or_default())
    }

    async fn terminal_kill(&self, handle: &TerminalHandle) -> io::Result<()> {
        tracing::debug!(id = %handle.id, "terminal_kill");
        self.process(handle, |process| process.kill.cancel())
    }

    async fn terminal_release(&self, handle: TerminalHandle) -> io::Result<()> {
        tracing::debug!(id = %handle.id, "terminal_release");
        // Dropping the `LocalProcess` kills the child if it is still running.
        self.processes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&handle.id)
            .map(drop)
            .ok_or_else(|| unknown(&handle))
    }
}

/// Keeps the most recent `limit` bytes of output.
struct OutputBuffer {
    bytes: VecDeque<u8>,
    limit: usize,
    truncated: bool,
}

impl OutputBuffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: VecDeque::new(),
            limit,
            truncated: false,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend(chunk);
        let excess = self.bytes.len().saturating_sub(self.limit);
        if excess > 0 {
            self.bytes.drain(..excess);
            self.truncated = true;
        }
    }

    fn contents(&mut self) -> (String, bool) {
        let mut bytes: &[u8] = self.bytes.make_contiguous();
        if self.truncated {
            // Dropping from the front can split a UTF-8 sequence; skip its
            // orphaned continuation bytes rather than emit a replacement char.
            let start = bytes
                .iter()
                .take(3)
                .take_while(|&&b| b & 0xC0 == 0x80)
                .count();
            bytes = &bytes[start..];
        }
        (String::from_utf8_lossy(bytes).into_owned(), self.truncated)
    }
}

fn spawn_reader(
    mut reader: impl AsyncRead + Unpin + Send + 'static,
    output: Arc<Mutex<OutputBuffer>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk).await {
                Ok(0) => break,
                Ok(n) => output
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(&chunk[..n]),
                Err(e) => {
                    tracing::warn!(error = %e, "terminal: output read failed");
                    break;
                }
            }
        }
    })
}

fn exit_status(status: ExitStatus) -> TerminalExitStatus {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map(signal_name)
    };
    #[cfg(not(unix))]
    let signal = None;
    TerminalExitStatus {
        exit_code: status.code(),
        signal,
    }
}

#[cfg(unix)]
fn signal_name(signal: i32) -> String {
    let name = match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        6 => "SIGABRT",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        15 => "SIGTERM",
        other => return other.to_string(),
    };
    name.to_string()
}

fn unknown(handle: &TerminalHandle) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("unknown terminal: {}", handle.id),
    )
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
