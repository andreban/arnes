// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Pricing registry: lookup of `ModelCostInfo` by `(provider, model_id)`.

use std::collections::HashMap;

use crate::{Cost, ModelCostInfo, ModelKey, TokenCounts};

/// One row of pricing data, as produced by the loader or built directly
/// in tests.
#[derive(Clone, Debug)]
pub enum PricingEntry {
    Exact {
        provider: String,
        model_id: String,
        info: ModelCostInfo,
    },
    ProviderDefault {
        provider: String,
        info: ModelCostInfo,
    },
}

#[derive(Clone, Debug, Default)]
pub struct PricingRegistry {
    exact: HashMap<(String, String), ModelCostInfo>,
    provider_default: HashMap<String, ModelCostInfo>,
}

impl PricingRegistry {
    /// Build a registry from bundled and user-override entries. User
    /// entries overwrite bundled entries for the same key.
    pub fn from_entries(bundled: Vec<PricingEntry>, user: Vec<PricingEntry>) -> Self {
        let mut reg = PricingRegistry::default();
        reg.insert_all(bundled);
        reg.insert_all(user);
        reg
    }

    fn insert_all(&mut self, entries: Vec<PricingEntry>) {
        for e in entries {
            match e {
                PricingEntry::Exact {
                    provider,
                    model_id,
                    info,
                } => {
                    self.exact.insert((provider, model_id), info);
                }
                PricingEntry::ProviderDefault { provider, info } => {
                    self.provider_default.insert(provider, info);
                }
            }
        }
    }

    /// Look up the cost info for a `ModelKey`. Exact match wins;
    /// provider default is the fallback; otherwise `Unknown`.
    pub fn lookup(&self, key: &ModelKey) -> ModelCostInfo {
        if let Some(info) = self
            .exact
            .get(&(key.provider.clone(), key.model_id.clone()))
        {
            return info.clone();
        }
        if let Some(info) = self.provider_default.get(&key.provider) {
            return info.clone();
        }
        ModelCostInfo::Unknown
    }

    /// Compute the cost for a turn. Returns `None` when the model's
    /// pricing is `Unknown`.
    pub fn compute_cost(&self, key: &ModelKey, tokens: &TokenCounts) -> Option<Cost> {
        match self.lookup(key) {
            ModelCostInfo::Unknown => None,
            ModelCostInfo::Free => Some(Cost::default()),
            ModelCostInfo::Priced(p) => Some(p.cost_for(tokens)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Micros, ModelPricing};

    fn key(provider: &str, model_id: &str) -> ModelKey {
        ModelKey {
            provider: provider.into(),
            model_id: model_id.into(),
        }
    }

    fn priced(input: i64, output: i64) -> ModelCostInfo {
        ModelCostInfo::Priced(ModelPricing {
            input_per_mtok: Micros(input),
            output_per_mtok: Micros(output),
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
            thinking_per_mtok: None,
        })
    }

    fn exact(provider: &str, model_id: &str, info: ModelCostInfo) -> PricingEntry {
        PricingEntry::Exact {
            provider: provider.into(),
            model_id: model_id.into(),
            info,
        }
    }

    fn provider_default(provider: &str, info: ModelCostInfo) -> PricingEntry {
        PricingEntry::ProviderDefault {
            provider: provider.into(),
            info,
        }
    }

    fn one_mtok_each() -> TokenCounts {
        TokenCounts {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            thinking: 0,
        }
    }

    #[test]
    fn exact_match() {
        let reg = PricingRegistry::from_entries(
            vec![exact("gemini", "gemini-2.5-flash", priced(75, 300))],
            vec![],
        );
        let info = reg.lookup(&key("gemini", "gemini-2.5-flash"));
        assert!(matches!(info, ModelCostInfo::Priced(_)));
        let other = reg.lookup(&key("gemini", "gemini-2.5-pro"));
        assert!(matches!(other, ModelCostInfo::Unknown));
    }

    #[test]
    fn provider_default_matches_any_model_id() {
        let reg = PricingRegistry::from_entries(
            vec![provider_default("ollama", ModelCostInfo::Free)],
            vec![],
        );
        assert!(matches!(
            reg.lookup(&key("ollama", "llama3.2")),
            ModelCostInfo::Free
        ));
        assert!(matches!(
            reg.lookup(&key("ollama", "some-custom-finetune")),
            ModelCostInfo::Free
        ));
    }

    #[test]
    fn exact_beats_provider_default() {
        let reg = PricingRegistry::from_entries(
            vec![
                provider_default("ollama", ModelCostInfo::Free),
                exact("ollama", "billed-model", priced(10, 20)),
            ],
            vec![],
        );
        assert!(matches!(
            reg.lookup(&key("ollama", "billed-model")),
            ModelCostInfo::Priced(_)
        ));
        assert!(matches!(
            reg.lookup(&key("ollama", "anything-else")),
            ModelCostInfo::Free
        ));
    }

    #[test]
    fn unmatched_returns_unknown() {
        let reg = PricingRegistry::default();
        assert!(matches!(
            reg.lookup(&key("nobody", "nothing")),
            ModelCostInfo::Unknown
        ));
    }

    #[test]
    fn user_overrides_bundled_exact() {
        let reg = PricingRegistry::from_entries(
            vec![exact("gemini", "flash", ModelCostInfo::Free)],
            vec![exact("gemini", "flash", priced(75, 300))],
        );
        assert!(matches!(
            reg.lookup(&key("gemini", "flash")),
            ModelCostInfo::Priced(_)
        ));
    }

    #[test]
    fn user_overrides_bundled_provider_default() {
        let reg = PricingRegistry::from_entries(
            vec![provider_default("ollama", ModelCostInfo::Free)],
            vec![provider_default("ollama", ModelCostInfo::Unknown)],
        );
        assert!(matches!(
            reg.lookup(&key("ollama", "anything")),
            ModelCostInfo::Unknown
        ));
    }

    #[test]
    fn compute_cost_three_state() {
        let reg = PricingRegistry::from_entries(
            vec![
                exact("gemini", "flash", priced(75, 300)),
                exact("local", "self-hosted", ModelCostInfo::Free),
            ],
            vec![],
        );

        let priced_cost = reg.compute_cost(&key("gemini", "flash"), &one_mtok_each());
        let Some(c) = priced_cost else {
            panic!("expected Some for Priced");
        };
        assert_eq!(c.total, Micros(75 + 300));

        let free_cost = reg.compute_cost(&key("local", "self-hosted"), &one_mtok_each());
        let Some(c) = free_cost else {
            panic!("expected Some for Free");
        };
        assert_eq!(c.input, Micros::ZERO);
        assert_eq!(c.output, Micros::ZERO);
        assert_eq!(c.total, Micros::ZERO);

        let unknown_cost = reg.compute_cost(&key("unknown", "model"), &one_mtok_each());
        assert!(unknown_cost.is_none());
    }
}
