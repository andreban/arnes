// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{collections::VecDeque, sync::Mutex};

use agent_rig::{
    error::Error,
    model::{LlmModel, ModelRequest, ModelResponse, ToolCall},
};
use async_trait::async_trait;
use serde_json::Value;

pub enum ScriptedTurn {
    Text(String),
    ToolCall {
        name: String,
        args: Value,
    },
    ToolCallThenText {
        name: String,
        args: Value,
        follow_up: String,
    },
}

enum GenerateTurn {
    Text(String),
    ToolCall { name: String, args: Value },
}

pub struct ScriptedLlm {
    turns: Mutex<VecDeque<GenerateTurn>>,
}

impl ScriptedLlm {
    pub fn new(turns: Vec<ScriptedTurn>) -> Self {
        let mut queue = VecDeque::new();
        for turn in turns {
            match turn {
                ScriptedTurn::Text(t) => queue.push_back(GenerateTurn::Text(t)),
                ScriptedTurn::ToolCall { name, args } => {
                    queue.push_back(GenerateTurn::ToolCall { name, args })
                }
                ScriptedTurn::ToolCallThenText {
                    name,
                    args,
                    follow_up,
                } => {
                    queue.push_back(GenerateTurn::ToolCall { name, args });
                    queue.push_back(GenerateTurn::Text(follow_up));
                }
            }
        }
        Self {
            turns: Mutex::new(queue),
        }
    }
}

#[async_trait]
impl LlmModel for ScriptedLlm {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, Error> {
        let turn = self
            .turns
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| GenerateTurn::Text("[scripted response exhausted]".into()));
        match turn {
            GenerateTurn::Text(text) => Ok(ModelResponse {
                text: Some(text),
                tool_calls: vec![],
                thinking: None,
                token_usage: None,
            }),
            GenerateTurn::ToolCall { name, args } => Ok(ModelResponse {
                text: None,
                tool_calls: vec![ToolCall::new("scripted-tc".into(), name, args)],
                thinking: None,
                token_usage: None,
            }),
        }
    }
}
