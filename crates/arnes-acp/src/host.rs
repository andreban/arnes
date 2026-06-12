// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, io, path::Path, sync::Arc};

use arnes_core::{ReadTextFile, WriteTextFile};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use crate::types::{
    filesystem::{FsReadTextFileParams, FsWriteTextFileParams},
    jsonrpc::OutboundRequest,
};

pub type PendingRequests = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

/// Fulfils `WriteTextFile` by delegating to the ACP client via `fs/write_text_file`.
pub struct AcpWriteTextFile {
    session_id: String,
    notify_tx: mpsc::UnboundedSender<String>,
    pending: PendingRequests,
}

impl AcpWriteTextFile {
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
impl WriteTextFile for AcpWriteTextFile {
    async fn write_text_file(&self, path: &Path, content: &str) -> io::Result<()> {
        let request_id = Uuid::now_v7().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(request_id.clone(), tx);

        let params = FsWriteTextFileParams {
            session_id: self.session_id.clone(),
            path: path.to_string_lossy().into_owned(),
            content: content.to_owned(),
        };
        let req = OutboundRequest::new(request_id.clone(), "fs/write_text_file", params);
        let serialized = serde_json::to_string(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if self.notify_tx.send(serialized).is_err() {
            self.pending.lock().await.remove(&request_id);
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "client disconnected",
            ));
        }

        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(Ok(result))) => {
                if let Some(err_msg) = result["error"].as_str() {
                    Err(io::Error::other(err_msg.to_owned()))
                } else {
                    Ok(())
                }
            }
            Ok(Ok(Err(msg))) => Err(io::Error::other(msg)),
            Ok(Err(_)) => {
                self.pending.lock().await.remove(&request_id);
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "response channel closed",
                ))
            }
            Err(_) => {
                self.pending.lock().await.remove(&request_id);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "fs/write_text_file timed out",
                ))
            }
        }
    }
}

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
        let req = OutboundRequest::new(request_id.clone(), "fs/read_text_file", params);
        let serialized = serde_json::to_string(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if self.notify_tx.send(serialized).is_err() {
            self.pending.lock().await.remove(&request_id);
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "client disconnected",
            ));
        }

        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(Ok(result))) => result["content"]
                .as_str()
                .map(|s| s.to_owned())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing content")),
            Ok(Ok(Err(msg))) => Err(io::Error::other(msg)),
            Ok(Err(_)) => {
                self.pending.lock().await.remove(&request_id);
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "response channel closed",
                ))
            }
            Err(_) => {
                self.pending.lock().await.remove(&request_id);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "fs/read_text_file timed out",
                ))
            }
        }
    }
}
