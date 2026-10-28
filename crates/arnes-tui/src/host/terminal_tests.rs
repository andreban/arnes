// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use super::*;

/// Runs `script` through the platform shell.
fn shell(script: &str) -> TerminalSpec {
    let (command, flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    TerminalSpec {
        command: command.into(),
        args: vec![flag.into(), script.into()],
        ..Default::default()
    }
}

fn long_running() -> TerminalSpec {
    if cfg!(windows) {
        shell("ping -n 60 127.0.0.1 > nul")
    } else {
        shell("sleep 60")
    }
}

#[tokio::test]
async fn test_terminal_captures_stdout_and_stderr() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();

    // -- Exec
    let handle = terminal
        .terminal_create(shell("echo out&& echo err 1>&2"))
        .await?;
    let status = terminal.terminal_wait_for_exit(&handle).await?;
    let snapshot = terminal.terminal_snapshot(&handle).await?;

    // -- Check
    assert_eq!(status.exit_code, Some(0));
    assert_eq!(snapshot.exit_status, Some(status));
    assert!(snapshot.output.contains("out"), "{:?}", snapshot.output);
    assert!(snapshot.output.contains("err"), "{:?}", snapshot.output);
    assert!(!snapshot.truncated);

    Ok(())
}

#[tokio::test]
async fn test_terminal_reports_nonzero_exit_code() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();

    // -- Exec
    let handle = terminal.terminal_create(shell("exit 3")).await?;
    let status = terminal.terminal_wait_for_exit(&handle).await?;

    // -- Check
    assert_eq!(status.exit_code, Some(3));
    assert_eq!(status.signal, None);

    Ok(())
}

#[tokio::test]
async fn test_terminal_applies_cwd_and_env() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();
    let dir = std::env::temp_dir().canonicalize()?;
    let script = if cfg!(windows) {
        "echo %ARNES_TEST_VAR%&& cd"
    } else {
        "echo $ARNES_TEST_VAR && pwd -P"
    };
    let spec = TerminalSpec {
        cwd: Some(dir.clone()),
        env: HashMap::from([("ARNES_TEST_VAR".into(), "marker-value".into())]),
        ..shell(script)
    };

    // -- Exec
    let handle = terminal.terminal_create(spec).await?;
    terminal.terminal_wait_for_exit(&handle).await?;
    let snapshot = terminal.terminal_snapshot(&handle).await?;

    // -- Check
    assert!(
        snapshot.output.contains("marker-value"),
        "{:?}",
        snapshot.output
    );
    let last_line = snapshot.output.lines().last().unwrap_or_default().trim();
    assert_eq!(std::path::Path::new(last_line).canonicalize()?, dir);

    Ok(())
}

#[tokio::test]
async fn test_terminal_keeps_most_recent_output_when_over_limit() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();
    let spec = TerminalSpec {
        output_byte_limit: Some(8),
        ..shell("echo first&& echo last")
    };

    // -- Exec
    let handle = terminal.terminal_create(spec).await?;
    terminal.terminal_wait_for_exit(&handle).await?;
    let snapshot = terminal.terminal_snapshot(&handle).await?;

    // -- Check
    assert!(snapshot.truncated);
    assert!(snapshot.output.len() <= 8, "{:?}", snapshot.output);
    assert!(
        snapshot.output.trim_end().ends_with("last"),
        "{:?}",
        snapshot.output
    );
    assert!(!snapshot.output.contains("first"), "{:?}", snapshot.output);

    Ok(())
}

#[tokio::test]
async fn test_terminal_kill_stops_process_and_keeps_handle() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();
    let handle = terminal.terminal_create(long_running()).await?;
    assert_eq!(terminal.terminal_snapshot(&handle).await?.exit_status, None);

    // -- Exec
    terminal.terminal_kill(&handle).await?;
    let status = tokio::time::timeout(
        Duration::from_secs(10),
        terminal.terminal_wait_for_exit(&handle),
    )
    .await??;

    // -- Check
    assert_ne!(status.exit_code, Some(0));
    #[cfg(unix)]
    assert_eq!(status.signal.as_deref(), Some("SIGKILL"));
    let snapshot = terminal.terminal_snapshot(&handle).await?;
    assert_eq!(snapshot.exit_status, Some(status));

    Ok(())
}

