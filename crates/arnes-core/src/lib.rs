// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod error;
mod frontend;
mod host;
mod identity;
mod message;
mod metering;
mod pricing;
mod tool_context;

pub use error::{CoreError, Result};
pub use frontend::{
    EventKind, Frontend, FrontendCapabilities, Permission, PermissionRequest, SessionEvent,
    StopReason,
};
pub use host::{DirEntry, Host, LocalHost, TerminalHandle, TerminalSnapshot, TerminalSpec};
pub use identity::{AgentId, ModelKey};
pub use message::{ContentBlock, Message};
pub use metering::{
    Cost, CumulativeUsage, Micros, ModelCostInfo, ModelPricing, ModelUsage, TokenCounts, TurnUsage,
};
pub use pricing::{PricingRegistry, ProviderPricing, parse_pricing};
pub use tool_context::{ProgressSink, ProgressUpdate, ToolContext};
