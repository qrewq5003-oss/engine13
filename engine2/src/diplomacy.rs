//! Diplomacy (brief §2 «Дипломатия», plan §19): the relation of every pair of polities as a
//! rating with a norm derived from the state of the pair, treaties with a term (E0 Д1),
//! vassalage with its tribute (E0 Д2, `economy.rs`), the defensive alliance that brings the
//! allies of the attacked into the war (`army.rs`), and the marriage of two houses.
//!
//! **The relation** (`World::relations`, 0–100, one per pair, lower id first) is an
//! evaluation, so it has a norm and moves towards it (brief §2: no clamp as a mechanic; the
//! edge of the scale is an invariant the sentinel watches). The norm comes from the state of
//! the pair alone: `relation.base`, less `relation.norm_war` at war, plus
//! `relation.norm_alliance` under an alliance, plus `relation.norm_marriage` while a
//! marriage binds a house of each, plus `relation.norm_vassalage` between a vassal and its
//! lord, less `relation.norm_border` when their provinces touch (the threat of a neighbour;
//! the weights of threat — strength, intent, distance — are E3's), less
//! `relation.norm_faith` and `relation.norm_culture` when the two courts pray or speak
//! differently (E2 step 8, `faith.rs`). A peace in force shifts
//! nothing: neither enmity nor friendship. Verbs push the relation once, with saturation
//! towards the end of the scale they move to; treaties shift the norm.
//!
//! **Readers.** The acceptance of proposals (`treaty.accept_at_relation`); the threat and
//! the utility of the agents of E3. A rating nobody reads is a dead field (brief §5 rule
//! 7), so acceptance goes through the relation and nothing else.
//!
//! **Treaties** (`World::treaties`): an alliance or a peace between a pair, running to a
//! tick; the term is E0 Д1 for both; it ends with `TreatyExpired` in the diplomacy phase and
//! is renewed only by a new proposal («no longer and no shorter than the term», E0 §1.7).
//! Breaking one is a price, not a prohibition (owner's rule on §19).
//!
//! **Vassalage** (`Actor::vassal_of`): a fact of the scenario or the outcome of a peace;
//! the tribute flows in the economy (E0 Д2), not while the vassal is at war with its lord;
//! the lord's wars are the vassal's and the lord defends the vassal (`army.rs::open_war`);
//! a fallen lord frees its vassals (`army.rs`); the way out is a war on the lord.
//!
//! **Obligations of a bond** (`army.rs::open_war`, owner's rule on §19): a war declared on
//! a polity brings its allies, its lord and its vassals in against the attacker, and the
//! attacker's vassals in against the attacked; the entry is no declaration — it costs
//! nothing, breaks no treaty and pulls nobody further in. Bound polities march through each
//! other's land and are supplied there by their pay, never by foraging (`movement.rs`).
//!
//! **Verbs** (plan §19.3; every one through the offers with its `why`, the player and the
//! agents on one rule — B42). `declare_war` costs `legitimacy.cost_declare_war`, times
//! `treaty.breach_factor` when a treaty with the target is in force (the treaty goes, the
//! relations with the victim's allies go to nothing — a price, not a prohibition); a vassal
//! declares war on its lord alone (the way out). `make_peace` is accepted **by the state of
//! the war, not by terms** (`VerbCall` carries none): the losing side — the one that has
//! lost more provinces in this war — accepts always; the winning side accepts when the
//! relation is at `treaty.accept_at_relation`, so at the norm of war peace comes in effect
//! when the winner proposes it (owner's rule on §19); nothing taken either way is a status
//! quo both accept. The peace closes the wars that were obligations of this one, binds the
//! pair by a treaty of peace for the term, and settles the loser: down to half the
//! provinces it held at the start it **submits** as the winner's vassal; a vassal that beat
//! its lord is **released**. `propose_alliance` spends an embassy's gifts, accepted or not,
//! and is accepted at the threshold between polities neither at war, nor bound by vassalage,
//! nor at war with each other's allies. `send_embassy` spends the gifts for a push of the
//! relation; `bribe` is the gift of E0 Д2 — a transfer that stays in the world — pushing in
//! proportion to what it is worth to the receiver. `propose_marriage` is open to every house
//! of a polity, ruling or not (its way up, owner's rule): the eldest unmarried adults of the
//! two houses marry, the dowry is a share of the proposing house's treasury, and across
//! polities the threshold applies.
//!
//! **Phase** (after the house and the throne, before the people): treaties that have run
//! out end; every relation of two living polities moves `relation.rate_per_tick` of the way
//! to its norm.

use crate::economy::VerbError;
use crate::event::Event;
use crate::ids::{ActorId, HouseId, PersonId};
use crate::ledger::{Account, Source};
use crate::params::Params;
use crate::time::Tick;
use crate::verbs::{ActorRef, Target, VerbCall, VerbId};
use crate::world::{Treaty, TreatyKind, WarAim, World, WorldError};

