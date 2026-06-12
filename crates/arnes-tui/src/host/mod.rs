// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{path::Path, sync::Arc};

use arnes_core::{Host, ReadTextFile, WriteTextFile};
use async_trait::async_trait;
use tokio::io;

pub fn tui_host() -> Host {
    Host {
        read_text_file: Some(Arc::new(ReadLocalTextFile {})),
        write_text_file: Some(Arc::new(WriteLocalTextFile {})),
        ..Default::default()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReadLocalTextFile {}

#[async_trait]
impl ReadTextFile for ReadLocalTextFile {
    async fn read_text_file(
        &self,
        path: &Path,
        line: Option<usize>,
        limit: Option<usize>,
    ) -> io::Result<String> {
        tracing::debug!(path = %path.display(), line, limit, "read_text_file: starting");
        let contents = tokio::fs::read_to_string(path).await.inspect_err(|e| {
            tracing::error!(path = %path.display(), error = %e, "read_text_file: failed");
        })?;
        tracing::debug!(path = %path.display(), bytes = contents.len(), "read_text_file: read ok");
        if line.is_none() && limit.is_none() {
            return Ok(contents);
        }
        let start = line.unwrap_or(1).saturating_sub(1);
        let count = limit.unwrap_or(usize::MAX);
        let mut out = contents
            .lines()
            .skip(start)
            .take(count)
            .collect::<Vec<_>>()
            .join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WriteLocalTextFile {}

#[async_trait]
impl WriteTextFile for WriteLocalTextFile {
    async fn write_text_file(&self, path: &Path, content: &str) -> io::Result<()> {
        tracing::debug!(path = %path.display(), bytes = content.len(), "write_text_file: starting");
        tokio::fs::write(path, content).await.inspect_err(|e| {
            tracing::error!(path = %path.display(), error = %e, "write_text_file: failed");
        })?;
        tracing::debug!(path = %path.display(), "write_text_file: write ok");
        Ok(())
    }
}
