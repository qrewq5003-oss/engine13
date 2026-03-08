use std::collections::{HashMap, VecDeque};

use rand::Rng;
use rand_chacha::ChaCha8Rng;
use crate::core::{
    Actor, ActorDelta, ActorMetrics, ComparisonOperator, Event, EventConditionType, EventCondition,
    EventType, MetricRef, Scenario, WorldState,
};
use serde::Serialize;

mod interactions;

/// Tick explanation for debug mode
#[derive(Debug, Default, Serialize)]
pub struct TickExplanation {
    pub tick: u32,
    pub year: i32,
    pub auto_deltas_applied: Vec<DeltaEntry>,
    pub interactions_fired: Vec<InteractionEntry>,
    pub milestones_fired: Vec<MilestoneEntry>,
    pub random_events_fired: Vec<RandomEventEntry>,
    pub foreground_changes: Vec<ForegroundChange>,
    pub collapses: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DeltaEntry {
    pub metric: String,
    pub base_delta: f64,
    pub ratio_delta: f64,
    pub final_delta: f64,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct InteractionEntry {
    pub interaction_type: String,
    pub actor_a: String,
    pub actor_b: String,
    pub details: String,
}

#[derive(Debug, Serialize)]
pub struct MilestoneEntry {
    pub id: String,
    pub conditions_met: Vec<String>,
    pub effects_applied: HashMap<String, f64>,
}

#[derive(Debug, Serialize)]
pub struct RandomEventEntry {
    pub id: String,
    pub target: String,
    pub effects_applied: HashMap<String, f64>,
}

#[derive(Debug, Serialize)]
pub struct ForegroundChange {
    pub actor_id: String,
    pub reason: String,
}

/// Coefficients for dependency graph relationships
#[derive(Debug, Clone, Copy)]
pub struct DependencyCoefficients {
    // legitimacy ↓10 → cohesion ↓3
    pub legitimacy_to_cohesion: f64,
    // cohesion ↓10 → legitimacy ↓2
    pub cohesion_to_legitimacy: f64,
    // legitimacy ↓10 → military_quality ↓2
    pub legitimacy_to_military_quality: f64,
    // cohesion ↓10 → economic_output ↓3
    pub cohesion_to_economic_output: f64,
    // external_pressure ↑10 → cohesion ↓2
    pub external_pressure_to_cohesion: f64,
    // external_pressure ↑10 → legitimacy ↓1
    pub external_pressure_to_legitimacy: f64,
    // external_pressure ↑10 → military_quality ↓2
    pub external_pressure_to_military_quality: f64,
    // external_pressure ↑10 → military_size ↓1
    pub external_pressure_to_military_size: f64,
    // economic_output ↓10 → treasury ↓15
    pub economic_output_to_treasury: f64,
    // military_size ↓10 → economic_output ↓1
    pub military_size_to_economic_output: f64,
    // population ↑5000 → economic_output ↑0.5
    pub population_to_economic_output: f64,
    // economic_output ↓10 → population ↓200
    pub economic_output_to_population: f64,
    // cohesion bonus when external_pressure > 65 AND legitimacy > 60
    pub cohesion_bonus_value: f64,
    // legitimacy < 20 → military_quality falls -0.5/tick
    pub low_legitimacy_military_quality_decay: f64,
    // economic_output < 15 → population falls -100/tick
    pub low_economic_output_population_decay: f64,
}

impl Default for DependencyCoefficients {
    fn default() -> Self {
        Self {
            legitimacy_to_cohesion: 0.03,
            cohesion_to_legitimacy: 0.02,
            legitimacy_to_military_quality: 0.02,
            cohesion_to_economic_output: 0.03,
            external_pressure_to_cohesion: 0.02,
            external_pressure_to_legitimacy: 0.01,
            external_pressure_to_military_quality: 0.02,
            external_pressure_to_military_size: 0.01,
            economic_output_to_treasury: 0.15,
            military_size_to_economic_output: 0.01,
            population_to_economic_output: 0.00005,
            economic_output_to_population: 20.0,
            cohesion_bonus_value: 5.0,
            low_legitimacy_military_quality_decay: 0.5,
            low_economic_output_population_decay: 100.0,
        }
    }
}

/// Thresholds for dependency graph effects
#[derive(Debug, Clone, Copy)]
pub struct DependencyThresholds {
    pub legitimacy_low: f64,           // 50.0
    pub cohesion_low: f64,             // 50.0
    pub external_pressure_high: f64,   // 50.0
    pub external_pressure_critical: f64, // 65.0
    pub economic_output_low: f64,      // 50.0
    pub military_size_low: f64,        // 50.0
    pub population_high: f64,          // 3000.0
    pub legitimacy_critical: f64,      // 20.0
    pub economic_output_critical: f64, // 15.0
}

impl Default for DependencyThresholds {
    fn default() -> Self {
        Self {
            legitimacy_low: 50.0,
            cohesion_low: 50.0,
            external_pressure_high: 50.0,
            external_pressure_critical: 65.0,
            economic_output_low: 50.0,
            military_size_low: 50.0,
            population_high: 3000.0,
            legitimacy_critical: 20.0,
            economic_output_critical: 15.0,
        }
    }
}

/// Event log for recording simulation events
#[derive(Debug, Clone, Default)]
pub struct EventLog {
    pub events: Vec<Event>,
}

impl EventLog {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn add(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Main simulation tick function
///
/// Canonical 8-phase pipeline:
/// 1. Auto-deltas via MetricRef (treasury, scenario metrics)
/// 2. Dependency graph and interactions
/// 3. Actor tag effects
/// 4. Clamp metrics to bounds
/// 5. Events: thresholds, ranks, milestones, game mode, relevance
/// 6. Actor collapses
/// 7. Record changes and generation mechanics
/// 8. Advance tick state
pub fn tick(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog) {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    let current_tick = world.tick;
    let current_year = world.year;

    // Initialize RNG once at the start of the tick
    let mut rng = ChaCha8Rng::from_seed(world.rng_state);

    // Store initial state for event comparison
    let initial_states: HashMap<String, ActorMetrics> = world
        .actors
        .iter()
        .map(|(id, actor)| (id.clone(), actor.metrics.clone()))
        .collect();

    // Phase 1: Auto-deltas via MetricRef
    phase_auto_deltas(world, scenario, &mut rng);

    // Phase 2: Dependency graph and interactions
    phase_interactions(world, scenario, event_log, &mut rng);

    // Phase 3: Random events
    phase_random_events(world, scenario, event_log, &mut rng);

    // Phase 4: Actor tag effects
    phase_actor_tags(world, scenario);

    // Phase 5: Clamp metrics
    phase_clamp(world);

    // Phase 6: Events (thresholds, ranks, milestones, game mode, relevance)
    phase_events(world, scenario, event_log);

    // Phase 7: Actor collapses
    phase_collapses(world, scenario, event_log);

    // Phase 8: Record changes and generation mechanics
    phase_record(world, scenario, &initial_states, current_tick, current_year, event_log);

    // Phase 9: Advance tick state
    phase_advance(world, scenario, &mut rng);
}

// ============================================================================
// Phase 1: Auto-deltas via MetricRef
// ============================================================================

fn phase_auto_deltas(world: &mut WorldState, scenario: &Scenario, rng: &mut rand_chacha::ChaCha8Rng) {
    // Treasury via income/expenses formula (separate from auto_deltas)
    apply_treasury(world);

    // Apply auto_deltas via MetricRef - unified for actor/family/global
    for auto_delta in &scenario.auto_deltas {
        // Check conditions
        let mut delta = auto_delta.base;
        for cond in &auto_delta.conditions {
            if check_auto_delta_condition(world, cond) {
                delta += cond.delta;
            }
        }

        // Check ratio conditions
        for ratio_cond in &auto_delta.ratio_conditions {
            let val_a = crate::core::MetricRef::parse(&ratio_cond.metric_a).get(world);
            let val_b = crate::core::MetricRef::parse(&ratio_cond.metric_b).get(world);
            
            if val_b == 0.0 {
                continue;
            }
            
            let actual_ratio = val_a / val_b;
            let condition_met = ratio_cond.operator.evaluate(actual_ratio, ratio_cond.ratio);
            
            if condition_met {
                delta += ratio_cond.delta;
            }
        }

        // Apply noise
        let noise = (rng.gen::<f64>() - 0.5) * 2.0 * auto_delta.noise;
        let final_delta = delta + noise;

        // Apply via MetricRef - single path for all metric types
        MetricRef::parse(&auto_delta.metric).apply(world, final_delta);
    }
}

/// Check auto_delta condition against world state
fn check_auto_delta_condition(world: &WorldState, cond: &crate::core::DeltaCondition) -> bool {
    let value = MetricRef::parse(&cond.metric).get(world);
    match cond.operator {
        crate::core::ComparisonOperator::Less => value < cond.value,
        crate::core::ComparisonOperator::LessOrEqual => value <= cond.value,
        crate::core::ComparisonOperator::Greater => value > cond.value,
        crate::core::ComparisonOperator::GreaterOrEqual => value >= cond.value,
        crate::core::ComparisonOperator::Equal => (value - cond.value).abs() < 0.001,
    }
}

// ============================================================================
// Phase 2: Dependency graph and interactions
// ============================================================================

fn phase_interactions(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog, rng: &mut ChaCha8Rng) {
    // Apply dependency graph coefficients
    apply_dependency_graph(world);

    // Calculate neighbor interactions (six types: military, trade, diplomatic, migration, vassalage, cultural)
    interactions::calculate_interactions(world, scenario, event_log, rng);
}

// ============================================================================
// Phase 3: Random events
// ============================================================================

fn phase_random_events(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
    rng: &mut rand_chacha::ChaCha8Rng,
) {
    use rand::seq::SliceRandom;

    // Combine common events with scenario-specific events
    let all_events: Vec<crate::core::RandomEvent> = crate::events::common_events()
        .into_iter()
        .chain(scenario.random_events.iter().cloned())
        .collect();

    // Get sea actor IDs for SeaActors target
    let sea_actor_ids: std::collections::HashSet<String> = scenario.actors.iter()
        .filter(|a| a.tags.contains(&"maritime".to_string()) || a.tags.contains(&"trade_empire".to_string()))
        .map(|a| a.id.clone())
        .collect();

    // Get foreground actor IDs
    let foreground_ids: Vec<String> = world.actors.values()
        .filter(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground && !world.dead_actor_ids.contains(&a.id))
        .map(|a| a.id.clone())
        .collect();

    for event in &all_events {
        // Skip one-time events that already fired
        if event.one_time && world.fired_events.contains(&event.id) {
            continue;
        }

        // Roll for event probability
        if rng.gen::<f64>() > event.probability {
            continue;
        }

        // Determine target actor(s)
        let target_ids: Vec<String> = match &event.target {
            crate::core::EventTarget::Actor(id) => {
                if world.actors.contains_key(id) && !world.dead_actor_ids.contains(id) {
                    vec![id.clone()]
                } else {
                    vec![]
                }
            },
            crate::core::EventTarget::Any => {
                foreground_ids.choose(rng).cloned().into_iter().collect()
            },
            crate::core::EventTarget::SeaActors => {
                let sea_foreground: Vec<&String> = foreground_ids.iter()
                    .filter(|id| sea_actor_ids.contains(*id))
                    .collect();
                sea_foreground.choose(rng).cloned().cloned().into_iter().collect()
            },
            crate::core::EventTarget::All => foreground_ids.clone(),
        };

        if target_ids.is_empty() {
            continue;
        }

        // Check conditions for each target
        for target_id in &target_ids {
            let conditions_met = event.conditions.iter().all(|cond| {
                let metric = cond.metric.replace("self.", &format!("{}.", target_id));
                let value = crate::core::MetricRef::parse(&metric).get(world);
                cond.operator.evaluate(value, cond.value)
            });

            if !conditions_met {
                continue;
            }

            // Apply effects
            for (metric, delta) in &event.effects {
                let resolved = metric.replace("self.", &format!("{}.", target_id));
                crate::core::MetricRef::parse(&resolved).apply(world, *delta);
            }

            // Record event
            let event_record = crate::core::Event::new(
                event.id.clone(),
                world.tick,
                world.year,
                target_id.clone(),
                crate::core::EventType::Threshold,
                true,
                event.llm_context.clone(),
            );
            event_log.add(event_record);

            // Mark one-time event as fired
            if event.one_time {
                world.fired_events.insert(event.id.clone());
            }
        }
    }
}

// ============================================================================
// Phase 4: Actor tag effects
// ============================================================================

fn phase_actor_tags(world: &mut WorldState, scenario: &Scenario) {
    apply_actor_tags(world, scenario);
}

// ============================================================================
// Phase 4: Clamp metrics
// ============================================================================

fn phase_clamp(world: &mut WorldState) {
    clamp_metrics(world);
}

// ============================================================================
// Phase 5: Events (thresholds, ranks, milestones, game mode, relevance)
// ============================================================================

fn phase_events(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog) {
    check_threshold_effects(world, scenario, event_log);
    check_rank_conditions(world, scenario, event_log);
    check_milestone_events(world, scenario, event_log);
    check_game_mode_transitions(world, scenario, event_log);
    check_relevance_thresholds(world, scenario, event_log);
}

// ============================================================================
// Phase 6: Actor collapses
// ============================================================================

fn phase_collapses(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog) {
    check_collapses(world, scenario, event_log);
}

// ============================================================================
// Phase 7: Record changes and generation mechanics
// ============================================================================

fn phase_record(world: &mut WorldState, scenario: &Scenario, initial_states: &HashMap<String, ActorMetrics>, current_tick: u32, current_year: i32, event_log: &mut EventLog) {
    record_metric_changes(world, initial_states, current_tick, current_year, event_log);
    check_generation_transfer(world, scenario, event_log);
    update_metric_history(world);
    update_prev_metrics(world);
    world.ticks_since_last_narrative += 1;
}

// ============================================================================
// Phase 8: Advance tick state
// ============================================================================

fn phase_advance(world: &mut WorldState, scenario: &Scenario, rng: &mut rand_chacha::ChaCha8Rng) {
    world.rng_state = rng.get_seed();
    world.tick += 1;
    world.year += scenario.tick_span as i32;
}

fn apply_treasury(world: &mut WorldState) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            let incomes = actor.metrics.economic_output * actor.metrics.population * 0.001;
            let expenses = actor.metrics.military_size * 0.8;
            actor.metrics.treasury += incomes - expenses;
        }
    }
}

// Helper functions for metric operations
fn get_metric_value(metrics: &ActorMetrics, name: &str) -> f64 {
    match name {
        "population" => metrics.population,
        "military_size" => metrics.military_size,
        "military_quality" => metrics.military_quality,
        "economic_output" => metrics.economic_output,
        "cohesion" => metrics.cohesion,
        "legitimacy" => metrics.legitimacy,
        "external_pressure" => metrics.external_pressure,
        "treasury" => metrics.treasury,
        _ => 0.0,
    }
}

fn apply_metric_delta(metrics: &mut ActorMetrics, metric: &str, delta: f64) {
    match metric {
        "population" => metrics.population += delta,
        "military_size" => metrics.military_size += delta,
        "military_quality" => metrics.military_quality += delta,
        "economic_output" => metrics.economic_output += delta,
        "cohesion" => metrics.cohesion += delta,
        "legitimacy" => metrics.legitimacy += delta,
        "external_pressure" => metrics.external_pressure += delta,
        "treasury" => metrics.treasury += delta,
        _ => {}
    }
}

// ============================================================================
// Step 2: Dependency Graph
// ============================================================================

const COEF: DependencyCoefficients = DependencyCoefficients {
    legitimacy_to_cohesion: 0.03,
    cohesion_to_legitimacy: 0.02,
    legitimacy_to_military_quality: 0.02,
    cohesion_to_economic_output: 0.03,
    external_pressure_to_cohesion: 0.02,
    external_pressure_to_legitimacy: 0.01,
    external_pressure_to_military_quality: 0.02,
    external_pressure_to_military_size: 0.01,
    economic_output_to_treasury: 0.15,
    military_size_to_economic_output: 0.01,
    population_to_economic_output: 0.00005,
    economic_output_to_population: 20.0,
    cohesion_bonus_value: 5.0,
    low_legitimacy_military_quality_decay: 0.5,
    low_economic_output_population_decay: 100.0,
};

const THRESH: DependencyThresholds = DependencyThresholds {
    legitimacy_low: 50.0,
    cohesion_low: 50.0,
    external_pressure_high: 50.0,
    external_pressure_critical: 65.0,
    economic_output_low: 50.0,
    military_size_low: 50.0,
    population_high: 3000.0,
    legitimacy_critical: 20.0,
    economic_output_critical: 15.0,
};

fn apply_dependency_graph(world: &mut WorldState) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            // Early exit: skip actors already marked for removal
            // (cohesion < 10 OR legitimacy < 5 per check_collapses)
            if actor.metrics.cohesion < 10.0 || actor.metrics.legitimacy < 5.0 {
                continue;
            }

