// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use agent_rig::{model::TokenUsage, runner::AgentEvent};
use futures_util::StreamExt;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    AgentId, ContentBlock, CoreError, EventKind, Frontend, Message, Permission, PermissionRequest,
    Result, StopReason, TokenCounts, ToolCallOutcome, TurnUsage, frontend::ToolCallUpdate,
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
        self.frontend.on_event(EventKind::TurnStart).await;

        let mut stream = self
            .runner
            .run_with_cancellation(&self.agent, thread, cancel);

        let mut blocks: Vec<ContentBlock> = Vec::new();
        let mut tokens = TokenCounts::default();
        let mut stop_reason = StopReason::EndTurn;

        while let Some(ev) = stream.next().await {
            match ev.agent_event {
                AgentEvent::TextDelta(text) => {
                    self.frontend
                        .on_event(EventKind::TextDelta { text: text.clone() })
                        .await;
                    append_block(&mut blocks, text, false);
                }
                AgentEvent::ThinkingDelta(text) => {
                    self.frontend
                        .on_event(EventKind::ThinkingDelta { text: text.clone() })
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
                        .on_event(EventKind::Error {
                            message: e.to_string(),
                        })
                        .await;
                    return Err(CoreError::Session(e.to_string()));
                }
                AgentEvent::ToolCall(req) => {
                    tracing::debug!(tool = %req.details.name, "prompt: tool call");
                    self.frontend
                        .on_event(EventKind::ToolCallStarted {
                            id: req.details.id.clone(),
                            name: req.details.name.clone(),
                            args: req.details.args.clone(),
                            title: req.details.name.clone(),
                        })
                        .await;
                    let outcome = self
                        .tool_registry
                        .call(
                            &req,
                            |request| self.request_permission(request),
                            |update| self.tool_update(update),
                        )
                        .await;
                    tracing::debug!(tool = %req.details.name, outcome = ?outcome, "prompt: tool call finished");
                    self.frontend
                        .on_event(EventKind::ToolCallFinished {
                            id: req.details.id.clone(),
                            name: req.details.name.clone(),
                            outcome: outcome.clone(),
                        })
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
            .on_event(EventKind::TurnEnd { stop_reason, usage })
            .await;
        Ok(())
    }

    /// Asks the frontend to approve a gated tool call, reporting whether it was
    /// granted.
    async fn request_permission(&self, request: PermissionRequest) -> bool {
        matches!(
            self.frontend.request_permission(request).await,
            Permission::AllowOnce
        )
    }

    async fn tool_update(&self, update: ToolCallUpdate) {
        self.frontend
            .on_event(EventKind::ToolCallUpdated(update))
            .await;
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
