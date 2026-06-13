mod edit;
mod read_text_file;
mod write_text_file;

use agent_rig::tools::Tool as AgentRigTool;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::ToolKind;

pub use edit::Edit;
pub use read_text_file::ReadTextFile;
pub use write_text_file::WriteTextFile;

#[async_trait]
pub trait Tool<I, O>: AgentRigTool<I, O>
where
    I: Serialize + DeserializeOwned + Send + Sync,
    O: Serialize + DeserializeOwned + Send + Sync,
{
    fn prompt_guidelines(&self) -> &str;
    #[allow(dead_code)]
    fn prompt_snippet(&self) -> &str;
    #[allow(dead_code)]
    fn permission_required(&self) -> bool;

    /// Semantic category the permission prompt shows for this tool.
    fn tool_kind(&self) -> ToolKind {
        ToolKind::Other
    }
}