            let metrics = actor.metrics.clone();

            // legitimacy ↓10 → cohesion ↓3
            if metrics.legitimacy < THRESH.legitimacy_low {
                let deficit = THRESH.legitimacy_low - metrics.legitimacy;
                actor.metrics.cohesion -= deficit * COEF.legitimacy_to_cohesion;
            }

            // cohesion ↓10 → legitimacy ↓2
            if metrics.cohesion < THRESH.cohesion_low {
                let deficit = THRESH.cohesion_low - metrics.cohesion;
                actor.metrics.legitimacy -= deficit * COEF.cohesion_to_legitimacy;
            }

            // legitimacy ↓10 → military_quality ↓2
            if metrics.legitimacy < THRESH.legitimacy_low {
                let deficit = THRESH.legitimacy_low - metrics.legitimacy;
                actor.metrics.military_quality -= deficit * COEF.legitimacy_to_military_quality;
            }

            // cohesion ↓10 → economic_output ↓3
            if metrics.cohesion < THRESH.cohesion_low {
                let deficit = THRESH.cohesion_low - metrics.cohesion;
                actor.metrics.economic_output -= deficit * COEF.cohesion_to_economic_output;
            }

            // external_pressure ↑10 → cohesion ↓2
            if metrics.external_pressure > THRESH.external_pressure_high {
                let excess = metrics.external_pressure - THRESH.external_pressure_high;
                actor.metrics.cohesion -= excess * COEF.external_pressure_to_cohesion;
            }

