// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T, E = Box<dyn std::error::Error>> = core::result::Result<T, E>;

use std::io::Cursor;

use agent_rig::{
    error::Error as RigError,
    model::{LlmModel, ModelRequest, ModelResponse},
};
use async_trait::async_trait;

use super::*;

struct DummyLlm;

#[async_trait]
impl LlmModel for DummyLlm {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, RigError> {
        Ok(ModelResponse {
            text: Some("dummy".into()),
            tool_calls: vec![],
            thinking: None,
            provider_metadata: None,
            token_usage: None,
        })
    }
}

#[tokio::test]
async fn continues_processing_after_decoding_error() -> Result<()> {
    // -- Setup & Fixtures
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
    let llm = Arc::new(DummyLlm);
    let model_key = ModelKey {
        provider: "dummy".into(),
        model_id: "dummy".into(),
    };
    let handler = Arc::new(Handler::new(llm, model_key, write_tx));

    // Input contains invalid UTF-8 bytes followed by newline, then a valid JSON-RPC request.
    let mut input = Vec::new();
    input.extend_from_slice(b"\xff\xfe\xfd\n");
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"test/ping\"}\n");
    let reader = Cursor::new(input);

    // -- Exec
    run_read_loop(reader, handler).await;

    // -- Check
    let response_raw = write_rx
        .recv()
        .await
        .ok_or("expected a response for the valid request following the invalid frame")?;
    let response: Value = serde_json::from_str(&response_raw)?;
    assert_eq!(response["id"], "1");
    assert_eq!(response["error"]["code"], error_code::METHOD_NOT_FOUND);

    Ok(())
}

#[tokio::test]
async fn continues_processing_after_json_parse_error() -> Result<()> {
    // -- Setup & Fixtures
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
    let llm = Arc::new(DummyLlm);
    let model_key = ModelKey {
        provider: "dummy".into(),
        model_id: "dummy".into(),
    };
    let handler = Arc::new(Handler::new(llm, model_key, write_tx));

    // Input contains malformed JSON followed by a valid JSON-RPC request.
    let mut input = Vec::new();
    input.extend_from_slice(b"not valid json\n");
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":\"2\",\"method\":\"test/ping\"}\n");
    let reader = Cursor::new(input);

    // -- Exec
    run_read_loop(reader, handler).await;

    // -- Check
    let parse_err_raw = write_rx
        .recv()
        .await
        .ok_or("expected a parse error response")?;
    let parse_err: Value = serde_json::from_str(&parse_err_raw)?;
    assert_eq!(parse_err["error"]["code"], -32700);

    let next_resp_raw = write_rx
        .recv()
        .await
        .ok_or("expected a response for the next request")?;
    let next_resp: Value = serde_json::from_str(&next_resp_raw)?;
    assert_eq!(next_resp["id"], "2");
    assert_eq!(next_resp["error"]["code"], error_code::METHOD_NOT_FOUND);

    Ok(())
}

#[tokio::test]
async fn shuts_down_cleanly_on_eof() -> Result<()> {
    // -- Setup & Fixtures
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
    let llm = Arc::new(DummyLlm);
    let model_key = ModelKey {
        provider: "dummy".into(),
        model_id: "dummy".into(),
    };
    let handler = Arc::new(Handler::new(llm, model_key, write_tx));

    let reader = Cursor::new(Vec::new());

    // -- Exec
    run_read_loop(reader, handler).await;

    // -- Check
    assert!(write_rx.recv().await.is_none());

    Ok(())
}
