//! The `Simulation` contract (plan §5) and its implementation on the E1 world.
//!
//! The shell talks to this trait only: create from a scenario, tick with the player's
//! verbs, view for the UI, snapshot for the chronicler, save and load. `observe` for the
//! sentinel arrives with the `sentinel` crate (E1c).
//!
//! A load checks that the save belongs to the scenario it is loaded with — scenario id and
//! the hash of the compiled facts — so a run of one scenario cannot continue as another
//! (AGENTS §8, owner's review of PR #278).

use crate::event::{Event, Stamped};
use crate::params::Params;
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

    pub fn display(&self) -> &Display {
        &self.display
    }

    /// The one availability-and-cost function (lesson B42). E1b: every enabled verb is on
    /// offer at no cost and has no effect; E3 computes both from the state.
    fn offers(&self, _who: ActorRef) -> Vec<VerbOffer> {
        self.display.verbs.iter().map(|(verb, _)| VerbOffer { verb: *verb, available: true, cost_kg: 0 }).collect()
    }

    fn enabled(&self, verb: VerbId) -> bool {
        self.display.verbs.iter().any(|(v, _)| *v == verb)
    }
}

impl Simulation for Game {
    fn new(scenario: &ScenarioData, params: Params, seed: u64) -> Result<Game, SimError> {
        let c = scenario.compile()?;
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
        let c = scenario.compile()?;
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
