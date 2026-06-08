// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, io, path::Path, sync::Arc};

use arnes_core::ReadTextFile;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use crate::types::{filesystem::FsReadTextFileParams, jsonrpc::OutboundRequest};

pub type PendingRequests = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

/// Fulfils `ReadTextFile` by delegating to the ACP client via `fs/read_text_file`.
pub struct AcpReadTextFile {
    session_id: String,
    notify_tx: mpsc::UnboundedSender<String>,
    pending: PendingRequests,
}

impl AcpReadTextFile {
    pub fn new(
        session_id: String,
        notify_tx: mpsc::UnboundedSender<String>,
        pending: PendingRequests,
    ) -> Self {
        Self {
            session_id,
            notify_tx,
            pending,
        }
    }
}

#[async_trait]
impl ReadTextFile for AcpReadTextFile {
    async fn read_text_file(
        &self,
        path: &Path,
        line: Option<usize>,
        limit: Option<usize>,
    ) -> io::Result<String> {
        let request_id = Uuid::now_v7().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(request_id.clone(), tx);

        let params = FsReadTextFileParams {
            session_id: self.session_id.clone(),
            path: path.to_string_lossy().into_owned(),
            line: line.map(|n| n as u64),
            limit: limit.map(|n| n as u64),
        };
        let req = OutboundRequest::new(request_id, "fs/read_text_file", params);
        if let Ok(serialized) = serde_json::to_string(&req) {
            let _ = self.notify_tx.send(serialized);
        }

        match rx.await {
            Ok(Ok(result)) => result["content"]
                .as_str()
                .map(|s| s.to_owned())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing content")),
            Ok(Err(msg)) => Err(io::Error::other(msg)),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "response channel closed",
            )),
        }
    }
}
