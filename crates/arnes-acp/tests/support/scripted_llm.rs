// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{collections::VecDeque, sync::Mutex};

use agent_rig::{
    error::Error,
    model::{LlmModel, ModelRequest, ModelResponse},
};
use async_trait::async_trait;

pub struct ScriptedLlm {
    turns: Mutex<VecDeque<String>>,
}

impl ScriptedLlm {
    pub fn new(turns: Vec<String>) -> Self {
        Self {
            turns: Mutex::new(turns.into_iter().collect()),
        }
    }
}

#[async_trait]
impl LlmModel for ScriptedLlm {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, Error> {
        let text = self
            .turns
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| "[scripted response exhausted]".into());
        Ok(ModelResponse {
            text: Some(text),
            tool_calls: vec![],
            thinking: None,
            token_usage: None,
        })
    }
}
