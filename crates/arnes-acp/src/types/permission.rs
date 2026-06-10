// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

/// Option id arnes sends for "allow this call once".
pub const ALLOW_ONCE: &str = "allow-once";
/// Option id arnes sends for "reject this call once".
pub const REJECT_ONCE: &str = "reject-once";

/// Params for the `session/request_permission` request sent to the ACP client.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionParams {
    pub session_id: String,
    pub tool_call: PermissionToolCall,
    pub options: Vec<PermissionOption>,
}

/// The tool call the prompt is about. A minimal `ToolCallUpdate`: the spec
/// only requires `toolCallId`, and `title` gives the client something to show.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionToolCall {
    pub tool_call_id: String,
    pub title: String,
}

/// One choice offered to the user. `kind` is an ACP `PermissionOptionKind`
/// (`allow_once`, `allow_always`, `reject_once`, `reject_always`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
}

/// The two choices arnes offers, mirroring the TUI's allow-once / deny. The
/// `*_always` kinds are omitted because `arnes_core::Permission` has no
/// persistent verdict to map them onto yet.
pub fn default_options() -> Vec<PermissionOption> {
    vec![
        PermissionOption {
            option_id: ALLOW_ONCE,
            name: "Allow",
            kind: "allow_once",
        },
        PermissionOption {
            option_id: REJECT_ONCE,
            name: "Reject",
            kind: "reject_once",
        },
    ]
}

/// Result of `session/request_permission` returned by the client.
#[derive(Debug, Deserialize)]
pub struct RequestPermissionResult {
    pub outcome: PermissionOutcome,
}

/// The user's answer, or a cancellation if the turn ended first.
#[derive(Debug, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PermissionOutcome {
    Selected {
        #[serde(rename = "optionId")]
        option_id: String,
    },
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins the on-the-wire JSON shape for the session/request_permission
    // round trip. Field names and discriminator strings are protocol; a
    // change here must be deliberate.

    #[test]
    fn request_params_wire_shape() {
        let params = RequestPermissionParams {
            session_id: "sess-1".into(),
            tool_call: PermissionToolCall {
                tool_call_id: "perm-1".into(),
                title: "read_text_file".into(),
            },
            options: default_options(),
        };
        assert_eq!(
            serde_json::to_string(&params).unwrap(),
            r#"{"sessionId":"sess-1","toolCall":{"toolCallId":"perm-1","title":"read_text_file"},"options":[{"optionId":"allow-once","name":"Allow","kind":"allow_once"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]}"#,
        );
    }

    #[test]
    fn selected_outcome_deserializes() {
        let value =
            serde_json::json!({ "outcome": { "outcome": "selected", "optionId": "allow-once" } });
        let result: RequestPermissionResult = serde_json::from_value(value).unwrap();
        match result.outcome {
            PermissionOutcome::Selected { option_id } => assert_eq!(option_id, ALLOW_ONCE),
            other => panic!("expected selected, got {other:?}"),
        }
    }

    #[test]
    fn cancelled_outcome_deserializes() {
        let value = serde_json::json!({ "outcome": { "outcome": "cancelled" } });
        let result: RequestPermissionResult = serde_json::from_value(value).unwrap();
        assert!(matches!(result.outcome, PermissionOutcome::Cancelled));
    }
}
