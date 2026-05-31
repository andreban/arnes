// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! OS-resource abstraction.

use std::{
    io,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use uuid::Uuid;

/// OS-resource trait. Every method defaults to
/// `io::ErrorKind::Unsupported`; impls override what they support.
#[async_trait]
pub trait Host: Send + Sync {
    async fn read_text_file(&self, _path: &Path) -> io::Result<String> {
        unsupported()
    }

    async fn write_text_file(&self, _path: &Path, _contents: &str) -> io::Result<()> {
        unsupported()
    }

    async fn list_directory(&self, _path: &Path) -> io::Result<Vec<DirEntry>> {
        unsupported()
    }

    async fn terminal_create(&self, _spec: TerminalSpec) -> io::Result<TerminalHandle> {
        unsupported()
    }

    async fn terminal_snapshot(&self, _handle: &TerminalHandle) -> io::Result<TerminalSnapshot> {
        unsupported()
    }

    async fn terminal_kill(&self, _handle: &TerminalHandle) -> io::Result<()> {
        unsupported()
    }

    async fn terminal_release(&self, _handle: TerminalHandle) -> io::Result<()> {
        unsupported()
    }
}

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "operation not implemented",
    ))
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug)]
pub struct TerminalSpec {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
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

/// In-process host. All methods inherit the trait's default Unsupported
/// behaviour until concrete implementations are added.
pub struct LocalHost;

#[async_trait]
impl Host for LocalHost {}
