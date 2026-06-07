// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use agent_rig::{Agent, model::LlmModel, runner::AgentRunner, tools::ToolRegistry};

use crate::{
    AgentId, CumulativeUsage, Frontend, Host, Message, ModelKey, ToolContext,
    tools::{ReadTextFile, Tool},
};

mod prompt;

/// The single entry point a frontend talks to.
pub struct Session<F: Frontend> {
    pub(super) frontend: Arc<F>,
    #[allow(dead_code)]
    pub(super) host: Host,
    pub(super) runner: AgentRunner,
    pub(super) agent: Agent,
    pub(super) history: Vec<Message>,
    pub(super) cumulative_usage: CumulativeUsage,
    pub(super) model: ModelKey,
}

impl<F: Frontend> Session<F> {
    pub fn new(frontend: Arc<F>, host: Host, llm: Arc<dyn LlmModel>, model: ModelKey) -> Self {
        let mut tool_registry = ToolRegistry::new();
        let mut tool_guidelines: Vec<String> = Vec::new();

        if host.read_text_file.is_some() {
            let tool_context = ToolContext {
                host: host.clone(),
                progress: None,
                agent_id: AgentId::Root,
            };
            let tool = ReadTextFile::new(tool_context);
            tool_guidelines.push(tool.prompt_guidelines().to_string());
            tool_registry = tool_registry.register(tool);
        }

        let mut instructions = String::from("You are a helpful assistant.");
        for g in &tool_guidelines {
            instructions.push_str("\n\n");
            instructions.push_str(g);
        }

        let agent = Agent::builder()
            .name("arnes")
            .instructions(&instructions)
            .build();

        let runner = AgentRunner::with_registry(llm, Arc::new(tool_registry));

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
