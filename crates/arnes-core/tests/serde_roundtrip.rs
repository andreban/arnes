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
