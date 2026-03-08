use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use super::actor::{Actor, ActorMetrics};

/// Current save format version
pub const SAVE_FORMAT_VERSION: u32 = 1;

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

/// Actor delta for tracking metric changes between ticks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorDelta {
    pub actor_id: String,
    pub actor_name: String,
    pub metric_changes: HashMap<String, f64>,
}

/// Game mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GameMode {
    Scenario,
    Consequences,
    Free,
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
    /// Set of dead actor IDs for fast lookup (prevents duplicate death events)
    pub dead_actor_ids: HashSet<String>,
    pub alliances: Vec<Alliance>,
    pub milestone_events_fired: Vec<String>,
    pub milestone_condition_ticks: HashMap<String, u32>, // Tracks how many consecutive ticks a milestone condition has been met
    /// Global scenario metrics (includes family_*, federation_progress, etc.) - key: metric_name, value: current value
    pub global_metrics: HashMap<String, f64>,
    /// Metric history for relevance threshold tracking - key: "actor_id:metric_name", value: last 5 ticks
    pub metric_history: HashMap<String, VecDeque<f64>>,
    /// Ticks since last internal upheaval for each actor - for background return check
    pub actor_upheaval_ticks: HashMap<String, u32>,
    /// RNG seed - generated once at scenario start, preserved for reproducibility
    pub rng_seed: u64,
    /// RNG state - serialized/deserialized with WorldState for save/load
    pub rng_state: [u8; 32],
    /// Previous tick metrics for each actor - for calculating deltas
    pub prev_metrics: HashMap<String, ActorMetrics>,
    /// Ticks since last narrative trigger - for time-based trigger
    pub ticks_since_last_narrative: u32,
    /// Interaction cooldowns - key: "actor_a_vs_actor_b", value: last tick
    pub interaction_cooldowns: HashMap<String, u32>,
    /// Set of fired one-time random event IDs
    pub fired_events: HashSet<String>,
    /// Milestone cooldowns - milestone_id -> tick of last firing
    pub milestone_cooldowns: HashMap<String, u32>,
    /// Save format version for compatibility checking
    pub save_version: u32,
}

impl WorldState {
    pub fn new(scenario_id: String, start_year: i32) -> Self {
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        // Generate random seed for new game
        let rng_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        // Initialize RNG and capture its state
        let rng = ChaCha8Rng::seed_from_u64(rng_seed);
        let rng_state = rng.get_seed();

        Self {
            tick: 0,
            year: start_year,
            scenario_id,
            game_mode: GameMode::Scenario,
            actors: HashMap::new(),
            dead_actors: Vec::new(),
            dead_actor_ids: HashSet::new(),
            alliances: Vec::new(),
            milestone_events_fired: Vec::new(),
            milestone_condition_ticks: HashMap::new(),
            global_metrics: HashMap::new(),
            metric_history: HashMap::new(),
            actor_upheaval_ticks: HashMap::new(),
            rng_seed,
            rng_state,
            prev_metrics: HashMap::new(),
            ticks_since_last_narrative: 0,
            interaction_cooldowns: HashMap::new(),
            fired_events: HashSet::new(),
            milestone_cooldowns: HashMap::new(),
            save_version: SAVE_FORMAT_VERSION,
        }
    }

    /// Create WorldState with explicit seed (for save/load)
    pub fn with_seed(scenario_id: String, start_year: i32, rng_seed: u64, rng_state: [u8; 32]) -> Self {
        Self {
            tick: 0,
            year: start_year,
            scenario_id,
            game_mode: GameMode::Scenario,
            actors: HashMap::new(),
            dead_actors: Vec::new(),
            dead_actor_ids: HashSet::new(),
            alliances: Vec::new(),
            milestone_events_fired: Vec::new(),
            milestone_condition_ticks: HashMap::new(),
            global_metrics: HashMap::new(),
            metric_history: HashMap::new(),
            actor_upheaval_ticks: HashMap::new(),
            rng_seed,
            rng_state,
            prev_metrics: HashMap::new(),
            ticks_since_last_narrative: 0,
            interaction_cooldowns: HashMap::new(),
            fired_events: HashSet::new(),
            milestone_cooldowns: HashMap::new(),
            save_version: SAVE_FORMAT_VERSION,
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
