// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewParams {
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewResult {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptParams {
    pub session_id: String,
    pub prompt: Vec<PromptContentBlock>,
}

#[derive(Debug, Deserialize)]
pub struct PromptContentBlock {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
}

impl SessionPromptParams {
    pub fn text(&self) -> String {
        self.prompt
            .iter()
            .filter(|b| b.kind == "text")
            .filter_map(|b| b.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptResult {
    pub stop_reason: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCancelParams {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateParams {
    pub session_id: String,
    pub update: SessionUpdate,
}

#[derive(Debug, Serialize)]
#[serde(tag = "sessionUpdate")]
pub enum SessionUpdate {
    #[serde(rename = "agent_message_chunk")]
    AgentMessageChunk {
        #[serde(rename = "messageId")]
        message_id: String,
        content: MessageContent,
    },
    #[serde(rename = "agent_thought_chunk")]
    AgentThoughtChunk {
        #[serde(rename = "messageId")]
        message_id: String,
        content: MessageContent,
    },
    /// LLM has requested a tool call; status is always "pending".
    #[serde(rename = "tool_call")]
    ToolCall {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        kind: &'static str,
        status: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "rawInput")]
        raw_input: Option<Value>,
    },
    /// Tool execution lifecycle update (in_progress or completed).
    #[serde(rename = "tool_call_update")]
    ToolCallUpdate {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        status: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ToolCallContent>>,
    },
}

#[derive(Debug, Serialize)]
pub struct MessageContent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

/// One block of a tool call's content. Tool output is a [`Content`] block; a
/// proposed file edit is a [`Diff`] block, which clients render as a
/// before/after view in the tool-call card and its approval prompt.
///
/// [`Content`]: ToolCallContent::Content
/// [`Diff`]: ToolCallContent::Diff
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ToolCallContent {
    #[serde(rename = "content")]
    Content { content: MessageContent },
    #[serde(rename = "diff")]
    Diff {
        path: String,
        /// The file's contents before the edit; `None` for a new file.
        #[serde(rename = "oldText", skip_serializing_if = "Option::is_none")]
        old_text: Option<String>,
        #[serde(rename = "newText")]
        new_text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins the on-the-wire JSON shape for ACP session/update notifications.
    // Any change to discriminator strings or field names is a protocol break;
    // updating these literals must be a deliberate decision.

    #[test]
    fn agent_message_chunk_wire_shape() {
        let update = SessionUpdateParams {
            session_id: "sess-1".into(),
            update: SessionUpdate::AgentMessageChunk {
                message_id: "msg-1".into(),
                content: MessageContent {
                    kind: "text",
                    text: "hello".into(),
                },
            },
        };
        assert_eq!(
            serde_json::to_string(&update).unwrap(),
            r#"{"sessionId":"sess-1","update":{"sessionUpdate":"agent_message_chunk","messageId":"msg-1","content":{"type":"text","text":"hello"}}}"#,
        );
    }

    #[test]
    fn agent_thought_chunk_wire_shape() {
        let update = SessionUpdateParams {
            session_id: "sess-1".into(),
            update: SessionUpdate::AgentThoughtChunk {
                message_id: "msg-1".into(),
                content: MessageContent {
                    kind: "text",
                    text: "thinking...".into(),
                },
            },
        };
        assert_eq!(
            serde_json::to_string(&update).unwrap(),
            r#"{"sessionId":"sess-1","update":{"sessionUpdate":"agent_thought_chunk","messageId":"msg-1","content":{"type":"text","text":"thinking..."}}}"#,
        );
    }

    #[test]
    fn tool_call_wire_shape() {
        let update = SessionUpdateParams {
            session_id: "sess-1".into(),
            update: SessionUpdate::ToolCall {
                tool_call_id: "tc-1".into(),
                title: Some("read_text_file".into()),
                kind: "tool_use",
                status: "pending",
                raw_input: None,
            },
        };
        assert_eq!(
            serde_json::to_string(&update).unwrap(),
            r#"{"sessionId":"sess-1","update":{"sessionUpdate":"tool_call","toolCallId":"tc-1","title":"read_text_file","kind":"tool_use","status":"pending"}}"#,
        );
    }

    #[test]
    fn tool_call_update_completed_wire_shape() {
        let update = SessionUpdateParams {
            session_id: "sess-1".into(),
            update: SessionUpdate::ToolCallUpdate {
                tool_call_id: "tc-1".into(),
                status: "completed",
                title: None,
                content: Some(vec![ToolCallContent::Content {
                    content: MessageContent {
                        kind: "text",
                        text: "file content".into(),
                    },
                }]),
            },
        };
        assert_eq!(
            serde_json::to_string(&update).unwrap(),
            r#"{"sessionId":"sess-1","update":{"sessionUpdate":"tool_call_update","toolCallId":"tc-1","status":"completed","content":[{"type":"content","content":{"type":"text","text":"file content"}}]}}"#,
        );
    }

    #[test]
    fn tool_call_update_diff_wire_shape() {
        let update = SessionUpdateParams {
            session_id: "sess-1".into(),
            update: SessionUpdate::ToolCallUpdate {
                tool_call_id: "tc-1".into(),
                status: "in_progress",
                title: None,
                content: Some(vec![ToolCallContent::Diff {
                    path: "/home/user/f.txt".into(),
                    old_text: Some("hello world".into()),
                    new_text: "hi world".into(),
                }]),
            },
        };
        assert_eq!(
            serde_json::to_string(&update).unwrap(),
            r#"{"sessionId":"sess-1","update":{"sessionUpdate":"tool_call_update","toolCallId":"tc-1","status":"in_progress","content":[{"type":"diff","path":"/home/user/f.txt","oldText":"hello world","newText":"hi world"}]}}"#,
        );
    }

    #[test]
    fn tool_call_update_in_progress_wire_shape() {
        let update = SessionUpdateParams {
            session_id: "sess-1".into(),
            update: SessionUpdate::ToolCallUpdate {
                tool_call_id: "tc-1".into(),
                status: "in_progress",
                title: None,
                content: None,
            },
        };
        assert_eq!(
            serde_json::to_string(&update).unwrap(),
            r#"{"sessionId":"sess-1","update":{"sessionUpdate":"tool_call_update","toolCallId":"tc-1","status":"in_progress"}}"#,
        );
    }
}
