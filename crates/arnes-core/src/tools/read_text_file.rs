// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{ToolContext, ToolKind};

use super::Tool;
use agent_rig::tools::{Tool as RigTool, ToolDefinition, ToolResult};
use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    definition: ToolDefinition,
}

impl ReadTextFile {
    pub fn new(context: ToolContext) -> Self {
        Self {
            context,
            definition: ToolDefinition {
                name: NAME.to_string(),
                description: DESCRIPTION.to_string(),
                parameters: schema_for!(ReadTextFileParams),
            },
        }
    }
}

#[async_trait]
impl RigTool for ReadTextFile {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    fn title(&self, args: &Value) -> String {
        match serde_json::from_value::<ReadTextFileParams>(args.clone()) {
            Ok(args) => match args.path.to_str() {
                Some(path) => format!("Read {}", path),
                None => "Read".to_string(),
            },
            // Unparseable args: fall back to the tool name.
            Err(_) => NAME.to_string(),
        }
    }

    fn requires_approval(&self, _args: &Value) -> bool {
        true
    }

    // `propose` is left as the default — the proposal is the raw args — so
    // `apply` decodes straight into `ReadTextFileParams`.
    async fn apply(&self, proposal: Value, _cancel: CancellationToken) -> ToolResult {
        let args: ReadTextFileParams = match serde_json::from_value(proposal) {
            Ok(args) => args,
            Err(e) => return ToolResult::error(format!("invalid tool arguments: {e}")),
        };

        if let Ok(mut read_grants) = self.context.read_grants.lock() {
            read_grants.insert(args.path.clone());
        }

        let Some(host) = self.context.host.read_text_file.as_ref() else {
            return ToolResult::error("Capability unavailable");
        };
        let path = if args.path.is_relative() {
            self.context.cwd.join(&args.path)
        } else {
            args.path.clone()
        };
        // Always return Ok so the model receives a valid JSON object.
        // (Gemini requires FunctionResponse.response to be an object; a bare
        // string causes an empty/null candidate and a silent non-response.)
        let output = match host.read_text_file(&path, args.line, args.limit).await {
            Ok(content) => ReadTextFileOutput {
                content: Some(content),
                error: None,
            },
            Err(e) => ReadTextFileOutput {
                content: None,
                error: Some(format!("Failed to read '{}': {}", path.display(), e)),
            },
        };
        match serde_json::to_value(output) {
            Ok(value) => ToolResult::ok(value),
            Err(e) => ToolResult::error(format!("failed to serialize tool result: {e}")),
        }
    }
}

impl Tool for ReadTextFile {
    fn prompt_guidelines(&self) -> &str {
        "Use `read_text_file` to inspect file contents before editing. \
         Prefer `line` and `limit` when the file is large; both are 1-indexed \
         and bound the returned slice."
    }

    fn prompt_snippet(&self) -> &str {
        "read_text_file(path, line?, limit?) -> file contents"
    }

