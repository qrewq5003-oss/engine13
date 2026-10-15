//! Units on the province graph: movement and supply (brief §2a, plan §14, §16).
//!
//! **Edges.** A province has land neighbours and sea neighbours; both are facts of the
//! scenario (which coasts face each other), never derived from a flag. Sea edges are what
//! make islands, colonies and overseas holdings reachable — a `coastal` mark alone is not a
//! link (owner's review of PR #280).
//!
//! **Passability (E2 steps 2, 5 and 7).** A unit may enter a province its owner holds, a
//! province of a polity its owner is at war with (`army.rs`), or the land of an ally, of its
//! lord or of a vassal (`diplomacy.rs`: the right of passage comes with the bond), by land
//! or by sea. Other foreign land is closed; sea crossing needs no fleet yet — the fleet
//! becomes the condition when institutions arrive (E0 А10). With no passable path the order
//! is dropped with a `NoPath` event.
//!
//! **The march (E2 step 5, E0 А5).** A unit's reach in a tick is the day's march times the
//! campaign days of a half-year, over the width of a province (√ of the mean area) — on
//! this scale dozens of provinces, so the march is no brake on war (owner's note on PR #281);
//! sieges and supply are. A unit therefore walks its whole path across its own land in a
//! tick and **stops at contact**: the first province that belongs to an enemy or holds an
//! enemy unit ends the tick's march there, where the battle or the siege takes place.
//!
//! **Supply is pay at home, foraging abroad (E2 steps 4 and 5).** At home a unit is
//! supplied to the share of the army its polity paid this tick (`economy.rs`); the shortfall
//! is shared pro rata among the polity's units, and the unpaid share melts — an unpaid army
//! deserts or mutinies, it does not live off its own subjects (owner's rule on PR #284: a
//! foraging army at home would void the budget as the limit and А11 with it; requisitions
//! at home, if ever, come with internal politics and a price in legitimacy). On **friendly
//! land** — an ally's, the lord's, a vassal's — a unit is supplied as at home: the paid
//! share buys its food, the unpaid share melts, nobody forages on a friend (owner's note on
//! #284). On **enemy land** a unit **forages** from the province it stands in: up to
//! `forage.soldiers_per_1000_people` of its people, shared pro rata among the foragers; the
//! province loses `ruin.population_loss_per_tick_at_full_load` of its people scaled by the
//! load (E0 П6, `Source::Ruin`). Whatever is neither paid nor fed melts at
//! `army.attrition_unsupplied_per_year / 2` a tick (E0 А7) through the ledger; a stock that
//! reaches zero is a `BoundHit` like any other.
//!
//! Decisions are in canonical order (ascending `UnitId`), so the ordinal of any future draw
//! is the same whatever the caller's traversal (plan §3).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::event::Event;
use crate::ids::{ActorId, ProvinceId, UnitId};
use crate::ledger::{Account, Source};
use crate::params::Params;
use crate::world::{World, WorldError};

/// How a step was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Way {
    Land,
    Sea,
}

/// Why an order is refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OrderError {
    #[error("unknown unit {0:?}")]
    UnknownUnit(UnitId),
    #[error("unknown province {0:?}")]
    UnknownProvince(ProvinceId),
    #[error("{0:?} does not command unit {1:?}")]
    NotYours(crate::verbs::ActorRef, UnitId),
    #[error("polity {0:?} has fallen")]
    Fallen(ActorId),
}

impl World {
    /// Whether `who` commands `unit`: a ruler the polity's own units and the ruling house's
    /// retinue, a house its own retinue (E0 Х2).
    pub fn commands(&self, who: crate::verbs::ActorRef, unit: UnitId) -> bool {
        let Some(u) = self.units.get(&unit) else { return false };
        match who {
            crate::verbs::ActorRef::Ruler(a) => u.owner == a && u.house.is_none_or(|h| h == self.ruling_house(a)),
            crate::verbs::ActorRef::House(h) => u.house == Some(h),
        }
    }

    /// Whether an order can be given: the unit exists and is the caller's, the province
    /// exists, the ruler's polity stands.
    pub fn order_room(&self, o: &crate::verbs::Order) -> Result<(), OrderError> {
        if !self.units.contains_key(&o.unit) {
            return Err(OrderError::UnknownUnit(o.unit));
        }
        if !self.provinces.contains_key(&o.to) {
            return Err(OrderError::UnknownProvince(o.to));
        }
        if let crate::verbs::ActorRef::Ruler(a) = o.actor {
            if !self.alive(a) {
                return Err(OrderError::Fallen(a));
            }
        }
        if !self.commands(o.actor, o.unit) {
            return Err(OrderError::NotYours(o.actor, o.unit));
        }
        Ok(())
    }

