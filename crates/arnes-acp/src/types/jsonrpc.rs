// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 reserved error codes.
pub mod error_code {
    /// The request is not a valid Request object.
    pub const INVALID_REQUEST: i32 = -32600;
    /// The requested method does not exist.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// The method's parameters are invalid.
    pub const INVALID_PARAMS: i32 = -32602;
    /// An internal error occurred while handling the request.
    pub const INTERNAL_ERROR: i32 = -32603;
}

#[derive(Debug, Deserialize)]
pub struct Request {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}

impl Response {
    pub fn ok(id: Value, result: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(serde_json::to_value(result).unwrap()),
            error: None,
        }
    }

    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(ErrorObject {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorObject {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Notification {
    pub jsonrpc: &'static str,
    pub method: &'static str,
    pub params: Value,
}

impl Notification {
    pub fn new(method: &'static str, params: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0",
            method,
            params: serde_json::to_value(params).unwrap(),
        }
    }
}

/// An outbound JSON-RPC request from the agent to the client (expects a response).
#[derive(Debug, Serialize)]
pub struct OutboundRequest {
    pub jsonrpc: &'static str,
    pub id: String,
    pub method: &'static str,
    pub params: Value,
}

impl OutboundRequest {
    pub fn new(id: String, method: &'static str, params: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method,
            params: serde_json::to_value(params).unwrap(),
        }
    }
}
