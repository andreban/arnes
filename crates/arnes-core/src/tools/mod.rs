mod edit_text_file;
mod read_text_file;
mod write_text_file;

use std::collections::HashMap;

use agent_rig::tools::{ToolCallRequest, ToolResult};

pub use edit_text_file::{EditTextFile, EditTextFileProposal};
pub use read_text_file::ReadTextFile;
pub use write_text_file::WriteTextFile;

use crate::{PermissionRequest, ToolCallOutcome};

#[allow(clippy::enum_variant_names)]
pub enum Tool {
    ReadTextFile(read_text_file::ReadTextFile),
    WriteTextFile(write_text_file::WriteTextFile),
    EditTextFile(edit_text_file::EditTextFile),
}

impl Tool {
    pub fn name(&self) -> &str {
        match self {
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

    pub async fn call<F, Fut>(
        &self,
        req: &ToolCallRequest,
        request_permission: F,
    ) -> ToolCallOutcome
    where
        F: Fn(PermissionRequest) -> Fut,
        Fut: Future<Output = bool>,
    {
        let Some(tool) = self.tools.get(&req.tool_name) else {
            return ToolCallOutcome::Unknown;
        };

        let tool_result = match tool {
            Tool::ReadTextFile(tool) => {
                tool.call(
                    req.args.clone(),
                    req.tool_call_id.clone(),
                    request_permission,
                    req.cancellation_token.clone(),
                )
                .await
            }
            Tool::WriteTextFile(tool) => {
                tool.call(
                    req.args.clone(),
                    req.tool_call_id.clone(),
                    request_permission,
                    req.cancellation_token.clone(),
                )
                .await
            }
            Tool::EditTextFile(tool) => {
                tool.call(
                    req.args.clone(),
                    req.tool_call_id.clone(),
                    request_permission,
                    req.cancellation_token.clone(),
                )
                .await
            }
        };

        match tool_result {
            ToolResult::Ok(value) => ToolCallOutcome::Ok(value),
            ToolResult::Err(error) => ToolCallOutcome::Err(error.to_string()),
        }
    }
}
