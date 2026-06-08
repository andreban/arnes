// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use agent_rig::{
    model::TokenUsage,
    runner::{AgentEvent, ToolCallResult},
};
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::{
    AgentId, ContentBlock, CoreError, EventKind, Frontend, Message, Result, SessionEvent,
    StopReason, TokenCounts, ToolCallOutcome, TurnUsage,
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
                    tokens += to_counts(u);
                }
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
                AgentEvent::ToolCallStarted { name, args } => {
                    tracing::debug!(tool = %name, "prompt: tool call started");
                    self.frontend
                        .on_event(mk_event(EventKind::ToolCallStarted { name, args }))
                        .await;
                }
                AgentEvent::ToolCallFinished { name, result } => {
                    tracing::debug!(tool = %name, result = ?result, "prompt: tool call finished");
                    self.frontend
                        .on_event(mk_event(EventKind::ToolCallFinished {
                            name,
                            outcome: to_outcome(result),
                        }))
                        .await;
                }
                AgentEvent::EndTurn { thread } => {
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

fn to_outcome(result: ToolCallResult) -> ToolCallOutcome {
    match result {
        ToolCallResult::Ok(value) => ToolCallOutcome::Ok(value),
        ToolCallResult::Err(err) => ToolCallOutcome::Err(err.to_string()),
        ToolCallResult::Denied => ToolCallOutcome::Denied,
        ToolCallResult::Unknown => ToolCallOutcome::Unknown,
    }
}

fn to_counts(u: TokenUsage) -> TokenCounts {
    TokenCounts {
        total: u64::from(u.input_tokens.unwrap_or(0))
            + u64::from(u.output_tokens.unwrap_or(0))
            + u64::from(u.thinking_tokens.unwrap_or(0))
            + u64::from(u.tool_use_prompt_tokens.unwrap_or(0)),
    }
}
