// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Add, AddAssign};

/// How many tokens a turn consumed. One number for now; the struct exists
/// so a per-kind breakdown can grow back without renaming every site.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenCounts {
    pub total: u64,
}

impl Add for TokenCounts {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        TokenCounts {
            total: self.total + rhs.total,
        }
    }
}

impl AddAssign for TokenCounts {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

#[cfg(test)]
mod tests {
    use crate::TokenCounts;

    #[test]
    fn token_counts_add_sums_totals() {
        let mut sum = TokenCounts::default();
        sum += TokenCounts { total: 15 };
        sum += TokenCounts { total: 110 };
        assert_eq!(sum, TokenCounts { total: 125 });
    }
}