#[tokio::test]
async fn test_terminal_release_invalidates_handle() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();
    let handle = terminal.terminal_create(long_running()).await?;
    let mut exit = terminal.process(&handle, |process| process.exit.clone())?;

    // -- Exec
    terminal.terminal_release(handle.clone()).await?;

    // -- Check
    let err = terminal.terminal_snapshot(&handle).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    let err = terminal.terminal_release(handle).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    // Releasing a running terminal kills it.
    tokio::time::timeout(Duration::from_secs(10), exit.wait_for(Option::is_some)).await??;

    Ok(())
}

#[tokio::test]
async fn test_terminal_create_reports_spawn_failure() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();
    let spec = TerminalSpec {
        command: "arnes-definitely-not-a-real-command".into(),
        ..Default::default()
    };

    // -- Exec
    let err = terminal.terminal_create(spec).await.unwrap_err();

    // -- Check
    assert_eq!(err.kind(), io::ErrorKind::NotFound);

    Ok(())
}

#[test]
fn test_output_buffer_skips_split_utf8_sequence() -> Result<()> {
    // -- Setup & Fixtures
    let mut buffer = OutputBuffer::new(4);

    // -- Exec
    // "é" is two bytes; a 4-byte window over "aé€" (1 + 2 + 3 bytes) starts
    // on the second byte of "é".
    buffer.push("aé€".as_bytes());
    let (output, truncated) = buffer.contents();

    // -- Check
    assert!(truncated);
    assert_eq!(output, "€");

    Ok(())
}

/// A fresh path for a marker file that a background job writes if it
/// survives long enough.
#[cfg(unix)]
fn marker_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("arnes-{name}-{}", Uuid::now_v7()))
}

/// Starts a background grandchild that writes `marker` after one second.
#[cfg(unix)]
fn background_writer(marker: &std::path::Path, then: &str) -> TerminalSpec {
    shell(&format!(
        "(sleep 1; echo alive > '{}') & {then}",
        marker.display()
    ))
}

#[cfg(unix)]
#[tokio::test]
async fn test_terminal_kill_stops_background_grandchild() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();
    let marker = marker_path("kill");
    let handle = terminal
        .terminal_create(background_writer(&marker, "echo started; wait"))
        .await?;
    // Kill only once the shell has forked the job, or there is nothing to leak.
    tokio::time::timeout(Duration::from_secs(10), async {
        while !terminal
            .terminal_snapshot(&handle)
            .await
            .is_ok_and(|snapshot| snapshot.output.contains("started"))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;

    // -- Exec
    terminal.terminal_kill(&handle).await?;
    tokio::time::timeout(
        Duration::from_secs(10),
        terminal.terminal_wait_for_exit(&handle),
    )
    .await??;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // -- Check
    let survived = marker.exists();
    let _ = std::fs::remove_file(&marker);
    assert!(!survived, "background grandchild outlived terminal_kill");

    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_terminal_release_stops_background_job_after_shell_exits() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();
    let marker = marker_path("release");
    let handle = terminal
        .terminal_create(background_writer(&marker, "exit 0"))
        .await?;
    let status = terminal.terminal_wait_for_exit(&handle).await?;
    assert_eq!(status.exit_code, Some(0));

    // -- Exec
    terminal.terminal_release(handle).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // -- Check
    let survived = marker.exists();
    let _ = std::fs::remove_file(&marker);
    assert!(!survived, "background job outlived terminal_release");

    Ok(())
}

#[tokio::test]
async fn test_terminal_wait_is_not_held_by_background_output() -> Result<()> {
    // -- Setup & Fixtures
    let terminal = LocalTerminal::default();
    let spec = if cfg!(windows) {
        shell("start /b ping -n 30 127.0.0.1")
    } else {
        shell("sleep 30 &")
    };
    let started = Instant::now();

    // -- Exec
    let handle = terminal.terminal_create(spec).await?;
    terminal.terminal_wait_for_exit(&handle).await?;

    // -- Check
    // A pipe held open by the background job must not delay the exit
    // status until the job ends.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "wait_for_exit took {:?}",
        started.elapsed()
    );
    terminal.terminal_release(handle).await?;

    Ok(())
}