            // external_pressure ↑10 → legitimacy ↓1
            if metrics.external_pressure > THRESH.external_pressure_high {
                let excess = metrics.external_pressure - THRESH.external_pressure_high;
                actor.metrics.legitimacy -= excess * COEF.external_pressure_to_legitimacy;
            }

            // external_pressure ↑10 → military_quality ↓2
            if metrics.external_pressure > THRESH.external_pressure_high {
                let excess = metrics.external_pressure - THRESH.external_pressure_high;
                actor.metrics.military_quality -= excess * COEF.external_pressure_to_military_quality;
            }

            // external_pressure ↑10 → military_size ↓1
            if metrics.external_pressure > THRESH.external_pressure_high {
                let excess = metrics.external_pressure - THRESH.external_pressure_high;
                actor.metrics.military_size -= excess * COEF.external_pressure_to_military_size;
            }

            // economic_output ↓10 → treasury ↓15
            if metrics.economic_output < THRESH.economic_output_low {
                let deficit = THRESH.economic_output_low - metrics.economic_output;
                actor.metrics.treasury -= deficit * COEF.economic_output_to_treasury;
            }

            // military_size ↓10 → economic_output ↓1
            if metrics.military_size < THRESH.military_size_low {
                let deficit = THRESH.military_size_low - metrics.military_size;
                actor.metrics.economic_output -= deficit * COEF.military_size_to_economic_output;
            }

