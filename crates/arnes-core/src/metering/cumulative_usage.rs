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
}