pub(crate) use crate::army::pair;

/// The rule of the peace (§19; plan §28.1 and §28.8, owner's decision 2026-10-17) on the
/// state of a war: would `b` accept a peace from `a`? A `b` **forced** to peace always
/// (`World::forced_to_peace`: it lost more provinces in the war and its side is the weaker by
/// the battle formula); a `b` with a **cause** to go on — a live war goal against `a`, or
/// provinces lost with the better odds — only at the threshold of the relation or exhausted;
/// a `b` with nothing to fight for always. Who declared the war does not enter: the rule of
/// #291 bound the attacked to accept the declarer's peace, and an attacked side that had lost
/// provinces with the stronger army took the peace and declared again the next tick (68 of
/// 97 renewals the tick after a peace, plan §28.8). One function for `peace_room` and for
/// the bots' worth of a war (E3 (е2)): whether a beaten side can end its war.
pub fn peace_rule(b_forced: bool, b_has_cause: bool, at_threshold: bool, b_exhausted: bool) -> bool {
    if b_forced {
        return true;
    }
    if b_has_cause {
        return at_threshold || b_exhausted;
    }
    true
}

/// A relation after a tick of its drift to the norm (`relation.rate_per_tick` of the way).
pub fn relation_step(r: f64, norm: f64, params: &Params) -> f64 {
    r + params.relation_rate_per_tick * (norm - r)
}

/// A relation after a push of `delta`: the push takes its share of the distance left to the
/// bound it moves toward (`push_relation`; the worth of an embassy or a gift reads it).
pub fn pushed_relation(r: f64, delta: f64) -> f64 {
    let moved = if delta >= 0.0 { delta * (100.0 - r) / 100.0 } else { delta * r / 100.0 };
    (r + moved).clamp(0.0, 100.0)
}

/// The verbs this system prices and applies.
pub fn is_diplomatic(verb: VerbId) -> bool {
    matches!(verb, VerbId::DeclareWar | VerbId::MakePeace | VerbId::ProposeAlliance | VerbId::ProposeMarriage | VerbId::SendEmbassy | VerbId::Bribe)
}

/// What a diplomatic verb offers `who`: the one availability-and-cost answer the offers
/// and the application share.
#[derive(Debug, Clone, PartialEq)]
pub struct DiplomacyOffer {
    pub available: bool,
    pub cost_kg: i64,
    pub cost_legitimacy: i64,
    pub why: Option<String>,
    pub threshold: Option<i64>,
}

impl World {
    /// The relation of a pair, 0–100; a polity has none with itself.
    pub fn relation(&self, a: ActorId, b: ActorId) -> f64 {
        if a == b {
            return 0.0;
        }
        self.relations.get(&pair(a, b)).copied().unwrap_or(0.0)
    }

    /// The treaty in force between a pair, if any.
    pub fn treaty(&self, a: ActorId, b: ActorId) -> Option<&Treaty> {
        if a == b {
            return None;
        }
        self.treaties.get(&pair(a, b))
    }

    pub fn allied(&self, a: ActorId, b: ActorId) -> bool {
        self.treaty(a, b).is_some_and(|t| t.kind == TreatyKind::Alliance)
    }

    pub fn peace_in_force(&self, a: ActorId, b: ActorId) -> bool {
        self.treaty(a, b).is_some_and(|t| t.kind == TreatyKind::Peace)
    }

    /// Friendly land for a polity's units: its own, an ally's, its lord's or a vassal's —
    /// open to the march, and supplied there by the pay, not by foraging (owner's note on
    /// #284: a paid army on an ally's land buys its food, it does not ruin the ally).
    pub fn friendly(&self, a: ActorId, b: ActorId) -> bool {
        a == b || self.allied(a, b) || self.bound_by_vassalage(a, b)
    }

    /// The polities allied with `a` by a treaty in force, in id order.
    pub fn allies_of(&self, a: ActorId) -> Vec<ActorId> {
        self.treaties
            .iter()
            .filter(|((x, y), t)| t.kind == TreatyKind::Alliance && (*x == a || *y == a))
            .map(|((x, y), _)| if *x == a { *y } else { *x })
            .collect()
    }

    pub fn lord_of(&self, a: ActorId) -> Option<ActorId> {
        self.actors.get(&a).and_then(|x| x.vassal_of)
    }

    /// The vassals of a lord, in id order.
    pub fn vassals_of(&self, lord: ActorId) -> Vec<ActorId> {
        self.actors.iter().filter(|(_, x)| x.vassal_of == Some(lord)).map(|(id, _)| *id).collect()
    }