            // population ↑5000 → economic_output ↑0.5 (Rome-scale populations)
            if metrics.population > THRESH.population_high {
                actor.metrics.economic_output +=
                    (metrics.population - THRESH.population_high) * COEF.population_to_economic_output;
            }

            // economic_output ↓10 → population ↓200
            if metrics.economic_output < THRESH.economic_output_low {
                let deficit = THRESH.economic_output_low - metrics.economic_output;
                actor.metrics.population -= deficit * COEF.economic_output_to_population;
            }

            // Cohesion bonus effect (external_pressure > 65 AND legitimacy > 60)
            if metrics.external_pressure > THRESH.external_pressure_critical
                && metrics.legitimacy > 60.0
            {
                actor.metrics.cohesion += COEF.cohesion_bonus_value;
            }

            // Threshold effects
            // legitimacy < 20 → military_quality falls -0.5/tick
            if metrics.legitimacy < THRESH.legitimacy_critical {
                actor.metrics.military_quality -= COEF.low_legitimacy_military_quality_decay;
            }

            // economic_output < 15 → population falls -100/tick
            if metrics.economic_output < THRESH.economic_output_critical {
                actor.metrics.population -= COEF.low_economic_output_population_decay;
            }
        }
    }
}

// ============================================================================
// Step 3: Neighbor Interactions
// ============================================================================
// Step 4: Actor Tags Effects
// ============================================================================

fn apply_actor_tags(world: &mut WorldState, _scenario: &Scenario) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            for (_tag_id, actor_tag) in &actor.actor_tags {
                for (metric, modifier) in &actor_tag.metrics_modifier {
                    apply_metric_delta(&mut actor.metrics, metric, *modifier as f64);
                }
            }
            // Note: No clamping here - clamp_metrics is called on step 5
        }
    }
}

// ============================================================================
// Step 5: Clamp Metrics
// ============================================================================

fn clamp_metrics(world: &mut WorldState) {
    for actor in world.actors.values_mut() {
        actor.metrics.population = actor.metrics.population.max(0.0);
        actor.metrics.military_size = actor.metrics.military_size.max(0.0);
        actor.metrics.military_quality = actor.metrics.military_quality.max(0.0).min(100.0);
        actor.metrics.economic_output = actor.metrics.economic_output.max(0.0).min(100.0);
        actor.metrics.cohesion = actor.metrics.cohesion.max(0.0).min(100.0);
        actor.metrics.legitimacy = actor.metrics.legitimacy.max(0.0).min(100.0);
        actor.metrics.external_pressure = actor.metrics.external_pressure.max(0.0).min(100.0);
        // Treasury can be negative
    }
}

// ============================================================================
// Step 6: Threshold Effects, Rank Conditions, Milestone Events
// ============================================================================

fn check_threshold_effects(
    world: &mut WorldState,
    _scenario: &Scenario,
    event_log: &mut EventLog,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    for actor in world.actors.values() {
        // cohesion < 25 → any legitimacy fall is doubled
        if actor.metrics.cohesion < 25.0 {
            // This is handled in the dependency graph step
            // Here we just log if critical
            if actor.metrics.legitimacy < 30.0 {
                let event = Event::new(
                    format!("threshold_{}_low_cohesion", actor.id),
                    current_tick,
                    current_year,
                    actor.id.clone(),
                    EventType::Threshold,
                    false,
                    format!(
                        "{}: критически низкая сплочённость ({:.1}) угрожает стабильности",
                        actor.name_short, actor.metrics.cohesion
                    ),
                );
                event_log.add(event);
            }
        }

        // external_pressure > 80 → trigger migration for neighbors
        if actor.metrics.external_pressure > 80.0 {
            for neighbor in &actor.neighbors {
                if let Some(neighbor_actor) = world.actors.get(&neighbor.id) {
                    if neighbor_actor.metrics.external_pressure < 50.0 {
                        // Neighbor will receive migration pressure
                    }
                }
            }
        }
    }
}

