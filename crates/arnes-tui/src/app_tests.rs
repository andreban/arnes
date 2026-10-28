// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use serde_json::json;

use super::*;

fn png(data_len: usize) -> ToolResultContent {
    ToolResultContent::Media(Media::Inline {
        mime_type: "image/png".into(),
        data: "A".repeat(data_len),
    })
}

#[test]
fn single_json_block_renders_as_json() -> Result<()> {
    let outcome = ToolCallOutcome::Ok(vec![ToolResultContent::Json(json!({ "ok": true }))]);

    let RenderedOutcome::Ok(summary) = render_outcome(outcome) else {
        return Err("expected RenderedOutcome::Ok".into());
    };

    assert_eq!(summary, r#"{"ok":true}"#);
    Ok(())
}

#[test]
fn media_renders_as_type_and_size_placeholder() -> Result<()> {
    let outcome = ToolCallOutcome::Ok(vec![
        ToolResultContent::Json(json!("shot")),
        png(16_384),
        png(8),
    ]);

    let RenderedOutcome::Ok(summary) = render_outcome(outcome) else {
        return Err("expected RenderedOutcome::Ok".into());
    };

    assert_eq!(summary, r#""shot" [image/png, 12 KB] [image/png, 6 B]"#);
    Ok(())
}

#[test]
fn error_field_in_any_json_block_renders_as_error() -> Result<()> {
    let outcome = ToolCallOutcome::Ok(vec![
        png(8),
        ToolResultContent::Json(json!({ "error": "not found" })),
    ]);

    let RenderedOutcome::Err(msg) = render_outcome(outcome) else {
        return Err("expected RenderedOutcome::Err".into());
    };

    assert_eq!(msg, "not found");
    Ok(())
}

#[test]
fn denied_cancelled_and_unknown_keep_their_variants() -> Result<()> {
    assert!(matches!(
        render_outcome(ToolCallOutcome::Denied),
        RenderedOutcome::Denied
    ));
    assert!(matches!(
        render_outcome(ToolCallOutcome::Cancelled),
        RenderedOutcome::Cancelled
    ));
    assert!(matches!(
        render_outcome(ToolCallOutcome::Unknown),
        RenderedOutcome::Unknown
    ));
    Ok(())
}

fn command_request(command: &str) -> PermissionRequest {
    let args = json!({ "command": command });
    PermissionRequest {
        tool_call: std::sync::Arc::new(agent_rig::model::ToolCall::new(
            "call-1".into(),
            "bash".into(),
            args.clone(),
        )),
        kind: ToolKind::Execute,
        proposal: args,
    }
}

/// A command whose dangerous part sits past the inline summary's limit.
fn long_command() -> String {
    format!("echo {} ; rm -rf ~", "x".repeat(MAX_INLINE_JSON * 2))
}

#[test]
fn command_permission_previews_full_command() -> Result<()> {
    // -- Setup & Fixtures
    let command = long_command();
    let (responder, _rx) = oneshot::channel();

    // -- Exec
    let pending = PendingPermission::new(&command_request(&command), responder);

    // -- Check
    assert!(!pending.args.contains("rm -rf"), "summary is truncated");
    let preview = pending.preview.ok_or("expected a command preview")?;
    let body: Vec<String> = preview.body.iter().map(|line| line.to_string()).collect();
    assert_eq!(body.join("\n"), command);

    Ok(())
}

#[test]
fn command_permission_prompt_renders_command_tail() -> Result<()> {
    // -- Setup & Fixtures
    let command = long_command();
    let (responder, _rx) = oneshot::channel();
    let mut pending = PendingPermission::new(&command_request(&command), responder);
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(60, 20))?;

    // -- Exec
    terminal.draw(|f| render_permission_prompt(f, f.area(), &mut pending))?;

    // -- Check
    // Wrapping breaks the command across rows; rejoin the cells and drop the
    // popup border to recover the words in order.
    let buffer = terminal.backend().buffer();
    let text: String = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace('│', " ");
    let words: Vec<&str> = text.split_whitespace().collect();
    assert!(
        words.windows(3).any(|w| w == ["rm", "-rf", "~"]),
        "command tail missing from prompt: {text}"
    );

    Ok(())
}

#[test]
fn non_execute_request_has_no_command_preview() -> Result<()> {
    // -- Setup & Fixtures
    let mut request = command_request("ls");
    request.kind = ToolKind::Other;
    let (responder, _rx) = oneshot::channel();

    // -- Exec
    let pending = PendingPermission::new(&request, responder);

    // -- Check
    assert!(pending.preview.is_none());

    Ok(())
}
