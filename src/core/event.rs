use serde::{Deserialize, Serialize};

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

/// One entry of the game's event log.
///
/// Since B31 the log is saved with the game, so this struct **is a save format**: a
/// field removed here fails to load in any build that still requires it. The optional
/// fields carry `#[serde(default)]` so that the next removal is safe for this build.
/// B36 removed `metrics_snapshot` (one writer, the death event, duplicating
/// `DeadActor.final_metrics`; no reader) and `scenario_id` (always empty) — a save
/// written after B36 does not load in a build before it (accepted, see docs/TRIAGE.md).
/// A save written before B36 loads here: unknown fields are ignored.
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
    #[serde(default)]
    pub involved_actors: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
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
            tags: Vec::new(),
            metadata: String::new(),
        }
    }

    /// Add involved actors
    pub fn with_involved_actors(mut self, actors: Vec<String>) -> Self {
        self.involved_actors = actors;
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
