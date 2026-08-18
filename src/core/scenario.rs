use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::actor::{Actor, Era, Neighbor, RegionRank, TagSpreadType};
use super::metric_ref::{resolve_at_load, MetricName, MetricRef, RelativeMetricRef};

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
    /// Penalty when from < threshold, sized as a *share of the target's own stock*
    /// rather than as an absolute amount:
    ///
    /// ```text
    /// delta = -to * coefficient * (threshold - from) / threshold
    /// ```
    ///
    /// `Deficit` prices the penalty in the units of the *source* metric, which only
    /// works when every actor's `to` metric lives on one scale. Task 22 measured what
    /// happens when it does not: `population` spans `15…8000` across the three
    /// scenarios while `economic_output_to_population` charges a flat
    /// `20 * (50 - eo)`, so the same rule costs `rome` 0.5 % of its people per tick and
    /// costs `byzantium` ten times everything it has — 25 living actors of 41 are
    /// zeroed, 18 of them on tick 1. No absolute coefficient can be both meaningful at
    /// `8000` and survivable at `50` (`docs/investigation_eo_population_attractor.md`
    /// §4.1 closes that interval).
    ///
    /// This mode removes the unit mismatch instead of re-picking the number: the
    /// deficit `(threshold - from) / threshold` is dimensionless, so `coefficient` is
    /// the share of the stock lost per tick at *full* deficit and the rule reads the
    /// same at every scale. It is a strictly stronger contract than `Deficit`: for
    /// `coefficient < 1` the target can never be driven negative or to zero in one
    /// tick, at any scale, by construction.
    ///
    /// Requires `threshold > 0` (it is the normalizer) — enforced at load by
    /// `engine::validate_dependency_thresholds`.
    DeficitProportional,
}

/// Dependency rule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyRule {
    /// Identifier for logging and debugging (e.g., "legitimacy_to_cohesion")
    pub id: String,
    /// Source metric name
    pub from: MetricName,
    /// Target metric name
    pub to: MetricName,
    /// Coefficient for delta calculation
    pub coefficient: f64,
    /// Threshold value (required for Deficit/Excess/Bonus modes, None for Linear)
    pub threshold: Option<f64>,
    /// Mode of operation
    pub mode: DependencyMode,
}

/// Which actor a condition or effect applies to
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConditionActor {
    /// The source actor in the interaction
    Source,
    /// The target actor in the interaction
    Target,
}

/// Condition for interaction rule applicability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionCondition {
    /// Which actor to check
    pub actor: ConditionActor,
    /// Metric to check — a bare name. The actor is `self.actor` (source/target of
    /// the rule), not the string, so a scope prefix here has never meant anything
    /// and used to read a silent 0.0. `MetricName` now rejects it at load.
    pub metric: MetricName,
    /// Comparison operator (snake_case: "less", "less_or_equal", "greater", "greater_or_equal", "equal")
    pub operator: ComparisonOperator,
    /// Threshold value
    pub value: f64,
}

/// Effect applied by an interaction rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionEffect {
    /// Which actor receives the effect
    pub actor: ConditionActor,
    /// Metric to modify — a bare name; the actor is `self.actor`.
    pub metric: MetricName,
    /// Flat delta to apply
    pub delta: f64,
}

/// Data-driven interaction rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRule {
    /// Unique identifier (used in cooldown key)
    pub id: String,
    /// Maximum distance for this interaction (>= 1)
    pub max_distance: u32,
    /// Border type filter: "land" | "sea" | None = any
    #[serde(default)]
    pub border_type: Option<String>,
    /// Cooldown in ticks (0 = no cooldown)
    pub cooldown_ticks: u32,
    /// Conditions that must all pass for rule to apply
    #[serde(default)]
    pub conditions: Vec<InteractionCondition>,
    /// Effects to apply (must not be empty)
    pub effects: Vec<InteractionEffect>,
    /// Optional event type for logging
    #[serde(default)]
    pub event_type: Option<String>,
    /// Minimum total abs delta to trigger event
    #[serde(default)]
    pub event_threshold: f64,
}

