use std::collections::HashMap;

use crate::core::{
    Actor, ActorMetrics, ComparisonOperator, Event, EventConditionType, EventCondition,
    EventType, Scenario, WorldState,
};

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
/// Order of operations (from architecture):
/// 1. Apply auto_deltas from scenario
/// 2. Apply dependency graph coefficients
/// 3. Calculate neighbor interactions (trade, pressure, migration)
/// 4. Apply actor tags effects
/// 5. Clamp all metrics to valid bounds
/// 6. Check threshold effects, rank_conditions, milestone_events
/// 7. Check on_collapse conditions
/// 8. Record events to EventLog
/// 9. Update tick and year in WorldState
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

    // Step 1: Apply auto_deltas
    apply_auto_deltas(world, scenario, &mut rng);

    // Step 1b: Apply treasury calculation (incomes - expenses)
    apply_treasury(world);

    // Step 2: Apply dependency graph coefficients
    apply_dependency_graph(world);

    // Step 3: Calculate neighbor interactions
    calculate_interactions(world);

    // Step 4: Apply actor tags effects
    apply_actor_tags(world, scenario);

    // Step 5: Clamp metrics to valid bounds
    clamp_metrics(world);

    // Step 6: Check threshold effects, rank_conditions, milestone_events
    check_threshold_effects(world, scenario, event_log);
    check_rank_conditions(world, scenario, event_log);
    check_milestone_events(world, scenario, event_log);

    // Step 7: Check on_collapse conditions
    check_collapses(world, scenario, event_log);

    // Step 8: Record metric change events if significant
    record_metric_changes(world, &initial_states, current_tick, current_year, event_log);

    // Step 8b: Check generation transfer (patriarch aging)
    check_generation_transfer(world, scenario, event_log);

    // Step 8c: Apply family auto deltas (passive changes per tick)
    apply_family_auto_deltas(world);

    // Save RNG state once at the end of the tick
    world.rng_state = rng.get_seed();

    // Step 9: Update tick and year
    world.tick += 1;
    world.year += scenario.tick_span as i32;
}

// ============================================================================
// Step 1: Auto Deltas
// ============================================================================

fn apply_auto_deltas(world: &mut WorldState, scenario: &Scenario, rng: &mut rand_chacha::ChaCha8Rng) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            for auto_delta in &scenario.auto_deltas {
                // Filter by actor_id if specified
                if let Some(ref delta_actor_id) = auto_delta.actor_id {
                    if delta_actor_id != &actor.id {
                        continue;
                    }
                }
                apply_single_auto_delta(actor, auto_delta, rng);
            }
        }
    }
}

/// Apply treasury calculation: treasury += incomes - expenses
/// Formula:
///   incomes = economic_output × population × 0.001
///   expenses = military_size × 0.8
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

fn apply_single_auto_delta(actor: &mut Actor, auto_delta: &crate::core::AutoDelta, rng: &mut rand_chacha::ChaCha8Rng) {
    use rand::Rng;

    // Treasury is calculated separately via income/expenses formula
    if auto_delta.metric == "treasury" {
        return;
    }

    // Calculate delta: base + sum of matching conditions
    let mut delta = auto_delta.base;
    for cond in &auto_delta.conditions {
        if check_condition(&actor.metrics, cond) {
            delta += cond.delta;
        }
    }

    // Apply noise using deterministic RNG
    let noise = (rng.gen::<f64>() - 0.5) * 2.0 * auto_delta.noise;
    let final_delta = delta + noise;

    apply_metric_delta(&mut actor.metrics, &auto_delta.metric, final_delta);
}

fn check_condition(metrics: &ActorMetrics, cond: &crate::core::DeltaCondition) -> bool {
    let value = get_metric_value(metrics, &cond.metric);
    match cond.operator {
        ComparisonOperator::Less => value < cond.value,
        ComparisonOperator::LessOrEqual => value <= cond.value,
        ComparisonOperator::Greater => value > cond.value,
        ComparisonOperator::GreaterOrEqual => value >= cond.value,
        ComparisonOperator::Equal => (value - cond.value).abs() < 0.001,
    }
}

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

