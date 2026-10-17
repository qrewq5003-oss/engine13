//! The army (brief §2 «Армия», §2a, plan §17): war as the minimal relation a battle needs,
//! the battle with a winner and an end, the siege that changes a province's owner, the fall
//! of a polity without land, and the verbs of the levy, the disbanding and the hire.
//!
//! **War** is a set of pairs of polities. Nothing in a game build opens one: `declare_war`
//! is closed in the offers («awaits diplomacy» — a verb without a price is a degeneration,
//! the same rule as the tax), and the tests open wars through [`World::declare_war`] behind
//! the `testing` feature. A war closes by itself when a side falls; peace with its terms and
//! its term (E0 Д1) is diplomacy's.
//!
//! **Battle** (phase after the march): in every province where units of two polities at war
//! stand, one battle a tick between those two sides — all their units there. Strength of a
//! side = Σ soldiers × (0.5 + quality / 100). The attacker wins with probability
//! `s_A^k / (s_A^k + s_B^k)` (k = `battle.strength_exponent`, a model parameter checked
//! through А13), by a keyed draw at `DrawPoint::Battle` on the province. Losses by E0 А6,
//! the winner's and the loser's share, pro rata over each side's units, through the ledger
//! (`Source::BattleLoss`). The loser's units fall back to the nearest province of their own
//! through passable land; a unit with nowhere to go is destroyed. The defender is the
//! province's owner when it takes part, else the lower id.
//!
//! **Siege** (phase after the battle): an enemy standing in a province with no unit of the
//! owner's besieges it. An ordinary province is stormed within the tick and changes hands at
//! once (E0 А9); a capital holds `siege.capital_ticks` consecutive ticks. Until diplomacy,
//! occupation is annexation — a simplification the owner recorded: with treaties, «held»
//! and «ceded» part ways. A polity that loses its capital moves it to its most populous
//! province; a polity that loses its last province **falls**: its levies join the people of
//! the province they stand in and its mercenaries leave the world (`Source::Disband`), its
//! wars close, its record stays (no dangling ids) with no land, and `alive` is false.
//!
//! **Verbs.** `recruit` moves people of one own province into soldiers — a transfer, the
//! sum of people and soldiers unchanged — within the levy ceiling (E0 А1, a share of the
//! polity's people) and the rate of a tick (А2); the men join a levy unit standing there or
//! form a new one. `disband` sends a levy back to the people of the province it stands in;
//! mercenaries leave the world. `hire_mercenaries` is open to every polity (E0, second
//! round: the market existed in all three ages, so it is no institution): the advance (А4)
//! leaves the treasury and the men come from outside the world (`Source::Hire`), as many as
//! the advance covers within the ceiling, into a unit of their own at the capital. Because
//! verbs apply before the tick, a loan taken and an advance paid in the same tick meet before
//! the economy sweeps the surplus into repayment (owner's note on #283).
//!
//! Supply at war is in `movement.rs`: pay at home, foraging with its cost (E0 П6) on foreign
//! land only — an unpaid army at home melts, it does not live off its own people.
//!
//! Quality stays at 50 — its dynamics (experience, idleness) are this system's line for E3.

use std::collections::BTreeMap;

use crate::economy::VerbError;
use crate::event::{Event, Outcome, Transfer};
use crate::ids::{ActorId, ProvinceId, UnitId};
use crate::ledger::{Account, Source};
use crate::params::Params;
use crate::rng::DrawPoint;
use crate::verbs::{Target, VerbCall, VerbId};
use crate::world::{Unit, World, WorldError};

/// A pair of polities as the maps of wars, treaties and relations key it: lower id first.
pub(crate) fn pair(a: ActorId, b: ActorId) -> (ActorId, ActorId) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// The chance that strength `a` beats strength `b` in a battle, `a^k / (a^k + b^k)` with
/// k = `battle.strength_exponent` (E0 А13): the one formula the battles draw by and the
/// bots' worth reads (E3 (е1): the bot calls the world's formulas, never copies them).
pub fn win_chance(a: f64, b: f64, params: &Params) -> f64 {
    let k = params.battle_strength_exponent;
    a.powf(k) / (a.powf(k) + b.powf(k))
}

/// The share of its soldiers a side loses in a battle, parts per million (E0 А6): the
/// winner's or the loser's.
pub fn battle_losses_ppm(won: bool, params: &Params) -> i64 {
    if won {
        params.battle_losses_winner_ppm
    } else {
        params.battle_losses_loser_ppm
    }
}

