// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Money types: integer micros + per-token-kind cost breakdown.

use std::collections::HashMap;
use std::ops::{Add, AddAssign};

use crate::{AgentId, ModelKey};

/// A money amount in millionths of a dollar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Micros(pub i64);

impl Micros {
    pub const ZERO: Micros = Micros(0);
}

impl Add for Micros {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Micros(self.0 + rhs.0)
    }
}

impl AddAssign for Micros {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

/// Cost breakdown for a single turn.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cost {
    pub input: Micros,
    pub output: Micros,
    pub cache_read: Micros,
    pub cache_write: Micros,
    pub thinking: Micros,
    pub total: Micros,
}

impl Add for Cost {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Cost {
            input: self.input + rhs.input,
            output: self.output + rhs.output,
            cache_read: self.cache_read + rhs.cache_read,
            cache_write: self.cache_write + rhs.cache_write,
            thinking: self.thinking + rhs.thinking,
            total: self.total + rhs.total,
        }
    }
}

impl AddAssign for Cost {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

/// Per-million-token rates for a model.
#[derive(Clone, Debug)]
pub struct ModelPricing {
    pub input_per_mtok: Micros,
    pub output_per_mtok: Micros,
    pub cache_read_per_mtok: Option<Micros>,
    pub cache_write_per_mtok: Option<Micros>,
    pub thinking_per_mtok: Option<Micros>,
}

/// Per-token-kind counts a turn consumed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub thinking: u64,
}

impl Add for TokenCounts {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        TokenCounts {
            input: self.input + rhs.input,
            output: self.output + rhs.output,
            cache_read: self.cache_read + rhs.cache_read,
            cache_write: self.cache_write + rhs.cache_write,
            thinking: self.thinking + rhs.thinking,
        }
    }
}

