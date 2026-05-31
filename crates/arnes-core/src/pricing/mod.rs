// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Pricing registry: lookup of `ModelCostInfo` by `(provider, model_id)`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::{Cost, ModelCostInfo, ModelKey, ModelPricing, TokenCounts};

/// Pricing for one provider: a default applied when no per-model entry
/// matches, plus per-model overrides keyed by `model_id`. This is also
/// the on-disk JSON shape — `parse_pricing` is a thin wrapper over
/// `serde_json::from_str`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProviderPricing {
    pub provider: String,
    #[serde(default)]
    pub default: ModelCostInfo,
    #[serde(default)]
    pub models: HashMap<String, ModelPricing>,
}

#[derive(Clone, Debug, Default)]
pub struct PricingRegistry {
    providers: HashMap<String, ProviderPricing>,
}

impl PricingRegistry {
    /// Build a registry from provider blocks. If the same provider name
    /// appears twice, the later block replaces the earlier one entirely.
    pub fn from_providers(blocks: impl IntoIterator<Item = ProviderPricing>) -> Self {
        PricingRegistry {
            providers: blocks
                .into_iter()
                .map(|p| (p.provider.clone(), p))
                .collect(),
        }
    }

    /// Look up the cost info for a `ModelKey`. Per-model entry wins;
    /// the provider's default is the fallback; missing providers resolve
    /// to `Unknown`.
    pub fn lookup(&self, key: &ModelKey) -> ModelCostInfo {
        let Some(p) = self.providers.get(&key.provider) else {
            return ModelCostInfo::Unknown;
        };
        match p.models.get(&key.model_id) {
            Some(pricing) => ModelCostInfo::Priced(pricing.clone()),
            None => p.default.clone(),
        }
    }

    /// Compute the cost for a turn. `None` when the model's pricing is
    /// `Unknown`.
    pub fn compute_cost(&self, key: &ModelKey, tokens: &TokenCounts) -> Option<Cost> {
        match self.lookup(key) {
            ModelCostInfo::Unknown => None,
            ModelCostInfo::Free => Some(Cost::default()),
            ModelCostInfo::Priced(p) => Some(p.cost_for(tokens)),
        }
    }
}

/// Parse the on-disk pricing JSON into a `Vec<ProviderPricing>`. Pure
/// function over the file contents; I/O and path resolution belong to
/// the caller. Equivalent to `serde_json::from_str(src)` — kept as a
/// named entry point so callers don't have to depend on `serde_json`
/// directly.
pub fn parse_pricing(src: &str) -> Result<Vec<ProviderPricing>, serde_json::Error> {
    serde_json::from_str(src)
}

#[cfg(test)]
mod tests;