impl World {
    /// A polity is alive while it holds land and its ruler lives: a polity whose last house
    /// died out has fallen with its land (`politics.rs`), one conquered has none.
    pub fn alive(&self, a: ActorId) -> bool {
        self.actors.get(&a).is_some_and(|x| !x.provinces.is_empty() && self.person_alive(x.ruler))
    }

    pub fn at_war(&self, a: ActorId, b: ActorId) -> bool {
        a != b && self.wars.contains_key(&pair(a, b))
    }

    /// Open a war without a price. For the tests only: the verb `declare_war` pays in
    /// legitimacy (`diplomacy.rs`) and then comes here.
    #[cfg(feature = "testing")]
    pub fn declare_war(&mut self, a: ActorId, b: ActorId) -> Result<(), WorldError> {
        self.open_war(a, b)
    }

    /// The war itself, with its obligations (E2 step 7, plan §19.2 p. 3–4, owner's rule):
    /// the attacked side's allies, its lord and its vassals enter the war against the
    /// attacker, and the attacker's vassals enter against the attacked — **their entry is no
    /// declaration**: it costs nothing, breaks no treaty, and pulls nobody further in, so one
    /// war does not light the world in a chain. An entrant whose peace with the other side
    /// is in force stays out: the treaty holds.
    pub(crate) fn open_war(&mut self, attacker: ActorId, victim: ActorId) -> Result<(), WorldError> {
        if attacker == victim {
            return Err(WorldError::WarWithSelf(attacker));
        }
        for x in [attacker, victim] {
            if !self.actors.contains_key(&x) {
                return Err(WorldError::UnknownActor(x));
            }
        }
        self.war_between(attacker, victim, None, attacker);
        let principal = Some(pair(attacker, victim));
        let mut with_victim: Vec<ActorId> = self.allies_of(victim);
        with_victim.extend(self.lord_of(victim));
        with_victim.extend(self.vassals_of(victim));
        for x in with_victim {
            if x != attacker && self.alive(x) && !self.peace_in_force(x, attacker) {
                self.war_between(x, attacker, principal, attacker);
            }
        }
        for x in self.vassals_of(attacker) {
            if x != victim && self.alive(x) && !self.peace_in_force(x, victim) {
                self.war_between(x, victim, principal, x);
            }
        }
        Ok(())
    }

    /// Record a war between a pair if there is none, with its event; `of` names the
    /// principal pair when the war is an obligation of theirs.
    fn war_between(&mut self, a: ActorId, b: ActorId, of: Option<(ActorId, ActorId)>, by: ActorId) {
        let (a, b) = pair(a, b);
        if !self.wars.contains_key(&(a, b)) {
            let held = (self.actors[&a].provinces.len() as u32, self.actors[&b].provinces.len() as u32);
            self.wars.insert((a, b), crate::world::War { since: self.tick, taken: (0, 0), held, of, by: Some(by) });
            self.emit(Event::War { a, b });
        }
    }

    /// All soldiers of a polity, levies and mercenaries.
    pub fn army_size(&self, a: ActorId) -> i64 {
        self.units_of(a).iter().map(|u| self.soldiers(*u)).sum()
    }

    /// The levy ceiling (E0 А1): a share of the polity's people — those at home and those
    /// already under arms, so that raising a levy does not lower the ceiling it counts against.
    /// The militia a province levies at the ceiling of А1: what a polity born of a rising
    /// raises in it (`collapse.rs`), and what the worth of a secession counts on.
    pub fn province_levy(&self, q: ProvinceId, params: &Params) -> i64 {
        self.population(q) * params.recruit_ceiling_ppm / 1_000_000
    }

    pub fn recruit_ceiling(&self, a: ActorId, params: &Params) -> i64 {
        let people: i64 = self.actors[&a].provinces.iter().map(|p| self.population(*p)).sum::<i64>() + self.army_size(a);
        people * params.recruit_ceiling_ppm / 1_000_000
    }

    /// Room under the ceiling.
    pub fn recruit_room(&self, a: ActorId, params: &Params) -> i64 {
        (self.levy_ceiling(a, params) - self.army_size(a)).max(0)
    }

    /// The ceiling a levy can draw to (plan §27.1, owner's answer 1 on §27.5): the ceiling of
    /// А1 less the men of its levies killed and not yet replaced by the cohort growing up
    /// (`Actor::manpower_deficit`), never below nothing. After a lost army there is nobody to
    /// levy: the cohort was spent, and comes back over the years a man stays in it
    /// (`Params::manpower_renewal_years`, from the mortality table).
    pub fn levy_ceiling(&self, a: ActorId, params: &Params) -> i64 {
        (self.recruit_ceiling(a, params) as f64 - self.actors[&a].manpower_deficit).max(0.0) as i64
    }

