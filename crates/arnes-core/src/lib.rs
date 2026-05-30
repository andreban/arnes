// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod error;
mod frontend;
mod identity;
mod message;

pub use error::{CoreError, Result};
pub use frontend::{
    EventKind, Frontend, FrontendCapabilities, Permission, PermissionRequest, SessionEvent,
    StopReason,
};
pub use identity::{AgentId, ModelKey};
pub use message::{ContentBlock, Message};
