//! What a caller may call, by its target (E5, plan §31.77, executor's decision № 28).
//!
//! Lesson B42 said it of the verb: availability and cost come from one function, the same
//! for the player and for the agents. Here it is said of the **target**. Until E5 only the
//! bot knew whom a verb could name — `World::options_seen` (`agents.rs`) went over the
//! targets with closures of its own — and the shell was offered the verb alone. The going
//! over the targets now lives here, and both read it: the bot draws its calls from
//! `target_candidates` and `target_room` (its two narrowings — the levy from one province in
//! six sizes, no disbanding below the starting army — stay its own choice among what is
//! open), and the shell gets the offers by target (`target_offers`, `Simulation::targets`).
//!
//! The rules themselves are not here: `caller_room` and `target_room` call the rooms the
//! application calls (`war_room`, `claim_room`, `claim_met`, `peace_room`, `alliance_room`,
//! `tax_room`, `recruit_per_tick`, `hire_count`, `loan_room`, …). One rule is read in its
//! one-pass form: a marriage's two unmarried adults (`World::unmarried_adults`, read once a
//! decision — `TargetSeen`) where the application reads them house by house
//! (`marriage_room`); the test «an offer is open exactly where the application goes through»
//! holds the two together.

use std::cell::OnceCell;
use std::collections::BTreeMap;

use crate::ids::{ActorId, HouseId, PersonId};
use crate::params::Params;
use crate::verbs::{ActorRef, Target, TargetOffer, VerbId};
use crate::world::{Institution, WarAim, World};

/// What one decision reads once for all its targets: each house's eldest unmarried adult.
#[derive(Debug, Default)]
pub struct TargetSeen {
    unmarried: OnceCell<BTreeMap<HouseId, PersonId>>,
}

