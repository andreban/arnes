use serde::Deserialize;

use crate::{Cost, Micros, TokenCounts};

/// Per-million-token rates for a model.
#[derive(Clone, Debug, Deserialize)]
pub struct ModelPricing {
    pub input_per_mtok: Micros,
    pub output_per_mtok: Micros,
    pub cache_read_per_mtok: Option<Micros>,
    pub cache_write_per_mtok: Option<Micros>,
    pub thinking_per_mtok: Option<Micros>,
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

#[cfg(test)]
mod tests {
    use crate::{Micros, ModelPricing, TokenCounts};

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
        assert_eq!(cost.cache_read, Micros(100));
        assert_eq!(cost.cache_write, Micros(100));
        assert_eq!(cost.thinking, Micros(400));
    }
}