    /// Vassal and lord, either way round.
    pub fn bound_by_vassalage(&self, a: ActorId, b: ActorId) -> bool {
        a != b && (self.lord_of(a) == Some(b) || self.lord_of(b) == Some(a))
    }

    /// Do the two polities' provinces touch, by land or by sea?
    pub fn borders(&self, a: ActorId, b: ActorId) -> bool {
        a != b
            && self.actors[&a].provinces.iter().any(|p| {
                let pr = &self.provinces[p];
                pr.neighbours.iter().chain(pr.sea_neighbours.iter()).any(|n| self.provinces[n].owner == b)
            })
    }

    /// Is a living marriage binding a house of `a` to a house of `b`?
    pub fn married_across(&self, a: ActorId, b: ActorId) -> bool {
        a != b
            && self.persons.iter().any(|(_, p)| {
                p.died.is_none()
                    && self.houses[&p.house].polity == a
                    && p.spouse.is_some_and(|s| {
                        let sp = &self.persons[&s];
                        sp.died.is_none() && self.houses[&sp.house].polity == b
                    })
            })
    }

    /// The norm of a pair's relation from the state of the pair (plan §19.2).
    pub fn relation_norm(&self, a: ActorId, b: ActorId, params: &Params) -> f64 {
        let mut norm = params.relation_base;
        if self.at_war(a, b) {
            norm -= params.relation_norm_war;
        }
        if self.allied(a, b) {
            norm += params.relation_norm_alliance;
        }
        if self.married_across(a, b) {
            norm += params.relation_norm_marriage;
        }
        if self.bound_by_vassalage(a, b) {
            norm += params.relation_norm_vassalage;
        }
        if self.borders(a, b) {
            norm -= params.relation_norm_border;
        }
        // The friction of faith and tongue between the two courts (E2 step 8).
        if self.polity_religion(a) != self.polity_religion(b) {
            norm -= params.relation_norm_faith;
        }
        if self.polity_culture(a) != self.polity_culture(b) {
            norm -= params.relation_norm_culture;
        }
        norm.clamp(0.0, 100.0)
    }

    /// A one-off push of a relation, saturating towards the end of the scale it moves to:
    /// `delta × (100 − R) / 100` upwards, `delta × R / 100` downwards — so no push pins the
    /// relation to an edge.
    pub fn push_relation(&mut self, a: ActorId, b: ActorId, delta: f64) {
        if a == b {
            return;
        }
        let r = pushed_relation(self.relation(a, b), delta);
        self.relations.insert(pair(a, b), r);
    }

    /// How far a gift moves the giver in the receiver's relation: by its worth to the
    /// receiver, the gift over the receiver's revenue (E0 Д2).
    pub fn gift_push(&self, a: ActorId, b: ActorId, params: &Params) -> f64 {
        params.bribe_gain_per_share * (self.gift_size(a, params) as f64 / self.yearly_income(b, params).max(1) as f64)
    }

    /// Bind a pair by a treaty running `treaty.term_years` from now; a new treaty of the
    /// pair replaces the old one.
    pub fn bind(&mut self, a: ActorId, b: ActorId, kind: TreatyKind, params: &Params) -> Result<Tick, WorldError> {
        if a == b {
            return Err(WorldError::RelationWithSelf(a));
        }
        let until = Tick(self.tick.0 + params.treaty_term_ticks);
        self.treaties.insert(pair(a, b), Treaty { kind, until, since: Some(self.tick) });
        Ok(until)
    }

    /// For the tests: an alliance made now (the verb comes with step (в)).
    #[cfg(feature = "testing")]
    pub fn ally_now(&mut self, a: ActorId, b: ActorId, params: &Params) -> Result<Tick, WorldError> {
        self.bind(a, b, TreatyKind::Alliance, params)
    }

    /// For the tests: a vassalage made now.
    #[cfg(feature = "testing")]
    pub fn make_vassal(&mut self, vassal: ActorId, lord: ActorId) -> Result<(), WorldError> {
        if vassal == lord {
            return Err(WorldError::RelationWithSelf(vassal));
        }
        self.actors.get_mut(&vassal).ok_or(WorldError::UnknownActor(vassal))?.vassal_of = Some(lord);
        Ok(())
    }

    /// For the tests: a marriage made now between two persons of different houses.
    #[cfg(feature = "testing")]
    pub fn marry_now(&mut self, p: crate::ids::PersonId, q: crate::ids::PersonId) {
        self.persons.get_mut(&p).unwrap().spouse = Some(q);
        self.persons.get_mut(&q).unwrap().spouse = Some(p);
    }

    // ---- the verbs ----

