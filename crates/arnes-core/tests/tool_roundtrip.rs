// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod support;

use std::{path::PathBuf, sync::Arc};

use arnes_core::{
    EventKind, ModelKey, Session, StopReason, ToolCallFinish, ToolCallOutcome, normalize_path,
};
use support::{InMemoryEnv, RecordingFrontend, ScriptedLlm, ScriptedTurn};
use tokio_util::sync::CancellationToken;

fn model_key() -> ModelKey {
    ModelKey {
        provider: "test".into(),
        model_id: "scripted".into(),
    }
}

#[tokio::test]
async fn read_tool_roundtrip() {
    let cwd = PathBuf::from(".");
    let mut env = InMemoryEnv::new();
    // The tool resolves relative paths against `cwd`, so the fixture must be
    // keyed by the same resolved path the tool will look up.
    let target_path = normalize_path(&cwd, &PathBuf::from("Cargo.toml"));
    env.add_file(target_path, "[package]\nname = \"arnes\"\n");
    let host = env.build_host();

    let llm = ScriptedLlm::new(vec![ScriptedTurn::ToolCallThenText {
        name: "read_text_file".into(),
        args: serde_json::json!({ "path": "Cargo.toml" }),
        follow_up: "The file contains a package named arnes.".into(),
    }]);

    let frontend = RecordingFrontend::new();
    let mut session = Session::new(Arc::clone(&frontend), host, llm, model_key(), cwd);
    session
        .prompt("what is in Cargo.toml?", CancellationToken::new())
        .await
        .unwrap();

    let events = frontend.events();

    let started = events
        .iter()
        .any(|e| matches!(e, EventKind::ToolCallStarted(start) if start.tool_call.name == "read_text_file"));
    assert!(started, "expected a ToolCallStarted for read_text_file");

    let finished_ok = events.iter().any(|e| {
        matches!(
            e,
            EventKind::ToolCallFinished(ToolCallFinish { tool_call, outcome: ToolCallOutcome::Ok(_), .. })
                if tool_call.name == "read_text_file"
        )
    });
    assert!(
        finished_ok,
        "expected a successful ToolCallFinished for read_text_file"
    );

    let last_text = events.iter().rev().find_map(|e| match e {
        EventKind::TextDelta { text } => Some(text.clone()),
        _ => None,
    });
    assert_eq!(
        last_text.as_deref(),
        Some("The file contains a package named arnes."),
        "final TextDelta should carry the scripted follow-up"
    );

    assert!(
        matches!(
            events.last().unwrap(),
            EventKind::TurnEnd {
                stop_reason: StopReason::EndTurn,
                ..
            }
        ),
        "last event should be TurnEnd with EndTurn stop reason"
    );
}

#[tokio::test]
async fn edit_tool_roundtrip() {
    let cwd = PathBuf::from(".");
    let mut env = InMemoryEnv::new();
    let target_path = normalize_path(&cwd, &PathBuf::from("greeting.txt"));
    env.add_file(target_path.clone(), "hello world");
    let host = env.build_host();

    // The agent must read a file before it may edit it, so the read-then-edit
    // sequence mirrors the real flow: the read grants the path, the edit uses it.
    let llm = ScriptedLlm::new(vec![
        ScriptedTurn::ToolCall {
            name: "read_text_file".into(),
            args: serde_json::json!({ "path": "greeting.txt" }),
        },
        ScriptedTurn::ToolCallThenText {
            name: "edit_text_file".into(),
            args: serde_json::json!({
                "path": "greeting.txt",
                "edits": [{ "old_text": "world", "new_text": "agent" }],
            }),
            follow_up: "File edited successfully.".into(),
        },
    ]);

    let frontend = RecordingFrontend::new();
    let mut session = Session::new(Arc::clone(&frontend), host, llm, model_key(), cwd);
    session
        .prompt("replace world with agent", CancellationToken::new())
        .await
        .unwrap();

    let events = frontend.events();

    let started = events
        .iter()
        .any(|e| matches!(e, EventKind::ToolCallStarted(start) if start.tool_call.name == "edit_text_file"));
    assert!(started, "expected a ToolCallStarted for edit_text_file");

    let finished_ok = events.iter().any(|e| {
        matches!(
            e,
            EventKind::ToolCallFinished(ToolCallFinish { tool_call, outcome: ToolCallOutcome::Ok(_), .. })
                if tool_call.name == "edit_text_file"
        )
    });
    assert!(
        finished_ok,
        "expected a successful ToolCallFinished for edit_text_file"
    );

    assert_eq!(
        env.read_file(&target_path).as_deref(),
        Some("hello agent"),
        "edited content should be visible in the env"
    );

    assert!(
        matches!(
            events.last().unwrap(),
            EventKind::TurnEnd {
                stop_reason: StopReason::EndTurn,
                ..
            }
        ),
        "last event should be TurnEnd with EndTurn stop reason"
    );
}