/// Rank bonus effect — either a flat delta or a floor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankBonusEffect {
    /// Metric to modify — a bare name; the actor is the one holding the rank.
    pub metric: MetricName,
    /// Flat delta to apply (ignored if floor is set)
    #[serde(default)]
    pub delta: f64,
    /// Minimum value (floor) — if set, applies as min(), not delta
    #[serde(default)]
    pub floor: Option<f64>,
}

/// Rank bonus rule for a specific region rank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankBonusRule {
    /// Region rank this rule applies to
    pub rank: RegionRank,
    /// Effects to apply for this rank
    #[serde(default)]
    pub effects: Vec<RankBonusEffect>,
}

/// Default spread cooldown in ticks
fn default_spread_cooldown() -> u32 { 5 }
/// Default spread probability
fn default_spread_chance() -> f64 { 0.3 }

/// Tag definition loaded from scenario config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagDefinition {
    pub id: String,
    /// Bare metric names; the actor is the one carrying the tag.
    pub metrics_modifier: HashMap<MetricName, i32>,
    pub spreads_via: Vec<TagSpreadType>,
    #[serde(default = "default_spread_cooldown")]
    pub spread_cooldown_ticks: u32,
    #[serde(default = "default_spread_chance")]
    pub spread_chance: f64,
    #[serde(default)]
    pub requires_era: Option<Era>,
    #[serde(default)]
    pub unlocks: Vec<String>,
}

/// Era definition loaded from scenario config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraDefinition {
    pub era: Era,
    #[serde(default)]
    pub min_tick: u32,
    #[serde(default)]
    pub requires_tags: u32,
    #[serde(default)]
    pub from_tags: Vec<String>,
    #[serde(default)]
    pub auto_delta_modifier: f64,
    #[serde(default)]
    pub unlocks_tags: Vec<String>,
}

/// Narrative configuration for data-driven chronicle generation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NarrativeConfig {
    /// Key metrics to include in factual block
    pub key_metrics: Vec<MetricRef>,
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
    /// Global metric weights by source actor: metric -> {source_actor -> weight}.
    /// Keyed by the same type as `PatronAction.effects` so the lookup cannot miss
    /// through a string mismatch (a miss silently applies weight 1.0).
    pub global_metric_weights: HashMap<MetricRef, HashMap<String, f64>>,
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
    /// Interaction rules loaded from interaction_rules.toml
    #[serde(default)]
    pub interaction_rules: Vec<InteractionRule>,
    /// Rank bonus rules loaded from rank_bonuses.toml
    #[serde(default)]
    pub rank_bonuses: Vec<RankBonusRule>,
    /// Map configuration loaded from map.toml
    #[serde(default)]
    pub map: Option<MapConfig>,
    /// Tag definitions loaded from tags.toml
    #[serde(default)]
    pub tag_definitions: Vec<TagDefinition>,
    /// Era definitions loaded from eras.toml
    #[serde(default)]
    pub era_definitions: Vec<EraDefinition>,
}

/// Metric display configuration for UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDisplay {
    pub metric: MetricRef,
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
    pub metric: MetricRef,
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
    pub metric: MetricRef,
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

/// A condition on an *absolute* metric key.
///
/// Used by `victory_condition.additional_conditions`. Until this refactor the same
/// struct also carried `random_events.conditions`, whose keys are `self.`-relative
/// to a target actor chosen at runtime — one struct with two incompatible parsing
/// semantics. That is now [`RelativeCondition`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub metric: MetricRef,
    pub operator: ComparisonOperator,
    pub value: f64,
}

/// A condition on a key that may be `self.`-relative to the event's target actor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelativeCondition {
    pub metric: RelativeMetricRef,
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
    pub conditions: Vec<RelativeCondition>,
    pub effects: HashMap<RelativeMetricRef, f64>,
    pub llm_context: String,
    pub one_time: bool,
}

