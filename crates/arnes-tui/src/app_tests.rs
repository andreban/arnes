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
fn denied_and_unknown_keep_their_variants() -> Result<()> {
    assert!(matches!(
        render_outcome(ToolCallOutcome::Denied),
        RenderedOutcome::Denied
    ));
    assert!(matches!(
        render_outcome(ToolCallOutcome::Unknown),
        RenderedOutcome::Unknown
    ));
    Ok(())
}
