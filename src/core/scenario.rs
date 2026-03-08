use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::actor::{Actor, Era};

/// Main Scenario configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub label: String,
    pub description: String,
    pub start_year: i32,
    pub tempo: f64,
    pub tick_span: u32,
    pub era: Era,
    pub tick_label: String,
    pub actors: Vec<Actor>,
    pub auto_deltas: Vec<AutoDelta>,
    pub patron_actions: Vec<PatronAction>,
    pub milestone_events: Vec<MilestoneEvent>,
    pub rank_conditions: Vec<RankCondition>,
    pub generation_mechanics: Option<GenerationMechanics>,
    pub llm_context: String,
    pub consequence_context: String,
    pub player_actor_id: Option<String>,
    /// Status indicators for UI display
    pub status_indicators: Vec<StatusIndicator>,
    /// Global metric weights by source actor: metric_name -> {source_actor -> weight}
    pub global_metric_weights: HashMap<String, HashMap<String, f64>>,
    /// Feature flags for UI
    pub features: ScenarioFeatures,
}

/// Status indicator for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusIndicator {
    pub label: String,
    pub metric: String,
    pub invert: bool,
    pub thresholds: Vec<(f64, String)>,
}

/// Scenario feature flags for UI
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScenarioFeatures {
    pub family_panel: bool,
    pub global_metrics_panel: bool,
    pub patron_actions: bool,
}

/// Autonomous delta configuration for metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoDelta {
    pub metric: String,
    pub base: f64,
    pub conditions: Vec<DeltaCondition>,
    pub noise: f64,
    pub actor_id: Option<String>,
}

/// Condition for auto delta modification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaCondition {
    pub metric: String,
    pub operator: ComparisonOperator,
    pub value: f64,
    pub delta: f64,
}

/// Comparison operator for conditions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
}

/// Player action definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatronAction {
    pub id: String,
    pub name: String,
    pub source_actor_id: Option<String>,
    pub available_if: ActionCondition,
    pub effects: HashMap<String, f64>,
    pub cost: HashMap<String, f64>,
}

/// Condition for action availability
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCondition {
    Always,
    Metric { metric: String, operator: ComparisonOperator, value: f64 },
}

/// Milestone event that changes narrative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneEvent {
    pub id: String,
    pub condition: EventCondition,
    pub is_key: bool,
    pub triggers_collapse: bool,
    pub llm_context_shift: String,
}

/// Condition for milestone event triggering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCondition {
    #[serde(flatten)]
    pub condition_type: EventConditionType,
    pub duration: Option<u32>,
}

/// Type of event condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventConditionType {
    Metric {
        metric: String,
        actor_id: Option<String>,
        operator: ComparisonOperator,
        value: f64,
    },
    ActorState {
        actor_id: String,
        state: ActorState,
    },
    Tick {
        tick: u32,
    },
}

/// Actor state for conditions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActorState {
    Dead,
    Alive,
    Foreground,
    Background,
}

/// Rank condition for region rank changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankCondition {
    pub region_id: String,
    pub condition: EventCondition,
    pub result: RankResult,
    pub is_key: bool,
}

/// Result of rank condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankResult {
    pub rank: String,
}

/// Generation mechanics for family/patriarch system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationMechanics {
    pub tick_span: u32,
    pub patriarch_start_age: u32,
    pub patriarch_end_age: u32,
    /// Inheritance coefficients per family metric (default 0.7 if not specified)
    pub inheritance_coefficients: HashMap<String, f64>,
}

/// Player context for scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerContext {
    pub actor_id: String,
    pub role_description: String,
}

/// Scenario metrics definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioMetric {
    pub id: String,
    pub label: String,
    pub description: String,
    pub default_value: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
