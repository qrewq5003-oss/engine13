use std::collections::HashMap;

use rand::Rng;
use rand_chacha::ChaCha8Rng;
use crate::core::{
    ActorDelta, ComparisonOperator, DependencyMode, DependencyRule, Event, EventConditionType, EventCondition,
    EventType, MetricRef, Scenario, WorldState,
};
use serde::Serialize;

pub mod interactions;

/// Validate that every non-Linear dependency rule carries the threshold its mode
/// needs.
///
/// Split out from `validate_dependencies` so the single load choke point
/// (`scenarios::registry::validate_scenario`) can enforce it for *every* scenario
/// without needing that scenario's `KNOWN_METRICS` — only the from/to metric-name
/// check in `validate_dependencies` needs those. This guarantees the threshold
/// invariant relied on by `apply_dependency_rule` even for a scenario that forgets
/// its own per-scenario validation call.
pub fn validate_dependency_thresholds(rules: &[DependencyRule]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for rule in rules {
        // threshold is required for non-Linear modes
        match rule.mode {
            DependencyMode::Linear => {}
            _ => {
                if rule.threshold.is_none() {
                    errors.push(format!(
                        "dependency rule '{}': threshold required for mode {:?}",
                        rule.id, rule.mode
                    ));
                }
            }
        }
        // `DeficitProportional` divides by the threshold, so `Some(0.0)` — which every
        // other mode accepts as an ordinary comparison point — would be a silent
        // infinity here. Rejected at load rather than guarded in the hot path, so the
        // hot path keeps exactly one branch per mode.
        if matches!(rule.mode, DependencyMode::DeficitProportional)
            && matches!(rule.threshold, Some(t) if t <= 0.0)
        {
            errors.push(format!(
                "dependency rule '{}': mode DeficitProportional requires threshold > 0 (got {:?})",
                rule.id, rule.threshold
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validate dependency rules against known metrics.
///
/// Runs at scenario load time (see each scenario's `load_dependencies`). Returns
/// every problem found instead of panicking so callers can surface a clear load
/// error rather than letting a malformed rule reach the per-tick hot path, where
/// a missing threshold would otherwise panic deep inside `apply_dependency_rule`.
pub fn validate_dependencies(
    rules: &[DependencyRule],
    known_metrics: &[&str],
) -> Result<(), Vec<String>> {
    // threshold presence (mode-required) — shared with the centralized choke point
    let mut errors = match validate_dependency_thresholds(rules) {
        Ok(()) => Vec::new(),
        Err(errs) => errs,
    };
    for rule in rules {
        // from and to must be known metrics
        if !known_metrics.contains(&rule.from.as_str()) {
            errors.push(format!(
                "dependency rule '{}': unknown 'from' metric '{}'",
                rule.id, rule.from
            ));
        }
        if !known_metrics.contains(&rule.to.as_str()) {
            errors.push(format!(
                "dependency rule '{}': unknown 'to' metric '{}'",
                rule.id, rule.to
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Apply a single dependency rule to an actor
/// Sequential mutation semantics - each rule reads the current state
/// of the actor (already modified by previous rules).
fn apply_dependency_rule(actor: &mut crate::core::Actor, rule: &DependencyRule) {
    let from_val = actor.get_metric(rule.from.as_str());
    // Non-Linear modes require `threshold`. `validate_dependency_thresholds` runs
    // centrally at load (`load_by_id` -> `validate_scenario`) for every scenario,
    // plus per-scenario in `validate_dependencies`, so `None` is unreachable for
    // any validated scenario. The `debug_assert!` makes a slip loud in debug/test
    // while the `_ => 0.0` arms below stay a safe no-op (delta 0.0) in release
    // rather than a per-tick panic. Note the assert targets only the missing-
    // threshold case; `_ => 0.0` also legitimately fires when `threshold` is
    // `Some` but the comparison guard does not hold (the normal per-tick case).
    debug_assert!(
        matches!(rule.mode, DependencyMode::Linear) || rule.threshold.is_some(),
        "dependency rule '{}': threshold required for mode {:?} reached hot path unvalidated",
        rule.id,
        rule.mode
    );
    let delta = match rule.mode {
        DependencyMode::Deficit => match rule.threshold {
            Some(threshold) if from_val < threshold => -((threshold - from_val) * rule.coefficient),
            _ => 0.0,
        },
        DependencyMode::Excess => match rule.threshold {
            Some(threshold) if from_val > threshold => -((from_val - threshold) * rule.coefficient),
            _ => 0.0,
        },
        DependencyMode::Bonus => match rule.threshold {
            Some(threshold) if from_val > threshold => (from_val - threshold) * rule.coefficient,
            _ => 0.0,
        },
        DependencyMode::Linear => from_val * rule.coefficient,
        // Priced on the *target's* stock, not on the source's units — see
        // `DependencyMode::DeficitProportional`. `threshold > 0` is a load-time
        // invariant (`validate_dependency_thresholds`), so the division is safe;
        // the `_ => 0.0` arm stays the no-op for an unvalidated scenario, exactly
        // as for the other three modes.
        DependencyMode::DeficitProportional => match rule.threshold {
            Some(threshold) if threshold > 0.0 && from_val < threshold => {
                -(actor.get_metric(rule.to.as_str()) * rule.coefficient
                    * (threshold - from_val)
                    / threshold)
            }
            _ => 0.0,
        },
    };
    if delta != 0.0 {
        actor.add_metric(rule.to.as_str(), delta);
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
    // Step 3b: mobilisation recovery — armies regrow toward the capacity their
    // population supports, before this tick's fighting. Placed here, immediately
    // ahead of the interaction phase, because that is where it was measured; moving
    // it changes the numbers in docs/investigation_military_source.md §4.
    phase_military_recovery(world);

    phase_interactions(world, scenario, event_log, rng);

    // Phase 3: Random events
    phase_random_events(world, scenario, event_log, rng);

    // Phase 4: Actor tag effects + displacement decay
    phase_actor_tags(world, scenario);

    // Phase 4b: Era progression (after tags settle)
    phase_era_progression(world, scenario, event_log);

    // Phase 5: Clamp metrics
    phase_clamp(world);

    // Phase 6: Events (thresholds, ranks, milestones, game mode, relevance)
    phase_events(world, scenario, event_log);

    // Phase 7: Actor collapses
    phase_collapses(world, scenario, event_log);

    // Phase 7b: Vassalage formation / dissolution (parallel to collapses).
    // Runs after collapses so dead actors are already pruned and never vassalized.
    phase_vassalage(world, event_log);

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
            let val_a = ratio_cond.metric_a.get(world);
            let val_b = ratio_cond.metric_b.get(world);

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

        // Apply via MetricRef - scope to actor if actor_id is set
        auto_delta.metric.apply(world, final_delta);
    }
}

/// Check auto_delta condition against world state. The key already carries its
/// scope — it was resolved against the block's `actor_id` at load.
fn check_auto_delta_condition(world: &WorldState, cond: &crate::core::DeltaCondition) -> bool {
    let value = cond.metric.get(world);
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
                        let current = actor.get_metric(effect.metric.as_str());
                        if current < floor {
                            actor.set_metric(effect.metric.as_str(), floor);
                        }
                    } else {
                        actor.add_metric(effect.metric.as_str(), effect.delta);
                    }
                }
            }
        }
    }
}

// ============================================================================
// Phase 3: Dependency graph and interactions
// ============================================================================

fn phase_military_recovery(world: &mut WorldState) {
    interactions::apply_military_recovery(world);
}

fn phase_interactions(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog, rng: &mut ChaCha8Rng) {
    // Apply dependency rules from scenario
    phase_apply_dependencies(world, scenario);

    // Calculate neighbor interactions (six types: military, trade, diplomatic, migration, vassalage, cultural)
    interactions::calculate_interactions(world, scenario, event_log, rng);

    // Vassal tribute over persistent vassalage relationships (not neighbor-pair
    // based, so applied once here rather than inside calculate_interactions).
    // Rolls RNG only when a vassalage exists, keeping vassalage-free scenarios
    // byte-identical.
    interactions::calculate_vassalage_interaction(world, event_log, rng);
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

    // Get foreground actor IDs.
    // Sorted for a deterministic order: `world.actors` is a HashMap, so this
    // collection order is randomized per process. `foreground_ids.choose(rng)`
    // below picks an element by index, so an unsorted order makes the chosen
    // event target vary run-to-run, breaking fixed-seed reproducibility.
    let mut foreground_ids: Vec<String> = world.actors.values()
        .filter(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground && !world.dead_actor_ids.contains(&a.id))
        .map(|a| a.id.clone())
        .collect();
    foreground_ids.sort();

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
                let value = cond.metric
                    .resolve(target_id)
                    .expect("event target actor id")
                    .get(world);
                cond.operator.evaluate(value, cond.value)
            });

            if !conditions_met {
                continue;
            }

            // Apply effects
            for (metric, delta) in &event.effects {
                metric
                    .resolve(target_id)
                    .expect("event target actor id")
                    .apply(world, *delta);
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
    // Decay cultural displacement progress
    for val in world.cultural_displacement_progress.values_mut() {
        *val = (*val - 5.0).max(0.0);
    }
    world.cultural_displacement_progress.retain(|_, v| *v > 0.0);

    apply_actor_tags(world, scenario);
}

// ============================================================================
// Phase 4b: Era progression
// ============================================================================

fn phase_era_progression(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog) {
    for era_def in &scenario.era_definitions {
        // Skip ancient (starting era)
        if era_def.era == crate::core::Era::Ancient { continue; }

        for actor in world.actors.values_mut() {
            // Skip if already at or past this era
            if actor.era >= era_def.era { continue; }
            // Skip if tick too early
            if world.tick < era_def.min_tick { continue; }

            // Count matching tags
            let matching = actor.tags.iter()
                .filter(|t| era_def.from_tags.contains(t))
                .count() as u32;

            if matching >= era_def.requires_tags {
                let old_era = actor.era.clone();
                actor.era = era_def.era.clone();

                let event = Event::new(
                    format!("era_{}_{}", actor.id, format!("{:?}", era_def.era).to_lowercase()),
                    world.tick,
                    world.year,
                    actor.id.clone(),
                    crate::core::EventType::Milestone,
                    true,
                    format!("{} перешёл из {:?} в {:?} эру", actor.name, old_era, era_def.era),
                );
                event_log.add(event);
            }
        }
    }
}

// ============================================================================
// Phase 5: Clamp metrics
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
            let value = vc.metric.get(world);
            let main_condition = value >= vc.threshold;

            // Check additional conditions
            let additional_ok = vc.additional_conditions.iter().all(|cond| {
                let metric_value = cond.metric.get(world);
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
// Phase 7b: Vassalage (formation / dissolution)
// ============================================================================

fn phase_vassalage(world: &mut WorldState, event_log: &mut EventLog) {
    interactions::check_vassalage(world, event_log);
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
    world.year = scenario.start_year + (world.tick / 2) as i32;
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
            for actor_tag in actor.actor_tags.values() {
                for (metric, modifier) in &actor_tag.metrics_modifier {
                    let current = actor.metrics.get(metric.as_str()).copied().unwrap_or(0.0);
                    actor.metrics.insert(metric.as_str().to_string(), current + *modifier as f64);
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
            } => eval_metric_condition(world, metric, actor_id, operator, *value),
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

/// Evaluate a milestone / rank metric condition.
///
/// The key is already a `MetricRef` — resolved against its sibling `actor_id` at
/// load, not re-parsed here. What still needs the `actor_id` is the one thing the
/// key cannot express:
///
/// **An actor-scoped condition on an actor that is not in the world is `false`,
/// not `0.0`.** Actors are *removed* from `world.actors` on collapse
/// (`world.actors.remove(&actor_id)`), and three live conditions across the three
/// scenarios are `less`-gated on exactly such actors — `rome_splits`
/// (`rome.cohesion < 30`, and `rome` is the actor that splits), the `rome_city`
/// rank condition (`rome.legitimacy < 20`), and constantinople's mamluk spawn
/// (`ottomans.cohesion < 40`, and the ottomans reach 0.00 military in most runs).
/// Reading a dead actor's metric as the `0.0` default would satisfy every one of
/// them, forever, from the tick the actor dies. `try_get` returns `None` for an
/// absent actor, which is what keeps that from happening.
///
/// An unscoped condition (`actor_id = None`) carries its scope in the key itself
/// and keeps the `0.0` default, exactly as before.
fn eval_metric_condition(
    world: &WorldState,
    metric: &MetricRef,
    actor_id: &Option<String>,
    operator: &ComparisonOperator,
    value: f64,
) -> bool {
    if actor_id.is_some() {
        let Some(current) = metric.try_get(world) else {
            return false; // the actor is not in the world
        };
        return compare(current, operator, &value);
    }
    compare(metric.get(world), operator, &value)
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

            // Spawn actor if configured
            if let Some(cfg) = &milestone.spawn_actor {
                // Idempotency: don't spawn if already exists
                if !world.actors.contains_key(&cfg.actor_id)
                    && !world.dead_actors.iter().any(|d| d.id == cfg.actor_id)
                {
                    use crate::core::{Actor, GeoCoordinate, NarrativeStatus, RegionRank, Religion, Culture};
                    
                    let actor = Actor {
                        id: cfg.actor_id.clone(),
                        name: cfg.label.clone(),
                        name_short: cfg.label.clone(),
                        region: cfg.actor_id.clone(),
                        region_rank: RegionRank::C,
                        era: scenario.era.clone(),
                        narrative_status: NarrativeStatus::Background,
                        tags: vec![],
                        metrics: cfg.initial_metrics.iter()
                            .map(|(k, v)| (k.as_str().to_string(), *v))
                            .collect(),
                        scenario_metrics: HashMap::new(),
                        // Neighbor edges from config. `get_neighbor_pairs` treats
                        // an edge as bidirectional (it dedups sorted pairs), so
                        // listing them on the spawned actor alone is enough for it
                        // to enter interactions — no need to mutate the neighbors of
                        // pre-existing actors.
                        neighbors: cfg.neighbors.clone(),
                        on_collapse: vec![],
                        actor_tags: HashMap::new(),
                        center: Some(GeoCoordinate { lat: cfg.lat, lng: cfg.lng }),
                        is_successor_template: false,
                        religion: Religion::Orthodox,
                        culture: Culture::Slavic,
                        minimum_survival_ticks: None,
                        leader: None,
                    };
                    
                    world.actors.insert(cfg.actor_id.clone(), actor);

                    // The other direction of each configured edge. The comment above
                    // is right about pairs and wrong about everything else: three
                    // readers walk an actor's OWN list — `besieged` in
                    // `check_collapses`, the overlord choice in `check_vassalage`,
                    // `condition_contact` in relevance — so a one-sided edge has a
                    // direction for them. France could be besieged by Savoy; Savoy,
                    // whose list never named France, could not be besieged by France
                    // (measured: 9 of 9 surviving Savoys would fall, 9 more earlier —
                    // docs/investigation_spawn_reverse_edges.md §2). A living
                    // neighbour that already lists the spawn keeps its own entry.
                    link_spawn_back(world, &cfg.actor_id, &cfg.neighbors);

                    // is_key event for spawn
                    let event = Event::new(
                        format!("spawn_{}", cfg.actor_id),
                        current_tick,
                        current_year,
                        cfg.actor_id.clone(),
                        EventType::Milestone,
                        true,
                        format!("{} появился на сцене истории.", cfg.label),
                    );
                    event_log.add(event);
                }
            }

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

/// Give every living neighbour listed by a freshly spawned actor the reverse entry,
/// with the same distance and border type, unless it already lists the spawn.
fn link_spawn_back(world: &mut WorldState, spawn_id: &str, edges: &[crate::core::Neighbor]) {
    for edge in edges {
        if edge.id == spawn_id {
            continue;
        }
        if let Some(other) = world.actors.get_mut(&edge.id) {
            if !other.neighbors.iter().any(|n| n.id == spawn_id) {
                other.neighbors.push(crate::core::Neighbor {
                    id: spawn_id.to_string(),
                    distance: edge.distance,
                    border_type: edge.border_type.clone(),
                });
            }
        }
    }
}

/// Apply one-time effects for specific milestone events
fn apply_milestone_effects(world: &mut WorldState, milestone_id: &str) {
    if milestone_id == "mehmed_accelerates" {
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
}

/// Check and handle game mode transitions
/// Scenario → Consequences: automatic when milestone with triggers_collapse fires
/// "Split as shrink": a milestone with `triggers_collapse` divides the actor its
/// condition names instead of killing it.
///
/// The literal reading of `ENGINE13_ARCHITECTURE.md` — run `on_collapse`, i.e. kill
/// the parent and bear both heirs — was implemented as a probe and measured: it
/// deletes the actor 82 content sites address, hands the heirs only their authored
/// distance-2 edges (so the limes border dies with Rome and the barbarians stop being
/// besiegeable), and leaves both halves defenceless and immortal. Shrinking instead
/// keeps every one of those sites addressing a living actor, keeps the border, and
/// reproduces the historical shape on its own: the West ends as a rump, the East as
/// the real power. Details and the measured comparison:
/// docs/investigation_split_as_shrink.md §4, §11.
///
/// The seat-keeping heir is marked in the content (`Successor::keeps_seat`); it is
/// renamed after its template and its shares are cut, but its `cohesion`,
/// `legitimacy` and `external_pressure` are **left alone** — overwriting the state of
/// an actor that already exists is the defect class PR #47 removed from successor
/// entry. The newborn heir has no prior state to overwrite, so it takes the
/// architecture's trauma values.
///
/// No RNG is drawn.
fn apply_seat_split(
    world: &mut WorldState,
    scenario: &Scenario,
    milestone: &crate::core::MilestoneEvent,
    event_log: &mut EventLog,
) {
    let actor_id = match &milestone.condition.condition_type {
        crate::core::EventConditionType::Metric { actor_id, .. } => actor_id.clone(),
        crate::core::EventConditionType::ActorState { actor_id, .. } => Some(actor_id.clone()),
        crate::core::EventConditionType::Tick { .. } => None,
    };
    let Some(actor_id) = actor_id else { return };
    let Some(parent) = world.actors.get(&actor_id) else { return };
    let heirs = parent.on_collapse.clone();
    let Some(seat) = heirs.iter().find(|h| h.keeps_seat).cloned() else { return };
    let parent_metrics = parent.metrics.clone();
    let parent_name = parent.name.clone();
    let total: f64 = heirs.iter().map(|h| h.weight).sum();
    if total <= 0.0 {
        return;
    }

    // The architecture's split formula. `trauma` covers the three metrics it fixes
    // outright; they are applied only to an heir that is being born.
    let cut = |src: &HashMap<String, f64>, share: f64, trauma: bool| -> HashMap<String, f64> {
        let g = |k: &str| src.get(k).copied().unwrap_or(0.0);
        let mut m = src.clone();
        m.insert("population".to_string(), g("population") * share);
        m.insert("military_size".to_string(), g("military_size") * share * 0.7);
        m.insert("treasury".to_string(), g("treasury") * share * 0.5);
        m.insert("military_quality".to_string(), g("military_quality") * 0.8);
        m.insert("economic_output".to_string(), g("economic_output") * 0.7);
        if trauma {
            m.insert("cohesion".to_string(), 20.0);
            m.insert("legitimacy".to_string(), 30.0);
            m.insert("external_pressure".to_string(), (g("external_pressure") * 1.3).min(100.0));
        }
        m
    };

    // The seat: same id, same neighbours, new name, reduced share.
    let seat_name = scenario
        .actors
        .iter()
        .find(|a| a.id == seat.id)
        .map(|t| (t.name.clone(), t.name_short.clone()));
    if let Some(p) = world.actors.get_mut(&actor_id) {
        p.metrics = cut(&parent_metrics, seat.weight / total, false);
        if let Some((name, short)) = seat_name {
            p.name = name;
            p.name_short = short;
        }
    }

    // Every other declared heir separates, born from the parent's living metrics.
    for heir in heirs.iter().filter(|h| !h.keeps_seat) {
        if world.actors.contains_key(&heir.id) || world.dead_actor_ids.contains(&heir.id) {
            continue;
        }
        let Some(tpl) = scenario.actors.iter().find(|a| a.id == heir.id) else { continue };
        let mut new_actor = tpl.clone();
        new_actor.metrics = cut(&parent_metrics, heir.weight / total, true);
        crate::core::actor::ensure_default_metrics(&mut new_actor.metrics);
        new_actor.narrative_status = crate::core::NarrativeStatus::Foreground;
        new_actor.is_successor_template = false;
        if new_actor.neighbors.is_empty() {
            new_actor.neighbors = world
                .actors
                .get(&actor_id)
                .map(|p| p.neighbors.clone())
                .unwrap_or_default();
        }
        let edges = new_actor.neighbors.clone();
        let heir_name = new_actor.name.clone();
        world.actors.insert(heir.id.clone(), new_actor);
        // The other side of each edge, as for a spawn (PR #52): three readers walk an
        // actor's own list, so a one-sided edge has a direction for them.
        for edge in &edges {
            if let Some(other) = world.actors.get_mut(&edge.id) {
                if !other.neighbors.iter().any(|n| n.id == heir.id) {
                    other.neighbors.push(crate::core::Neighbor {
                        id: heir.id.clone(),
                        distance: edge.distance,
                        border_type: edge.border_type.clone(),
                    });
                }
            }
        }
        event_log.add(
            Event::new(
                format!("birth_{}", heir.id),
                world.tick,
                world.year,
                heir.id.clone(),
                EventType::Birth,
                true,
                format!("Держава {} отделилась от державы {}", heir_name, parent_name),
            )
            .with_tags(vec!["birth".to_string(), heir.id.clone()]),
        );
    }
}

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
            // The scenario's turning point actually happens to the world now, not
            // only in the chronicler's text: the actor the condition names shrinks to
            // its share and the other heir separates. See
            // docs/investigation_split_as_shrink.md §11.
            apply_seat_split(world, scenario, milestone, event_log);

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
            let history = world.metric_history.entry(key).or_default();
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
        } => eval_metric_condition(world, metric, actor_id, operator, *value),
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
    if world.tick.is_multiple_of(2) {
        family_state.patriarch_age += 1;
    }

    // Check triggers
    let patriarch_age = family_state.patriarch_age;
    let normal_trigger = patriarch_age >= gen_mechanics.patriarch_end_age;

    // For early trigger, we need to check external metric - do this after dropping family_state borrow
    let early_trigger_check = gen_mechanics.early_transfer.as_ref().map(|early| {
        (early.age, early.condition_metric.clone(), early.condition_operator.clone(), early.condition_value)
    });

    // Drop the mutable borrow before checking external metric
    let _ = family_state; // End mutable borrow scope

    // Check early trigger condition (needs world access)
    let early_trigger = early_trigger_check.is_some_and(|(age, metric, operator, value)| {
        if patriarch_age < age {
            return false;
        }
        let metric_value = metric.get(world);
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
        family_state.patriarch_age = gen_mechanics.patriarch_start_age;

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

    // Get actor IDs to avoid borrow conflict with collapse_warning_ticks.
    //
    // Sorted, because `world.actors` is a std `HashMap` whose iteration order is
    // random per process. When two actors reach their third warning tick on the
    // same tick — milan and genoa do, in 8 of 30 no-player milan runs — the order
    // in which they are processed below decides the order of the `Death` events
    // in the log, the order of `dead_actors`, and, for a parent and its heir
    // falling together, absorption versus nothing. Measured before this line: 8
    // processes of one seed gave 2…6 distinct outputs (3! for a triple death),
    // identical up to line order. See docs/investigation_collapse_order.md.
    let mut actor_ids: Vec<String> = world.actors.keys().cloned().collect();
    actor_ids.sort();

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

        // Path 3: conquest by exhaustion — no army left, no authority, saturated
        // pressure, and an armed neighbour on the border still able to finish the job.
        //
        // Paths 1 and 2 both require low `cohesion`, and until the combat termination
        // guard landed the *only* thing that pushed cohesion down was the defender's
        // per-fight `cohesion_loss` — including in fights against an army that no
        // longer existed. Mortality was therefore a by-product of that defect: remove
        // the phantom fights and cohesion recovers (nothing pushes it back), leaving a
        // defenceless actor immortal — legitimacy 0, external_pressure 100, no army,
        // and alive forever. See docs/investigation_combat_self_destruction.md.
        //
        // The besieged clause is what makes this a *conquest* condition rather than a
        // blanket one. In the no-player world `legitimacy → 0` and
        // `external_pressure → 100` for nearly every actor, so those two gates alone
        // discriminate nothing: without the clause this predicate kills 12 of Rome's
        // actors and both protagonists (byzantium at median tick 41, milan at 71) in
        // 20/20 runs. With it, mortality lands back on the actors that a hostile border
        // was actually grinding down.
        //
        // `MIN_DEFENSIBLE_MILITARY` on both sides is the same belligerence test the
        // combat guard uses: you die to a neighbour that could still fight you, and only
        // once you no longer can.
        let besieged = actor.neighbors.iter().any(|n| {
            n.distance == 1
                && world
                    .actors
                    .get(&n.id)
                    .map(|nb| {
                        nb.get_metric("military_size")
                            >= crate::engine::interactions::MIN_DEFENSIBLE_MILITARY
                    })
                    .unwrap_or(false)
        });
        let conquest_collapse =
            actor.get_metric("military_size") < crate::engine::interactions::MIN_DEFENSIBLE_MILITARY
            && actor.get_metric("legitimacy") < 10.0
            && actor.get_metric("external_pressure") > 85.0
            && besieged;

        let in_danger = classic_collapse || internal_collapse || conquest_collapse;

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
        // Human-readable name of the power that just fell. Captured here because
        // the actor is removed from `world.actors` a few lines below, while the
        // successor loop that needs the name runs after that removal.
        let mut parent_name = actor_id.clone();
        // The fallen power's border, captured for the same reason: an heir born
        // below with no authored neighbours stands where the parent stood.
        let mut parent_neighbors: Vec<crate::core::Neighbor> = Vec::new();

        // Record death event
        if let Some(actor) = world.actors.get(&actor_id) {
            parent_name = actor.name.clone();
            parent_neighbors = actor.neighbors.clone();
            let event = Event::new(
                format!("death_{}", actor_id),
                current_tick,
                current_year,
                actor_id.clone(),
                EventType::Death,
                true,
                // Prefixed with "Держава" rather than agreeing with the actor name:
                // actor names in the content are of every gender and number
                // ("Византия", "Остготы", "Савойя"), and the old wording produced
                // "Византия прекратил существование" in a prompt that demands
                // Russian prose. The prefix agrees with itself and is name-agnostic.
                format!("Держава {} прекратила существование", actor.name),
            )
            .with_metrics_snapshot(metrics_to_snapshot(&actor.metrics))
            .with_tags(vec!["collapse".to_string(), actor_id.clone()]);

            event_log.add(event);

            // Move to dead_actors and add to dead_actor_ids HashSet
            let dead_actor = crate::core::DeadActor {
                id: actor_id.clone(),
                name: actor.name.clone(),
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

        // Heirs. Three outcomes per declared id — and `registry::validate_scenario`
        // guarantees the id names an actor of this scenario, so "no template" is
        // not a fourth one any more:
        //
        //   * a template (or a not-yet-living actor) → born, with its authored
        //     metrics VERBATIM;
        //   * a living power → absorption: no new actor, the heir's shared
        //     expansion counter is credited (the `else` branch below);
        //   * a dead actor → nothing. Without this guard `milan`, heir of `savoy`,
        //     came back from the dead in 2 of 30 no-player runs whenever savoy fell
        //     after it — the protagonist resurrected past its own defeat screen.
        //
        // "Verbatim" is deliberate and replaces `split_metrics_for_successor`. That
        // function implemented the architecture's split formula (`родитель × …` on
        // every line), but its only caller ever fed it the heir's OWN template, so
        // 7 of the 8 authored values were overwritten on entry (`ostrogoth_kingdom`:
        // ep 35→45.5, coh 50→20, leg 35→30, mil 50→35, …), and `rome_west` /
        // `rome_east` — whose templates already carry the parent's share by hand
        // (3600 = 8000 × 0.45) — would have been split a second time. Feeding it
        // the parent instead is degenerate by construction: every collapse path
        // requires ep > 85 or leg < 5, so the heir would be born at ep = 100 with
        // the parent's zero army and inherit the death itself. The formula stays
        // in the architecture as the unimplemented procedural split of a LIVING
        // power. See docs/investigation_successor_entry.md §4.
        for successor in &successors {
            if world.dead_actor_ids.contains(&successor.id) {
                continue;
            }
            if !world.actors.contains_key(&successor.id) {
                if let Some(scenario_actor) = scenario.actors.iter().find(|a| a.id == successor.id) {
                    let mut new_actor = scenario_actor.clone();
                    crate::core::actor::ensure_default_metrics(&mut new_actor.metrics);
                    new_actor.narrative_status = crate::core::NarrativeStatus::Foreground;
                    new_actor.is_successor_template = false; // Clear the template flag for the actual actor
                    // Succession of the border. Five of rome's seven templates are
                    // authored with `neighbors: vec![]` and no living actor lists
                    // them, so an heir used to enter a world in which it was in no
                    // pair at all: `ostrogoth_kingdom` lived 52–79 ticks with
                    // `military_size` and `external_pressure` frozen at their
                    // template values, never fought, never migrated, and died of
                    // isolation. An authored list (`rome_west`, `rome_east`) wins;
                    // an empty one inherits the parent's edges.
                    if new_actor.neighbors.is_empty() {
                        new_actor.neighbors = parent_neighbors.clone();
                    }
                    let successor_name = new_actor.name.clone();
                    world.actors.insert(successor.id.clone(), new_actor);

                    // The other direction. Three readers walk an actor's OWN list —
                    // `besieged` in this function, the overlord choice in
                    // `check_vassalage`, and `condition_contact` in relevance — so a
                    // one-sided edge has a direction for them: the heir could be
                    // besieged by the huns, the huns never by the heir, because
                    // their list still named the dead parent. A sole heir takes the
                    // parent's place in every living neighbour's list; with two or
                    // more heirs (rome → west + east) the replacement is undefined
                    // and the heirs' authored lists carry the split instead.
                    // Per-actor and order-independent, so HashMap iteration is safe.
                    // See docs/investigation_successor_edges.md §2, §5.
                    if successors.len() == 1 {
                        for (other_id, other) in world.actors.iter_mut() {
                            if *other_id == successor.id {
                                continue;
                            }
                            if other.neighbors.iter().any(|n| n.id == successor.id) {
                                other.neighbors.retain(|n| n.id != actor_id);
                            } else {
                                for n in other.neighbors.iter_mut() {
                                    if n.id == actor_id {
                                        n.id = successor.id.clone();
                                    }
                                }
                            }
                        }
                    }

                    // Birth of a successor as a chronicle event.
                    //
                    // Until this, the birth of an heir was the only actor-lifecycle
                    // transition the engine performed silently: `EventType::Birth`
                    // had zero producers in the whole codebase, so `ostrogoth_kingdom`
                    // could appear in rome_375, live 30 half-years and fall, and none
                    // of the three appearances reached the chronicler. Tagged the same
                    // way the death event is, so the canonical relevance selection
                    // ranks it the same way (`db::thematic_similarity`).
                    let event = Event::new(
                        format!("birth_{}", successor.id),
                        current_tick,
                        current_year,
                        successor.id.clone(),
                        EventType::Birth,
                        true,
                        format!(
                            "Держава {} возникла на месте, которое занимала держава {}",
                            successor_name, parent_name
                        ),
                    )
                    .with_tags(vec!["birth".to_string(), successor.id.clone()]);
                    event_log.add(event);
                }
            } else {
                // Heir is an already-living power: this is full absorption via
                // collapse, not a split into a fresh successor. Credit the shared
                // expansion counter (same counter vassalage formation increments;
                // consumed by the coalition trigger in task D).
                if let Some(heir) = world.actors.get_mut(&successor.id) {
                    heir.add_metric("expansion_count", 1.0);
                }
            }
        }
    }
}

fn metrics_to_snapshot(metrics: &HashMap<String, f64>) -> HashMap<String, f64> {
    crate::core::actor::metrics_to_snapshot(metrics)
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

    /// Resolve a content key exactly as load does: against its sibling `actor_id`.
    fn mr(metric: &str, actor_id: Option<&str>) -> MetricRef {
        crate::core::resolve_at_load(metric, actor_id).expect("test metric key")
    }

    fn empty_scenario() -> Scenario {
        Scenario {
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
            tag_definitions: vec![],
            era_definitions: vec![],
        }
    }

    #[test]
    fn test_tick_advances_time() {
        let mut world = WorldState::new("test".to_string(), 375);
        let scenario = empty_scenario();
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

    // ------------------------------------------------------------------
    // Metric-condition resolution (task 4): check_event_condition /
    // check_rank_conditions must resolve global:/family:/actor: scopes the
    // same way victory_condition does, via the shared eval_metric_condition.
    // ------------------------------------------------------------------

    #[test]
    fn eval_metric_condition_resolves_all_scopes() {
        let mut world = WorldState::new("test".to_string(), 1430);
        world.global_metrics.insert("federation_progress".to_string(), 90.0);
        world.family_state = Some(crate::core::FamilyState {
            metrics: HashMap::from([("influence".to_string(), 75.0)]),
            patriarch_age: 40,
            generation_count: 0,
        });
        world
            .actors
            .insert("ottomans".into(), vassalage_actor("ottomans", 260.0, 30.0, 60.0, 60.0, &[]));

        // global: scope with actor_id = None — was unconditionally false before the fix.
        assert!(eval_metric_condition(&world, &mr("global:federation_progress", None), &None, &ComparisonOperator::Greater, 60.0));
        assert!(!eval_metric_condition(&world, &mr("global:federation_progress", None), &None, &ComparisonOperator::Greater, 95.0));

        // family: scope with actor_id = None.
        assert!(eval_metric_condition(&world, &mr("family:influence", None), &None, &ComparisonOperator::Greater, 70.0));
        assert!(!eval_metric_condition(&world, &mr("family:influence", None), &None, &ComparisonOperator::Less, 70.0));

        // actor:id.metric embedded in the metric string (rank-condition style, actor_id = None).
        assert!(eval_metric_condition(&world, &mr("actor:ottomans.military_size", None), &None, &ComparisonOperator::Greater, 250.0));

        // Split actor-scoped form (actor_id = Some) still works unchanged.
        assert!(eval_metric_condition(&world, &mr("military_size", Some("ottomans")), &Some("ottomans".to_string()), &ComparisonOperator::Greater, 250.0));

        // Missing actor in the split form stays false — NOT a 0.0 comparison.
        // (a `less` check must not newly fire just because the actor is absent).
        assert!(!eval_metric_condition(&world, &mr("military_size", Some("nonexistent")), &Some("nonexistent".to_string()), &ComparisonOperator::Less, 999.0));
    }

    #[test]
    fn check_event_condition_fires_global_and_family_scoped_milestones() {
        let mut world = WorldState::new("test".to_string(), 1430);
        world.global_metrics.insert("federation_progress".to_string(), 85.0);
        world.family_state = Some(crate::core::FamilyState {
            metrics: HashMap::from([("influence".to_string(), 20.0)]),
            patriarch_age: 40,
            generation_count: 0,
        });

        // constantinople_1430 `outcome_fell_federation`: global:federation_progress >= 80.
        let global_cond = EventCondition {
            condition_type: EventConditionType::Metric {
                metric: MetricRef::literal("global:federation_progress"),
                actor_id: None,
                operator: ComparisonOperator::GreaterOrEqual,
                value: 80.0,
            },
            duration: None,
        };
        assert!(check_event_condition(&world, &global_cond));

        // rome_375 `family_falls`: family:family_influence below a floor.
        let family_cond = EventCondition {
            condition_type: EventConditionType::Metric {
                metric: MetricRef::literal("family:family_influence"),
                actor_id: None,
                operator: ComparisonOperator::Less,
                value: 25.0,
            },
            duration: None,
        };
        assert!(check_event_condition(&world, &family_cond));
    }

    // ------------------------------------------------------------------
    // Vassalage (task A)
    // ------------------------------------------------------------------

    fn vassalage_actor(id: &str, military: f64, ep: f64, leg: f64, coh: f64, neighbors: &[&str]) -> crate::core::Actor {
        use crate::core::{Actor, BorderType, Culture, Era, NarrativeStatus, Neighbor, RegionRank, Religion};
        let mut metrics = crate::core::actor::default_metrics();
        metrics.insert("military_size".to_string(), military);
        metrics.insert("external_pressure".to_string(), ep);
        metrics.insert("legitimacy".to_string(), leg);
        metrics.insert("cohesion".to_string(), coh);
        metrics.insert("economic_output".to_string(), 50.0);
        metrics.insert("treasury".to_string(), 100.0);
        Actor {
            id: id.to_string(),
            name: id.to_string(),
            name_short: id.to_string(),
            region: id.to_string(),
            region_rank: RegionRank::C,
            era: Era::LateMedieval,
            narrative_status: NarrativeStatus::Foreground,
            tags: vec![],
            metrics,
            scenario_metrics: HashMap::new(),
            neighbors: neighbors.iter().map(|n| Neighbor { id: n.to_string(), distance: 1, border_type: BorderType::Land }).collect(),
            on_collapse: vec![],
            actor_tags: HashMap::new(),
            center: None,
            is_successor_template: false,
            religion: Religion::Catholic,
            culture: Culture::Latin,
            minimum_survival_ticks: None,
            leader: None,
        }
    }

    #[test]
    fn test_vassalage_forms_after_three_ticks_and_pays_tribute() {
        let mut world = WorldState::new("test".to_string(), 1477);
        // Weak actor sitting inside the danger band, strong healthy neighbour.
        world.actors.insert("small".into(), vassalage_actor("small", 10.0, 78.0, 18.0, 22.0, &["big"]));
        world.actors.insert("big".into(), vassalage_actor("big", 100.0, 30.0, 60.0, 60.0, &["small"]));
        let mut log = EventLog::new();

        // Needs 3 consecutive ticks in band before forming.
        interactions::check_vassalage(&mut world, &mut log);
        assert!(world.vassalages.is_empty(), "must not form before 3 ticks");
        interactions::check_vassalage(&mut world, &mut log);
        assert!(world.vassalages.is_empty(), "must not form before 3 ticks");
        interactions::check_vassalage(&mut world, &mut log);

        assert_eq!(world.vassalages.len(), 1);
        let v = &world.vassalages[0];
        assert_eq!(v.vassal_id, "small");
        assert_eq!(v.overlord_id, "big");
        // Overlord's shared expansion counter incremented on formation.
        assert_eq!(world.actors.get("big").unwrap().get_metric("expansion_count"), 1.0);

        // Tribute: 3–5% of vassal economic_output (50.0) => 1.5..=2.5, symmetric.
        let vassal_before = world.actors.get("small").unwrap().get_metric("treasury");
        let overlord_before = world.actors.get("big").unwrap().get_metric("treasury");
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
        interactions::calculate_vassalage_interaction(&mut world, &mut log, &mut rng);
        let paid = vassal_before - world.actors.get("small").unwrap().get_metric("treasury");
        let received = world.actors.get("big").unwrap().get_metric("treasury") - overlord_before;
        assert!((paid - received).abs() < 1e-9, "tribute must be symmetric");
        assert!((1.5..=2.5).contains(&paid), "tribute {paid} out of 3-5% band");
    }

    #[test]
    fn test_vassalage_overlord_is_never_a_vassal() {
        // No hierarchy: a vassal can never gain a vassal, so overlord attribution
        // must skip a neighbour that is itself a vassal — even if it is the
        // strongest one available.
        let mut world = WorldState::new("test".to_string(), 1477);
        world.actors.insert("small".into(), vassalage_actor("small", 10.0, 78.0, 18.0, 22.0, &["free", "v"]));
        world.actors.insert("free".into(), vassalage_actor("free", 50.0, 30.0, 60.0, 60.0, &["small"]));
        world.actors.insert("v".into(), vassalage_actor("v", 100.0, 30.0, 60.0, 60.0, &["small", "lord"]));
        world.actors.insert("lord".into(), vassalage_actor("lord", 200.0, 30.0, 60.0, 60.0, &["v"]));
        // "v" is already a vassal of a healthy "lord", so it stays bound.
        world.vassalages.push(crate::core::Vassalage { vassal_id: "v".into(), overlord_id: "lord".into(), formed_tick: 0 });
        let mut log = EventLog::new();

        for _ in 0..3 {
            interactions::check_vassalage(&mut world, &mut log);
        }

        // "small" submits — but to "free", not to the stronger vassal "v".
        assert_eq!(world.vassalages.len(), 2);
        let small_v = world.vassalages.iter().find(|v| v.vassal_id == "small").expect("small should be a vassal");
        assert_eq!(small_v.overlord_id, "free", "must not pick a vassal as overlord");
        assert_eq!(world.actors.get("free").unwrap().get_metric("expansion_count"), 1.0);
    }

    #[test]
    fn test_vassalage_revolt_conditions() {
        let mut world = WorldState::new("test".to_string(), 1477);
        world.actors.insert("small".into(), vassalage_actor("small", 10.0, 30.0, 60.0, 60.0, &["big"]));
        world.actors.insert("big".into(), vassalage_actor("big", 20.0, 30.0, 60.0, 60.0, &["small"]));
        world.vassalages.push(crate::core::Vassalage { vassal_id: "small".into(), overlord_id: "big".into(), formed_tick: 0 });
        let mut log = EventLog::new();

        // Stable: vassal weak (10 < 80% of 20) and overlord healthy.
        interactions::check_vassalage(&mut world, &mut log);
        assert_eq!(world.vassalages.len(), 1, "must stay bound while weak");

        // Revolt path 1: vassal's military catches up (>= 80% of overlord).
        world.actors.get_mut("small").unwrap().set_metric("military_size", 16.0); // 16 >= 20*0.8
        interactions::check_vassalage(&mut world, &mut log);
        assert!(world.vassalages.is_empty(), "vassal must revolt once strong enough");

        // Revolt path 2: overlord itself enters the FULL vassalage band (all three
        // metrics together), not merely one slipped metric.
        world.actors.get_mut("small").unwrap().set_metric("military_size", 5.0);
        world.vassalages.push(crate::core::Vassalage { vassal_id: "small".into(), overlord_id: "big".into(), formed_tick: 0 });
        // Only external_pressure in band — legitimacy/cohesion still healthy: must NOT revolt.
        let big = world.actors.get_mut("big").unwrap();
        big.set_metric("external_pressure", 75.0);
        interactions::check_vassalage(&mut world, &mut log);
        assert_eq!(world.vassalages.len(), 1, "single slipped metric must not free the vassal");
        // Now drive legitimacy and cohesion into band too → full band → revolt.
        let big = world.actors.get_mut("big").unwrap();
        big.set_metric("legitimacy", 18.0);
        big.set_metric("cohesion", 22.0);
        interactions::check_vassalage(&mut world, &mut log);
        assert!(world.vassalages.is_empty(), "vassal must break free from a fully-weakened overlord");
    }

    // ------------------------------------------------------------------
    // Successor entry (docs/investigation_successor_entry.md)
    // ------------------------------------------------------------------

    /// An actor already inside the `internal_collapse` band (`leg < 5`, `coh < 8`);
    /// three consecutive `check_collapses` calls kill it.
    fn doomed_actor(id: &str, heirs: &[&str]) -> crate::core::Actor {
        let mut a = vassalage_actor(id, 0.0, 100.0, 1.0, 1.0, &[]);
        a.on_collapse = heirs
            .iter()
            .map(|h| crate::core::Successor { id: h.to_string(), weight: 1.0, keeps_seat: false })
            .collect();
        a
    }

    fn kill(world: &mut WorldState, scenario: &Scenario, log: &mut EventLog) {
        for _ in 0..3 {
            check_collapses(world, scenario, log);
        }
    }

    #[test]
    fn heir_template_enters_world_verbatim() {
        let mut template = vassalage_actor("heir", 50.0, 35.0, 35.0, 50.0, &[]);
        template.is_successor_template = true;
        let mut scenario = empty_scenario();
        scenario.actors = vec![doomed_actor("parent", &["heir"]), template.clone()];
        let mut world = WorldState::new("test".into(), 375);
        world.actors.insert("parent".into(), doomed_actor("parent", &["heir"]));
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        assert!(world.dead_actor_ids.contains("parent"));
        let heir = world.actors.get("heir").expect("heir must be born");
        assert!(!heir.is_successor_template);
        for (k, v) in &template.metrics {
            assert_eq!(heir.get_metric(k), *v, "metric {k} must enter verbatim");
        }
        // The values the old split used to overwrite.
        assert_eq!(heir.get_metric("external_pressure"), 35.0);
        assert_eq!(heir.get_metric("cohesion"), 50.0);
        assert_eq!(heir.get_metric("legitimacy"), 35.0);
        assert_eq!(heir.get_metric("military_size"), 50.0);
        assert_eq!(
            log.events.iter().filter(|e| e.event_type == EventType::Birth && e.actor_id == "heir").count(),
            1
        );
    }

    #[test]
    fn dead_heir_is_not_resurrected() {
        let mut scenario = empty_scenario();
        scenario.actors = vec![doomed_actor("first", &[]), doomed_actor("second", &["first"])];
        let mut world = WorldState::new("test".into(), 375);
        world.actors.insert("first".into(), doomed_actor("first", &[]));
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);
        assert!(world.dead_actor_ids.contains("first"));

        world.actors.insert("second".into(), doomed_actor("second", &["first"]));
        kill(&mut world, &scenario, &mut log);

        assert!(world.dead_actor_ids.contains("second"));
        assert!(!world.actors.contains_key("first"), "a dead heir must stay dead");
        assert_eq!(log.events.iter().filter(|e| e.event_type == EventType::Birth).count(), 0);
        assert_eq!(world.dead_actors.len(), 2);
    }

    #[test]
    fn same_tick_deaths_are_processed_in_id_order() {
        // Three actors in the collapse band from tick 0: all three reach their
        // third warning on the same `check_collapses` call. The processing order
        // must not depend on HashMap iteration order.
        let mut scenario = empty_scenario();
        scenario.actors = vec![doomed_actor("milan", &[]), doomed_actor("genoa", &[]), doomed_actor("france", &[])];
        let mut world = WorldState::new("test".into(), 375);
        for id in ["milan", "genoa", "france"] {
            world.actors.insert(id.into(), doomed_actor(id, &[]));
        }
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        let dead: Vec<&str> = world.dead_actors.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(dead, ["france", "genoa", "milan"]);
        let deaths: Vec<&str> = log.events.iter()
            .filter(|e| e.event_type == EventType::Death)
            .map(|e| e.actor_id.as_str())
            .collect();
        assert_eq!(deaths, ["france", "genoa", "milan"]);
    }

    fn neighbor_ids(world: &WorldState, id: &str) -> Vec<String> {
        world.actors.get(id).unwrap().neighbors.iter().map(|n| n.id.clone()).collect()
    }

    #[test]
    fn heir_with_empty_list_inherits_parent_edges_and_takes_its_place() {
        let mut template = vassalage_actor("heir", 50.0, 35.0, 35.0, 50.0, &[]);
        template.is_successor_template = true;
        let mut parent = doomed_actor("parent", &["heir"]);
        parent.neighbors = vassalage_actor("parent", 0.0, 0.0, 0.0, 0.0, &["huns", "rome"]).neighbors;
        let huns = vassalage_actor("huns", 120.0, 5.0, 60.0, 72.0, &["parent"]);
        let rome = vassalage_actor("rome", 350.0, 38.0, 62.0, 42.0, &["parent", "huns"]);
        let mut scenario = empty_scenario();
        scenario.actors = vec![parent.clone(), template, huns.clone(), rome.clone()];
        let mut world = WorldState::new("test".into(), 375);
        world.actors.insert("parent".into(), parent);
        world.actors.insert("huns".into(), huns);
        world.actors.insert("rome".into(), rome);
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        assert_eq!(neighbor_ids(&world, "heir"), ["huns", "rome"], "empty list inherits the parent's");
        assert_eq!(neighbor_ids(&world, "huns"), ["heir"], "sole heir replaces the parent");
        assert_eq!(neighbor_ids(&world, "rome"), ["heir", "huns"], "other entries untouched");
        let d = world.actors.get("huns").unwrap().neighbors[0].distance;
        assert_eq!(d, 1, "distance and border type of the retargeted edge are kept");
    }

    #[test]
    fn heir_with_authored_list_keeps_it() {
        let mut template = vassalage_actor("heir", 50.0, 35.0, 35.0, 50.0, &["rome"]);
        template.is_successor_template = true;
        let mut parent = doomed_actor("parent", &["heir"]);
        parent.neighbors = vassalage_actor("parent", 0.0, 0.0, 0.0, 0.0, &["huns"]).neighbors;
        let huns = vassalage_actor("huns", 120.0, 5.0, 60.0, 72.0, &["parent"]);
        let rome = vassalage_actor("rome", 350.0, 38.0, 62.0, 42.0, &[]);
        let mut scenario = empty_scenario();
        scenario.actors = vec![parent.clone(), template, huns.clone(), rome.clone()];
        let mut world = WorldState::new("test".into(), 375);
        world.actors.insert("parent".into(), parent);
        world.actors.insert("huns".into(), huns);
        world.actors.insert("rome".into(), rome);
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        assert_eq!(neighbor_ids(&world, "heir"), ["rome"], "authored list wins over inheritance");
        assert_eq!(neighbor_ids(&world, "huns"), ["heir"], "retargeting is independent of the heir's own list");
    }

    #[test]
    fn two_heirs_do_not_retarget_neighbours() {
        let mut west = vassalage_actor("west", 50.0, 35.0, 35.0, 50.0, &["huns"]);
        west.is_successor_template = true;
        let mut east = vassalage_actor("east", 50.0, 35.0, 35.0, 50.0, &[]);
        east.is_successor_template = true;
        let mut parent = doomed_actor("parent", &["west", "east"]);
        parent.neighbors = vassalage_actor("parent", 0.0, 0.0, 0.0, 0.0, &["huns"]).neighbors;
        let huns = vassalage_actor("huns", 120.0, 5.0, 60.0, 72.0, &["parent"]);
        let mut scenario = empty_scenario();
        scenario.actors = vec![parent.clone(), west, east, huns.clone()];
        let mut world = WorldState::new("test".into(), 375);
        world.actors.insert("parent".into(), parent);
        world.actors.insert("huns".into(), huns);
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        assert_eq!(neighbor_ids(&world, "huns"), ["parent"], "split: replacement undefined, list left as authored");
        assert_eq!(neighbor_ids(&world, "east"), ["huns"], "empty heir list still inherits");
        assert_eq!(neighbor_ids(&world, "west"), ["huns"]);
    }

    #[test]
    fn absorption_does_not_touch_any_list() {
        let heir = vassalage_actor("heir", 50.0, 30.0, 60.0, 60.0, &[]);
        let mut parent = doomed_actor("parent", &["heir"]);
        parent.neighbors = vassalage_actor("parent", 0.0, 0.0, 0.0, 0.0, &["huns"]).neighbors;
        let huns = vassalage_actor("huns", 120.0, 5.0, 60.0, 72.0, &["parent"]);
        let mut scenario = empty_scenario();
        scenario.actors = vec![parent.clone(), heir.clone(), huns.clone()];
        let mut world = WorldState::new("test".into(), 375);
        world.actors.insert("parent".into(), parent);
        world.actors.insert("heir".into(), heir);
        world.actors.insert("huns".into(), huns);
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        assert!(neighbor_ids(&world, "heir").is_empty());
        assert_eq!(neighbor_ids(&world, "huns"), ["parent"]);
    }

    // ------------------------------------------------------------------
    // Mobilisation capacity and recovery (docs/investigation_military_source.md)
    // ------------------------------------------------------------------

    #[test]
    fn army_recovers_toward_capacity_but_never_above_it() {
        use crate::engine::interactions::{military_capacity, MILITARY_RECOVERY_RATE};
        let mut world = WorldState::new("test".into(), 375);
        // pop 8000 -> capacity 0.767 * 8000^(2/3) = 306.8 (rome's authored army is 350)
        let mut spent = vassalage_actor("spent", 2.0, 30.0, 60.0, 60.0, &[]);
        spent.set_metric("population", 8000.0);
        let capacity = military_capacity(&spent);
        assert!((capacity - 306.8).abs() < 0.5, "capacity {capacity}");
        world.actors.insert("spent".into(), spent);

        // An actor already above capacity keeps its army untouched.
        let mut over = vassalage_actor("over", 350.0, 30.0, 60.0, 60.0, &[]);
        over.set_metric("population", 8000.0);
        world.actors.insert("over".into(), over);

        // No population, no recruits.
        let mut empty = vassalage_actor("empty", 0.0, 30.0, 60.0, 60.0, &[]);
        empty.set_metric("population", 0.0);
        world.actors.insert("empty".into(), empty);

        interactions::apply_military_recovery(&mut world);

        let expected = 2.0 + (capacity - 2.0) * MILITARY_RECOVERY_RATE;
        assert!((world.actors["spent"].get_metric("military_size") - expected).abs() < 1e-9);
        assert_eq!(world.actors["over"].get_metric("military_size"), 350.0, "above capacity is left alone");
        assert_eq!(world.actors["empty"].get_metric("military_size"), 0.0, "no population, no recovery");
    }

    #[test]
    fn recovery_converges_to_capacity_and_stops() {
        use crate::engine::interactions::military_capacity;
        let mut world = WorldState::new("test".into(), 375);
        let mut a = vassalage_actor("a", 0.0, 30.0, 60.0, 60.0, &[]);
        a.set_metric("population", 250.0);
        let capacity = military_capacity(&a);
        world.actors.insert("a".into(), a);
        for _ in 0..400 {
            interactions::apply_military_recovery(&mut world);
        }
        let mil = world.actors["a"].get_metric("military_size");
        assert!(mil <= capacity, "never exceeds capacity: {mil} > {capacity}");
        assert!((mil - capacity).abs() < 1e-6, "converges: {mil} vs {capacity}");
    }

    // ------------------------------------------------------------------
    // Split as shrink (docs/investigation_split_as_shrink.md §11)
    // ------------------------------------------------------------------

    #[test]
    fn seat_keeping_heir_shrinks_in_place_and_the_other_separates() {
        use crate::core::{BorderType, EventCondition, EventConditionType, MilestoneEvent, Neighbor, Successor};
        let mut west = vassalage_actor("west", 0.0, 0.0, 0.0, 0.0, &[]);
        west.name = "Запад".into();
        west.name_short = "З".into();
        west.is_successor_template = true;
        let mut east = vassalage_actor("east", 0.0, 0.0, 0.0, 0.0, &["huns"]);
        east.name = "Восток".into();
        east.is_successor_template = true;

        let mut parent = vassalage_actor("parent", 100.0, 40.0, 62.0, 42.0, &["huns"]);
        parent.metrics.insert("population".into(), 8000.0);
        parent.metrics.insert("treasury".into(), 1000.0);
        parent.metrics.insert("economic_output".into(), 50.0);
        parent.metrics.insert("military_quality".into(), 60.0);
        parent.on_collapse = vec![
            Successor { id: "west".into(), weight: 0.45, keeps_seat: true },
            Successor { id: "east".into(), weight: 0.55, keeps_seat: false },
        ];
        let huns = vassalage_actor("huns", 120.0, 5.0, 60.0, 72.0, &["parent"]);

        let mut scenario = empty_scenario();
        scenario.actors = vec![parent.clone(), west, east, huns.clone()];
        scenario.milestone_events = vec![MilestoneEvent {
            id: "the_split".into(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: mr("cohesion", Some("parent")),
                    actor_id: Some("parent".into()),
                    operator: crate::core::ComparisonOperator::Less,
                    value: 100.0,
                },
                duration: None,
            },
            is_key: true,
            triggers_collapse: true,
            llm_context_shift: String::new(),
            cooldown_ticks: None,
            spawn_actor: None,
        }];
        let mut world = WorldState::new("test".into(), 375);
        world.actors.insert("parent".into(), parent);
        world.actors.insert("huns".into(), huns);
        let mut log = EventLog::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);

        tick(&mut world, &scenario, &mut log, &mut rng);

        // The seat keeps its id and its own state; only the shares are cut.
        let p = world.actors.get("parent").expect("the seat keeper stays in the world");
        assert_eq!(p.name, "Запад", "renamed after its template");
        assert_eq!(p.name_short, "З");
        assert_eq!(p.get_metric("population"), 8000.0 * 0.45);
        // Treasury is asserted as the invariant, not against the pre-tick value: the
        // income phase runs before the milestone inside the same tick, so the sum
        // being split is not the one the test set up.
        let seat_treasury = p.get_metric("treasury");
        assert!((p.get_metric("cohesion") - 42.0).abs() < 3.0, "cohesion is NOT overwritten: {}", p.get_metric("cohesion"));
        assert!(p.get_metric("legitimacy") > 50.0, "legitimacy is NOT overwritten: {}", p.get_metric("legitimacy"));

        // The other heir is born from the parent's living metrics, with the trauma values.
        let e = world.actors.get("east").expect("the other heir separates");
        assert_eq!(e.get_metric("population"), 8000.0 * 0.55);
        assert!(
            (seat_treasury / e.get_metric("treasury") - 0.45 / 0.55).abs() < 1e-9,
            "treasury is split by the declared shares: {} vs {}",
            seat_treasury,
            e.get_metric("treasury")
        );
        assert_eq!(e.get_metric("cohesion"), 20.0);
        assert_eq!(e.get_metric("legitimacy"), 30.0);
        assert!(!e.is_successor_template);
        // and its edge is written back, as for a spawn
        let huns_edges: Vec<&str> = world.actors["huns"].neighbors.iter().map(|n| n.id.as_str()).collect();
        assert!(huns_edges.contains(&"east"), "reverse edge written: {huns_edges:?}");
        assert_eq!(
            log.events.iter().filter(|ev| ev.event_type == EventType::Birth && ev.actor_id == "east").count(),
            1
        );
        assert!(!world.dead_actor_ids.contains("parent"), "the parent does not die");
        assert_eq!(world.game_mode, crate::core::GameMode::Consequences);
        let _ = BorderType::Land;
        let _ = Neighbor { id: "x".into(), distance: 1, border_type: BorderType::Land };
    }

    #[test]
    fn spawned_actor_links_its_neighbours_back() {
        use crate::core::{BorderType, EventCondition, EventConditionType, MilestoneEvent, Neighbor, SpawnActorConfig};
        let mut scenario = empty_scenario();
        scenario.actors = vec![
            vassalage_actor("savoy", 20.0, 30.0, 60.0, 60.0, &["milan"]),
            vassalage_actor("genoa", 20.0, 30.0, 60.0, 60.0, &[]),
            vassalage_actor("milan", 50.0, 30.0, 60.0, 60.0, &["savoy"]),
        ];
        scenario.milestone_events = vec![MilestoneEvent {
            id: "france_intervenes".into(),
            condition: EventCondition { condition_type: EventConditionType::Tick { tick: 0 }, duration: None },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: String::new(),
            cooldown_ticks: None,
            spawn_actor: Some(SpawnActorConfig {
                actor_id: "france".into(),
                label: "Франция".into(),
                initial_metrics: HashMap::new(),
                lat: 48.85,
                lng: 2.35,
                color: "#1e3a8a".into(),
                neighbors: vec![
                    Neighbor { id: "savoy".into(), distance: 1, border_type: BorderType::Land },
                    Neighbor { id: "genoa".into(), distance: 2, border_type: BorderType::Sea },
                    Neighbor { id: "milan".into(), distance: 3, border_type: BorderType::Land },
                ],
            }),
        }];
        // Milan already names France on its own terms — that entry must survive as is.
        let mut milan_lists_france = vassalage_actor("milan", 50.0, 30.0, 60.0, 60.0, &["savoy"]);
        milan_lists_france.neighbors.push(Neighbor { id: "france".into(), distance: 2, border_type: BorderType::Land });
        let mut world = WorldState::new("test".into(), 1477);
        for a in &scenario.actors { world.actors.insert(a.id.clone(), a.clone()); }
        world.actors.insert("milan".into(), milan_lists_france);
        let mut log = EventLog::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);

        tick(&mut world, &scenario, &mut log, &mut rng);

        assert!(world.actors.contains_key("france"), "spawn fired");
        let savoy = &world.actors["savoy"].neighbors;
        let back = savoy.iter().find(|n| n.id == "france").expect("savoy lists france");
        assert_eq!((back.distance, back.border_type.clone()), (1, BorderType::Land));
        let genoa = &world.actors["genoa"].neighbors;
        let back = genoa.iter().find(|n| n.id == "france").expect("genoa lists france");
        assert_eq!((back.distance, back.border_type.clone()), (2, BorderType::Sea));
        let milan = &world.actors["milan"].neighbors;
        assert_eq!(milan.iter().filter(|n| n.id == "france").count(), 1, "existing entry kept, no duplicate");
        assert_eq!(milan.iter().find(|n| n.id == "france").unwrap().distance, 2, "existing entry not overwritten");
    }

    #[test]
    fn living_heir_absorbs_instead_of_being_reborn() {
        let heir = vassalage_actor("heir", 50.0, 30.0, 60.0, 60.0, &[]);
        let mut scenario = empty_scenario();
        scenario.actors = vec![doomed_actor("parent", &["heir"]), heir.clone()];
        let mut world = WorldState::new("test".into(), 375);
        world.actors.insert("parent".into(), doomed_actor("parent", &["heir"]));
        world.actors.insert("heir".into(), heir);
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        let heir = world.actors.get("heir").unwrap();
        assert_eq!(heir.get_metric("expansion_count"), 1.0);
        assert_eq!(heir.get_metric("external_pressure"), 30.0, "absorption must not touch the heir");
        assert_eq!(log.events.iter().filter(|e| e.event_type == EventType::Birth).count(), 0);
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
