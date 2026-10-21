// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T, E = Box<dyn std::error::Error>> = core::result::Result<T, E>;

use super::*;
use agent_rig::{
    error::Error,
    model::{ModelRequest, ModelResponse},
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::oneshot;

struct BlockingLlm {
    rx: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
}

#[async_trait]
impl LlmModel for BlockingLlm {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, Error> {
        let rx = self.rx.lock().await.take();
        if let Some(rx) = rx {
            let _ = rx.await;
        }
        Ok(ModelResponse {
            text: Some("done".into()),
            tool_calls: vec![],
            thinking: None,
            provider_metadata: None,
            token_usage: None,
        })
    }
}

#[tokio::test]
async fn concurrent_sessions_do_not_block_session_creation_or_other_prompts() {
    let (tx, rx) = oneshot::channel::<()>();
    let llm = Arc::new(BlockingLlm {
        rx: tokio::sync::Mutex::new(Some(rx)),
    });
    let model_key = ModelKey {
        provider: "test".into(),
        model_id: "test".into(),
    };
    let (notify_tx, _notify_rx) = mpsc::unbounded_channel();
    let handler = Arc::new(Handler::new(llm, model_key, notify_tx));

    // Create session 1
    let resp1 = handler.handle_session_new(json!(1), json!({})).await;
    let v1 = serde_json::to_value(&resp1).unwrap();
    let session_id1 = v1["result"]["sessionId"].as_str().unwrap().to_string();

    // Spawn prompt for session 1 (will block on oneshot channel rx)
    let handler_clone = Arc::clone(&handler);
    let s1_prompt = tokio::spawn(async move {
        handler_clone
            .handle_session_prompt(
                json!(2),
                json!({
                    "sessionId": session_id1,
                    "prompt": [{ "type": "text", "text": "hi" }]
                }),
            )
            .await
    });

    // Small pause to ensure prompt task has started and called handle_session_prompt
    tokio::task::yield_now().await;

    // While session 1 prompt is blocked, creating session 2 must not block
    let resp2 = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        handler.handle_session_new(json!(3), json!({})),
    )
    .await
    .expect("handle_session_new timed out - sessions mutex was held across prompt!");

    let v2 = serde_json::to_value(&resp2).unwrap();
    assert!(v2["result"]["sessionId"].as_str().is_some());

    // Unblock session 1
    let _ = tx.send(());
    let prompt_res = s1_prompt.await.unwrap();
    let prompt_val = serde_json::to_value(&prompt_res).unwrap();
    assert_eq!(prompt_val["result"]["stopReason"], "end_turn");
}

#[tokio::test]
async fn sessions_registry_returns_none_for_missing() {
    let sessions = Sessions::new();
    assert!(sessions.get("missing-id").await.is_none());
}

#[test]
fn negotiate_returns_requested_version_when_supported() -> Result<()> {
    assert_eq!(negotiate_protocol_version(1), 1);
    Ok(())
}

#[test]
fn negotiate_returns_latest_supported_for_newer_version() -> Result<()> {
    assert_eq!(negotiate_protocol_version(2), 1);
    Ok(())
}

#[test]
fn negotiate_returns_latest_supported_for_unknown_versions() -> Result<()> {
    for requested in [0, 99, u32::MAX] {
        assert_eq!(negotiate_protocol_version(requested), 1);
    }
    Ok(())
}
