use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Event type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Collapse,
    War,
    Migration,
    Threshold,
    Birth,
    Death,
    Trade,
    Cultural,
    Diplomatic,
    PlayerAction,
    Milestone,
}

/// Key event record for indexed storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub tick: u32,
    pub year: i32,
    pub actor_id: String,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub is_key: bool,
    pub description: String,
    pub involved_actors: Vec<String>,
    pub metrics_snapshot: HashMap<String, f64>,
    pub tags: Vec<String>,
    pub scenario_id: String,
    pub metadata: String,
}

impl Event {
    /// Create a new event
    pub fn new(
        id: String,
        tick: u32,
        year: i32,
        actor_id: String,
        event_type: EventType,
        is_key: bool,
        description: String,
    ) -> Self {
        Self {
            id,
            tick,
            year,
            actor_id,
            event_type,
            is_key,
            description,
            involved_actors: Vec::new(),
            metrics_snapshot: HashMap::new(),
            tags: Vec::new(),
            scenario_id: String::new(),
            metadata: String::new(),
        }
    }

    /// Set scenario_id
    pub fn with_scenario_id(mut self, scenario_id: String) -> Self {
        self.scenario_id = scenario_id;
        self
    }

    /// Add involved actors
    pub fn with_involved_actors(mut self, actors: Vec<String>) -> Self {
        self.involved_actors = actors;
        self
    }

    /// Add metrics snapshot
    pub fn with_metrics_snapshot(mut self, snapshot: HashMap<String, f64>) -> Self {
        self.metrics_snapshot = snapshot;
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set metadata (for storing effects, etc.)
    pub fn with_metadata(mut self, metadata: String) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Event record for SQLite storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: String,
    pub tick: u32,
    pub year: i32,
    pub actor_id: String,
    pub event_type: String,
    pub is_key: bool,
    pub description: String,
    pub involved_actors: String, // JSON array
    pub metrics_snapshot: String, // JSON object
    pub tags: String,            // JSON array
}

impl From<Event> for StoredEvent {
    fn from(event: Event) -> Self {
        Self {
            id: event.id,
            tick: event.tick,
            year: event.year,
            actor_id: event.actor_id,
            event_type: format!("{:?}", event.event_type),
            is_key: event.is_key,
            description: event.description,
            involved_actors: serde_json::to_string(&event.involved_actors).unwrap_or_default(),
            metrics_snapshot: serde_json::to_string(&event.metrics_snapshot).unwrap_or_default(),
            tags: serde_json::to_string(&event.tags).unwrap_or_default(),
        }
    }
}

/// Event query result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventQueryResult {
    pub event: Event,
    pub relevance_score: f64,
    pub temporal_coefficient: f64,
    pub thematic_similarity: f64,
}

/// Temporal decay configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDecay {
    pub recent_ticks: u32,
    pub recent_coefficient: f64,
    pub tiers: Vec<DecayTier>,
    pub key_event_min_coefficient: f64,
}

/// Decay tier for temporal relevance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayTier {
    pub max_ticks_ago: u32,
    pub coefficient: f64,
}

impl Default for TemporalDecay {
    fn default() -> Self {
        Self {
            recent_ticks: 10,
            recent_coefficient: 1.0,
            tiers: vec![
                DecayTier { max_ticks_ago: 30, coefficient: 0.7 },
                DecayTier { max_ticks_ago: 60, coefficient: 0.4 },
                DecayTier { max_ticks_ago: 100, coefficient: 0.2 },
            ],
            key_event_min_coefficient: 0.3,
        }
    }
}
