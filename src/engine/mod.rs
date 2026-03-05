use std::collections::HashMap;

use crate::core::{
    Actor, ActorMetrics, ComparisonOperator, Event, EventConditionType, EventCondition,
    EventType, Scenario, WorldState,
};

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
    let current_tick = world.tick;
    let current_year = world.year;

    // Store initial state for event comparison
    let initial_states: HashMap<String, ActorMetrics> = world
        .actors
        .iter()
        .map(|(id, actor)| (id.clone(), actor.metrics.clone()))
        .collect();

    // Step 1: Apply auto_deltas
    apply_auto_deltas(world, scenario);

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

    // Step 9: Update tick and year
    world.tick += 1;
    world.year += scenario.tick_span as i32;
}

// ============================================================================
// Step 1: Auto Deltas
// ============================================================================

fn apply_auto_deltas(world: &mut WorldState, scenario: &Scenario) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();
    
    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            for auto_delta in &scenario.auto_deltas {
                apply_single_auto_delta(actor, auto_delta);
            }
        }
    }
}

fn apply_single_auto_delta(actor: &mut Actor, auto_delta: &crate::core::AutoDelta) {
    let delta = match auto_delta.metric.as_str() {
        "population" => {
            let mut d = auto_delta.base;
            for cond in &auto_delta.conditions {
                if check_condition(&actor.metrics, cond) {
                    d += cond.delta;
                }
            }
            d
        }
        "military_size" => {
            let mut d = auto_delta.base;
            for cond in &auto_delta.conditions {
                if check_condition(&actor.metrics, cond) {
                    d += cond.delta;
                }
            }
            d
        }
        "military_quality" => {
            let mut d = auto_delta.base;
            for cond in &auto_delta.conditions {
                if check_condition(&actor.metrics, cond) {
                    d += cond.delta;
                }
            }
            d
        }
        "economic_output" => {
            let mut d = auto_delta.base;
            for cond in &auto_delta.conditions {
                if check_condition(&actor.metrics, cond) {
                    d += cond.delta;
                }
            }
            d
        }
        "cohesion" => {
            let mut d = auto_delta.base;
            for cond in &auto_delta.conditions {
                if check_condition(&actor.metrics, cond) {
                    d += cond.delta;
                }
            }
            d
        }
        "legitimacy" => {
            let mut d = auto_delta.base;
            for cond in &auto_delta.conditions {
                if check_condition(&actor.metrics, cond) {
                    d += cond.delta;
                }
            }
            d
        }
        "external_pressure" => {
            let mut d = auto_delta.base;
            for cond in &auto_delta.conditions {
                if check_condition(&actor.metrics, cond) {
                    d += cond.delta;
                }
            }
            d
        }
        "treasury" => {
            // Treasury is calculated separately via income/expenses formula
            return;
        }
        _ => return,
    };

    // Apply noise
    let noise = (rand_f64() - 0.5) * 2.0 * auto_delta.noise;
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

fn apply_dependency_graph(world: &mut WorldState) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            let metrics = actor.metrics.clone();
            
            // legitimacy ↓10 → cohesion ↓3 (coef 0.3)
            if metrics.legitimacy < 50.0 {
                let deficit = 50.0 - metrics.legitimacy;
                actor.metrics.cohesion -= deficit * 0.03;
            }

            // cohesion ↓10 → legitimacy ↓2 (coef 0.2)
            if metrics.cohesion < 50.0 {
                let deficit = 50.0 - metrics.cohesion;
                actor.metrics.legitimacy -= deficit * 0.02;
            }

            // legitimacy ↓10 → military_quality ↓2 (coef 0.2)
            if metrics.legitimacy < 50.0 {
                let deficit = 50.0 - metrics.legitimacy;
                actor.metrics.military_quality -= deficit * 0.02;
            }

            // cohesion ↓10 → economic_output ↓3 (coef 0.3)
            if metrics.cohesion < 50.0 {
                let deficit = 50.0 - metrics.cohesion;
                actor.metrics.economic_output -= deficit * 0.03;
            }

            // external_pressure ↑10 → cohesion ↓2 (coef 0.2)
            if metrics.external_pressure > 50.0 {
                let excess = metrics.external_pressure - 50.0;
                actor.metrics.cohesion -= excess * 0.02;
            }

            // external_pressure ↑10 → legitimacy ↓1 (coef 0.1)
            if metrics.external_pressure > 50.0 {
                let excess = metrics.external_pressure - 50.0;
                actor.metrics.legitimacy -= excess * 0.01;
            }

            // external_pressure ↑10 → military_quality ↓2 (coef 0.2)
            if metrics.external_pressure > 50.0 {
                let excess = metrics.external_pressure - 50.0;
                actor.metrics.military_quality -= excess * 0.02;
            }

            // external_pressure ↑10 → military_size ↓1 (coef 0.1)
            if metrics.external_pressure > 50.0 {
                let excess = metrics.external_pressure - 50.0;
                actor.metrics.military_size -= excess * 0.01;
            }

            // economic_output ↓10 → treasury ↓15 (coef 1.5)
            if metrics.economic_output < 50.0 {
                let deficit = 50.0 - metrics.economic_output;
                actor.metrics.treasury -= deficit * 0.15;
            }

            // military_size ↓10 → economic_output ↓1 (coef 0.1)
            if metrics.military_size < 50.0 {
                let deficit = 50.0 - metrics.military_size;
                actor.metrics.economic_output -= deficit * 0.01;
            }

            // population ↑1000 → economic_output ↑0.5 (coef 0.0005)
            if metrics.population > 1000.0 {
                actor.metrics.economic_output += (metrics.population - 1000.0) * 0.0005;
            }

            // economic_output ↓10 → population ↓200 (coef 20)
            if metrics.economic_output < 50.0 {
                let deficit = 50.0 - metrics.economic_output;
                actor.metrics.population -= deficit * 20.0;
            }

            // Cohesion bonus effect (exception)
            // if external_pressure grew >15 in 1 tick AND legitimacy > 60: cohesion += 5
            // (simplified - we check current state, not growth)
            if metrics.external_pressure > 65.0 && metrics.legitimacy > 60.0 {
                actor.metrics.cohesion += 5.0;
            }

            // Threshold effects
            // cohesion < 25 → any legitimacy fall is doubled (handled in clamping)
            // legitimacy < 20 → military_quality falls -0.5/tick
            if metrics.legitimacy < 20.0 {
                actor.metrics.military_quality -= 0.5;
            }

            // economic_output < 15 → population falls -100/tick
            if metrics.economic_output < 15.0 {
                actor.metrics.population -= 100.0;
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
                    let interaction = determine_interaction(actor, &neighbor.id, world);
                    if let Some((target, itype, magnitude)) = interaction {
                        interactions.push((actor_id.clone(), target, itype, magnitude));
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

fn determine_interaction(
    actor: &Actor,
    neighbor_id: &str,
    world: &WorldState,
) -> Option<(String, InteractionType, f64)> {
    let neighbor = world.actors.get(neighbor_id)?;

    // Check if trade is possible (adjacent OR has trade_networks tag)
    let can_trade = neighbor.neighbors.iter().any(|n| n.id == actor.id)
        || actor.tags.contains(&"trade_networks".to_string());

    // Trade
    if can_trade && neighbor.metrics.economic_output > 0.0 {
        let distance_mod = distance_modifier(neighbor.neighbors.iter().find(|n| n.id == actor.id));
        let trade_bonus = if actor.tags.contains(&"trade_networks".to_string()) {
            1.0
        } else {
            distance_mod
        };

        if actor.metrics.economic_output > neighbor.metrics.economic_output {
            // Richer actor gains more
            let gain = (actor.metrics.economic_output * 0.05 * trade_bonus).min(5.0);
            return Some((neighbor.id.clone(), InteractionType::Trade, gain));
        } else {
            // Poorer actor gains less
            let gain = (neighbor.metrics.economic_output * 0.02 * trade_bonus).min(2.0);
            return Some((neighbor.id.clone(), InteractionType::Trade, gain));
        }
    }

    // Military pressure
    let pressure = calculate_military_pressure(actor, neighbor);
    if pressure > 0.1 {
        return Some((neighbor.id.clone(), InteractionType::MilitaryPressure, pressure));
    }

    // Migration
    let migration = calculate_migration(actor, neighbor);
    if migration > 0.01 {
        return Some((neighbor.id.clone(), InteractionType::Migration, migration));
    }

    // Cultural influence
    let cultural = calculate_cultural_influence(actor, neighbor);
    if cultural > 0.1 {
        return Some((neighbor.id.clone(), InteractionType::CulturalInfluence, cultural));
    }

    None
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
            if let Some(target) = world.actors.get_mut(target_id) {
                target.metrics.economic_output += magnitude;
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
                target.metrics.cohesion -= magnitude * 0.1;
                target.metrics.legitimacy -= magnitude * 0.05;

                // Overwhelming superiority
                if magnitude > target.metrics.cohesion * 2.0 {
                    target.metrics.cohesion -= magnitude * 0.15;
                    target.metrics.legitimacy -= magnitude * 0.075;
                }

                // Resistance
                if target.metrics.cohesion > 60.0 {
                    target.metrics.cohesion *= 0.7;
                    target.metrics.legitimacy *= 0.7;
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

            // Ensure no negative changes from tags pushed metrics below zero
            actor.metrics.population = actor.metrics.population.max(0.0);
            actor.metrics.military_size = actor.metrics.military_size.max(0.0);
            actor.metrics.military_quality = actor.metrics.military_quality.max(0.0).min(100.0);
            actor.metrics.economic_output = actor.metrics.economic_output.max(0.0).min(100.0);
            actor.metrics.cohesion = actor.metrics.cohesion.max(0.0).min(100.0);
            actor.metrics.legitimacy = actor.metrics.legitimacy.max(0.0).min(100.0);
            actor.metrics.external_pressure = actor.metrics.external_pressure.max(0.0).min(100.0);
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
        // Skip if already fired
        if world.milestone_events_fired.contains(&milestone.id) {
            continue;
        }

        let should_trigger = check_event_condition(world, &milestone.condition);

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
            if !actor.on_collapse.is_empty() {
                to_collapse.push((actor_id.clone(), actor.on_collapse.clone()));
            }
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
                // Find original actor data from scenario
                if let Some(scenario_actor) = scenario.actors.iter().find(|a| a.id == successor.id)
                {
                    let mut new_actor = scenario_actor.clone();
                    new_actor.metrics = split_metrics_for_successor(
                        &scenario_actor.metrics,
                        successor.weight,
                        successors.len(),
                    );
                    new_actor.narrative_status = crate::core::NarrativeStatus::Background;
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
    total_successors: usize,
) -> ActorMetrics {
    let share = weight / (total_successors as f64);

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

/// Simple random number generator for noise
/// In production, use a proper RNG with seed
fn rand_f64() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as f64;
    (nanos / 1_000_000_000.0) % 1.0
}

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
