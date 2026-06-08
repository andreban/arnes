// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use arnes_core::{Host, ReadTextFile};
use async_trait::async_trait;

pub struct InMemoryEnv {
    files: HashMap<PathBuf, String>,
}

impl InMemoryEnv {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    pub fn add_file(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files.insert(path.into(), content.into());
    }

    pub fn build_host(&self) -> Host {
        Host {
            read_text_file: Some(Arc::new(InMemoryReadTextFile {
                files: self.files.clone(),
            })),
            ..Default::default()
        }
    }
}

struct InMemoryReadTextFile {
    files: HashMap<PathBuf, String>,
}

#[async_trait]
impl ReadTextFile for InMemoryReadTextFile {
    async fn read_text_file(
        &self,
        path: &Path,
        line: Option<usize>,
        limit: Option<usize>,
    ) -> io::Result<String> {
        let content = self.files.get(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found: {}", path.display()),
            )
        })?;

        let lines: Vec<&str> = content.lines().collect();
        // line is 1-indexed; None means start from beginning
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
