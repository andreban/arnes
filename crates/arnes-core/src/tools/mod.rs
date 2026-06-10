mod read_text_file;

use agent_rig::tools::Tool as AgentRigTool;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::ToolKind;

pub use read_text_file::ReadTextFile;

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

    /// The names of this tool's argument fields that hold a file path. For
    /// `read_text_file`, whose args are `{ "path": "f.txt", "line": 2 }`, this
    /// is `["path"]` — not `"line"`, which is not a path.
    ///
    /// The permission layer sees a call's args as untyped JSON and can't tell
    /// which fields are paths. This list tells it which fields to read the
    /// paths out of, so it can show the files the call will touch in the
    /// approval prompt (`PermissionRequest::locations`). Empty for tools that
    /// take no file paths.
    fn location_arg_keys(&self) -> &'static [&'static str] {
        &[]
    }
}