    /// The losing side of a war: the one that has lost more provinces in it (owner's rule
    /// on §19: by the outcome, since both sides may lose something); none when even. The
    /// terms of the peace read it (who cedes, who submits); who must accept a peace is
    /// `forced_to_peace`.
    pub fn losing_side(&self, a: ActorId, b: ActorId) -> Option<ActorId> {
        let (lo, hi) = pair(a, b);
        let war = self.wars.get(&(lo, hi))?;
        match war.taken.0.cmp(&war.taken.1) {
            std::cmp::Ordering::Greater => Some(hi),
            std::cmp::Ordering::Less => Some(lo),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// The side forced to accept any peace in the war of `a` and `b` (plan §28.1, owner's
    /// rule after the stop of (е2) on #298): the one that has lost more provinces in it
    /// (`losing_side`) **and** whose side is the weaker by the battle formula — the strength
    /// of its side against the other's, each with those in the war by its obligations
    /// (`war_sides`, `army_strength`), `win_chance` below a half; at even strength nobody is
    /// forced. A loser of provinces with the stronger side is not forced: it would take back
    /// what it lost (measured on #298: polity 9 with 13 441 men against 6 lost undefended
    /// provinces, accepted the peace as the loser and took them back the next tick — the
    /// revanche that stopped (е2)).
    pub fn forced_to_peace(&self, a: ActorId, b: ActorId, params: &Params) -> Option<ActorId> {
        let loser = self.losing_side(a, b)?;
        let (side_a, side_b) = self.war_sides(a, b);
        let strength = |side: &[ActorId]| side.iter().map(|x| self.army_strength(*x)).sum::<f64>();
        let (mine, theirs) = if loser == a { (strength(&side_a), strength(&side_b)) } else { (strength(&side_b), strength(&side_a)) };
        let odds = if theirs <= 0.0 {
            if mine > 0.0 {
                1.0
            } else {
                0.5
            }
        } else if mine <= 0.0 {
            0.0
        } else {
            crate::army::win_chance(mine, theirs, params)
        };
        (odds < 0.5).then_some(loser)
    }

    /// Could `a` declare war on `b`?
    pub fn war_room(&self, a: ActorId, b: ActorId) -> Result<(), &'static str> {
        if a == b {
            return Err("no polity to fight");
        }
        if !self.alive(b) {
            return Err("the polity has fallen");
        }
        if self.at_war(a, b) {
            return Err("already at war");
        }
        if self.lord_of(a).is_some_and(|lord| lord != b) {
            return Err("a vassal fights only its lord");
        }
        Ok(())
    }

    /// The price of declaring war on `b` in legitimacy: the base, times the breach factor
    /// when a treaty with `b` is in force — and **a fresh treaty costs more to break**
    /// (owner's rule on #293: a price, not a prohibition): `treaty.fresh_breach_factor` times
    /// more on the day it was signed, falling in a line to the plain breach over
    /// `treaty.fresh_years`. A treaty of the scenario's facts is not fresh.
    pub fn war_price(&self, a: ActorId, b: ActorId, params: &Params) -> f64 {
        let Some(t) = self.treaty(a, b) else { return params.legitimacy_cost_declare_war };
        let fresh = match t.since {
            Some(since) if params.treaty_fresh_ticks > 0 => {
                let left = 1.0 - f64::from(self.tick.0.saturating_sub(since.0)) / f64::from(params.treaty_fresh_ticks);
                1.0 + (params.treaty_fresh_breach_factor - 1.0) * left.max(0.0)
            }
            _ => 1.0,
        };
        params.legitimacy_cost_declare_war * params.treaty_breach_factor * fresh
    }

    /// Would `b` accept peace from `a`? By the state of the war (§19) and its declarer
    /// (E3 (б), owner's rule on #291): a receiver that is losing accepts always; **the
    /// declarer** accepts only at the threshold of the relation, or when its war goal
    /// against the other is gone (put aside, or never there), or when it is exhausted — its
    /// army's pay short; a winner that did not declare accepts at the threshold; **the
    /// attacked** accepts a status quo always. With «nothing taken — both accept» the
    /// attacked ended every war of the first bots at once, before a battle (829 peaces on
    /// 695 declarations); with «the declarer at the threshold» alone an aggressor without a
    /// chance would fight for ever — wars ended more often by exhaustion than by conquest.
    pub fn peace_room(&self, a: ActorId, b: ActorId, params: &Params) -> Result<(), &'static str> {
        if a == b || !self.at_war(a, b) {
            return Err("not at war");
        }
        let forced = self.forced_to_peace(a, b, params);
        let at_threshold = self.relation(a, b) >= params.treaty_accept_at_relation;
        if peace_rule(forced == Some(b), self.war_cause(b, a, params), at_threshold, self.exhausted(b)) {
            return Ok(());
        }
        Err("the other side wants its war: the relation below the threshold, its cause alive (its claim unmet, or provinces lost with the better odds), its army paid")
    }

