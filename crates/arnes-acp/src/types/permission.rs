// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use arnes_core::ToolKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
/// `rawInput` carries the tool's argument object so the client can render what
/// the call will act on (e.g. the path a file read targets). `kind` and
/// `locations` enrich that further: a semantic category for the icon/label and
/// the file paths the call touches, which the client can show and follow.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionToolCall {
    pub tool_call_id: String,
    pub title: String,
    pub raw_input: Value,
    /// ACP `ToolKind`. Omitted for the `other` default the client assumes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
}

/// Maps a core [`ToolKind`] to its ACP wire string, returning `None` for
/// [`ToolKind::Other`] so the prompt omits a redundant `"other"` (the client's
/// own default).
pub fn acp_kind(kind: ToolKind) -> Option<&'static str> {
    match kind {
        ToolKind::Read => Some("read"),
        ToolKind::Edit => Some("edit"),
        ToolKind::Delete => Some("delete"),
        ToolKind::Move => Some("move"),
        ToolKind::Search => Some("search"),
        ToolKind::Execute => Some("execute"),
        ToolKind::Think => Some("think"),
        ToolKind::Fetch => Some("fetch"),
        ToolKind::Other => None,
    }
}

/// ACP `PermissionOptionKind`. All four kinds are modelled even though
/// [`default_options`] only offers the two arnes can currently honor.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

/// One choice offered to the user.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: &'static str,
    pub name: &'static str,
    pub kind: PermissionOptionKind,
}

/// The two choices arnes offers, mirroring the TUI's allow-once / deny. The
/// `*_always` kinds aren't offered yet: `arnes_core::Permission` has no
/// persistent verdict to honor them with.
pub fn default_options() -> Vec<PermissionOption> {
    vec![
        PermissionOption {
            option_id: ALLOW_ONCE,
            name: "Allow",
            kind: PermissionOptionKind::AllowOnce,
        },
        PermissionOption {
            option_id: REJECT_ONCE,
            name: "Reject",
            kind: PermissionOptionKind::RejectOnce,
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
                raw_input: serde_json::json!({ "path": "Cargo.toml" }),
                kind: Some("read"),
            },
            options: default_options(),
        };
        assert_eq!(
            serde_json::to_string(&params).unwrap(),
            r#"{"sessionId":"sess-1","toolCall":{"toolCallId":"perm-1","title":"read_text_file","rawInput":{"path":"Cargo.toml"},"kind":"read"},"options":[{"optionId":"allow-once","name":"Allow","kind":"allow_once"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]}"#,
        );
    }

    #[test]
    fn kind_and_locations_omitted_when_absent() {
        // An `other`-kind tool with no file locations drops both fields rather
        // than sending a redundant `"other"` and an empty array.
        let tool_call = PermissionToolCall {
            tool_call_id: "perm-1".into(),
            title: "some_tool".into(),
            raw_input: serde_json::json!({}),
            kind: acp_kind(ToolKind::Other),
        };
        assert_eq!(
            serde_json::to_string(&tool_call).unwrap(),
            r#"{"toolCallId":"perm-1","title":"some_tool","rawInput":{}}"#,
        );
    }

    #[test]
    fn acp_kind_maps_known_kinds() {
        assert_eq!(acp_kind(ToolKind::Read), Some("read"));
        assert_eq!(acp_kind(ToolKind::Execute), Some("execute"));
        assert_eq!(acp_kind(ToolKind::Other), None);
    }

    #[test]
    fn option_kind_wire_strings() {
        for (kind, expected) in [
            (PermissionOptionKind::AllowOnce, r#""allow_once""#),
            (PermissionOptionKind::AllowAlways, r#""allow_always""#),
            (PermissionOptionKind::RejectOnce, r#""reject_once""#),
            (PermissionOptionKind::RejectAlways, r#""reject_always""#),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), expected);
        }
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
