// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use arnes_core::{Host, ReadTextFile, WriteTextFile};
use async_trait::async_trait;

pub struct InMemoryEnv {
    files: Arc<Mutex<HashMap<PathBuf, String>>>,
}

impl InMemoryEnv {
    pub fn new() -> Self {
        Self {
            files: Arc::new(Mutex::new(HashMap::new())),
        }
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
            ..Default::default()
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
