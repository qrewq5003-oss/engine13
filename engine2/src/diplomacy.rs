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
//! the weights of threat — strength, intent, distance — are E3's). A peace in force shifts
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
//! **Phase** (after the house and the throne, before the people): treaties that have run
//! out end; every relation of two living polities moves `relation.rate_per_tick` of the way
//! to its norm.

use crate::event::Event;
use crate::ids::ActorId;
use crate::params::Params;
use crate::time::Tick;
use crate::world::{Treaty, TreatyKind, World, WorldError};

pub(crate) use crate::army::pair;

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
        norm.clamp(0.0, 100.0)
    }

    /// A one-off push of a relation, saturating towards the end of the scale it moves to:
    /// `delta × (100 − R) / 100` upwards, `delta × R / 100` downwards — so no push pins the
    /// relation to an edge.
    pub fn push_relation(&mut self, a: ActorId, b: ActorId, delta: f64) {
        if a == b {
            return;
        }
        let r = self.relation(a, b);
        let moved = if delta >= 0.0 { delta * (100.0 - r) / 100.0 } else { delta * r / 100.0 };
        self.relations.insert(pair(a, b), (r + moved).clamp(0.0, 100.0));
    }

    /// Bind a pair by a treaty running `treaty.term_years` from now; a new treaty of the
    /// pair replaces the old one.
    pub fn bind(&mut self, a: ActorId, b: ActorId, kind: TreatyKind, params: &Params) -> Result<Tick, WorldError> {
        if a == b {
            return Err(WorldError::RelationWithSelf(a));
        }
        let until = Tick(self.tick.0 + params.treaty_term_ticks);
        self.treaties.insert(pair(a, b), Treaty { kind, until });
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
            self.relations.insert((a, b), r + params.relation_rate_per_tick * (norm - r));
        }
        Ok(())
    }
}
