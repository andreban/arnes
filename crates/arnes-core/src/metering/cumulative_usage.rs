// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::{AgentId, ModelKey, ModelUsage, TokenCounts, TurnUsage};

/// All token totals for one session, bucketed by (agent, model).
#[derive(Clone, Debug, Default)]
pub struct CumulativeUsage {
    by_agent_and_model: HashMap<(AgentId, ModelKey), ModelUsage>,
    pub turns_total: u32,
}

impl CumulativeUsage {
    /// Fold one turn's `TurnUsage` into the matching bucket.
    pub fn record(&mut self, u: TurnUsage) {
        let key = (u.agent_id.clone(), u.model.clone());
        let bucket = self
            .by_agent_and_model
            .entry(key)
            .or_insert_with(|| ModelUsage {
                model: u.model.clone(),
                tokens: TokenCounts::default(),
                turns: 0,
            });

        bucket.tokens += u.tokens;
        bucket.turns += 1;

        self.turns_total += 1;
    }

    /// Per-`ModelKey` totals, summed across agents.
    pub fn by_model(&self) -> HashMap<&ModelKey, ModelUsage> {
        let mut out: HashMap<&ModelKey, ModelUsage> = HashMap::new();
        for ((_, model), bucket) in &self.by_agent_and_model {
            let entry = out.entry(model).or_insert_with(|| ModelUsage {
                model: model.clone(),
                tokens: TokenCounts::default(),
                turns: 0,
            });
            merge_into(entry, bucket);
        }
        out
    }

    /// Per-`AgentId` totals, summed across models. The `model` field on
    /// each row carries whichever model was first encountered for that
    /// agent and is not meaningful when reading per-agent rows.
    pub fn by_agent(&self) -> HashMap<&AgentId, ModelUsage> {
        let mut out: HashMap<&AgentId, ModelUsage> = HashMap::new();
        for ((agent, model), bucket) in &self.by_agent_and_model {
            let entry = out.entry(agent).or_insert_with(|| ModelUsage {
                model: model.clone(),
                tokens: TokenCounts::default(),
                turns: 0,
            });
            merge_into(entry, bucket);
        }
        out
    }
}

fn merge_into(into: &mut ModelUsage, from: &ModelUsage) {
    into.tokens += from.tokens;
    into.turns += from.turns;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_usage(provider: &str, model_id: &str, total: u64) -> TurnUsage {
        TurnUsage {
            agent_id: AgentId::Root,
            model: ModelKey {
                provider: provider.into(),
                model_id: model_id.into(),
            },
            tokens: TokenCounts { total },
        }
    }

    fn sub_agent(n: u128) -> AgentId {
        AgentId::SubAgent(uuid::Uuid::from_u128(n))
    }

    fn sample_usage_for(
        agent_id: AgentId,
        provider: &str,
        model_id: &str,
        total: u64,
    ) -> TurnUsage {
        TurnUsage {
            agent_id,
            model: ModelKey {
                provider: provider.into(),
                model_id: model_id.into(),
            },
            tokens: TokenCounts { total },
        }
    }

    fn model_key(provider: &str, model_id: &str) -> ModelKey {
        ModelKey {
            provider: provider.into(),
            model_id: model_id.into(),
        }
    }

    #[test]
    fn cumulative_usage_default_is_empty() {
        let cu = CumulativeUsage::default();
        assert_eq!(cu.turns_total, 0);
        assert!(cu.by_model().is_empty());
        assert!(cu.by_agent().is_empty());
    }

    #[test]
    fn cumulative_usage_records_one_bucket() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", 225));
        assert_eq!(cu.turns_total, 1);
        let by_model = cu.by_model();
        assert_eq!(by_model.len(), 1);
        assert_eq!(by_model.values().next().unwrap().tokens.total, 225);
    }

    #[test]
    fn cumulative_usage_buckets_by_agent_and_model() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", 225));
        cu.record(sample_usage("gemini", "pro", 900));
        assert_eq!(cu.by_agent_and_model.len(), 2);
        assert_eq!(cu.turns_total, 2);
    }

    #[test]
    fn bucket_turn_count_invariant() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", 100));
        cu.record(sample_usage("gemini", "flash", 200));
        cu.record(sample_usage("gemini", "flash", 50));

        let bucket = cu.by_agent_and_model.values().next().expect("one bucket");
        assert_eq!(bucket.turns, 3);
        assert_eq!(bucket.tokens.total, 350);
    }

    #[test]
    fn single_bucket_appears_in_both_projections() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", 225));

        let by_model = cu.by_model();
        assert_eq!(by_model.len(), 1);
        let m = by_model
            .get(&model_key("gemini", "flash"))
            .expect("model row present");
        assert_eq!(m.turns, 1);
        assert_eq!(m.tokens.total, 225);

        let by_agent = cu.by_agent();
        assert_eq!(by_agent.len(), 1);
        let a = by_agent.get(&AgentId::Root).expect("agent row present");
        assert_eq!(a.turns, 1);
        assert_eq!(a.tokens.total, 225);
    }

    #[test]
    fn by_model_separates_models_for_one_agent() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", 225));
        cu.record(sample_usage("gemini", "pro", 900));

        let by_model = cu.by_model();
        assert_eq!(by_model.len(), 2);
        assert_eq!(
            by_model.get(&model_key("gemini", "flash")).unwrap().turns,
            1
        );
        assert_eq!(by_model.get(&model_key("gemini", "pro")).unwrap().turns, 1);

        let by_agent = cu.by_agent();
        assert_eq!(by_agent.len(), 1);
        let row = by_agent.get(&AgentId::Root).expect("root present");
        assert_eq!(row.turns, 2);
        assert_eq!(row.tokens.total, 1_125);
    }

    #[test]
    fn by_agent_separates_agents_for_one_model() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage_for(AgentId::Root, "gemini", "flash", 225));
        cu.record(sample_usage_for(sub_agent(1), "gemini", "flash", 30));

        let by_agent = cu.by_agent();
        assert_eq!(by_agent.len(), 2);
        assert_eq!(by_agent.get(&AgentId::Root).unwrap().turns, 1);
        assert_eq!(by_agent.get(&sub_agent(1)).unwrap().turns, 1);

        let by_model = cu.by_model();
        assert_eq!(by_model.len(), 1);
        let row = by_model
            .get(&model_key("gemini", "flash"))
            .expect("model present");
        assert_eq!(row.turns, 2);
        assert_eq!(row.tokens.total, 255);
    }

    #[test]
    fn projection_consistency_across_views() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage_for(AgentId::Root, "gemini", "flash", 225));
        cu.record(sample_usage_for(AgentId::Root, "gemini", "pro", 900));
        cu.record(sample_usage_for(sub_agent(1), "gemini", "flash", 30));
        cu.record(sample_usage_for(sub_agent(1), "mystery", "model", 42));

        let by_model = cu.by_model();
        let by_agent = cu.by_agent();

        let sum_tokens_by_model: u64 = by_model.values().map(|m| m.tokens.total).sum();
        let sum_tokens_by_agent: u64 = by_agent.values().map(|a| a.tokens.total).sum();
        assert_eq!(sum_tokens_by_model, sum_tokens_by_agent);
        assert_eq!(sum_tokens_by_model, 225 + 900 + 30 + 42);

        let sum_turns_by_model: u32 = by_model.values().map(|m| m.turns).sum();
        let sum_turns_by_agent: u32 = by_agent.values().map(|a| a.turns).sum();
        assert_eq!(sum_turns_by_model, cu.turns_total);
        assert_eq!(sum_turns_by_agent, cu.turns_total);
    }
}
