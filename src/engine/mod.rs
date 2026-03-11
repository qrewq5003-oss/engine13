use std::collections::{HashMap, VecDeque};

use rand::Rng;
use rand_chacha::ChaCha8Rng;
use crate::core::{
    ActorDelta, ComparisonOperator, DependencyMode, DependencyRule, Event, EventConditionType, EventCondition,
    EventType, MetricRef, Scenario, WorldState,
};
use serde::Serialize;

mod interactions;

/// Validate dependency rules against known metrics
pub fn validate_dependencies(rules: &[DependencyRule], known_metrics: &[&str]) {
    for rule in rules {
        // threshold is required for non-Linear modes
        match rule.mode {
            DependencyMode::Linear => {}
            _ => {
                assert!(
                    rule.threshold.is_some(),
                    "DependencyRule '{}': threshold required for mode {:?}",
                    rule.id, rule.mode
                );
            }
        }
        // from and to must be known metrics
        assert!(
            known_metrics.contains(&rule.from.as_str()),
            "DependencyRule '{}': unknown 'from' metric '{}'",
            rule.id, rule.from
        );
        assert!(
            known_metrics.contains(&rule.to.as_str()),
            "DependencyRule '{}': unknown 'to' metric '{}'",
            rule.id, rule.to
        );
    }
}

/// Apply a single dependency rule to an actor
/// Sequential mutation semantics - each rule reads the current state
/// of the actor (already modified by previous rules).
fn apply_dependency_rule(actor: &mut crate::core::Actor, rule: &DependencyRule) {
    let from_val = actor.get_metric(&rule.from);
    let delta = match rule.mode {
        DependencyMode::Deficit => {
            let threshold = rule.threshold.expect("threshold required for Deficit");
            if from_val < threshold {
                -((threshold - from_val) * rule.coefficient)
            } else { 0.0 }
        }
        DependencyMode::Excess => {
            let threshold = rule.threshold.expect("threshold required for Excess");
            if from_val > threshold {
                -((from_val - threshold) * rule.coefficient)
            } else { 0.0 }
        }
        DependencyMode::Bonus => {
            let threshold = rule.threshold.expect("threshold required for Bonus");
            if from_val > threshold {
                (from_val - threshold) * rule.coefficient
            } else { 0.0 }
        }
        DependencyMode::Linear => from_val * rule.coefficient,
    };
    if delta != 0.0 {
        actor.add_metric(&rule.to, delta);
    }
}

