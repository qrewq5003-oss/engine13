//! Agents (brief §3 «Агенты: решают все, движок исполняет», plan §23; E3 step (а)): goals
//! as a state of the actor, utility without learning, the choice by softmax through the
//! world's keyed randomness, every bot deciding on the snapshot of the tick's start before
//! anything is applied, and the player going through the same offers.
//!
//! **Goals** (`Actor::goals`). A goal is a kind of the engine's closed list (E0 §6: hold,
//! take, subdue, free, unite, alliance against, union; «survive» is implicit — every polity
//! at war with a stronger enemy puts it first) with a target in the facts of the world: a
//! province, a polity, a region (a named set of provinces, a fact of the scenario — a
//! kingdom or an empire, never a people: owner's rule on §23), a faith, a route of two
//! provinces. Its **progress** is a function of the state alone ([`World::goal_progress`]):
//! what is to be held is held and not in revolt or under siege; what is to be taken is
//! owned; what is to be subdued is a vassal; a region is united when every province of it
//! is owned. A goal reached is `GoalReached` and goes on as «hold» where there is something
//! to hold; a goal that made no progress for `goal.horizon_years` is put aside
//! (`GoalDropped`) and the next reachable one is taken by the rules — free when a vassal,
//! else hold the capital — never by the lot. Starting aspirations are facts of the scenario
//! (`aspirations`); the generator draws them by a weighted lot from reachable targets
//! (owner's answer 3: the peaceful kinds heavier than the greedy, checked through А12).
//!
//! **Utility** ([`World::utility`]): for every offer the ruler has — the same offers the
//! player sees (`sim.rs`), with the same rules of availability — the progress the verb
//! would move over the polity's goals, the survival it buys at war with a stronger enemy,
//! less its price: a point of legitimacy per hundred at `agent.weight_price_legitimacy`, a
//! kilogram per year of revenue at `agent.weight_price_treasury`; a lasting decision — the
//! tax — as a flow over the goal's horizon ([`World::tax_utility`], owner's rule on #291).
//! No learning, no
//! look-ahead: an estimate off the state (a war on the owner of what is wanted, peace when
//! it is taken or the war is lost, a levy at war, a loan when the pay fell short, an
//! embassy to whom an alliance is wanted). **The choice is softmax** at `agent.temperature`
//! through one keyed draw at `DrawPoint::Decision` per deciding actor and tick; «do
//! nothing» is always in the set at a utility of nothing; a verb enters once, with its best
//! target, and only when it is better than doing nothing ([`World::choices`], plan §23b).
//!
//! **Order** (brief §3): [`World::decide_all`] scores every bot on the state as it stands,
//! then the calls are applied in canonical order of actors (`sim.rs`), the player's first.
//! A bot never calls what is not on offer, and its calls go through the same
//! `apply_verb` as the player's: the parity of B42 is a test, not a promise.
//!
//! Orders to units, the suppression of revolts, the opportunities a house answers
//! (secession, usurpation, union, votes), the threat in the relation and the quality of
//! units are the later steps of E3 (plan §23, owner's cut on #291).

use serde::{Deserialize, Serialize};

use crate::event::Event;
use crate::ids::{ActorId, ProvinceId, RegionId, ReligionId};
use crate::params::Params;
use crate::rng::{unit, Draw, DrawPoint};
use crate::time::Tick;
use crate::verbs::{ActorRef, Order, Target, VerbCall, VerbId};
use crate::world::{World, WorldError};

/// The kinds of a goal: the engine's closed list (E0 §6). «Survive» is implicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GoalKind {
    /// Keep what is held: a province, a route, a region, a vassal.
    Hold,
    /// Take a province, a route, a region, or all the land of a polity.
    Take,
    /// Make a polity a vassal.
    Subdue,
    /// Leave vassalage.
    Free,
    /// Own every province of a region.
    Unite,
    /// An alliance with someone against a polity.
    AllianceAgainst,
    /// The court's faith to another (the decision comes with the opportunities channel).
    Union,
}

/// The target of a goal: a fact of the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GoalTarget {
    None,
    Province(ProvinceId),
    Polity(ActorId),
    Region(RegionId),
    Religion(ReligionId),
    Route(ProvinceId, ProvinceId),
}

/// A goal as the state of an actor: what, of what, since when, and the best progress seen
/// (the horizon is measured against it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub kind: GoalKind,
    pub target: GoalTarget,
    pub since: Tick,
    /// Provinces the target polity held when the goal was set: the baseline of «take a
    /// polity»; zero otherwise.
    pub baseline: u32,
    /// The best progress reached since the goal was set or last moved, parts per million.
    pub best_ppm: i64,
}

/// A starting aspiration of the facts: the goal without its clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GoalFact {
    pub kind: GoalKind,
    pub target: GoalTarget,
}

/// What a bot reads once per decision rather than once per target (E3 (б), measured: the
/// choice of a world's bots took four times the rest of the tick, four fifths of it the
/// marriages — every pair of houses scanned every person of the world).
#[derive(Debug, Clone)]
pub struct Context {
    enemy: Option<ActorId>,
    at_war: bool,
    income: f64,
    unpaid: bool,
    lawful: f64,
    wanted_allies: Vec<ActorId>,
}

/// One scored option of a bot: the call and its utility; `None` is doing nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Option_ {
    pub call: Option<VerbCall>,
    pub utility: f64,
}

/// A progress as parts per million.
fn ppm(progress: f64) -> i64 {
    (progress * 1_000_000.0).round() as i64
}

impl World {
    /// Set a goal for a polity, with its baseline and its event.
    pub fn set_goal(&mut self, a: ActorId, kind: GoalKind, target: GoalTarget) -> Result<(), WorldError> {
        if !self.actors.contains_key(&a) {
            return Err(WorldError::UnknownActor(a));
        }
        match target {
            GoalTarget::Province(p) | GoalTarget::Route(p, _) if !self.provinces.contains_key(&p) => return Err(WorldError::UnknownProvince(p)),
            GoalTarget::Route(_, q) if !self.provinces.contains_key(&q) => return Err(WorldError::UnknownProvince(q)),
            GoalTarget::Polity(b) if !self.actors.contains_key(&b) => return Err(WorldError::UnknownActor(b)),
            GoalTarget::Region(r) if r.index() >= self.regions.len() => return Err(WorldError::UnknownRegion(r)),
            GoalTarget::Religion(f) if f.index() >= self.names.religions.len() => return Err(WorldError::UnknownReligion(f)),
            _ => {}
        }
        let baseline = match target {
            GoalTarget::Polity(b) => self.actors[&b].provinces.len() as u32,
            _ => 0,
        };
        let mut goal = Goal { kind, target, since: self.tick, baseline, best_ppm: 0 };
        goal.best_ppm = ppm(self.goal_progress(a, &goal));
        self.actors.get_mut(&a).unwrap().goals.push(goal);
        self.emit(Event::GoalSet { actor: a, goal });
        Ok(())
    }

