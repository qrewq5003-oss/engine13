//! The world state and the tick (plan §4, §5 — the empty world of E1a).
//!
//! Everything is in ordered collections and derives `Serialize`/`Deserialize`; the save
//! format is this struct (see `save.rs`). Systems arrive in E2 and fill [`World::tick`];
//! until then the tick carries one **stand-in** flow — population moving toward capacity —
//! so that the acceptance tests exercise the ledger, the remainder carry and a save taken
//! in the middle of an accumulated remainder (owner's review of PR #277).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::event::{Event, Stamped};
use crate::ids::{ActorId, ProvinceId};
use crate::ledger::{Account, Ledger, LedgerError, Posting, Source};
use crate::params::Params;
use crate::rng::{self, Draw, DrawPoint, RngDraw};
use crate::time::Tick;

/// A polity as the engine sees it: which provinces it owns and a legitimacy rating.
///
/// `legitimacy` is the one `f64` of E1a; it exists so that save/load exercises the float
/// round trip the old engine needed `float_roundtrip` for (B31).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub provinces: Vec<ProvinceId>,
    pub legitimacy: f64,
}

/// A province as the engine sees it: owner and land capacity in people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Province {
    pub owner: ActorId,
    pub capacity: u64,
    /// Neighbouring provinces, sorted, no duplicates.
    pub neighbours: Vec<ProvinceId>,
}

/// The starting facts the world is built from (E1a stand-in for the scenario schema of E1b).
///
/// Only facts: counts in world units, geography, ownership. No rule numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub start_year: i32,
    /// `(capacity, population, owner, neighbours)` per province, index = `ProvinceId`.
    pub provinces: Vec<(u64, u64, ActorId, Vec<ProvinceId>)>,
    /// Starting treasury in kg of wheat-equivalent per actor, index = `ActorId`.
    pub treasuries: Vec<i64>,
}

/// The whole state of one run. **This is the save format**: see `save.rs` for the rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub seed: u64,
    pub start_year: i32,
    pub tick: Tick,
    pub actors: BTreeMap<ActorId, Actor>,
    pub provinces: BTreeMap<ProvinceId, Province>,
    pub ledger: Ledger,
    /// Draws of the tick being played; drained into the tick report. The ordinal `n` of a
    /// draw is its count among the earlier draws of the tick with the same point and id,
    /// so the journal itself is the counter and nothing else needs saving.
    draws: Vec<RngDraw>,
    /// Events of the tick being played; drained into the tick report.
    events: Vec<Stamped>,
    /// Next event sequence number; saved, so numbering continues across a load.
    next_seq: u64,
}