    /// Give an order through the contract's channel.
    pub fn apply_order(&mut self, o: &crate::verbs::Order) -> Result<(), OrderError> {
        self.order_room(o)?;
        self.units.get_mut(&o.unit).unwrap().destination = Some(o.to);
        Ok(())
    }

    /// Order a unit to move to a province. The path is found when the tick is played.
    pub fn order_move(&mut self, unit: UnitId, to: ProvinceId) -> Result<(), WorldError> {
        if !self.provinces.contains_key(&to) {
            return Err(WorldError::UnknownProvince(to));
        }
        let u = self.units.get_mut(&unit).ok_or(WorldError::UnknownUnit(unit))?;
        u.destination = Some(to);
        Ok(())
    }

    /// Can `owner`'s unit enter `p`? Its own or friendly land, or the land of an enemy at war.
    pub fn passable(&self, owner: ActorId, p: ProvinceId) -> bool {
        self.provinces.get(&p).is_some_and(|pr| self.friendly(owner, pr.owner) || self.at_war(owner, pr.owner))
    }

    /// Does `p` stop `owner`'s march: enemy land, or an enemy unit standing there.
    pub fn contact(&self, owner: ActorId, p: ProvinceId) -> bool {
        self.at_war(owner, self.provinces[&p].owner) || self.units.values().any(|u| u.position == p && self.at_war(owner, u.owner))
    }

    /// Provinces a unit can cross in a tick: the reach of E0 А5 over the width of a province
    /// (√ of the mean area of the world's provinces), at least one.
    pub fn march_steps_per_tick(&self, params: &Params) -> usize {
        let n = self.provinces.len().max(1) as f64;
        let mean_area = self.provinces.values().map(|p| p.area_km2 as f64).sum::<f64>() / n;
        let width = mean_area.sqrt().max(1.0);
        ((params.march_km_per_tick as f64 / width).floor() as usize).max(1)
    }

    /// Breadth-first search over `owner`'s passable provinces from `from`; returns the first
    /// province satisfying `goal` and the steps to it.
    fn nearest(&self, owner: ActorId, from: ProvinceId, goal: impl Fn(ProvinceId) -> bool) -> Option<(ProvinceId, Vec<(ProvinceId, Way)>)> {
        let mut prev: BTreeMap<ProvinceId, (ProvinceId, Way)> = BTreeMap::new();
        let mut seen: BTreeSet<ProvinceId> = BTreeSet::new();
        let mut queue = VecDeque::new();
        seen.insert(from);
        queue.push_back(from);
        while let Some(c) = queue.pop_front() {
            let pr = &self.provinces[&c];
            let steps = pr.neighbours.iter().map(|n| (*n, Way::Land)).chain(pr.sea_neighbours.iter().map(|n| (*n, Way::Sea)));
            for (n, way) in steps {
                if seen.contains(&n) || !self.passable(owner, n) {
                    continue;
                }
                seen.insert(n);
                prev.insert(n, (c, way));
                if goal(n) {
                    let mut out = vec![(n, way)];
                    let mut at = c;
                    while at != from {
                        let (p, w) = prev[&at];
                        out.push((at, w));
                        at = p;
                    }
                    out.reverse();
                    return Some((n, out));
                }
                queue.push_back(n);
            }
        }
        None
    }

    /// The nearest province of `owner`'s own from `from`, through passable land, if any.
    pub fn nearest_own(&self, owner: ActorId, from: ProvinceId) -> Option<ProvinceId> {
        self.nearest(owner, from, |p| self.provinces[&p].owner == owner).map(|(p, _)| p)
    }

    /// Shortest passable path from `from` to `to` for `owner`, as the steps after `from`.
    pub fn path(&self, owner: ActorId, from: ProvinceId, to: ProvinceId) -> Option<Vec<(ProvinceId, Way)>> {
        if from == to {
            return Some(Vec::new());
        }
        self.nearest(owner, from, |p| p == to).map(|(_, steps)| steps)
    }

