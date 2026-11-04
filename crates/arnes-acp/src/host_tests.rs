// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use super::*;
use serde_json::json;
use std::path::PathBuf;

/// An `AcpTerminal` wired to a channel the test reads client requests from.
fn terminal() -> (
    Arc<AcpTerminal>,
    mpsc::UnboundedReceiver<String>,
    PendingRequests,
) {
    let (notify_tx, notify_rx) = mpsc::unbounded_channel();
    let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
    let terminal = Arc::new(AcpTerminal::new(
        "sess-1".into(),
        notify_tx,
        Arc::clone(&pending),
    ));
    (terminal, notify_rx, pending)
}

/// Receives the next outbound request and answers it with `response`.
async fn answer(
    rx: &mut mpsc::UnboundedReceiver<String>,
    pending: &PendingRequests,
    response: core::result::Result<Value, String>,
) -> Result<Value> {
    let raw = rx.recv().await.ok_or("channel closed")?;
    let req: Value = serde_json::from_str(&raw)?;
    let id = req["id"].as_str().ok_or("request missing id")?.to_owned();
    let tx = pending
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or("request not pending")?;
    let _ = tx.send(response);
    Ok(req)
}

/// Creates a terminal, answering `terminal/create` with `client-term`.
async fn create(
    terminal: &Arc<AcpTerminal>,
    rx: &mut mpsc::UnboundedReceiver<String>,
    pending: &PendingRequests,
    spec: TerminalSpec,
) -> Result<(TerminalHandle, Value)> {
    let t = Arc::clone(terminal);
    let task = tokio::spawn(async move { t.terminal_create(spec).await });
    let req = answer(rx, pending, Ok(json!({ "terminalId": "client-term" }))).await?;
    Ok((task.await??, req))
}

#[tokio::test]
async fn test_terminal_create_sends_spec_on_the_wire() -> Result<()> {
    // -- Setup & Fixtures
    let (terminal, mut rx, pending) = terminal();
    let spec = TerminalSpec {
        command: "bash".into(),
        args: vec!["-c".into(), "echo hi".into()],
        cwd: Some(PathBuf::from("/work")),
        env: HashMap::from([("B".into(), "2".into()), ("A".into(), "1".into())]),
        output_byte_limit: Some(1024),
    };

    // -- Exec
    let (_handle, req) = create(&terminal, &mut rx, &pending, spec).await?;

    // -- Check
    assert_eq!(req["method"], "terminal/create");
    assert_eq!(
        req["params"],
        json!({
            "sessionId": "sess-1",
            "command": "bash",
            "args": ["-c", "echo hi"],
            "env": [{ "name": "A", "value": "1" }, { "name": "B", "value": "2" }],
            "cwd": "/work",
            "outputByteLimit": 1024,
        })
    );
    Ok(())
}

#[tokio::test]
async fn test_terminal_create_omits_empty_optional_fields() -> Result<()> {
    // -- Setup & Fixtures
    let (terminal, mut rx, pending) = terminal();
    let spec = TerminalSpec {
        command: "ls".into(),
        ..Default::default()
    };

    // -- Exec
    let (_handle, req) = create(&terminal, &mut rx, &pending, spec).await?;

    // -- Check
    assert_eq!(
        req["params"],
        json!({ "sessionId": "sess-1", "command": "ls" })
    );
    Ok(())
}

#[tokio::test]
async fn test_terminal_methods_address_the_client_terminal_id() -> Result<()> {
    // -- Setup & Fixtures
    let (terminal, mut rx, pending) = terminal();
    let (handle, _) = create(&terminal, &mut rx, &pending, TerminalSpec::default()).await?;
    let expected_params = json!({ "sessionId": "sess-1", "terminalId": "client-term" });

    // -- Exec & Check: wait_for_exit
    let t = Arc::clone(&terminal);
    let h = handle.clone();
    let task = tokio::spawn(async move { t.terminal_wait_for_exit(&h).await });
    let req = answer(
        &mut rx,
        &pending,
        Ok(json!({ "exitCode": 3, "signal": null })),
    )
    .await?;
    assert_eq!(req["method"], "terminal/wait_for_exit");
    assert_eq!(req["params"], expected_params);
    assert_eq!(
        task.await??,
        TerminalExitStatus {
            exit_code: Some(3),
            signal: None
        }
    );

    // -- Exec & Check: output
    let t = Arc::clone(&terminal);
    let h = handle.clone();
    let task = tokio::spawn(async move { t.terminal_snapshot(&h).await });
    let req = answer(
        &mut rx,
        &pending,
        Ok(json!({
            "output": "hi\n",
            "truncated": true,
            "exitStatus": { "exitCode": null, "signal": "SIGKILL" },
        })),
    )
    .await?;
    assert_eq!(req["method"], "terminal/output");
    assert_eq!(req["params"], expected_params);
    let snapshot = task.await??;
    assert_eq!(snapshot.output, "hi\n");
    assert!(snapshot.truncated);
    assert_eq!(
        snapshot.exit_status,
        Some(TerminalExitStatus {
            exit_code: None,
            signal: Some("SIGKILL".into())
        })
    );

    // -- Exec & Check: kill
    let t = Arc::clone(&terminal);
    let h = handle.clone();
    let task = tokio::spawn(async move { t.terminal_kill(&h).await });
    let req = answer(&mut rx, &pending, Ok(json!({}))).await?;
    assert_eq!(req["method"], "terminal/kill");
    assert_eq!(req["params"], expected_params);
    task.await??;

    // -- Exec & Check: release
    let t = Arc::clone(&terminal);
    let h = handle.clone();
    let task = tokio::spawn(async move { t.terminal_release(h).await });
    let req = answer(&mut rx, &pending, Ok(json!(null))).await?;
    assert_eq!(req["method"], "terminal/release");
    assert_eq!(req["params"], expected_params);
    task.await??;

    Ok(())
}

