// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use crate::{AgentId, ModelKey, TokenCounts};

/// One turn's resource usage: who used what model and how many tokens.
/// The per-turn record that rolls up into `ModelUsage` and
/// `CumulativeUsage`.
#[derive(Clone, Debug)]
pub struct TurnUsage {
    pub agent_id: AgentId,
    pub model: ModelKey,
    pub tokens: TokenCounts,
}

#[cfg(test)]
mod tests {
    use crate::{AgentId, ModelKey, TokenCounts, TurnUsage};

    #[test]
    fn turn_usage_round_trip() {
        let u = TurnUsage {
            agent_id: AgentId::Root,
            model: ModelKey {
                provider: "gemini".into(),
                model_id: "gemini-2.5-flash".into(),
            },
            tokens: TokenCounts { total: 1_850 },
        };
        assert_eq!(u.agent_id, AgentId::Root);
        assert_eq!(u.model.model_id, "gemini-2.5-flash");
        assert_eq!(u.tokens.total, 1_850);
    }
}