fn calculate_interactions(world: &mut WorldState) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    // Collect all interactions first to avoid borrow issues
    let mut interactions: Vec<(String, String, InteractionType, f64)> = Vec::new();

    for actor_id in &actor_ids {
        if let Some(actor) = world.actors.get(actor_id) {
            for neighbor in &actor.neighbors {
                if world.actors.contains_key(&neighbor.id) {
                    // Get ALL interactions (trade, pressure, migration, cultural) not just the first one
                    let actor_interactions = determine_all_interactions(actor, &neighbor.id, world);
                    // Add source actor_id to each interaction
                    for (target_id, itype, magnitude) in actor_interactions {
                        interactions.push((actor_id.clone(), target_id, itype, magnitude));
                    }
                }
            }
        }
    }

    // Apply interactions
    for (source_id, target_id, itype, magnitude) in interactions {
        apply_interaction(world, &source_id, &target_id, itype, magnitude);
    }
}

#[derive(Debug, Clone)]
enum InteractionType {
    Trade,
    MilitaryPressure,
    Migration,
    CulturalInfluence,
}

/// Determine ALL possible interactions between two actors (not just the first one)
fn determine_all_interactions(
    actor: &Actor,
    neighbor_id: &str,
    world: &WorldState,
) -> Vec<(String, InteractionType, f64)> {
    let mut result = Vec::new();

    let neighbor = match world.actors.get(neighbor_id) {
        Some(n) => n,
        None => return result,
    };

    // Military pressure - calculate first to determine trade suppression
    let pressure = calculate_military_pressure(actor, neighbor);
    if pressure > 0.1 {
        result.push((neighbor.id.clone(), InteractionType::MilitaryPressure, pressure));
    }

    // Trade suppression logic based on military pressure
    // > 0.4: full suppression (war suppresses trade)
    // 0.2 - 0.4: trade with 0.5 coefficient (border tensions)
    // < 0.2: trade works normally
    let trade_suppressed = pressure > 0.4;
    let trade_coefficient = if pressure > 0.4 {
        0.0
    } else if pressure > 0.2 {
        0.5
    } else {
        1.0
    };

    // Check if trade is possible (adjacent OR has trade_networks tag)
    let can_trade = neighbor.neighbors.iter().any(|n| n.id == actor.id)
        || actor.tags.contains(&"trade_networks".to_string());

    // Trade - both actors get a small bonus (if not suppressed by military pressure)
    if !trade_suppressed && can_trade && neighbor.metrics.economic_output > 0.0 && actor.metrics.economic_output > 0.0 {
        let distance_mod = distance_modifier(neighbor.neighbors.iter().find(|n| n.id == actor.id));
        let trade_bonus = if actor.tags.contains(&"trade_networks".to_string())
            || neighbor.tags.contains(&"trade_networks".to_string()) {
            1.0
        } else {
            distance_mod
        };

        // Both actors gain equally: small base bonus × distance modifier × pressure coefficient
        let base_gain = 2.0; // Small fixed base gain
        let gain = ((base_gain * trade_bonus * trade_coefficient).min(3.0 * trade_coefficient)).max(0.0);
        if gain > 0.0 {
            result.push((neighbor.id.clone(), InteractionType::Trade, gain));
        }
    }

    // Migration
    let migration = calculate_migration(actor, neighbor);
    if migration > 0.01 {
        result.push((neighbor.id.clone(), InteractionType::Migration, migration));
    }

    // Cultural influence
    let cultural = calculate_cultural_influence(actor, neighbor);
    if cultural > 0.1 {
        result.push((neighbor.id.clone(), InteractionType::CulturalInfluence, cultural));
    }

    result
}

/// Legacy function - kept for compatibility, returns only the first interaction
#[allow(dead_code)]
fn determine_interaction(
    actor: &Actor,
    neighbor_id: &str,
    world: &WorldState,
) -> Option<(String, InteractionType, f64)> {
    let all = determine_all_interactions(actor, neighbor_id, world);
    all.into_iter().next()
}

fn distance_modifier(neighbor: Option<&crate::core::Neighbor>) -> f64 {
    match neighbor {
        Some(n) => match n.distance {
            1 => 1.0,
            2 => 0.7,
            3 => 0.4,
            _ => 0.1,
        },
        None => 0.1,
    }
}

fn border_type_modifier(border_type: &crate::core::BorderType) -> f64 {
    match border_type {
        crate::core::BorderType::Land => 1.0,
        crate::core::BorderType::Sea => 0.5,
    }
}

