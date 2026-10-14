//! The population system (brief §2 «Население», plan §15): people in provinces grow toward
//! the land's capacity, starve above it, die of plague where plague exists, and migrate
//! toward emptier neighbours — all through the ledger, so every person is accounted for.
//!
//! - **Growth** (E0 П2, kind (а) derived from the observed П7): logistic,
//!   `P × r/2 × (1 − P/K)` a tick, in thousandths with the remainder carried, so a province
//!   of 400 people at 0.1 % a year still grows (owner's rule, E1a).
//! - **Famine** (E0 П3): while a province holds more people than its land carries, it loses
//!   up to `famine.mortality_per_year / 2` of them a tick, but never more than the overshoot
//!   itself — a province one person over capacity loses one person, not five per cent of
//!   itself (the rounding of growth can overshoot by a person). Nothing in this step pushes
//!   a province far over capacity (foraging and capacity shocks come with war), so the event
//!   is declared possible only when something can cause it.
//! - **Plague** (E0 П4): a fact of the world — the scenario says whether plague is in its
//!   pool of diseases (brief §0.2: a fact, like culture). Where it is, every province draws
//!   once a tick (`DrawPoint::Event`, keyed by province, canonical order) and an outbreak
//!   takes `plague.mortality` of its people. **The first randomness in the engine:** the
//!   acceptance now plays each world with several game seeds (plan §13).
//! - **Migration** (E0 П5, kind (в) checked through П7): people move from a fuller
//!   province to an emptier land neighbour at `migration.rate × P × (fill gap)` a tick,
//!   posted out and in with the same thousandths — the sum of people never changes.
//!
//! Order inside the phase is canonical (ascending province id). The remainder carry means
//! a posting's whole part is the ceiling for losses and the floor for gains.

use std::collections::BTreeMap;

use crate::event::Event;
use crate::ids::ProvinceId;
use crate::ledger::{Account, Source};
use crate::params::Params;
use crate::rng::{Draw, DrawPoint};
use crate::world::{Disease, World, WorldError};

impl World {
    /// The population phase of a tick.
    pub(crate) fn phase_population(&mut self, params: &Params) -> Result<(), WorldError> {
        let ids: Vec<ProvinceId> = self.provinces.keys().copied().collect();
        let plague = self.diseases.contains(&Disease::Plague);

        // Growth or famine, per province.
        for p in &ids {
            let account = Account::Population(*p);
            let people = self.ledger.stock(account).unwrap_or(0);
            let capacity = self.provinces[p].capacity as i64;
            if people <= capacity {
                // Logistic growth in thousandths: P·1000 × r_tick × (1 − P/K).
                let room = if capacity == 0 { 0 } else { (capacity - people) * 1_000_000 / capacity };
                let milli = people * params.population_growth_ppm_per_tick / 1000 * room / 1_000_000;
                if milli > 0 {
                    self.post(account, Source::Growth, milli)?;
                }
            } else {
                // Famine takes no more than the overshoot (so a province one person over its
                // capacity loses one person, not five per cent of itself), and no faster than
                // the П3 rate.
                let overshoot_milli = (people - capacity) * 1000 + self.ledger.carry(account).unwrap_or(0);
                let rate_milli = people * params.famine_mortality_ppm_per_tick / 1000;
                let milli = -overshoot_milli.min(rate_milli);
                if milli < 0 {
                    self.post(account, Source::Famine, milli)?;
                    let dead = (-milli as u64).div_ceil(1000);
                    self.emit(Event::Famine { province: *p, dead });
                }
            }
        }

        // Plague: one keyed draw per province, in canonical order.
        if plague {
            for p in &ids {
                let u = {
                    let d = Draw { point: DrawPoint::Event, tick: self.tick, id: p.0, n: self.draw_ordinal(DrawPoint::Event, p.0) };
                    let value = crate::rng::draw(self.seed, d);
                    self.journal_draw(d, value);
                    crate::rng::unit(value)
                };
                if u < params.plague_outbreak_probability_per_tick {
                    let account = Account::Population(*p);
                    let people = self.ledger.stock(account).unwrap_or(0);
                    let milli = -(people * params.plague_mortality_ppm / 1000);
                    if milli < 0 {
                        self.post(account, Source::Plague, milli)?;
                        let dead = (-milli as u64).div_ceil(1000);
                        self.emit(Event::Plague { province: *p, dead });
                    }
                }
            }
        }

        // Migration: from each province to its emptiest land neighbour when the fill gap is
        // worth the move; out and in with the same thousandths.
        let fills: BTreeMap<ProvinceId, i64> = ids
            .iter()
            .map(|p| {
                let cap = self.provinces[p].capacity as i64;
                let people = self.ledger.stock(Account::Population(*p)).unwrap_or(0);
                (*p, if cap == 0 { 1_000_000 } else { people * 1_000_000 / cap })
            })
            .collect();
        let mut moves: Vec<(ProvinceId, ProvinceId, i64)> = Vec::new();
        for p in &ids {
            let fill = fills[p];
            let Some((to, gap)) = self.provinces[p]
                .neighbours
                .iter()
                .map(|n| (*n, fill - fills[n]))
                .filter(|(_, gap)| *gap > params.migration_min_gap_ppm)
                .max_by_key(|(n, gap)| (*gap, std::cmp::Reverse(*n)))
            else {
                continue;
            };
            let people = self.ledger.stock(Account::Population(*p)).unwrap_or(0);
            // migrants·1000 = P × rate × gap, rate in ppm per tick, gap in ppm.
            let milli = people * params.migration_rate_ppm_per_tick / 1000 * gap / 1_000_000;
            if milli > 0 {
                moves.push((*p, to, milli));
            }
        }
        // Several provinces may send to the same emptiest neighbour in one tick: each
        // inflow is capped by the room still left there, in canonical order of senders, so
        // migration never pushes a province over its capacity.
        let mut received: BTreeMap<ProvinceId, i64> = BTreeMap::new();
        for (from, to, wanted) in moves {
            let room = (self.provinces[&to].capacity as i64 - self.ledger.stock(Account::Population(to)).unwrap_or(0)) * 1000
                - self.ledger.carry(Account::Population(to)).unwrap_or(0)
                - received.get(&to).copied().unwrap_or(0);
            let milli = wanted.min(room.max(0));
            if milli <= 0 {
                continue;
            }
            *received.entry(to).or_insert(0) += milli;
            self.post(Account::Population(from), Source::MigrationOut, -milli)?;
            self.post(Account::Population(to), Source::MigrationIn, milli)?;
            self.emit(Event::Migration { from, to, people: (milli as u64) / 1000 });
        }
        Ok(())
    }
}
