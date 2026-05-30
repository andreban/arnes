// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Message + content-block vocabulary.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A message in conversation history.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    User { content: Vec<ContentBlock> },
    Assistant { content: Vec<ContentBlock> },
    System { content: String },
}

/// One part of a message's content.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    /// Model reasoning, distinct from response text. Frontends choose
    /// whether to display.
    Thinking {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Value,
        is_error: bool,
    },
}

impl Message {
    /// Convenience constructor for a single-text-block user message.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::User {
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}
