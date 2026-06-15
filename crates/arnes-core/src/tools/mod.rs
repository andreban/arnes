mod edit_text_file;
mod read_text_file;
mod write_text_file;

use agent_rig::tools::Tool as RigTool;

use crate::ToolKind;

pub use edit_text_file::{EditTextFile, EditTextFileProposal};
pub use read_text_file::ReadTextFile;
pub use write_text_file::WriteTextFile;

/// Adds the prompt and permission metadata arnes needs on top of an agent-rig
/// [`Tool`](RigTool).
pub trait Tool: RigTool {
    fn prompt_guidelines(&self) -> &str;
    #[allow(dead_code)]
    fn prompt_snippet(&self) -> &str;

    /// Semantic category the permission prompt shows for this tool.
    fn tool_kind(&self) -> ToolKind {
        ToolKind::Other
    }
}
