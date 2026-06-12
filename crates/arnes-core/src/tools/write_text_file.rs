// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{ToolContext, ToolKind};

use super::Tool;
use agent_rig::error::Error as AgentRigError;
use agent_rig::tools::{Tool as AgentRigTool, ToolDefinition};
use async_trait::async_trait;
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
}

#[async_trait]
impl AgentRigTool<WriteTextFileParams, WriteTextFileOutput> for WriteTextFile {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    fn title(&self, args: &WriteTextFileParams) -> String {
        match args.path.to_str() {
            Some(path) => format!("Write {}", path),
            None => "Write".to_string(),
        }
    }

    async fn call(
        &self,
        args: WriteTextFileParams,
        _cancellation: CancellationToken,
    ) -> Result<WriteTextFileOutput, AgentRigError> {
        let host = self
            .context
            .host
            .write_text_file
            .as_ref()
            .ok_or(AgentRigError::Agent("Capability unavailable".to_string()))?;
        let path = if args.path.is_relative() {
            self.context.cwd.join(&args.path)
        } else {
            args.path.clone()
        };
        // Always return Ok so the model receives a valid JSON object.
        // (Gemini requires FunctionResponse.response to be an object; a bare
        // string causes an empty/null candidate and a silent non-response.)
        match host.write_text_file(&path, &args.content).await {
            Ok(()) => Ok(WriteTextFileOutput {
                written: true,
                error: None,
            }),
            Err(e) => Ok(WriteTextFileOutput {
                written: false,
                error: Some(format!("Failed to write '{}': {}", path.display(), e)),
            }),
        }
    }
}

#[async_trait]
impl Tool<WriteTextFileParams, WriteTextFileOutput> for WriteTextFile {
    fn prompt_guidelines(&self) -> &str {
        "Use `write_text_file` to create or overwrite a file with the given content. \
         The entire content replaces any existing file; there is no append mode."
    }

    fn prompt_snippet(&self) -> &str {
        "write_text_file(path, content) -> written status"
    }

    fn permission_required(&self) -> bool {
        true
    }

    fn tool_kind(&self) -> ToolKind {
        ToolKind::Edit
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{AgentId, Host, ToolContext, host::WriteTextFile as WriteTextFileTrait};

    struct CapturingHost {
        called_with: Mutex<Option<(PathBuf, String)>>,
    }

    #[async_trait]
    impl WriteTextFileTrait for CapturingHost {
        async fn write_text_file(&self, path: &Path, content: &str) -> io::Result<()> {
            *self.called_with.lock().unwrap() = Some((path.to_path_buf(), content.to_owned()));
            Ok(())
        }
    }

    struct FailingHost;

    #[async_trait]
    impl WriteTextFileTrait for FailingHost {
        async fn write_text_file(&self, _path: &Path, _content: &str) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "permission denied",
            ))
        }
    }

    fn make_tool_with_host(
        cwd: PathBuf,
        host_impl: impl WriteTextFileTrait + 'static,
    ) -> WriteTextFile {
        let context = ToolContext {
            host: Host {
                write_text_file: Some(Arc::new(host_impl)),
                ..Host::default()
            },
            progress: None,
            agent_id: AgentId::Root,
            cwd,
        };
        WriteTextFile::new(context)
    }

    fn capturing_tool(cwd: PathBuf) -> (WriteTextFile, Arc<CapturingHost>) {
        let host = Arc::new(CapturingHost {
            called_with: Mutex::new(None),
        });
        let context = ToolContext {
            host: Host {
                write_text_file: Some(Arc::clone(&host) as Arc<dyn WriteTextFileTrait>),
                ..Host::default()
            },
            progress: None,
            agent_id: AgentId::Root,
            cwd,
        };
        (WriteTextFile::new(context), host)
    }

    #[tokio::test]
    async fn happy_path_returns_written_true() {
        let tool = make_tool_with_host(
            PathBuf::from("/"),
            CapturingHost {
                called_with: Mutex::new(None),
            },
        );
        let args = WriteTextFileParams {
            path: PathBuf::from("/output.txt"),
            content: "hello world".to_string(),
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(out.written);
        assert!(out.error.is_none());
    }

    #[tokio::test]
    async fn host_error_becomes_output_error() {
        let tool = make_tool_with_host(PathBuf::from("/"), FailingHost);
        let args = WriteTextFileParams {
            path: PathBuf::from("/protected.txt"),
            content: "content".to_string(),
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(!out.written);
        assert!(
            out.error.is_some(),
            "host error should populate error field"
        );
    }

    #[tokio::test]
    async fn capability_unavailable_returns_err() {
        let context = ToolContext {
            host: Host::default(),
            progress: None,
            agent_id: AgentId::Root,
            cwd: PathBuf::from("/"),
        };
        let tool = WriteTextFile::new(context);
        let args = WriteTextFileParams {
            path: PathBuf::from("/anything.txt"),
            content: "content".to_string(),
        };
        let result = tool.call(args, CancellationToken::new()).await;
        assert!(
            result.is_err(),
            "tool should error when the capability is absent"
        );
    }

    #[test]
    fn title_includes_the_path() {
        let tool = make_tool_with_host(
            PathBuf::from("/"),
            CapturingHost {
                called_with: Mutex::new(None),
            },
        );
        let args = WriteTextFileParams {
            path: PathBuf::from("/output.txt"),
            content: String::new(),
        };
        assert_eq!(tool.title(&args), "Write /output.txt");
    }

    #[tokio::test]
    async fn relative_path_resolves_against_cwd() {
        let cwd = std::env::temp_dir();
        let (tool, capturing) = capturing_tool(cwd.clone());
        let args = WriteTextFileParams {
            path: PathBuf::from("subdir/out.txt"),
            content: "data".to_string(),
        };
        let _ = tool.call(args, CancellationToken::new()).await.unwrap();
        let (called_path, _) = capturing.called_with.lock().unwrap().clone().unwrap();
        assert_eq!(called_path, cwd.join("subdir/out.txt"));
    }

    #[tokio::test]
    async fn absolute_path_is_not_rebased() {
        let cwd = std::env::temp_dir();
        let (tool, capturing) = capturing_tool(cwd.clone());
        let abs_path = cwd.join("absolute.txt");
        let args = WriteTextFileParams {
            path: abs_path.clone(),
            content: "data".to_string(),
        };
        let _ = tool.call(args, CancellationToken::new()).await.unwrap();
        let (called_path, _) = capturing.called_with.lock().unwrap().clone().unwrap();
        assert_eq!(called_path, abs_path);
    }

    #[tokio::test]
    async fn content_is_forwarded_verbatim() {
        let cwd = PathBuf::from("/");
        let (tool, capturing) = capturing_tool(cwd);
        let args = WriteTextFileParams {
            path: PathBuf::from("/out.txt"),
            content: "line1\nline2\n".to_string(),
        };
        let _ = tool.call(args, CancellationToken::new()).await.unwrap();
        let (_, written_content) = capturing.called_with.lock().unwrap().clone().unwrap();
        assert_eq!(written_content, "line1\nline2\n");
    }
}