impl AddAssign for TokenCounts {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl ModelPricing {
    /// Cost of a turn's token counts under this pricing. Optional cache
    /// and thinking rates fall back to `input_per_mtok` and
    /// `output_per_mtok` respectively.
    pub fn cost_for(&self, tokens: &TokenCounts) -> Cost {
        let input = scale_per_mtok(tokens.input, self.input_per_mtok);
        let output = scale_per_mtok(tokens.output, self.output_per_mtok);
        let cache_read = scale_per_mtok(
            tokens.cache_read,
            self.cache_read_per_mtok.unwrap_or(self.input_per_mtok),
        );
        let cache_write = scale_per_mtok(
            tokens.cache_write,
            self.cache_write_per_mtok.unwrap_or(self.input_per_mtok),
        );
        let thinking = scale_per_mtok(
            tokens.thinking,
            self.thinking_per_mtok.unwrap_or(self.output_per_mtok),
        );
        let total = input + output + cache_read + cache_write + thinking;
        Cost {
            input,
            output,
            cache_read,
            cache_write,
            thinking,
            total,
        }
    }
}

// i128 intermediate so `tokens * per_mtok` doesn't overflow at large
// counts before the divide by 1_000_000.
fn scale_per_mtok(tokens: u64, per_mtok: Micros) -> Micros {
    let product = (tokens as i128) * (per_mtok.0 as i128);
    Micros((product / 1_000_000) as i64)
}

/// What we know about a model's cost.
#[derive(Clone, Debug)]
pub enum ModelCostInfo {
    Priced(ModelPricing),
    Free,
    Unknown,
}

/// What a single turn cost.
#[derive(Clone, Debug)]
pub struct Usage {
    pub agent_id: AgentId,
    pub model: ModelKey,
    pub tokens: TokenCounts,
    /// `None` when the model's pricing is `ModelCostInfo::Unknown`.
    pub cost: Option<Cost>,
}

/// One bucket's aggregated state in `CumulativeUsage`.
#[derive(Clone, Debug)]
pub struct ModelUsage {
    pub model: ModelKey,
    pub tokens: TokenCounts,
    pub turns: u32,
    pub known_cost: Cost,
    pub turns_with_unknown_cost: u32,
}

/// All token + cost totals for one session, bucketed by (agent, model).
#[derive(Clone, Debug, Default)]
pub struct CumulativeUsage {
    by_agent_and_model: HashMap<(AgentId, ModelKey), ModelUsage>,
    pub turns_total: u32,
}

impl CumulativeUsage {
    /// Fold one turn's `Usage` into the matching bucket.
    pub fn record(&mut self, u: Usage) {
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

    #[test]
    fn micros_add() {
        assert_eq!(Micros(75) + Micros(25), Micros(100));
    }

    #[test]
    fn micros_add_assign_accumulates() {
        let mut acc = Micros::ZERO;
        for n in [10, 20, 30] {
            acc += Micros(n);
        }
        assert_eq!(acc, Micros(60));
    }

    #[test]
    fn cost_default_is_zero() {
        let c = Cost::default();
        assert_eq!(c.input, Micros::ZERO);
        assert_eq!(c.output, Micros::ZERO);
        assert_eq!(c.cache_read, Micros::ZERO);
        assert_eq!(c.cache_write, Micros::ZERO);
        assert_eq!(c.thinking, Micros::ZERO);
        assert_eq!(c.total, Micros::ZERO);
    }

    #[test]
    fn model_cost_info_priced_round_trip() {
        let with_opts = ModelCostInfo::Priced(ModelPricing {
            input_per_mtok: Micros(75),
            output_per_mtok: Micros(300),
            cache_read_per_mtok: Some(Micros(18)),
            cache_write_per_mtok: Some(Micros(94)),
            thinking_per_mtok: Some(Micros(450)),
        });
        let ModelCostInfo::Priced(p) = with_opts else {
            panic!("expected Priced");
        };
        assert_eq!(p.input_per_mtok, Micros(75));
        assert_eq!(p.cache_read_per_mtok, Some(Micros(18)));
        assert_eq!(p.thinking_per_mtok, Some(Micros(450)));

        let no_opts = ModelCostInfo::Priced(ModelPricing {
            input_per_mtok: Micros(75),
            output_per_mtok: Micros(300),
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
            thinking_per_mtok: None,
        });
        let ModelCostInfo::Priced(p) = no_opts else {
            panic!("expected Priced");
        };
        assert!(p.cache_read_per_mtok.is_none());
        assert!(p.cache_write_per_mtok.is_none());
        assert!(p.thinking_per_mtok.is_none());
    }

    #[test]
    fn cost_add_is_field_wise() {
        let a = Cost {
            input: Micros(1),
            output: Micros(2),
            cache_read: Micros(3),
            cache_write: Micros(4),
            thinking: Micros(5),
            total: Micros(15),
        };
        let b = Cost {
            input: Micros(10),
            output: Micros(20),
            cache_read: Micros(30),
            cache_write: Micros(40),
            thinking: Micros(50),
            total: Micros(150),
        };
        let sum = a + b;
        assert_eq!(sum.input, Micros(11));
        assert_eq!(sum.output, Micros(22));
        assert_eq!(sum.cache_read, Micros(33));
        assert_eq!(sum.cache_write, Micros(44));
        assert_eq!(sum.thinking, Micros(55));
        assert_eq!(sum.total, Micros(165));
    }

    #[test]
    fn usage_with_priced_cost_round_trip() {
        let u = Usage {
            agent_id: AgentId::Root,
            model: ModelKey {
                provider: "gemini".into(),
                model_id: "gemini-2.5-flash".into(),
            },
            tokens: TokenCounts {
                input: 1_000,
                output: 500,
                cache_read: 200,
                cache_write: 100,
                thinking: 50,
            },
            cost: Some(Cost {
                input: Micros(75),
                output: Micros(150),
                cache_read: Micros(4),
                cache_write: Micros(9),
                thinking: Micros(22),
                total: Micros(260),
            }),
        };
        assert_eq!(u.agent_id, AgentId::Root);
        assert_eq!(u.model.model_id, "gemini-2.5-flash");
        assert_eq!(u.tokens.input, 1_000);
        let Some(cost) = u.cost else {
            panic!("expected Some(cost)");
        };
        assert_eq!(cost.total, Micros(260));
    }

    #[test]
    fn usage_with_unknown_cost_is_none() {
        let u = Usage {
            agent_id: AgentId::Root,
            model: ModelKey {
                provider: "unknown-provider".into(),
                model_id: "mystery-model".into(),
            },
            tokens: TokenCounts {
                input: 10,
                output: 5,
                ..TokenCounts::default()
            },
            cost: None,
        };
        assert!(u.cost.is_none());
    }

    fn sample_usage(provider: &str, model_id: &str, cost: Option<Cost>) -> Usage {
        Usage {
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

    #[test]
    fn token_counts_add_is_field_wise() {
        let a = TokenCounts {
            input: 1,
            output: 2,
            cache_read: 3,
            cache_write: 4,
            thinking: 5,
        };
        let b = TokenCounts {
            input: 10,
            output: 20,
            cache_read: 30,
            cache_write: 40,
            thinking: 50,
        };
        let mut sum = TokenCounts::default();
        sum += a;
        sum += b;
        assert_eq!(
            sum,
            TokenCounts {
                input: 11,
                output: 22,
                cache_read: 33,
                cache_write: 44,
                thinking: 55,
            }
        );
    }

    #[test]
    fn cost_for_per_mtok_arithmetic() {
        let pricing = ModelPricing {
            input_per_mtok: Micros(75),
            output_per_mtok: Micros(300),
            cache_read_per_mtok: Some(Micros(18)),
            cache_write_per_mtok: Some(Micros(94)),
            thinking_per_mtok: Some(Micros(450)),
        };
        let tokens = TokenCounts {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            cache_write: 1_000_000,
            thinking: 1_000_000,
        };
        let cost = pricing.cost_for(&tokens);
        assert_eq!(cost.input, Micros(75));
        assert_eq!(cost.output, Micros(300));
        assert_eq!(cost.cache_read, Micros(18));
        assert_eq!(cost.cache_write, Micros(94));
        assert_eq!(cost.thinking, Micros(450));
        assert_eq!(cost.total, Micros(75 + 300 + 18 + 94 + 450));
    }

    #[test]
    fn cost_for_falls_back_when_optional_rates_absent() {
        let pricing = ModelPricing {
            input_per_mtok: Micros(100),
            output_per_mtok: Micros(400),
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
            thinking_per_mtok: None,
        };
        let tokens = TokenCounts {
            input: 0,
            output: 0,
            cache_read: 1_000_000,
            cache_write: 1_000_000,
            thinking: 1_000_000,
        };
        let cost = pricing.cost_for(&tokens);
        assert_eq!(cost.cache_read, Micros(100)); // fell back to input rate
        assert_eq!(cost.cache_write, Micros(100)); // fell back to input rate
        assert_eq!(cost.thinking, Micros(400)); // fell back to output rate
    }
}
