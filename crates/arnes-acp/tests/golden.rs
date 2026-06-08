// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod support;

use std::sync::Arc;

use arnes_acp::handler::Handler;
use arnes_core::ModelKey;
use serde_json::{Value, json};
use support::ScriptedLlm;
use tokio::sync::mpsc;

#[tokio::test]
async fn m1_basic_golden() {
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
    let llm = Arc::new(ScriptedLlm::new(vec!["Hello!".into()]));
    let model_key = ModelKey {
        provider: "test".into(),
        model_id: "scripted".into(),
    };
    let handler = Handler::new(llm, model_key, write_tx);

    let golden_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/m1_basic.jsonl");
    let frames: Vec<String> = std::fs::read_to_string(golden_path)
        .expect("golden file not found")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_owned())
        .collect();

    let mut responses: Vec<Value> = Vec::new();
    let mut session_id = String::new();

    for frame in &frames {
        let req: Value = serde_json::from_str(frame).expect("invalid JSON in golden file");
        let method = req["method"].as_str().expect("frame missing method");
        let id = req["id"].clone();
        let params = req["params"].clone();

        match method {
            "initialize" => {
                let resp = handler.handle_initialize(id, params).await;
                responses.push(serde_json::to_value(&resp).unwrap());
            }
            "session/new" => {
                let resp = handler.handle_session_new(id, params).await;
                let v = serde_json::to_value(&resp).unwrap();
                if let Some(sid) = v["result"]["sessionId"].as_str() {
                    session_id = sid.to_owned();
                }
                responses.push(v);
            }
            "session/prompt" => {
                let params_str = params.to_string().replace("<session-id>", &session_id);
                let params_sub: Value = serde_json::from_str(&params_str).unwrap();
                let resp = handler.handle_session_prompt(id, params_sub).await;
                responses.push(serde_json::to_value(&resp).unwrap());
            }
            _ => panic!("unexpected method in golden file: {method}"),
        }
    }

    let mut notifications: Vec<Value> = Vec::new();
    while let Ok(raw) = write_rx.try_recv() {
        let v: Value = serde_json::from_str(&raw).expect("notification should be valid JSON");
        notifications.push(v);
    }

    // initialize response includes protocolVersion and agentCapabilities
    let init_result = &responses[0]["result"];
    assert!(
        init_result.get("protocolVersion").is_some(),
        "initialize response missing protocolVersion"
    );
    assert!(
        init_result.get("agentCapabilities").is_some(),
        "initialize response missing agentCapabilities"
    );

    // session/new response includes a UUID session ID
    let new_result = &responses[1]["result"];
    assert_eq!(
        new_result["sessionId"]
            .as_str()
            .map(|s| s.len())
            .unwrap_or(0),
        36,
        "sessionId should be a UUID (36 chars)"
    );

    // at least one agent_message_chunk notification
    assert!(
        notifications.iter().any(|n| {
            n.get("method") == Some(&json!("session/update"))
                && n["params"]["update"]["sessionUpdate"] == json!("agent_message_chunk")
        }),
        "expected at least one session/update notification with agent_message_chunk"
    );

    // session/prompt response has stopReason: "end_turn"
    assert_eq!(
        responses[2]["result"]["stopReason"].as_str(),
        Some("end_turn"),
        "session/prompt should have stopReason: end_turn"
    );
}
