// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Money types: integer micros + per-token-kind cost breakdown.

use std::ops::{Add, AddAssign};

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
}
