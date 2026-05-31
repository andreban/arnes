// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

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

// On the wire, money is dollars (e.g. `0.075`); in memory it's
// integer micros. The Deserialize impl bakes that conversion in so the
// JSON loader can derive `Deserialize` on rate-bearing structs directly.
impl<'de> serde::Deserialize<'de> for Micros {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let dollars = f64::deserialize(d)?;
        Ok(Micros((dollars * 1_000_000.0).round() as i64))
    }
}

#[cfg(test)]
mod tests {
    use crate::Micros;

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
}
