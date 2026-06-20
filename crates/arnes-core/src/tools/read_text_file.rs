// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    PermissionRequest, ToolContext, ToolKind, frontend::ToolCallUpdate, helpers::normalize_path,
};

use agent_rig::tools::{ToolDefinition, ToolResult};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

const NAME: &str = "read_text_file";
const DESCRIPTION: &str = "Reads a UTF-8 text file from the host filesystem and returns its \
                           contents as a string. Use `line` (1-indexed) to start reading at a \
                           specific line and `limit` to cap the number of lines returned, which \
                           is useful when sampling large files.";

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ReadTextFileParams {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadTextFileOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct ReadTextFile {
    context: ToolContext,
    definition: ToolDefinition,
}

impl ReadTextFile {
    pub fn new(context: ToolContext) -> Self {
        Self {
            context,
            definition: ToolDefinition {
                name: NAME.to_string(),
                description: DESCRIPTION.to_string(),
                parameters: schema_for!(ReadTextFileParams),
            },
        }
    }

    pub fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    pub async fn call<F, Fut, U, UFut>(
        &self,
        args: Value,
        tool_call_id: String,
        request_permission: F,
        update_toolcall: U,
        _cancel: CancellationToken,
    ) -> ToolResult
    where
        F: Fn(PermissionRequest) -> Fut,
        Fut: Future<Output = bool>,
        U: Fn(ToolCallUpdate) -> UFut,
        UFut: Future<Output = ()>,
    {
        let read_text_file_params: ReadTextFileParams = match serde_json::from_value(args.clone()) {
            Ok(args) => args,
            Err(e) => return ToolResult::error(format!("invalid tool arguments: {e}")),
        };
        let title = format!("Reading file {}", &read_text_file_params.path.display());
        update_toolcall(ToolCallUpdate {
            tool_call_id: tool_call_id.clone(),
            tool_name: NAME.to_string(),
            args: args.clone(),
            title,
        })
        .await;

        let request = PermissionRequest {
            tool_call_id,
            tool_name: NAME.to_string(),
            args: args.clone(),
            kind: ToolKind::Read,
            proposal: args.clone(),
        };
        if !request_permission(request).await {
            return ToolResult::error("User rejected tool call");
        }

        if let Ok(mut read_grants) = self.context.read_grants.lock() {
            read_grants.insert(read_text_file_params.path.clone());
        }

        let Some(host) = self.context.host.read_text_file.as_ref() else {
            return ToolResult::error("Capability unavailable");
        };
        let path = normalize_path(&self.context.cwd, &read_text_file_params.path);
        // Always return Ok so the model receives a valid JSON object.
        // (Gemini requires FunctionResponse.response to be an object; a bare
        // string causes an empty/null candidate and a silent non-response.)
        let output = match host
            .read_text_file(
                &path,
                read_text_file_params.line,
                read_text_file_params.limit,
            )
            .await
        {
            Ok(content) => ReadTextFileOutput {
                content: Some(content),
                error: None,
            },
            Err(e) => ReadTextFileOutput {
                content: None,
                error: Some(format!("Failed to read '{}': {}", path.display(), e)),
            },
        };
        match serde_json::to_value(output) {
            Ok(value) => ToolResult::ok(value),
            Err(e) => ToolResult::error(format!("failed to serialize tool result: {e}")),
        }
    }

    pub fn prompt_guidelines(&self) -> &str {
        "Use `read_text_file` to inspect file contents before editing. \
         Prefer `line` and `limit` when the file is large; both are 1-indexed \
         and bound the returned slice."
    }
}
