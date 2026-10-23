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

/// ACP protocol versions this agent implements, oldest first.
const SUPPORTED_PROTOCOL_VERSIONS: &[u32] = &[1];

/// The newest version in [`SUPPORTED_PROTOCOL_VERSIONS`].
const LATEST_PROTOCOL_VERSION: u32 =
    SUPPORTED_PROTOCOL_VERSIONS[SUPPORTED_PROTOCOL_VERSIONS.len() - 1];

/// Picks the `protocolVersion` to send back in the `initialize` response.
///
/// Per the ACP spec, this is the client's requested version if the agent
/// supports it, and otherwise the latest version the agent supports. The
/// client then decides whether to disconnect.
fn negotiate_protocol_version(requested: u32) -> u32 {
    if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        LATEST_PROTOCOL_VERSION
    }
}

/// An active ACP session wrapped in a concurrency-safe handle.
#[derive(Clone)]
pub(crate) struct SessionHandle {
    inner: Arc<Mutex<AcpSession>>,
}

impl SessionHandle {
    fn new(session: AcpSession) -> Self {
        Self {
            inner: Arc::new(Mutex::new(session)),
        }
    }

    /// Executes a prompt on this session, locking only this session for the prompt duration.
    pub async fn prompt(&self, text: String, cancel: CancellationToken) -> arnes_core::Result<()> {
        self.inner.lock().await.prompt(text, cancel).await
    }
}

/// Thread-safe registry of active ACP sessions.
///
/// Hides the internal locking complexity from [`Handler`]: looking up a session
/// acquires and releases the map lock immediately, returning an isolated [`SessionHandle`]
/// that can be prompted without holding the registry lock.
#[derive(Default)]
pub(crate) struct Sessions {
    inner: Mutex<HashMap<String, SessionHandle>>,
}

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, id: String, session: AcpSession) {
        self.inner
            .lock()
            .await
            .insert(id, SessionHandle::new(session));
    }

    pub async fn get(&self, id: &str) -> Option<SessionHandle> {
        self.inner.lock().await.get(id).cloned()
    }
}

pub struct Handler {
    llm: Arc<dyn LlmModel>,
    model_key: ModelKey,
    notify_tx: mpsc::UnboundedSender<String>,
    sessions: Sessions,
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
            sessions: Sessions::new(),
            cancel_tokens: Mutex::new(HashMap::new()),
            fs_read_text_file: AtomicBool::new(false),
            fs_write_text_file: AtomicBool::new(false),
            pending_requests: Arc::new(std::sync::Mutex::new(HashMap::new())),
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
                protocol_version: negotiate_protocol_version(p.protocol_version),
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
        self.sessions.insert(session_id.clone(), session).await;
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
        let Some(session) = self.sessions.get(&p.session_id).await else {
            return Response::err(id, error_code::INVALID_PARAMS, "unknown session_id");
        };
        let result = session.prompt(text, cancel).await;
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
        let tx = self
            .pending_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        if let Some(tx) = tx {
            let _ = tx.send(result);
        }
    }
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
