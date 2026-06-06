// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use agent_rig::{
    model::{Message as RigMessage, TokenUsage},
    runner::AgentEvent,
};
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::{
    AgentId, ContentBlock, CoreError, EventKind, Frontend, Host, Message, Result, SessionEvent,
    StopReason, TokenCounts, TurnUsage,
};

use super::Session;

impl<F: Frontend, H: Host> Session<F, H> {
    pub async fn prompt(
        &mut self,
        input: impl Into<String>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let input = input.into();
        self.history.push(Message::user_text(&input));

        let thread = to_rig_thread(&self.history);
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
                    stop_reason = StopReason::Cancelled;
                    break;
                }
                AgentEvent::Error(e) => {
                    self.frontend
                        .on_event(mk_event(EventKind::Error {
                            message: e.to_string(),
                        }))
                        .await;
                    return Err(CoreError::Session(e.to_string()));
                }
                AgentEvent::ToolCallStarted { .. } | AgentEvent::ToolCallFinished { .. } => {
                    // M2
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

fn to_counts(u: TokenUsage) -> TokenCounts {
    TokenCounts {
        total: u64::from(u.input_tokens.unwrap_or(0))
            + u64::from(u.output_tokens.unwrap_or(0))
            + u64::from(u.thinking_tokens.unwrap_or(0))
            + u64::from(u.tool_use_prompt_tokens.unwrap_or(0)),
    }
}

fn to_rig_thread(history: &[Message]) -> Vec<RigMessage> {
    history
        .iter()
        .filter_map(|m| match m {
            Message::User { content } => {
                let text: String = content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                Some(RigMessage::user(text))
            }
            Message::Assistant { content } => {
                let text: String = content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                Some(RigMessage::assistant(text))
            }
            Message::System { .. } => None,
        })
        .collect()
}
