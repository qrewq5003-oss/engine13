//! The time contract (AGENTS.md §1, plan §2): 2 ticks = 1 year, parity is the half-year.
//!
//! Year and half-year are methods, not fields: there is nothing to fall out of sync.

use serde::{Deserialize, Serialize};

/// A half-year step of the simulation, counted from the scenario start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tick(pub u32);

/// Which half of the year a tick falls in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HalfYear {
    First,
    Second,
}

impl Tick {
    pub const ZERO: Tick = Tick(0);

    /// `year = start_year + tick / 2`.
    pub const fn year(self, start_year: i32) -> i32 {
        start_year + (self.0 / 2) as i32
    }

    /// Tick parity determines the half-year.
    pub const fn half_year(self) -> HalfYear {
        if self.0.is_multiple_of(2) {
            HalfYear::First
        } else {
            HalfYear::Second
        }
    }

    pub const fn next(self) -> Tick {
        Tick(self.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_ticks_make_a_year_and_parity_is_the_half() {
        assert_eq!(Tick(0).year(1477), 1477);
        assert_eq!(Tick(1).year(1477), 1477);
        assert_eq!(Tick(2).year(1477), 1478);
        assert_eq!(Tick(41).year(375), 395);
        assert_eq!(Tick(0).half_year(), HalfYear::First);
        assert_eq!(Tick(1).half_year(), HalfYear::Second);
        assert_eq!(Tick(5).next(), Tick(6));
    }
}
