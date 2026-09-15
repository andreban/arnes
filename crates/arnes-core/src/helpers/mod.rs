// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::path::{Component, Path, PathBuf};

/// Lexically normalizes `path` relative to `cwd`.
///
/// If `path` is relative, it is joined to `cwd`. The resulting path is then
/// normalized purely by inspecting its lexical components: `.` components are
/// dropped, and `..` components pop the preceding normal directory component
/// if one exists.
///
/// This function does not perform any filesystem I/O, does not resolve
/// symlinks, and does not require target paths to exist.
pub fn normalize_path(cwd: &Path, path: &Path) -> PathBuf {
    let combined = if path.is_relative() {
        cwd.join(path)
    } else {
        path.to_path_buf()
    };

    let mut components = Vec::new();
    for component in combined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match components.last() {
                Some(Component::Normal(_)) => {
                    components.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                Some(Component::ParentDir) | None => {
                    components.push(component);
                }
                Some(Component::CurDir) => unreachable!(),
            },
            c => components.push(c),
        }
    }

    if components.is_empty() {
        PathBuf::from(".")
    } else {
        components.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_and_dot_prefixes_match() {
        let cwd = Path::new("/workspace");
        let p1 = normalize_path(cwd, Path::new("src/main.rs"));
        let p2 = normalize_path(cwd, Path::new("./src/main.rs"));
        let p3 = normalize_path(cwd, Path::new("././src/./main.rs"));
        assert_eq!(p1, p2);
        assert_eq!(p2, p3);
    }

    #[test]
    fn parent_dir_components_resolve() {
        let cwd = Path::new("/workspace");
        let p1 = normalize_path(cwd, Path::new("src/../src/main.rs"));
        let p2 = normalize_path(cwd, Path::new("src/main.rs"));
        assert_eq!(p1, p2);

        let p3 = normalize_path(cwd, Path::new("../workspace/src/main.rs"));
        assert_eq!(p3, p2);
    }

    #[test]
    fn absolute_path_matches_relative_under_cwd() {
        let cwd = Path::new("/workspace");
        let relative = normalize_path(cwd, Path::new("src/main.rs"));
        let absolute = normalize_path(cwd, Path::new("/workspace/src/main.rs"));
        assert_eq!(relative, absolute);
    }

    #[test]
    fn dot_cwd_normalizes_cleanly() {
        let cwd = Path::new(".");
        let p1 = normalize_path(cwd, Path::new("Cargo.toml"));
        let p2 = normalize_path(cwd, Path::new("./Cargo.toml"));
        assert_eq!(p1, PathBuf::from("Cargo.toml"));
        assert_eq!(p1, p2);
    }

    #[test]
    fn empty_or_dot_path_resolves_to_dot() {
        assert_eq!(
            normalize_path(Path::new("."), Path::new(".")),
            PathBuf::from(".")
        );
    }
}
