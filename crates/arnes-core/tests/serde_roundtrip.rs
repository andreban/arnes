// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use arnes_core::{ContentBlock, Message};

#[test]
fn message_user_text_roundtrip() {
    let m = Message::User {
        content: vec![
            ContentBlock::Text { text: "hi".into() },
            ContentBlock::Thinking {
                text: "let me think".into(),
            },
        ],
    };
    let json = serde_json::to_string(&m).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    match back {
        Message::User { content } => {
            assert_eq!(content.len(), 2);
            assert!(matches!(content[0], ContentBlock::Text { .. }));
            assert!(matches!(content[1], ContentBlock::Thinking { .. }));
        }
        _ => panic!("expected User variant"),
    }
}

// Pins the on-the-wire JSON shape. Any change to the serde tagging or
// variant names is a wire-format break that touches every consumer of
// persisted or transmitted messages; updating these literal strings
// has to be a deliberate decision.
#[test]
fn message_wire_shape_is_pinned() {
    let m = Message::User {
        content: vec![ContentBlock::Text { text: "hi".into() }],
    };
    let json = serde_json::to_string(&m).unwrap();
    assert_eq!(
        json,
        r#"{"role":"user","content":[{"type":"text","text":"hi"}]}"#,
    );
}

#[test]
fn system_message_is_plain_string() {
    let m = Message::System {
        content: "be helpful".into(),
    };
    let json = serde_json::to_string(&m).unwrap();
    assert_eq!(json, r#"{"role":"system","content":"be helpful"}"#);
}

#[test]
fn message_assistant_tool_use_roundtrip() {
    let m = Message::Assistant {
        content: vec![
            ContentBlock::Thinking {
                text: "thinking about calling a tool".into(),
            },
            ContentBlock::ToolUse {
                id: "call_123".into(),
                name: "read_text_file".into(),
                input: serde_json::json!({ "path": "Cargo.toml" }),
            },
        ],
    };
    let json = serde_json::to_string(&m).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    match back {
        Message::Assistant { content } => {
            assert_eq!(content.len(), 2);
            assert!(matches!(content[0], ContentBlock::Thinking { .. }));
            if let ContentBlock::ToolUse { id, name, input } = &content[1] {
                assert_eq!(id, "call_123");
                assert_eq!(name, "read_text_file");
                assert_eq!(input, &serde_json::json!({ "path": "Cargo.toml" }));
            } else {
                panic!("expected ToolUse block");
            }
        }
        _ => panic!("expected Assistant variant"),
    }
}

#[test]
fn message_user_tool_result_roundtrip() {
    let m = Message::User {
        content: vec![ContentBlock::ToolResult {
            tool_use_id: "call_123".into(),
            content: serde_json::json!({ "content": "file contents" }),
            is_error: false,
        }],
    };
    let json = serde_json::to_string(&m).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    match back {
        Message::User { content } => {
            assert_eq!(content.len(), 1);
            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } = &content[0]
            {
                assert_eq!(tool_use_id, "call_123");
                assert_eq!(content, &serde_json::json!({ "content": "file contents" }));
                assert!(!is_error);
            } else {
                panic!("expected ToolResult block");
            }
        }
        _ => panic!("expected User variant"),
    }
}

#[test]
fn message_tool_use_wire_shape_is_pinned() {
    let m = Message::Assistant {
        content: vec![ContentBlock::ToolUse {
            id: "call_123".into(),
            name: "read_text_file".into(),
            input: serde_json::json!({ "path": "Cargo.toml" }),
        }],
    };
    let json = serde_json::to_string(&m).unwrap();
    assert_eq!(
        json,
        r#"{"role":"assistant","content":[{"type":"tool_use","id":"call_123","name":"read_text_file","input":{"path":"Cargo.toml"}}]}"#,
    );
}

#[test]
fn message_tool_result_wire_shape_is_pinned() {
    let m = Message::User {
        content: vec![ContentBlock::ToolResult {
            tool_use_id: "call_123".into(),
            content: serde_json::json!({ "content": "file contents" }),
            is_error: false,
        }],
    };
    let json = serde_json::to_string(&m).unwrap();
    assert_eq!(
        json,
        r#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_123","content":{"content":"file contents"},"is_error":false}]}"#,
    );
}
