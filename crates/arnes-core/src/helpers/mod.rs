// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

pub fn normalize_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_relative() {
        cwd.join(path)
    } else {
        path.to_path_buf()
    }
}
