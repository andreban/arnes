// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    PermissionRequest, ToolContext, ToolKind, frontend::ToolCallUpdate, helpers::normalize_path,
};

use agent_rig::tools::{ToolDefinition, ToolResult};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

const NAME: &str = "edit_text_file";
const DESCRIPTION: &str = "Replaces anchor text in an existing file. Each edit names an \
                           `old_text` that must appear exactly once in the current file and \
                           the `new_text` to put in its place. Use this for targeted changes; \
                           use `write_text_file` to replace a file's entire contents.";

/// A single anchor-text replacement.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct EditTextFileOp {
    /// Text to find in the original file. Must match exactly once.
    pub old_text: String,
    /// Text to substitute for `old_text`.
    pub new_text: String,
}

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct EditTextFileParams {
    pub path: PathBuf,
    pub edits: Vec<EditTextFileOp>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EditTextFileOutput {
    pub edits_applied: usize,
}

/// The concrete change [`EditTextFile::propose`] resolved — the target path and
/// the file's contents before and after — handed verbatim to
/// [`EditTextFile::apply`], so the authorized change and the written change are
/// one value. The same proposal feeds the approval prompt: a frontend reads it
/// back with [`from_proposal`](Self::from_proposal) to render a before/after
/// diff.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditTextFileProposal {
    pub path: PathBuf,
    pub old_content: String,
    pub new_content: String,
    /// How many edits [`apply`](EditTextFile::apply) reports; not part of the
    /// diff a frontend shows.
    edits_applied: usize,
}

impl EditTextFileProposal {
    /// Reads an [`EditTextFileProposal`] out of a tool's opaque proposal,
    /// returning `None` when the proposal does not describe a file edit.
    pub fn from_proposal(proposal: &Value) -> Option<Self> {
        serde_json::from_value(proposal.clone()).ok()
    }
}

impl From<&EditTextFileProposal> for Value {
    fn from(value: &EditTextFileProposal) -> Self {
        serde_json::to_value(value).expect("Serialization failed")
    }
}

pub struct EditTextFile {
    context: ToolContext,
    definition: ToolDefinition,
}

impl EditTextFile {
    pub fn new(context: ToolContext) -> Self {
        Self {
            context,
            definition: ToolDefinition {
                name: NAME.to_string(),
                description: DESCRIPTION.to_string(),
                parameters: schema_for!(EditTextFileParams),
            },
        }
    }

    /// Applies every edit against `original` and returns the assembled file.
    ///
    /// All anchors match the original content, never the text a prior edit
    /// produced. Validation is complete before assembly, so an error means
    /// nothing changed.
    fn apply_edits(original: &str, edits: &[EditTextFileOp]) -> Result<String, String> {
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

    pub fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    pub async fn call<F, Fut, U, UFut>(
        &self,
        args: Value,
        tool_call_id: String,
        request_permission: F,
        update_toolcall: U,
        _cancel: CancellationToken,
    ) -> ToolResult
    where
        F: Fn(PermissionRequest) -> Fut,
        Fut: Future<Output = bool>,
        U: Fn(ToolCallUpdate) -> UFut,
        UFut: Future<Output = ()>,
    {
        let (Some(read_host), Some(write_host)) = (
            self.context.host.read_text_file.as_ref(),
            self.context.host.write_text_file.as_ref(),
        ) else {
            return ToolResult::error("Capability unavailable");
        };

        let params: EditTextFileParams = match serde_json::from_value(args.clone()) {
            Ok(params) => params,
            Err(e) => return ToolResult::error(format!("invalid tool arguments: {e}")),
        };

        let title = format!("Editing file {}", &params.path.display());
        update_toolcall(ToolCallUpdate {
            tool_call_id: tool_call_id.clone(),
            tool_name: NAME.to_string(),
            args: args.clone(),
            title,
        })
        .await;

        match self.context.read_grants.lock() {
            Ok(grants) => {
                if !grants.contains(&params.path) {
                    return ToolResult::error(format!(
                        "File {} must be read before it can be edited",
                        params.path.to_string_lossy()
                    ));
                }
            }
            Err(e) => return ToolResult::error(format!("Error reading grants: {e}")),
        }

        let path = normalize_path(&self.context.cwd, &params.path);

        let original = match read_host.read_text_file(&path, None, None).await {
            Ok(original) => original,
            Err(e) => {
                return ToolResult::error(format!("Failed to read '{}': {}", path.display(), e));
            }
        };
        let new_content = match Self::apply_edits(&original, &params.edits) {
            Ok(new_content) => new_content,
            Err(e) => return ToolResult::error(e),
        };

        let proposal = EditTextFileProposal {
            edits_applied: params.edits.len(),
            path,
            old_content: original,
            new_content,
        };

        let request = PermissionRequest {
            tool_call_id,
            tool_name: NAME.to_string(),
            args,
            kind: ToolKind::Edit,
            proposal: (&proposal).into(),
        };
        if !request_permission(request).await {
            return ToolResult::error("User rejected tool call");
        }

        if let Err(e) = write_host
            .write_text_file(&proposal.path, &proposal.new_content)
            .await
        {
            return ToolResult::error(format!(
                "Failed to write '{}': {}",
                proposal.path.display(),
                e
            ));
        }
        let output = EditTextFileOutput {
            edits_applied: proposal.edits_applied,
        };
        match serde_json::to_value(output) {
            Ok(value) => ToolResult::ok(value),
            Err(e) => ToolResult::error(format!("failed to serialize tool result: {e}")),
        }
    }

    pub fn prompt_guidelines(&self) -> &str {
        "Use `edit_text_file` to change part of an existing file by anchor text. Each edit's \
         `old_text` must appear exactly once in the current file; pick an anchor with \
         enough surrounding context to be unique. All edits match the original file, \
         not the text earlier edits produce, and the whole batch is applied together or \
         not at all."
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