    /// A province is securely held: owned, not in revolt, not under siege.
    fn held(&self, a: ActorId, p: ProvinceId) -> bool {
        let pr = &self.provinces[&p];
        pr.owner == a && pr.revolt.is_none() && pr.siege.is_none()
    }

    /// The share of a region's provinces a polity holds.
    fn region_share(&self, a: ActorId, r: RegionId) -> f64 {
        let ps = &self.regions[r.index()];
        if ps.is_empty() {
            return 1.0;
        }
        ps.iter().filter(|p| self.provinces[p].owner == a).count() as f64 / ps.len() as f64
    }

    /// Whether `b` is friendly land for `a`'s route: its own, an ally's, its lord's, a vassal's.
    fn route_end(&self, a: ActorId, p: ProvinceId) -> f64 {
        let o = self.provinces[&p].owner;
        if o == a {
            if self.held(a, p) {
                1.0
            } else {
                0.5
            }
        } else if self.friendly(a, o) {
            0.5
        } else {
            0.0
        }
    }

    /// The progress of a goal, 0–1, read off the state (plan §23.2 p. 1).
    pub fn goal_progress(&self, a: ActorId, g: &Goal) -> f64 {
        let at_war_with = |b: ActorId| self.at_war(a, b);
        match (g.kind, g.target) {
            (GoalKind::Hold, GoalTarget::Province(p)) => f64::from(u8::from(self.held(a, p))),
            (GoalKind::Hold, GoalTarget::Route(p, q)) => (self.route_end(a, p) + self.route_end(a, q)) / 2.0,
            (GoalKind::Hold, GoalTarget::Region(r)) => self.region_share(a, r),
            (GoalKind::Hold, GoalTarget::Polity(b)) => f64::from(u8::from(self.lord_of(b) == Some(a))),
            (GoalKind::Take, GoalTarget::Province(p)) => {
                if self.provinces[&p].owner == a {
                    1.0
                } else if at_war_with(self.provinces[&p].owner) {
                    0.3
                } else {
                    0.0
                }
            }
            (GoalKind::Take, GoalTarget::Route(p, q)) => (self.route_end(a, p) + self.route_end(a, q)) / 2.0,
            (GoalKind::Take, GoalTarget::Region(r)) => self.region_share(a, r),
            (GoalKind::Take, GoalTarget::Polity(b)) => {
                if g.baseline == 0 {
                    return 1.0;
                }
                let left = self.actors[&b].provinces.len() as f64;
                (1.0 - left / f64::from(g.baseline)).clamp(0.0, 1.0)
            }
            (GoalKind::Subdue, GoalTarget::Polity(b)) => {
                if self.lord_of(b) == Some(a) {
                    1.0
                } else if at_war_with(b) {
                    0.3
                } else {
                    0.0
                }
            }
            (GoalKind::Free, _) => match self.lord_of(a) {
                None => 1.0,
                Some(lord) if at_war_with(lord) => 0.5,
                Some(_) => 0.0,
            },
            (GoalKind::Unite, GoalTarget::Region(r)) => self.region_share(a, r),
            (GoalKind::AllianceAgainst, GoalTarget::Polity(b)) => {
                let allies = self.allies_of(a);
                if allies.iter().any(|x| self.at_war(*x, b) || self.borders(*x, b)) {
                    1.0
                } else if !allies.is_empty() {
                    0.5
                } else {
                    0.0
                }
            }
            (GoalKind::Union, GoalTarget::Religion(f)) => f64::from(u8::from(self.polity_religion(a) == f)),
            _ => 0.0,
        }
    }

    /// The reachable target of a kind for `a`, as the generator draws them: hold its most
    /// populous province, an alliance against its most populous neighbour, take the most
    /// populous province on its border, subdue its least populous neighbour, unite its
    /// capital's region (ties: the lower id). None when the kind has nothing to aim at or
    /// `a` holds that goal already.
    fn reachable(&self, a: ActorId, kind: GoalKind) -> Option<GoalTarget> {
        let mine = &self.actors[&a].provinces;
        let mut border: std::collections::BTreeMap<ActorId, i64> = std::collections::BTreeMap::new();
        let mut across: Vec<ProvinceId> = Vec::new();
        for p in mine {
            let pr = &self.provinces[p];
            for n in pr.neighbours.iter().chain(pr.sea_neighbours.iter()) {
                let o = self.provinces[n].owner;
                if o != a && self.alive(o) {
                    *border.entry(o).or_insert(0) += self.population(*n);
                    across.push(*n);
                }
            }
        }
        across.sort();
        across.dedup();
        let target = match kind {
            GoalKind::Hold => mine.iter().copied().max_by_key(|p| (self.population(*p), std::cmp::Reverse(*p))).map(GoalTarget::Province),
            GoalKind::AllianceAgainst => border.iter().max_by_key(|(b, n)| (**n, std::cmp::Reverse(**b))).map(|(b, _)| GoalTarget::Polity(*b)),
            GoalKind::Take => across.iter().copied().max_by_key(|p| (self.population(*p), std::cmp::Reverse(*p))).map(GoalTarget::Province),
            GoalKind::Subdue => border.iter().filter(|(b, _)| self.lord_of(**b) != Some(a)).min_by_key(|(b, n)| (**n, **b)).map(|(b, _)| GoalTarget::Polity(*b)),
            GoalKind::Unite => {
                let capital = self.actors[&a].capital;
                self.regions.iter().position(|r| r.contains(&capital) && r.iter().any(|p| self.provinces[p].owner != a)).map(|i| GoalTarget::Region(RegionId(i as u32)))
            }
            // The faith of the most populous neighbour whose court prays otherwise (owner's
            // rule on #293).
            GoalKind::Union => {
                let court = self.polity_religion(a);
                border.iter().filter(|(b, _)| self.polity_religion(**b) != court).max_by_key(|(b, n)| (**n, std::cmp::Reverse(**b))).map(|(b, _)| GoalTarget::Religion(self.polity_religion(*b)))
            }
            _ => None,
        }?;
        if self.actors[&a].goals.iter().any(|g| g.kind == kind && g.target == target) {
            return None;
        }
        Some(target)
    }

