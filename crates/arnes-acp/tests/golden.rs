// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod support;

use std::sync::Arc;

use arnes_acp::handler::Handler;
use arnes_core::ModelKey;
use serde_json::{Value, json};
use support::{ScriptedLlm, ScriptedTurn};
use tokio::sync::mpsc;

#[tokio::test]
async fn m1_basic_golden() {
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
    let llm = Arc::new(ScriptedLlm::new(vec![ScriptedTurn::Text("Hello!".into())]));
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

#[tokio::test]
async fn m2_tool_call_golden() {
    const FILE_CONTENT: &str = "[package]\nname = \"arnes\"\n";

    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
    let llm = Arc::new(ScriptedLlm::new(vec![ScriptedTurn::ToolCallThenText {
        name: "read_text_file".into(),
        args: json!({ "path": "Cargo.toml" }),
        follow_up: "The file contains a package named arnes.".into(),
    }]));
    let model_key = ModelKey {
        provider: "test".into(),
        model_id: "scripted".into(),
    };
    let handler = Arc::new(Handler::new(llm, model_key, write_tx));

    let golden_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/m2_tool_call.jsonl"
    );
    let frames: Vec<String> = std::fs::read_to_string(golden_path)
        .expect("golden file not found")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_owned())
        .collect();

    // Outbound frames (notifications and the fs/read_text_file request) in the
    // order the handler emits them.
    let mut outbound: Vec<Value> = Vec::new();
    let mut session_id = String::new();
    let mut prompt_resp: Option<Value> = None;

    for frame in &frames {
        let req: Value = serde_json::from_str(frame).expect("invalid JSON in golden file");
        let method = req["method"].as_str().expect("frame missing method");
        let id = req["id"].clone();
        let params = req["params"].clone();

        match method {
            "initialize" => {
                handler.handle_initialize(id, params).await;
            }
            "session/new" => {
                let resp = handler.handle_session_new(id, params).await;
                let v = serde_json::to_value(&resp).unwrap();
                session_id = v["result"]["sessionId"]
                    .as_str()
                    .expect("session/new should return a session id")
                    .to_owned();
            }
            "session/prompt" => {
                let params_str = params.to_string().replace("<session-id>", &session_id);
                let params_sub: Value = serde_json::from_str(&params_str).unwrap();
                // The prompt blocks on the outbound fs/read_text_file request, so
                // drive it on a task while this test plays the client.
                let h = Arc::clone(&handler);
                let task =
                    tokio::spawn(async move { h.handle_session_prompt(id, params_sub).await });

                // Collect outbound frames, answering each client request as it
                // arrives: allow the permission prompt, then return the file
                // contents. The loop ends once the read has been answered.
                loop {
                    let raw = write_rx
                        .recv()
                        .await
                        .expect("channel closed before fs/read_text_file");
                    let v: Value = serde_json::from_str(&raw).unwrap();
                    let method = v.get("method").cloned();
                    outbound.push(v.clone());
                    if method == Some(json!("session/request_permission")) {
                        let req_id = v["id"].as_str().unwrap().to_string();
                        handler
                            .handle_response(
                                req_id,
                                Ok(json!({
                                    "outcome": { "outcome": "selected", "optionId": "allow-once" }
                                })),
                            )
                            .await;
                    } else if method == Some(json!("fs/read_text_file")) {
                        let req_id = v["id"].as_str().unwrap().to_string();
                        handler
                            .handle_response(req_id, Ok(json!({ "content": FILE_CONTENT })))
                            .await;
                        break;
                    }
                }

                let resp = task.await.unwrap();
                prompt_resp = Some(serde_json::to_value(&resp).unwrap());

                // Drain the remaining frames emitted after the client response.
                while let Ok(raw) = write_rx.try_recv() {
                    outbound.push(serde_json::from_str(&raw).unwrap());
                }
            }
            _ => panic!("unexpected method in golden file: {method}"),
        }
    }

    let is_session_update = |v: &Value, kind: &str| {
        v.get("method") == Some(&json!("session/update"))
            && v["params"]["update"]["sessionUpdate"] == json!(kind)
    };

    // A `tool_call` lifecycle update is emitted for the read, carrying the
    // tool's descriptive title and the raw arguments the model supplied.
    let tool_call = outbound
        .iter()
        .find(|v| is_session_update(v, "tool_call"))
        .expect("expected a tool_call session/update");
    assert_eq!(
        tool_call["params"]["update"]["title"].as_str(),
        Some("Read Cargo.toml"),
        "tool_call update should carry the tool's descriptive title"
    );
    assert_eq!(
        tool_call["params"]["update"]["rawInput"],
        json!({ "path": "Cargo.toml" }),
        "tool_call update should carry the model's raw arguments"
    );
    // The agent reaches out to the client over `fs/read_text_file`.
    //
    // Note: this request is observed *before* the `tool_call` update above.
    // The runner emits `ToolCallStarted` and then runs the tool concurrently
    // with the consumer that forwards the event to the frontend, so the tool's
    // outbound request can win the race onto the wire. We therefore assert
    // presence and the post-response ordering rather than tool_call-first.
    let read_request_idx = outbound
        .iter()
        .position(|v| v.get("method") == Some(&json!("fs/read_text_file")))
        .expect("expected an fs/read_text_file outbound request");
    // The agent gates the tool behind the client: a session/request_permission
    // request goes out before the tool runs (and thus before it reads the file).
    let permission_idx = outbound
        .iter()
        .position(|v| v.get("method") == Some(&json!("session/request_permission")))
        .expect("expected a session/request_permission outbound request");
    assert!(
        permission_idx < read_request_idx,
        "permission request should precede the fs/read_text_file the tool issues"
    );
    // The completed update is emitted once the client response arrives.
    let completed_idx = outbound
        .iter()
        .position(|v| {
            is_session_update(v, "tool_call_update")
                && v["params"]["update"]["status"] == json!("completed")
        })
        .expect("expected a completed tool_call_update");
    // The assistant's reply chunk follows.
    let message_idx = outbound
        .iter()
        .position(|v| is_session_update(v, "agent_message_chunk"))
        .expect("expected an agent_message_chunk");

    // completed update arrives after the client response.
    assert!(
        completed_idx > read_request_idx,
        "tool_call_update completed should follow the fs/read_text_file response"
    );
    // the assistant's reply chunk follows the completed tool call.
    assert!(
        message_idx > completed_idx,
        "agent_message_chunk should follow the completed tool call"
    );

    // session/prompt response has stopReason: "end_turn"
    assert_eq!(
        prompt_resp.expect("session/prompt produced no response")["result"]["stopReason"].as_str(),
        Some("end_turn"),
        "session/prompt should have stopReason: end_turn"
    );
}
