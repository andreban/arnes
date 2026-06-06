// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use agent_rig::model::LlmModel;
use arnes_core::{ModelKey, Session};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    frontend::AcpFrontend,
    host::AcpHost,
    types::{
        initialize::{AgentCapabilities, AgentInfo, InitializeParams, InitializeResult},
        jsonrpc::Response,
        session::{
            SessionCancelParams, SessionNewParams, SessionNewResult, SessionPromptParams,
            SessionPromptResult,
        },
    },
};

type AcpSession = Session<AcpFrontend, AcpHost>;

pub struct Handler {
    llm: Arc<dyn LlmModel>,
    model_key: ModelKey,
    notify_tx: mpsc::UnboundedSender<String>,
    sessions: Mutex<HashMap<String, AcpSession>>,
    cancel_tokens: Mutex<HashMap<String, CancellationToken>>,
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
        }
    }

    pub async fn handle_initialize(&self, id: Value, params: Value) -> Response {
        let Ok(p) = serde_json::from_value::<InitializeParams>(params) else {
            return Response::err(id, -32602, "invalid params");
        };
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
                },
                auth_methods: vec![],
            },
        )
    }

    pub async fn handle_session_new(&self, id: Value, params: Value) -> Response {
        let _p = serde_json::from_value::<SessionNewParams>(params)
            .unwrap_or(SessionNewParams { cwd: None });
        let session_id = Uuid::now_v7().to_string();
        let frontend = Arc::new(AcpFrontend::new(session_id.clone(), self.notify_tx.clone()));
        let host = Arc::new(AcpHost);
        let session = Session::new(frontend, host, self.llm.clone(), self.model_key.clone());
        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), session);
        Response::ok(id, SessionNewResult { session_id })
    }

    pub async fn handle_session_prompt(&self, id: Value, params: Value) -> Response {
        let Ok(p) = serde_json::from_value::<SessionPromptParams>(params) else {
            return Response::err(id, -32602, "invalid params");
        };
        let text = p.text();
        let cancel = CancellationToken::new();
        self.cancel_tokens
            .lock()
            .await
            .insert(p.session_id.clone(), cancel.clone());
        let result = {
            let mut sessions = self.sessions.lock().await;
            let Some(session) = sessions.get_mut(&p.session_id) else {
                return Response::err(id, -32602, "unknown session_id");
            };
            session.prompt(text, cancel).await
        };
        self.cancel_tokens.lock().await.remove(&p.session_id);
        let stop_reason = match result {
            Ok(()) => "end_turn",
            Err(_) => "error",
        };
        Response::ok(id, SessionPromptResult { stop_reason })
    }

    pub async fn handle_session_cancel(&self, params: Value) {
        let Ok(p) = serde_json::from_value::<SessionCancelParams>(params) else {
            return;
        };
        if let Some(token) = self.cancel_tokens.lock().await.get(&p.session_id) {
            token.cancel();
        }
    }
}