/// Autonomous delta configuration for metrics.
///
/// Its metric keys are written *relative to `actor_id`* — a bare `cohesion` next
/// to `actor_id = "rome"` means Rome's cohesion. That scope lives in a **sibling
/// field**, which serde's per-field `deserialize_with` cannot see, so the whole
/// struct is read through a shadow type (below) and every key is resolved **once,
/// at load**. The tick loop then applies a `MetricRef` directly — the per-tick
/// re-parsing that produced sites 1–3 of the metric-scoping class is gone.
#[derive(Debug, Clone, Serialize)]
pub struct AutoDelta {
    pub metric: MetricRef,
    pub base: f64,
    pub conditions: Vec<DeltaCondition>,
    pub ratio_conditions: Vec<DeltaConditionRatio>,
    pub noise: f64,
    /// Load-time scope for the keys above. The engine no longer reads it: by the
    /// time a tick runs it is already baked into every `MetricRef` in this block.
    pub actor_id: Option<String>,
}

/// Condition for auto delta modification
#[derive(Debug, Clone, Serialize)]
pub struct DeltaCondition {
    pub metric: MetricRef,
    pub operator: ComparisonOperator,
    pub value: f64,
    pub delta: f64,
}

/// Ratio-based condition for auto delta modification
/// Applies additional delta if ratio between two metrics meets threshold
#[derive(Debug, Clone, Serialize)]
pub struct DeltaConditionRatio {
    pub metric_a: MetricRef,  // numerator
    pub metric_b: MetricRef,  // denominator
    pub ratio: f64,           // threshold ratio
    pub operator: ComparisonOperator,
    pub delta: f64,           // additional delta if condition met
}

// --- Shadow types: the raw TOML shape, before `actor_id` is folded into the keys ---

#[derive(Deserialize)]
struct AutoDeltaRaw {
    metric: String,
    base: f64,
    #[serde(default)]
    conditions: Vec<DeltaConditionRaw>,
    #[serde(default)]
    ratio_conditions: Vec<DeltaConditionRatioRaw>,
    noise: f64,
    actor_id: Option<String>,
}

#[derive(Deserialize)]
struct DeltaConditionRaw {
    metric: String,
    operator: ComparisonOperator,
    value: f64,
    delta: f64,
}

#[derive(Deserialize)]
struct DeltaConditionRatioRaw {
    metric_a: String,
    metric_b: String,
    ratio: f64,
    operator: ComparisonOperator,
    delta: f64,
}

impl<'de> Deserialize<'de> for AutoDelta {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = AutoDeltaRaw::deserialize(deserializer)?;
        let scope = raw.actor_id.as_deref();
        let resolve = |s: &str| resolve_at_load(s, scope).map_err(D::Error::custom);

        Ok(AutoDelta {
            metric: resolve(&raw.metric)?,
            base: raw.base,
            conditions: raw
                .conditions
                .into_iter()
                .map(|c| {
                    Ok(DeltaCondition {
                        metric: resolve(&c.metric)?,
                        operator: c.operator,
                        value: c.value,
                        delta: c.delta,
                    })
                })
                .collect::<Result<Vec<_>, D::Error>>()?,
            ratio_conditions: raw
                .ratio_conditions
                .into_iter()
                .map(|r| {
                    Ok(DeltaConditionRatio {
                        metric_a: resolve(&r.metric_a)?,
                        metric_b: resolve(&r.metric_b)?,
                        ratio: r.ratio,
                        operator: r.operator,
                        delta: r.delta,
                    })
                })
                .collect::<Result<Vec<_>, D::Error>>()?,
            noise: raw.noise,
            actor_id: raw.actor_id,
        })
    }
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
    pub effects: HashMap<MetricRef, f64>,
    pub cost: HashMap<MetricRef, f64>,
}

/// Condition for action availability
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionCondition {
    Always,
    Metric { metric: MetricRef, operator: ComparisonOperator, value: f64 },
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
    #[serde(default)]
    pub spawn_actor: Option<SpawnActorConfig>,
}

