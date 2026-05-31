// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use crate::{AgentId, Cost, ModelKey, TokenCounts};

/// One turn's resource usage: who used what model, how many tokens, at
/// what cost. The per-turn record that rolls up into `ModelUsage` and
/// `CumulativeUsage`.
#[derive(Clone, Debug)]
pub struct TurnUsage {
    pub agent_id: AgentId,
    pub model: ModelKey,
    pub tokens: TokenCounts,
    /// `None` when the model's pricing is `ModelCostInfo::Unknown`.
    pub cost: Option<Cost>,
}

#[cfg(test)]
mod tests {
    use crate::{AgentId, Cost, Micros, ModelKey, TokenCounts, TurnUsage};

    #[test]
    fn turn_usage_with_priced_cost_round_trip() {
        let u = TurnUsage {
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
    fn turn_usage_with_unknown_cost_is_none() {
        let u = TurnUsage {
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
}
