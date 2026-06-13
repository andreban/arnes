mod edit;
mod read_text_file;
mod write_text_file;

use agent_rig::tools::SimpleTool;

use crate::ToolKind;

pub use edit::Edit;
pub use read_text_file::ReadTextFile;
pub use write_text_file::WriteTextFile;

/// Adds the prompt and permission metadata arnes needs on top of an agent-rig
/// [`SimpleTool`].
pub trait Tool: SimpleTool {
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

#[cfg(test)]
pub(crate) mod test_support {
    use agent_rig::tools::{ProgressDetails, ProgressReporter};
    use async_trait::async_trait;

    /// A [`ProgressReporter`] that drops every update, for tool tests that do
    /// not exercise mid-call progress reporting.
    pub(crate) struct NoopProgress;

    #[async_trait]
    impl ProgressReporter for NoopProgress {
        async fn update(&self, _details: ProgressDetails) {}
    }
}