fn calculate_military_pressure(actor: &Actor, target: &Actor) -> f64 {
    let neighbor_info = target.neighbors.iter().find(|n| n.id == actor.id);
    let distance_mod = distance_modifier(neighbor_info);
    let border_mod = neighbor_info
        .map(|n| border_type_modifier(&n.border_type))
        .unwrap_or(0.5);

    let power_ratio = actor.power_projection(1.0) / target.power_projection(1.0).max(1.0);
    power_ratio * distance_mod * border_mod
}

fn calculate_migration(actor: &Actor, _target: &Actor) -> f64 {
    let mut rate: f64 = 0.0;

    if actor.metrics.external_pressure > 70.0 {
        rate += 0.05;
    }
    if actor.metrics.economic_output < 20.0 {
        rate += 0.03;
    }
    if actor.metrics.cohesion < 25.0 {
        rate += 0.04;
    }

    // Combination bonuses
    let conditions = [
        actor.metrics.external_pressure > 70.0,
        actor.metrics.economic_output < 20.0,
        actor.metrics.cohesion < 25.0,
    ]
    .iter()
    .filter(|&&c| c)
    .count();

    if conditions >= 2 {
        rate += 0.04;
    }
    if conditions >= 3 {
        rate += 0.04;
    }

    rate.min(0.15)
}

fn calculate_cultural_influence(actor: &Actor, target: &Actor) -> f64 {
    let neighbor_info = target.neighbors.iter().find(|n| n.id == actor.id);
    let distance_mod = distance_modifier(neighbor_info);

    let cultural_strength = (actor.metrics.legitimacy * 0.4
        + actor.metrics.cohesion * 0.3
        + actor.metrics.economic_output * 0.3)
        * distance_mod;

    let target_strength = target.metrics.legitimacy * 0.4
        + target.metrics.cohesion * 0.3
        + target.metrics.economic_output * 0.3;

    if cultural_strength > target_strength {
        cultural_strength - target_strength
    } else {
        0.0
    }
}

fn apply_interaction(
    world: &mut WorldState,
    source_id: &str,
    target_id: &str,
    itype: InteractionType,
    magnitude: f64,
) {
    match itype {
        InteractionType::Trade => {
            // Both actors get the trade bonus equally
            if let Some(source) = world.actors.get_mut(source_id) {
                source.metrics.economic_output += magnitude * 0.5;
                source.metrics.economic_output = source.metrics.economic_output.min(100.0);
            }
            if let Some(target) = world.actors.get_mut(target_id) {
                target.metrics.economic_output += magnitude * 0.5;
                target.metrics.economic_output = target.metrics.economic_output.min(100.0);
            }
        }
        InteractionType::MilitaryPressure => {
            if let Some(target) = world.actors.get_mut(target_id) {
                target.metrics.external_pressure += magnitude * 10.0;
                target.metrics.external_pressure = target.metrics.external_pressure.min(100.0);
            }
        }
        InteractionType::Migration => {
            if let Some(source) = world.actors.get_mut(source_id) {
                let migration_amount = source.metrics.population * magnitude;
                source.metrics.population -= migration_amount;
                source.metrics.military_size -= migration_amount * 0.0003;

                if let Some(target) = world.actors.get_mut(target_id) {
                    target.metrics.population += migration_amount;
                    let rel_amount = migration_amount / target.metrics.population.max(1.0);
                    target.metrics.cohesion -= rel_amount * 0.5;
                    target.metrics.external_pressure += rel_amount * 0.3;
                }
            }
        }
        InteractionType::CulturalInfluence => {
            if let Some(target) = world.actors.get_mut(target_id) {
                // If target cohesion > 60, reduce the effect magnitude by 70%
                let effective_magnitude = if target.metrics.cohesion > 60.0 {
                    magnitude * 0.3
                } else {
                    magnitude
                };

                target.metrics.cohesion -= effective_magnitude * 0.1;
                target.metrics.legitimacy -= effective_magnitude * 0.05;

                // Overwhelming superiority
                if effective_magnitude > target.metrics.cohesion * 2.0 {
                    target.metrics.cohesion -= effective_magnitude * 0.15;
                    target.metrics.legitimacy -= effective_magnitude * 0.075;
                }
            }
        }
    }
}

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
// Family Auto Deltas (Passive changes per tick)
// ============================================================================

