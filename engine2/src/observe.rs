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

    fn unit_id(&self, id: crate::ids::UnitId) -> ObsId {
        ObsId(format!("u{}", id.0))
    }

    fn house_id(&self, id: crate::ids::HouseId) -> ObsId {
        ObsId(self.world().names.houses[id.index()].0.clone())
    }

    fn account(&self, a: Account) -> (ObsId, String) {
        match a {
            Account::Population(p) => (self.province_id(p), "population".into()),
            Account::Soldiers(u) => (self.unit_id(u), "soldiers".into()),
            Account::Treasury(a) => (self.actor_id(a), "treasury".into()),
            Account::Debt(a) => (self.actor_id(a), "debt".into()),
            Account::HouseTreasury(h) => (self.house_id(h), "treasury".into()),
        }
    }
}

impl Observed for Game {
    fn observe(&self) -> Observation {
        let w = self.world();
        let alive = w.actors.keys().filter(|a| w.alive(**a)).map(|a| self.actor_id(*a)).collect();
        let owners = w.provinces.iter().map(|(p, pr)| (self.province_id(*p), self.actor_id(pr.owner))).collect();
        // Only the living are rated: a fallen polity keeps its record, and its legitimacy
        // frozen at its last value is no rating of the world (since E3 (б) polities fall
        // by conquest; the full run flagged a fallen one as pinned to a bound).
        let ratings = w
            .actors
            .iter()
            .filter(|(a, _)| w.alive(**a))
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
                        crate::verbs::ActorRef::House(h) => self.house_id(*h),
                    },
                    verb: verb.key().into(),
                },
                Event::Battle { province, attacker, defender, outcome, .. } => ObsEvent::Battle {
                    place: self.province_id(*province),
                    a: self.actor_id(*attacker),
                    b: self.actor_id(*defender),
                    winner: match outcome {
                        crate::event::Outcome::AttackerWon => Some(self.actor_id(*attacker)),
                        crate::event::Outcome::DefenderWon => Some(self.actor_id(*defender)),
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
                Event::UnitMoved { .. } => ObsEvent::Other("UnitMoved".into()),
                Event::NoPath { .. } => ObsEvent::Other("NoPath".into()),
                Event::Attrition { .. } => ObsEvent::Other("Attrition".into()),
                Event::Famine { province, .. } => ObsEvent::Other(format!("Famine@{}", self.province_id(*province).0)),
                Event::Plague { .. } => ObsEvent::Other("Plague".into()),
                Event::Migration { .. } => ObsEvent::Other("Migration".into()),
                Event::PayShortfall { actor, .. } => ObsEvent::Other(format!("PayShortfall@{}", self.actor_id(*actor).0)),
                Event::Loan { .. } => ObsEvent::Other("Loan".into()),
                Event::Retreat { .. } => ObsEvent::Other("Retreat".into()),
                Event::UnitDestroyed { .. } => ObsEvent::Other("UnitDestroyed".into()),
                Event::CapitalMoved { .. } => ObsEvent::Other("CapitalMoved".into()),
                Event::Ruin { province, .. } => ObsEvent::Other(format!("Ruin@{}", self.province_id(*province).0)),
                Event::Recruited { .. } => ObsEvent::Other("Recruited".into()),
                Event::Disbanded { .. } => ObsEvent::Other("Disbanded".into()),
                Event::Hired { .. } => ObsEvent::Other("Hired".into()),
                Event::PersonDied { .. } => ObsEvent::Other("PersonDied".into()),
                Event::PersonBorn { .. } => ObsEvent::Other("PersonBorn".into()),
                Event::HouseExtinct { house } => ObsEvent::Other(format!("HouseExtinct@{}", self.house_id(*house).0)),
                Event::NewDynasty { actor, .. } => ObsEvent::Other(format!("NewDynasty@{}", self.actor_id(*actor).0)),
                Event::Succession { actor, .. } => ObsEvent::Other(format!("Succession@{}", self.actor_id(*actor).0)),
                Event::Regency { .. } => ObsEvent::Other("Regency".into()),
                Event::RegencyEnded { .. } => ObsEvent::Other("RegencyEnded".into()),
                Event::GovernorChanged { .. } => ObsEvent::Other("GovernorChanged".into()),
                Event::TaxRateChanged { actor, .. } => ObsEvent::Other(format!("TaxRateChanged@{}", self.actor_id(*actor).0)),
                Event::OrderEnforced { actor, .. } => ObsEvent::Other(format!("OrderEnforced@{}", self.actor_id(*actor).0)),
                Event::Default { actor, .. } => ObsEvent::Other(format!("Default@{}", self.actor_id(*actor).0)),
                Event::CreditRestored { .. } => ObsEvent::Other("CreditRestored".into()),
                Event::TreatyExpired { a, b, .. } => ObsEvent::Other(format!("TreatyExpired@{}-{}", self.actor_id(*a).0, self.actor_id(*b).0)),
                Event::Released { vassal, .. } => ObsEvent::Other(format!("Released@{}", self.actor_id(*vassal).0)),
                Event::TributeShort { vassal, .. } => ObsEvent::Other(format!("TributeShort@{}", self.actor_id(*vassal).0)),
                Event::Alliance { a, b, .. } => ObsEvent::Other(format!("Alliance@{}-{}", self.actor_id(*a).0, self.actor_id(*b).0)),
                Event::Refused { actor, verb, .. } => ObsEvent::Other(format!("Refused@{}:{}", self.actor_id(*actor).0, verb.key())),
                Event::TreatyBroken { a, b, .. } => ObsEvent::Other(format!("TreatyBroken@{}-{}", self.actor_id(*a).0, self.actor_id(*b).0)),
                Event::Embassy { from, to, .. } => ObsEvent::Other(format!("Embassy@{}-{}", self.actor_id(*from).0, self.actor_id(*to).0)),
                Event::Gift { from, to, .. } => ObsEvent::Other(format!("Gift@{}-{}", self.actor_id(*from).0, self.actor_id(*to).0)),
                Event::Marriage { .. } => ObsEvent::Other("Marriage".into()),
                Event::Submission { vassal, .. } => ObsEvent::Other(format!("Submission@{}", self.actor_id(*vassal).0)),
                // Plain kinds: the sentinel's `Never` is read over them in the full matrix.
                Event::Converted { .. } => ObsEvent::Other("Converted".into()),
                Event::Assimilated { province, .. } => ObsEvent::Other(format!("Assimilated@{}", self.province_id(*province).0)),
                Event::Adopted { .. } => ObsEvent::Other("Adopted".into()),
                Event::HouseConverted { house, .. } => ObsEvent::Other(format!("HouseConverted@{}", self.house_id(*house).0)),
                Event::HousePayShortfall { house, .. } => ObsEvent::Other(format!("HousePayShortfall@{}", self.house_id(*house).0)),
                // Plain kinds (E2 step 9): `Never` is read over them by the horizon.
                Event::Secession { .. } => ObsEvent::Other("Secession".into()),
                Event::Revolt { .. } => ObsEvent::Other("Revolt".into()),
                Event::RevoltEnded { province, .. } => ObsEvent::Other(format!("RevoltEnded@{}", self.province_id(*province).0)),
                Event::ActorBorn { actor, .. } => ObsEvent::Born(self.actor_id(*actor)),
                Event::GoalSet { actor, .. } => ObsEvent::Other(format!("GoalSet@{}", self.actor_id(*actor).0)),
                // Plain kinds (E3): `Never` is read over them by the horizon.
                Event::GoalReached { .. } => ObsEvent::Other("GoalReached".into()),
                Event::GoalDropped { .. } => ObsEvent::Other("GoalDropped".into()),
                Event::Preempted { verb, .. } => ObsEvent::Other(format!("Preempted:{}", verb.key())),
            })
            .collect()
    }

    /// Kinds the systems so far can produce **without anyone ordering anything**. Plague,
    /// where the world holds it (E0 П4). Not yet: movement and attrition need an order (E3
    /// agents); famine needs something to push a province over its capacity (foraging,
    /// capacity shocks — with war); migration happens only with a fill gap. A declared kind
    /// that never occurs across the matrix is the sentinel's `Never`; registering kinds that
    /// nothing can cause yet would flag the absence of later systems, not a defect.
    fn possible_kinds(&self, horizon_ticks: u32) -> Vec<String> {
        let mut out = vec!["PersonDied".to_string(), "PersonBorn".to_string()];
        let w = self.world();
        if w.diseases.contains(&crate::world::Disease::Plague) {
            out.push("Plague".to_string());
        }
        // A conversion (E2 step 8): declared when some province has a channel at the start
        // and the curve at that channel's rate turns the majority inside the horizon; the
        // slow drift of the neighbourhood takes two centuries and is declared only then.
        let p = self.params();
        let turns_within = |rate_ppm: i64| {
            let mut share = p.conversion_seed_ppm;
            for _ in 0..horizon_ticks {
                share = (share + rate_ppm * share / 1_000_000 * (1_000_000 - share) / 1_000_000).min(1_000_000);
                if share >= 500_000 {
                    return true;
                }
            }
            false
        };
        if w.provinces.keys().any(|q| w.conversion_channel(*q, p).is_some_and(|(_, _, rate)| turns_within(rate))) {
            out.push("Converted".to_string());
        }
        // An adoption: declared when some polity has a channel to an institution it lacks
        // and the horizon expects at least half an adoption of it.
        let expected = f64::from(horizon_ticks) * p.institution_adoption_per_tick;
        if expected >= 0.5 && w.actors.keys().any(|a| crate::faith::SPREADING.iter().any(|i| w.adoption_source(*a, *i).is_some())) {
            out.push("Adopted".to_string());
        }
        // A rising (E2 step 9): declared when the horizon expects three or more of its kind
        // over the provinces that can rise — a chance of nineteen in twenty that it happens
        // in one run, so the flag names a defect and not the luck of a few runs. `Born`
        // for the polity it bears, `Revolt` for a province rising alone, `Secession` for
        // one with a house to carry it (`collapse.rs`).
        let (mut alone, mut carried) = (0.0, 0.0);
        for q in w.provinces.keys() {
            let expected = w.revolt_chance_per_tick(*q, p) * f64::from(horizon_ticks);
            if w.secession_carrier(*q).is_some() {
                carried += expected;
            } else {
                alone += expected;
            }
        }
        if alone >= 3.0 {
            out.push("Revolt".to_string());
        }
        if carried >= 3.0 {
            out.push("Born".to_string());
            out.push("Secession".to_string());
        }
        out
    }
}
