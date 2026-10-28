// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod support;

use std::{path::PathBuf, sync::Arc, time::Duration};

use arnes_core::{
    EventKind, Host, ModelKey, Session, ToolCallFinish, ToolCallOutcome, ToolResultContent,
};
use serde_json::{Value, json};
use support::{InMemoryEnv, RecordingFrontend, ScriptedLlm, ScriptedTurn};
use tokio_util::sync::CancellationToken;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

fn model_key() -> ModelKey {
    ModelKey {
        provider: "test".into(),
        model_id: "scripted".into(),
    }
}

fn bash_call(args: Value) -> Arc<ScriptedLlm> {
    ScriptedLlm::new(vec![ScriptedTurn::ToolCallThenText {
        name: "bash".into(),
        args,
        follow_up: "Done.".into(),
    }])
}

/// The outcome of the session's single `bash` call.
fn bash_outcome(frontend: &RecordingFrontend) -> Result<ToolCallOutcome> {
    frontend
        .events()
        .into_iter()
        .find_map(|e| match e {
            EventKind::ToolCallFinished(ToolCallFinish { tool_call, outcome })
                if tool_call.name == "bash" =>
            {
                Some(outcome)
            }
            _ => None,
        })
        .ok_or_else(|| "no ToolCallFinished for bash".into())
}

/// The JSON a successful `bash` call returned to the model.
fn bash_output(frontend: &RecordingFrontend) -> Result<Value> {
    match bash_outcome(frontend)? {
        ToolCallOutcome::Ok(blocks) => match blocks.as_slice() {
            [ToolResultContent::Json(value)] => Ok(value.clone()),
            other => Err(format!("unexpected result blocks: {other:?}").into()),
        },
        other => Err(format!("expected a successful bash call, got {other:?}").into()),
    }
}

#[tokio::test]
async fn bash_tool_roundtrip() -> Result<()> {
    // -- Setup & Fixtures
    let cwd = PathBuf::from("/work/project");
    let mut env = InMemoryEnv::new();
    env.add_command("bash -c cargo test", "test result: ok\n", 0);
    let frontend = RecordingFrontend::new();
    let mut session = Session::new(
        Arc::clone(&frontend),
        env.build_host(),
        bash_call(json!({ "command": "cargo test" })),
        model_key(),
        cwd.clone(),
    );

    // -- Exec
    session
        .prompt("run the tests", CancellationToken::new())
        .await?;

    // -- Check
    let output = bash_output(&frontend)?;
    assert_eq!(output["output"], "test result: ok\n");
    assert_eq!(output["exit_code"], 0);
    assert_eq!(output["timed_out"], false);
    assert_eq!(output["truncated"], false);
    let spawned = env.spawned();
    assert_eq!(spawned.len(), 1);
    assert_eq!(spawned[0].cwd.as_deref(), Some(cwd.as_path()));
    assert_eq!(env.live_terminals(), 0, "terminal should be released");

    Ok(())
}

#[tokio::test]
async fn bash_tool_reports_nonzero_exit_as_output() -> Result<()> {
    // -- Setup & Fixtures
    let mut env = InMemoryEnv::new();
    env.add_command(
        "bash -c cargo build",
        "error[E0308]: mismatched types\n",
        101,
    );
    let frontend = RecordingFrontend::new();
    let mut session = Session::new(
        Arc::clone(&frontend),
        env.build_host(),
        bash_call(json!({ "command": "cargo build" })),
        model_key(),
        PathBuf::from("."),
    );

    // -- Exec
    session.prompt("build it", CancellationToken::new()).await?;

    // -- Check
    let output = bash_output(&frontend)?;
    assert_eq!(output["exit_code"], 101);
    assert_eq!(output["output"], "error[E0308]: mismatched types\n");

    Ok(())
}

#[tokio::test]
async fn bash_tool_denied_does_not_run() -> Result<()> {
    // -- Setup & Fixtures
    let env = InMemoryEnv::new();
    let frontend = RecordingFrontend::denying();
    let mut session = Session::new(
        Arc::clone(&frontend),
        env.build_host(),
        bash_call(json!({ "command": "rm -rf build" })),
        model_key(),
        PathBuf::from("."),
    );

    // -- Exec
    session.prompt("clean up", CancellationToken::new()).await?;

    // -- Check
    assert!(
        matches!(bash_outcome(&frontend)?, ToolCallOutcome::Err(e) if e == "User rejected tool call")
    );
    assert!(
        env.spawned().is_empty(),
        "denied command must not be spawned"
    );

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn bash_tool_kills_command_on_timeout() -> Result<()> {
    // -- Setup & Fixtures
    let mut env = InMemoryEnv::new();
    env.add_hanging_command("bash -c npm run dev", "listening on :3000\n");
    let frontend = RecordingFrontend::new();
    let mut session = Session::new(
        Arc::clone(&frontend),
        env.build_host(),
        bash_call(json!({ "command": "npm run dev", "timeout_seconds": 2 })),
        model_key(),
        PathBuf::from("."),
    );

    // -- Exec
    session
        .prompt("start the server", CancellationToken::new())
        .await?;

    // -- Check
    let output = bash_output(&frontend)?;
    assert_eq!(output["timed_out"], true);
    assert_eq!(output["signal"], "SIGKILL");
    assert_eq!(output["exit_code"], Value::Null);
    assert_eq!(output["output"], "listening on :3000\n");
    assert_eq!(env.kills(), 1);
    assert_eq!(env.live_terminals(), 0, "terminal should be released");

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn bash_tool_releases_terminal_when_cancelled() -> Result<()> {
    // -- Setup & Fixtures
    let mut env = InMemoryEnv::new();
    env.add_hanging_command("bash -c sleep 1000", "");
    let frontend = RecordingFrontend::new();
    let mut session = Session::new(
        Arc::clone(&frontend),
        env.build_host(),
        bash_call(json!({ "command": "sleep 1000" })),
        model_key(),
        PathBuf::from("."),
    );
    let cancel = CancellationToken::new();
    let canceller = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        canceller.cancel();
    });

    // -- Exec
    session.prompt("wait a while", cancel).await?;
    // The release runs on a task spawned when the tool call is dropped.
    tokio::task::yield_now().await;

    // -- Check
    let outcome = bash_outcome(&frontend)?;
    assert!(matches!(outcome, ToolCallOutcome::Cancelled), "{outcome:?}");
    assert_eq!(env.live_terminals(), 0, "terminal should be released");

    Ok(())
}

#[tokio::test]
async fn bash_tool_not_registered_without_terminal() -> Result<()> {
    // -- Setup & Fixtures
    let frontend = RecordingFrontend::new();
    let mut session = Session::new(
        Arc::clone(&frontend),
        Host::default(),
        bash_call(json!({ "command": "ls" })),
        model_key(),
        PathBuf::from("."),
    );

    // -- Exec
    session
        .prompt("list files", CancellationToken::new())
        .await?;

    // -- Check
    assert!(matches!(bash_outcome(&frontend)?, ToolCallOutcome::Unknown));

    Ok(())
}