fn apply_family_auto_deltas(world: &mut WorldState) {
    let rome_cohesion = world.actors.get("rome")
        .map(|a| a.metrics.cohesion).unwrap_or(50.0);
    let rome_econ = world.actors.get("rome")
        .map(|a| a.metrics.economic_output).unwrap_or(50.0);
    let rome_legitimacy = world.actors.get("rome")
        .map(|a| a.metrics.legitimacy).unwrap_or(50.0);
    let rome_pressure = world.actors.get("rome")
        .map(|a| a.metrics.external_pressure).unwrap_or(30.0);

    let influence = world.family_metrics.get("family_influence").copied().unwrap_or(0.0);
    let knowledge = world.family_metrics.get("family_knowledge").copied().unwrap_or(0.0);
    let wealth = world.family_metrics.get("family_wealth").copied().unwrap_or(0.0);
    let connections = world.family_metrics.get("family_connections").copied().unwrap_or(0.0);

    // family_influence
    let mut d_influence: f64 = -0.5; // пассивный спад
    if connections > 30.0 { d_influence += 0.3; }
    if wealth > 40.0      { d_influence += 0.2; }
    if rome_legitimacy > 60.0 { d_influence += 0.1; }
    if rome_cohesion < 30.0   { d_influence -= 0.2; }

    // family_knowledge
    let mut d_knowledge: f64 = 0.2; // всегда растёт
    if knowledge > 50.0 { d_knowledge += 0.1; } // ускоряется при накоплении

    // family_wealth
    let mut d_wealth: f64 = 0.0;
    if connections > 20.0      { d_wealth += 0.5; }
    else if connections < 5.0  { d_wealth -= 0.5; }
    if rome_econ > 60.0        { d_wealth += 0.2; }

    // family_connections
    let mut d_connections: f64 = -0.3; // нужно поддерживать
    if rome_pressure > 70.0 { d_connections -= 0.2; } // люди разбегаются

    world.family_metrics.insert("family_influence".to_string(),   (influence   + d_influence).max(0.0).min(100.0));
    world.family_metrics.insert("family_knowledge".to_string(),   (knowledge   + d_knowledge).max(0.0).min(100.0));
    world.family_metrics.insert("family_wealth".to_string(),      (wealth      + d_wealth).max(0.0).min(100.0));
    world.family_metrics.insert("family_connections".to_string(), (connections + d_connections).max(0.0).min(100.0));
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
        // Skip if already fired
        if world.milestone_events_fired.contains(&milestone.id) {
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
    let current_age = world.family_metrics
        .get(&patriarch_age_key)
        .copied()
        .unwrap_or(gen_mechanics.patriarch_start_age as f64);

    // Age the patriarch by tick_span years
    let new_age = current_age + scenario.tick_span as f64;
    world.family_metrics.insert(patriarch_age_key.clone(), new_age);

    // Check if patriarch has reached end age - trigger generation transfer
    if new_age >= gen_mechanics.patriarch_end_age as f64 {
        // Apply inheritance coefficients to family metrics
        // Per architecture: new generation starts with reduced metrics
        let inheritance_coefficient = 0.7; // New generation inherits 70% of family strength

        let metrics_to_scale = ["family_influence", "family_knowledge", "family_wealth", "family_connections"];
        
        for metric in &metrics_to_scale {
            if let Some(value) = world.family_metrics.get(*metric) {
                let new_value = value * inheritance_coefficient;
                world.family_metrics.insert(metric.to_string(), new_value);
            }
        }

        // Reset patriarch age to start age for new generation
        world.family_metrics.insert(patriarch_age_key, gen_mechanics.patriarch_start_age as f64);

        // Record generation transfer event
        let event = Event::new(
            "generation_transfer".to_string(),
            current_tick,
            current_year,
            "scenario".to_string(),
            EventType::Milestone,
            true, // is_key event
            "Новое поколение семьи Ди Милано вступает во власть".to_string(),
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

            // Move to dead_actors
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
        };
        let mut event_log = EventLog::new();

        let initial_tick = world.tick;
        let initial_year = world.year;

        tick(&mut world, &scenario, &mut event_log);

        assert_eq!(world.tick, initial_tick + 1);
        assert_eq!(world.year, initial_year + 5);
    }
}
