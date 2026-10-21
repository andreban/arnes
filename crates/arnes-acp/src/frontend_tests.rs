// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

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