    /// A new goal by the weighted lot of reachable kinds (owner's answer 3 on §23: the
    /// peaceful kinds heavier than the greedy), by one keyed draw; hold the capital when
    /// nothing is reachable.
    fn draw_goal(&mut self, a: ActorId, params: &Params) -> (GoalKind, GoalTarget) {
        let options: Vec<(GoalKind, GoalTarget, f64)> = params.aspiration_weights.iter().filter_map(|(k, w)| self.reachable(a, *k).map(|t| (*k, t, *w))).collect();
        let u = unit(self.draw(DrawPoint::Goal, a.0));
        let total: f64 = options.iter().map(|(_, _, w)| w).sum();
        let mut x = u * total;
        for (k, t, w) in &options {
            if x < *w {
                return (*k, *t);
            }
            x -= w;
        }
        options.last().map(|(k, t, _)| (*k, *t)).unwrap_or((GoalKind::Hold, GoalTarget::Province(self.actors[&a].capital)))
    }

    /// The phase of goals, at the end of the tick: progress read; goals reached go on as
    /// «hold»; a goal is put aside when the horizon passes without new progress (an
    /// ambition that went nowhere, a «hold» kept for the horizon); the freed slots are
    /// filled — revanche for a province conquered from the polity this tick, the wish of a
    /// vassal to be free, else the weighted lot (E3 (б): renewed only as «hold the
    /// capital», the world fell asleep — the goals of war of 0.42 of the polities fell to
    /// 0.13 in a century and a half).
    pub(crate) fn phase_goals(&mut self, params: &Params) -> Result<(), WorldError> {
        let conquered: Vec<(ProvinceId, ActorId)> = self
            .events_of_tick()
            .iter()
            .filter_map(|e| match e.event {
                Event::ProvinceChangedHands { province, from, how: crate::event::Transfer::Conquest, .. } => Some((province, from)),
                _ => None,
            })
            .collect();
        let ids: Vec<ActorId> = self.actors.keys().copied().filter(|a| self.alive(*a)).collect();
        for a in ids {
            let goals = self.actors[&a].goals.clone();
            let mut kept: Vec<Goal> = Vec::with_capacity(goals.len());
            for mut g in goals {
                let progress = self.goal_progress(a, &g);
                if ppm(progress) > g.best_ppm {
                    g.best_ppm = ppm(progress);
                    g.since = self.tick;
                }
                if progress >= 1.0 - 1e-9 && matches!(g.kind, GoalKind::Take | GoalKind::Subdue | GoalKind::Unite | GoalKind::Free | GoalKind::Union) {
                    self.emit(Event::GoalReached { actor: a, goal: g });
                    match (g.kind, g.target) {
                        (GoalKind::Take, t @ (GoalTarget::Province(_) | GoalTarget::Route(..) | GoalTarget::Region(_))) | (GoalKind::Unite, t) => {
                            kept.push(Goal { kind: GoalKind::Hold, target: t, since: self.tick, baseline: 0, best_ppm: 1_000_000 });
                        }
                        (GoalKind::Subdue, t) => kept.push(Goal { kind: GoalKind::Hold, target: t, since: self.tick, baseline: 0, best_ppm: 1_000_000 }),
                        _ => {}
                    }
                    continue;
                }
                if self.tick.0.saturating_sub(g.since.0) >= params.goal_horizon_ticks {
                    self.emit(Event::GoalDropped { actor: a, goal: g });
                    continue;
                }
                kept.push(g);
            }
            self.actors.get_mut(&a).unwrap().goals = kept;
            // Revanche: a province conquered from the polity this tick, still the
            // conqueror's — by the rule, first into the free slots (plan §23d). Beyond the
            // slots every lost province was a goal of its own against the same enemy, and a
            // loser that lost three wanted the war three times over: no price of a fresh
            // treaty held it back (owner's rule on #293, plan §23l).
            for p in conquered.iter().filter(|(_, from)| *from == a).map(|(p, _)| *p) {
                let target = GoalTarget::Province(p);
                if self.actors[&a].goals.len() < params.goal_open_per_polity && self.provinces[&p].owner != a && !self.actors[&a].goals.iter().any(|g| g.kind == GoalKind::Take && g.target == target) {
                    self.set_goal(a, GoalKind::Take, target)?;
                }
            }
            // A vassal wants to be free — by the rule.
            if self.lord_of(a).is_some() && !self.actors[&a].goals.iter().any(|g| g.kind == GoalKind::Free) {
                self.set_goal(a, GoalKind::Free, GoalTarget::None)?;
            }
            // The free slots by the weighted lot, one draw a tick.
            if self.actors[&a].goals.len() < params.goal_open_per_polity {
                let (kind, target) = self.draw_goal(a, params);
                if !self.actors[&a].goals.iter().any(|g| g.kind == kind && g.target == target) {
                    self.set_goal(a, kind, target)?;
                }
            }
        }
        Ok(())
    }

    // ---- utility ----

    /// The strongest enemy of `a` at war with it, by soldiers, and whether it outnumbers `a`.
    fn outmatched(&self, a: ActorId) -> Option<ActorId> {
        let mine = self.army_size(a);
        self.actors
            .keys()
            .copied()
            .filter(|b| self.at_war(a, *b) && self.alive(*b))
            .filter(|b| self.army_size(*b) > mine)
            .max_by_key(|b| self.army_size(*b))
    }

    /// The polities `a` wants an alliance with, by its goals: those at war with or bordering
    /// the polity an alliance is wanted against, and — at war with a stronger enemy — those
    /// at war with that enemy.
    fn wanted_allies(&self, a: ActorId) -> Vec<ActorId> {
        let mut out = Vec::new();
        for g in &self.actors[&a].goals {
            if let (GoalKind::AllianceAgainst, GoalTarget::Polity(x)) = (g.kind, g.target) {
                out.extend(self.actors.keys().copied().filter(|b| *b != a && *b != x && self.alive(*b) && (self.at_war(*b, x) || self.borders(*b, x))));
            }
        }
        if let Some(enemy) = self.outmatched(a) {
            out.extend(self.actors.keys().copied().filter(|b| *b != a && self.alive(*b) && self.at_war(*b, enemy)));
        }
        out.sort();
        out.dedup();
        out
    }

