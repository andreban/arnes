// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use agent_rig::{
    Agent,
    model::{LlmModel, Message as RigMessage},
    runner::AgentRunner,
    tools::{Tool as AgentRigTool, ToolRegistry},
};

use crate::{
    AgentId, CumulativeUsage, Frontend, Host, Message, ModelKey, ToolContext,
    auth::{AuthManager, ToolPermissionMeta},
    tools::{ReadTextFile, Tool, WriteTextFile},
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
    /// Full agent-rig thread, including tool-call and tool-result messages,
    /// carried forward across `prompt()` calls so the model retains context.
    pub(super) rig_thread: Vec<RigMessage>,
    pub(super) cumulative_usage: CumulativeUsage,
    pub(super) model: ModelKey,
    pub(super) cwd: PathBuf,
}

impl<F: Frontend> Session<F> {
    pub fn new(
        frontend: Arc<F>,
        host: Host,
        llm: Arc<dyn LlmModel>,
        model: ModelKey,
        cwd: PathBuf,
    ) -> Self {
        let mut tool_registry = ToolRegistry::new();
        let mut tool_guidelines: Vec<String> = Vec::new();
        let mut tool_metadata: HashMap<String, ToolPermissionMeta> = HashMap::new();

        if host.read_text_file.is_some() {
            let tool_context = ToolContext {
                host: host.clone(),
                progress: None,
                agent_id: AgentId::Root,
                cwd: cwd.clone(),
            };
            let tool = ReadTextFile::new(tool_context);
            tool_guidelines.push(tool.prompt_guidelines().to_string());
            tool_metadata.insert(
                tool.definition().name.clone(),
                ToolPermissionMeta {
                    kind: tool.tool_kind(),
                },
            );
            tool_registry = tool_registry.register(tool);
        }

        if host.write_text_file.is_some() {
            let tool_context = ToolContext {
                host: host.clone(),
                progress: None,
                agent_id: AgentId::Root,
                cwd: cwd.clone(),
            };
            let tool = WriteTextFile::new(tool_context);
            tool_guidelines.push(tool.prompt_guidelines().to_string());
            tool_metadata.insert(
                tool.definition().name.clone(),
                ToolPermissionMeta {
                    kind: tool.tool_kind(),
                },
            );
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

        let auth_manager = Arc::new(AuthManager::new(frontend.clone(), tool_metadata));

        let runner = AgentRunner::with_registry(llm, Arc::new(tool_registry))
            .with_auth_manager(auth_manager);

        Self {
            frontend,
            host,
            runner,
            agent,
            history: Vec::new(),
            rig_thread: Vec::new(),
            cumulative_usage: CumulativeUsage::default(),
            model,
            cwd,
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
