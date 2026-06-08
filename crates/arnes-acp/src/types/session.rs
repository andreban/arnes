// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewParams {
    #[allow(dead_code)]
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
}

#[derive(Debug, Serialize)]
pub struct MessageContent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
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
}
