// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Identity types for agents and models.

use uuid::Uuid;

/// Identifies an agent within a session.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum AgentId {
    Root,
    SubAgent(Uuid),
}

/// Identifies a model for cost accounting and usage attribution.
///
/// The pair `(provider, model_id)` is the natural key; pricing data is
/// indexed by it, and per-model usage breakdowns bucket by it.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModelKey {
    pub provider: String,
    pub model_id: String,
}
