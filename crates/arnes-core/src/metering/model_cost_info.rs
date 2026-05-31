// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

use crate::ModelPricing;

/// What we know about a model's cost.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCostInfo {
    Priced(ModelPricing),
    Free,
    #[default]
    #[serde(skip)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use crate::{Micros, ModelCostInfo, ModelPricing};

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
}