    /// The progress a war on `b` would move over `a`'s goals.
    fn war_wanted(&self, a: ActorId, b: ActorId) -> f64 {
        let mut want = 0.0f64;
        for g in &self.actors[&a].goals {
            let p = self.goal_progress(a, g);
            if p >= 1.0 - 1e-9 {
                continue;
            }
            want += match (g.kind, g.target) {
                (GoalKind::Take | GoalKind::Hold, GoalTarget::Province(q)) if self.provinces[&q].owner == b => 1.0,
                (GoalKind::Take, GoalTarget::Route(q, r)) if self.provinces[&q].owner == b || self.provinces[&r].owner == b => 0.5,
                (GoalKind::Take | GoalKind::Unite | GoalKind::Hold, GoalTarget::Region(r)) => {
                    let ps = &self.regions[r.index()];
                    ps.iter().filter(|q| self.provinces[q].owner == b).count() as f64 / ps.len().max(1) as f64
                }
                (GoalKind::Take | GoalKind::Subdue, GoalTarget::Polity(x)) if x == b => 1.0,
                (GoalKind::Free, _) if self.lord_of(a) == Some(b) => 1.0,
                _ => 0.0,
            };
        }
        want
    }

    /// Whether `a` has a live goal a war on `b` would serve: the war goal the declarer's
    /// acceptance of a peace reads (`diplomacy::peace_room`, owner's rule on #291).
    pub fn war_goal_against(&self, a: ActorId, b: ActorId) -> bool {
        self.war_wanted(a, b) > 0.0
    }

    /// The province a war on `b` aims at for `a`: the province of `b` a goal of `a` wants
    /// (the lowest id among them), else `b`'s capital — the march takes what lies on the
    /// way and stops at contact.
    fn war_target(&self, a: ActorId, b: ActorId) -> ProvinceId {
        let wanted = self.actors[&a].goals.iter().filter_map(|g| match (g.kind, g.target) {
            (GoalKind::Take | GoalKind::Hold, GoalTarget::Province(q)) if self.provinces[&q].owner == b => Some(q),
            _ => None,
        });
        wanted.min().unwrap_or(self.actors[&b].capital)
    }

    /// The orders a bot gives its units (E3 (б), plan §23.2 p. 4; the same channel as the
    /// player's): no draws, by the state alone. **At peace, revolts first** — for every
    /// province in revolt the nearest unit by path goes there and stands (owner's rule on
    /// #289: the suppression is an ordinary decision of the bot); at war a revolt waits (a
    /// stub, owner on #292: to be chosen by utility, the front against the suppression);
    /// **at war** — the enemy whose land a
    /// goal wants most (ties: the lower id), its wanted province or its capital, when the
    /// units are at least `agent.advance_ratio` of the enemy's whole army, else home to the
    /// capital to defend it; **at peace** — home. The ruler commands its own units and the
    /// ruling house's retinue; the other houses' men wait for their houses (step (в)).
    pub fn bot_orders(&self, a: ActorId, params: &Params) -> Vec<Order> {
        let who = ActorRef::Ruler(a);
        if !self.alive(a) {
            return Vec::new();
        }
        let mut units: Vec<crate::ids::UnitId> = self.units.keys().copied().filter(|u| self.commands(who, *u)).collect();
        let capital = self.actors[&a].capital;
        let mut out = Vec::new();
        let give = |u: crate::ids::UnitId, to: ProvinceId, out: &mut Vec<Order>| {
            let unit = &self.units[&u];
            if unit.position != to && unit.destination != Some(to) && self.path(a, unit.position, to).is_some() {
                out.push(Order { actor: who, unit: u, to });
            }
        };
        // STUB (owner on #292): at war the army goes to the war and a revolt waits for the
        // peace — a rule of behaviour written by hand, like the tax anchored to the tithe
        // that #291 removed, not a utility. To be replaced by the choice «the front or the
        // suppression» by utility: the worth of the war goal against the loss to the revolt.
        // Measured: with the revolts put down at once, mid-war, they lasted a year and
        // recurred, and no base rate of the rising kept both В7 and В9 in their bands.
        let at_war = self.actors.keys().any(|b| self.at_war(a, *b) && self.alive(*b));
        let revolts: Vec<ProvinceId> = if at_war { Vec::new() } else { self.actors[&a].provinces.iter().copied().filter(|p| self.in_revolt(*p)).collect() };
        for p in revolts {
            if let Some(on_duty) = units.iter().copied().find(|u| self.units[u].position == p || self.units[u].destination == Some(p)) {
                units.retain(|u| *u != on_duty);
                continue;
            }
            let nearest = units.iter().copied().filter_map(|u| self.path(a, self.units[&u].position, p).map(|path| (path.len(), u))).min();
            if let Some((_, u)) = nearest {
                give(u, p, &mut out);
                units.retain(|x| *x != u);
            }
        }
        let enemies: Vec<ActorId> = self.actors.keys().copied().filter(|b| self.at_war(a, *b) && self.alive(*b)).collect();
        let to = match enemies.iter().copied().max_by_key(|b| ((self.war_wanted(a, *b) * 1e6) as i64, std::cmp::Reverse(*b))) {
            Some(enemy) => {
                let mine: i64 = units.iter().map(|u| self.soldiers(*u)).sum();
                if mine > 0 && mine as f64 >= params.agent_advance_ratio * self.army_size(enemy) as f64 {
                    self.war_target(a, enemy)
                } else {
                    capital
                }
            }
            None => capital,
        };
        for u in units {
            give(u, to, &mut out);
        }
        out
    }

    /// The orders of every bot, on the state as it stands, the player's actor excepted.
    pub fn bot_orders_all(&self, player: Option<ActorRef>, params: &Params) -> Vec<Order> {
        self.actors.keys().copied().filter(|a| self.alive(*a) && player != Some(ActorRef::Ruler(*a))).flat_map(|a| self.bot_orders(a, params)).collect()
    }

