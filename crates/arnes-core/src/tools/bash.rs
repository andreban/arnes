// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use crate::{
    PermissionRequest, Terminal, TerminalHandle, TerminalSpec, ToolContext, ToolKind,
    frontend::ToolCallUpdate,
};

use agent_rig::{
    model::ToolCall,
    tools::{ToolDefinition, ToolResult},
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

const NAME: &str = "bash";
const DESCRIPTION: &str = "Runs a shell command with `bash -c` in the session's working \
                           directory and returns its combined stdout and stderr together with \
                           its exit code. The command is killed if it runs longer than \
                           `timeout_seconds` (default 120, maximum 600).";

const DEFAULT_TIMEOUT_SECONDS: u64 = 120;
const MAX_TIMEOUT_SECONDS: u64 = 600;

/// Output returned to the model. When a command prints more, only the most
/// recent output is kept, since errors and summaries tend to come last.
const OUTPUT_BYTE_LIMIT: usize = 32 * 1024;

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct BashParams {
    /// The command to run, interpreted by `bash -c`.
    pub command: String,
    /// Seconds to wait before killing the command. Defaults to 120; clamped to 1..=600.
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BashOutput {
    /// Combined stdout and stderr.
    pub output: String,
    /// Whether older output was dropped to fit the output limit.
    pub truncated: bool,
    /// The exit code, or `None` if the command was killed by a signal.
    pub exit_code: Option<i32>,
    /// The signal that terminated the command, if any.
    pub signal: Option<String>,
    /// Whether the command was killed for exceeding its timeout.
    pub timed_out: bool,
}

pub struct Bash {
    context: ToolContext,
    definition: ToolDefinition,
}

impl Bash {
    pub fn new(context: ToolContext) -> Self {
        Self {
            context,
            definition: ToolDefinition {
                name: NAME.to_string(),
                description: DESCRIPTION.to_string(),
                parameters: schema_for!(BashParams),
            },
        }
    }

    pub fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    pub async fn call<F, Fut, U, UFut>(
        &self,
        tool_call: &Arc<ToolCall>,
        request_permission: F,
        update_toolcall: U,
        // The session drops this future on cancellation; `ReleaseGuard`
        // then kills the command.
        _cancel: CancellationToken,
    ) -> ToolResult
    where
        F: Fn(PermissionRequest) -> Fut,
        Fut: Future<Output = bool>,
        U: Fn(ToolCallUpdate) -> UFut,
        UFut: Future<Output = ()>,
    {
        let params = match BashParams::deserialize(&tool_call.args) {
            Ok(args) => args,
            Err(e) => return ToolResult::error(format!("invalid tool arguments: {e}")),
        };
        if params.command.trim().is_empty() {
            return ToolResult::error("invalid tool arguments: `command` is empty");
        }
        update_toolcall(ToolCallUpdate {
            tool_call: tool_call.clone(),
            title: format!("Running `{}`", params.command),
        })
        .await;

        let request = PermissionRequest {
            tool_call: tool_call.clone(),
            kind: ToolKind::Execute,
            proposal: tool_call.args.clone(),
        };
        if !request_permission(request).await {
            return ToolResult::error("User rejected tool call");
        }
        let Some(terminal) = self.context.host.terminal.as_ref() else {
            return ToolResult::error("Capability unavailable");
        };

        let spec = TerminalSpec {
            command: "bash".into(),
            args: vec!["-c".into(), params.command.clone()],
            cwd: Some(self.context.cwd.clone()),
            output_byte_limit: Some(OUTPUT_BYTE_LIMIT),
            ..Default::default()
        };
        let handle = match terminal.terminal_create(spec).await {
            Ok(handle) => handle,
            Err(e) => return ToolResult::error(format!("Failed to start command: {e}")),
        };
        let guard = ReleaseGuard {
            terminal: Arc::clone(terminal),
            handle: Some(handle.clone()),
        };

        let timeout = Duration::from_secs(
            params
                .timeout_seconds
                .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
                .clamp(1, MAX_TIMEOUT_SECONDS),
        );
        let waited = tokio::time::timeout(timeout, terminal.terminal_wait_for_exit(&handle)).await;
        let timed_out = match waited {
            Ok(Ok(_)) => false,
            Ok(Err(e)) => return ToolResult::error(format!("Failed waiting for command: {e}")),
            Err(_) => {
                tracing::debug!(command = %params.command, ?timeout, "bash: timed out");
                if let Err(e) = terminal.terminal_kill(&handle).await {
                    tracing::warn!(error = %e, "bash: kill after timeout failed");
                }
                // The kill is asynchronous; give the host a moment to record
                // the exit status before taking the final snapshot.
                let _ = tokio::time::timeout(
                    Duration::from_secs(5),
                    terminal.terminal_wait_for_exit(&handle),
                )
                .await;
                true
            }
        };

        let snapshot = match terminal.terminal_snapshot(&handle).await {
            Ok(snapshot) => snapshot,
            Err(e) => return ToolResult::error(format!("Failed to read command output: {e}")),
        };
        guard.release().await;

        let (output, cut) = tail(&snapshot.output, OUTPUT_BYTE_LIMIT);
        let exit_status = snapshot.exit_status.unwrap_or_default();
        let output = BashOutput {
            output: output.to_string(),
            truncated: snapshot.truncated || cut,
            exit_code: exit_status.exit_code,
            signal: exit_status.signal,
            timed_out,
        };
        match serde_json::to_value(output) {
            Ok(value) => ToolResult::ok(value),
            Err(e) => ToolResult::error(format!("failed to serialize tool result: {e}")),
        }
    }

    pub fn prompt_guidelines(&self) -> &str {
        "Use `bash` to run shell commands such as builds, tests, and git. Each call \
         runs in a fresh shell in the working directory, so `cd` and exported \
         variables do not carry over between calls. Commands cannot read stdin; pass \
         non-interactive flags. Prefer `read_text_file`, `write_text_file`, and \
         `edit_text_file` over shell equivalents for file contents. Long output is \
         truncated to its most recent part."
    }
}

/// Releases the terminal when the tool call ends, including when its future
/// is dropped on cancellation, so the command never outlives the call.
struct ReleaseGuard {
    terminal: Arc<dyn Terminal>,
    handle: Option<TerminalHandle>,
}

impl ReleaseGuard {
    async fn release(mut self) {
        if let Some(handle) = self.handle.take()
            && let Err(e) = self.terminal.terminal_release(handle).await
        {
            tracing::warn!(error = %e, "bash: terminal release failed");
        }
    }
}

impl Drop for ReleaseGuard {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        // `Drop` cannot await, so hand the release to the runtime.
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                let terminal = Arc::clone(&self.terminal);
                runtime.spawn(async move {
                    if let Err(e) = terminal.terminal_release(handle).await {
                        tracing::warn!(error = %e, "bash: terminal release failed");
                    }
                });
            }
            Err(_) => tracing::warn!(id = %handle.id, "bash: no runtime to release terminal"),
        }
    }
}

/// Returns the last `limit` bytes of `text`, starting on a char boundary,
/// and whether anything was cut.
fn tail(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut start = text.len() - limit;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    (&text[start..], true)
}

#[cfg(test)]
#[path = "bash_tests.rs"]
mod tests;
