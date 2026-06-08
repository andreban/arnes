// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::ToolContext;

use super::Tool;
use agent_rig::error::Error as AgentRigError;
use agent_rig::tools::{Tool as AgentRigTool, ToolDefinition};
use async_trait::async_trait;
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
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
    params: Schema,
}

impl ReadTextFile {
    pub fn new(context: ToolContext) -> Self {
        Self {
            context,
            params: schema_for!(ReadTextFileParams),
        }
    }
}

#[async_trait]
impl AgentRigTool<ReadTextFileParams, ReadTextFileOutput> for ReadTextFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: NAME.to_string(),
            description: DESCRIPTION.to_string(),
            parameters: self.params.clone(),
        }
    }

    async fn call(
        &self,
        args: ReadTextFileParams,
        _cancellation: CancellationToken,
    ) -> Result<ReadTextFileOutput, AgentRigError> {
        let host = self
            .context
            .host
            .read_text_file
            .as_ref()
            .ok_or(AgentRigError::Agent("Capability unavailable".to_string()))?;
        // Always return Ok so the model receives a valid JSON object.
        // (Gemini requires FunctionResponse.response to be an object; a bare
        // string causes an empty/null candidate and a silent non-response.)
        match host.read_text_file(&args.path, args.line, args.limit).await {
            Ok(content) => Ok(ReadTextFileOutput {
                content: Some(content),
                error: None,
            }),
            Err(e) => Ok(ReadTextFileOutput {
                content: None,
                error: Some(format!("Failed to read '{}': {}", args.path.display(), e)),
            }),
        }
    }
}

#[async_trait]
impl Tool<ReadTextFileParams, ReadTextFileOutput> for ReadTextFile {
    fn prompt_guidelines(&self) -> &str {
        "Use `read_text_file` to inspect file contents before editing. \
         Prefer `line` and `limit` when the file is large; both are 1-indexed \
         and bound the returned slice."
    }

    fn prompt_snippet(&self) -> &str {
        "read_text_file(path, line?, limit?) -> file contents"
    }

    fn permission_required(&self) -> bool {
        false
    }
}