    /// The army's paid share as the bot reads it: the mean over the last
    /// `agent.pay_memory_ticks` ticks (owner's rule on #292 — an input of the bot, not a
    /// rule of the world: the peace still reads the pay of the tick, `diplomacy.rs`).
    pub fn paid_share_seen_ppm(&self, a: ActorId) -> i64 {
        let actor = &self.actors[&a];
        let h = &actor.paid_share_history_ppm;
        if h.is_empty() {
            actor.paid_share_ppm
        } else {
            h.iter().sum::<i64>() / h.len() as i64
        }
    }

    /// Whether `b`'s army is too strong for `a` to march against: the bot's units advance
    /// only at `agent.advance_ratio` of the enemy's whole army (`bot_orders`), and the war
    /// and the peace weigh the enemy's strength by the same measure.
    fn outweighed(&self, a: ActorId, b: ActorId, params: &Params) -> bool {
        (self.army_size(a) as f64) < params.agent_advance_ratio * self.army_size(b) as f64
    }

    /// The bots' answers to the open opportunities (E3 (в)): every opportunity the player
    /// does not answer, by its decider's utility, on the state as it stands.
    pub fn bot_answers(&self, player: Option<ActorRef>, params: &Params) -> Vec<crate::opportunity::Answer> {
        use crate::opportunity::Answer;
        self.opportunities
            .iter()
            .filter(|o| !player.is_some_and(|p| self.answers_for(p, o)))
            .map(|o| Answer { who: o.to, opportunity: o.id, reply: self.bot_reply(o, params) })
            .collect()
    }

    /// A bot's reply to an opportunity: a vote for its own house's candidate (each house for
    /// itself — the owner's note on §23: the bargains are E4's), else for the interim; any
    /// other chance taken when its utility is above doing nothing.
    pub fn bot_reply(&self, o: &crate::opportunity::Opportunity, params: &Params) -> crate::opportunity::Reply {
        use crate::opportunity::{Chance, Reply};
        if let (ActorRef::House(h), Chance::Vote { candidates, interim, .. }) = (o.to, &o.what) {
            return Reply::Vote(candidates.iter().copied().find(|c| self.persons[c].house == h).unwrap_or(*interim));
        }
        if self.bot_takes(o, params) {
            Reply::Take
        } else {
            Reply::Decline
        }
    }

    /// Whether a bot takes an opportunity: its utility above doing nothing.
    pub fn bot_takes(&self, o: &crate::opportunity::Opportunity, params: &Params) -> bool {
        use crate::opportunity::Chance;
        match (o.to, &o.what) {
            (ActorRef::House(h), Chance::Secession { polity, province }) => self.house_would_secede(h, *polity, *province, params),
            (_, Chance::Usurpation { polity, candidate, .. }) => self.house_would_usurp(*polity, *candidate, params),
            (_, Chance::Union { polity, faith }) => self.union_utility(*polity, *faith) > 0.0,
            (_, Chance::Policy { polity, .. }) => self.policy_utility(*polity, params) > 0.0,
            (ActorRef::House(g), Chance::Marriage { from, .. }) => self.marriage_utility(g, *from, params) > 0.0,
            _ => false,
        }
    }

    /// The utility of a match offered to house `g` by house `from`: the bond — with its own
    /// polity's ruling house or with a polity its own wants as an ally, as the proposer
    /// weighs it (0.4, else −0.2) — and the dowry, at the bot's price of wheat in shares of
    /// its polity's revenue.
    pub fn marriage_utility(&self, g: crate::ids::HouseId, from: crate::ids::HouseId, params: &Params) -> f64 {
        let (mine, theirs) = (self.houses[&g].polity, self.houses[&from].polity);
        let bond = if mine == theirs || self.wanted_allies(mine).contains(&theirs) { 0.4 } else { -0.2 };
        let income = self.yearly_income(mine, params).max(1) as f64;
        bond + self.dowry(from, params) as f64 / income * params.agent_weight_price_treasury
    }

    /// The utility of the union for a court (owner's answer 6 on §23): its goal reached, 1,
    /// and the change in the people praying with the court — the share of the new faith
    /// against the share of the old, at twice the goal (the alienation of the old faith is
    /// the price, owner's rule on §20).
    pub fn union_utility(&self, polity: ActorId, faith: crate::ids::ReligionId) -> f64 {
        let court = self.polity_religion(polity);
        1.0 + 2.0 * (self.people_share_praying(polity, faith) - self.people_share_praying(polity, court))
    }

    /// The utility of the policy of conversion for a court: the share of its people praying
    /// otherwise than the court, whom the policy turns, at twice the goal, against its price
    /// in legitimacy at the bot's price.
    pub fn policy_utility(&self, polity: ActorId, params: &Params) -> f64 {
        let other = 1.0 - self.people_share_praying(polity, self.polity_religion(polity));
        2.0 * other - params.policy_cost_legitimacy / 100.0 * params.agent_weight_price_legitimacy
    }

    /// The utility of a secession for the house that would carry it (plan §23.2 p. 5): the
    /// weakness of the ruler it leaves — `1 − L/100` — against the odds of being crushed,
    /// the parent's army over both armies: the militia the provinces would levy at the
    /// ceiling of А1 and the house's own men, which go with it.
    pub fn secession_utility(&self, house: crate::ids::HouseId, polity: ActorId, province: ProvinceId, params: &Params) -> f64 {
        let capital = self.actors[&polity].capital;
        let provinces: Vec<ProvinceId> = self.actors[&polity].provinces.iter().copied().filter(|q| *q != capital && self.provinces[q].governor.is_some_and(|g| self.persons[&g].house == house)).collect();
        let retinue: i64 = self.units_of_house(house).iter().map(|u| self.soldiers(*u)).sum();
        let own: i64 = provinces.iter().chain(std::iter::once(&province)).collect::<std::collections::BTreeSet<_>>().into_iter().map(|q| self.population(*q) * params.recruit_ceiling_ppm / 1_000_000).sum::<i64>() + retinue;
        let parent = (self.army_size(polity) - retinue).max(0);
        let crushed = if parent + own == 0 { 0.5 } else { parent as f64 / (parent + own) as f64 };
        params.agent_weight_independence * (1.0 - self.actors[&polity].legitimacy / 100.0) - crushed
    }