impl World {
    fn unmarried_seen<'a>(&self, seen: &'a TargetSeen, params: &Params) -> &'a BTreeMap<HouseId, PersonId> {
        seen.unmarried.get_or_init(|| self.unmarried_adults(params))
    }

    /// The house a caller's marriage speaks for, and its polity; none for a fallen ruler.
    fn marrying_house(&self, who: ActorRef) -> Option<(HouseId, ActorId)> {
        match who {
            ActorRef::Ruler(a) => self.alive(a).then(|| (self.ruling_house(a), a)),
            ActorRef::House(h) => self.houses.get(&h).map(|x| (h, x.polity)),
        }
    }

    /// The caller's side of a verb: what must hold of the caller whatever the target — it
    /// rules a living polity, it can pay, it has room. The same conditions the application
    /// checks before it looks at the target.
    pub fn caller_room(&self, who: ActorRef, verb: VerbId, params: &Params) -> Result<(), &'static str> {
        self.caller_room_seen(who, verb, params, &TargetSeen::default())
    }

    /// `caller_room` with what the decision has read already.
    pub(crate) fn caller_room_seen(&self, who: ActorRef, verb: VerbId, params: &Params, seen: &TargetSeen) -> Result<(), &'static str> {
        if verb == VerbId::ProposeMarriage {
            let (h, _) = self.marrying_house(who).ok_or("the polity has fallen")?;
            return self.unmarried_seen(seen, params).contains_key(&h).then_some(()).ok_or("no unmarried adult to marry");
        }
        let ActorRef::Ruler(a) = who else { return Err("a house does not rule the polity") };
        if !self.alive(a) {
            return Err("the polity has fallen");
        }
        let pays = |cost: i64| -> Result<(), &'static str> {
            if cost <= 0 {
                Err("no revenue to pay from")
            } else if self.treasury(a) < cost {
                Err("cannot pay")
            } else {
                Ok(())
            }
        };
        match verb {
            VerbId::DeclareWar | VerbId::MakePeace | VerbId::Disband | VerbId::ProposeMarriage => Ok(()),
            VerbId::ProposeAlliance => {
                pays(self.embassy_cost(a, params))?;
                if self.lord_of(a).is_some() {
                    return Err("a vassal makes no alliance");
                }
                Ok(())
            }
            VerbId::SendEmbassy => pays(self.embassy_cost(a, params)),
            VerbId::Bribe => pays(self.gift_size(a, params)),
            VerbId::Recruit => (self.recruit_per_tick(a, params) > 0).then_some(()).ok_or("at the levy ceiling"),
            VerbId::HireMercenaries => {
                if self.hire_count(a, params) > 0 {
                    Ok(())
                } else if self.recruit_room(a, params) <= 0 {
                    Err("at the levy ceiling")
                } else {
                    Err("cannot pay an advance")
                }
            }
            VerbId::TakeLoan => {
                if !self.actors[&a].institutions.contains(&Institution::Bank) {
                    Err("no bank")
                } else if self.credit_closed(a) {
                    Err("in default: no credit")
                } else if self.yearly_income(a, params).min(self.loan_room(a, params)) <= 0 {
                    Err("at the debt ceiling")
                } else {
                    Ok(())
                }
            }
            VerbId::RaiseTax | VerbId::LowerTax => self.tax_room(a, verb, params),
            VerbId::EnforceOrder => pays(self.order_cost(a, params)),
        }
    }

    /// Whom a verb of `who` could name by the make of the world, in canonical order: the
    /// other living polities (for a peace, those at war with it), for a declaration their
    /// provinces after them (a claim on a province is a war on its owner); the other
    /// houses; the polity's own provinces; its units; nothing at all.
    pub fn target_candidates(&self, who: ActorRef, verb: VerbId) -> Vec<Target> {
        if verb == VerbId::ProposeMarriage {
            let Some((h, _)) = self.marrying_house(who) else { return Vec::new() };
            return self.houses.keys().filter(|g| **g != h).map(|g| Target::House(*g)).collect();
        }
        let ActorRef::Ruler(a) = who else { return Vec::new() };
        if !self.alive(a) {
            return Vec::new();
        }
        let others = || self.actors.keys().copied().filter(move |b| *b != a && self.alive(*b));
        match verb {
            VerbId::DeclareWar => others().map(Target::Actor).chain(self.provinces.iter().filter(|(_, pr)| pr.owner != a).map(|(q, _)| Target::Province(*q))).collect(),
            VerbId::MakePeace => others().filter(|b| self.at_war(a, *b)).map(Target::Actor).collect(),
            VerbId::ProposeAlliance | VerbId::SendEmbassy | VerbId::Bribe => others().map(Target::Actor).collect(),
            VerbId::Recruit => self.actors[&a].provinces.iter().map(|p| Target::Province(*p)).collect(),
            VerbId::Disband => self.units_of(a).into_iter().map(Target::Unit).collect(),
            VerbId::HireMercenaries | VerbId::TakeLoan | VerbId::RaiseTax | VerbId::LowerTax | VerbId::EnforceOrder => vec![Target::None],
            VerbId::ProposeMarriage => Vec::new(),
        }
    }

    /// The offers of a verb of `who` over its targets (the shell's side of the going over;
    /// `Simulation::targets`): every target the world could name, open or not, with the
    /// reason and the price the application would charge.
    pub fn target_offers(&self, who: ActorRef, verb: VerbId, params: &Params) -> Vec<TargetOffer> {
        let seen = TargetSeen::default();
        let caller = self.caller_room_seen(who, verb, params, &seen);
        self.target_candidates(who, verb)
            .into_iter()
            .map(|target| {
                let room = caller.and_then(|()| self.target_room_seen(who, verb, target, params, &seen));
                let mut offer = self.target_price(who, verb, target, params);
                offer.available = room.is_ok();
                offer.why = room.err().map(str::to_string);
                offer
            })
            .collect()
    }

    /// What the call would cost and move, read where the application reads it; the
    /// availability is `target_offers`'s to fill.
    fn target_price(&self, who: ActorRef, verb: VerbId, target: Target, params: &Params) -> TargetOffer {
        let mut o = TargetOffer { target, available: false, why: None, cost_kg: 0, cost_legitimacy: 0.0, threshold: None, relation: None, up_to: None, receives_kg: None, dowry_kg: None };
        let threshold = Some(params.treaty_accept_at_relation.round() as i64);
        if verb == VerbId::ProposeMarriage {
            if let (Some((h, a)), Target::House(g)) = (self.marrying_house(who), target) {
                o.dowry_kg = Some(self.dowry(h, params));
                o.threshold = threshold;
                o.relation = self.houses.get(&g).map(|x| x.polity).filter(|b| *b != a).map(|b| self.relation(a, b));
            }
            return o;
        }
        let ActorRef::Ruler(a) = who else { return o };
        if !self.alive(a) {
            return o;
        }
        // The polity the target stands for: its own, or the owner of the province named.
        let b = match target {
            Target::Actor(b) => Some(b),
            Target::Province(q) if verb == VerbId::DeclareWar => self.provinces.get(&q).map(|pr| pr.owner),
            _ => None,
        }
        .filter(|b| *b != a && self.actors.contains_key(b));
        o.relation = b.map(|b| self.relation(a, b));
        match (verb, target) {
            (VerbId::DeclareWar, _) => o.cost_legitimacy = b.map_or(params.legitimacy_cost_declare_war, |b| self.war_price(a, b, params)),
            (VerbId::MakePeace, _) => o.threshold = threshold,
            (VerbId::ProposeAlliance, _) => {
                o.cost_kg = self.embassy_cost(a, params);
                o.threshold = threshold;
            }
            (VerbId::SendEmbassy, _) => o.cost_kg = self.embassy_cost(a, params),
            (VerbId::Bribe, _) => o.cost_kg = self.gift_size(a, params),
            (VerbId::Recruit, Target::Province(p) | Target::Levy(p, _)) => {
                let own = self.provinces.get(&p).is_some_and(|x| x.owner == a) && !self.in_revolt(p);
                o.up_to = Some(if own { self.recruit_per_tick(a, params).min(self.population(p)).max(0) as u32 } else { 0 });
            }
            (VerbId::HireMercenaries, _) => {
                o.cost_kg = self.hire_advance(a, params);
                o.up_to = Some(self.hire_count(a, params).max(0) as u32);
            }
            (VerbId::TakeLoan, _) => o.receives_kg = Some(self.yearly_income(a, params).min(self.loan_room(a, params)).max(0)),
            (VerbId::RaiseTax | VerbId::LowerTax, _) => o.cost_legitimacy = self.tax_step_cost(a, verb == VerbId::RaiseTax, params),
            (VerbId::EnforceOrder, _) => {
                o.cost_kg = self.order_cost(a, params);
                o.cost_legitimacy = -params.order_gain;
            }
            _ => {}
        }
        o
    }

    /// Could `a` open a war with `b` or, in the war `b` declared on it, state its claim?
    /// The pair's side of a declaration; the claim it names is read by `target_room`.
    pub fn declare_room(&self, a: ActorId, b: ActorId) -> Result<(), &'static str> {
        if self.at_war(a, b) {
            self.claim_room(a, b)
        } else {
            self.war_room(a, b)
        }
    }

    /// Whether `target` is open to the verb of `who` — the target's side of the call, by the
    /// rooms the application reads; the caller's side is `caller_room`. The reason it is not.
    pub fn target_room(&self, who: ActorRef, verb: VerbId, target: Target, params: &Params) -> Result<(), &'static str> {
        self.target_room_seen(who, verb, target, params, &TargetSeen::default())
    }

    /// `target_room` with what the decision has read already.
    pub(crate) fn target_room_seen(&self, who: ActorRef, verb: VerbId, target: Target, params: &Params, seen: &TargetSeen) -> Result<(), &'static str> {
        if verb == VerbId::ProposeMarriage {
            let (h, a) = self.marrying_house(who).ok_or("the polity has fallen")?;
            let Target::House(g) = target else { return Err("a house to marry into") };
            if h == g {
                return Err("a house does not marry itself");
            }
            let Some(other) = self.houses.get(&g) else { return Err("no such house") };
            let unmarried = self.unmarried_seen(seen, params);
            if !unmarried.contains_key(&h) {
                return Err("no unmarried adult to marry");
            }
            if !unmarried.contains_key(&g) {
                return Err("the other house has no unmarried adult");
            }
            let b = other.polity;
            if a != b {
                if self.at_war(a, b) {
                    return Err("at war");
                }
                if self.relation(a, b) < params.treaty_accept_at_relation {
                    return Err("the relation is below the threshold");
                }
            }
            return Ok(());
        }
        let ActorRef::Ruler(a) = who else { return Err("a house does not rule the polity") };
        match (verb, target) {
            (VerbId::DeclareWar, Target::Actor(_) | Target::Province(_)) => {
                let (b, aim) = match target {
                    Target::Province(q) => match self.provinces.get(&q) {
                        Some(pr) => (pr.owner, WarAim::Province(q)),
                        None => return Err("no such province"),
                    },
                    Target::Actor(b) => (b, WarAim::Polity(b)),
                    _ => unreachable!(),
                };
                if !self.actors.contains_key(&b) {
                    return Err("no such polity");
                }
                self.declare_room(a, b)?;
                if self.claim_met(a, b, aim) {
                    return Err("the claim is met already: claim a province");
                }
                Ok(())
            }
            (VerbId::MakePeace, Target::Actor(b)) => self.peace_room(a, b, params),
            (VerbId::ProposeAlliance, Target::Actor(b)) => {
                if a == b || !self.actors.contains_key(&b) {
                    return Err("no polity to ally with");
                }
                self.alliance_room(a, b, params)
            }
            (VerbId::SendEmbassy | VerbId::Bribe, Target::Actor(b)) => (a != b && self.alive(b)).then_some(()).ok_or("another living polity"),
            (VerbId::Recruit, Target::Province(p) | Target::Levy(p, _)) => {
                let men = match target {
                    Target::Levy(_, 0) => return Err("no men to levy"),
                    Target::Levy(_, men) => i64::from(men),
                    _ => i64::MAX,
                };
                if self.provinces.get(&p).map(|x| x.owner) != Some(a) {
                    return Err("not the polity's own province");
                }
                if self.in_revolt(p) {
                    return Err("the province is in revolt");
                }
                if self.recruit_per_tick(a, params).min(self.population(p)).min(men) <= 0 {
                    return Err("at the levy ceiling");
                }
                Ok(())
            }
            (VerbId::Disband, Target::Unit(u)) => (self.units.get(&u).map(|x| x.owner) == Some(a)).then_some(()).ok_or("not the polity's own unit"),
            (VerbId::HireMercenaries | VerbId::TakeLoan | VerbId::RaiseTax | VerbId::LowerTax | VerbId::EnforceOrder, Target::None) => Ok(()),
            _ => Err("not a target of the verb"),
        }
    }
}
