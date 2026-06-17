// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use agent_rig::{
    model::TokenUsage,
    runner::AgentEvent,
    tools::{ToolCallRequest, ToolRegistry, ToolResult},
};
use futures_util::StreamExt;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    AgentId, ContentBlock, CoreError, EventKind, Frontend, Message, Permission, PermissionRequest,
    Result, SessionEvent, StopReason, TokenCounts, ToolCallOutcome, TurnUsage,
};

use super::Session;

impl<F: Frontend> Session<F> {
    /// Runs one turn against the model: appends `input` to the
    /// conversation, streams agent events to the frontend, and records
    /// the assistant's reply in history.
    ///
    /// Cancelling `cancel` ends the stream early with
    /// [`StopReason::Cancelled`]; the partial reply still lands in
    /// history.
    pub async fn prompt(
        &mut self,
        input: impl Into<String>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let input = input.into();
        tracing::debug!(
            input_len = input.len(),
            thread_len = self.rig_thread.len(),
            "prompt: starting turn"
        );
        self.history.push(Message::user_text(&input));

        let mut thread = self.rig_thread.clone();
        thread.push(agent_rig::model::Message::user(&input));
        self.frontend.on_event(mk_event(EventKind::TurnStart)).await;

        let mut stream = self
            .runner
            .run_with_cancellation(&self.agent, thread, cancel);

        // The runner now only announces tool calls; arnes resolves them against
        // its own registry. Clone the `Arc` so the borrows below are
        // independent of `&self`.
        let registry = self.tools.clone();

        let mut blocks: Vec<ContentBlock> = Vec::new();
        let mut tokens = TokenCounts::default();
        let mut stop_reason = StopReason::EndTurn;

        while let Some(ev) = stream.next().await {
            match ev.agent_event {
                AgentEvent::TextDelta(text) => {
                    self.frontend
                        .on_event(mk_event(EventKind::TextDelta { text: text.clone() }))
                        .await;
                    append_block(&mut blocks, text, false);
                }
                AgentEvent::ThinkingDelta(text) => {
                    self.frontend
                        .on_event(mk_event(EventKind::ThinkingDelta { text: text.clone() }))
                        .await;
                    append_block(&mut blocks, text, true);
                }
                AgentEvent::Usage(u) => {
                    tokens += u.into();
                }
                AgentEvent::TurnStart => {}
                AgentEvent::Cancelled => {
                    tracing::debug!("prompt: stream cancelled");
                    stop_reason = StopReason::Cancelled;
                    break;
                }
                AgentEvent::Error(e) => {
                    tracing::error!(error = %e, "prompt: agent error");
                    self.frontend
                        .on_event(mk_event(EventKind::Error {
                            message: e.to_string(),
                        }))
                        .await;
                    return Err(CoreError::Session(e.to_string()));
                }
                AgentEvent::ToolCall(req) => {
                    tracing::debug!(tool = %req.tool_name, "prompt: tool call");
                    let outcome = self.resolve_tool_call(&registry, &req).await;
                    tracing::debug!(tool = %req.tool_name, outcome = ?outcome, "prompt: tool call finished");
                    self.frontend
                        .on_event(mk_event(EventKind::ToolCallFinished {
                            id: req.tool_call_id.clone(),
                            name: req.tool_name.clone(),
                            outcome: outcome.clone(),
                        }))
                        .await;
                    req.resolve(outcome);
                }
                AgentEvent::TurnFinish { thread } => {
                    tracing::debug!(
                        thread_len = thread.len(),
                        "prompt: EndTurn received, persisting thread"
                    );
                    self.rig_thread = thread;
                }
            }
        }

        let usage = TurnUsage {
            agent_id: AgentId::Root,
            model: self.model.clone(),
            tokens,
        };
        self.cumulative_usage.record(usage.clone());
        self.history.push(Message::Assistant { content: blocks });
        tracing::debug!(stop_reason = ?stop_reason, "prompt: emitting TurnEnd");
        self.frontend
            .on_event(mk_event(EventKind::TurnEnd { stop_reason, usage }))
            .await;
        Ok(())
    }

