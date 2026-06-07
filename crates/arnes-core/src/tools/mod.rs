mod read_text_file;

use agent_rig::tools::Tool as AgentRigTool;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

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
}
