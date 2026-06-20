// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod support;

use std::sync::Arc;

use arnes_core::{EventKind, Host, ModelKey, Session};
use support::{RecordingFrontend, ScriptedLlm, ScriptedTurn};
use tokio_util::sync::CancellationToken;

fn model_key() -> ModelKey {
    ModelKey {
        provider: "test".into(),
        model_id: "scripted".into(),
    }
}

#[tokio::test]
async fn basic_text_roundtrip() {
    let frontend = RecordingFrontend::new();
    let llm = ScriptedLlm::new(vec![ScriptedTurn::Text("Hello!".into())]);
    let host = Host::default();
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut session = Session::new(Arc::clone(&frontend), host, llm, model_key(), cwd);

    session
        .prompt("hi", CancellationToken::new())
        .await
        .unwrap();

    let events = frontend.events();
    assert!(
        matches!(events[0], EventKind::TurnStart),
        "first event should be TurnStart"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EventKind::TextDelta { .. })),
        "expected at least one TextDelta event"
    );
    assert!(
        matches!(events.last().unwrap(), EventKind::TurnEnd { .. }),
        "last event should be TurnEnd"
    );
}