/// Configuration for spawning a new actor via milestone event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnActorConfig {
    pub actor_id: String,
    pub label: String,
    /// Bare metric names; the actor is the one being spawned.
    pub initial_metrics: HashMap<MetricName, f64>,
    pub lat: f64,
    pub lng: f64,
    pub color: String,
    /// Neighbor edges the spawned actor enters the world with.
    /// Explicit (not derived from lat/lng) because `border_type` (land/sea)
    /// and the small hand-authored integer `distance` scale are load-bearing
    /// for interaction rules and cannot be inferred from coordinates. Without
    /// at least one live neighbor here, the spawned actor never appears in any
    /// pair from `get_neighbor_pairs` and stays inert (the France-in-Milan bug).
    /// Defaults to empty for back-compat with configs that predate this field.
    #[serde(default)]
    pub neighbors: Vec<Neighbor>,
}

/// Condition for milestone event triggering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCondition {
    #[serde(flatten)]
    pub condition_type: EventConditionType,
    pub duration: Option<u32>,
}

impl EventCondition {
    /// Extract all metric strings from condition.
    ///
    /// `ActorState` yields none: its `actor_id` is an actor id, not a metric key, and
    /// returning it here made the only caller (`validate_scenario`) check an actor id
    /// against the metric rules. That was harmless while the metric check ignored
    /// global-shaped keys, and became a false positive the moment it stopped ignoring
    /// them. Use [`actor_state_actor_id`](Self::actor_state_actor_id) for that field.
    pub fn metric_ref(&self) -> Option<&MetricRef> {
        match &self.condition_type {
            EventConditionType::Metric { metric, .. } => Some(metric),
            EventConditionType::ActorState { .. } => None,
            EventConditionType::Tick { .. } => None,
        }
    }

    /// The actor an `actor_state` condition refers to, if this is one.
    pub fn actor_state_actor_id(&self) -> Option<&str> {
        match &self.condition_type {
            EventConditionType::ActorState { actor_id, .. } => Some(actor_id.as_str()),
            _ => None,
        }
    }
}

/// Type of event condition
///
/// `Metric` pairs a key with a sibling `actor_id`, exactly like `AutoDelta`, and is
/// resolved the same way — once, at load, through the shadow type below. The
/// `actor_id` is **kept** after resolution because it carries a second meaning the
/// key cannot: an actor-scoped condition on an actor that is *not in the world*
/// (never spawned, or removed on collapse) is **false**, not `0.0`. See
/// `eval_metric_condition`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventConditionType {
    Metric {
        metric: MetricRef,
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

/// Shadow type: the raw TOML shape of an event condition, before `actor_id` is
/// folded into the metric key.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EventConditionTypeRaw {
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

impl<'de> Deserialize<'de> for EventConditionType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        Ok(match EventConditionTypeRaw::deserialize(deserializer)? {
            EventConditionTypeRaw::Metric { metric, actor_id, operator, value } => {
                let metric = resolve_at_load(&metric, actor_id.as_deref())
                    .map_err(D::Error::custom)?;
                EventConditionType::Metric { metric, actor_id, operator, value }
            }
            EventConditionTypeRaw::ActorState { actor_id, state } => {
                EventConditionType::ActorState { actor_id, state }
            }
            EventConditionTypeRaw::Tick { tick } => EventConditionType::Tick { tick },
        })
    }
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
    pub condition_metric: MetricRef,
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

