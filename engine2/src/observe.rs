//! engine2 through the sentinel's eyes (plan §8): `Observed` for `Game`, on the world's
//! string ids. The mapping is the only place where engine2 types meet sentinel types.

use sentinel::{ObsEvent, ObsId, Observation, Observed, Rating, Stock};

use crate::event::Event;
use crate::ledger::Account;
use crate::sim::Game;

impl Game {
    fn actor_id(&self, id: crate::ids::ActorId) -> ObsId {
        ObsId(self.world().names.actors[id.index()].0.clone())
    }

    fn province_id(&self, id: crate::ids::ProvinceId) -> ObsId {
        ObsId(self.world().names.provinces[id.index()].0.clone())
    }

    fn account(&self, a: Account) -> (ObsId, String) {
        match a {
            Account::Population(p) => (self.province_id(p), "population".into()),
            Account::Soldiers(u) => (ObsId(format!("unit{}", u.0)), "soldiers".into()),
            Account::Treasury(a) => (self.actor_id(a), "treasury".into()),
            Account::HouseTreasury(h) => (ObsId(format!("house{}", h.0)), "treasury".into()),
        }
    }
}

impl Observed for Game {
    fn observe(&self) -> Observation {
        let w = self.world();
        let alive = w.actors.keys().map(|a| self.actor_id(*a)).collect();
        let owners = w.provinces.iter().map(|(p, pr)| (self.province_id(*p), self.actor_id(pr.owner))).collect();
        let ratings = w
            .actors
            .iter()
            .map(|(a, actor)| Rating { who: self.actor_id(*a), name: "legitimacy".into(), value: actor.legitimacy, lo: 0.0, hi: 100.0 })
            .collect();
        let stocks = w
            .ledger
            .accounts()
            .map(|acc| {
                let (who, name) = self.account(acc);
                Stock { who, name, value: w.ledger.stock(acc).unwrap_or(0) }
            })
            .collect();
        Observation { tick: w.tick.0, alive, owners, ratings, stocks }
    }

    fn events_of_last_tick(&self) -> Vec<ObsEvent> {
        self.last_events()
            .iter()
            .map(|s| match &s.event {
                Event::TickPlayed => ObsEvent::TickPlayed,
                Event::Verb { actor, verb, .. } => ObsEvent::Verb {
                    actor: match actor {
                        crate::verbs::ActorRef::Ruler(a) => self.actor_id(*a),
                        crate::verbs::ActorRef::House(h) => ObsId(format!("house{}", h.0)),
                    },
                    verb: verb.key().into(),
                },
                Event::Battle { province, outcome, .. } => ObsEvent::Battle {
                    place: self.province_id(*province),
                    a: ObsId("attacker".into()),
                    b: ObsId("defender".into()),
                    winner: match outcome {
                        crate::event::Outcome::AttackerWon => Some(ObsId("attacker".into())),
                        crate::event::Outcome::DefenderWon => Some(ObsId("defender".into())),
                        crate::event::Outcome::Drawn => None,
                    },
                },
                Event::ProvinceChangedHands { province, from, to, .. } => {
                    ObsEvent::Transfer { place: self.province_id(*province), from: self.actor_id(*from), to: self.actor_id(*to) }
                }
                Event::War { a, b } => ObsEvent::War(self.actor_id(*a), self.actor_id(*b)),
                Event::Peace { a, b } => ObsEvent::Peace(self.actor_id(*a), self.actor_id(*b)),
                Event::RulerDied { .. } => ObsEvent::Other("RulerDied".into()),
                Event::ActorDied { actor } => ObsEvent::Died(self.actor_id(*actor)),
                Event::BoundHit { account, source } => {
                    let (who, stock) = self.account(*account);
                    ObsEvent::BoundHit { who, stock, source: format!("{source:?}") }
                }
            })
            .collect()
    }

    /// E1: no system registered yet. Each system of E2 adds the kinds it can produce; a
    /// declared kind that never occurs across the matrix is the sentinel's `Never`.
    fn possible_kinds(&self) -> Vec<String> {
        Vec::new()
    }
}
