//! The `Simulation` contract (plan §5) and its implementation on the E1 world.
//!
//! The shell talks to this trait only: create from a scenario, tick with the player's
//! verbs, view for the UI, snapshot for the chronicler, save and load. `observe` for the
//! sentinel arrives with the `sentinel` crate (E1c).

use crate::event::{Event, Stamped};
use crate::params::Params;
use crate::save::{self, SaveError};
use crate::scenario::{Names, ScenarioData, SchemaError};
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
}

pub trait Simulation: Sized {
    fn new(scenario: &ScenarioData, params: Params, seed: u64) -> Result<Self, SimError>;
    /// Play one tick with the player's calls. Returns exactly what the tick did.
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
    names: Names,
    params: Params,
    /// Events of the last played tick, for the chronicle snapshot.
    last_events: Vec<Stamped>,
}

impl Game {
    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn names(&self) -> &Names {
        &self.names
    }

    /// The one availability-and-cost function (lesson B42). E1b: every enabled verb is on
    /// offer at no cost and has no effect; E3 computes both from the state.
    fn offers(&self, _who: ActorRef) -> Vec<VerbOffer> {
        self.names.verbs.iter().map(|(verb, _)| VerbOffer { verb: *verb, available: true, cost_kg: 0 }).collect()
    }

    fn enabled(&self, verb: VerbId) -> bool {
        self.names.verbs.iter().any(|(v, _)| *v == verb)
    }
}

impl Simulation for Game {
    fn new(scenario: &ScenarioData, params: Params, seed: u64) -> Result<Game, SimError> {
        let (facts, names) = scenario.compile()?;
        let world = World::from_facts(&facts, seed)?;
        Ok(Game { world, names, params, last_events: Vec::new() })
    }

    fn tick(&mut self, player: &[VerbCall]) -> Result<TickReport, SimError> {
        for call in player {
            // A disabled verb is not an error of the engine but of the caller; it is
            // journaled as nothing. The offer list is the contract.
            if self.enabled(call.verb) {
                self.world.emit(Event::Verb { actor: call.actor, verb: call.verb, target: call.target });
            }
        }
        let report = self.world.tick(&self.params)?;
        self.last_events = report.events.clone();
        Ok(report)
    }

    fn verbs(&self, who: ActorRef) -> Vec<VerbOffer> {
        self.offers(who)
    }

    fn view(&self) -> WorldView {
        view::world_view(&self.world, &self.names)
    }

    fn chronicle(&self) -> ChronicleSnapshot {
        view::chronicle_snapshot(&self.world, &self.names, &self.last_events)
    }

    fn to_save(&self) -> Result<String, SimError> {
        Ok(save::to_save(&self.world)?)
    }

    fn from_save(scenario: &ScenarioData, params: Params, s: &str) -> Result<Game, SimError> {
        let (_, names) = scenario.compile()?;
        let world = save::from_save(s)?;
        Ok(Game { world, names, params, last_events: Vec::new() })
    }
}