    /// Men a levy can raise this tick: the rate of a tick (А2) of what can be levied, within
    /// the room.
    pub fn recruit_per_tick(&self, a: ActorId, params: &Params) -> i64 {
        (self.levy_ceiling(a, params) * params.recruit_rate_ppm_per_tick / 1_000_000).min(self.recruit_room(a, params))
    }

    /// Mercenaries the treasury's advance covers within the ceiling of А1 — the men come from
    /// outside the world, so the polity's spent cohort does not hold them back.
    pub fn hire_count(&self, a: ActorId, params: &Params) -> i64 {
        if params.mercenary_advance_kg <= 0 {
            return 0;
        }
        (self.treasury(a) / params.mercenary_advance_kg).min((self.recruit_ceiling(a, params) - self.army_size(a)).max(0)).max(0)
    }

    /// The men of a levy killed: they leave its polity's ceiling until the cohort grows up.
    fn levy_dead(&mut self, unit: UnitId, men: i64) {
        if men > 0 && !self.units[&unit].mercenary {
            let owner = self.units[&unit].owner;
            if let Some(actor) = self.actors.get_mut(&owner) {
                actor.manpower_deficit += men as f64;
            }
        }
    }

    fn strength(&self, units: &[UnitId]) -> f64 {
        units.iter().map(|u| self.unit_strength(*u)).sum()
    }

    /// A unit's strength in battle: soldiers × (0.5 + quality/100).
    pub fn unit_strength(&self, u: UnitId) -> f64 {
        self.soldiers(u) as f64 * (0.5 + self.units[&u].quality / 100.0)
    }

    /// The strength of a polity's whole army, by the battle's measure.
    pub fn army_strength(&self, a: ActorId) -> f64 {
        self.strength(&self.units_of(a))
    }

    /// The ticks an unopposed enemy needs to take a province: a capital holds
    /// `siege.capital_ticks`, any other falls within the tick (E0 А9).
    pub fn siege_ticks(&self, p: ProvinceId, params: &Params) -> u32 {
        let owner = self.provinces[&p].owner;
        if self.actors[&owner].capital == p {
            params.siege_capital_ticks
        } else {
            1
        }
    }

    /// Units standing in `p`, grouped by owner in canonical order.
    fn units_in(&self, p: ProvinceId) -> BTreeMap<ActorId, Vec<UnitId>> {
        let mut out: BTreeMap<ActorId, Vec<UnitId>> = BTreeMap::new();
        for (id, u) in &self.units {
            if u.position == p {
                out.entry(u.owner).or_default().push(*id);
            }
        }
        out
    }

