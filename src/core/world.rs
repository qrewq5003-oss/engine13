use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::actor::Actor;

/// Dead actor record - preserves history after collapse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadActor {
    pub id: String,
    pub tick_death: u32,
    pub year_death: i32,
    pub final_metrics: HashMap<String, f64>,
    pub successor_ids: Vec<SuccessorWeight>,
}

/// Weight for successor inheritance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessorWeight {
    pub id: String,
    pub weight: f64,
}

/// Alliance between actors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alliance {
    pub actor_ids: Vec<String>,
    pub common_enemy: Option<String>,
    pub trade_benefit: bool,
    pub formed_tick: u32,
}

/// Game mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GameMode {
    Sandbox,
    Scenario,
}

/// Current state of the world simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    pub tick: u32,
    pub year: i32,
    pub scenario_id: String,
    pub game_mode: GameMode,
    pub actors: HashMap<String, Actor>,
    pub dead_actors: Vec<DeadActor>,
    pub alliances: Vec<Alliance>,
    pub milestone_events_fired: Vec<String>,
}

impl WorldState {
    pub fn new(scenario_id: String, start_year: i32) -> Self {
        Self {
            tick: 0,
            year: start_year,
            scenario_id,
            game_mode: GameMode::Scenario,
            actors: HashMap::new(),
            dead_actors: Vec::new(),
            alliances: Vec::new(),
            milestone_events_fired: Vec::new(),
        }
    }

    /// Get actor by ID
    pub fn get_actor(&self, id: &str) -> Option<&Actor> {
        self.actors.get(id)
    }

    /// Get mutable actor by ID
    pub fn get_actor_mut(&mut self, id: &str) -> Option<&mut Actor> {
        self.actors.get_mut(id)
    }

    /// Check if actor is alive
    pub fn is_actor_alive(&self, id: &str) -> bool {
        self.actors.contains_key(id)
    }

    /// Get dead actor by ID
    pub fn get_dead_actor(&self, id: &str) -> Option<&DeadActor> {
        self.dead_actors.iter().find(|a| a.id == id)
    }
}
