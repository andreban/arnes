// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::{AgentId, Cost, ModelKey, ModelUsage, TokenCounts, TurnUsage};

/// All token + cost totals for one session, bucketed by (agent, model).
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
                known_cost: Cost::default(),
                turns_with_unknown_cost: 0,
            });

        bucket.tokens += u.tokens;
        bucket.turns += 1;

        match u.cost {
            Some(c) => bucket.known_cost += c,
            None => bucket.turns_with_unknown_cost += 1,
        }

        self.turns_total += 1;
    }

    /// Sum of `known_cost` across every bucket.
    pub fn total_known_cost(&self) -> Cost {
        let mut total = Cost::default();
        for bucket in self.by_agent_and_model.values() {
            total += bucket.known_cost;
        }
        total
    }

    /// Total count of turns whose cost was `None`.
    pub fn total_turns_with_unknown_cost(&self) -> u32 {
        self.by_agent_and_model
            .values()
            .map(|b| b.turns_with_unknown_cost)
            .sum()
    }

    /// Per-`ModelKey` totals, summed across agents.
    pub fn by_model(&self) -> HashMap<&ModelKey, ModelUsage> {
        let mut out: HashMap<&ModelKey, ModelUsage> = HashMap::new();
        for ((_, model), bucket) in &self.by_agent_and_model {
            let entry = out.entry(model).or_insert_with(|| ModelUsage {
                model: model.clone(),
                tokens: TokenCounts::default(),
                turns: 0,
                known_cost: Cost::default(),
                turns_with_unknown_cost: 0,
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
                known_cost: Cost::default(),
                turns_with_unknown_cost: 0,
            });
            merge_into(entry, bucket);
        }
        out
    }
}