/// Phase: Apply dependency rules to all actors
/// Rules are applied in strict file order - order is part of simulation logic.
fn phase_apply_dependencies(world: &mut WorldState, scenario: &Scenario) {
    for actor in world.actors.values_mut() {
        for rule in &scenario.dependencies {
            apply_dependency_rule(actor, rule);
        }
    }
}

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
/// 3. Random events
/// 4. Actor tag effects
/// 5. Clamp metrics to bounds
/// 6. Events: thresholds, ranks, milestones, game mode, relevance
/// 7. Actor collapses
/// 8. Record changes and generation mechanics
/// 9. Advance tick state
pub fn tick(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
    rng: &mut rand_chacha::ChaCha8Rng,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    // Store initial state for event comparison
    let initial_states: HashMap<String, HashMap<String, f64>> = world
        .actors
        .iter()
        .map(|(id, actor)| (id.clone(), actor.metrics.clone()))
        .collect();

    // Phase 1: Auto-deltas via MetricRef
    phase_auto_deltas(world, scenario, rng);

    // Phase 2: Region rank bonuses (fixed deltas, legitimacy floor)
    phase_region_ranks(world, scenario);

    // Phase 3: Dependency graph and interactions
    phase_interactions(world, scenario, event_log, rng);

    // Phase 3: Random events
    phase_random_events(world, scenario, event_log, rng);

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
    phase_advance(world, scenario);
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
// Phase 2: Region rank bonuses (fixed deltas, legitimacy floor)
// ============================================================================

fn phase_region_ranks(world: &mut WorldState, scenario: &Scenario) {
    // Region rank bonuses are passive fixed deltas and floors.
    // Intentionally non-compounding: delta is constant, not % of current value.
    for actor in world.actors.values_mut() {
        for rule in &scenario.rank_bonuses {
            if rule.rank == actor.region_rank {
                for effect in &rule.effects {
                    if let Some(floor) = effect.floor {
                        // floor: apply as min(), don't change if already above
                        let current = actor.get_metric(&effect.metric);
                        if current < floor {
                            actor.set_metric(&effect.metric, floor);
                        }
                    } else {
                        actor.add_metric(&effect.metric, effect.delta);
                    }
                }
            }
        }
    }
}

// ============================================================================
// Phase 3: Dependency graph and interactions
// ============================================================================

fn phase_interactions(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog, rng: &mut ChaCha8Rng) {
    // Apply dependency rules from scenario
    phase_apply_dependencies(world, scenario);

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

    // Shuffle events to avoid ordering bias - use continue not break for cap
    let mut shuffled_events = all_events;
    shuffled_events.shuffle(rng);

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

    // Track fired events this tick for cap
    let mut fired_this_tick = 0u32;

    for event in &shuffled_events {
        // Cap check - use continue not break to avoid ordering bias
        if scenario.max_random_events_per_tick > 0
            && fired_this_tick >= scenario.max_random_events_per_tick {
            continue;
        }

        // Skip one-time events that already fired
        if event.one_time && world.fired_events.contains(&event.id) {
            continue;
        }

        // Roll for event probability
        let roll: f64 = rng.gen();

        if roll > event.probability {
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

            // Increment fired counter
            fired_this_tick += 1;

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
    check_victory_condition(world, scenario);
}

/// Check victory condition
fn check_victory_condition(world: &mut WorldState, scenario: &Scenario) {
    if world.victory_achieved {
        return;
    }

    if let Some(ref vc) = scenario.victory_condition {
        if world.tick >= vc.minimum_tick {
            let value = crate::core::MetricRef::parse(&vc.metric).get(world);
            let main_condition = value >= vc.threshold;

            // Check additional conditions
            let additional_ok = vc.additional_conditions.iter().all(|cond| {
                let metric_value = crate::core::MetricRef::parse(&cond.metric).get(world);
                cond.operator.evaluate(metric_value, cond.value)
            });

            if main_condition && additional_ok {
                world.victory_sustained_ticks += 1;
                if world.victory_sustained_ticks >= vc.sustained_ticks_required.max(1) {
                    world.victory_achieved = true;
                    world.game_mode = crate::core::GameMode::Ended;
                }
            } else {
                world.victory_sustained_ticks = 0;
            }
        }
    }
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

fn phase_record(world: &mut WorldState, scenario: &Scenario, initial_states: &HashMap<String, HashMap<String, f64>>, current_tick: u32, current_year: i32, event_log: &mut EventLog) {
    record_metric_changes(world, initial_states, current_tick, current_year, event_log);
    check_generation_transfer(world, scenario, event_log);
    update_metric_history(world);
    update_prev_metrics(world);
    world.ticks_since_last_narrative += 1;
}

// ============================================================================
// Phase 8: Advance tick state
// ============================================================================

fn phase_advance(world: &mut WorldState, scenario: &Scenario) {
    world.tick += 1;
    // Year is derived from tick: 2 ticks per year (tick 0-1 = year 0, tick 2-3 = year 1, etc.)
    world.year = scenario.start_year as i32 + (world.tick / 2) as i32;
    world.actions_this_tick = 0;
}

fn apply_treasury(world: &mut WorldState) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            let incomes = actor.get_metric("economic_output") * actor.get_metric("population") * 0.001;
            let expenses = actor.get_metric("military_size") * 0.8;
            actor.add_metric("treasury", incomes - expenses);
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
                    let current = actor.metrics.get(metric).copied().unwrap_or(0.0);
                    actor.metrics.insert(metric.clone(), current + *modifier as f64);
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
    // Only clamp known metrics - treasury can be negative
    let clamp_0_100 = [
        "legitimacy", "cohesion", "military_quality",
        "economic_output", "external_pressure"
    ];
    let clamp_min_0 = ["military_size", "population"];

    for actor in world.actors.values_mut() {
        for key in &clamp_0_100 {
            actor.clamp_metric(key, 0.0, 100.0);
        }
        for key in &clamp_min_0 {
            actor.clamp_metric(key, 0.0, f64::MAX);
        }
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
        if actor.get_metric("cohesion") < 25.0 {
            // This is handled in the dependency graph step
            // Here we just log if critical
            if actor.get_metric("legitimacy") < 30.0 {
                let event = Event::new(
                    format!("threshold_{}_low_cohesion", actor.id),
                    current_tick,
                    current_year,
                    actor.id.clone(),
                    EventType::Threshold,
                    false,
                    format!(
                        "{}: критически низкая сплочённость ({:.1}) угрожает стабильности",
                        actor.name_short, actor.get_metric("cohesion")
                    ),
                );
                event_log.add(event);
            }
        }

        // external_pressure > 80 → trigger migration for neighbors
        if actor.get_metric("external_pressure") > 80.0 {
            for neighbor in &actor.neighbors {
                if let Some(neighbor_actor) = world.actors.get(&neighbor.id) {
                    if neighbor_actor.get_metric("external_pressure") < 50.0 {
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
                        let actor_value = actor.metrics.get(metric).copied().unwrap_or(0.0);
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
                let mil_q = ottomans.get_metric("military_quality");
                ottomans.set_metric("military_quality", (mil_q - 15.0).max(0.0));
                ottomans.add_metric("treasury", -200.0);
                let coh = ottomans.get_metric("cohesion");
                ottomans.set_metric("cohesion", (coh - 10.0).max(0.0));
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

    // Calculate max military_size for normalization
    let max_military_size = world.actors.values()
        .map(|a| a.get_metric("military_size"))
        .fold(1.0_f64, f64::max);

    // Calculate average power projection for all active actors
    let avg_power_projection: f64 = world.actors.values()
        .map(|a| a.power_projection(1.0, max_military_size))
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

        let power_proj = actor.power_projection(1.0, max_military_size);

        // Condition 1: Power projection > 70% of average
        let condition_power = power_proj > avg_power_projection * 0.7;

        // Condition 2: Contact with narrative actor via neighbor relationship
        // Check if this actor is a neighbor (distance <= 2) of any foreground actor
        let condition_contact = narrative_actor_ids.iter()
            .filter(|narr_id| narr_id.as_str() != actor_id.as_str())
            .any(|narr_id| {
                // Check if narrative actor has this actor as a neighbor with distance <= 2
                if let Some(narr_actor) = world.actors.get(narr_id) {
                    narr_actor.neighbors.iter().any(|n| n.id == *actor_id && n.distance <= 2)
                } else {
                    false
                }
            });

        // Condition 3: Internal upheaval
        // Check if any metric changed by >30 in last 5 ticks
        let condition_upheaval = check_actor_upheaval(world, actor_id)
            || actor.get_metric("cohesion") < 25.0
            || actor.get_metric("legitimacy") < 20.0;

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

        let power_proj = actor.power_projection(1.0, max_military_size);

        // Condition for return to background:
        // power_projection < 40% of average
        // AND no active interactions with narrative actors
        // AND no internal upheaval for 10+ ticks
        let low_power = power_proj < avg_power_projection * 0.4;

        // Check for recent upheaval
        let recent_upheaval = world.actor_upheaval_ticks.get(actor_id).copied().unwrap_or(0) < 10;

        // Check for interactions with narrative actors via neighbor relationship
        let has_narrative_contact = narrative_actor_ids.iter()
            .filter(|&narr_id| narr_id != actor_id)
            .any(|narr_id| {
                if let Some(narr_actor) = world.actors.get(narr_id) {
                    // Check if either actor is a neighbor of the other with distance <= 2
                    narr_actor.neighbors.iter().any(|n| n.id == *actor_id && n.distance <= 2)
                        || actor.neighbors.iter().any(|n| n.id == *narr_id && n.distance <= 2)
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
            ("population", actor.get_metric("population")),
            ("military_size", actor.get_metric("military_size")),
            ("military_quality", actor.get_metric("military_quality")),
            ("economic_output", actor.get_metric("economic_output")),
            ("cohesion", actor.get_metric("cohesion")),
            ("legitimacy", actor.get_metric("legitimacy")),
            ("external_pressure", actor.get_metric("external_pressure")),
            ("treasury", actor.get_metric("treasury")),
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
            let pop_delta = actor.get_metric("population") - prev.get("population").copied().unwrap_or(0.0);
            if pop_delta.abs() > 0.01 {
                metric_changes.insert("population".to_string(), pop_delta);
            }

            let mil_delta = actor.get_metric("military_size") - prev.get("military_size").copied().unwrap_or(0.0);
            if mil_delta.abs() > 0.01 {
                metric_changes.insert("military_size".to_string(), mil_delta);
            }

            let qual_delta = actor.get_metric("military_quality") - prev.get("military_quality").copied().unwrap_or(0.0);
            if qual_delta.abs() > 0.01 {
                metric_changes.insert("military_quality".to_string(), qual_delta);
            }

            let econ_delta = actor.get_metric("economic_output") - prev.get("economic_output").copied().unwrap_or(0.0);
            if econ_delta.abs() > 0.01 {
                metric_changes.insert("economic_output".to_string(), econ_delta);
            }

            let coh_delta = actor.get_metric("cohesion") - prev.get("cohesion").copied().unwrap_or(0.0);
            if coh_delta.abs() > 0.01 {
                metric_changes.insert("cohesion".to_string(), coh_delta);
            }

            let leg_delta = actor.get_metric("legitimacy") - prev.get("legitimacy").copied().unwrap_or(0.0);
            if leg_delta.abs() > 0.01 {
                metric_changes.insert("legitimacy".to_string(), leg_delta);
            }

            let pres_delta = actor.get_metric("external_pressure") - prev.get("external_pressure").copied().unwrap_or(0.0);
            if pres_delta.abs() > 0.01 {
                metric_changes.insert("external_pressure".to_string(), pres_delta);
            }

            let treas_delta = actor.get_metric("treasury") - prev.get("treasury").copied().unwrap_or(0.0);
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
                    let actor_value = actor.metrics.get(metric).copied().unwrap_or(0.0);
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

    // Only process if family_state exists
    let Some(ref mut family_state) = world.family_state else {
        return;
    };

    let current_tick = world.tick;
    let current_year = world.year;

    // Age the patriarch only on even ticks (FirstHalf = start of year)
    // This ensures 1 year of aging per 2 ticks
    if world.tick % 2 == 0 {
        family_state.patriarch_age += 1;
    }

    // Check triggers
    let patriarch_age = family_state.patriarch_age;
    let normal_trigger = patriarch_age >= gen_mechanics.patriarch_end_age as u32;

    // For early trigger, we need to check external metric - do this after dropping family_state borrow
    let early_trigger_check = gen_mechanics.early_transfer.as_ref().map(|early| {
        (early.age, early.condition_metric.clone(), early.condition_operator.clone(), early.condition_value)
    });

    // Drop the mutable borrow before checking external metric
    let _ = family_state; // End mutable borrow scope

    // Check early trigger condition (needs world access)
    let early_trigger = early_trigger_check.map_or(false, |(age, metric, operator, value)| {
        if patriarch_age < age {
            return false;
        }
        let metric_value = crate::core::MetricRef::parse(&metric).get(world);
        operator.evaluate(metric_value, value)
    });

    // Process generation transfer if triggered
    if early_trigger || normal_trigger {
        let family_state = world.family_state.as_mut().unwrap();

        // Strict order of operations:
        // 1. Increment generation_count
        family_state.generation_count += 1;

        // 2. Apply inheritance coefficients to all family metrics
        let family_metric_keys: Vec<String> = family_state.metrics.keys().cloned().collect();

        for metric in &family_metric_keys {
            if let Some(value) = family_state.metrics.get(metric) {
                // Get coefficient from scenario, default to 0.7
                let coefficient = gen_mechanics.inheritance_coefficients
                    .get(metric)
                    .copied()
                    .unwrap_or(0.7);
                let new_value = value * coefficient;
                family_state.metrics.insert(metric.clone(), new_value);
            }
        }

        // 3. Reset patriarch age to start age for new generation
        family_state.patriarch_age = gen_mechanics.patriarch_start_age as u32;

        // 4. Log event with current generation number
        let event = Event::new(
            "generation_transfer".to_string(),
            current_tick,
            current_year,
            "scenario".to_string(),
            EventType::Milestone,
            true, // is_key event
            format!("Поколение {} вступает во власть", family_state.generation_count),
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

    // Get actor IDs to avoid borrow conflict with collapse_warning_ticks
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    for actor_id in &actor_ids {
        let actor = match world.actors.get(actor_id) {
            Some(a) => a,
            None => continue,
        };

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

        // Path 1: classic collapse (external pressure + internal weakness)
        let classic_collapse =
            actor.get_metric("legitimacy") < 10.0
            && actor.get_metric("cohesion") < 15.0
            && actor.get_metric("external_pressure") > 85.0;

        // Path 2: internal collapse (civil war / disintegration without external threat)
        let internal_collapse =
            actor.get_metric("legitimacy") < 5.0
            && actor.get_metric("cohesion") < 8.0;

        let in_danger = classic_collapse || internal_collapse;

        if in_danger {
            // Increment warning counter
            let counter = world.collapse_warning_ticks
                .entry(actor_id.clone())
                .or_insert(0);
            *counter += 1;

            // Collapse only after 3 consecutive dangerous ticks
            if *counter >= 3 {
                to_collapse.push((actor_id.clone(), actor.on_collapse.clone()));
            }
        } else {
            // Reset counter if actor is no longer in danger
            world.collapse_warning_ticks.remove(actor_id);
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

fn metrics_to_snapshot(metrics: &HashMap<String, f64>) -> HashMap<String, f64> {
    crate::core::actor::metrics_to_snapshot(metrics)
}

fn split_metrics_for_successor(
    parent: &HashMap<String, f64>,
    weight: f64,
    _total_successors: usize,
) -> HashMap<String, f64> {
    let mut m = parent.clone();
    let ms = m.get("military_size").copied().unwrap_or(0.0);
    m.insert("military_size".to_string(), ms * weight * 0.7);
    let mil_q = m.get("military_quality").copied().unwrap_or(0.0);
    m.insert("military_quality".to_string(), mil_q * 0.8);
    let eco = m.get("economic_output").copied().unwrap_or(0.0);
    m.insert("economic_output".to_string(), eco * 0.7);
    let pop = m.get("population").copied().unwrap_or(0.0);
    m.insert("population".to_string(), pop * weight);
    let tr = m.get("treasury").copied().unwrap_or(0.0);
    m.insert("treasury".to_string(), tr * weight * 0.5);
    let ep = m.get("external_pressure").copied().unwrap_or(0.0);
    m.insert("external_pressure".to_string(), (ep * 1.3).min(100.0));
    m.insert("cohesion".to_string(), 20.0);
    m.insert("legitimacy".to_string(), 30.0);
    crate::core::actor::ensure_default_metrics(&mut m);
    m
}

// ============================================================================
// Step 8: Record Metric Changes
// ============================================================================

fn record_metric_changes(
    world: &WorldState,
    initial_states: &HashMap<String, HashMap<String, f64>>,
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
    current: &HashMap<String, f64>,
    initial: &HashMap<String, f64>,
) -> Vec<(String, f64)> {
    let mut changes = Vec::new();

    let pop_change = current.get("population").copied().unwrap_or(0.0) - initial.get("population").copied().unwrap_or(0.0);
    if pop_change.abs() > 10.0 {
        changes.push(("population".to_string(), pop_change));
    }

    let mil_change = current.get("military_size").copied().unwrap_or(0.0) - initial.get("military_size").copied().unwrap_or(0.0);
    if mil_change.abs() > 1.0 {
        changes.push(("military_size".to_string(), mil_change));
    }

    let qual_change = current.get("military_quality").copied().unwrap_or(0.0) - initial.get("military_quality").copied().unwrap_or(0.0);
    if qual_change.abs() > 1.0 {
        changes.push(("military_quality".to_string(), qual_change));
    }

    let econ_change = current.get("economic_output").copied().unwrap_or(0.0) - initial.get("economic_output").copied().unwrap_or(0.0);
    if econ_change.abs() > 1.0 {
        changes.push(("economic_output".to_string(), econ_change));
    }

    let coh_change = current.get("cohesion").copied().unwrap_or(0.0) - initial.get("cohesion").copied().unwrap_or(0.0);
    if coh_change.abs() > 2.0 {
        changes.push(("cohesion".to_string(), coh_change));
    }

    let leg_change = current.get("legitimacy").copied().unwrap_or(0.0) - initial.get("legitimacy").copied().unwrap_or(0.0);
    if leg_change.abs() > 2.0 {
        changes.push(("legitimacy".to_string(), leg_change));
    }

    let press_change = current.get("external_pressure").copied().unwrap_or(0.0) - initial.get("external_pressure").copied().unwrap_or(0.0);
    if press_change.abs() > 3.0 {
        changes.push(("external_pressure".to_string(), press_change));
    }

    let treas_change = current.get("treasury").copied().unwrap_or(0.0) - initial.get("treasury").copied().unwrap_or(0.0);
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
    use rand::SeedableRng;

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
            generation_length: None,
            actions_per_tick: 0,
            victory_condition: None,
            universal_actions: vec![],
            global_metrics_display: vec![],
            initial_family_metrics: None,
            max_random_events_per_tick: 0,
            narrative_config: crate::core::NarrativeConfig::default(),
            dependencies: vec![],
            interaction_rules: vec![],
            rank_bonuses: vec![],
            map: None,
        };
        let mut event_log = EventLog::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

        let initial_tick = world.tick;
        let initial_year = world.year;

        tick(&mut world, &scenario, &mut event_log, &mut rng);

        assert_eq!(world.tick, initial_tick + 1);
        // Year is derived from tick: 2 ticks per year, so after 1 tick year stays same
        assert_eq!(world.year, initial_year);  // tick 1 = start_year + (1/2) = start_year
        
        tick(&mut world, &scenario, &mut event_log, &mut rng);
        
        // After 2 ticks, year should increment
        assert_eq!(world.tick, 2);
        assert_eq!(world.year, initial_year + 1);  // tick 2 = start_year + (2/2) = start_year + 1
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
