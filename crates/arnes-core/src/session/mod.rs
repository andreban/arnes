// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use agent_rig::{Agent, model::LlmModel, runner::AgentRunner};

use crate::{CumulativeUsage, Frontend, Host, Message, ModelKey};

mod prompt;

/// The single entry point a frontend talks to.
pub struct Session<F: Frontend, H: Host> {
    pub(super) frontend: Arc<F>,
    #[allow(dead_code)]
    pub(super) host: Arc<H>,
    pub(super) runner: AgentRunner,
    pub(super) agent: Agent,
    pub(super) history: Vec<Message>,
    pub(super) cumulative_usage: CumulativeUsage,
    pub(super) model: ModelKey,
}

impl<F: Frontend, H: Host> Session<F, H> {
    pub fn new(frontend: Arc<F>, host: Arc<H>, llm: Arc<dyn LlmModel>, model: ModelKey) -> Self {
        let runner = AgentRunner::new(llm);
        let agent = Agent::builder()
            .name("arnes")
            .instructions("You are a helpful assistant.")
            .build();
        Self {
            frontend,
            host,
            runner,
            agent,
            history: Vec::new(),
            cumulative_usage: CumulativeUsage::default(),
            model,
        }
    }

    pub fn cumulative_usage(&self) -> &CumulativeUsage {
        &self.cumulative_usage
    }
}
