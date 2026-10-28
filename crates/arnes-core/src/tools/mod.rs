// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod bash;
mod edit_text_file;
mod read_text_file;
mod write_text_file;

use std::collections::HashMap;

use agent_rig::tools::{ToolCallRequest, ToolResult};
use serde_json::Value;

pub use bash::Bash;
pub use edit_text_file::{EditTextFile, EditTextFileProposal};
pub use read_text_file::ReadTextFile;
pub use write_text_file::WriteTextFile;

use crate::{PermissionRequest, ToolCallOutcome, frontend::ToolCallUpdate};

pub enum Tool {
    Bash(bash::Bash),
    ReadTextFile(read_text_file::ReadTextFile),
    WriteTextFile(write_text_file::WriteTextFile),
    EditTextFile(edit_text_file::EditTextFile),
}

impl Tool {
    pub fn name(&self) -> &str {
        match self {
            Tool::Bash(tool) => &tool.definition().name,
            Tool::ReadTextFile(tool) => &tool.definition().name,
            Tool::WriteTextFile(tool) => &tool.definition().name,
            Tool::EditTextFile(tool) => &tool.definition().name,
        }
    }
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Tool>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Tool) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub async fn call<P, PFut, U, UFut>(
        &self,
        req: &ToolCallRequest,
        request_permission: P,
        update_toolcall: U,
    ) -> ToolCallOutcome
    where
        P: Fn(PermissionRequest) -> PFut,
        PFut: Future<Output = bool>,
        U: Fn(ToolCallUpdate) -> UFut,
        UFut: Future<Output = ()>,
    {
        let Some(tool) = self.tools.get(&req.details.name) else {
            return ToolCallOutcome::Unknown;
        };

        let tool_result = match tool {
            Tool::Bash(tool) => {
                tool.call(
                    &req.details,
                    request_permission,
                    update_toolcall,
                    req.cancellation_token.clone(),
                )
                .await
            }
            Tool::ReadTextFile(tool) => {
                tool.call(
                    &req.details,
                    request_permission,
                    update_toolcall,
                    req.cancellation_token.clone(),
                )
                .await
            }
            Tool::WriteTextFile(tool) => {
                tool.call(
                    &req.details,
                    request_permission,
                    update_toolcall,
                    req.cancellation_token.clone(),
                )
                .await
            }
            Tool::EditTextFile(tool) => {
                tool.call(
                    &req.details,
                    request_permission,
                    update_toolcall,
                    req.cancellation_token.clone(),
                )
                .await
            }
        };

        match tool_result {
            ToolResult::Ok(blocks) => ToolCallOutcome::Ok(blocks),
            // A string error is passed through as-is; `to_string` would add JSON quotes.
            ToolResult::Err(Value::String(error)) => ToolCallOutcome::Err(error),
            ToolResult::Err(error) => ToolCallOutcome::Err(error.to_string()),
        }
    }
}
