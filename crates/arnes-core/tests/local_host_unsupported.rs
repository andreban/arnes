// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{io, path::PathBuf};

use arnes_core::{Host, LocalHost, TerminalHandle, TerminalSpec};
use uuid::Uuid;

fn handle() -> TerminalHandle {
    TerminalHandle { id: Uuid::now_v7() }
}

fn spec() -> TerminalSpec {
    TerminalSpec {
        command: "echo".into(),
        args: vec!["hi".into()],
        cwd: None,
    }
}

#[tokio::test]
async fn read_text_file_is_unsupported() {
    let err = LocalHost
        .read_text_file("/tmp/x".as_ref())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
}

#[tokio::test]
async fn write_text_file_is_unsupported() {
    let err = LocalHost
        .write_text_file("/tmp/x".as_ref(), "hi")
        .await
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
}

#[tokio::test]
async fn list_directory_is_unsupported() {
    let err = LocalHost
        .list_directory(PathBuf::from("/tmp").as_ref())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
}

#[tokio::test]
async fn terminal_create_is_unsupported() {
    let err = LocalHost.terminal_create(spec()).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
}

#[tokio::test]
async fn terminal_snapshot_is_unsupported() {
    let h = handle();
    let err = LocalHost.terminal_snapshot(&h).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
}

#[tokio::test]
async fn terminal_kill_is_unsupported() {
    let h = handle();
    let err = LocalHost.terminal_kill(&h).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
}

#[tokio::test]
async fn terminal_release_is_unsupported() {
    let err = LocalHost.terminal_release(handle()).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
}