/// What one tick did: every event, every posting and every draw, nothing more (plan §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickReport {
    pub tick: Tick,
    pub events: Vec<Stamped>,
    pub postings: Vec<Posting>,
    pub draws: Vec<RngDraw>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorldError {
    #[error("province {0:?} is owned by unknown actor {1:?}")]
    UnknownOwner(ProvinceId, ActorId),
    #[error("province {0:?} lists unknown neighbour {1:?}")]
    UnknownNeighbour(ProvinceId, ProvinceId),
    #[error("province {0:?}: neighbours must be sorted and unique")]
    NeighboursUnsorted(ProvinceId),
    #[error("ledger: {0}")]
    Ledger(#[from] LedgerError),
}

impl World {
    /// Build a world from starting facts. Fails on a dangling reference rather than
    /// carrying it (lesson of the old engine's dangling neighbours).
    pub fn from_facts(facts: &Facts, seed: u64) -> Result<World, WorldError> {
        let tick = Tick::ZERO;
        let mut ledger = Ledger::new();
        let mut actors: BTreeMap<ActorId, Actor> = BTreeMap::new();
        for (i, treasury) in facts.treasuries.iter().enumerate() {
            let id = ActorId(i as u32);
            actors.insert(id, Actor { provinces: Vec::new(), legitimacy: 50.0 });
            ledger.open(tick, Account::Treasury(id), *treasury)?;
        }
        let mut provinces: BTreeMap<ProvinceId, Province> = BTreeMap::new();
        let n = facts.provinces.len() as u32;
        for (i, (capacity, population, owner, neighbours)) in facts.provinces.iter().enumerate() {
            let id = ProvinceId(i as u32);
            if !neighbours.windows(2).all(|w| w[0] < w[1]) {
                return Err(WorldError::NeighboursUnsorted(id));
            }
            if let Some(bad) = neighbours.iter().find(|p| p.0 >= n) {
                return Err(WorldError::UnknownNeighbour(id, *bad));
            }
            let actor = actors.get_mut(owner).ok_or(WorldError::UnknownOwner(id, *owner))?;
            actor.provinces.push(id);
            provinces.insert(id, Province { owner: *owner, capacity: *capacity, neighbours: neighbours.clone() });
            ledger.open(tick, Account::Population(id), *population as i64)?;
        }
        // The opening postings belong to no tick report; the world starts settled.
        ledger.drain_postings();
        Ok(World {
            seed,
            start_year: facts.start_year,
            tick,
            actors,
            provinces,
            ledger,
            draws: Vec::new(),
            events: Vec::new(),
            next_seq: 0,
        })
    }

    /// Append an event of the current tick.
    pub fn emit(&mut self, event: Event) {
        self.events.push(Stamped { seq: self.next_seq, tick: self.tick, event });
        self.next_seq += 1;
    }

    /// A keyed draw for this world, journaled. Repeated draws of one point for one id in
    /// one tick get successive ordinals and therefore different numbers.
    pub fn draw(&mut self, point: DrawPoint, id: u32) -> u64 {
        let n = self.draws.iter().filter(|r| r.draw.point == point && r.draw.id == id).count() as u32;
        let d = Draw { point, tick: self.tick, id, n };
        let value = rng::draw(self.seed, d);
        self.draws.push(RngDraw { draw: d, value });
        value
    }

    /// Draws for several ids of one point, **in canonical order** — ascending id, duplicates
    /// collapsed — whatever order the caller holds them in. The ordinal `n` depends on the
    /// order of draws within a `(point, id)` pair, so systems must not let the traversal
    /// order of their code leak into the key (owner's rule after PR #277). Returns the
    /// values keyed by id.
    pub fn draw_each(&mut self, point: DrawPoint, ids: impl IntoIterator<Item = u32>) -> BTreeMap<u32, u64> {
        let sorted: std::collections::BTreeSet<u32> = ids.into_iter().collect();
        sorted.into_iter().map(|id| (id, self.draw(point, id))).collect()
    }

    /// Play one tick. E1: advance time, journal one decision draw per actor, move each
    /// province's population toward its capacity through the ledger (the share per tick is
    /// `params.population_pull_to_capacity_ppm`), and close the tick with `TickPlayed`.
    /// The flow is a stand-in that E2's population system replaces; it exists so the
    /// acceptance tests cover postings and the remainder carry, not an empty world.
    pub fn tick(&mut self, params: &Params) -> Result<TickReport, WorldError> {
        let before = self.ledger.snapshot();
        let actor_ids: Vec<u32> = self.actors.keys().map(|a| a.0).collect();
        let _ = self.draw_each(DrawPoint::Decision, actor_ids);
        let provinces: Vec<(ProvinceId, u64)> = self.provinces.iter().map(|(p, pr)| (*p, pr.capacity)).collect();
        for (p, capacity) in provinces {
            let account = Account::Population(p);
            let gap_people = capacity as i64 - self.ledger.stock(account).unwrap_or(0);
            // gap people = gap·1000 milli; times ppm/1 000 000 = gap·ppm/1000 milli.
            let milli = gap_people * params.population_pull_to_capacity_ppm / 1000;
            self.ledger.post(self.tick, account, Source::Growth, milli)?;
        }
        self.ledger.check_tick(&before)?;
        self.emit(Event::TickPlayed);
        let report = TickReport {
            tick: self.tick,
            events: std::mem::take(&mut self.events),
            postings: self.ledger.drain_postings(),
            draws: std::mem::take(&mut self.draws),
        };
        self.tick = self.tick.next();
        Ok(report)
    }

    pub fn year(&self) -> i32 {
        self.tick.year(self.start_year)
    }

    pub fn population(&self, p: ProvinceId) -> i64 {
        self.ledger.stock(Account::Population(p)).unwrap_or(0)
    }

    pub fn treasury(&self, a: ActorId) -> i64 {
        self.ledger.stock(Account::Treasury(a)).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld;

    fn params() -> Params {
        Params::load().unwrap()
    }

    #[test]
    fn dangling_owner_and_neighbour_are_refused() {
        let mut facts = testworld::five_states();
        facts.provinces[0].2 = ActorId(99);
        assert!(matches!(World::from_facts(&facts, 1), Err(WorldError::UnknownOwner(..))));
        let mut facts = testworld::five_states();
        facts.provinces[0].3 = vec![ProvinceId(999)];
        assert!(matches!(World::from_facts(&facts, 1), Err(WorldError::UnknownNeighbour(..))));
    }

    #[test]
    fn tick_advances_time_and_reports_one_draw_per_actor() {
        let mut w = World::from_facts(&testworld::five_states(), 7).unwrap();
        let r = w.tick(&params()).unwrap();
        assert_eq!(r.tick, Tick(0));
        assert_eq!(r.draws.len(), w.actors.len());
        assert_eq!(r.postings.len(), w.provinces.len());
        assert!(r.postings.iter().all(|p| p.source == Source::Growth));
        assert_eq!(r.events.len(), 1);
        assert!(matches!(r.events[0].event, Event::TickPlayed));
        assert_eq!(r.events[0].seq, 0);
        assert_eq!(w.tick, Tick(1));
        assert_eq!(w.year(), 1477);
        let r2 = w.tick(&params()).unwrap();
        assert_eq!(r2.events[0].seq, 1, "sequence numbers continue across ticks");
    }

    #[test]
    fn draw_each_is_canonical_whatever_the_callers_order() {
        let mut a = World::from_facts(&testworld::five_states(), 11).unwrap();
        let mut b = World::from_facts(&testworld::five_states(), 11).unwrap();
        let va = a.draw_each(DrawPoint::Battle, [4, 1, 3, 1, 2]);
        let vb = b.draw_each(DrawPoint::Battle, [2, 3, 1, 4]);
        assert_eq!(va, vb);
        assert_eq!(va.len(), 4);
        // And a second round for the same ids gets the next ordinals, still canonically.
        let va2 = a.draw_each(DrawPoint::Battle, [3, 4, 2, 1]);
        let vb2 = b.draw_each(DrawPoint::Battle, [1, 2, 3, 4]);
        assert_eq!(va2, vb2);
        assert_ne!(va, va2);
    }

    #[test]
    fn repeated_draws_of_one_point_in_one_tick_differ() {
        let mut w = World::from_facts(&testworld::five_states(), 7).unwrap();
        let a = w.draw(DrawPoint::Battle, 3);
        let b = w.draw(DrawPoint::Battle, 3);
        assert_ne!(a, b);
        let r = w.tick(&params()).unwrap();
        let ordinals: Vec<u32> = r.draws.iter().filter(|d| d.draw.point == DrawPoint::Battle).map(|d| d.draw.n).collect();
        assert_eq!(ordinals, vec![0, 1]);
    }

    #[test]
    fn stand_in_growth_moves_population_toward_capacity_and_keeps_a_remainder() {
        let mut w = World::from_facts(&testworld::five_states(), 7).unwrap();
        let p = ProvinceId(0);
        let cap = w.provinces[&p].capacity as i64;
        let before = w.population(p);
        for _ in 0..10 {
            w.tick(&params()).unwrap();
        }
        let after = w.population(p);
        assert!(after > before && after <= cap, "{before} -> {after} <= {cap}");
        // 10 000 - 6 000 = 4 000 gap: 4 000 milli a tick at first, shrinking; a remainder appears.
        assert!(w.ledger.carry(Account::Population(p)).unwrap() > 0);
    }
}