    /// A bot's answer to a chance of secession (E2: always yes; E3 (в): by utility).
    pub fn house_would_secede(&self, house: crate::ids::HouseId, polity: ActorId, province: ProvinceId, params: &Params) -> bool {
        self.secession_utility(house, polity, province, params) > 0.0
    }

    /// The utility of a usurpation for the candidate's house (plan §23.2 p. 5, owner's rule
    /// on #290): the throne — a goal reached, 1 — against the risk and the price. **The
    /// risk:** the usurper rules at the usurper's norm, and the throne at that legitimacy
    /// kills — the next usurpation (the usurper executed) or a murder (E0 В3) — before the
    /// candidate's natural death within the goal's horizon, by the candidate's age on the
    /// mortality table: an old man risks less than a young one, since the reign is short
    /// either way. **The price:** the legitimacy the polity loses down to the usurper's norm,
    /// at the bot's price of legitimacy. A legitimate ruler is not worth deposing; a weak one
    /// is.
    pub fn usurpation_utility(&self, polity: ActorId, candidate: crate::ids::PersonId, params: &Params) -> f64 {
        let l = self.actors[&polity].legitimacy;
        let lu = l.min(params.legitimacy_norm_usurper);
        let throne = ((params.usurpation_per_tick + params.violent_death_per_tick) * (100.0 - lu) / 50.0).clamp(0.0, 1.0);
        let age = self.age(candidate);
        let (mut alive, mut killed) = (1.0, 0.0);
        for t in 0..params.goal_horizon_ticks {
            let natural = (params.mortality_per_year(age + (t / 2) as i32) * params.mortality_elite_factor / 2.0).clamp(0.0, 1.0);
            killed += alive * throne;
            alive *= (1.0 - throne) * (1.0 - natural);
        }
        params.agent_weight_throne - killed - (l - lu) / 100.0 * params.agent_weight_price_legitimacy
    }

    /// A bot's answer to a chance of usurpation (E2: always yes; E3 (в): by utility).
    pub fn house_would_usurp(&self, polity: ActorId, candidate: crate::ids::PersonId, params: &Params) -> bool {
        self.usurpation_utility(polity, candidate, params) > 0.0
    }

    /// The context of a bot's decision, read once.
    pub fn context(&self, a: ActorId, params: &Params) -> Context {
        Context {
            enemy: self.outmatched(a),
            at_war: self.actors.keys().any(|b| self.at_war(a, *b)),
            income: self.yearly_income(a, params).max(1) as f64,
            unpaid: self.paid_share_seen_ppm(a) < 1_000_000,
            lawful: self.lawful_norm(a, params),
            wanted_allies: self.wanted_allies(a),
        }
    }

    /// The utility of a call for its ruler (plan §23.2 p. 2): goal progress and survival,
    /// less the price.
    pub fn utility(&self, a: ActorId, call: &VerbCall, params: &Params) -> f64 {
        self.utility_in(a, call, params, &self.context(a, params))
    }

    /// The utility of a call in a context read once.
    fn utility_in(&self, a: ActorId, call: &VerbCall, params: &Params, ctx: &Context) -> f64 {
        let actor = &self.actors[&a];
        let (enemy, at_war, income, unpaid, lawful) = (ctx.enemy, ctx.at_war, ctx.income, ctx.unpaid, ctx.lawful);
        let price_kg = |kg: i64| kg as f64 / income * params.agent_weight_price_treasury;
        let price_l = |l: f64| l / 100.0 * params.agent_weight_price_legitimacy;
        match (call.verb, call.target) {
            (VerbId::DeclareWar, Target::Actor(b)) => {
                // What the war is worth: the goals it serves — nothing for a ruler whose army
                // is unpaid, since an unpaid army is an exhausted one by the rule of the
                // peace (`diplomacy::peace_room`) and the war ends at the first peace the
                // other side offers (measured: four fifths of the declarations came the tick
                // after a peace of the same pair, most of them by a declarer already short of
                // its pay — a war a tick long, a peace treaty broken every other tick).
                let mut u = if unpaid { 0.0 } else { self.war_wanted(a, b) };
                // A war the bot would not march in is not worth declaring: its units advance
                // only at `agent.advance_ratio` of the enemy's army (`bot_orders`), and a
                // declarer that stays home loses provinces, accepts the peace as the loser
                // and declares again the next tick (measured: 128 of 170 renewals).
                if self.outweighed(a, b, params) {
                    u -= 1.0;
                }
                if at_war {
                    u -= 0.5;
                }
                u - price_l(self.war_price(a, b, params))
            }
            (VerbId::MakePeace, Target::Actor(b)) => {
                // The peace is the war the other way round: worth what going on would lose —
                // the goals the war serves, less the strength of the enemy, weighed as the
                // declaration weighed them (an army the bot would not march against); a
                // losing war is worth ending, and with nothing to win the peace is wanted.
                // (Measured: «stronger» was every larger army here and half again as large at
                // the declaration, and a bot ended the next tick the war it had just declared.)
                let losing = self.losing_side(a, b) == Some(a);
                let wanted = self.war_wanted(a, b);
                let strength = if self.outweighed(a, b, params) { 1.0 } else { 0.0 };
                let mut u = strength - wanted;
                if losing {
                    u += 1.0;
                }
                if wanted <= 0.0 {
                    u += 0.3;
                }
                u
            }
            (VerbId::Recruit, Target::Province(_)) => {
                let room = self.recruit_room(a, params) as f64 / self.recruit_ceiling(a, params).max(1) as f64;
                let mut u = 0.0;
                if at_war {
                    u += 0.5 * room;
                }
                if enemy.is_some() {
                    u += 0.5;
                }
                if unpaid {
                    u -= 1.0;
                }
                u
            }
            (VerbId::Disband, Target::Unit(_)) => {
                if !at_war && unpaid {
                    0.5
                } else {
                    -1.0
                }
            }
            (VerbId::HireMercenaries, _) => {
                let mut u = 0.0;
                if enemy.is_some() {
                    u += 0.5;
                }
                u - price_kg(self.hire_count(a, params) * params.mercenary_advance_kg)
            }
            (VerbId::TakeLoan, _) => {
                if unpaid && at_war {
                    0.5
                } else {
                    -0.5
                }
            }
            (VerbId::RaiseTax | VerbId::LowerTax, _) => self.tax_utility(a, call.verb, params),
            (VerbId::EnforceOrder, _) => {
                let gap = (lawful - actor.legitimacy).max(0.0) / 100.0;
                2.0 * gap - price_kg(self.order_cost(a, params))
            }
            (VerbId::ProposeAlliance, Target::Actor(b)) => {
                let mut u = 0.0;
                if ctx.wanted_allies.contains(&b) {
                    u += 1.0;
                }
                if enemy.is_some() && self.army_size(b) > self.army_size(a) {
                    u += 0.2;
                }
                u - price_kg(self.embassy_cost(a, params))
            }
            (VerbId::SendEmbassy, Target::Actor(b)) => {
                let wanted = ctx.wanted_allies.contains(&b) && self.relation(a, b) < params.treaty_accept_at_relation;
                (if wanted { 0.5 } else { 0.0 }) - price_kg(self.embassy_cost(a, params))
            }
            (VerbId::Bribe, Target::Actor(b)) => {
                let wanted = ctx.wanted_allies.contains(&b) && self.relation(a, b) < params.treaty_accept_at_relation;
                (if wanted { 0.4 } else { 0.0 }) - price_kg(self.gift_size(a, params))
            }
            (VerbId::ProposeMarriage, Target::House(g)) => {
                let b = self.houses[&g].polity;
                if b != a && ctx.wanted_allies.contains(&b) {
                    0.4
                } else {
                    -0.2
                }
            }
            _ => 0.0,
        }
    }