    /// The battle phase: one battle per province between the first pair of sides at war.
    pub(crate) fn phase_battle(&mut self, params: &Params) -> Result<(), WorldError> {
        let provinces: Vec<ProvinceId> = self.provinces.keys().copied().collect();
        for p in provinces {
            let present = self.units_in(p);
            let owners: Vec<ActorId> = present.keys().copied().collect();
            let Some((x, y)) = owners.iter().enumerate().find_map(|(i, a)| owners[i + 1..].iter().find(|b| self.at_war(*a, **b)).map(|b| (*a, *b))) else {
                continue;
            };
            let holder = self.provinces[&p].owner;
            let (defender, attacker) = if y == holder { (y, x) } else { (x, y) };
            let men = |side: ActorId| present[&side].iter().map(|u| self.soldiers(*u)).sum::<i64>() as u64;
            let soldiers = (men(attacker), men(defender));
            let (s_att, s_def) = (self.strength(&present[&attacker]), self.strength(&present[&defender]));
            if s_att <= 0.0 && s_def <= 0.0 {
                continue;
            }
            let p_att = win_chance(s_att, s_def, params);
            let u = (self.draw(DrawPoint::Battle, p.0) >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
            let attacker_won = u < p_att;
            let (winner, loser) = if attacker_won { (attacker, defender) } else { (defender, attacker) };
            let mut lost = BTreeMap::new();
            for (side, share) in [(winner, battle_losses_ppm(true, params)), (loser, battle_losses_ppm(false, params))] {
                let mut sum = 0i64;
                for u in &present[&side] {
                    let soldiers = self.soldiers(*u);
                    let milli = soldiers * share / 1000;
                    if milli > 0 {
                        self.post(Account::Soldiers(*u), Source::BattleLoss, -milli)?;
                    }
                    let dead = soldiers - self.soldiers(*u);
                    self.levy_dead(*u, dead);
                    sum += dead;
                }
                lost.insert(side, sum as u64);
            }
            self.emit(Event::Battle {
                province: p,
                attacker,
                defender,
                outcome: if attacker_won { Outcome::AttackerWon } else { Outcome::DefenderWon },
                soldiers,
                losses: (lost[&attacker], lost[&defender]),
            });
            for u in &present[&loser] {
                self.retreat(*u)?;
            }
        }
        Ok(())
    }

    /// A beaten unit falls back to the nearest province of its own; with none reachable it
    /// is destroyed.
    fn retreat(&mut self, unit: UnitId) -> Result<(), WorldError> {
        let (owner, at) = (self.units[&unit].owner, self.units[&unit].position);
        match self.nearest_own(owner, at) {
            Some(to) => {
                let u = self.units.get_mut(&unit).unwrap();
                u.position = to;
                u.destination = None;
                self.emit(Event::Retreat { unit, to });
            }
            None => {
                let soldiers = self.soldiers(unit);
                if soldiers > 0 {
                    self.post(Account::Soldiers(unit), Source::BattleLoss, -soldiers * 1000)?;
                }
                self.levy_dead(unit, soldiers);
                self.units.remove(&unit);
                self.emit(Event::UnitDestroyed { unit });
            }
        }
        Ok(())
    }

    /// The siege phase: an unopposed enemy takes a province — at once, or a capital after
    /// `siege.capital_ticks` consecutive ticks.
    pub(crate) fn phase_siege(&mut self, params: &Params) -> Result<(), WorldError> {
        let provinces: Vec<ProvinceId> = self.provinces.keys().copied().collect();
        for p in provinces {
            let owner = self.provinces[&p].owner;
            let present = self.units_in(p);
            let defended = present.contains_key(&owner);
            let besieger = present.keys().copied().find(|a| self.at_war(*a, owner));
            let Some(by) = besieger.filter(|_| !defended) else {
                self.provinces.get_mut(&p).unwrap().siege = None;
                continue;
            };
            let need = self.siege_ticks(p, params);
            if need > 1 {
                let ticks = match self.provinces[&p].siege {
                    Some(s) if s.by == by => s.ticks + 1,
                    _ => 1,
                };
                if ticks < need {
                    self.provinces.get_mut(&p).unwrap().siege = Some(crate::world::Siege { by, ticks });
                    continue;
                }
            }
            self.transfer(p, owner, by, Transfer::Conquest)?;
        }
        Ok(())
    }

    /// A province changes hands; the loser's capital moves or the loser falls.
    pub(crate) fn transfer(&mut self, p: ProvinceId, from: ActorId, to: ActorId, how: Transfer) -> Result<(), WorldError> {
        // A new owner: the siege is over and so is a revolt against the old one.
        self.provinces.get_mut(&p).unwrap().siege = None;
        self.provinces.get_mut(&p).unwrap().revolt = None;
        self.provinces.get_mut(&p).unwrap().owner = to;
        let f = self.actors.get_mut(&from).unwrap();
        f.provinces.retain(|x| *x != p);
        let t = self.actors.get_mut(&to).unwrap();
        t.provinces.push(p);
        t.provinces.sort();
        // The war's record: who has taken what in it (peace reads it, `diplomacy.rs`).
        if how == Transfer::Conquest {
            if let Some(war) = self.wars.get_mut(&pair(from, to)) {
                if to < from {
                    war.taken.0 += 1;
                } else {
                    war.taken.1 += 1;
                }
            }
        }
        self.emit(Event::ProvinceChangedHands { province: p, from, to, how });
        if self.actors[&from].provinces.is_empty() {
            self.fall(from)?;
        } else if self.actors[&from].capital == p {
            // The most populous province left; the lowest id among equals.
            let new_capital = *self.actors[&from].provinces.iter().max_by_key(|x| (self.population(**x), std::cmp::Reverse(**x))).unwrap();
            self.actors.get_mut(&from).unwrap().capital = new_capital;
            self.emit(Event::CapitalMoved { actor: from, to: new_capital });
        }
        Ok(())
    }

    /// A polity falls — without land, or without anyone to take its throne: levies join
    /// the people where they stand, mercenaries leave the world, wars close, the record stays.
    pub(crate) fn fall(&mut self, a: ActorId) -> Result<(), WorldError> {
        for u in self.units_of(a) {
            self.disband_unit(u)?;
        }
        self.wars.retain(|(x, y), _| *x != a && *y != a);
        self.treaties.retain(|(x, y), _| *x != a && *y != a);
        // A fallen lord frees its vassals (plan §19.2 p. 3).
        for v in self.vassals_of(a) {
            self.actors.get_mut(&v).unwrap().vassal_of = None;
            self.emit(Event::Released { vassal: v, lord: a });
        }
        self.emit(Event::ActorDied { actor: a });
        Ok(())
    }

    /// Disband a unit: a levy back to the people of the province it stands in, mercenaries
    /// out of the world. Both legs signed `Disband`.
    pub(crate) fn disband_unit(&mut self, unit: UnitId) -> Result<u64, WorldError> {
        let (at, mercenary) = (self.units[&unit].position, self.units[&unit].mercenary);
        let soldiers = self.soldiers(unit);
        if soldiers > 0 {
            self.post(Account::Soldiers(unit), Source::Disband, -soldiers * 1000)?;
            if !mercenary {
                self.post(Account::Population(at), Source::Disband, soldiers * 1000)?;
            }
        }
        self.units.remove(&unit);
        self.emit(Event::Disbanded { unit, soldiers: soldiers as u64 });
        Ok(soldiers as u64)
    }

    pub(crate) fn new_unit(&mut self, owner: ActorId, at: ProvinceId, mercenary: bool) -> Result<UnitId, WorldError> {
        let id = UnitId(self.next_unit);
        self.next_unit += 1;
        self.units.insert(id, Unit { owner, position: at, quality: 50.0, destination: None, mercenary, house: None });
        self.ledger.open(self.tick, Account::Soldiers(id), 0)?;
        Ok(id)
    }

    /// The army's verbs: `recruit` (target: an own province), `disband` (target: an own
    /// unit), `hire_mercenaries` (no target). Called by `apply_verb` for a living ruler.
    pub(crate) fn apply_army_verb(&mut self, call: &VerbCall, a: ActorId, params: &Params) -> Result<(), VerbError> {
        let ledger = |_: WorldError| VerbError::NotAvailable(call.verb, "ledger");
        match call.verb {
            VerbId::Recruit => {
                let Target::Province(p) = call.target else { return Err(VerbError::BadTarget(call.verb, "a province to levy from")) };
                if self.provinces.get(&p).map(|x| x.owner) != Some(a) {
                    return Err(VerbError::BadTarget(call.verb, "not the polity's own province"));
                }
                if self.in_revolt(p) {
                    return Err(VerbError::NotAvailable(call.verb, "the province is in revolt"));
                }
                let n = self.recruit_per_tick(a, params).min(self.population(p));
                if n <= 0 {
                    return Err(VerbError::NotAvailable(call.verb, "at the levy ceiling"));
                }
                let unit = match self.units.iter().find(|(_, u)| u.owner == a && u.position == p && !u.mercenary && u.house.is_none()) {
                    Some((id, _)) => *id,
                    None => self.new_unit(a, p, false).map_err(ledger)?,
                };
                self.post(Account::Population(p), Source::Recruit, -n * 1000).map_err(ledger)?;
                self.post(Account::Soldiers(unit), Source::Recruit, n * 1000).map_err(ledger)?;
                self.emit(Event::Recruited { unit, soldiers: n as u64 });
                Ok(())
            }
            VerbId::Disband => {
                let Target::Unit(u) = call.target else { return Err(VerbError::BadTarget(call.verb, "a unit to disband")) };
                if self.units.get(&u).map(|x| x.owner) != Some(a) {
                    return Err(VerbError::BadTarget(call.verb, "not the polity's own unit"));
                }
                self.disband_unit(u).map_err(ledger)?;
                Ok(())
            }
            VerbId::HireMercenaries => {
                let n = self.hire_count(a, params);
                if n <= 0 {
                    return Err(VerbError::NotAvailable(call.verb, if self.recruit_room(a, params) <= 0 { "at the levy ceiling" } else { "cannot pay an advance" }));
                }
                let advance = n * params.mercenary_advance_kg;
                let capital = self.actors[&a].capital;
                let unit = self.new_unit(a, capital, true).map_err(ledger)?;
                self.post(Account::Treasury(a), Source::Hire, -advance * 1000).map_err(ledger)?;
                self.post(Account::Soldiers(unit), Source::Hire, n * 1000).map_err(ledger)?;
                self.emit(Event::Hired { unit, soldiers: n as u64, advance_kg: advance as u64 });
                Ok(())
            }
            _ => Err(VerbError::NotAvailable(call.verb, "not an army verb")),
        }
    }
}
