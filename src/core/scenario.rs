use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::actor::{Actor, Era};

/// Dependency rule mode - determines how the dependency affects the target metric
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyMode {
    /// Penalty when from < threshold
    Deficit,
    /// Penalty when from > threshold
    Excess,
    /// Bonus when from > threshold
    Bonus,
    /// Linear: delta = from * coefficient, no threshold
    Linear,
}

/// Dependency rule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyRule {
    /// Identifier for logging and debugging (e.g., "legitimacy_to_cohesion")
    pub id: String,
    /// Source metric name
    pub from: String,
    /// Target metric name
    pub to: String,
    /// Coefficient for delta calculation
    pub coefficient: f64,
    /// Threshold value (required for Deficit/Excess/Bonus modes, None for Linear)
    pub threshold: Option<f64>,
    /// Mode of operation
    pub mode: DependencyMode,
}

/// Narrative configuration for data-driven chronicle generation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NarrativeConfig {
    /// Key metrics to include in factual block
    pub key_metrics: Vec<String>,
    /// Narrative axes for framing (e.g., "stability vs ambition", "tradition vs innovation")
    pub narrative_axes: Vec<String>,
    /// Tone tags for chronicler style (e.g., "formal", "epic", "intimate")
    pub tone_tags: Vec<String>,
    /// Claims the chronicler should NOT make (anti-hallucination guards)
    pub forbidden_claims: Vec<String>,
    /// Target paragraph count for generation
    pub paragraph_target: u32,
    /// Output length hint for model (e.g., "long-form chronicle", "detailed account")
    pub output_length_hint: String,
}

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
    /// Base probability for land military conflicts (0.0-1.0)
    pub military_conflict_probability: f64,
    /// Base probability for naval conflicts (0.0-1.0)
    pub naval_conflict_probability: f64,
    /// Random events pool for this scenario
    pub random_events: Vec<RandomEvent>,
    /// Generation length in years (for family scenarios, None = not a family scenario)
    pub generation_length: Option<u32>,
    /// Maximum actions per tick (0 = unlimited)
    pub actions_per_tick: u32,
    /// Victory condition for the scenario (None = no victory condition)
    pub victory_condition: Option<VictoryCondition>,
    /// Universal actions available in Consequences/Free modes (replaces get_universal_actions())
    pub universal_actions: Vec<PatronAction>,
    /// Global metrics to display in UI (for scenarios with global_metrics_panel)
    pub global_metrics_display: Vec<MetricDisplay>,
    /// Initial family metrics for family-based scenarios (None = not a family scenario)
    pub initial_family_metrics: Option<HashMap<String, f64>>,
    /// Maximum random events per tick (0 = unlimited)
    pub max_random_events_per_tick: u32,
    /// Narrative configuration for data-driven chronicle generation
    pub narrative_config: NarrativeConfig,
    /// Dependency rules loaded from dependencies.toml
    #[serde(default)]
    pub dependencies: Vec<DependencyRule>,
}

/// Metric display configuration for UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDisplay {
    pub metric: String,
    pub label: String,
    pub panel_title: String,
    pub thresholds: Vec<MetricThreshold>,
}

/// Threshold for metric display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricThreshold {
    pub below: f64,
    pub text: String,
}

/// Victory condition for scenario completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoryCondition {
    pub metric: String,
    pub threshold: f64,
    pub title: String,
    pub description: String,
    pub minimum_tick: u32,
    pub additional_conditions: Vec<Condition>,
    pub sustained_ticks_required: u32,
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

/// Condition for random event triggering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub metric: String,
    pub operator: ComparisonOperator,
    pub value: f64,
}

/// Target for random event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventTarget {
    Actor(String),
    Any,
    All,
    SeaActors,
}

/// Random event definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomEvent {
    pub id: String,
    pub probability: f64,
    pub target: EventTarget,
    pub conditions: Vec<Condition>,
    pub effects: HashMap<String, f64>,
    pub llm_context: String,
    pub one_time: bool,
}

/// Autonomous delta configuration for metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoDelta {
    pub metric: String,
    pub base: f64,
    pub conditions: Vec<DeltaCondition>,
    pub ratio_conditions: Vec<DeltaConditionRatio>,
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

/// Ratio-based condition for auto delta modification
/// Applies additional delta if ratio between two metrics meets threshold
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaConditionRatio {
    pub metric_a: String,  // numerator
    pub metric_b: String,  // denominator
    pub ratio: f64,        // threshold ratio
    pub operator: ComparisonOperator,
    pub delta: f64,        // additional delta if condition met
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

impl ComparisonOperator {
    pub fn evaluate(&self, value: f64, target: f64) -> bool {
        match self {
            ComparisonOperator::Less => value < target,
            ComparisonOperator::LessOrEqual => value <= target,
            ComparisonOperator::Greater => value > target,
            ComparisonOperator::GreaterOrEqual => value >= target,
            ComparisonOperator::Equal => (value - target).abs() < 0.001,
        }
    }
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
    pub cooldown_ticks: Option<u32>,  // Minimum ticks between firings
}

/// Condition for milestone event triggering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCondition {
    #[serde(flatten)]
    pub condition_type: EventConditionType,
    pub duration: Option<u32>,
}

impl EventCondition {
    /// Extract all metric strings from condition
    pub fn to_metric_strings(&self) -> Vec<String> {
        match &self.condition_type {
            EventConditionType::Metric { metric, .. } => vec![metric.clone()],
            EventConditionType::ActorState { actor_id, .. } => vec![actor_id.clone()],
            EventConditionType::Tick { .. } => vec![],
        }
    }
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

/// Era text for family panel context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraText {
    pub from_year: i32,
    pub to_year: i32,
    pub text: String,
}

/// Generation mechanics for family/patriarch system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationMechanics {
    pub tick_span: u32,
    pub patriarch_start_age: u32,
    pub patriarch_end_age: u32,
    /// Generation length in years (separate from tick_span)
    pub generation_length: u32,
    /// Inheritance coefficients per family metric (default 0.7 if not specified)
    pub inheritance_coefficients: HashMap<String, f64>,
    /// Panel label for FamilyPanel UI
    pub panel_label: String,
    /// Era-specific context texts
    pub era_texts: Vec<EraText>,
    /// Early transfer conditions (optional)
    #[serde(default)]
    pub early_transfer: Option<EarlyTransfer>,
}

/// Early transfer condition for generation mechanics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarlyTransfer {
    pub age: u32,
    pub condition_metric: String,
    pub condition_operator: ComparisonOperator,
    pub condition_value: f64,
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
