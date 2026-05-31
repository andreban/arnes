use std::ops::{Add, AddAssign};

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

#[cfg(test)]
mod tests {
    use crate::TokenCounts;

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
}
