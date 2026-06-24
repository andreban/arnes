// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, sync::Arc};

use crate::{
    PermissionRequest, ToolContext, ToolKind, frontend::ToolCallUpdate, helpers::normalize_path,
};

use agent_rig::{
    model::ToolCall,
    tools::{ToolDefinition, ToolResult},
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

const NAME: &str = "write_text_file";
const DESCRIPTION: &str = "Writes a UTF-8 text string to a file on the host filesystem, \
                           creating the file if it does not exist or replacing its contents \
                           if it does. Use this to persist generated or modified text.";

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct WriteTextFileParams {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WriteTextFileOutput {
    pub written: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct WriteTextFile {
    context: ToolContext,
    definition: ToolDefinition,
}

impl WriteTextFile {
    pub fn new(context: ToolContext) -> Self {
        Self {
            context,
            definition: ToolDefinition {
                name: NAME.to_string(),
                description: DESCRIPTION.to_string(),
                parameters: schema_for!(WriteTextFileParams),
            },
        }
    }

    pub fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    pub async fn call<F, Fut, U, UFut>(
        &self,
        tool_call: &Arc<ToolCall>,
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
        let write_text_file_params = match WriteTextFileParams::deserialize(&tool_call.args) {
            Ok(args) => args,
            Err(e) => return ToolResult::error(format!("invalid tool arguments: {e}")),
        };
        let title = format!("Writing file {}", &write_text_file_params.path.display());
        update_toolcall(ToolCallUpdate {
            tool_call: tool_call.clone(),
            title,
        })
        .await;

        let request = PermissionRequest {
            tool_call: tool_call.clone(),
            kind: ToolKind::Other,
            proposal: tool_call.args.clone(),
        };
        if !request_permission(request).await {
            return ToolResult::error("User rejected tool call");
        }
        let Some(host) = self.context.host.write_text_file.as_ref() else {
            return ToolResult::error("Capability unavailable");
        };
        let path = normalize_path(&self.context.cwd, &write_text_file_params.path);
        // Always return Ok so the model receives a valid JSON object.
        // (Gemini requires FunctionResponse.response to be an object; a bare
        // string causes an empty/null candidate and a silent non-response.)
        let output = match host
            .write_text_file(&path, &write_text_file_params.content)
            .await
        {
            Ok(()) => WriteTextFileOutput {
                written: true,
                error: None,
            },
            Err(e) => WriteTextFileOutput {
                written: false,
                error: Some(format!("Failed to write '{}': {}", path.display(), e)),
            },
        };
        match serde_json::to_value(output) {
            Ok(value) => ToolResult::ok(value),
            Err(e) => ToolResult::error(format!("failed to serialize tool result: {e}")),
        }
    }

    pub fn prompt_guidelines(&self) -> &str {
        "Use `write_text_file` to create or overwrite a file with the given content. \
         The entire content replaces any existing file; there is no append mode."
    }
}
