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
//!   **The fill a migrant compares is the land the tax leaves him** (owner's rule after
//!   #295: people fled heavy taxes — Lewis 1993 on the anachoresis of Roman Egypt): people
//!   over the land the rate their owner assesses leaves them ([`World::migration_fill_ppm`]).
//!   Derived, no weight: capacity is the density observed at the Malthusian ceiling (E0 П1)
//!   under the ordinary burden of its age, the tithe, where what people keep feeds them; a
//!   rate `t` leaves food for `(1 − t)/(1 − tithe)` of them. Inside one polity the
//!   rate is one and nothing changes; across a border people drift from the heavier tax to
//!   the lighter, and a province in revolt, paying nothing, draws them.
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
    /// The share of their harvest a province's people are assessed, parts per million: the
    /// tax rate of its owner — nothing in a province in revolt (it pays no tax) or of a
    /// fallen polity.
    pub fn tax_burden_ppm(&self, p: ProvinceId) -> i64 {
        let owner = self.provinces[&p].owner;
        if !self.alive(owner) || self.in_revolt(p) {
            return 0;
        }
        self.actors[&owner].tax_rate_ppm
    }

    /// The fill a migrant compares, parts per million: the people over the land the burden
    /// leaves them (owner's rule after #295; derived, no weight). The capacity is the density
    /// observed at the Malthusian ceiling (E0 П1) under the ordinary burden of its age, the
    /// tithe: a rate `t` leaves food for `(1 − t)/(1 − tithe)` of it. At the tithe the fill is
    /// the plain fill — a world where every polity taxes the tithe migrates as before. A
    /// province of no capacity counts as full.
    pub fn migration_fill_ppm(&self, p: ProvinceId, burden_ppm: i64, params: &Params) -> i64 {
        let cap = self.provinces[&p].capacity as i64;
        if cap == 0 {
            return 1_000_000;
        }
        let people = self.ledger.stock(Account::Population(p)).unwrap_or(0);
        people * 1_000_000 / cap * (1_000_000 - params.tax_default_ppm) / (1_000_000 - burden_ppm).max(1)
    }

    /// Every province's migration fill under the burdens as they stand.
    pub fn migration_fills(&self, params: &Params) -> BTreeMap<ProvinceId, i64> {
        self.provinces.keys().map(|p| (*p, self.migration_fill_ppm(*p, self.tax_burden_ppm(*p), params))).collect()
    }

    /// Where the people of `p` would go a tick under `fills` (E0 П5): the land neighbour with
    /// the largest gap above `migration.min_gap` (ties: the lower id), the gap, and the
    /// migrants wanted in thousandths — `P × rate × gap` — before the room at the other end
    /// caps them.
    pub fn migration_wanted(&self, p: ProvinceId, fills: &BTreeMap<ProvinceId, i64>, params: &Params) -> Option<(ProvinceId, i64, i64)> {
        let fill = fills[&p];
        let (to, gap) = self.provinces[&p]
            .neighbours
            .iter()
            .map(|n| (*n, fill - fills[n]))
            .filter(|(_, gap)| *gap > params.migration_min_gap_ppm)
            .max_by_key(|(n, gap)| (*gap, std::cmp::Reverse(*n)))?;
        let people = self.ledger.stock(Account::Population(p)).unwrap_or(0);
        // migrants·1000 = P × rate × gap, rate in ppm per tick, gap in ppm.
        Some((to, gap, people * params.migration_rate_ppm_per_tick / 1000 * gap / 1_000_000))
    }

    /// How many migrants from `from` to `to` would close a fill gap of `gap_ppm` down to the
    /// threshold of a move (`migration.min_gap`), the burdens of the two provinces given: each
    /// migrant lowers the one fill and raises the other by a head over the land the burden
    /// leaves (`migration_fill_ppm`). The world's own limit of a flight, read by the bots'
    /// worth (E3).
    pub fn migrants_to_close(&self, from: ProvinceId, to: ProvinceId, gap_ppm: i64, burdens: (i64, i64), params: &Params) -> f64 {
        let per_head = |q: ProvinceId, burden: i64| {
            let cap = self.provinces[&q].capacity as f64;
            if cap <= 0.0 {
                0.0
            } else {
                1e6 / cap * (1e6 - params.tax_default_ppm as f64) / (1e6 - burden as f64).max(1.0)
            }
        };
        let closing = per_head(from, burdens.0) + per_head(to, burdens.1);
        if closing <= 0.0 {
            return 0.0;
        }
        (gap_ppm - params.migration_min_gap_ppm).max(0) as f64 / closing
    }

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

        // Migration: from each province to its emptiest land neighbour, by the fill the tax
        // leaves, when the gap is worth the move; out and in with the same thousandths.
        let fills = self.migration_fills(params);
        let mut moves: Vec<(ProvinceId, ProvinceId, i64)> = Vec::new();
        for p in &ids {
            if let Some((to, _, milli)) = self.migration_wanted(*p, &fills, params) {
                if milli > 0 {
                    moves.push((*p, to, milli));
                }
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