    /// Whether `b` has a cause to go on with its war against `a` (plan §28.8, §28.9): the
    /// claim its declaration stated is not met yet (`claim_met`), or it lost provinces in the
    /// war with the better odds — the loser that is not forced (`forced_to_peace`) would take
    /// them back. Read off the record alone, so the bots read this very function.
    pub fn war_cause(&self, b: ActorId, a: ActorId, params: &Params) -> bool {
        let claim = self.wars.get(&pair(a, b)).filter(|w| w.by == Some(b)).and_then(|w| w.aim);
        claim.is_some_and(|aim| !self.claim_met(b, a, aim)) || (self.losing_side(a, b) == Some(b) && self.forced_to_peace(a, b, params) != Some(b))
    }

    /// Whether the claim `aim` of `claimant` in its war with `other` is met (plan §28.9): the
    /// province is the claimant's; the polity would submit at a peace now — down to half the
    /// land it held when the war began (`make_peace_between`) — or is the claimant's vassal
    /// already, or has fallen.
    pub fn claim_met(&self, claimant: ActorId, other: ActorId, aim: WarAim) -> bool {
        match aim {
            WarAim::Province(q) => match self.provinces.get(&q) {
                Some(pr) => pr.owner == claimant,
                None => true,
            },
            WarAim::Polity(x) => {
                if !self.alive(x) || self.lord_of(x) == Some(claimant) {
                    return true;
                }
                let key = pair(claimant, other);
                self.wars.get(&key).is_some_and(|w| {
                    let held = if x == key.0 { w.held.0 } else { w.held.1 };
                    (self.actors[&x].provinces.len() as u32) * 2 <= held
                })
            }
        }
    }

    /// The legitimacy a province ceded at a peace costs its loser (`make_peace_between`):
    /// the bots price the land at risk by it (plan §28.9).
    pub fn ceded_province_cost(params: &Params) -> f64 {
        params.legitimacy_cost_province_lost
    }

    /// Whether a polity's army is short of its pay this tick — exhausted, by the rule of the
    /// peace.
    pub fn exhausted(&self, a: ActorId) -> bool {
        self.actors[&a].paid_share_ppm < 1_000_000
    }