#[tokio::test]
async fn test_terminal_output_while_running_has_no_exit_status() -> Result<()> {
    // -- Setup & Fixtures
    let (terminal, mut rx, pending) = terminal();
    let (handle, _) = create(&terminal, &mut rx, &pending, TerminalSpec::default()).await?;

    // -- Exec
    let t = Arc::clone(&terminal);
    let task = tokio::spawn(async move { t.terminal_snapshot(&handle).await });
    answer(&mut rx, &pending, Ok(json!({ "output": "" }))).await?;

    // -- Check
    let snapshot = task.await??;
    assert!(!snapshot.truncated);
    assert_eq!(snapshot.exit_status, None);
    Ok(())
}

#[tokio::test]
async fn test_released_handle_is_unknown() -> Result<()> {
    // -- Setup & Fixtures
    let (terminal, mut rx, pending) = terminal();
    let (handle, _) = create(&terminal, &mut rx, &pending, TerminalSpec::default()).await?;
    let t = Arc::clone(&terminal);
    let h = handle.clone();
    let task = tokio::spawn(async move { t.terminal_release(h).await });
    answer(&mut rx, &pending, Ok(json!({}))).await?;
    task.await??;

    // -- Exec
    let err = terminal.terminal_kill(&handle).await.unwrap_err();

    // -- Check
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    assert!(rx.try_recv().is_err(), "no request should reach the client");
    Ok(())
}

#[tokio::test]
async fn test_client_error_becomes_io_error() -> Result<()> {
    // -- Setup & Fixtures
    let (terminal, mut rx, pending) = terminal();
    let t = Arc::clone(&terminal);
    let task = tokio::spawn(async move { t.terminal_create(TerminalSpec::default()).await });

    // -- Exec
    answer(&mut rx, &pending, Err("spawn failed".into())).await?;

    // -- Check
    let err = task.await?.unwrap_err();
    assert_eq!(err.to_string(), "spawn failed");
    Ok(())
}

#[tokio::test]
async fn test_malformed_create_result_is_invalid_data() -> Result<()> {
    // -- Setup & Fixtures
    let (terminal, mut rx, pending) = terminal();
    let t = Arc::clone(&terminal);
    let task = tokio::spawn(async move { t.terminal_create(TerminalSpec::default()).await });

    // -- Exec
    answer(&mut rx, &pending, Ok(json!({}))).await?;

    // -- Check
    let err = task.await?.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    Ok(())
}

#[tokio::test]
async fn test_wait_for_exit_has_no_request_timeout() -> Result<()> {
    // -- Setup & Fixtures
    let (terminal, mut rx, pending) = terminal();
    let (handle, _) = create(&terminal, &mut rx, &pending, TerminalSpec::default()).await?;
    tokio::time::pause();

    // -- Exec
    let t = Arc::clone(&terminal);
    let task = tokio::spawn(async move { t.terminal_wait_for_exit(&handle).await });
    let raw = rx.recv().await.ok_or("channel closed")?;
    tokio::time::advance(REQUEST_TIMEOUT * 4).await;

    // -- Check
    assert!(!task.is_finished(), "wait_for_exit should still be waiting");
    let req: Value = serde_json::from_str(&raw)?;
    let id = req["id"].as_str().ok_or("missing id")?;
    let tx = pending.lock().unwrap().remove(id).ok_or("not pending")?;
    let _ = tx.send(Ok(json!({ "exitCode": 0 })));
    assert_eq!(task.await??.exit_code, Some(0));
    Ok(())
}
