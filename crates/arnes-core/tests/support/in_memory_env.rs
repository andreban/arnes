// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use arnes_core::{
    Host, ReadTextFile, Terminal, TerminalExitStatus, TerminalHandle, TerminalSnapshot,
    TerminalSpec, WriteTextFile,
};
use async_trait::async_trait;
use uuid::Uuid;

pub struct InMemoryEnv {
    files: Arc<Mutex<HashMap<PathBuf, String>>>,
    terminal: Arc<FakeTerminal>,
}

impl InMemoryEnv {
    pub fn new() -> Self {
        Self {
            files: Arc::new(Mutex::new(HashMap::new())),
            terminal: Arc::new(FakeTerminal::default()),
        }
    }

    /// Scripts the result of running `command_line` (the command and its
    /// arguments joined by spaces). Unscripted commands exit with 127.
    pub fn add_command(
        &mut self,
        command_line: impl Into<String>,
        output: impl Into<String>,
        exit_code: i32,
    ) {
        self.terminal.commands.lock().unwrap().insert(
            command_line.into(),
            CannedCommand {
                output: output.into(),
                exit_code: Some(exit_code),
            },
        );
    }

    /// Scripts `command_line` to print `output` and then run until it is
    /// killed or released.
    pub fn add_hanging_command(
        &mut self,
        command_line: impl Into<String>,
        output: impl Into<String>,
    ) {
        self.terminal.commands.lock().unwrap().insert(
            command_line.into(),
            CannedCommand {
                output: output.into(),
                exit_code: None,
            },
        );
    }

    /// How many times `terminal_kill` was called.
    pub fn kills(&self) -> usize {
        *self.terminal.kills.lock().unwrap()
    }

    /// Every spec passed to `terminal_create`, in call order.
    pub fn spawned(&self) -> Vec<TerminalSpec> {
        self.terminal.spawned.lock().unwrap().clone()
    }

    /// Terminals created but not yet released.
    pub fn live_terminals(&self) -> usize {
        self.terminal.live.lock().unwrap().len()
    }

    pub fn add_file(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), content.into());
    }

    /// Returns the current contents of `path`, including anything written via the host.
    pub fn read_file(&self, path: impl AsRef<Path>) -> Option<String> {
        self.files.lock().unwrap().get(path.as_ref()).cloned()
    }

    pub fn build_host(&self) -> Host {
        Host {
            read_text_file: Some(Arc::new(InMemoryReadTextFile {
                files: Arc::clone(&self.files),
            })),
            write_text_file: Some(Arc::new(InMemoryWriteTextFile {
                files: Arc::clone(&self.files),
            })),
            terminal: Some(Arc::clone(&self.terminal) as Arc<dyn Terminal>),
        }
    }
}

struct InMemoryReadTextFile {
    files: Arc<Mutex<HashMap<PathBuf, String>>>,
}

#[async_trait]
impl ReadTextFile for InMemoryReadTextFile {
    async fn read_text_file(
        &self,
        path: &Path,
        line: Option<usize>,
        limit: Option<usize>,
    ) -> io::Result<String> {
        let files = self.files.lock().unwrap();
        let content = files.get(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found: {}", path.display()),
            )
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let start = line
            .map(|n| n.saturating_sub(1))
            .unwrap_or(0)
            .min(lines.len());
        let slice = &lines[start..];
        let slice = match limit {
            Some(lim) => &slice[..slice.len().min(lim)],
            None => slice,
        };
        Ok(slice.join("\n"))
    }
}

struct InMemoryWriteTextFile {
    files: Arc<Mutex<HashMap<PathBuf, String>>>,
}

#[async_trait]
impl WriteTextFile for InMemoryWriteTextFile {
    async fn write_text_file(&self, path: &Path, content: &str) -> io::Result<()> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), content.to_owned());
        Ok(())
    }
}

struct CannedCommand {
    output: String,
    /// `None` keeps the command running until it is killed or released.
    exit_code: Option<i32>,
}

struct FakeProcess {
    output: String,
    exit: tokio::sync::watch::Sender<Option<TerminalExitStatus>>,
}

/// Records spawned commands and completes each one with its scripted
/// output and exit code; hanging commands run until killed.
#[derive(Default)]
struct FakeTerminal {
    commands: Mutex<HashMap<String, CannedCommand>>,
    spawned: Mutex<Vec<TerminalSpec>>,
    live: Mutex<HashMap<Uuid, FakeProcess>>,
    kills: Mutex<usize>,
}

impl FakeTerminal {
    fn with_process<T>(
        &self,
        handle: &TerminalHandle,
        f: impl FnOnce(&FakeProcess) -> T,
    ) -> io::Result<T> {
        self.live
            .lock()
            .unwrap()
            .get(&handle.id)
            .map(f)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown terminal"))
    }
}

fn exited(exit_code: i32) -> Option<TerminalExitStatus> {
    Some(TerminalExitStatus {
        exit_code: Some(exit_code),
        signal: None,
    })
}

#[async_trait]
impl Terminal for FakeTerminal {
    async fn terminal_create(&self, spec: TerminalSpec) -> io::Result<TerminalHandle> {
        let command_line = std::iter::once(&spec.command)
            .chain(&spec.args)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        let (output, exit) = match self.commands.lock().unwrap().get(&command_line) {
            Some(canned) => (canned.output.clone(), canned.exit_code.and_then(exited)),
            None => (
                format!("{}: command not found\n", spec.command),
                exited(127),
            ),
        };
        self.spawned.lock().unwrap().push(spec);
        let handle = TerminalHandle { id: Uuid::now_v7() };
        let (exit, _) = tokio::sync::watch::channel(exit);
        self.live
            .lock()
            .unwrap()
            .insert(handle.id, FakeProcess { output, exit });
        Ok(handle)
    }

    async fn terminal_snapshot(&self, handle: &TerminalHandle) -> io::Result<TerminalSnapshot> {
        self.with_process(handle, |process| TerminalSnapshot {
            output: process.output.clone(),
            truncated: false,
            exit_status: process.exit.borrow().clone(),
        })
    }

    async fn terminal_wait_for_exit(
        &self,
        handle: &TerminalHandle,
    ) -> io::Result<TerminalExitStatus> {
        let mut exit = self.with_process(handle, |process| process.exit.subscribe())?;
        let status = exit
            .wait_for(Option::is_some)
            .await
            .map_err(|_| io::Error::other("terminal released while waiting"))?;
        Ok(status.clone().unwrap_or_default())
    }

    async fn terminal_kill(&self, handle: &TerminalHandle) -> io::Result<()> {
        *self.kills.lock().unwrap() += 1;
        self.with_process(handle, kill)
    }

    async fn terminal_release(&self, handle: TerminalHandle) -> io::Result<()> {
        let process = self
            .live
            .lock()
            .unwrap()
            .remove(&handle.id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown terminal"))?;
        kill(&process);
        Ok(())
    }
}

fn kill(process: &FakeProcess) {
    process.exit.send_if_modified(|exit| {
        if exit.is_some() {
            return false;
        }
        *exit = Some(TerminalExitStatus {
            exit_code: None,
            signal: Some("SIGKILL".into()),
        });
        true
    });
}
