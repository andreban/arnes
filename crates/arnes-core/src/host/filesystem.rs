//! Filesystem capabilities a host may expose.

use std::{io, path::Path};

use async_trait::async_trait;

/// Reads a UTF-8 text file.
#[async_trait]
pub trait ReadTextFile: Send + Sync {
    /// Reads `path`, optionally skipping to a starting line and capping
    /// the number of lines returned.
    ///
    /// `line` is 1-indexed; `None` reads from the start. `limit` is the
    /// maximum number of lines to return; `None` reads to EOF.
    async fn read_text_file(
        &self,
        path: &Path,
        line: Option<usize>,
        limit: Option<usize>,
    ) -> io::Result<String>;
}

/// Writes a UTF-8 text file, creating it or replacing its contents.
#[async_trait]
pub trait WriteTextFile: Send + Sync {
    async fn write_text_file(&self, path: &Path, content: &str) -> io::Result<()>;
}