fn check_rank_conditions(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    for rank_cond in &scenario.rank_conditions {
        let should_trigger = match &rank_cond.condition.condition_type {
            EventConditionType::Metric {
                metric,
                actor_id,
                operator,
                value,
            } => {
                if let Some(aid) = actor_id {
                    if let Some(actor) = world.actors.get(aid) {
                        let actor_value = get_metric_value(&actor.metrics, metric);
                        compare(actor_value, operator, value)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            EventConditionType::ActorState { actor_id, state } => match state {
                crate::core::ActorState::Dead => !world.is_actor_alive(actor_id),
                crate::core::ActorState::Alive => world.is_actor_alive(actor_id),
                crate::core::ActorState::Foreground => world
                    .actors
                    .get(actor_id)
                    .map(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground)
                    .unwrap_or(false),
                crate::core::ActorState::Background => world
                    .actors
                    .get(actor_id)
                    .map(|a| a.narrative_status == crate::core::NarrativeStatus::Background)
                    .unwrap_or(false),
            },
            EventConditionType::Tick { tick } => current_tick >= *tick,
        };

        if should_trigger {
            // Apply rank change (note: this would need region tracking)
            if rank_cond.is_key {
                let event = Event::new(
                    format!("rank_{}_{}", rank_cond.region_id, rank_cond.result.rank),
                    current_tick,
                    current_year,
                    rank_cond.region_id.clone(),
                    EventType::Threshold,
                    true,
                    format!(
                        "Регион {} изменил ранг на {}",
                        rank_cond.region_id, rank_cond.result.rank
                    ),
                );
                event_log.add(event);
            }
        }
    }
}

fn compare(value: f64, operator: &ComparisonOperator, target: &f64) -> bool {
    match operator {
        ComparisonOperator::Less => value < *target,
        ComparisonOperator::LessOrEqual => value <= *target,
        ComparisonOperator::Greater => value > *target,
        ComparisonOperator::GreaterOrEqual => value >= *target,
        ComparisonOperator::Equal => (value - target).abs() < 0.001,
    }
}

fn check_milestone_events(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    for milestone in &scenario.milestone_events {
        // Skip if already fired (one-time)
        if world.milestone_events_fired.contains(&milestone.id) {
            continue;
        }

        // Check cooldown
        if let Some(cooldown) = milestone.cooldown_ticks {
            if let Some(last_tick) = world.milestone_cooldowns.get(&milestone.id) {
                if current_tick - last_tick < cooldown {
                    continue;  // Still on cooldown
                }
            }
        }

        // Outcome milestones require tick >= 20 to fire
        if milestone.id.starts_with("outcome_") && current_tick < 20 {
            continue;
        }

        let condition_met = check_event_condition(world, &milestone.condition);

        // Handle duration: condition must be met for `duration` consecutive ticks
        let should_trigger = if let Some(duration) = milestone.condition.duration {
            let counter = world.milestone_condition_ticks.entry(milestone.id.clone()).or_insert(0);

            if condition_met {
                *counter += 1;
                *counter >= duration
            } else {
                // Reset counter if condition is not met
                *counter = 0;
                false
            }
        } else {
            // No duration specified - trigger immediately when condition is met
            condition_met
        };

        if should_trigger {
            world.milestone_events_fired.push(milestone.id.clone());
            world.milestone_cooldowns.insert(milestone.id.clone(), current_tick);

            // Log milestone firing
            eprintln!("[MILESTONE] {} fired at year {}", milestone.id, current_year);

            // Apply one-time effects for specific milestones
            apply_milestone_effects(world, &milestone.id);

            let event_type = if milestone.triggers_collapse {
                EventType::Collapse
            } else {
                EventType::Milestone
            };

            let event = Event::new(
                milestone.id.clone(),
                current_tick,
                current_year,
                "scenario".to_string(),
                event_type,
                milestone.is_key,
                milestone.llm_context_shift.clone(),
            );
            event_log.add(event);
        }
    }
}

/// Apply one-time effects for specific milestone events
fn apply_milestone_effects(world: &mut WorldState, milestone_id: &str) {
    match milestone_id {
        "mehmed_accelerates" => {
            // Ottoman response: all-in acceleration
            // military_quality -15, treasury -200, cohesion -10
            if let Some(ottomans) = world.actors.get_mut("ottomans") {
                ottomans.metrics.military_quality = (ottomans.metrics.military_quality - 15.0).max(0.0);
                ottomans.metrics.treasury -= 200.0;
                ottomans.metrics.cohesion = (ottomans.metrics.cohesion - 10.0).max(0.0);
            }
        }
        _ => {}
    }
}

/// Check and handle game mode transitions
/// Scenario → Consequences: automatic when milestone with triggers_collapse fires
fn check_game_mode_transitions(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
) {
    // Only transition from Scenario to Consequences
    if world.game_mode != crate::core::GameMode::Scenario {
        return;
    }
    
    // Check if any milestone with triggers_collapse fired this tick
    for milestone in &scenario.milestone_events {
        if world.milestone_events_fired.contains(&milestone.id) 
            && milestone.triggers_collapse 
        {
            // Transition to Consequences mode
            world.game_mode = crate::core::GameMode::Consequences;
            
            // Record the mode change event
            let event = Event::new(
                "game_mode_consequences".to_string(),
                world.tick,
                world.year,
                "scenario".to_string(),
                EventType::Milestone,
                true,
                "Сценарий завершён. Симуляция продолжается в режиме последствий.".to_string(),
            );
            event_log.add(event);
            
            eprintln!("[GAME_MODE] Transitioned to Consequences mode at tick {}", world.tick);
            return; // Only one transition per tick
        }
    }
}

/// Check relevance thresholds for actors to move between foreground and background
/// Implements architecture rules for actor relevance
fn check_relevance_thresholds(
    world: &mut WorldState,
    _scenario: &Scenario,
    event_log: &mut EventLog,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    // Calculate average power projection for all active actors
    let avg_power_projection: f64 = world.actors.values()
        .map(|a| a.power_projection(1.0))
        .sum::<f64>() / world.actors.len().max(1) as f64;

    // Get list of narrative actor IDs for contact check (collect as owned Strings to avoid borrow issues)
    let narrative_actor_ids: Vec<String> = world.actors.iter()
        .filter(|(_, a)| a.narrative_status == crate::core::NarrativeStatus::Foreground)
        .map(|(id, _)| id.clone())
        .collect();

    // Check each background actor for potential promotion to foreground
    let mut to_promote: Vec<String> = Vec::new();

    for (actor_id, actor) in &world.actors {
        if actor.narrative_status != crate::core::NarrativeStatus::Background {
            continue; // Already foreground
        }

        let power_proj = actor.power_projection(1.0);

        // Condition 1: Power projection > 70% of average
        let condition_power = power_proj > avg_power_projection * 0.7;

        // Condition 2: Contact with narrative actor (simplified - military pressure only)
        // TODO: Full implementation should check trade, culture, migration interactions
        // For now, use external_pressure as proxy for military pressure from narrative actors
        let condition_contact = narrative_actor_ids.iter()
            .filter(|narr_id| narr_id.as_str() != actor_id.as_str())
            .any(|narr_id| {
                if let Some(_narr_actor) = world.actors.get(narr_id) {
                    // Check if this actor has high external_pressure (proxy for military pressure)
                    // and the narrative actor is the source
                    // Simplified: just check if actor's external_pressure is high
                    actor.metrics.external_pressure > 40.0 && power_proj > 50.0
                } else {
                    false
                }
            });

        // Condition 3: Internal upheaval
        // Check if any metric changed by >30 in last 5 ticks
        let condition_upheaval = check_actor_upheaval(world, actor_id)
            || actor.metrics.cohesion < 25.0
            || actor.metrics.legitimacy < 20.0;

        if condition_power || condition_contact || condition_upheaval {
            let mut reasons = Vec::new();
            if condition_power {
                reasons.push(format!("power_projection {:.0} > 70% avg {:.0}", power_proj, avg_power_projection * 0.7));
            }
            if condition_contact {
                reasons.push("military contact with narrative actor".to_string());
            }
            if condition_upheaval {
                reasons.push("internal upheaval".to_string());
            }

            to_promote.push(actor_id.clone());

            // Record event
            let event = Event::new(
                format!("foreground_{}", actor_id),
                current_tick,
                current_year,
                actor_id.clone(),
                EventType::Threshold,
                true,
                format!("{} вышел на передний план: {}", actor.name, reasons.join(", ")),
            );
            event_log.add(event);

            eprintln!("[THRESHOLD] Actor {} gained foreground status: {}", actor_id, reasons.join(", "));
        }
    }

    // Apply promotions
    for actor_id in &to_promote {
        if let Some(actor) = world.actors.get_mut(actor_id) {
            actor.narrative_status = crate::core::NarrativeStatus::Foreground;
        }
        // Reset upheaval counter
        world.actor_upheaval_ticks.insert(actor_id.clone(), 0);
    }

    // Check foreground actors for potential demotion to background
    let mut to_demote: Vec<String> = Vec::new();

    for (actor_id, actor) in &world.actors {
        if actor.narrative_status != crate::core::NarrativeStatus::Foreground {
            continue; // Already background
        }

        let power_proj = actor.power_projection(1.0);

        // Condition for return to background:
        // power_projection < 40% of average
        // AND no active interactions with narrative actors
        // AND no internal upheaval for 10+ ticks
        let low_power = power_proj < avg_power_projection * 0.4;

        // Check for recent upheaval
        let recent_upheaval = world.actor_upheaval_ticks.get(actor_id).copied().unwrap_or(0) < 10;

        // Check for interactions with narrative actors (simplified)
        let has_narrative_contact = narrative_actor_ids.iter()
            .filter(|&narr_id| narr_id != actor_id)
            .any(|narr_id| {
                if let Some(narr_actor) = world.actors.get(narr_id) {
                    // Simplified: check if either actor has high external_pressure
                    actor.metrics.external_pressure > 30.0 || narr_actor.metrics.external_pressure > 30.0
                } else {
                    false
                }
            });

        if low_power && !has_narrative_contact && !recent_upheaval {
            to_demote.push(actor_id.clone());

            let event = Event::new(
                format!("background_{}", actor_id),
                current_tick,
                current_year,
                actor_id.clone(),
                EventType::Threshold,
                false,
                format!("{} вернулся в фон: низкая релевантность", actor.name),
            );
            event_log.add(event);

            eprintln!("[THRESHOLD] Actor {} lost foreground status: low relevance", actor_id);
        }
    }

    // Apply demotions
    for actor_id in &to_demote {
        if let Some(actor) = world.actors.get_mut(actor_id) {
            actor.narrative_status = crate::core::NarrativeStatus::Background;
        }
    }
}

/// Check if an actor has had a metric change of >30 in the last 5 ticks
fn check_actor_upheaval(world: &WorldState, actor_id: &str) -> bool {
    // Check all metrics for this actor
    let metrics_to_check = [
        "population", "military_size", "military_quality", "economic_output",
        "cohesion", "legitimacy", "external_pressure", "treasury",
    ];

    for metric in &metrics_to_check {
        let key = format!("{}:{}", actor_id, metric);
        if let Some(history) = world.metric_history.get(&key) {
            if history.len() >= 2 {
                let oldest = history.front().copied().unwrap_or(0.0);
                let newest = history.back().copied().unwrap_or(0.0);
                if (newest - oldest).abs() > 30.0 {
                    return true;
                }
            }
        }
    }

    false
}

/// Update metric history for all actors (called at end of tick)
fn update_metric_history(world: &mut WorldState) {
    let max_history_len = 5;

    for (actor_id, actor) in &world.actors {
        // Update history for each metric
        let metrics = [
            ("population", actor.metrics.population),
            ("military_size", actor.metrics.military_size),
            ("military_quality", actor.metrics.military_quality),
            ("economic_output", actor.metrics.economic_output),
            ("cohesion", actor.metrics.cohesion),
            ("legitimacy", actor.metrics.legitimacy),
            ("external_pressure", actor.metrics.external_pressure),
            ("treasury", actor.metrics.treasury),
        ];

        for (metric_name, value) in &metrics {
            let key = format!("{}:{}", actor_id, metric_name);
            let history = world.metric_history.entry(key).or_insert_with(VecDeque::new);
            history.push_back(*value);

            // Keep only last 5 ticks
            while history.len() > max_history_len {
                history.pop_front();
            }
        }
    }

    // Update upheaval counters for all actors
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();
    for actor_id in actor_ids {
        let has_upheaval = check_actor_upheaval(world, &actor_id);
        let counter = world.actor_upheaval_ticks.entry(actor_id).or_insert(0);
        if has_upheaval {
            *counter = 0; // Reset on upheaval
        } else {
            *counter += 1; // Increment otherwise
        }
    }
}

/// Update prev_metrics for all actors (called at end of tick, after all changes applied)
fn update_prev_metrics(world: &mut WorldState) {
    for (actor_id, actor) in &world.actors {
        world.prev_metrics.insert(actor_id.clone(), actor.metrics.clone());
    }
}

/// Calculate actor deltas by comparing current metrics with prev_metrics
pub fn calculate_actor_deltas(world: &WorldState) -> Vec<ActorDelta> {
    use std::collections::HashMap;

    let mut deltas = Vec::new();

    for (actor_id, actor) in &world.actors {
        if let Some(prev) = world.prev_metrics.get(actor_id) {
            let mut metric_changes = HashMap::new();

            // Calculate delta for each metric
            let pop_delta = actor.metrics.population - prev.population;
            if pop_delta.abs() > 0.01 {
                metric_changes.insert("population".to_string(), pop_delta);
            }

            let mil_delta = actor.metrics.military_size - prev.military_size;
            if mil_delta.abs() > 0.01 {
                metric_changes.insert("military_size".to_string(), mil_delta);
            }

            let qual_delta = actor.metrics.military_quality - prev.military_quality;
            if qual_delta.abs() > 0.01 {
                metric_changes.insert("military_quality".to_string(), qual_delta);
            }

            let econ_delta = actor.metrics.economic_output - prev.economic_output;
            if econ_delta.abs() > 0.01 {
                metric_changes.insert("economic_output".to_string(), econ_delta);
            }

            let coh_delta = actor.metrics.cohesion - prev.cohesion;
            if coh_delta.abs() > 0.01 {
                metric_changes.insert("cohesion".to_string(), coh_delta);
            }

            let leg_delta = actor.metrics.legitimacy - prev.legitimacy;
            if leg_delta.abs() > 0.01 {
                metric_changes.insert("legitimacy".to_string(), leg_delta);
            }

            let pres_delta = actor.metrics.external_pressure - prev.external_pressure;
            if pres_delta.abs() > 0.01 {
                metric_changes.insert("external_pressure".to_string(), pres_delta);
            }

            let treas_delta = actor.metrics.treasury - prev.treasury;
            if treas_delta.abs() > 0.01 {
                metric_changes.insert("treasury".to_string(), treas_delta);
            }

            if !metric_changes.is_empty() {
                deltas.push(ActorDelta {
                    actor_id: actor_id.clone(),
                    actor_name: actor.name.clone(),
                    metric_changes,
                });
            }
        }
    }

    deltas
}

fn check_event_condition(world: &WorldState, condition: &EventCondition) -> bool {
    match &condition.condition_type {
        EventConditionType::Metric {
            metric,
            actor_id,
            operator,
            value,
        } => {
            if let Some(aid) = actor_id {
                if let Some(actor) = world.actors.get(aid) {
                    let actor_value = get_metric_value(&actor.metrics, metric);
                    compare(actor_value, operator, value)
                } else {
                    false
                }
            } else {
                false
            }
        }
        EventConditionType::ActorState { actor_id, state } => match state {
            crate::core::ActorState::Dead => !world.is_actor_alive(actor_id),
            crate::core::ActorState::Alive => world.is_actor_alive(actor_id),
            crate::core::ActorState::Foreground => world
                .actors
                .get(actor_id)
                .map(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground)
                .unwrap_or(false),
            crate::core::ActorState::Background => world
                .actors
                .get(actor_id)
                .map(|a| a.narrative_status == crate::core::NarrativeStatus::Background)
                .unwrap_or(false),
        },
        EventConditionType::Tick { tick } => world.tick >= *tick,
    }
}

// ============================================================================
// Generation Transfer (Patriarch Aging)
// ============================================================================

/// Check and handle generation transfer for the family patriarch
fn check_generation_transfer(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
) {
    let Some(gen_mechanics) = &scenario.generation_mechanics else {
        return; // No generation mechanics defined for this scenario
    };

    let current_tick = world.tick;
    let current_year = world.year;

    // Get current patriarch age (default to start age if not set)
    let patriarch_age_key = "patriarch_age".to_string();
    let current_age = world.global_metrics
        .get(&patriarch_age_key)
        .copied()
        .unwrap_or(gen_mechanics.patriarch_start_age as f64);

    // Age the patriarch by tick_span years
    let new_age = current_age + scenario.tick_span as f64;
    world.global_metrics.insert(patriarch_age_key.clone(), new_age);

    // Check if patriarch has reached end age - trigger generation transfer
    if new_age >= gen_mechanics.patriarch_end_age as f64 {
        // Apply inheritance coefficients to all family metrics
        // Per architecture: new generation starts with reduced metrics
        // Use scenario-specific coefficients if available, default to 0.7

        // Scale all family_ metrics (metrics starting with "family_")
        let family_metric_keys: Vec<String> = world.global_metrics
            .keys()
            .filter(|k| k.starts_with("family_"))
            .cloned()
            .collect();

        for metric in &family_metric_keys {
            if let Some(value) = world.global_metrics.get(metric) {
                // Get coefficient from scenario, default to 0.7
                let coefficient = gen_mechanics.inheritance_coefficients
                    .get(metric)
                    .copied()
                    .unwrap_or(0.7);
                let new_value = value * coefficient;
                world.global_metrics.insert(metric.clone(), new_value);
            }
        }

        // Reset patriarch age to start age for new generation
        world.global_metrics.insert(patriarch_age_key, gen_mechanics.patriarch_start_age as f64);

        // Record generation transfer event
        let event = Event::new(
            "generation_transfer".to_string(),
            current_tick,
            current_year,
            "scenario".to_string(),
            EventType::Milestone,
            true, // is_key event
            "Новое поколение семьи вступает во власть".to_string(),
        );
        event_log.add(event);
    }
}

// ============================================================================
// Step 7: Check Collapses (on_collapse)
// ============================================================================

fn check_collapses(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    // Find actors that should collapse
    let mut to_collapse: Vec<(String, Vec<crate::core::Successor>)> = Vec::new();

    for (actor_id, actor) in &world.actors {
        // Skip if already dead (use HashSet for fast lookup)
        if world.dead_actor_ids.contains(actor_id) {
            continue;
        }

        // Skip if actor has minimum survival guarantee
        if let Some(min_ticks) = actor.minimum_survival_ticks {
            if current_tick < min_ticks {
                continue;
            }
        }

        // cohesion < 10 OR legitimacy < 5
        if actor.metrics.cohesion < 10.0 || actor.metrics.legitimacy < 5.0 {
            // Collapse regardless of whether on_collapse is empty
            // Empty on_collapse just means no successors, but actor still dies
            to_collapse.push((actor_id.clone(), actor.on_collapse.clone()));
        }
    }

    // Process collapses
    for (actor_id, successors) in to_collapse {
        // Record death event
        if let Some(actor) = world.actors.get(&actor_id) {
            let event = Event::new(
                format!("death_{}", actor_id),
                current_tick,
                current_year,
                actor_id.clone(),
                EventType::Death,
                true,
                format!("{} прекратил существование", actor.name),
            )
            .with_metrics_snapshot(metrics_to_snapshot(&actor.metrics))
            .with_tags(vec!["collapse".to_string(), actor_id.clone()]);

            event_log.add(event);

            // Move to dead_actors and add to dead_actor_ids HashSet
            let dead_actor = crate::core::DeadActor {
                id: actor_id.clone(),
                tick_death: current_tick,
                year_death: current_year,
                final_metrics: metrics_to_snapshot(&actor.metrics),
                successor_ids: successors
                    .iter()
                    .map(|s| crate::core::SuccessorWeight {
                        id: s.id.clone(),
                        weight: s.weight,
                    })
                    .collect(),
            };
            world.dead_actors.push(dead_actor);
            world.dead_actor_ids.insert(actor_id.clone());

            // Remove from active actors
            world.actors.remove(&actor_id);
        }

        // Create successors (simplified - just add with split metrics)
        // In full implementation, this would use the formula from architecture
        for successor in &successors {
            if !world.actors.contains_key(&successor.id) {
                // Find actor template in scenario (includes successor templates with is_successor_template: true)
                if let Some(scenario_actor) = scenario.actors.iter().find(|a| a.id == successor.id) {
                    let mut new_actor = scenario_actor.clone();
                    new_actor.metrics = split_metrics_for_successor(
                        &scenario_actor.metrics,
                        successor.weight,
                        successors.len(),
                    );
                    new_actor.narrative_status = crate::core::NarrativeStatus::Foreground;
                    new_actor.is_successor_template = false; // Clear the template flag for the actual actor
                    world.actors.insert(successor.id.clone(), new_actor);
                }
            }
        }
    }
}

fn metrics_to_snapshot(metrics: &ActorMetrics) -> HashMap<String, f64> {
    let mut snapshot = HashMap::new();
    snapshot.insert("population".to_string(), metrics.population);
    snapshot.insert("military_size".to_string(), metrics.military_size);
    snapshot.insert("military_quality".to_string(), metrics.military_quality);
    snapshot.insert("economic_output".to_string(), metrics.economic_output);
    snapshot.insert("cohesion".to_string(), metrics.cohesion);
    snapshot.insert("legitimacy".to_string(), metrics.legitimacy);
    snapshot.insert("external_pressure".to_string(), metrics.external_pressure);
    snapshot.insert("treasury".to_string(), metrics.treasury);
    snapshot
}

fn split_metrics_for_successor(
    parent: &ActorMetrics,
    weight: f64,
    _total_successors: usize,
) -> ActorMetrics {
    let share = weight;

    ActorMetrics {
        population: parent.population * share,
        military_size: parent.military_size * share * 0.7, // losses during split
        military_quality: parent.military_quality * 0.8,   // degradation
        economic_output: parent.economic_output * 0.7,
        cohesion: 20.0,  // trauma from split
        legitimacy: 30.0, // new power not established
        external_pressure: parent.external_pressure * 1.3, // enemies sense weakness
        treasury: parent.treasury * share * 0.5, // plundering
    }
}

// ============================================================================
// Step 8: Record Metric Changes
// ============================================================================

fn record_metric_changes(
    world: &WorldState,
    initial_states: &HashMap<String, ActorMetrics>,
    tick: u32,
    year: i32,
    event_log: &mut EventLog,
) {
    for (actor_id, actor) in &world.actors {
        if let Some(initial) = initial_states.get(actor_id) {
            let changes = calculate_metric_changes(&actor.metrics, initial);

            if !changes.is_empty() {
                let change_desc = changes
                    .iter()
                    .map(|(k, v)| format!("{}: {:+.1}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ");

                let event = Event::new(
                    format!("metrics_{}_{}", actor_id, tick),
                    tick,
                    year,
                    actor_id.clone(),
                    EventType::Threshold,
                    false,
                    format!("{}: {}", actor.name_short, change_desc),
                )
                .with_metrics_snapshot(metrics_to_snapshot(&actor.metrics));

                event_log.add(event);
            }
        }
    }
}

fn calculate_metric_changes(
    current: &ActorMetrics,
    initial: &ActorMetrics,
) -> Vec<(String, f64)> {
    let mut changes = Vec::new();

    let pop_change = current.population - initial.population;
    if pop_change.abs() > 10.0 {
        changes.push(("population".to_string(), pop_change));
    }

    let mil_change = current.military_size - initial.military_size;
    if mil_change.abs() > 1.0 {
        changes.push(("military_size".to_string(), mil_change));
    }

    let qual_change = current.military_quality - initial.military_quality;
    if qual_change.abs() > 1.0 {
        changes.push(("military_quality".to_string(), qual_change));
    }

    let econ_change = current.economic_output - initial.economic_output;
    if econ_change.abs() > 1.0 {
        changes.push(("economic_output".to_string(), econ_change));
    }

    let coh_change = current.cohesion - initial.cohesion;
    if coh_change.abs() > 2.0 {
        changes.push(("cohesion".to_string(), coh_change));
    }

    let leg_change = current.legitimacy - initial.legitimacy;
    if leg_change.abs() > 2.0 {
        changes.push(("legitimacy".to_string(), leg_change));
    }

    let press_change = current.external_pressure - initial.external_pressure;
    if press_change.abs() > 3.0 {
        changes.push(("external_pressure".to_string(), press_change));
    }

    let treas_change = current.treasury - initial.treasury;
    if treas_change.abs() > 10.0 {
        changes.push(("treasury".to_string(), treas_change));
    }

    changes
}

// ============================================================================
// Utility
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_advances_time() {
        let mut world = WorldState::new("test".to_string(), 375);
        let scenario = Scenario {
            id: "test".to_string(),
            label: "Test".to_string(),
            description: "Test scenario".to_string(),
            start_year: 375,
            tempo: 1.0,
            tick_span: 5,
            era: crate::core::Era::Ancient,
            tick_label: "year".to_string(),
            actors: vec![],
            auto_deltas: vec![],
            patron_actions: vec![],
            milestone_events: vec![],
            rank_conditions: vec![],
            generation_mechanics: None,
            llm_context: "".to_string(),
            consequence_context: "".to_string(),
            player_actor_id: None,
            status_indicators: vec![],
            global_metric_weights: HashMap::new(),
            features: crate::core::ScenarioFeatures::default(),
            military_conflict_probability: 0.3,
            naval_conflict_probability: 0.1,
            random_events: vec![],
        };
        let mut event_log = EventLog::new();

        let initial_tick = world.tick;
        let initial_year = world.year;

        tick(&mut world, &scenario, &mut event_log);

        assert_eq!(world.tick, initial_tick + 1);
        assert_eq!(world.year, initial_year + 5);
    }
}

// ============================================================================
// Debug/Explain mode
// ============================================================================

/// Generate explanation for the last tick from event log
pub fn generate_tick_explanation(
    world: &WorldState,
    event_log: &EventLog,
) -> TickExplanation {
    let current_tick = world.tick;
    let current_year = world.year;

    let mut explanation = TickExplanation {
        tick: current_tick,
        year: current_year,
        ..Default::default()
    };

    // Get events from the last tick
    let tick_events: Vec<&Event> = event_log.events.iter()
        .filter(|e| e.tick == current_tick)
        .collect();

    for event in tick_events {
        match event.event_type {
            EventType::Milestone => {
                explanation.milestones_fired.push(MilestoneEntry {
                    id: event.id.clone(),
                    conditions_met: vec![event.description.clone()],
                    effects_applied: HashMap::new(),
                });
            }
            EventType::Threshold => {
                explanation.random_events_fired.push(RandomEventEntry {
                    id: event.id.clone(),
                    target: event.actor_id.clone(),
                    effects_applied: HashMap::new(),
                });
            }
            EventType::War => {
                explanation.interactions_fired.push(InteractionEntry {
                    interaction_type: "military".to_string(),
                    actor_a: event.actor_id.clone(),
                    actor_b: String::new(),
                    details: event.description.clone(),
                });
            }
            EventType::Collapse => {
                explanation.collapses.push(event.actor_id.clone());
            }
            _ => {}
        }
    }

    explanation
}
