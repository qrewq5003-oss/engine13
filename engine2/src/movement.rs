//! Units on the province graph: movement and supply (brief §2a, plan §14, §16).
//!
//! **Edges.** A province has land neighbours and sea neighbours; both are facts of the
//! scenario (which coasts face each other), never derived from a flag. Sea edges are what
//! make islands, colonies and overseas holdings reachable — a `coastal` mark alone is not a
//! link (owner's review of PR #280).
//!
//! **Passability (E2 step 2).** A unit may enter a province its owner holds, by land or by
//! sea. Foreign land is closed until diplomacy grants passage or war opens it (later steps);
//! sea crossing needs no fleet yet — the fleet becomes the condition when institutions
//! arrive (E0 А10). A unit moves `movement.provinces_per_tick` edges a tick along the
//! shortest passable path; with no path the order is dropped with a `NoPath` event.
//!
//! **Supply is pay (E2 step 4, replacing the step 2 stand-in).** At home a unit is supplied
//! to the share of the army its polity paid this tick (`economy.rs`): the budget is the
//! limit, and the shortfall is shared pro rata among the polity's units — not "lower ids eat
//! first". An enclave or a colony is as supplied as the rest of the army: pay reaches it
//! without a land link. Any unit on foreign land is unsupplied until the army system brings
//! foraging with its cost (E0 П6). Unsupplied soldiers melt at
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

impl World {
    /// Order a unit to move to a province. The path is found when the tick is played.
    pub fn order_move(&mut self, unit: UnitId, to: ProvinceId) -> Result<(), WorldError> {
        if !self.provinces.contains_key(&to) {
            return Err(WorldError::UnknownProvince(to));
        }
        let u = self.units.get_mut(&unit).ok_or(WorldError::UnknownUnit(unit))?;
        u.destination = Some(to);
        Ok(())
    }

    /// Can `owner`'s unit enter `p`? Own land only, for now (see the module doc).
    pub fn passable(&self, owner: ActorId, p: ProvinceId) -> bool {
        self.provinces.get(&p).is_some_and(|pr| pr.owner == owner)
    }

    /// Shortest passable path from `from` to `to` for `owner`, as the steps after `from`.
    pub fn path(&self, owner: ActorId, from: ProvinceId, to: ProvinceId) -> Option<Vec<(ProvinceId, Way)>> {
        if from == to {
            return Some(Vec::new());
        }
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
                if n == to {
                    let mut out = vec![(n, way)];
                    let mut at = c;
                    while at != from {
                        let (p, w) = prev[&at];
                        out.push((at, w));
                        at = p;
                    }
                    out.reverse();
                    return Some(out);
                }
                queue.push_back(n);
            }
        }
        None
    }

    /// The movement phase of a tick: every ordered unit takes its steps.
    pub(crate) fn phase_movement(&mut self, params: &Params) -> Result<(), WorldError> {
        let ids: Vec<UnitId> = self.units.keys().copied().collect();
        for id in &ids {
            let (owner, mut at, dest) = {
                let u = &self.units[id];
                (u.owner, u.position, u.destination)
            };
            let Some(dest) = dest else { continue };
            match self.path(owner, at, dest) {
                None => {
                    self.units.get_mut(id).unwrap().destination = None;
                    self.emit(Event::NoPath { unit: *id, to: dest });
                }
                Some(steps) => {
                    for (to, way) in steps.into_iter().take(params.movement_provinces_per_tick as usize) {
                        self.emit(Event::UnitMoved { unit: *id, from: at, to, way });
                        at = to;
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
    /// share of its men supplied; on foreign land none. The rest melts by А7.
    pub(crate) fn phase_supply(&mut self, params: &Params) -> Result<(), WorldError> {
        let ids: Vec<UnitId> = self.units.keys().copied().collect();
        for id in &ids {
            let (owner, at) = {
                let u = &self.units[id];
                (u.owner, u.position)
            };
            let soldiers = self.ledger.stock(Account::Soldiers(*id)).unwrap_or(0);
            let supplied = if self.provinces[&at].owner == owner { soldiers * self.actors[&owner].paid_share_ppm / 1_000_000 } else { 0 };
            let unsupplied = soldiers - supplied;
            if unsupplied > 0 {
                // Thousandths of soldiers lost: unsupplied × rate per tick.
                let milli = -(unsupplied * params.army_attrition_unsupplied_ppm_per_tick / 1000);
                if milli != 0 {
                    self.post(Account::Soldiers(*id), Source::Desertion, milli)?;
                    self.emit(Event::Attrition { unit: *id, unsupplied: unsupplied as u64 });
                }
            }
        }
        Ok(())
    }
}
