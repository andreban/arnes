// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Money types: integer micros + per-token-kind cost breakdown.

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
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub thinking_tokens: u64,
    /// `None` when the model's pricing is `ModelCostInfo::Unknown`.
    pub cost: Option<Cost>,
}

/// One bucket's aggregated state in `CumulativeUsage`.
#[derive(Clone, Debug)]
pub struct ModelUsage {
    pub model: ModelKey,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub thinking_tokens: u64,
    pub turns: u32,
    pub known_cost: Cost,
    pub turns_with_unknown_cost: u32,
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
            input_tokens: 1_000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_write_tokens: 100,
            thinking_tokens: 50,
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
        assert_eq!(u.input_tokens, 1_000);
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
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            thinking_tokens: 0,
            cost: None,
        };
        assert!(u.cost.is_none());
    }
}
