// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use agent_rig::model::{Media, ToolResultContent};
use serde_json::json;

use super::*;

/// What the model receives for `outcome`, after agent-rig wraps the result.
fn sent_to_model(outcome: ToolCallOutcome) -> Vec<ToolResultContent> {
    ToolResult::from(outcome).into()
}

#[test]
fn ok_blocks_reach_the_model_as_success() -> Result<()> {
    let image = Media::Inline {
        mime_type: "image/png".into(),
        data: "iVBORw0KGgo=".into(),
    };
    let outcome = ToolCallOutcome::Ok(vec![
        ToolResultContent::Json(json!({ "content": "hi" })),
        ToolResultContent::Media(image.clone()),
    ]);

    let sent = sent_to_model(outcome);

    assert_eq!(
        sent,
        vec![
            ToolResultContent::Json(json!({ "success": { "content": "hi" } })),
            ToolResultContent::Media(image),
        ]
    );
    Ok(())
}

#[test]
fn non_ok_outcomes_reach_the_model_as_errors() -> Result<()> {
    let cases = [
        (
            ToolCallOutcome::Err("disk full".into()),
            "Tool call error: disk full",
        ),
        (ToolCallOutcome::Denied, "User rejected the tool call"),
        (ToolCallOutcome::Cancelled, "Tool call cancelled"),
        (ToolCallOutcome::Unknown, "Unknown tool"),
    ];

    for (outcome, expected) in cases {
        let sent = sent_to_model(outcome);

        assert_eq!(
            sent,
            vec![ToolResultContent::Json(json!({ "error": expected }))]
        );
    }
    Ok(())
}