    /// The utility of a step of the tax (owner's rule on #291): **a lasting decision valued
    /// as a flow over the goal's horizon** — the error of the prices of 6b would otherwise
    /// sit in the bot's head, a one-off point of legitimacy weighed against a tick's take
    /// while the rate moves the take every year. The take: the rate times the share
    /// collected at the legitimacy the norm settles at, before and after the step; its
    /// change is valued in log over the horizon in years at `agent.weight_price_treasury`
    /// (a step down and the step back up cancel — valued as a share of the current take,
    /// the bot would cycle a step around its equilibrium), and weighs more when the army's
    /// pay falls short (revenue short of the pay is worth its inverse share, down to a
    /// quarter). The shift of the norm: points over the horizon at
    /// `agent.weight_price_legitimacy`; the blow or the gain: once. With the shipped
    /// weights the equilibrium is the tithe — both steps from it cost — and the bot comes
    /// back to it from either side by itself; at war with half the pay unmet a step up is
    /// still not worth it, at a third it is. **No bound in the prices** (owner's rule on
    /// #292): the take reads the share as the world collects it, at the unclamped norm below
    /// the bottom, and the lasting shift is the unclamped norm's too — clamped, a ruler at
    /// legitimacy 0 saw no price in a step up and no gain in a step down, raised the tax at
    /// the bottom and stayed there for the term of its default (seed 35 of the full run).
    /// A step at the bottom costs what it costs above zero, in the world and in the bot.
    pub fn tax_utility(&self, a: ActorId, verb: VerbId, params: &Params) -> f64 {
        let actor = &self.actors[&a];
        let years = f64::from(params.goal_horizon_ticks) / 2.0;
        let lawful = self.lawful_norm(a, params);
        let unclamped = self.unclamped_legitimacy_norm(a, params);
        let up = verb == VerbId::RaiseTax;
        let step = if up { params.tax_step_ppm } else { -params.tax_step_ppm } as f64 / 1e6;
        let next_unclamped = unclamped + if up { -params.legitimacy_norm_per_tax_step } else { params.legitimacy_norm_per_tax_step };
        let collected = |l: f64| (1.0 - (lawful - l).max(0.0) * params.tax_evasion_ppm_per_legitimacy_point as f64 / 1e6).max(0.0);
        let rate = actor.tax_rate_ppm as f64 / 1e6;
        let (now, next) = (rate * collected(unclamped), (rate + step) * collected(next_unclamped));
        if now <= 0.0 || next <= 0.0 {
            return -1.0;
        }
        let need = 1.0 / (self.paid_share_seen_ppm(a) as f64 / 1e6).max(0.25);
        let take = years * params.agent_weight_price_treasury * need * (next / now).ln();
        let lasting = years * params.agent_weight_price_legitimacy / 100.0 * (next_unclamped - unclamped);
        let once = params.agent_weight_price_legitimacy / 100.0 * if up { -params.legitimacy_cost_raise_tax } else { params.legitimacy_gain_lower_tax };
        take + lasting + once
    }

