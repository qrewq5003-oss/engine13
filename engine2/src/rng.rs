//! Keyed deterministic randomness with a draw journal (plan §3).
//!
//! Every draw is addressed by `(seed, point, tick, id, n)` — `n` being the ordinal of the
//! draw among those with the same point, tick and id — and computed from a ChaCha8 stream
//! seeded with exactly that key. Without `n`, two battles in one province in one tick, or
//! two decisions of one actor, would share one number (owner's review of PR #277). There is no shared stream to advance, so:
//! - the same key gives the same value in any process on any platform (ChaCha is
//!   platform-independent, the key is little-endian bytes);
//! - a run that draws *more* in one place (a different player verb, an extra battle) gets
//!   the same values everywhere else — a paired run player-vs-bot measures strategy, not
//!   stream shift (E0 §2, E3 acceptance);
//! - nothing about randomness needs saving beyond the seed.
//!
//! This is the keyed variant the plan recommends; the brief's letter (§5.1, "one stream")
//! is a one-function change in [`draw`]. The facade and the journal stay the same.
//!
//! Every draw the world makes is recorded as an [`RngDraw`], so two runs that diverge are
//! compared at the first differing point rather than "somewhere".

use rand::RngCore;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::time::Tick;

/// The named places where the engine is allowed to draw.
///
/// The discriminant is part of the key, so adding a variant at the end never changes the
/// values of the existing points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u32)]
pub enum DrawPoint {
    /// An actor's decision among its verbs.
    Decision = 1,
    /// A battle in a province.
    Battle = 2,
    /// A random event in a province.
    Event = 3,
    /// Mortality of a person.
    Mortality = 4,
    /// Violent death of a polity's ruler (keyed by polity).
    Violence = 5,
    /// A birth to a person.
    Birth = 6,
    /// The household of a house rising from the people (keyed by polity).
    Rise = 7,
    /// A usurpation of the throne (keyed by polity).
    Usurpation = 8,
    /// A polity taking up an institution its neighbours, allies or in-laws hold (keyed by
    /// polity; E2 step 8).
    Adoption = 9,
}

/// One request for randomness: where, when, for whom, which in turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draw {
    pub point: DrawPoint,
    pub tick: Tick,
    /// The id the draw belongs to (actor, province, person…); which table is implied by
    /// the point.
    pub id: u32,
    /// Ordinal of this draw among the draws with the same `(point, tick, id)`, from 0.
    /// The world assigns it (`World::draw`); callers never choose it.
    pub n: u32,
}

/// A journaled draw: the request and the value it produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RngDraw {
    pub draw: Draw,
    pub value: u64,
}

/// The 32-byte ChaCha seed for a draw: run seed, point, tick, id, ordinal, and a domain tag.
fn key(seed: u64, draw: Draw) -> [u8; 32] {
    let mut k = [0u8; 32];
    k[0..8].copy_from_slice(&seed.to_le_bytes());
    k[8..12].copy_from_slice(&(draw.point as u32).to_le_bytes());
    k[12..16].copy_from_slice(&draw.tick.0.to_le_bytes());
    k[16..20].copy_from_slice(&draw.id.to_le_bytes());
    k[20..24].copy_from_slice(&draw.n.to_le_bytes());
    // Domain separation so that another keyed use (e.g. a future map generator) cannot
    // collide with world draws even with equal numeric fields.
    k[24..32].copy_from_slice(b"e2draw02");
    k
}

/// The value of a draw. Pure: no state, no order dependence.
pub fn draw(seed: u64, draw: Draw) -> u64 {
    ChaCha8Rng::from_seed(key(seed, draw)).next_u64()
}

/// A draw as a fraction in `[0, 1)`, for probability checks.
pub fn draw_unit(seed: u64, d: Draw) -> f64 {
    unit(draw(seed, d))
}

/// The fraction in `[0, 1)` of a value already drawn: the systems that journal the value
/// take the fraction from it instead of running the cipher a second time (a tick with a
/// thousand persons ran it four thousand times: #285 review).
pub fn unit(value: u64) -> f64 {
    // 53 bits of mantissa, the standard open-interval construction.
    (value >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_key_same_value_and_keys_differ() {
        let d = Draw { point: DrawPoint::Decision, tick: Tick(7), id: 3, n: 0 };
        assert_eq!(draw(42, d), draw(42, d));
        assert_ne!(draw(42, d), draw(43, d));
        assert_ne!(draw(42, d), draw(42, Draw { id: 4, ..d }));
        assert_ne!(draw(42, d), draw(42, Draw { tick: Tick(8), ..d }));
        assert_ne!(draw(42, d), draw(42, Draw { point: DrawPoint::Battle, ..d }));
        // Two draws of one point for one id in one tick are two different numbers.
        assert_ne!(draw(42, d), draw(42, Draw { n: 1, ..d }));
    }

    #[test]
    fn unit_draw_is_in_half_open_interval() {
        for id in 0..1000 {
            let u = draw_unit(1, Draw { point: DrawPoint::Event, tick: Tick(0), id, n: 0 });
            assert!((0.0..1.0).contains(&u), "{u}");
        }
    }

    /// Pins the construction: a change to the key layout shows up here, not in a save
    /// that silently plays a different game.
    #[test]
    fn value_is_pinned() {
        let d = Draw { point: DrawPoint::Decision, tick: Tick(0), id: 0, n: 0 };
        let v = draw(0, d);
        assert_eq!(v, draw(0, d));
        // Recorded on first implementation; if this changes, every saved seed replays
        // differently and the plan's "bit-exact" promise is broken.
        assert_eq!(v, PINNED_FIRST_DRAW);
    }

    const PINNED_FIRST_DRAW: u64 = 13_227_027_778_929_364_601;
}
