// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use std::collections::HashMap;

use agent_rig::model::ToolCall;
use arnes_core::ToolKind;
use serde_json::json;

use super::*;

fn content_json(outcome: ToolCallOutcome) -> Result<Value> {
    Ok(serde_json::to_value(outcome_content(outcome))?)
}

#[test]
fn json_block_shows_its_content_field() -> Result<()> {
    let outcome = ToolCallOutcome::Ok(vec![ToolResultContent::Json(
        json!({ "content": "fn main() {}" }),
    )]);

    let content = content_json(outcome)?;

    assert_eq!(
        content,
        json!([{ "type": "content", "content": { "type": "text", "text": "fn main() {}" } }])
    );
    Ok(())
}

#[test]
fn inline_media_becomes_image_content() -> Result<()> {
    let outcome = ToolCallOutcome::Ok(vec![ToolResultContent::Media(Media::Inline {
        mime_type: "image/png".into(),
        data: "iVBORw0KGgo=".into(),
    })]);

    let content = content_json(outcome)?;

    assert_eq!(
        content,
        json!([{
            "type": "content",
            "content": { "type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png" },
        }])
    );
    Ok(())
}

#[test]
fn non_image_media_falls_back_to_text() -> Result<()> {
    let outcome = ToolCallOutcome::Ok(vec![ToolResultContent::Media(Media::Inline {
        mime_type: "audio/wav".into(),
        data: "UklGRg==".into(),
    })]);

    let content = content_json(outcome)?;

    assert_eq!(
        content,
        json!([{
            "type": "content",
            "content": { "type": "text", "text": "unsupported media: audio/wav" },
        }])
    );
    Ok(())
}

#[test]
fn each_block_gets_its_own_entry_in_order() -> Result<()> {
    let outcome = ToolCallOutcome::Ok(vec![
        ToolResultContent::Json(json!({ "width": 2 })),
        ToolResultContent::Media(Media::Inline {
            mime_type: "image/png".into(),
            data: "iVBORw0KGgo=".into(),
        }),
    ]);

    let content = content_json(outcome)?;

    assert_eq!(content[0]["content"]["text"], r#"{"width":2}"#);
    assert_eq!(content[1]["content"]["type"], "image");
    Ok(())
}

#[test]
fn non_ok_outcomes_keep_their_text() -> Result<()> {
    let cases = [
        (ToolCallOutcome::Err("boom".into()), "boom"),
        (ToolCallOutcome::Denied, "denied"),
        (ToolCallOutcome::Unknown, "unknown"),
    ];

    for (outcome, expected) in cases {
        let content = content_json(outcome)?;

        assert_eq!(
            content,
            json!([{ "type": "content", "content": { "type": "text", "text": expected } }])
        );
    }
    Ok(())
}

#[tokio::test]
async fn dropping_in_flight_permission_request_removes_pending_entry() -> Result<()> {
    let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
    let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
    let frontend = AcpFrontend::new("sess-1".into(), notify_tx, Arc::clone(&pending));
    let req = PermissionRequest {
        tool_call: Arc::new(ToolCall::new(
            "call-1".into(),
            "read_text_file".into(),
            json!({ "path": "Cargo.toml" }),
        )),
        kind: ToolKind::Read,
        proposal: json!({ "path": "Cargo.toml" }),
    };

    let mut perm_fut = Box::pin(frontend.request_permission(req));
    let raw = tokio::select! {
        res = notify_rx.recv() => res.ok_or("notify channel closed before request was sent")?,
        _ = &mut perm_fut => return Err("request_permission resolved without a client reply".into()),
    };
    let outbound: Value = serde_json::from_str(&raw)?;
    assert_eq!(outbound["method"], "session/request_permission");
    assert_eq!(pending.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);

    drop(perm_fut);

    assert!(pending.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    Ok(())
}
