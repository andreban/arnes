// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod support;

use std::{path::PathBuf, sync::Arc};

use arnes_core::{EventKind, ModelKey, Session, StopReason, ToolCallOutcome};
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
    env.add_file(cwd.join("Cargo.toml"), "[package]\nname = \"arnes\"\n");
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

    let started = events.iter().any(
        |e| matches!(&e.kind, EventKind::ToolCallStarted { name, .. } if name == "read_text_file"),
    );
    assert!(started, "expected a ToolCallStarted for read_text_file");

    let finished_ok = events.iter().any(|e| {
        matches!(
            &e.kind,
            EventKind::ToolCallFinished { name, outcome: ToolCallOutcome::Ok(_), .. }
                if name == "read_text_file"
        )
    });
    assert!(
        finished_ok,
        "expected a successful ToolCallFinished for read_text_file"
    );

    let last_text = events.iter().rev().find_map(|e| match &e.kind {
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
            events.last().unwrap().kind,
            EventKind::TurnEnd {
                stop_reason: StopReason::EndTurn,
                ..
            }
        ),
        "last event should be TurnEnd with EndTurn stop reason"
    );
}