    /// The options a ruler has: every available call of the enabled verbs over their
    /// targets, by the same rules the offers use, each with its utility; doing nothing at
    /// nothing. Canonical order of verbs and targets.
    pub fn options(&self, a: ActorId, enabled: &[VerbId], params: &Params) -> Vec<Option_> {
        let who = ActorRef::Ruler(a);
        let mut out = vec![Option_ { call: None, utility: 0.0 }];
        if !self.alive(a) {
            return out;
        }
        let others: Vec<ActorId> = self.actors.keys().copied().filter(|b| *b != a && self.alive(*b)).collect();
        let mut calls: Vec<VerbCall> = Vec::new();
        for v in enabled {
            match v {
                VerbId::DeclareWar => calls.extend(others.iter().filter(|b| self.war_room(a, **b).is_ok()).map(|b| VerbCall { actor: who, verb: *v, target: Target::Actor(*b) })),
                VerbId::MakePeace => calls.extend(others.iter().filter(|b| self.peace_room(a, **b, params).is_ok()).map(|b| VerbCall { actor: who, verb: *v, target: Target::Actor(*b) })),
                VerbId::ProposeAlliance => {
                    let cost = self.embassy_cost(a, params);
                    if cost > 0 && self.treasury(a) >= cost && self.lord_of(a).is_none() {
                        calls.extend(others.iter().filter(|b| self.alliance_room(a, **b, params).is_ok()).map(|b| VerbCall { actor: who, verb: *v, target: Target::Actor(*b) }));
                    }
                }
                VerbId::SendEmbassy => {
                    let cost = self.embassy_cost(a, params);
                    if cost > 0 && self.treasury(a) >= cost {
                        calls.extend(others.iter().map(|b| VerbCall { actor: who, verb: *v, target: Target::Actor(*b) }));
                    }
                }
                VerbId::Bribe => {
                    let gift = self.gift_size(a, params);
                    if gift > 0 && self.treasury(a) >= gift {
                        calls.extend(others.iter().map(|b| VerbCall { actor: who, verb: *v, target: Target::Actor(*b) }));
                    }
                }
                VerbId::ProposeMarriage => {
                    // The rule of `marriage_room`, with each house's eldest unmarried adult
                    // read in one pass over the persons instead of one per pair of houses.
                    let h = self.ruling_house(a);
                    let unmarried = self.unmarried_adults(params);
                    if unmarried.contains_key(&h) {
                        calls.extend(
                            self.houses
                                .iter()
                                .filter(|(g, x)| **g != h && unmarried.contains_key(g) && (x.polity == a || (!self.at_war(a, x.polity) && self.relation(a, x.polity) >= params.treaty_accept_at_relation)))
                                .map(|(g, _)| VerbCall { actor: who, verb: *v, target: Target::House(*g) }),
                        );
                    }
                }
                VerbId::Recruit => {
                    if self.recruit_per_tick(a, params) > 0 {
                        calls.extend(self.actors[&a].provinces.iter().filter(|p| !self.in_revolt(**p) && self.population(**p) > 0).map(|p| VerbCall { actor: who, verb: *v, target: Target::Province(*p) }));
                    }
                }
                VerbId::Disband => calls.extend(self.units_of(a).into_iter().filter(|u| self.units[u].house.is_none()).map(|u| VerbCall { actor: who, verb: *v, target: Target::Unit(u) })),
                VerbId::HireMercenaries => {
                    if self.hire_count(a, params) > 0 {
                        calls.push(VerbCall { actor: who, verb: *v, target: Target::None });
                    }
                }
                VerbId::TakeLoan => {
                    if self.actors[&a].institutions.contains(&crate::world::Institution::Bank) && !self.credit_closed(a) && self.yearly_income(a, params).min(self.loan_room(a, params)) > 0 {
                        calls.push(VerbCall { actor: who, verb: *v, target: Target::None });
                    }
                }
                VerbId::RaiseTax | VerbId::LowerTax => {
                    if self.tax_room(a, *v, params).is_ok() {
                        calls.push(VerbCall { actor: who, verb: *v, target: Target::None });
                    }
                }
                VerbId::EnforceOrder => {
                    let cost = self.order_cost(a, params);
                    if cost > 0 && self.treasury(a) >= cost {
                        calls.push(VerbCall { actor: who, verb: *v, target: Target::None });
                    }
                }
            }
        }
        let ctx = self.context(a, params);
        for call in calls {
            let utility = self.utility_in(a, &call, params, &ctx);
            out.push(Option_ { call: Some(call), utility });
        }
        out
    }

    /// Each house's eldest living adult with no living spouse — `unmarried_adult` for every
    /// house at once, in one pass over the persons.
    pub fn unmarried_adults(&self, params: &Params) -> std::collections::BTreeMap<crate::ids::HouseId, crate::ids::PersonId> {
        let mut out: std::collections::BTreeMap<crate::ids::HouseId, (i32, crate::ids::PersonId)> = std::collections::BTreeMap::new();
        for (id, p) in &self.persons {
            if p.died.is_some() || !self.is_adult(*id, params) || p.spouse.is_some_and(|s| self.person_alive(s)) {
                continue;
            }
            let e = out.entry(p.house).or_insert((p.born, *id));
            if (p.born, *id) < *e {
                *e = (p.born, *id);
            }
        }
        out.into_iter().map(|(h, (_, id))| (h, id)).collect()
    }

    /// The set a bot chooses from: doing nothing at nothing, and for every verb its best
    /// target — when that is better than doing nothing. Two rules of the choice (plan
    /// §23b): **the number of targets must not decide the verb** (forty polities to bribe
    /// at a hair below nothing would outweigh nothing forty times over in a softmax over
    /// every call — measured: twelve thousand gifts in a run), so a verb enters once, with
    /// its best target (ties: the canonical first); and **what is no better than doing
    /// nothing is not chosen** — a price paid for nothing is not a choice but noise.
    pub fn choices(&self, a: ActorId, enabled: &[VerbId], params: &Params) -> Vec<Option_> {
        let mut best: Vec<Option_> = Vec::new();
        for o in self.options(a, enabled, params).into_iter().skip(1) {
            let verb = o.call.map(|c| c.verb);
            match best.iter_mut().find(|b| b.call.map(|c| c.verb) == verb) {
                Some(b) if o.utility > b.utility => *b = o,
                Some(_) => {}
                None => best.push(o),
            }
        }
        let mut out = vec![Option_ { call: None, utility: 0.0 }];
        out.extend(best.into_iter().filter(|o| o.utility > 0.0));
        out
    }

    /// One bot's decision: softmax over its choices at the temperature, by one keyed draw.
    pub fn decide(&mut self, a: ActorId, enabled: &[VerbId], params: &Params) -> Option<VerbCall> {
        let options = self.choices(a, enabled, params);
        let d = Draw { point: DrawPoint::Decision, tick: self.tick, id: a.0, n: self.draw_ordinal(DrawPoint::Decision, a.0) };
        let value = crate::rng::draw(self.seed, d);
        self.journal_draw(d, value);
        let u = unit(value);
        let t = params.agent_temperature.max(1e-6);
        let top = options.iter().map(|o| o.utility).fold(f64::MIN, f64::max);
        let weights: Vec<f64> = options.iter().map(|o| ((o.utility - top) / t).exp()).collect();
        let total: f64 = weights.iter().sum();
        let mut acc = 0.0;
        for (o, w) in options.iter().zip(weights) {
            acc += w / total;
            if u < acc {
                return o.call;
            }
        }
        options.last().and_then(|o| o.call)
    }

    /// Every bot decides on the state as it stands — the player's actor excepted — in
    /// canonical order; nothing is applied here (brief §3: first all decide, then all is
    /// applied).
    pub fn decide_all(&mut self, enabled: &[VerbId], player: Option<ActorRef>, params: &Params) -> Vec<VerbCall> {
        let ids: Vec<ActorId> = self.actors.keys().copied().filter(|a| self.alive(*a) && player != Some(ActorRef::Ruler(*a))).collect();
        let mut out = Vec::new();
        for a in ids {
            if let Some(call) = self.decide(a, enabled, params) {
                out.push(call);
            }
        }
        out
    }
}
