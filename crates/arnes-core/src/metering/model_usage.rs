// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use crate::{ModelKey, TokenCounts};

/// One bucket's aggregated state in `CumulativeUsage`.
#[derive(Clone, Debug)]
pub struct ModelUsage {
    pub model: ModelKey,
    pub tokens: TokenCounts,
    pub turns: u32,
}