#[tokio::test]
async fn write_tool_roundtrip() {
    let cwd = PathBuf::from(".");
    let env = InMemoryEnv::new();
    let host = env.build_host();
    let target_path = normalize_path(&cwd, &PathBuf::from("generated.txt"));

    let llm = ScriptedLlm::new(vec![ScriptedTurn::ToolCallThenText {
        name: "write_text_file".into(),
        args: serde_json::json!({ "path": "generated.txt", "content": "hello from agent" }),
        follow_up: "File written successfully.".into(),
    }]);

    let frontend = RecordingFrontend::new();
    let mut session = Session::new(Arc::clone(&frontend), host, llm, model_key(), cwd);
    session
        .prompt(
            "write hello from agent to generated.txt",
            CancellationToken::new(),
        )
        .await
        .unwrap();

    let events = frontend.events();

    let started = events
        .iter()
        .any(|e| matches!(e, EventKind::ToolCallStarted(start) if start.tool_call.name == "write_text_file"));
    assert!(started, "expected a ToolCallStarted for write_text_file");

    let finished_ok = events.iter().any(|e| {
        matches!(
            e,
            EventKind::ToolCallFinished(ToolCallFinish { tool_call, outcome: ToolCallOutcome::Ok(_), .. })
                if tool_call.name == "write_text_file"
        )
    });
    assert!(
        finished_ok,
        "expected a successful ToolCallFinished for write_text_file"
    );

    assert_eq!(
        env.read_file(&target_path).as_deref(),
        Some("hello from agent"),
        "written content should be visible in the env"
    );

    assert!(
        matches!(
            events.last().unwrap(),
            EventKind::TurnEnd {
                stop_reason: StopReason::EndTurn,
                ..
            }
        ),
        "last event should be TurnEnd with EndTurn stop reason"
    );
}

#[tokio::test]
async fn failed_read_does_not_grant_edit() {
    let cwd = PathBuf::from(".");
    let env = InMemoryEnv::new();
    let host = env.build_host();

    let llm = ScriptedLlm::new(vec![
        ScriptedTurn::ToolCall {
            name: "read_text_file".into(),
            args: serde_json::json!({ "path": "non_existent.txt" }),
        },
        ScriptedTurn::ToolCallThenText {
            name: "edit_text_file".into(),
            args: serde_json::json!({
                "path": "non_existent.txt",
                "edits": [{ "old_text": "foo", "new_text": "bar" }],
            }),
            follow_up: "Finished.".into(),
        },
    ]);

    let frontend = RecordingFrontend::new();
    let mut session = Session::new(Arc::clone(&frontend), host, llm, model_key(), cwd);
    session
        .prompt("read then edit non_existent.txt", CancellationToken::new())
        .await
        .unwrap();

    let events = frontend.events();

    let edit_finished_err = events.iter().any(|e| {
        matches!(
            e,
            EventKind::ToolCallFinished(ToolCallFinish {
                tool_call,
                outcome: ToolCallOutcome::Err(msg),
                ..
            }) if tool_call.name == "edit_text_file"
                && msg.contains("must be read before it can be edited")
        )
    });
    assert!(
        edit_finished_err,
        "expected edit_text_file to fail with missing read grant error"
    );
}

