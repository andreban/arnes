// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use agent_rig::model::LlmModel;
use arnes_core::{Host, ModelKey, Session};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    frontend::AcpFrontend,
    host::{AcpReadTextFile, AcpWriteTextFile, PendingRequests},
    types::{
        initialize::{AgentCapabilities, AgentInfo, InitializeParams, InitializeResult},
        jsonrpc::{Response, error_code},
        session::{
            SessionCancelParams, SessionNewParams, SessionNewResult, SessionPromptParams,
            SessionPromptResult,
        },
    },
};

type AcpSession = Session<AcpFrontend>;

pub struct Handler {
    llm: Arc<dyn LlmModel>,
    model_key: ModelKey,
    notify_tx: mpsc::UnboundedSender<String>,
    sessions: Mutex<HashMap<String, Arc<Mutex<AcpSession>>>>,
    cancel_tokens: Mutex<HashMap<String, CancellationToken>>,
    fs_read_text_file: AtomicBool,
    fs_write_text_file: AtomicBool,
    pending_requests: PendingRequests,
}

impl Handler {
    pub fn new(
        llm: Arc<dyn LlmModel>,
        model_key: ModelKey,
        notify_tx: mpsc::UnboundedSender<String>,
    ) -> Self {
        Self {
            llm,
            model_key,
            notify_tx,
            sessions: Mutex::new(HashMap::new()),
            cancel_tokens: Mutex::new(HashMap::new()),
            fs_read_text_file: AtomicBool::new(false),
            fs_write_text_file: AtomicBool::new(false),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn write_tx(&self) -> mpsc::UnboundedSender<String> {
        self.notify_tx.clone()
    }

    pub async fn handle_initialize(&self, id: Value, params: Value) -> Response {
        let Ok(p) = serde_json::from_value::<InitializeParams>(params) else {
            return Response::err(id, error_code::INVALID_PARAMS, "invalid params");
        };
        self.fs_read_text_file
            .store(p.client_capabilities.fs.read_text_file, Ordering::Relaxed);
        self.fs_write_text_file
            .store(p.client_capabilities.fs.write_text_file, Ordering::Relaxed);
        Response::ok(
            id,
            InitializeResult {
                protocol_version: p.protocol_version,
                agent_info: AgentInfo {
                    name: "arnes-acp",
                    title: "arnes",
                    version: "0.0.0",
                },
                agent_capabilities: AgentCapabilities {
                    load_session: false,
                    ..Default::default()
                },
                auth_methods: vec![],
            },
        )
    }

    pub async fn handle_session_new(&self, id: Value, params: Value) -> Response {
        let p = serde_json::from_value::<SessionNewParams>(params)
            .unwrap_or(SessionNewParams { cwd: None });
        let cwd = p
            .cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let session_id = Uuid::now_v7().to_string();
        let frontend = Arc::new(AcpFrontend::new(
            session_id.clone(),
            self.notify_tx.clone(),
            Arc::clone(&self.pending_requests),
        ));
        let host = Host {
            read_text_file: if self.fs_read_text_file.load(Ordering::Relaxed) {
                Some(Arc::new(AcpReadTextFile::new(
                    session_id.clone(),
                    self.notify_tx.clone(),
                    Arc::clone(&self.pending_requests),
                )))
            } else {
                None
            },
            write_text_file: if self.fs_write_text_file.load(Ordering::Relaxed) {
                Some(Arc::new(AcpWriteTextFile::new(
                    session_id.clone(),
                    self.notify_tx.clone(),
                    Arc::clone(&self.pending_requests),
                )))
            } else {
                None
            },
            ..Default::default()
        };
        let session = Session::new(
            frontend,
            host,
            self.llm.clone(),
            self.model_key.clone(),
            cwd,
        );
        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), Arc::new(Mutex::new(session)));
        Response::ok(id, SessionNewResult { session_id })
    }

    pub async fn handle_session_prompt(&self, id: Value, params: Value) -> Response {
        let Ok(p) = serde_json::from_value::<SessionPromptParams>(params) else {
            return Response::err(id, error_code::INVALID_PARAMS, "invalid params");
        };
        let text = p.text();
        let cancel = CancellationToken::new();
        self.cancel_tokens
            .lock()
            .await
            .insert(p.session_id.clone(), cancel.clone());
        let session_arc = {
            let sessions = self.sessions.lock().await;
            let Some(session) = sessions.get(&p.session_id) else {
                return Response::err(id, error_code::INVALID_PARAMS, "unknown session_id");
            };
            Arc::clone(session)
        };
        let result = session_arc.lock().await.prompt(text, cancel).await;
        self.cancel_tokens.lock().await.remove(&p.session_id);
        // A completed turn reports `end_turn`; a failed one is a JSON-RPC error,
        // since ACP defines no stop reason for failure.
        match result {
            Ok(()) => Response::ok(
                id,
                SessionPromptResult {
                    stop_reason: "end_turn",
                },
            ),
            Err(e) => Response::err(
                id,
                error_code::INTERNAL_ERROR,
                format!("prompt failed: {e}"),
            ),
        }
    }

    pub async fn handle_session_cancel(&self, params: Value) {
        let Ok(p) = serde_json::from_value::<SessionCancelParams>(params) else {
            return;
        };
        if let Some(token) = self.cancel_tokens.lock().await.get(&p.session_id) {
            token.cancel();
        }
    }

    /// Routes a JSON-RPC response (from the client) back to the waiting caller.
    pub async fn handle_response(&self, id: String, result: Result<Value, String>) {
        if let Some(tx) = self.pending_requests.lock().await.remove(&id) {
            let _ = tx.send(result);
        }
    }
}

#[cfg(test)]
mod tests {
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
}
