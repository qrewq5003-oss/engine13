//! The `Simulation` contract (plan §5) and its implementation on the E1 world.
//!
//! The shell talks to this trait only: create from a scenario, tick with the player's
//! verbs, view for the UI, snapshot for the chronicler, save and load. The sentinel reads
//! the game through `sentinel::Observed` (`observe.rs`).
//!
//! A load checks that the save belongs to the scenario it is loaded with — scenario id and
//! the hash of the compiled facts — so a run of one scenario cannot continue as another
//! (AGENTS §8, owner's review of PR #278).

use crate::economy::VerbError;
use crate::event::{Event, Stamped};
use crate::params::Params;
use crate::world::Institution;
use crate::save::{self, SaveError};
use crate::scenario::{Display, ScenarioData, SchemaError};
use crate::verbs::{ActorRef, VerbCall, VerbId, VerbOffer};
use crate::view::{self, ChronicleSnapshot, WorldView};
use crate::world::{TickReport, World, WorldError};

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("{0}")]
    Schema(#[from] SchemaError),
    #[error("{0}")]
    World(#[from] WorldError),
    #[error("{0}")]
    Save(#[from] SaveError),
    #[error("save belongs to scenario '{save}' (facts {save_hash:#x}), not to '{scenario}' (facts {scenario_hash:#x})")]
    WrongScenario { save: String, save_hash: u64, scenario: String, scenario_hash: u64 },
    #[error("verb {0:?} is not enabled by the scenario")]
    VerbNotEnabled(VerbId),
    #[error("{0}")]
    Verb(#[from] VerbError),
}

pub trait Simulation: Sized {
    fn new(scenario: &ScenarioData, params: Params, seed: u64) -> Result<Self, SimError>;
    /// Play one tick with the player's calls. Returns exactly what the tick did. A call of
    /// a verb the scenario does not enable is an error, and nothing is played.
    fn tick(&mut self, player: &[VerbCall]) -> Result<TickReport, SimError>;
    /// The verbs on offer to `who`, computed by the one function agents use too.
    fn verbs(&self, who: ActorRef) -> Vec<VerbOffer>;
    fn view(&self) -> WorldView;
    fn chronicle(&self) -> ChronicleSnapshot;
    fn to_save(&self) -> Result<String, SimError>;
    /// Restore a run: the scenario and params are the shell's, the world is the save's.
    fn from_save(scenario: &ScenarioData, params: Params, save: &str) -> Result<Self, SimError>;
}

/// A run: the world plus what the scenario contributed besides facts.
#[derive(Debug)]
pub struct Game {
    world: World,
    display: Display,
    params: Params,
    /// Events of the last played tick, for the chronicle snapshot.
    last_events: Vec<Stamped>,
}

impl Game {
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The constants this run plays with (the sentinel's observer reads the horizons).
    pub fn params(&self) -> &Params {
        &self.params
    }

    /// Mutable access for the acceptance tests (orders, injected stocks) until the verbs and
    /// agents of E3 give orders through the contract. Behind the `testing` feature so that
    /// nothing in a game build can change the world past the rules (review of PR #283).
    #[cfg(feature = "testing")]
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn display(&self) -> &Display {
        &self.display
    }

    /// Events of the last played tick (the chronicle and the sentinel read them).
    pub fn last_events(&self) -> &[Stamped] {
        &self.last_events
    }

    /// The one availability-and-cost function (lesson B42), the same for the player and
    /// for agents. Internal politics (E2 step 6b) prices the tax verbs in legitimacy, the
    /// economy (E2 step 4) decides the loan, the army (E2 step 5) the levy, the disbanding
    /// and the hire, diplomacy (E2 step 7) war, peace, alliance, marriage, the embassy and
    /// the gift — a proposal's offer carries the threshold the other side needs.
    fn offers(&self, who: ActorRef) -> Vec<VerbOffer> {
        self.display
            .verbs
            .iter()
            .map(|(verb, _)| {
                let fallen = match who {
                    ActorRef::Ruler(a) => !self.world.alive(a),
                    ActorRef::House(_) => false,
                };
                if !fallen && crate::diplomacy::is_diplomatic(*verb) {
                    let o = self.world.diplomacy_offer(*verb, who, &self.params);
                    return VerbOffer { verb: *verb, available: o.available, cost_kg: o.cost_kg, cost_legitimacy: o.cost_legitimacy, why: o.why, threshold: o.threshold };
                }
                let mut cost_kg = 0;
                let mut cost_legitimacy = 0;
                let (available, why) = match (verb, who) {
                    (_, ActorRef::Ruler(_)) if fallen => (false, Some("the polity has fallen")),
                    (VerbId::RaiseTax | VerbId::LowerTax | VerbId::EnforceOrder, ActorRef::House(_)) => (false, Some("a house does not rule the polity")),
                    (VerbId::EnforceOrder, ActorRef::Ruler(a)) => {
                        cost_kg = self.world.order_cost(a, &self.params);
                        cost_legitimacy = -(self.params.order_gain.round() as i64);
                        if cost_kg <= 0 {
                            (false, Some("no revenue to pay from"))
                        } else if self.world.treasury(a) < cost_kg {
                            (false, Some("cannot pay"))
                        } else {
                            (true, None)
                        }
                    }
                    (VerbId::RaiseTax, ActorRef::Ruler(a)) => {
                        cost_legitimacy = self.params.legitimacy_cost_raise_tax.round() as i64;
                        match self.world.tax_room(a, VerbId::RaiseTax, &self.params) {
                            Ok(()) => (true, None),
                            Err(why) => (false, Some(why)),
                        }
                    }
                    (VerbId::LowerTax, ActorRef::Ruler(a)) => {
                        cost_legitimacy = -(self.params.legitimacy_gain_lower_tax.round() as i64);
                        match self.world.tax_room(a, VerbId::LowerTax, &self.params) {
                            Ok(()) => (true, None),
                            Err(why) => (false, Some(why)),
                        }
                    }
                    (VerbId::Recruit, ActorRef::Ruler(a)) => {
                        if self.world.recruit_per_tick(a, &self.params) > 0 {
                            (true, None)
                        } else {
                            (false, Some("at the levy ceiling"))
                        }
                    }
                    (VerbId::Disband, ActorRef::Ruler(a)) => {
                        if self.world.units_of(a).is_empty() {
                            (false, Some("no unit to disband"))
                        } else {
                            (true, None)
                        }
                    }
                    (VerbId::HireMercenaries, ActorRef::Ruler(a)) => {
                        let n = self.world.hire_count(a, &self.params);
                        if n > 0 {
                            cost_kg = n * self.params.mercenary_advance_kg;
                            (true, None)
                        } else if self.world.recruit_room(a, &self.params) <= 0 {
                            (false, Some("at the levy ceiling"))
                        } else {
                            (false, Some("cannot pay an advance"))
                        }
                    }
                    (VerbId::TakeLoan, ActorRef::Ruler(a)) => {
                        if !self.world.actors.get(&a).is_some_and(|x| x.institutions.contains(&Institution::Bank)) {
                            (false, Some("no bank"))
                        } else if self.world.credit_closed(a) {
                            (false, Some("in default: no credit"))
                        } else if self.world.loan_room(a, &self.params) <= 0 {
                            (false, Some("at the debt ceiling"))
                        } else {
                            (true, None)
                        }
                    }
                    (VerbId::TakeLoan, ActorRef::House(_)) => (false, Some("a house does not borrow for the polity")),
                    _ => (false, Some("not implemented until its system (E2) and the agents (E3)")),
                };
                VerbOffer { verb: *verb, available, cost_kg, cost_legitimacy, why: why.map(str::to_string), threshold: None }
            })
            .collect()
    }

    fn enabled(&self, verb: VerbId) -> bool {
        self.display.verbs.iter().any(|(v, _)| *v == verb)
    }
}

impl Simulation for Game {
    fn new(scenario: &ScenarioData, params: Params, seed: u64) -> Result<Game, SimError> {
        let c = scenario.compile(&params)?;
        let world = World::from_facts(&c.facts, c.names, &params, seed)?;
        Ok(Game { world, display: c.display, params, last_events: Vec::new() })
    }

    fn tick(&mut self, player: &[VerbCall]) -> Result<TickReport, SimError> {
        // Validate every call before touching the world, so an error leaves nothing
        // half-played.
        if let Some(bad) = player.iter().find(|c| !self.enabled(c.verb)) {
            return Err(SimError::VerbNotEnabled(bad.verb));
        }
        for call in player {
            self.world.apply_verb(call, &self.params)?;
            self.world.emit(Event::Verb { actor: call.actor, verb: call.verb, target: call.target });
        }
        let report = self.world.tick(&self.params)?;
        self.last_events = report.events.clone();
        Ok(report)
    }

    fn verbs(&self, who: ActorRef) -> Vec<VerbOffer> {
        self.offers(who)
    }

    fn view(&self) -> WorldView {
        view::world_view(&self.world, &self.display)
    }

    fn chronicle(&self) -> ChronicleSnapshot {
        view::chronicle_snapshot(&self.world, &self.last_events)
    }

    fn to_save(&self) -> Result<String, SimError> {
        Ok(save::to_save(&self.world)?)
    }

    fn from_save(scenario: &ScenarioData, params: Params, s: &str) -> Result<Game, SimError> {
        let c = scenario.compile(&params)?;
        let world = save::from_save(s)?;
        let scenario_hash = c.facts.hash();
        if world.scenario_id != c.facts.scenario_id || world.facts_hash != scenario_hash {
            return Err(SimError::WrongScenario {
                save: world.scenario_id.clone(),
                save_hash: world.facts_hash,
                scenario: c.facts.scenario_id.clone(),
                scenario_hash,
            });
        }
        Ok(Game { world, display: c.display, params, last_events: Vec::new() })
    }
}