/// Validate interaction rules against known metrics
pub fn validate_interaction_rules(rules: &[InteractionRule], known_metrics: &[&str]) {
    use std::collections::HashSet;
    
    let mut seen_ids = HashSet::new();

    for rule in rules {
        // Unique id (affects cooldown key)
        assert!(
            seen_ids.insert(rule.id.clone()),
            "InteractionRule: duplicate id '{}'", rule.id
        );

        // max_distance >= 1
        assert!(
            rule.max_distance >= 1,
            "InteractionRule '{}': max_distance must be >= 1", rule.id
        );

        // border_type only valid values
        if let Some(ref bt) = rule.border_type {
            assert!(
                bt == "land" || bt == "sea",
                "InteractionRule '{}': invalid border_type '{}' (must be 'land' or 'sea')",
                rule.id, bt
            );
        }

        // event_type + event_threshold consistency
        if rule.event_type.is_some() {
            assert!(
                rule.event_threshold > 0.0,
                "InteractionRule '{}': event_threshold must be > 0 when event_type is set",
                rule.id
            );
        }

        // effects not empty
        assert!(
            !rule.effects.is_empty(),
            "InteractionRule '{}': effects must not be empty", rule.id
        );

        // known metrics in conditions
        for cond in &rule.conditions {
            assert!(
                known_metrics.contains(&cond.metric.as_str()),
                "InteractionRule '{}': unknown condition metric '{}'", rule.id, cond.metric
            );
        }

        // known metrics in effects
        for effect in &rule.effects {
            assert!(
                known_metrics.contains(&effect.metric.as_str()),
                "InteractionRule '{}': unknown effect metric '{}'", rule.id, effect.metric
            );
        }
    }
}

/// Validate patron actions
pub fn validate_patron_actions(actions: &[PatronAction], _known_metrics: &[&str]) {
    use std::collections::HashSet;
    
    let mut seen_ids = HashSet::new();
    for action in actions {
        // Unique id
        assert!(
            seen_ids.insert(action.id.clone()),
            "PatronAction: duplicate id '{}'", action.id
        );
        // effects not empty
        assert!(
            !action.effects.is_empty(),
            "PatronAction '{}': effects must not be empty", action.id
        );
        // Note: effects/costs metrics can be family:, actor:, global: prefixes
        // Full validation would require MetricRef parsing; skip for now
    }
}

/// Map polygon configuration for a specific actor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapPolygon {
    /// Actor ID this polygon represents
    pub actor_id: String,
    /// GeoJSON file name (e.g., "rome.geojson")
    pub geojson_file: String,
    /// Hex color code (e.g., "#8B0000")
    pub color: String,
    /// Opacity 0.0..=1.0
    pub opacity: f64,
}

/// Map configuration for scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapConfig {
    /// Tile layer URL template
    pub tile_url: String,
    /// Tile attribution (HTML-aware, rendered by Leaflet)
    pub tile_attribution: String,
    /// Center latitude
    pub center_lat: f64,
    /// Center longitude
    pub center_lon: f64,
    /// Default zoom level
    pub default_zoom: u32,
    /// Base path for GeoJSON files (e.g., "rome_375")
    pub geojson_base_path: String,
    /// Polygons to render
    pub polygons: Vec<MapPolygon>,
}

/// Validate map configuration
pub fn validate_map_config(config: &MapConfig, known_actor_ids: &[&str]) {
    use std::collections::HashSet;
    
    assert!(config.default_zoom > 0, "MapConfig: default_zoom must be > 0");
    assert!(!config.geojson_base_path.is_empty(), "MapConfig: geojson_base_path must not be empty");

    let mut seen_actor_ids = HashSet::new();
    for polygon in &config.polygons {
        assert!(
            (0.0..=1.0).contains(&polygon.opacity),
            "MapPolygon '{}': opacity must be 0.0..=1.0", polygon.actor_id
        );
        assert!(
            polygon.color.starts_with('#') && polygon.color.len() == 7,
            "MapPolygon '{}': color must be hex '#RRGGBB'", polygon.actor_id
        );
        assert!(
            !polygon.geojson_file.is_empty(),
            "MapPolygon '{}': geojson_file must not be empty", polygon.actor_id
        );
        assert!(
            known_actor_ids.contains(&polygon.actor_id.as_str()),
            "MapPolygon: unknown actor_id '{}'", polygon.actor_id
        );
        assert!(
            seen_actor_ids.insert(polygon.actor_id.clone()),
            "MapPolygon: duplicate actor_id '{}'", polygon.actor_id
        );
    }
}
