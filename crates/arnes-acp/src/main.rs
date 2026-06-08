// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use agent_rig::models::gemini::GeminiModel;
use arnes_core::ModelKey;
use clap::Parser;
use futures_util::StreamExt;
use geologia::prelude::ThinkingConfig;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};

use arnes_acp::handler::Handler;
use arnes_acp::types::jsonrpc::{Request, Response};

#[derive(Parser)]
#[command(about = "arnes ACP server — JSON-RPC 2.0 over stdio")]
struct Args {
    #[arg(long, default_value = "gemini-3.5-flash")]
    model: String,

    #[arg(long, env = "GEMINI_API_KEY")]
    api_key: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let log_file = std::fs::File::create("arnes-acp.log")?;
    let filter = tracing_subscriber::EnvFilter::new(
        "off,ollama_rs=debug,geologia=debug,agent_rig=debug,arnes_core=debug,arnes_acp=debug",
    );
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(log_file))
        .with_env_filter(filter)
        .with_ansi(false)
        .init();

    let thinking_config = ThinkingConfig {
        include_thoughts: true,
        thinking_level: Some(geologia::prelude::ThinkingLevel::High),
        ..Default::default()
    };
    let llm = Arc::new(
        GeminiModel::builder(&args.api_key, &args.model)
            .thinking_config(thinking_config)
            .build(),
    );
    let model_key = ModelKey {
        provider: "gemini".into(),
        model_id: args.model.clone(),
    };

    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<String>();
    let handler = Arc::new(Handler::new(llm, model_key, write_tx));

    let mut stdout = tokio::io::stdout();

    // Single writer: all output — notifications and responses — flows through write_rx.
    let write_handle = tokio::spawn(async move {
        while let Some(line) = write_rx.recv().await {
            let _ = stdout.write_all(line.as_bytes()).await;
            let _ = stdout.write_all(b"\n").await;
            let _ = stdout.flush().await;
        }
    });

    // Read loop: parse incoming JSON-RPC lines and dispatch.
    let stdin = tokio::io::stdin();
    let mut lines = FramedRead::new(stdin, LinesCodec::new());

    while let Some(Ok(line)) = lines.next().await {
        let handler = handler.clone();

        // Parse to Value first so we can distinguish requests from responses.
        let value: Value = match serde_json::from_str(&line) {
            Err(_) => {
                let resp = Response::err(Value::Null, -32700, "parse error");
                if let Ok(s) = serde_json::to_string(&resp) {
                    let _ = handler.write_tx().send(s);
                }
                continue;
            }
            Ok(v) => v,
        };

        // A JSON-RPC response has no "method" field; route it to any pending caller.
        if value.get("method").is_none() {
            if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
                let result = match (value.get("result"), value.get("error")) {
                    (Some(r), _) => Ok(r.clone()),
                    (_, Some(e)) => Err(e["message"].as_str().unwrap_or("error").to_string()),
                    _ => Err("invalid response".to_string()),
                };
                handler.handle_response(id.to_string(), result).await;
            }
            continue;
        }

        let response: Option<Response> = match serde_json::from_value::<Request>(value) {
            Err(_) => Some(Response::err(Value::Null, -32600, "invalid request")),
            Ok(req) => {
                let id = req.id.unwrap_or(Value::Null);
                let params = req.params.unwrap_or(Value::Null);
                match req.method.as_str() {
                    "initialize" => Some(handler.handle_initialize(id, params).await),
                    "session/new" => Some(handler.handle_session_new(id, params).await),
                    "session/prompt" => {
                        // Spawn so the read loop stays live for session/cancel
                        // and for fs/read_text_file responses.
                        let tx = handler.write_tx();
                        let h = handler.clone();
                        tokio::spawn(async move {
                            let resp = h.handle_session_prompt(id, params).await;
                            if let Ok(serialized) = serde_json::to_string(&resp) {
                                let _ = tx.send(serialized);
                            }
                        });
                        None
                    }
                    "session/cancel" => {
                        handler.handle_session_cancel(params).await;
                        None
                    }
                    _ => {
                        if id != Value::Null {
                            Some(Response::err(id, -32601, "method not found"))
                        } else {
                            None
                        }
                    }
                }
            }
        };

        if let Some(resp) = response
            && let Ok(serialized) = serde_json::to_string(&resp)
        {
            let _ = handler.write_tx().send(serialized);
        }
    }

    drop(handler);
    write_handle.await?;
    Ok(())
}