fn merge_into(into: &mut ModelUsage, from: &ModelUsage) {
    into.tokens += from.tokens;
    into.turns += from.turns;
    into.known_cost += from.known_cost;
    into.turns_with_unknown_cost += from.turns_with_unknown_cost;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Micros;

    fn sample_usage(provider: &str, model_id: &str, cost: Option<Cost>) -> TurnUsage {
        TurnUsage {
            agent_id: AgentId::Root,
            model: ModelKey {
                provider: provider.into(),
                model_id: model_id.into(),
            },
            tokens: TokenCounts {
                input: 100,
                output: 50,
                cache_read: 20,
                cache_write: 10,
                thinking: 5,
            },
            cost,
        }
    }

    fn cost_of(input: i64, output: i64) -> Cost {
        Cost {
            input: Micros(input),
            output: Micros(output),
            cache_read: Micros::ZERO,
            cache_write: Micros::ZERO,
            thinking: Micros::ZERO,
            total: Micros(input + output),
        }
    }

    #[test]
    fn cumulative_usage_default_is_empty() {
        let cu = CumulativeUsage::default();
        assert_eq!(cu.turns_total, 0);
        assert_eq!(cu.total_known_cost().total, Micros::ZERO);
        assert_eq!(cu.total_turns_with_unknown_cost(), 0);
    }

    #[test]
    fn cumulative_usage_records_one_bucket() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", Some(cost_of(75, 150))));
        assert_eq!(cu.turns_total, 1);
        assert_eq!(cu.total_known_cost().total, Micros(225));
        assert_eq!(cu.total_turns_with_unknown_cost(), 0);
    }

    #[test]
    fn cumulative_usage_buckets_by_agent_and_model() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", Some(cost_of(75, 150))));
        cu.record(sample_usage("gemini", "pro", Some(cost_of(300, 600))));
        assert_eq!(cu.by_agent_and_model.len(), 2);
        assert_eq!(cu.turns_total, 2);
        assert_eq!(cu.total_known_cost().total, Micros(75 + 150 + 300 + 600));
    }

    #[test]
    fn cumulative_usage_separates_unknown_from_known() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", Some(cost_of(75, 150))));
        cu.record(sample_usage("mystery", "model", None));
        assert_eq!(cu.turns_total, 2);
        assert_eq!(cu.total_known_cost().total, Micros(225));
        assert_eq!(cu.total_turns_with_unknown_cost(), 1);
    }

    #[test]
    fn bucket_turn_count_invariant() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", Some(cost_of(75, 150))));
        cu.record(sample_usage("gemini", "flash", None));
        cu.record(sample_usage("gemini", "flash", Some(cost_of(10, 20))));

        let bucket = cu.by_agent_and_model.values().next().expect("one bucket");
        let known = bucket.turns - bucket.turns_with_unknown_cost;
        assert_eq!(bucket.turns, known + bucket.turns_with_unknown_cost);
        assert_eq!(bucket.turns, 3);
        assert_eq!(bucket.turns_with_unknown_cost, 1);
    }

    fn sub_agent(n: u128) -> AgentId {
        AgentId::SubAgent(uuid::Uuid::from_u128(n))
    }

    fn sample_usage_for(
        agent_id: AgentId,
        provider: &str,
        model_id: &str,
        cost: Option<Cost>,
    ) -> TurnUsage {
        TurnUsage {
            agent_id,
            model: ModelKey {
                provider: provider.into(),
                model_id: model_id.into(),
            },
            tokens: TokenCounts {
                input: 100,
                output: 50,
                cache_read: 20,
                cache_write: 10,
                thinking: 5,
            },
            cost,
        }
    }

    fn model_key(provider: &str, model_id: &str) -> ModelKey {
        ModelKey {
            provider: provider.into(),
            model_id: model_id.into(),
        }
    }

    #[test]
    fn projections_on_empty_are_empty() {
        let cu = CumulativeUsage::default();
        assert!(cu.by_model().is_empty());
        assert!(cu.by_agent().is_empty());
    }

    #[test]
    fn single_bucket_appears_in_both_projections() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", Some(cost_of(75, 150))));

        let by_model = cu.by_model();
        assert_eq!(by_model.len(), 1);
        let m = by_model
            .get(&model_key("gemini", "flash"))
            .expect("model row present");
        assert_eq!(m.turns, 1);
        assert_eq!(m.known_cost.total, Micros(225));
        assert_eq!(m.tokens.input, 100);

        let by_agent = cu.by_agent();
        assert_eq!(by_agent.len(), 1);
        let a = by_agent.get(&AgentId::Root).expect("agent row present");
        assert_eq!(a.turns, 1);
        assert_eq!(a.known_cost.total, Micros(225));
    }

    #[test]
    fn by_model_separates_models_for_one_agent() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage("gemini", "flash", Some(cost_of(75, 150))));
        cu.record(sample_usage("gemini", "pro", Some(cost_of(300, 600))));

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
        assert_eq!(row.known_cost.total, Micros(75 + 150 + 300 + 600));
    }

    #[test]
    fn by_agent_separates_agents_for_one_model() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage_for(
            AgentId::Root,
            "gemini",
            "flash",
            Some(cost_of(75, 150)),
        ));
        cu.record(sample_usage_for(
            sub_agent(1),
            "gemini",
            "flash",
            Some(cost_of(10, 20)),
        ));

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
        assert_eq!(row.known_cost.total, Micros(75 + 150 + 10 + 20));
    }

    #[test]
    fn projection_consistency_across_views() {
        let mut cu = CumulativeUsage::default();
        cu.record(sample_usage_for(
            AgentId::Root,
            "gemini",
            "flash",
            Some(cost_of(75, 150)),
        ));
        cu.record(sample_usage_for(
            AgentId::Root,
            "gemini",
            "pro",
            Some(cost_of(300, 600)),
        ));
        cu.record(sample_usage_for(
            sub_agent(1),
            "gemini",
            "flash",
            Some(cost_of(10, 20)),
        ));
        cu.record(sample_usage_for(sub_agent(1), "mystery", "model", None));

        let total_known = cu.total_known_cost().total.0;
        let total_unknown = cu.total_turns_with_unknown_cost();

        let by_model = cu.by_model();
        let by_agent = cu.by_agent();

        let sum_known_by_model: i64 = by_model.values().map(|m| m.known_cost.total.0).sum();
        let sum_known_by_agent: i64 = by_agent.values().map(|a| a.known_cost.total.0).sum();
        assert_eq!(sum_known_by_model, total_known);
        assert_eq!(sum_known_by_agent, total_known);

        let sum_unknown_by_model: u32 = by_model.values().map(|m| m.turns_with_unknown_cost).sum();
        let sum_unknown_by_agent: u32 = by_agent.values().map(|a| a.turns_with_unknown_cost).sum();
        assert_eq!(sum_unknown_by_model, total_unknown);
        assert_eq!(sum_unknown_by_agent, total_unknown);

        let sum_input_by_model: u64 = by_model.values().map(|m| m.tokens.input).sum();
        let sum_input_by_agent: u64 = by_agent.values().map(|a| a.tokens.input).sum();
        assert_eq!(sum_input_by_model, sum_input_by_agent);

        let sum_turns_by_model: u32 = by_model.values().map(|m| m.turns).sum();
        let sum_turns_by_agent: u32 = by_agent.values().map(|a| a.turns).sum();
        assert_eq!(sum_turns_by_model, cu.turns_total);
        assert_eq!(sum_turns_by_agent, cu.turns_total);
    }
}
