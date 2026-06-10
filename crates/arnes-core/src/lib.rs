// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod auth;
mod error;
mod frontend;
mod host;
mod identity;
mod message;
mod metering;
mod session;
mod tool_context;
mod tools;

pub use error::{CoreError, Result};
pub use frontend::{
    EventKind, FilesystemCapabilities, Frontend, FrontendCapabilities, Permission,
    PermissionRequest, SessionEvent, StopReason, ToolCallOutcome, ToolKind,
};
pub use host::{
    Host, ReadTextFile, Terminal, TerminalHandle, TerminalSnapshot, TerminalSpec, WriteTextFile,
};
pub use identity::{AgentId, ModelKey};
pub use message::{ContentBlock, Message};
pub use metering::{CumulativeUsage, ModelUsage, TokenCounts, TurnUsage};
pub use session::Session;
pub use tool_context::{ProgressSink, ProgressUpdate, ToolContext};
