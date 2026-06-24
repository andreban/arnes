// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use agent_rig::{
    Agent,
    model::{LlmModel, MessageList},
    runner::AgentRunner,
    tools::ToolDefinition,
};

use crate::{
    CumulativeUsage, Frontend, Host, Message, ModelKey, ToolContext,
    tools::{EditTextFile, ReadTextFile, Tool, ToolRegistry, WriteTextFile},
};

mod prompt;

/// The single entry point a frontend talks to.
pub struct Session<F: Frontend> {
    pub(super) frontend: Arc<F>,
    pub(super) runner: AgentRunner,
    pub(super) agent: Agent,
    pub(super) history: Vec<Message>,
    /// Full agent-rig thread, including tool-call and tool-result messages,
    /// carried forward across `prompt()` calls so the model retains context.
    pub(super) rig_thread: MessageList,
    pub(super) cumulative_usage: CumulativeUsage,
    pub(super) model: ModelKey,
    pub(super) cwd: PathBuf,
    tool_registry: ToolRegistry,
}

impl<F: Frontend> Session<F> {
    pub fn new(
        frontend: Arc<F>,
        host: Host,
        llm: Arc<dyn LlmModel>,
        model: ModelKey,
        cwd: PathBuf,
    ) -> Self {
        let mut tool_guidelines: Vec<String> = Vec::new();
        let mut tool_definitions: Vec<ToolDefinition> = vec![];
        let mut tool_registry = ToolRegistry::default();
        let tool_context = ToolContext {
            host: host.clone(),
            cwd: cwd.clone(),
            read_grants: Arc::new(Mutex::new(HashSet::new())),
        };

        if host.read_text_file.is_some() {
            let tool = ReadTextFile::new(tool_context.clone());
            tool_definitions.push(tool.definition().clone());
            tool_guidelines.push(tool.prompt_guidelines().to_string());
            tool_registry.register(Tool::ReadTextFile(tool));
        };

        if host.write_text_file.is_some() {
            let tool = WriteTextFile::new(tool_context.clone());
            tool_definitions.push(tool.definition().clone());
            tool_guidelines.push(tool.prompt_guidelines().to_string());
            tool_registry.register(Tool::WriteTextFile(tool));
        };

        if host.read_text_file.is_some() && host.write_text_file.is_some() {
            let tool = EditTextFile::new(tool_context.clone());
            tool_definitions.push(tool.definition().clone());
            tool_guidelines.push(tool.prompt_guidelines().to_string());
            tool_registry.register(Tool::EditTextFile(tool));
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

        let runner = AgentRunner::with_tools(llm, tool_definitions);

        Self {
            frontend,
            runner,
            agent,
            history: Vec::new(),
            rig_thread: MessageList::new(),
            cumulative_usage: CumulativeUsage::default(),
            model,
            cwd,
            tool_registry,
        }
    }

    /// Returns the working directory this session resolves relative paths against.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn cumulative_usage(&self) -> &CumulativeUsage {
        &self.cumulative_usage
    }
}
