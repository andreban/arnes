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

const NAME: &str = "edit";
const DESCRIPTION: &str = "Replaces anchor text in an existing file. Each edit names an \
                           `old_text` that must appear exactly once in the current file and \
                           the `new_text` to put in its place. Use this for targeted changes; \
                           use `write_text_file` to replace a file's entire contents.";

/// A single anchor-text replacement.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct EditOp {
    /// Text to find in the original file. Must match exactly once.
    pub old_text: String,
    /// Text to substitute for `old_text`.
    pub new_text: String,
}

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct EditParams {
    pub path: PathBuf,
    pub edits: Vec<EditOp>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EditOutput {
    pub applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits_applied: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct Edit {
    context: ToolContext,
    definition: ToolDefinition,
}

impl Edit {
    pub fn new(context: ToolContext) -> Self {
        Self {
            context,
            definition: ToolDefinition {
                name: NAME.to_string(),
                description: DESCRIPTION.to_string(),
                parameters: schema_for!(EditParams),
            },
        }
    }

    /// Applies every edit against `original` and returns the assembled file.
    ///
    /// All anchors match the original content, never the text a prior edit
    /// produced. Validation is complete before assembly, so an error means
    /// nothing changed.
    fn apply_edits(original: &str, edits: &[EditOp]) -> Result<String, String> {
        let mut spans: Vec<(usize, usize, &str)> = Vec::with_capacity(edits.len());
        for (i, edit) in edits.iter().enumerate() {
            let n = i + 1;
            if edit.old_text.is_empty() {
                return Err(format!("edit {n}: old_text must not be empty"));
            }
            match count_occurrences(original, &edit.old_text) {
                0 => return Err(format!("edit {n}: old_text not found")),
                1 => {}
                count => {
                    return Err(format!(
                        "edit {n}: old_text matched {count} times; \
                         provide a more specific anchor"
                    ));
                }
            }
            let start = original
                .find(&edit.old_text)
                .expect("a single occurrence guarantees a match");
            let end = start + edit.old_text.len();
            spans.push((start, end, &edit.new_text));
        }

        spans.sort_by_key(|&(start, ..)| start);
        for pair in spans.windows(2) {
            let (_, prev_end, _) = pair[0];
            let (next_start, ..) = pair[1];
            if next_start < prev_end {
                return Err("edits overlap; each anchor must match a disjoint region".to_string());
            }
        }

        let mut result = String::with_capacity(original.len());
        let mut cursor = 0;
        for (start, end, new_text) in spans {
            result.push_str(&original[cursor..start]);
            result.push_str(new_text);
            cursor = end;
        }
        result.push_str(&original[cursor..]);
        Ok(result)
    }
}

/// Counts non-overlapping occurrences of `needle` in `haystack`.
fn count_occurrences(haystack: &str, needle: &str) -> usize {
    let mut count = 0;
    let mut offset = 0;
    while let Some(pos) = haystack[offset..].find(needle) {
        count += 1;
        offset += pos + needle.len();
    }
    count
}

#[async_trait]
impl AgentRigTool<EditParams, EditOutput> for Edit {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    fn title(&self, args: &EditParams) -> String {
        match args.path.to_str() {
            Some(path) => format!("Edit {}", path),
            None => "Edit".to_string(),
        }
    }

    async fn call(
        &self,
        args: EditParams,
        _cancellation: CancellationToken,
    ) -> Result<EditOutput, AgentRigError> {
        let read_host = self
            .context
            .host
            .read_text_file
            .as_ref()
            .ok_or(AgentRigError::Agent("Capability unavailable".to_string()))?;
        let write_host = self
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
        let original = match read_host.read_text_file(&path, None, None).await {
            Ok(content) => content,
            Err(e) => {
                return Ok(EditOutput {
                    applied: false,
                    edits_applied: None,
                    error: Some(format!("Failed to read '{}': {}", path.display(), e)),
                });
            }
        };

        let new_content = match Self::apply_edits(&original, &args.edits) {
            Ok(content) => content,
            Err(error) => {
                return Ok(EditOutput {
                    applied: false,
                    edits_applied: None,
                    error: Some(error),
                });
            }
        };

        match write_host.write_text_file(&path, &new_content).await {
            Ok(()) => Ok(EditOutput {
                applied: true,
                edits_applied: Some(args.edits.len()),
                error: None,
            }),
            Err(e) => Ok(EditOutput {
                applied: false,
                edits_applied: None,
                error: Some(format!("Failed to write '{}': {}", path.display(), e)),
            }),
        }
    }
}

#[async_trait]
impl Tool<EditParams, EditOutput> for Edit {
    fn prompt_guidelines(&self) -> &str {
        "Use `edit` to change part of an existing file by anchor text. Each edit's \
         `old_text` must appear exactly once in the current file; pick an anchor with \
         enough surrounding context to be unique. All edits match the original file, \
         not the text earlier edits produce, and the whole batch is applied together or \
         not at all."
    }

    fn prompt_snippet(&self) -> &str {
        "edit(path, edits: [{old_text, new_text}]) -> applied status"
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
        collections::HashMap,
        io,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{
        AgentId, Host, ToolContext,
        host::{ReadTextFile as ReadTextFileTrait, WriteTextFile as WriteTextFileTrait},
    };

    /// Serves file contents from an in-memory map and captures the single
    /// write a successful edit performs.
    struct MapHost {
        files: HashMap<PathBuf, String>,
        written: Mutex<Option<(PathBuf, String)>>,
    }

    #[async_trait]
    impl ReadTextFileTrait for MapHost {
        async fn read_text_file(
            &self,
            path: &Path,
            _line: Option<usize>,
            _limit: Option<usize>,
        ) -> io::Result<String> {
            self.files.get(path).cloned().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("not found: {path:?}"))
            })
        }
    }

    #[async_trait]
    impl WriteTextFileTrait for MapHost {
        async fn write_text_file(&self, path: &Path, content: &str) -> io::Result<()> {
            *self.written.lock().unwrap() = Some((path.to_path_buf(), content.to_owned()));
            Ok(())
        }
    }

    fn tool_with(cwd: &str, files: &[(&str, &str)]) -> (Edit, Arc<MapHost>) {
        let files = files
            .iter()
            .map(|(p, c)| (PathBuf::from(p), (*c).to_string()))
            .collect();
        let host = Arc::new(MapHost {
            files,
            written: Mutex::new(None),
        });
        let context = ToolContext {
            host: Host {
                read_text_file: Some(Arc::clone(&host) as Arc<dyn ReadTextFileTrait>),
                write_text_file: Some(Arc::clone(&host) as Arc<dyn WriteTextFileTrait>),
                ..Host::default()
            },
            progress: None,
            agent_id: AgentId::Root,
            cwd: PathBuf::from(cwd),
        };
        (Edit::new(context), host)
    }

    fn op(old_text: &str, new_text: &str) -> EditOp {
        EditOp {
            old_text: old_text.to_string(),
            new_text: new_text.to_string(),
        }
    }

    #[tokio::test]
    async fn zero_match_errors_and_writes_nothing() {
        let (tool, host) = tool_with("/", &[("/f.txt", "hello world")]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            edits: vec![op("absent", "x")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(!out.applied);
        assert!(out.error.is_some());
        assert!(host.written.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn multi_match_errors_and_writes_nothing() {
        let (tool, host) = tool_with("/", &[("/f.txt", "ab ab")]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            edits: vec![op("ab", "x")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(!out.applied);
        assert!(out.error.unwrap().contains("matched 2 times"));
        assert!(host.written.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn empty_old_text_errors() {
        let (tool, host) = tool_with("/", &[("/f.txt", "content")]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            edits: vec![op("", "x")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(!out.applied);
        assert!(out.error.is_some());
        assert!(host.written.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn overlapping_edits_error() {
        let (tool, host) = tool_with("/", &[("/f.txt", "abcdef")]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            // "abc" spans [0,3); "cde" spans [2,5) — they intersect.
            edits: vec![op("abc", "X"), op("cde", "Y")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(!out.applied);
        assert!(out.error.unwrap().contains("overlap"));
        assert!(host.written.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn nested_edits_error() {
        let (tool, host) = tool_with("/", &[("/f.txt", "abcdef")]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            // "cd" spans [2,4), nested inside "bcde" spanning [1,5).
            edits: vec![op("bcde", "X"), op("cd", "Y")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(!out.applied);
        assert!(host.written.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn multi_edit_happy_path_applies_all() {
        let (tool, host) = tool_with("/", &[("/f.txt", "hello world")]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            edits: vec![op("hello", "hi"), op("world", "earth")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(out.applied);
        assert_eq!(out.edits_applied, Some(2));
        let (_, content) = host.written.lock().unwrap().clone().unwrap();
        assert_eq!(content, "hi earth");
    }

    #[tokio::test]
    async fn edits_match_original_not_prior_output() {
        // edit 1 produces "bar"; edit 2's anchor "bar" must not match it,
        // because matching is against the original ("foo baz"), which has no
        // "bar". The batch fails and nothing is written.
        let (tool, host) = tool_with("/", &[("/f.txt", "foo baz")]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            edits: vec![op("foo", "bar"), op("bar", "qux")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(!out.applied);
        assert!(host.written.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn one_failing_edit_leaves_write_uncalled() {
        let (tool, host) = tool_with("/", &[("/f.txt", "hello world")]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            edits: vec![op("hello", "hi"), op("absent", "x")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(!out.applied);
        assert!(host.written.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn relative_path_resolves_against_cwd() {
        let (tool, host) = tool_with("/base", &[("/base/sub/f.txt", "abc")]);
        let args = EditParams {
            path: PathBuf::from("sub/f.txt"),
            edits: vec![op("abc", "xyz")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(out.applied);
        let (called_path, _) = host.written.lock().unwrap().clone().unwrap();
        assert_eq!(called_path, PathBuf::from("/base/sub/f.txt"));
    }

    #[tokio::test]
    async fn absolute_path_is_not_rebased() {
        let (tool, host) = tool_with("/base", &[("/elsewhere/f.txt", "abc")]);
        let args = EditParams {
            path: PathBuf::from("/elsewhere/f.txt"),
            edits: vec![op("abc", "xyz")],
        };
        let out = tool.call(args, CancellationToken::new()).await.unwrap();
        assert!(out.applied);
        let (called_path, _) = host.written.lock().unwrap().clone().unwrap();
        assert_eq!(called_path, PathBuf::from("/elsewhere/f.txt"));
    }

    #[tokio::test]
    async fn capability_unavailable_returns_err() {
        let context = ToolContext {
            host: Host::default(),
            progress: None,
            agent_id: AgentId::Root,
            cwd: PathBuf::from("/"),
        };
        let tool = Edit::new(context);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            edits: vec![op("a", "b")],
        };
        assert!(tool.call(args, CancellationToken::new()).await.is_err());
    }

    #[test]
    fn title_includes_the_path() {
        let (tool, _) = tool_with("/", &[]);
        let args = EditParams {
            path: PathBuf::from("/f.txt"),
            edits: Vec::new(),
        };
        assert_eq!(tool.title(&args), "Edit /f.txt");
    }
}