    /// Would `b` accept an alliance from `a`?
    pub fn alliance_room(&self, a: ActorId, b: ActorId, params: &Params) -> Result<(), &'static str> {
        if a == b {
            return Err("no polity to ally with");
        }
        if !self.alive(b) {
            return Err("the polity has fallen");
        }
        if self.at_war(a, b) {
            return Err("at war");
        }
        if self.allied(a, b) {
            return Err("already allied");
        }
        if self.lord_of(a).is_some() || self.lord_of(b).is_some() {
            return Err("a vassal makes no alliance");
        }
        if self.allies_of(a).iter().any(|x| self.at_war(*x, b)) || self.allies_of(b).iter().any(|x| self.at_war(*x, a)) {
            return Err("at war with the other's ally");
        }
        if self.relation(a, b) < params.treaty_accept_at_relation {
            return Err("the relation is below the threshold");
        }
        Ok(())
    }

    /// An embassy's gifts: a share of a year's revenue.
    pub fn embassy_cost(&self, a: ActorId, params: &Params) -> i64 {
        self.yearly_income(a, params) * params.embassy_cost_share_ppm / 1_000_000
    }

    /// A gift to another polity: the subsidy of E0 Д2, a share of the giver's revenue.
    pub fn gift_size(&self, a: ActorId, params: &Params) -> i64 {
        self.yearly_income(a, params) * params.bribe_share_ppm / 1_000_000
    }

    /// The eldest living adult of a house with no living spouse.
    pub fn unmarried_adult(&self, h: HouseId, params: &Params) -> Option<PersonId> {
        self.members(h)
            .into_iter()
            .filter(|p| self.is_adult(*p, params) && !self.persons[p].spouse.is_some_and(|s| self.person_alive(s)))
            .min_by_key(|p| (self.persons[p].born, *p))
    }

    /// The dowry a house would give: a share of its treasury.
    pub fn dowry(&self, h: HouseId, params: &Params) -> i64 {
        self.house_treasury(h) * params.dowry_share_ppm / 1_000_000
    }

    /// Could house `h` marry into house `g`? The pair that would marry.
    pub fn marriage_room(&self, h: HouseId, g: HouseId, params: &Params) -> Result<(PersonId, PersonId), &'static str> {
        if h == g {
            return Err("a house does not marry itself");
        }
        if !self.houses.contains_key(&g) {
            return Err("no such house");
        }
        let p = self.unmarried_adult(h, params).ok_or("no unmarried adult to marry")?;
        let q = self.unmarried_adult(g, params).ok_or("the other house has no unmarried adult")?;
        let (a, b) = (self.houses[&h].polity, self.houses[&g].polity);
        if a != b {
            if self.at_war(a, b) {
                return Err("at war");
            }
            if self.relation(a, b) < params.treaty_accept_at_relation {
                return Err("the relation is below the threshold");
            }
        }
        Ok((p, q))
    }

    /// The house a caller speaks for: a ruler for its ruling house, a house for itself.
    fn house_of_caller(&self, who: ActorRef) -> Result<HouseId, VerbError> {
        match who {
            ActorRef::Ruler(a) => {
                if !self.alive(a) {
                    return Err(VerbError::Fallen(a));
                }
                Ok(self.persons[&self.actors[&a].ruler].house)
            }
            ActorRef::House(h) => self.houses.contains_key(&h).then_some(h).ok_or(VerbError::NotARuler(who)),
        }
    }

    /// What a diplomatic verb offers `who` (the offers and the application share it).
    pub fn diplomacy_offer(&self, verb: VerbId, who: ActorRef, params: &Params) -> DiplomacyOffer {
        let threshold = Some(params.treaty_accept_at_relation.round() as i64);
        let offer = |available: bool, cost_kg: i64, cost_legitimacy: i64, why: Option<String>, threshold: Option<i64>| DiplomacyOffer { available, cost_kg, cost_legitimacy, why, threshold };
        let closed = |why: &str| offer(false, 0, 0, Some(why.to_string()), None);
        let others = |a: ActorId| -> Vec<ActorId> { self.actors.keys().copied().filter(|b| *b != a && self.alive(*b)).collect() };
        let best = |a: ActorId, cands: &[ActorId]| -> f64 { cands.iter().map(|b| self.relation(a, *b)).fold(0.0, f64::max) };
        match (verb, who) {
            (VerbId::ProposeMarriage, who) => {
                let h = match self.house_of_caller(who) {
                    Ok(h) => h,
                    Err(_) => return closed("the polity has fallen"),
                };
                let dowry = self.dowry(h, params);
                if self.unmarried_adult(h, params).is_none() {
                    return offer(false, dowry, 0, Some("no unmarried adult to marry".to_string()), threshold);
                }
                let houses: Vec<HouseId> = self.houses.keys().copied().filter(|g| *g != h).collect();
                if houses.iter().any(|g| self.marriage_room(h, *g, params).is_ok()) {
                    return offer(true, dowry, 0, None, threshold);
                }
                let a = self.houses[&h].polity;
                let best = houses.iter().map(|g| self.houses[g].polity).filter(|b| *b != a).map(|b| self.relation(a, b)).fold(0.0, f64::max);
                offer(false, dowry, 0, Some(format!("no house would accept: best relation {best:.0}, threshold {}", params.treaty_accept_at_relation)), threshold)
            }
            (_, ActorRef::House(_)) => closed("a house does not rule the polity"),
            (VerbId::DeclareWar, ActorRef::Ruler(a)) => {
                let cost = params.legitimacy_cost_declare_war.round() as i64;
                if others(a).iter().any(|b| self.war_room(a, *b).is_ok()) {
                    return offer(true, 0, cost, None, None);
                }
                let why = if self.lord_of(a).is_some() { "a vassal fights only its lord" } else { "nobody to fight" };
                offer(false, 0, cost, Some(why.to_string()), None)
            }
            (VerbId::MakePeace, ActorRef::Ruler(a)) => {
                let enemies: Vec<ActorId> = others(a).into_iter().filter(|b| self.at_war(a, *b)).collect();
                if enemies.is_empty() {
                    return closed("not at war");
                }
                if enemies.iter().any(|b| self.peace_room(a, *b, params).is_ok()) {
                    return offer(true, 0, 0, None, threshold);
                }
                offer(false, 0, 0, Some(format!("the winner wants more: best relation {:.0}, threshold {}", best(a, &enemies), params.treaty_accept_at_relation)), threshold)
            }
            (VerbId::ProposeAlliance, ActorRef::Ruler(a)) => {
                let cost = self.embassy_cost(a, params);
                if cost <= 0 {
                    return offer(false, cost, 0, Some("no revenue to pay from".to_string()), threshold);
                }
                if self.treasury(a) < cost {
                    return offer(false, cost, 0, Some("cannot pay".to_string()), threshold);
                }
                if self.lord_of(a).is_some() {
                    return offer(false, cost, 0, Some("a vassal makes no alliance".to_string()), threshold);
                }
                let cands = others(a);
                if cands.iter().any(|b| self.alliance_room(a, *b, params).is_ok()) {
                    return offer(true, cost, 0, None, threshold);
                }
                offer(false, cost, 0, Some(format!("no polity would accept: best relation {:.0}, threshold {}", best(a, &cands), params.treaty_accept_at_relation)), threshold)
            }
            (VerbId::SendEmbassy | VerbId::Bribe, ActorRef::Ruler(a)) => {
                let cost = if verb == VerbId::SendEmbassy { self.embassy_cost(a, params) } else { self.gift_size(a, params) };
                if cost <= 0 {
                    return offer(false, cost, 0, Some("no revenue to pay from".to_string()), None);
                }
                if self.treasury(a) < cost {
                    return offer(false, cost, 0, Some("cannot pay".to_string()), None);
                }
                if others(a).is_empty() {
                    return offer(false, cost, 0, Some("nobody to send to".to_string()), None);
                }
                offer(true, cost, 0, None, None)
            }
            _ => closed("not a verb of diplomacy"),
        }
    }

    /// Apply a diplomatic verb (the systems compute the effect; acceptance is a rule of the
    /// state, not another agent's choice — but a marriage is offered to the other house,
    /// which decides at the next turn, E3 (в), `opportunity.rs`).
    pub(crate) fn apply_diplomacy_verb(&mut self, call: &VerbCall, params: &Params) -> Result<(), VerbError> {
        let ledger = |_: WorldError| VerbError::NotAvailable(call.verb, "ledger");
        if call.verb == VerbId::ProposeMarriage {
            let h = self.house_of_caller(call.actor)?;
            let Target::House(g) = call.target else { return Err(VerbError::BadTarget(call.verb, "a house to marry into")) };
            let (p, q) = self.marriage_room(h, g, params).map_err(|why| VerbError::NotAvailable(call.verb, why))?;
            self.open_opportunity(ActorRef::House(g), crate::opportunity::Chance::Marriage { from: h, a: p, b: q, dowry_kg: self.dowry(h, params) });
            return Ok(());
        }
        let ActorRef::Ruler(a) = call.actor else { return Err(VerbError::NotARuler(call.actor)) };
        if !self.alive(a) {
            return Err(VerbError::Fallen(a));
        }
        // A declaration names its claim (plan §28.9): a province — the war is on its owner —
        // or a polity, whose submission it claims. The other verbs take a polity.
        let (b, aim) = match (call.verb, call.target) {
            (VerbId::DeclareWar, Target::Province(q)) => match self.provinces.get(&q) {
                Some(pr) => (pr.owner, Some(WarAim::Province(q))),
                None => return Err(VerbError::BadTarget(call.verb, "no such province")),
            },
            (_, Target::Actor(b)) => (b, Some(WarAim::Polity(b))),
            _ => return Err(VerbError::BadTarget(call.verb, "a polity")),
        };
        if !self.actors.contains_key(&b) {
            return Err(VerbError::BadTarget(call.verb, "no such polity"));
        }
        let not_available = |why: &'static str| VerbError::NotAvailable(call.verb, why);
        match call.verb {
            VerbId::DeclareWar => {
                self.war_room(a, b).map_err(not_available)?;
                // A claim met before the war is no claim (plan §28.9): a lord claiming the
                // submission of its own vassal declared, took the vassal's instant peace with no
                // cause left, and declared again the next tick (129 of 153 instant peaces).
                if aim.is_some_and(|aim| self.claim_met(a, b, aim)) {
                    return Err(not_available("the claim is met already: claim a province"));
                }
                let price = self.war_price(a, b, params);
                if let Some(t) = self.treaty(a, b).copied() {
                    self.treaties.remove(&pair(a, b));
                    let (x, y) = pair(a, b);
                    self.emit(Event::TreatyBroken { a: x, b: y, kind: t.kind });
                    for ally in self.allies_of(b) {
                        self.push_relation(a, ally, -100.0);
                    }
                }
                self.pay_legitimacy(a, price);
                self.open_war(a, b, aim).map_err(ledger)
            }
            VerbId::MakePeace => {
                self.peace_room(a, b, params).map_err(not_available)?;
                self.make_peace_between(a, b, params).map_err(ledger)
            }
            VerbId::ProposeAlliance => {
                if a == b {
                    return Err(VerbError::BadTarget(call.verb, "no polity to ally with"));
                }
                let cost = self.embassy_cost(a, params);
                if cost <= 0 {
                    return Err(not_available("no revenue to pay from"));
                }
                if self.treasury(a) < cost {
                    return Err(not_available("cannot pay"));
                }
                if self.lord_of(a).is_some() {
                    return Err(not_available("a vassal makes no alliance"));
                }
                self.post(Account::Treasury(a), Source::Verb, -cost * 1000).map_err(ledger)?;
                match self.alliance_room(a, b, params) {
                    Ok(()) => {
                        let until = self.bind(a, b, TreatyKind::Alliance, params).map_err(ledger)?;
                        let (x, y) = pair(a, b);
                        self.emit(Event::Alliance { a: x, b: y, until });
                    }
                    Err(_) => self.emit(Event::Refused { actor: a, verb: call.verb, target: b }),
                }
                Ok(())
            }
            VerbId::SendEmbassy => {
                if a == b || !self.alive(b) {
                    return Err(VerbError::BadTarget(call.verb, "another living polity"));
                }
                let cost = self.embassy_cost(a, params);
                if cost <= 0 {
                    return Err(not_available("no revenue to pay from"));
                }
                if self.treasury(a) < cost {
                    return Err(not_available("cannot pay"));
                }
                self.post(Account::Treasury(a), Source::Verb, -cost * 1000).map_err(ledger)?;
                self.push_relation(a, b, params.embassy_gain);
                self.emit(Event::Embassy { from: a, to: b, kg: cost as u64 });
                Ok(())
            }
            VerbId::Bribe => {
                if a == b || !self.alive(b) {
                    return Err(VerbError::BadTarget(call.verb, "another living polity"));
                }
                let gift = self.gift_size(a, params);
                if gift <= 0 {
                    return Err(not_available("no revenue to pay from"));
                }
                if self.treasury(a) < gift {
                    return Err(not_available("cannot pay"));
                }
                self.post(Account::Treasury(a), Source::Gift, -gift * 1000).map_err(ledger)?;
                self.post(Account::Treasury(b), Source::Gift, gift * 1000).map_err(ledger)?;
                let push = self.gift_push(a, b, params);
                self.push_relation(a, b, push);
                self.emit(Event::Gift { from: a, to: b, kg: gift as u64 });
                Ok(())
            }
            _ => Err(not_available("not a verb of diplomacy")),
        }
    }

    /// The peace itself: the war and the wars that were its obligations close, the pair is
    /// bound by a treaty of peace for the term, and the loser is settled — submission at
    /// half its land, release of a vassal that beat its lord.
    pub(crate) fn make_peace_between(&mut self, a: ActorId, b: ActorId, params: &Params) -> Result<(), WorldError> {
        let key = pair(a, b);
        let Some(war) = self.wars.remove(&key) else { return Ok(()) };
        self.emit(Event::Peace { a: key.0, b: key.1 });
        let dependents: Vec<(ActorId, ActorId)> = self.wars.iter().filter(|(_, w)| w.of == Some(key)).map(|(k, _)| *k).collect();
        for k in dependents {
            self.wars.remove(&k);
            self.emit(Event::Peace { a: k.0, b: k.1 });
        }
        let loser = match war.taken.0.cmp(&war.taken.1) {
            std::cmp::Ordering::Greater => Some(key.1),
            std::cmp::Ordering::Less => Some(key.0),
            std::cmp::Ordering::Equal => None,
        };
        if let Some(loser) = loser {
            let winner = if loser == key.0 { key.1 } else { key.0 };
            // A lost war costs the ruler who lost it (owner's rule on #293): the provinces
            // ceded on balance, each at `legitimacy.cost_province_lost`.
            let ceded = war.taken.0.abs_diff(war.taken.1);
            self.pay_legitimacy(loser, f64::from(ceded) * params.legitimacy_cost_province_lost);
            self.emit(Event::Defeat { actor: loser, by: winner, provinces: ceded });
            if self.lord_of(winner) == Some(loser) {
                self.actors.get_mut(&winner).unwrap().vassal_of = None;
                self.emit(Event::Released { vassal: winner, lord: loser });
            } else if self.lord_of(loser).is_none() {
                let held = if loser == key.0 { war.held.0 } else { war.held.1 };
                if (self.actors[&loser].provinces.len() as u32) * 2 <= held {
                    self.actors.get_mut(&loser).unwrap().vassal_of = Some(winner);
                    self.emit(Event::Submission { vassal: loser, lord: winner });
                }
            }
        }
        self.bind(key.0, key.1, TreatyKind::Peace, params)?;
        Ok(())
    }

    /// The diplomacy phase: treaties at their term end, relations move to their norms.
    pub(crate) fn phase_diplomacy(&mut self, params: &Params) -> Result<(), WorldError> {
        let expired: Vec<((ActorId, ActorId), TreatyKind)> =
            self.treaties.iter().filter(|(_, t)| self.tick >= t.until).map(|(k, t)| (*k, t.kind)).collect();
        for ((a, b), kind) in expired {
            self.treaties.remove(&(a, b));
            self.emit(Event::TreatyExpired { a, b, kind });
        }
        let pairs: Vec<(ActorId, ActorId)> = self.relations.keys().copied().collect();
        for (a, b) in pairs {
            if !self.alive(a) || !self.alive(b) {
                continue;
            }
            let norm = self.relation_norm(a, b, params);
            let r = self.relations[&(a, b)];
            self.relations.insert((a, b), relation_step(r, norm, params));
        }
        Ok(())
    }
}