    /// The movement phase of a tick: every ordered unit marches along its path until its
    /// reach is spent, its destination is reached, or it makes contact with the enemy.
    pub(crate) fn phase_movement(&mut self, params: &Params) -> Result<(), WorldError> {
        let reach = self.march_steps_per_tick(params);
        let ids: Vec<UnitId> = self.units.keys().copied().collect();
        for id in &ids {
            let (owner, mut at, dest) = {
                let u = &self.units[id];
                (u.owner, u.position, u.destination)
            };
            let Some(dest) = dest else { continue };
            if self.contact(owner, at) {
                // Engaged where it stands: the battle or the siege comes first.
                continue;
            }
            match self.path(owner, at, dest) {
                None => {
                    self.units.get_mut(id).unwrap().destination = None;
                    self.emit(Event::NoPath { unit: *id, to: dest });
                }
                Some(steps) => {
                    for (to, way) in steps.into_iter().take(reach) {
                        self.emit(Event::UnitMoved { unit: *id, from: at, to, way });
                        at = to;
                        if self.contact(owner, at) {
                            break;
                        }
                    }
                    let u = self.units.get_mut(id).unwrap();
                    u.position = at;
                    if at == dest {
                        u.destination = None;
                    }
                }
            }
        }
        Ok(())
    }

    /// The supply phase of a tick, after the economy paid: at home a unit keeps the paid
    /// share of its men and the rest melts by А7; on foreign land a unit forages from the
    /// province — up to what it can feed, at the cost of its people (E0 П6) — and what
    /// foraging cannot feed melts by А7. Provinces in canonical order, units within them
    /// likewise.
    pub(crate) fn phase_supply(&mut self, params: &Params) -> Result<(), WorldError> {
        let provinces: Vec<ProvinceId> = self.provinces.keys().copied().collect();
        for p in provinces {
            let owner = self.provinces[&p].owner;
            let here: Vec<(UnitId, ActorId, i64)> = self.units.iter().filter(|(_, u)| u.position == p).map(|(id, u)| (*id, u.owner, self.ledger.stock(Account::Soldiers(*id)).unwrap_or(0))).collect();
            // At home or on friendly land: the paid share is fed, the unpaid share melts —
            // the polity's share for its own men, the house's for a house's retinue (Х2). A
            // province in revolt feeds its owner's men as foreign land does (E2 step 9):
            // they forage there, and ruin it.
            let in_revolt = self.in_revolt(p);
            let here: Vec<(UnitId, bool, i64)> = here.into_iter().map(|(id, o, s)| (id, self.friendly(o, owner) && !(in_revolt && o == owner), s)).collect();
            let fed: Vec<(UnitId, i64)> = here.iter().filter(|(_, home, _)| *home).map(|(id, _, s)| (*id, s - s * self.unit_paid_share_ppm(*id) / 1_000_000)).collect();
            for (id, unsupplied) in fed {
                self.melt(id, unsupplied, params)?;
            }
            // On enemy land: foraging, shared pro rata by need.
            let needs: Vec<(UnitId, i64)> = here.iter().filter(|(_, home, s)| !*home && *s > 0).map(|(id, _, s)| (*id, *s)).collect();
            let total_need: i64 = needs.iter().map(|(_, n)| n).sum();
            if total_need == 0 {
                continue;
            }
            let people = self.population(p);
            let capacity = people * params.forage_soldiers_per_1000 / 1000;
            let fed_total = total_need.min(capacity);
            for (id, need) in &needs {
                self.melt(*id, need - need * fed_total / total_need, params)?;
            }
            if fed_total > 0 && capacity > 0 {
                // People lost in thousandths: people × rate at full load × load.
                let milli = -(people * params.ruin_loss_ppm_at_full_load / 1000 * fed_total / capacity);
                if milli != 0 {
                    self.post(Account::Population(p), Source::Ruin, milli)?;
                }
                self.emit(Event::Ruin { province: p, foragers: fed_total as u64 });
            }
        }
        Ok(())
    }

    /// Unsupplied soldiers of a unit melt by А7: thousandths lost = unsupplied × rate per tick.
    fn melt(&mut self, id: UnitId, unsupplied: i64, params: &Params) -> Result<(), WorldError> {
        if unsupplied <= 0 {
            return Ok(());
        }
        let milli = -(unsupplied * params.army_attrition_unsupplied_ppm_per_tick / 1000);
        if milli != 0 {
            self.post(Account::Soldiers(id), Source::Desertion, milli)?;
            self.emit(Event::Attrition { unit: id, unsupplied: unsupplied as u64 });
        }
        Ok(())
    }
}