    /// Resolves one tool call the runner announced: looks the tool up, emits
    /// [`EventKind::ToolCallStarted`], then runs the propose / approval / apply
    /// flow that agent-rig used to drive internally. The returned outcome is
    /// what the caller reports as finished and hands back to the runner.
    async fn resolve_tool_call(
        &self,
        registry: &ToolRegistry,
        req: &ToolCallRequest,
    ) -> ToolCallOutcome {
        let tool = registry.get(&req.tool_name);

        // A descriptive title for the start event; fall back to the tool name
        // when the tool is unknown. A tool that can't interpret its arguments
        // falls back to its own name inside `title`.
        let title = tool
            .map(|t| t.title(&req.args))
            .unwrap_or_else(|| req.tool_name.clone());
        self.frontend
            .on_event(mk_event(EventKind::ToolCallStarted {
                id: req.tool_call_id.clone(),
                name: req.tool_name.clone(),
                args: req.args.clone(),
                title,
            }))
            .await;

        let Some(tool) = tool else {
            return ToolCallOutcome::Unknown;
        };

        // `propose` is side-effect-free and resolves the raw args into the
        // value both the approval prompt and `apply` read from.
        let proposal = match tool
            .propose(&req.args, req.cancellation_token.clone())
            .await
        {
            ToolResult::Ok(proposal) => proposal,
            ToolResult::Err(error) => return ToolCallOutcome::Err(error.to_string()),
        };

        if tool.requires_approval(&req.args) {
            let kind = self
                .tool_metadata
                .get(&req.tool_name)
                .copied()
                .unwrap_or_default();
            let permission = PermissionRequest {
                tool_call_id: req.tool_call_id.clone(),
                tool_name: req.tool_name.clone(),
                args: req.args.clone(),
                kind,
                proposal: proposal.clone(),
            };
            if !matches!(
                self.frontend.request_permission(permission).await,
                Permission::AllowOnce
            ) {
                return ToolCallOutcome::Denied;
            }
        }

        match tool.apply(proposal, req.cancellation_token.clone()).await {
            ToolResult::Ok(value) => ToolCallOutcome::Ok(value),
            ToolResult::Err(error) => ToolCallOutcome::Err(error.to_string()),
        }
    }
}

fn mk_event(kind: EventKind) -> SessionEvent {
    SessionEvent {
        agent_id: AgentId::Root,
        depth: 0,
        kind,
    }
}

fn append_block(blocks: &mut Vec<ContentBlock>, text: String, thinking: bool) {
    let extend = if thinking {
        matches!(blocks.last(), Some(ContentBlock::Thinking { .. }))
    } else {
        matches!(blocks.last(), Some(ContentBlock::Text { .. }))
    };

    if extend {
        match blocks.last_mut().unwrap() {
            ContentBlock::Text { text: t } | ContentBlock::Thinking { text: t } => {
                t.push_str(&text)
            }
            _ => unreachable!(),
        }
    } else if thinking {
        blocks.push(ContentBlock::Thinking { text });
    } else {
        blocks.push(ContentBlock::Text { text });
    }
}

impl From<ToolCallOutcome> for Value {
    fn from(value: ToolCallOutcome) -> Self {
        match value {
            ToolCallOutcome::Ok(value) => value,
            ToolCallOutcome::Err(err) => Value::from(format!("Tool call error: {err}")),
            ToolCallOutcome::Denied => Value::from("User rejected the tool call"),
            ToolCallOutcome::Unknown => Value::from("Unknown tool"),
        }
    }
}

impl From<TokenUsage> for TokenCounts {
    fn from(value: TokenUsage) -> Self {
        TokenCounts {
            total: u64::from(value.input_tokens.unwrap_or(0))
                + u64::from(value.output_tokens.unwrap_or(0))
                + u64::from(value.thinking_tokens.unwrap_or(0))
                + u64::from(value.tool_use_prompt_tokens.unwrap_or(0)),
        }
    }
}