    fn tool_kind(&self) -> ToolKind {
        ToolKind::Read
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        io,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{AgentId, Host, ToolContext, host::ReadTextFile as ReadTextFileTrait};

    /// Drives the tool through its JSON `apply` surface with typed args,
    /// decoding the typed output — the path a real tool call takes.
    async fn run(
        tool: &ReadTextFile,
        args: ReadTextFileParams,
    ) -> Result<ReadTextFileOutput, Value> {
        let proposal = serde_json::to_value(args).unwrap();
        match tool.apply(proposal, CancellationToken::new()).await {
            ToolResult::Ok(value) => Ok(serde_json::from_value(value).unwrap()),
            ToolResult::Err(error) => Err(error),
        }
    }

    struct CapturingHost {
        called_with: Mutex<Option<PathBuf>>,
    }

    #[async_trait]
    impl ReadTextFileTrait for CapturingHost {
        async fn read_text_file(
            &self,
            path: &Path,
            _line: Option<usize>,
            _limit: Option<usize>,
        ) -> io::Result<String> {
            *self.called_with.lock().unwrap() = Some(path.to_path_buf());
            Ok(String::new())
        }
    }

    /// Serves file contents from an in-memory map, applying the same
    /// 1-indexed `line`/`limit` slicing the real host backends do.
    struct MapHost {
        files: std::collections::HashMap<PathBuf, String>,
    }

    #[async_trait]
    impl ReadTextFileTrait for MapHost {
        async fn read_text_file(
            &self,
            path: &Path,
            line: Option<usize>,
            limit: Option<usize>,
        ) -> io::Result<String> {
            let content = self.files.get(path).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("not found: {path:?}"))
            })?;
            let lines: Vec<&str> = content.lines().collect();
            let start = line
                .map(|n| n.saturating_sub(1))
                .unwrap_or(0)
                .min(lines.len());
            let slice = &lines[start..];
            let slice = match limit {
                Some(lim) => &slice[..slice.len().min(lim)],
                None => slice,
            };
            Ok(slice.join("\n"))
        }
    }

    fn map_tool(files: &[(&str, &str)]) -> ReadTextFile {
        let files = files
            .iter()
            .map(|(p, c)| (PathBuf::from(p), (*c).to_string()))
            .collect();
        let context = ToolContext {
            host: Host {
                read_text_file: Some(Arc::new(MapHost { files })),
                ..Host::default()
            },
            progress: None,
            agent_id: AgentId::Root,
            cwd: PathBuf::from("/"),
            read_grants: Arc::new(Mutex::new(HashSet::new())),
        };
        ReadTextFile::new(context)
    }

    #[tokio::test]
    async fn happy_path_returns_content() {
        let tool = map_tool(&[("/notes.txt", "hello world")]);
        let args = ReadTextFileParams {
            path: PathBuf::from("/notes.txt"),
            line: None,
            limit: None,
        };
        let out = run(&tool, args).await.unwrap();
        assert_eq!(out.content.as_deref(), Some("hello world"));
        assert!(out.error.is_none());
    }

    #[tokio::test]
    async fn missing_file_returns_error_output() {
        let tool = map_tool(&[("/present.txt", "here")]);
        let args = ReadTextFileParams {
            path: PathBuf::from("/absent.txt"),
            line: None,
            limit: None,
        };
        let out = run(&tool, args).await.unwrap();
        assert!(out.content.is_none());
        assert!(out.error.is_some(), "missing file should populate error");
    }

    #[tokio::test]
    async fn capability_unavailable_returns_err() {
        let context = ToolContext {
            host: Host::default(),
            progress: None,
            agent_id: AgentId::Root,
            cwd: PathBuf::from("/"),
            read_grants: Arc::new(Mutex::new(HashSet::new())),
        };
        let tool = ReadTextFile::new(context);
        let args = ReadTextFileParams {
            path: PathBuf::from("/anything.txt"),
            line: None,
            limit: None,
        };
        let result = run(&tool, args).await;
        assert!(
            result.is_err(),
            "tool should error when the capability is absent"
        );
    }

    #[test]
    fn title_includes_the_path() {
        let tool = map_tool(&[]);
        let args = ReadTextFileParams {
            path: PathBuf::from("/notes.txt"),
            line: None,
            limit: None,
        };
        let title = tool.title(&serde_json::to_value(args).unwrap());
        assert_eq!(title, "Read /notes.txt");
    }

    #[tokio::test]
    async fn read_grants_the_path_for_editing() {
        let tool = map_tool(&[("/notes.txt", "hello world")]);
        let args = ReadTextFileParams {
            path: PathBuf::from("/notes.txt"),
            line: None,
            limit: None,
        };
        run(&tool, args).await.unwrap();
        // A successful read authorizes a later edit of the same path.
        assert!(
            tool.context
                .read_grants
                .lock()
                .unwrap()
                .contains(&PathBuf::from("/notes.txt"))
        );
    }

    #[tokio::test]
    async fn line_and_limit_slice_the_file() {
        let tool = map_tool(&[("/multi.txt", "one\ntwo\nthree\nfour\nfive")]);
        let args = ReadTextFileParams {
            path: PathBuf::from("/multi.txt"),
            line: Some(2),
            limit: Some(2),
        };
        let out = run(&tool, args).await.unwrap();
        assert_eq!(out.content.as_deref(), Some("two\nthree"));
    }

    fn make_tool(cwd: PathBuf) -> (ReadTextFile, Arc<CapturingHost>) {
        let capturing = Arc::new(CapturingHost {
            called_with: Mutex::new(None),
        });
        let context = ToolContext {
            host: Host {
                read_text_file: Some(Arc::clone(&capturing) as Arc<dyn ReadTextFileTrait>),
                ..Host::default()
            },
            progress: None,
            agent_id: AgentId::Root,
            cwd,
            read_grants: Arc::new(Mutex::new(HashSet::new())),
        };
        (ReadTextFile::new(context), capturing)
    }

    #[tokio::test]
    async fn relative_path_resolves_against_cwd() {
        let cwd = std::env::temp_dir();
        let (tool, capturing) = make_tool(cwd.clone());
        let args = ReadTextFileParams {
            path: PathBuf::from("subdir/file.txt"),
            line: None,
            limit: None,
        };
        let _ = run(&tool, args).await.unwrap();
        let called = capturing.called_with.lock().unwrap().clone().unwrap();
        assert_eq!(called, cwd.join("subdir/file.txt"));
    }

    #[tokio::test]
    async fn absolute_path_is_not_rebased() {
        let cwd = std::env::temp_dir();
        let (tool, capturing) = make_tool(cwd.clone());
        let abs_path = cwd.join("absolute.txt");
        let args = ReadTextFileParams {
            path: abs_path.clone(),
            line: None,
            limit: None,
        };
        let _ = run(&tool, args).await.unwrap();
        let called = capturing.called_with.lock().unwrap().clone().unwrap();
        assert_eq!(called, abs_path);
    }
}