#[tokio::test]
async fn read_grants_path_normalization_mismatch() {
    let cwd = PathBuf::from(".");
    let mut env = InMemoryEnv::new();
    let target_path = normalize_path(&cwd, &PathBuf::from("greeting.txt"));
    env.add_file(target_path.clone(), "hello world");
    let host = env.build_host();

    // Model reads using `./greeting.txt` and edits using `greeting.txt`.
    let llm = ScriptedLlm::new(vec![
        ScriptedTurn::ToolCall {
            name: "read_text_file".into(),
            args: serde_json::json!({ "path": "./greeting.txt" }),
        },
        ScriptedTurn::ToolCallThenText {
            name: "edit_text_file".into(),
            args: serde_json::json!({
                "path": "greeting.txt",
                "edits": [{ "old_text": "world", "new_text": "agent" }],
            }),
            follow_up: "File edited successfully.".into(),
        },
    ]);

    let frontend = RecordingFrontend::new();
    let mut session = Session::new(Arc::clone(&frontend), host, llm, model_key(), cwd);
    session
        .prompt("replace world with agent", CancellationToken::new())
        .await
        .unwrap();

    let events = frontend.events();

    let edit_ok = events.iter().any(|e| {
        matches!(
            e,
            EventKind::ToolCallFinished(ToolCallFinish { tool_call, outcome: ToolCallOutcome::Ok(_), .. })
                if tool_call.name == "edit_text_file"
        )
    });
    assert!(
        edit_ok,
        "expected edit_text_file to succeed when read used ./ prefix"
    );
    assert_eq!(env.read_file(&target_path).as_deref(), Some("hello agent"));
}

#[tokio::test]
async fn write_tool_grants_edit_permission() {
    let cwd = PathBuf::from(".");
    let env = InMemoryEnv::new();
    let host = env.build_host();
    let target_path = normalize_path(&cwd, &PathBuf::from("notes.txt"));

    // Model writes a file, then edits it immediately without reading it back.
    let llm = ScriptedLlm::new(vec![
        ScriptedTurn::ToolCall {
            name: "write_text_file".into(),
            args: serde_json::json!({
                "path": "notes.txt",
                "content": "first draft",
            }),
        },
        ScriptedTurn::ToolCallThenText {
            name: "edit_text_file".into(),
            args: serde_json::json!({
                "path": "./notes.txt",
                "edits": [{ "old_text": "first", "new_text": "second" }],
            }),
            follow_up: "File updated.".into(),
        },
    ]);

    let frontend = RecordingFrontend::new();
    let mut session = Session::new(Arc::clone(&frontend), host, llm, model_key(), cwd);
    session
        .prompt("write and then edit notes.txt", CancellationToken::new())
        .await
        .unwrap();

    let events = frontend.events();

    let write_ok = events.iter().any(|e| {
        matches!(
            e,
            EventKind::ToolCallFinished(ToolCallFinish { tool_call, outcome: ToolCallOutcome::Ok(_), .. })
                if tool_call.name == "write_text_file"
        )
    });
    assert!(write_ok, "expected write_text_file to succeed");

    let edit_ok = events.iter().any(|e| {
        matches!(
            e,
            EventKind::ToolCallFinished(ToolCallFinish { tool_call, outcome: ToolCallOutcome::Ok(_), .. })
                if tool_call.name == "edit_text_file"
        )
    });
    assert!(
        edit_ok,
        "expected edit_text_file to succeed following write_text_file without explicit read"
    );

    assert_eq!(env.read_file(&target_path).as_deref(), Some("second draft"));
}
