use std::collections::HashMap;

use crate::core::census;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use crate::core::{
    ComparisonOperator, DependencyMode, DependencyRule, Event, EventConditionType, EventCondition,
    EventType, MetricRef, Scenario, WorldState,
};

pub mod interactions;
pub mod trace;

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
fn apply_dependency_rule(actor: &mut crate::core::Actor, rule: &DependencyRule, tick: u32, threshold_scale: f64, treasury_floor: bool) {
    let from_val = actor.get_metric(rule.from.as_str());
    // Economy v2 (Ц1 stage 3): a rule reading `economic_output` measures a fall below the
    // actor's own norm, not below an absolute line — `threshold × T / 100`. 1.0 in v1.
    let threshold = rule.threshold.map(|t| t * threshold_scale);
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
        DependencyMode::Deficit => match threshold {
            Some(threshold) if from_val < threshold => -((threshold - from_val) * rule.coefficient),
            _ => 0.0,
        },
        DependencyMode::Excess => match threshold {
            Some(threshold) if from_val > threshold => -((from_val - threshold) * rule.coefficient),
            _ => 0.0,
        },
        DependencyMode::Bonus => match threshold {
            Some(threshold) if from_val > threshold => (census::dependency_source(&rule.id, from_val) - threshold) * rule.coefficient,
            _ => 0.0,
        },
        DependencyMode::Linear => from_val * rule.coefficient,
        // Priced on the *target's* stock, not on the source's units — see
        // `DependencyMode::DeficitProportional`. `threshold > 0` is a load-time
        // invariant (`validate_dependency_thresholds`), so the division is safe;
        // the `_ => 0.0` arm stays the no-op for an unvalidated scenario, exactly
        // as for the other three modes.
        DependencyMode::DeficitProportional => match threshold {
            Some(threshold) if threshold > 0.0 && from_val < threshold => {
                -(actor.get_metric(rule.to.as_str()) * rule.coefficient
                    * (threshold - from_val)
                    / threshold)
            }
            _ => 0.0,
        },
    };
    // A46 stage 2: the rule as a reader — active or not, and its source at the boundary.
    census::begin(|| format!("dependency {}", rule.id));
    census::condition(|| format!("{:?} {:?}", rule.mode, rule.threshold), delta != 0.0);
    census::begin(|| format!("dependency {} source", rule.id));
    census::condition(|| "source at boundary".to_string(), !(1.0..99.0).contains(&from_val));
    // Emitted even when `delta == 0.0`: the share of actor-ticks on which a rule fires at
    // all is a question probes ask, and a zero is an answer to it.
    trace::record_dependency(|| trace::DependencyRow {
        tick,
        actor: actor.id.clone(),
        rule: rule.id.clone(),
        from_val,
        to_before: actor.get_metric(rule.to.as_str()),
        delta,
    });
    if delta != 0.0 {
        census::write_source(|| format!("dependency {}", rule.id));
        let delta = if treasury_floor && rule.to.as_str() == "treasury" {
            treasury_delta_at_floor(&actor.id, actor.get_metric("treasury"), delta)
        } else {
            delta
        };
        actor.add_metric(rule.to.as_str(), delta);
        census::clear_write_source();
    }
}

/// Phase: Apply dependency rules to all actors
/// Rules are applied in strict file order - order is part of simulation logic.
fn phase_apply_dependencies(world: &mut WorldState, scenario: &Scenario) {
    let tick = world.tick;
    // Economy v2 (Ц1 stage 3): each actor's `economic_output` target, read before the rules
    // mutate anything; a rule whose source is `economic_output` scales its threshold by T / 100.
    let targets: std::collections::HashMap<String, f64> = if scenario.features.economy_v2 && census::eo_relative_thresholds() {
        world.actors.keys().filter_map(|id| eo_target(world, scenario, id).map(|t| (id.clone(), t))).collect()
    } else {
        std::collections::HashMap::new()
    };
    // Economy v2 (Ц5): the same for a rule whose source is `legitimacy`, against `T_L`.
    let legitimacy_targets: std::collections::HashMap<String, f64> =
        world.actors.keys().filter_map(|id| legitimacy_target(world, scenario, id).map(|t| (id.clone(), t))).collect();
    // Economy v2 (Ц8): the same for a rule whose source is `cohesion`, against `T_C`; and the
    // cohesion-to-cohesion decay rule gives way to the pull (census can keep it, variant (б)).
    let cohesion_targets: std::collections::HashMap<String, f64> =
        world.actors.keys().filter_map(|id| cohesion_target(world, scenario, id).map(|t| (id.clone(), t))).collect();
    let skip_cohesion_decay = scenario.features.economy_v2 && scenario.economy_v2_cohesion_pull.is_some() && !census::keep_cohesion_decay();
    // Economy v2 (Ц10): under the population pull the economy-to-population deficit rules give way —
    // their meaning is in the norm `P₀ × eo / T` (census keeps them for variant (б)).
    let skip_population_rules = population_pull_on(scenario) && !census::population_constant_norm();
    for actor in world.actors.values_mut() {
        let eo_scale = targets.get(&actor.id).map_or(1.0, |t| t / 100.0);
        let legitimacy_scale = legitimacy_targets.get(&actor.id).map_or(1.0, |t| t / 100.0);
        let cohesion_scale = cohesion_targets.get(&actor.id).map_or(1.0, |t| t / 100.0);
        for rule in &scenario.dependencies {
            if skip_cohesion_decay && rule.from.as_str() == "cohesion" && rule.to.as_str() == "cohesion" {
                continue;
            }
            if skip_population_rules && rule.from.as_str() == "economic_output" && rule.to.as_str() == "population" {
                continue;
            }
            let scale = match rule.from.as_str() {
                "economic_output" => eo_scale,
                "legitimacy" => legitimacy_scale,
                "cohesion" => cohesion_scale,
                _ => 1.0,
            };
            apply_dependency_rule(actor, rule, tick, scale, scenario.features.economy_v2 && census::debt_as_pay());
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
    // Economy v2 (Ц9): the scenario's starting alliances enter the world on its first tick.
    if world.tick == 0 && world.alliances.is_empty() && interactions::alliances_on(scenario) {
        interactions::seed_starting_alliances(world, scenario);
    }

    // Phase 1: Auto-deltas via MetricRef
    phase_auto_deltas(world, scenario, rng);

    // Phase 2: Region rank bonuses (fixed deltas, legitimacy floor)
    phase_region_ranks(world, scenario);

    // Phase 3: Dependency graph and interactions
    // Step 3b: mobilisation recovery — armies regrow toward the capacity their
    // population supports, before this tick's fighting. Placed here, immediately
    // ahead of the interaction phase, because that is where it was measured; moving
    // it changes the numbers in docs/investigation_military_source.md §4.
    phase_military_recovery(world, scenario);

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
    phase_events(world, scenario, event_log, rng);

    // Phase 7: Actor collapses
    phase_collapses(world, scenario, event_log);

    // Phase 7b: Vassalage formation / dissolution (parallel to collapses).
    // Runs after collapses so dead actors are already pruned and never vassalized.
    phase_vassalage(world, scenario, event_log);

    // Phase 8: Record changes and generation mechanics
    phase_record(world, scenario, event_log);

    // Phase 9: Advance tick state
    phase_advance(world, scenario);
}

// ============================================================================
// Phase 1: Auto-deltas via MetricRef
// ============================================================================

fn phase_auto_deltas(world: &mut WorldState, scenario: &Scenario, rng: &mut rand_chacha::ChaCha8Rng) {
    // Treasury via income/expenses formula (separate from auto_deltas)
    apply_treasury(world, scenario);

    // Apply auto_deltas via MetricRef - unified for actor/family/global
    for (index, auto_delta) in scenario.auto_deltas.iter().enumerate() {
        // Check conditions
        let mut delta = auto_delta.base;
        for cond in &auto_delta.conditions {
            census::begin(|| format!("auto_delta[{index}] {} | if {}", auto_delta.metric, cond.metric));
            if check_auto_delta_condition(world, cond) {
                delta += cond.delta;
            }
        }

        // Check ratio conditions
        for ratio_cond in &auto_delta.ratio_conditions {
            census::begin(|| format!("auto_delta[{index}] {} | ratio {} / {}", auto_delta.metric, ratio_cond.metric_a, ratio_cond.metric_b));
            let val_a = ratio_cond.metric_a.get(world);
            let val_b = ratio_cond.metric_b.get(world);

            let Some(actual_ratio) = ratio_value(val_a, val_b) else {
                census::condition(|| format!("{:?} {} — skipped, 0 / 0", ratio_cond.operator, ratio_cond.ratio), false);
                continue;
            };
            let condition_met = census::condition(
                || format!("{:?} {}", ratio_cond.operator, ratio_cond.ratio),
                ratio_cond.operator.evaluate(actual_ratio, ratio_cond.ratio),
            );
            
            if condition_met {
                delta += ratio_cond.delta;
            }
        }

        // Apply noise
        let noise = (rng.gen::<f64>() - 0.5) * 2.0 * auto_delta.noise;
        let final_delta = delta + noise;

        // The number the engine is about to use, emitted where it is already computed —
        // nothing is recalculated alongside it. See `engine::trace`.
        trace::record_auto_delta(|| trace::AutoDeltaRow {
            tick: world.tick,
            index,
            metric: auto_delta.metric.to_string(),
            base: auto_delta.base,
            authored: delta,
            applied: final_delta,
        });

        // Apply via MetricRef - scope to actor if actor_id is set
        census::write_source(|| format!("auto_delta[{index}] {}", auto_delta.metric));
        auto_delta.metric.apply(world, final_delta);
        census::clear_write_source();
    }
}

/// The value of a ratio condition `a / b`, or `None` when it is undefined (B44 stage 2).
///
/// An absent actor reads as `0.0` (`MetricRef::get`), and the arithmetic takes one rule
/// for a zero denominator: a non-zero numerator over zero is a limit, `±∞` — a dead
/// opponent is the utmost superiority, so `own_army / enemy_army > r` holds. The rule is
/// the same for a dead actor and for a living one at zero: it is the same mathematics.
/// Only `0 / 0` is undefined and skipped, as before. A dead numerator needs no rule of
/// its own: it reads `0.0`, the ratio is `0`.
pub(crate) fn ratio_value(a: f64, b: f64) -> Option<f64> {
    if b == 0.0 {
        if a == 0.0 {
            return None;
        }
        return Some(if a > 0.0 { f64::INFINITY } else { f64::NEG_INFINITY });
    }
    Some(a / b)
}

/// Check auto_delta condition against world state. The key already carries its
/// scope — it was resolved against the block's `actor_id` at load.
///
/// An absent actor reads as `0.0`: `>` gives false, `<` gives true. Declared, not
/// accidental (B44 stage 2): for rome's family a fallen Rome is a Rome with zero
/// legitimacy and cohesion. Milestones and rank conditions with an `actor_id` answer
/// `false` instead (`eval_metric_condition`); the two rules differ only on `<`, and the
/// one such case, the `rome_city` rank, has a log line as its only consequence.
fn check_auto_delta_condition(world: &WorldState, cond: &crate::core::DeltaCondition) -> bool {
    let value = cond.metric.get(world);
    let result = match cond.operator {
        crate::core::ComparisonOperator::Less => value < cond.value,
        crate::core::ComparisonOperator::LessOrEqual => value <= cond.value,
        crate::core::ComparisonOperator::Greater => value > cond.value,
        crate::core::ComparisonOperator::GreaterOrEqual => value >= cond.value,
        crate::core::ComparisonOperator::Equal => (value - cond.value).abs() < 0.001,
    };
    census::condition(|| format!("{:?} {}", cond.operator, cond.value), result)
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

fn phase_military_recovery(world: &mut WorldState, scenario: &Scenario) {
    // Economy v2 (Ц2 stage 2): no money, no levy — an actor in debt does not recruit.
    interactions::apply_military_recovery(world, scenario.features.economy_v2 && census::debt_as_pay());
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
    interactions::calculate_vassalage_interaction(world, event_log, rng, scenario.features.economy_v2 && census::debt_as_pay() && census::tribute_floor());
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
    // Membership is a property the tag declares (`sea_going` in `tags.toml`), not a name
    // the engine knows. See `TagDefinition::sea_going`.
    let sea_tags: std::collections::HashSet<&str> = scenario
        .tag_definitions
        .iter()
        .filter(|t| t.sea_going)
        .map(|t| t.id.as_str())
        .collect();
    let sea_actor_ids: std::collections::HashSet<String> = scenario.actors.iter()
        .filter(|a| a.tags.iter().any(|t| sea_tags.contains(t.as_str())))
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
                census::begin(|| format!("event {} @{} | if {}", event.id, target_id, cond.metric));
                let value = cond.metric
                    .resolve(target_id)
                    .expect("event target actor id")
                    .get(world);
                census::condition(|| format!("{:?} {}", cond.operator, cond.value), cond.operator.evaluate(value, cond.value))
            });

            if !conditions_met {
                continue;
            }

            // Apply effects (a census counterfactual can mute one event's effects, A20b + A41)
            census::write_source(|| format!("event {}", event.id));
            for (metric, delta) in event.effects.iter().filter(|_| !census::event_muted(&event.id)) {
                let key = metric.resolve(target_id).expect("event target actor id");
                let mut delta = *delta;
                // Economy v2 (Ц10): under the population pull the blow to the target's people is a
                // share of them, not a number.
                if let Some(share) = event.economy_v2_population_share.filter(|_| population_pull_on(scenario)) {
                    if matches!(&key, crate::core::MetricRef::Actor { actor_id, metric } if actor_id.as_str() == target_id.as_str() && metric.as_str() == "population") {
                        delta = -share * world.actors.get(target_id.as_str()).map_or(0.0, |a| a.get_metric("population").max(0.0));
                    }
                }
                if scenario.features.economy_v2 && census::debt_as_pay() {
                    if let Some(id) = is_treasury(&key) {
                        let current = world.actors.get(id).map_or(0.0, |a| a.get_metric("treasury"));
                        delta = treasury_delta_at_floor(id, current, delta);
                    }
                }
                key.apply(world, delta);
            }
            census::clear_write_source();
            // Economy v2 (Ц9): the event turns an alliance against one of its members.
            if let Some(id) = event.leaves_alliance_as_enemy.as_deref().filter(|_| interactions::alliances_on(scenario)) {
                interactions::leave_alliances_as_enemy(world, id);
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
    // The decay of cultural displacement progress lived here — removed with the mechanic (A30).
    apply_actor_tags(world, scenario);
    // Economy v2 (Ц1 stage 2): here, after every outflow of the tick (dependencies and combat
    // in the interaction phase, then random events) and after the tags have set this tick's
    // levels — so the pull answers this tick's deviation toward a target that is already
    // current — and before the clamp, which keeps the result on 0..100.
    pull_economic_output_to_target(world, scenario);
    // Economy v2 (Ц6): pressure pulled toward the real threat, same place and reason.
    pull_pressure_to_threat(world, scenario);
    // Economy v2 (Ц5): legitimacy pulled toward its norm, same place and reason.
    pull_legitimacy_to_target(world, scenario);
    // Economy v2 (Ц8): cohesion pulled toward its norm, same place and reason.
    pull_cohesion_to_target(world, scenario);
    // Economy v2 (Ц10): population pulled toward its norm, after the economy it reads has settled.
    pull_population_to_norm(world, scenario);
}

/// Economy v2 (Ц6): the threat an actor faces — `100 × N / (N + own army)`, N = the armies of
/// its living neighbours at distance 1 (the neighbour graph of the scenario). 0 with no armed
/// neighbour; 100 with an armed neighbour and no army of one's own.
pub fn pressure_threat(world: &WorldState, actor_id: &str) -> Option<f64> {
    let actor = world.actors.get(actor_id)?;
    // Ц6 stage 2: a sea neighbour weighs half, and an army bound to us by vassalage (either
    // way) or an alliance is no threat. (Alliances exist only under Ц9's switch.)
    let refined = census::threat_items() >= 3;
    let bound = |other: &str| {
        world.vassalages.iter().any(|v| (v.vassal_id == actor_id && v.overlord_id == other) || (v.overlord_id == actor_id && v.vassal_id == other))
            || world.alliances.iter().any(|a| a.actor_ids.iter().any(|x| x == actor_id) && a.actor_ids.iter().any(|x| x == other))
    };
    let n: f64 = actor
        .neighbors
        .iter()
        .filter(|nb| nb.distance == 1 && !world.dead_actor_ids.contains(&nb.id))
        .filter(|nb| !refined || !bound(&nb.id))
        .filter_map(|nb| world.actors.get(&nb.id).map(|a| (nb, a)))
        .map(|(nb, a)| {
            let w = if refined && nb.border_type == crate::core::BorderType::Sea { 0.5 } else { 1.0 };
            w * a.get_metric("military_size").max(0.0)
        })
        .sum();
    let own = actor.get_metric("military_size").max(0.0);
    Some(if n <= 0.0 { 0.0 } else { 100.0 * n / (n + own) })
}

/// Economy v2 (Ц6): `ep += r × (T_p − ep)` for every living actor, in id order. Targets are read
/// before any is moved, so the order does not matter.
fn pull_pressure_to_threat(world: &mut WorldState, scenario: &Scenario) {
    if !scenario.features.economy_v2 {
        return;
    }
    let Some(r) = scenario.economy_v2_pressure_pull else { return };
    let mut ids: Vec<String> = world.actors.keys().filter(|id| !world.dead_actor_ids.contains(*id)).cloned().collect();
    ids.sort();
    let targets: Vec<(String, f64)> = ids.iter().filter_map(|id| pressure_threat(world, id).map(|t| (id.clone(), t))).collect();
    for (id, target) in targets {
        let Some(actor) = world.actors.get_mut(&id) else { continue };
        let current = actor.get_metric("external_pressure");
        let delta = r * (target - current);
        actor.metrics.insert("external_pressure".to_string(), current + delta);
        #[cfg(feature = "census")]
        {
            census::write_source(|| "pressure pull".to_string());
            census::metric_write(std::panic::Location::caller(), &id, "external_pressure", delta, current, current + delta);
            census::clear_write_source();
        }
    }
}

/// The authored starting value of a metric for an actor: a starting actor's or a successor
/// template's own value, or a spawn's initial value (Ц1 stage 2, generalised in Ц5). Read from
/// the scenario, so heirs and spawns are covered by construction and nothing new is saved. A
/// seat-keeping heir keeps its parent's id and therefore its parent's base.
pub fn metric_base(scenario: &Scenario, actor_id: &str, metric: &str) -> Option<f64> {
    scenario
        .actors
        .iter()
        .find(|a| a.id == actor_id)
        .and_then(|a| a.metrics.get(metric).copied())
        .or_else(|| {
            scenario
                .milestone_events
                .iter()
                .filter_map(|m| m.spawn_actor.as_ref())
                .find(|c| c.actor_id == actor_id)
                .and_then(|c| c.initial_metrics.iter().find(|(k, _)| k.as_str() == metric).map(|(_, v)| *v))
        })
}

/// Economy v2: the norm a metric is pulled toward — the authored base plus the levels the
/// actor's tags give now. `None` when the actor has no authored base.
pub fn metric_target(world: &WorldState, scenario: &Scenario, actor_id: &str, metric: &str) -> Option<f64> {
    let base = metric_base(scenario, actor_id, metric)?;
    let levels: f64 = world.tag_levels.get(metric).and_then(|m| m.get(actor_id)).map(|m| m.values().sum()).unwrap_or(0.0);
    Some(base + levels)
}

/// Economy v2 (Ц1): the `economic_output` norm T.
pub fn eo_target(world: &WorldState, scenario: &Scenario, actor_id: &str) -> Option<f64> {
    metric_target(world, scenario, actor_id, "economic_output")
}

/// Economy v2 (Ц5): the legitimacy norm `T_L`, when the scenario pulls legitimacy.
pub fn legitimacy_target(world: &WorldState, scenario: &Scenario, actor_id: &str) -> Option<f64> {
    if !scenario.features.economy_v2 || scenario.economy_v2_legitimacy_pull.is_none() {
        return None;
    }
    metric_target(world, scenario, actor_id, "legitimacy")
}

/// Economy v2: the metrics whose tag modifiers are levels in this scenario, in name order —
/// `cohesion` (Ц8, with its pull), `economic_output` (Ц1), `external_pressure` (Ц6, when asked),
/// `legitimacy` (Ц5, with its pull).
pub fn level_metrics(scenario: &Scenario) -> Vec<&'static str> {
    let mut metrics = Vec::new();
    if scenario.features.economy_v2 {
        if scenario.economy_v2_cohesion_pull.is_some() {
            metrics.push("cohesion");
        }
        metrics.push("economic_output");
        if scenario.economy_v2_pressure_tags_as_level {
            metrics.push("external_pressure");
        }
        if scenario.economy_v2_legitimacy_pull.is_some() {
            metrics.push("legitimacy");
        }
    }
    metrics
}

/// Economy v2 (Ц8): the cohesion norm `T_C`, when the scenario pulls cohesion.
pub fn cohesion_target(world: &WorldState, scenario: &Scenario, actor_id: &str) -> Option<f64> {
    if !scenario.features.economy_v2 || scenario.economy_v2_cohesion_pull.is_none() {
        return None;
    }
    metric_target(world, scenario, actor_id, "cohesion")
}

/// Economy v2 (Ц10): the population pull is on.
pub fn population_pull_on(scenario: &Scenario) -> bool {
    scenario.features.economy_v2 && scenario.economy_v2_population_pull.is_some()
}

/// Economy v2 (Ц10): the authored population base P₀ of an actor, scaled by the share of it a
/// split left the seat (`world.population_base_scale`).
pub fn population_base(world: &WorldState, scenario: &Scenario, actor_id: &str) -> Option<f64> {
    let base = metric_base(scenario, actor_id, "population")?;
    Some(base * world.population_base_scale.get(actor_id).copied().unwrap_or(1.0))
}

/// Economy v2 (Ц10): the population norm `N = P₀ × eo / T` — the land feeds fewer people when the
/// economy is below its own norm. (Census variant (б): the constant P₀.)
pub fn population_norm(world: &WorldState, scenario: &Scenario, actor_id: &str) -> Option<f64> {
    let p0 = population_base(world, scenario, actor_id)?;
    if census::population_constant_norm() {
        return Some(p0);
    }
    let t = eo_target(world, scenario, actor_id).filter(|t| *t > 0.0)?;
    let eo = world.actors.get(actor_id)?.get_metric("economic_output").max(0.0);
    Some(p0 * eo / t)
}

/// Economy v2 (Ц10): `P += r × (N − P)` for every living actor, in id order. Norms are read before
/// any population moves (a norm reads only the actor's own economy, so the order does not matter).
fn pull_population_to_norm(world: &mut WorldState, scenario: &Scenario) {
    if !population_pull_on(scenario) {
        return;
    }
    let Some(r) = scenario.economy_v2_population_pull else { return };
    let mut ids: Vec<String> = world.actors.keys().filter(|id| !world.dead_actor_ids.contains(*id)).cloned().collect();
    ids.sort();
    let norms: Vec<(String, f64)> = ids.into_iter().filter_map(|id| population_norm(world, scenario, &id).map(|n| (id, n))).collect();
    for (id, norm) in norms {
        let Some(actor) = world.actors.get_mut(&id) else { continue };
        let current = actor.get_metric("population");
        let delta = r * (norm - current);
        actor.metrics.insert("population".to_string(), current + delta);
        #[cfg(feature = "census")]
        {
            census::write_source(|| "population pull".to_string());
            census::metric_write(std::panic::Location::caller(), &id, "population", delta, current, current + delta);
            census::clear_write_source();
        }
    }
}

/// Economy v2 (Ц8): `C += r × (T_C − C)` for every living actor, in id order — the two-sided
/// successor of the one-sided cohesion decay rule, which is not applied with it.
fn pull_cohesion_to_target(world: &mut WorldState, scenario: &Scenario) {
    if !scenario.features.economy_v2 {
        return;
    }
    let Some(r) = scenario.economy_v2_cohesion_pull else { return };
    let mut ids: Vec<String> = world.actors.keys().filter(|id| !world.dead_actor_ids.contains(*id)).cloned().collect();
    ids.sort();
    for id in ids {
        let Some(target) = metric_target(world, scenario, &id, "cohesion") else { continue };
        let Some(actor) = world.actors.get_mut(&id) else { continue };
        let current = actor.get_metric("cohesion");
        let delta = r * (target - current);
        actor.metrics.insert("cohesion".to_string(), current + delta);
        #[cfg(feature = "census")]
        {
            census::write_source(|| "cohesion pull".to_string());
            census::metric_write(std::panic::Location::caller(), &id, "cohesion", delta, current, current + delta);
            census::clear_write_source();
        }
    }
}

/// Economy v2 (Ц5): `L += r × (T_L − L)` for every living actor, in id order — the same pull as
/// `economic_output`'s. Every other writer stays; under the pull a rate b becomes a shift of the
/// norm by b / r (brief §9.7).
fn pull_legitimacy_to_target(world: &mut WorldState, scenario: &Scenario) {
    if !scenario.features.economy_v2 {
        return;
    }
    let Some(r) = scenario.economy_v2_legitimacy_pull else { return };
    let mut ids: Vec<String> = world.actors.keys().filter(|id| !world.dead_actor_ids.contains(*id)).cloned().collect();
    ids.sort();
    for id in ids {
        let Some(target) = metric_target(world, scenario, &id, "legitimacy") else { continue };
        let Some(actor) = world.actors.get_mut(&id) else { continue };
        let current = actor.get_metric("legitimacy");
        let delta = r * (target - current);
        actor.metrics.insert("legitimacy".to_string(), current + delta);
        #[cfg(feature = "census")]
        {
            census::write_source(|| "legitimacy pull".to_string());
            census::metric_write(std::panic::Location::caller(), &id, "legitimacy", delta, current, current + delta);
            census::clear_write_source();
        }
    }
}

/// Economy v2 (Ц1 stage 2): `eo += r × (T − eo)` for every living actor, in id order.
fn pull_economic_output_to_target(world: &mut WorldState, scenario: &Scenario) {
    if !scenario.features.economy_v2 {
        return;
    }
    let Some(r) = scenario.economy_v2_eo_pull else { return };
    let mut ids: Vec<String> = world.actors.keys().filter(|id| !world.dead_actor_ids.contains(*id)).cloned().collect();
    ids.sort();
    for id in ids {
        let Some(target) = eo_target(world, scenario, &id) else { continue };
        let Some(actor) = world.actors.get_mut(&id) else { continue };
        let current = actor.get_metric("economic_output");
        let delta = r * (target - current);
        actor.metrics.insert("economic_output".to_string(), current + delta);
        #[cfg(feature = "census")]
        {
            census::write_source(|| "eo pull".to_string());
            census::metric_write(std::panic::Location::caller(), &id, "economic_output", delta, current, current + delta);
            census::clear_write_source();
        }
    }
}

// ============================================================================
// Phase 4b: Era progression
// ============================================================================

fn phase_era_progression(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog) {
    for era_def in &scenario.era_definitions {
        // Skip ancient (starting era)
        if era_def.era == crate::core::Era::Ancient { continue; }

        // Sorted: the event log must not inherit `world.actors`' per-instance hash order.
        // A shuffled log changes which events tie at the cut-off of the chronicler's
        // "last five", so one narrative run in roughly ten showed a different fifth
        // event for the same seed. None of the appending phases draws RNG, so ordering
        // them changes the log and nothing else.
        // See docs/investigation_event_log_order.md.
        let mut era_ids: Vec<String> = world.actors.keys().cloned().collect();
        era_ids.sort();
        let (tick_now, year_now) = (world.tick, world.year);
        for era_actor_id in &era_ids {
            let Some(actor) = world.actors.get_mut(era_actor_id) else { continue };
            // Skip if already at or past this era
            if actor.era >= era_def.era { continue; }
            // Skip if tick too early
            if tick_now < era_def.min_tick { continue; }

            // Count matching tags
            let matching = actor.tags.iter()
                .filter(|t| era_def.from_tags.contains(t))
                .count() as u32;

            if matching >= era_def.requires_tags {
                let old_era = actor.era.clone();
                actor.era = era_def.era.clone();

                let event = Event::new(
                    format!("era_{}_{}", actor.id, format!("{:?}", era_def.era).to_lowercase()),
                    tick_now,
                    year_now,
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

fn phase_events(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog, rng: &mut rand_chacha::ChaCha8Rng) {
    check_threshold_effects(world, scenario, event_log);
    check_rank_conditions(world, scenario, event_log);
    check_milestone_events(world, scenario, event_log, rng);
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
        // Before everything else (A10): a victory over a fallen power does not count, and
        // the streak it would have built is reset.
        let all_alive = vc.requires_alive.iter()
            .all(|id| world.actors.contains_key(id) && !world.dead_actor_ids.contains(id));
        if !all_alive {
            world.victory_sustained_ticks = 0;
            return;
        }
        if world.tick >= vc.minimum_tick {
            census::begin(|| format!("victory | {}", vc.metric));
            let value = vc.metric.get(world);
            let main_condition = census::condition(|| format!(">= {}", vc.threshold), value >= vc.threshold);

            // Check additional conditions
            let additional_ok = vc.additional_conditions.iter().all(|cond| {
                census::begin(|| format!("victory | and {}", cond.metric));
                let metric_value = cond.metric.get(world);
                census::condition(|| format!("{:?} {}", cond.operator, cond.value), cond.operator.evaluate(metric_value, cond.value))
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
    strip_tags_of_absent_actors(world, scenario);
}

/// A tag that names actors in `requires_alive` leaves every carrier once one of them is
/// gone (A37 stage 2). Run after the collapses, so heirs born this tick lose it too; with
/// every named actor alive it changes nothing. A tag whose `ends_with` milestone has fired
/// leaves the same way (A4) — on the milestone's own tick, after its last effect.
fn strip_tags_of_absent_actors(world: &mut WorldState, scenario: &Scenario) {
    let gone: Vec<&str> = scenario
        .tag_definitions
        .iter()
        .filter(|t| {
            t.requires_alive.iter().any(|id| !world.actors.contains_key(id))
                || t.ends_with.as_ref().is_some_and(|m| world.milestone_events_fired.contains(m))
        })
        .map(|t| t.id.as_str())
        .collect();
    if gone.is_empty() {
        return;
    }
    for actor in world.actors.values_mut() {
        actor.tags.retain(|t| !gone.contains(&t.as_str()));
        actor.actor_tags.retain(|t, _| !gone.contains(&t.as_str()));
    }
}

// ============================================================================
// Phase 7b: Vassalage (formation / dissolution)
// ============================================================================

fn phase_vassalage(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog) {
    interactions::check_vassalage(world, event_log, !interactions::conquest_on(scenario));
}

// ============================================================================
// Phase 7: Record changes and generation mechanics
// ============================================================================

// No per-actor `metrics_*` ledger events. They were written here every tick and had no
// positive reader: the chronicler and five tools excluded them by id prefix, and the one
// reader that forgot — the "Recent Events" window — showed them in 99.4–100 % of its
// slots, because this phase runs last. Excluded by construction now, not by six filters.
// Per-tick metric debugging is `engine::trace`. See docs/TRIAGE.md, «B31: стадия 1».
fn phase_record(world: &mut WorldState, scenario: &Scenario, event_log: &mut EventLog) {
    check_generation_transfer(world, scenario, event_log);
    update_metric_history(world);
    update_prev_metrics(world);
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

/// Economy v2 (Ц2 stage 2): a debt is unpaid soldiers' pay. Only the army's upkeep (the
/// treasury formula) takes a treasury below zero; every other loss stops at zero — a storm
/// cannot take money that is not there. Returns the part of `delta` that applies to a
/// treasury at `current`; the rest is recorded as lost at the floor.
pub(crate) fn treasury_delta_at_floor(actor_id: &str, current: f64, delta: f64) -> f64 {
    if delta >= 0.0 {
        return delta;
    }
    let applied = if current <= 0.0 { 0.0 } else { delta.max(-current) };
    if applied != delta {
        census::treasury_floor_loss(actor_id, applied - delta);
    }
    applied
}

/// Whether a metric key is an actor's treasury.
fn is_treasury(key: &crate::core::MetricRef) -> Option<&str> {
    match key {
        crate::core::MetricRef::Actor { actor_id, metric } if metric.as_str() == "treasury" => Some(actor_id.as_str()),
        _ => None,
    }
}

fn apply_treasury(world: &mut WorldState, scenario: &Scenario) {
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();
    // Economy v2 (Ц1): the refitted coefficient; v1 keeps the constant.
    let coefficient = if scenario.features.economy_v2 {
        scenario.economy_v2_income_coefficient.unwrap_or(0.001)
    } else {
        0.001
    };

    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            let incomes = actor.get_metric("economic_output") * actor.get_metric("population") * census::income_coefficient(coefficient);
            let expenses = actor.get_metric("military_size") * 0.8;
            #[cfg(feature = "census")]
            census::treasury_parts(&actor.id, incomes, expenses);
            census::write_source(|| "treasury formula".to_string());
            actor.add_metric("treasury", incomes - expenses);
            census::clear_write_source();
            // Economy v2 (Ц2): a debt held `debt_ticks` ticks in a row costs the army `debt_cut`
            // of itself every tick, until the treasury is back at zero or above.
            if let (true, Some(n), Some(cut)) = (scenario.features.economy_v2, scenario.economy_v2_debt_ticks, scenario.economy_v2_debt_cut) {
                if actor.get_metric("treasury") < 0.0 {
                    let held = world.debt_ticks.entry(actor_id.clone()).or_insert(0);
                    *held += 1;
                    if *held >= n {
                        let army = actor.get_metric("military_size");
                        census::write_source(|| "debt".to_string());
                        actor.add_metric("military_size", -army * cut);
                        census::clear_write_source();
                    }
                } else {
                    world.debt_ticks.remove(&actor_id);
                }
            }
        }
    }
    if scenario.features.economy_v2 {
        let actors = &world.actors;
        world.debt_ticks.retain(|id, _| actors.contains_key(id));
    }
}

// ============================================================================
// Step 3: Neighbor Interactions
// ============================================================================
// Step 4: Actor Tags Effects
// ============================================================================

fn apply_actor_tags(world: &mut WorldState, scenario: &Scenario) {
    // Economy v2 (brief §9.6): a tag's modifier of these metrics is a level — given once when
    // the tag appears, taken back when it leaves — instead of a rate every tick.
    let as_level = level_metrics(scenario);
    let actor_ids: Vec<String> = world.actors.keys().cloned().collect();

    for actor_id in actor_ids {
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            // Sorted by tag, then by metric (B21). Integer modifiers are added to a
            // fractional `f64` one at a time, and with mixed signs on one metric — rome's
            // `raid_economy −1` against `roman_contact +1` — the order changes the last
            // bit. Iterating the two HashMaps made that order depend on the process's hash
            // seed: rome drifted between processes in 20 of 30 seeds, 0 after sorting.
            // See docs/TRIAGE.md, «B21: стадия 1».
            let mut modifiers: Vec<(&str, &str, i32)> = actor
                .actor_tags
                .iter()
                .flat_map(|(tag, t)| t.metrics_modifier.iter().map(move |(m, v)| (tag.as_str(), m.as_str(), *v)))
                .collect();
            modifiers.sort_unstable_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
            for (_tag, metric, modifier) in modifiers {
                let current = actor.metrics.get(metric).copied().unwrap_or(0.0);
                let add = census::tag_modifier_for(&actor.id, _tag, metric, modifier as f64);
                if as_level.contains(&metric) {
                    let levels = world.tag_levels.entry(metric.to_string()).or_default().entry(actor_id.clone()).or_default();
                    if levels.contains_key(_tag) {
                        continue; // already given
                    }
                    levels.insert(_tag.to_string(), add);
                }
                actor.metrics.insert(metric.to_string(), current + add);
                #[cfg(feature = "census")]
                {
                    census::write_source(|| format!("tag {_tag}"));
                    census::metric_write(std::panic::Location::caller(), &actor.id, metric, add, current, current + add);
                    census::clear_write_source();
                }
            }
            // Economy v2: take back the level of tags the actor no longer carries, metric by
            // metric in name order.
            for metric in &as_level {
                let Some(levels) = world.tag_levels.get_mut(*metric).and_then(|m| m.get_mut(&actor_id)) else { continue };
                let gone: Vec<String> = levels.keys().filter(|t| !actor.actor_tags.contains_key(*t)).cloned().collect();
                for tag in gone {
                    let level = levels.remove(&tag).unwrap_or(0.0);
                    let current = actor.metrics.get(*metric).copied().unwrap_or(0.0);
                    actor.metrics.insert(metric.to_string(), current - level);
                    #[cfg(feature = "census")]
                    {
                        census::write_source(|| "tag level removal".to_string());
                        census::metric_write(std::panic::Location::caller(), &actor.id, metric, -level, current, current - level);
                        census::clear_write_source();
                    }
                }
            }
            // A46 stage 4 (д): under the tag-level counterfactual, take back the level of tags
            // the actor no longer carries.
            #[cfg(feature = "census")]
            {
                let present: Vec<String> = actor.actor_tags.keys().cloned().collect();
                for (metric, delta) in census::tag_level_removals(&actor.id, &present) {
                    let current = actor.metrics.get(&metric).copied().unwrap_or(0.0);
                    actor.metrics.insert(metric.clone(), current + delta);
                    census::write_source(|| "tag level removal".to_string());
                    census::metric_write(std::panic::Location::caller(), &actor.id, &metric, delta, current, current + delta);
                    census::clear_write_source();
                }
            }
            // Note: No clamping here - clamp_metrics is called on step 5
        }
    }
    let actors = &world.actors;
    for metric in &as_level {
        if let Some(per_actor) = world.tag_levels.get_mut(*metric) {
            per_actor.retain(|id, _| actors.contains_key(id));
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

    // Sorted: the event log must not inherit `world.actors`' per-instance hash order —
    // a shuffled log changes which events tie at the cut-off of the chronicler's "last
    // five". None of the appending phases draws RNG, so this changes the log and nothing
    // else. See docs/investigation_event_log_order.md.
    let mut threshold_ids: Vec<String> = world.actors.keys().cloned().collect();
    threshold_ids.sort();
    for threshold_id in &threshold_ids {
        let Some(actor) = world.actors.get(threshold_id) else { continue };
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
                census::begin(|| format!("rank_condition {}", rank_cond.region_id));
                eval_metric_condition(world, metric, actor_id, operator, *value)
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
            // the actor is not in the world
            return census::condition(|| format!("{operator:?} {value} (actor_id: absent → false)"), false);
        };
        return census::condition(|| format!("{operator:?} {value}"), compare(current, operator, &value));
    }
    census::condition(|| format!("{operator:?} {value}"), compare(metric.get(world), operator, &value))
}

fn check_milestone_events(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
    rng: &mut rand_chacha::ChaCha8Rng,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    for milestone in &scenario.milestone_events {
        // Skip if already fired (one-time)
        if world.milestone_events_fired.contains(&milestone.id) {
            continue;
        }

        // A milestone that follows another does not even start counting until that one
        // has fired (A2).
        if let Some(prev) = &milestone.after {
            if !world.milestone_events_fired.contains(prev) {
                continue;
            }
        }

        // A closed group does not fire (B46): another of its milestones fired first, or the
        // victory closed it. Derived from what has fired, so nothing new is saved.
        if let Some(group) = &milestone.group {
            let taken = scenario.milestone_events.iter().any(|o| {
                o.group.as_ref() == Some(group) && world.milestone_events_fired.contains(&o.id)
            });
            let won = world.victory_achieved
                && scenario.victory_condition.as_ref().and_then(|v| v.closes_group.as_ref()) == Some(group);
            if taken || won {
                continue;
            }
        }

        // Required actors alive (B46), the victory's rule: checked first, and a sustained
        // count does not survive their absence.
        if !milestone.requires_alive.iter().all(|id| world.is_actor_alive(id)) {
            world.milestone_condition_ticks.remove(&milestone.id);
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

        census::begin(|| format!("milestone {}", milestone.id));
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
            // One-time effects, authored on the milestone (B54)
            apply_milestone_effects(world, milestone, scenario.features.economy_v2, scenario.features.economy_v2 && census::debt_as_pay());
            // Economy v2 (Ц7): an authored war of conquest — the pair's vassalage is broken, it may
            // not bind again, and the target's streak starts anew against the attacker.
            if let Some(bc) = milestone.begins_conquest.as_ref().filter(|_| interactions::conquest_on(scenario)) {
                world.vassalages.retain(|v| !((v.vassal_id == bc.target && v.overlord_id == bc.attacker) || (v.vassal_id == bc.attacker && v.overlord_id == bc.target)));
                world.conquests.insert((bc.attacker.clone(), bc.target.clone()));
                world.war_streaks.remove(&bc.target);
                // the war opens with the assault: one battle on this tick, without the roll for an attack
                interactions::assault(world, scenario, &bc.attacker, &bc.target, event_log, rng);
            }

            // Spawn actor if configured
            if let Some(cfg) = &milestone.spawn_actor {
                // Idempotency: don't spawn if already exists
                if !world.actors.contains_key(&cfg.actor_id)
                    && !world.dead_actors.iter().any(|d| d.id == cfg.actor_id)
                {
                    use crate::core::{Actor, GeoCoordinate, NarrativeStatus};
                    
                    let actor = Actor {
                        id: cfg.actor_id.clone(),
                        name: cfg.label.clone(),
                        name_short: cfg.label.clone(),
                        region: cfg.actor_id.clone(),
                        // Identity from content (B28), not invented here.
                        region_rank: cfg.region_rank.clone(),
                        era: scenario.era.clone(),
                        narrative_status: NarrativeStatus::Background,
                        tags: vec![],
                        metrics: cfg.initial_metrics.iter()
                            .map(|(k, v)| (k.as_str().to_string(), *v))
                            .collect(),
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
                        religion: cfg.religion.clone(),
                        culture: cfg.culture.clone(),
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

                    // Появление державы — это рождение, и типом события оно должно
                    // совпадать с рождением наследника ниже (`EventType::Birth`,
                    // строка ~1938): текст того же рода, блок промпта тот же.
                    // Пока спавн был `Milestone`, он не попадал в блок «события
                    // периода» вовсе — `key_milestones_fired` строится из
                    // `scenario.milestone_events`, а синтетического `spawn_*` там нет,
                    // так что до летописца он доходил только случайным попаданием в
                    // окно релевантности. Теги — как у рождения наследника, чтобы
                    // канонический отбор оценивал его так же.
                    let event = Event::new(
                        format!("spawn_{}", cfg.actor_id),
                        current_tick,
                        current_year,
                        cfg.actor_id.clone(),
                        EventType::Birth,
                        true,
                        format!("{} появился на сцене истории.", cfg.label),
                    )
                    .with_tags(vec!["birth".to_string(), cfg.actor_id.clone()]);
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
/// A milestone's authored `effects`, applied once on its tick (B54). Sorted by key: the
/// `HashMap` order is per process, and a fixed order keeps any future overlapping keys
/// deterministic. An absent actor is skipped (`MetricRef::apply`). Economy v2 (Ц3): with v2 on,
/// the milestone's `economy_v2_effects` follow, the same way.
fn apply_milestone_effects(world: &mut WorldState, milestone: &crate::core::MilestoneEvent, v2: bool, treasury_floor: bool) {
    let mut effects: Vec<(&crate::core::MetricRef, &f64)> = milestone.effects.iter().collect();
    effects.sort_by_key(|(k, _)| k.to_string());
    if v2 {
        let mut more: Vec<(&crate::core::MetricRef, &f64)> = milestone.economy_v2_effects.iter().collect();
        more.sort_by_key(|(k, _)| k.to_string());
        effects.extend(more);
    }
    for (key, delta) in effects {
        census::write_source(|| format!("milestone {}", milestone.id));
        let mut delta = *delta;
        if treasury_floor {
            if let Some(id) = is_treasury(key) {
                let current = world.actors.get(id).map_or(0.0, |a| a.get_metric("treasury"));
                delta = treasury_delta_at_floor(id, current, delta);
            }
        }
        key.apply(world, delta);
        census::clear_write_source();
    }
}

/// Check and handle game mode transitions
/// Scenario → Consequences: automatic when milestone with triggers_collapse fires
/// "Split as shrink": a milestone with `triggers_collapse` divides the actor named in its
/// `splits_actor` instead of killing it.
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
    // Whom to split is the milestone's own field (A12), never its condition: `final_assault`
    // and `italy_unified` name the Ottomans and Milan in their conditions and are no
    // splits at all, and a split by date names nobody there.
    let Some(actor_id) = milestone.splits_actor.clone() else { return };
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

    // The seat: same id, same neighbours, new name and new heir list, reduced share.
    //
    // The heir list has to be adopted along with the name, and for the same reason.
    // The seat keeps the *parent's* id — `rome` goes on being `rome` while calling
    // itself "Западная Римская Империя" — and `rome_west` is never inserted into
    // `world.actors` at all. Leaving the parent's `on_collapse` in place therefore
    // leaves the seat declaring an heir that it has already become. When the seat
    // later dies, `check_collapses` finds `rome_west` neither among the living nor
    // in `dead_actor_ids` (the id that died is `rome`) and builds it from the
    // template verbatim: a full-strength copy of the power that has just fallen,
    // under the same name, with `external_pressure` back from 100 to the template's
    // 50. Measured at floor 12, before this line: **8 of 30 runs** ended with one
    // name standing in `alive_actors` and `dead_actors` at once —
    //
    //   сид 5, тик 298: мёртв id=rome name="Западная Римская Империя" ep=100.0
    //                 | жив  id=rome_west name="Западная Римская Империя" ep=50.0
    //
    // — which leaves the chronicler unable to say anything true about that name.
    // This is the resurrection class closed for `milan`/`savoy` in PR #47; the guard
    // there tests `dead_actor_ids.contains(&successor.id)` and cannot see this case,
    // because the id that dies is the parent's.
    //
    // The template's own `on_collapse` is the right source: `rome_west` declares
    // `vec![]`, i.e. the Western Empire has no further declared heirs — which is
    // exactly the statement the content makes. NOT adopted here: the template's
    // `region_rank`. The seat keeps rank `S` and the legitimacy floor written for
    // the undivided empire, and that is a separate question with its own numbers
    // (see docs/investigation_rome_immortality.md §10).
    let seat_template = scenario
        .actors
        .iter()
        .find(|a| a.id == seat.id)
        .map(|t| (t.name.clone(), t.name_short.clone(), t.on_collapse.clone()));
    // Economy v2 (Ц10): the seat keeps only its share of the people — and of its population base,
    // or the pull would regrow the undivided empire beside its living heirs.
    if population_pull_on(scenario) {
        let scale = world.population_base_scale.entry(actor_id.clone()).or_insert(1.0);
        *scale *= seat.weight / total;
    }
    if let Some(p) = world.actors.get_mut(&actor_id) {
        p.metrics = cut(&parent_metrics, seat.weight / total, false);
        // A46: the split rewrites the seat's metrics wholesale — record it for the census.
        #[cfg(feature = "census")]
        {
            census::write_source(|| "seat_split".to_string());
            for (k, v) in &p.metrics {
                let old = parent_metrics.get(k).copied().unwrap_or(0.0);
                census::metric_write(std::panic::Location::caller(), &p.id, k, v - old, old, *v);
            }
            census::clear_write_source();
        }
        if let Some((name, short, on_collapse)) = seat_template {
            p.name = name;
            p.name_short = short;
            p.on_collapse = on_collapse;
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
    // The split happens to the world on the tick its milestone fires, in any mode (A12).
    // It used to run inside the mode transition below, which returns unless the mode is
    // still `Scenario` — so a player who had already won (`Ended`) saw «Империя
    // разделилась» with no split behind it: in played rome, 7 of 30 `balanced` games won
    // before tick 40. The actor `splits_actor` names shrinks to its share and the other
    // heir separates. See docs/investigation_split_as_shrink.md §11.
    for milestone in &scenario.milestone_events {
        let fired_now = event_log.events.iter().rev()
            .take_while(|e| e.tick == world.tick)
            .any(|e| e.id == milestone.id);
        // By `splits_actor`, not by `triggers_collapse`: a split is not the end of a
        // scenario (rome's 395 leaves 130 years of it), and the two no longer ride one flag.
        if milestone.splits_actor.is_some() && fired_now {
            apply_seat_split(world, scenario, milestone, event_log);
        }
    }

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

    // Calculate average power projection for all active actors. Summed in id order
    // (B21): an `f64` sum in `HashMap` order differs in the last bit between processes,
    // and this average is compared against a promotion threshold below.
    let mut projection_ids: Vec<&String> = world.actors.keys().collect();
    projection_ids.sort();
    let avg_power_projection: f64 = projection_ids.iter()
        .map(|id| world.actors[*id].power_projection(1.0, max_military_size))
        .sum::<f64>() / world.actors.len().max(1) as f64;

    // Get list of narrative actor IDs for contact check (collect as owned Strings to avoid borrow issues)
    let mut narrative_actor_ids: Vec<String> = world.actors.iter()
        .filter(|(_, a)| a.narrative_status == crate::core::NarrativeStatus::Foreground)
        .map(|(id, _)| id.clone())
        .collect();
    narrative_actor_ids.sort();

    // Sorted: the event log must not inherit `world.actors`' per-instance hash order —
    // a shuffled log changes which events tie at the cut-off of the chronicler's "last
    // five". None of the appending phases draws RNG, so this changes the log and nothing
    // else. See docs/investigation_event_log_order.md.
    let mut relevance_ids: Vec<String> = world.actors.keys().cloned().collect();
    relevance_ids.sort();

    // Check each background actor for potential promotion to foreground
    let mut to_promote: Vec<String> = Vec::new();

    for actor_id in &relevance_ids {
        let Some(actor) = world.actors.get(actor_id) else { continue };
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

    for actor_id in &relevance_ids {
        let Some(actor) = world.actors.get(actor_id) else { continue };
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
        census::begin(|| "early_transfer".to_string());
        let metric_value = metric.get(world);
        census::condition(|| format!("{operator:?} {value}"), operator.evaluate(metric_value, value))
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
                #[cfg(feature = "census")]
                {
                    census::write_source(|| "generation_transfer".to_string());
                    census::metric_write(std::panic::Location::caller(), "family", metric, new_value - value, *value, new_value);
                    census::clear_write_source();
                }
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
        // Economy v2 (Ц7): the conquest path reads the war — a target beaten `K₂` times in a row by
        // the attacker of a declared war of conquest — not legitimacy, which stays with paths 1–2.
        let conquest_collapse = if interactions::conquest_on(scenario) {
            world.conquered_by.contains_key(actor_id)
        } else {
            actor.get_metric("military_size") < crate::engine::interactions::MIN_DEFENSIBLE_MILITARY
                && actor.get_metric("legitimacy") < 10.0
                && actor.get_metric("external_pressure") > 85.0
                && besieged
        };

        // Economy v2 (Ц7): a conquest kills on the tick it is completed — the streak of lost
        // battles was the duration; the three-tick hold is for the state-based paths 1–2.
        if conquest_collapse && interactions::conquest_on(scenario) {
            to_collapse.push((actor_id.clone(), actor.on_collapse.clone()));
            world.collapse_warning_ticks.remove(actor_id);
            continue;
        }

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

        // Economy v2 (Ц2 stage 3): a state without people does not go on living — population ≤ 1
        // for `economy_v2_depopulation_ticks` ticks in a row collapses by the usual path. None of
        // the three paths above sees it: they read legitimacy, cohesion and pressure.
        if let (true, Some(n)) = (scenario.features.economy_v2, scenario.economy_v2_depopulation_ticks) {
            if actor.get_metric("population") <= 1.0 {
                let held = world.depop_ticks.entry(actor_id.clone()).or_insert(0);
                *held += 1;
                if *held >= n && !to_collapse.iter().any(|(id, _)| id == actor_id) {
                    to_collapse.push((actor_id.clone(), actor.on_collapse.clone()));
                }
            } else {
                world.depop_ticks.remove(actor_id);
            }
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
                //
                // An heir that is *already a living power* is an absorption, not a
                // split, and the engine used to record it nowhere on either party's
                // line: the text named no one, and the absorber got only a silent
                // `expansion_count` increment. It is frequent — 10 of 20 deaths in
                // constantinople and 5 of 10 in milan over 5 seeds × 300 ticks.
                // Naming it here costs no new event and therefore does not shift the
                // relevance windows, which is why задача A24 refused adding one.
                // "державе"/"державам" agrees with itself, like the prefix above.
                {
                    let absorbers: Vec<String> = successors
                        .iter()
                        .filter(|s| s.id != actor_id)
                        .filter_map(|s| world.actors.get(&s.id).map(|a| a.name.clone()))
                        .collect();
                    match absorbers.len() {
                        0 => format!("Держава {} прекратила существование", actor.name),
                        1 => format!(
                            "Держава {} прекратила существование, её земли отошли державе {}",
                            actor.name, absorbers[0]
                        ),
                        _ => format!(
                            "Держава {} прекратила существование, её земли отошли державам {}",
                            actor.name,
                            absorbers.join(" и ")
                        ),
                    }
                },
            )
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

                // The border passes to a sole absorber, by the same rule as a fresh sole
                // heir above (B24): every living neighbour of the dead power names the
                // absorber in its place (or drops the dead id if it already names the
                // absorber), and the absorber gains those edges from their side. Before,
                // absorption touched no edges — neighbours kept a dangling reference and
                // the absorber never met them. Measured in emulation (100 seeds × 300):
                // dangling references −60…−67 %, deaths and victories unchanged, wars in
                // milan +20 %. Actors walked in id order. See docs/TRIAGE.md, «B24».
                if successors.len() == 1 {
                    let heir_id = successor.id.clone();
                    let mut ids: Vec<String> = world.actors.keys().cloned().collect();
                    ids.sort();
                    let mut inherited: Vec<crate::core::Neighbor> = Vec::new();
                    for id in &ids {
                        if *id == heir_id || *id == actor_id {
                            continue;
                        }
                        let Some(other) = world.actors.get_mut(id) else { continue };
                        let Some(edge) = other.neighbors.iter().find(|n| n.id == actor_id).cloned() else { continue };
                        if other.neighbors.iter().any(|n| n.id == heir_id) {
                            other.neighbors.retain(|n| n.id != actor_id);
                        } else {
                            for n in other.neighbors.iter_mut() {
                                if n.id == actor_id {
                                    n.id = heir_id.clone();
                                }
                            }
                            inherited.push(crate::core::Neighbor {
                                id: id.clone(),
                                distance: edge.distance,
                                border_type: edge.border_type,
                            });
                        }
                    }
                    if let Some(heir) = world.actors.get_mut(&heir_id) {
                        heir.neighbors.retain(|n| n.id != actor_id);
                        for n in inherited {
                            if !heir.neighbors.iter().any(|m| m.id == n.id) {
                                heir.neighbors.push(n);
                            }
                        }
                    }
                }
            }
        }
    }

    // Записи о погибших не снимал никто. Счётчик выше сбрасывается только когда актор
    // ВЫШЕЛ из опасности; погибший из `world.actors` просто исчезает, и цикл по живым
    // акторам его ключа больше не касается. Карта росла до конца партии.
    //
    // Это не только утечка в сериализуемом `WorldState`: её ключи читает `sim`
    // (`sim.rs:636`) и строит из них сигнал «кризис» для отбора случаев обзорной пачки.
    // Замер до этой строки, 3 сценария × 5 сидов × 300 тиков: тиков с непустым списком
    // 133…267, из них содержащих погибших 127…265, а состоящих ТОЛЬКО из погибших —
    // 125…255. После правки непустых тиков 4…14, осиротевших записей 0.
    // См. docs/TRIAGE.md, B10.
    let living: std::collections::HashSet<String> = world.actors.keys().cloned().collect();
    world.collapse_warning_ticks.retain(|id, _| living.contains(id));
    world.depop_ticks.retain(|id, _| living.contains(id));
}

fn metrics_to_snapshot(metrics: &HashMap<String, f64>) -> HashMap<String, f64> {
    crate::core::actor::metrics_to_snapshot(metrics)
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
            era: crate::core::Era::Ancient,
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
            economy_v2_income_coefficient: None,
            economy_v2_eo_pull: None,
            economy_v2_debt_ticks: None,
            economy_v2_debt_cut: None,
            economy_v2_depopulation_ticks: None,
            economy_v2_pressure_tags_as_level: false,
            economy_v2_pressure_pull: None,
            economy_v2_legitimacy_pull: None,
            economy_v2_combat_outcome: false,
            economy_v2_conquest_k2: None,
            economy_v2_alliances: false,
            starting_alliances: vec![],
            economy_v2_cohesion_pull: None,
            economy_v2_population_pull: None,
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

    /// Two identical worlds, same seed, **same process**: the event log must come out
    /// in the same order. `std::collections::HashMap` derives each instance's iteration
    /// order from a per-instance key, so two worlds built in one process iterate their
    /// actors differently — which is exactly the shuffle a second process would see.
    /// That makes this a deterministic probe for a defect that otherwise shows up in
    /// roughly one narrative run out of ten.
    #[test]
    fn event_log_order_is_independent_of_actor_hash_order() {
        fn run(seed: u64, ticks: u32) -> Vec<String> {
            let scenario = crate::scenarios::registry::load_by_id("rome_375").expect("scenario");
            let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
            for actor in &scenario.actors {
                if !actor.is_successor_template {
                    world.actors.insert(actor.id.clone(), actor.clone());
                }
            }
            let mut log = EventLog::new();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
            for _ in 0..ticks {
                tick(&mut world, &scenario, &mut log, &mut rng);
            }
            log.events.iter().map(|e| format!("{}@{}", e.id, e.tick)).collect()
        }
        for (seed, ticks) in [(1u64, 300u32), (2, 300), (42, 300)] {
            let a = run(seed, ticks);
            let b = run(seed, ticks);
            assert_eq!(a.len(), b.len(), "seed {seed}: same seed must produce the same number of events");
            let first_diff = a.iter().zip(b.iter()).position(|(x, y)| x != y);
            assert!(
                first_diff.is_none(),
                "seed {seed}: event log order differs between two worlds in one process at index {:?}: {:?} vs {:?}",
                first_diff,
                first_diff.map(|i| &a[i]),
                first_diff.map(|i| &b[i])
            );
        }
    }

    /// B21: tag modifiers are integers added one at a time to a fractional `f64`, so with
    /// mixed signs on one metric the order sets the last bit. Every world below builds
    /// its `actor_tags` into a fresh `HashMap` — a fresh per-instance hash key, i.e. the
    /// iteration order another process would see — and all must land on the same bits.
    /// The value is the one rome actually drifted on (`burgundians.economic_output`).
    #[test]
    fn tag_modifiers_apply_in_an_order_independent_of_hash_order() {
        use crate::core::{ActorTag, MetricName};
        let tag = |delta: i32| ActorTag {
            metrics_modifier: HashMap::from([(MetricName::new("economic_output").unwrap(), delta)]),
            spreads_via: vec![],
        };
        let scenario = crate::scenarios::registry::load_by_id("rome_375").expect("scenario");
        let template = scenario.actors.iter().find(|a| !a.is_successor_template).unwrap().clone();

        let mut results = std::collections::BTreeSet::new();
        for i in 0..32 {
            let mut actor = template.clone();
            actor.metrics.insert("economic_output".to_string(), 63.589101917601305);
            actor.actor_tags = HashMap::new();
            let pair = [("raid_economy", -1), ("roman_contact", 1)];
            for (name, delta) in if i % 2 == 0 { pair } else { [pair[1], pair[0]] } {
                actor.actor_tags.insert(name.to_string(), tag(delta));
            }
            let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, 1);
            world.actors.insert(actor.id.clone(), actor.clone());
            apply_actor_tags(&mut world, &scenario);
            results.insert(world.actors[&actor.id].metrics["economic_output"].to_bits());
        }
        assert_eq!(results.len(), 1, "tag application order leaked into the result: {results:x?}");
    }

    /// A10: constantinople's victory requires a living Byzantium. Federation at 90, the
    /// Ottoman army at 0, three sustained ticks from `minimum_tick`: the victory comes with
    /// Byzantium in the world and does not come with her removed — both ways.
    #[test]
    fn victory_requires_a_living_byzantium() {
        let scenario = crate::scenarios::registry::load_by_id("constantinople_1430").expect("scenario");
        let run = |byzantium_alive: bool| {
            let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, 1);
            for a in scenario.actors.iter().filter(|a| !a.is_successor_template) {
                world.actors.insert(a.id.clone(), a.clone());
            }
            if !byzantium_alive {
                world.actors.remove("byzantium");
                world.dead_actor_ids.insert("byzantium".to_string());
            }
            world.tick = scenario.victory_condition.as_ref().unwrap().minimum_tick;
            MetricRef::literal("global:federation_progress").apply(&mut world, 90.0);
            world.actors.get_mut("ottomans").unwrap().set_metric("military_size", 0.0);
            for _ in 0..3 {
                check_victory_condition(&mut world, &scenario);
            }
            world.victory_achieved
        };
        assert!(run(true), "with Byzantium alive the victory must come");
        assert!(!run(false), "over a fallen Byzantium there is no victory");
    }

    #[test]
    fn test_tick_advances_time() {
        let mut world = WorldState::with_seed("test".to_string(), 375, 0);
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

    // ------------------------------------------------------------------
    // B46: `group`, `requires_alive` on milestones, `closes_group` on the victory.
    // ------------------------------------------------------------------

    /// A milestone that holds from tick 0, with the B46 fields given.
    fn dated_milestone(id: &str, group: Option<&str>, requires_alive: &[&str]) -> crate::core::MilestoneEvent {
        crate::core::MilestoneEvent {
            id: id.to_string(),
            condition: crate::core::EventCondition {
                condition_type: EventConditionType::Tick { tick: 0 },
                duration: None,
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: String::new(),
            cooldown_ticks: None,
            spawn_actor: None,
            splits_actor: None,
            after: None,
            group: group.map(str::to_string),
            requires_alive: requires_alive.iter().map(|s| s.to_string()).collect(),
            effects: Default::default(),
            begins_conquest: None,
            economy_v2_effects: Default::default(),
        }
    }

    fn fired_after_one_check(scenario: &Scenario, world: &mut WorldState) -> Vec<String> {
        let mut log = EventLog::new();
        check_milestone_events(world, scenario, &mut log, &mut rand_chacha::ChaCha8Rng::seed_from_u64(0));
        world.milestone_events_fired.clone()
    }

    /// Within a group the first milestone to fire closes the rest — both hold on the same
    /// tick, only the first fires. Without the group both would.
    #[test]
    fn a_group_lets_only_its_first_milestone_fire() {
        let mut scenario = empty_scenario();
        scenario.milestone_events = vec![dated_milestone("a", Some("g"), &[]), dated_milestone("b", Some("g"), &[])];
        let mut world = WorldState::with_seed("test".into(), 1430, 0);
        assert_eq!(fired_after_one_check(&scenario, &mut world), vec!["a".to_string()]);
        scenario.milestone_events = vec![dated_milestone("a", None, &[]), dated_milestone("b", None, &[])];
        let mut world = WorldState::with_seed("test".into(), 1430, 0);
        assert_eq!(fired_after_one_check(&scenario, &mut world).len(), 2, "ungrouped, both fire");
    }

    /// A victory with `closes_group` closes the group; without it the milestone fires.
    #[test]
    fn a_victory_closes_its_group() {
        let mut scenario = empty_scenario();
        scenario.milestone_events = vec![dated_milestone("held", Some("ending"), &[])];
        scenario.victory_condition = Some(crate::core::VictoryCondition {
            metric: mr("global:x", None),
            threshold: 1.0,
            title: String::new(),
            description: String::new(),
            minimum_tick: 0,
            additional_conditions: vec![],
            sustained_ticks_required: 1,
            requires_alive: vec![],
            closes_group: Some("ending".into()),
        });
        let mut world = WorldState::with_seed("test".into(), 1430, 0);
        world.victory_achieved = true;
        assert!(fired_after_one_check(&scenario, &mut world).is_empty(), "won: the ending is closed");
        scenario.victory_condition.as_mut().unwrap().closes_group = None;
        let mut world = WorldState::with_seed("test".into(), 1430, 0);
        world.victory_achieved = true;
        assert_eq!(fired_after_one_check(&scenario, &mut world), vec!["held".to_string()]);
    }

    /// `requires_alive`: an absent actor keeps the milestone from firing; present, it fires.
    #[test]
    fn a_milestone_requires_its_actors_alive() {
        let mut scenario = empty_scenario();
        scenario.milestone_events = vec![dated_milestone("m", None, &["city"])];
        let mut world = WorldState::with_seed("test".into(), 1430, 0);
        assert!(fired_after_one_check(&scenario, &mut world).is_empty(), "the city is absent");
        let mut world = WorldState::with_seed("test".into(), 1430, 0);
        world.actors.insert("city".into(), vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]));
        assert_eq!(fired_after_one_check(&scenario, &mut world), vec!["m".to_string()]);
    }

    /// Economy v2 (Ц1, brief §9.6): a tag's `economic_output` modifier is a level — given once,
    /// taken back when the tag leaves; its other metrics stay a rate. Both ways: with the switch
    /// off the same tag adds every tick, as in v1.
    #[test]
    fn economy_v2_gives_economic_output_tags_as_a_level() {
        let run = |v2: bool| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.actor_tags.insert("trade".into(), crate::core::ActorTag {
                metrics_modifier: HashMap::from([
                    (crate::core::MetricName::new("economic_output").unwrap(), 2),
                    (crate::core::MetricName::new("cohesion").unwrap(), 1),
                ]),
                spreads_via: vec![],
            });
            world.actors.insert("city".into(), a);
            let mut seen = Vec::new();
            for t in 0..5 {
                if t == 3 {
                    world.actors.get_mut("city").unwrap().actor_tags.remove("trade");
                }
                apply_actor_tags(&mut world, &scenario);
                let c = &world.actors["city"];
                seen.push((c.get_metric("economic_output"), c.get_metric("cohesion")));
            }
            (seen, world.tag_levels.get("economic_output").and_then(|m| m.get("city")).map(|m| m.len()).unwrap_or(0))
        };
        let (v2, left) = run(true);
        assert_eq!(v2, vec![(52.0, 51.0), (52.0, 52.0), (52.0, 53.0), (50.0, 53.0), (50.0, 53.0)],
            "v2: economic_output +2 once and back on removal; cohesion stays a rate");
        assert_eq!(left, 0, "the level of a removed tag is forgotten");
        let (v1, _) = run(false);
        assert_eq!(v1, vec![(52.0, 51.0), (54.0, 52.0), (56.0, 53.0), (56.0, 53.0), (56.0, 53.0)], "v1: every tick");
    }

    /// Economy v2 (Ц1 stage 2): `economic_output` is pulled toward `T` = the authored base + its
    /// tags' levels, `eo += r × (T − eo)`. Both ways: with v2 off, or with no `r`, nothing pulls.
    #[test]
    fn economy_v2_pulls_economic_output_toward_its_target() {
        let run = |v2: bool, r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            scenario.economy_v2_eo_pull = r;
            // the authored base: 50 (as `vassalage_actor` sets it)
            scenario.actors.push(vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]));
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.set_metric("economic_output", 10.0);
            world.actors.insert("city".into(), a);
            world.tag_levels.entry("economic_output".into()).or_default().insert("city".into(), std::collections::BTreeMap::from([("trade".to_string(), 4.0)]));
            pull_economic_output_to_target(&mut world, &scenario);
            world.actors["city"].get_metric("economic_output")
        };
        // T = 50 + 4 = 54; 10 + 0.1 × (54 − 10) = 14.4
        assert!((run(true, Some(0.1)) - 14.4).abs() < 1e-9, "pulled a tenth of the way to T");
        assert_eq!(run(false, Some(0.1)), 10.0, "v1 does not pull");
        assert_eq!(run(true, None), 10.0, "no r, no pull");
    }

    /// A world of two neighbours for the war tests: a great power and a small one, with battles
    /// on every allowed tick (the conquest rule on when `k2` is given).
    fn war_world(k2: Option<u32>) -> (Scenario, WorldState) {
        let mut scenario = empty_scenario();
        scenario.features.economy_v2 = true;
        scenario.economy_v2_combat_outcome = true;
        scenario.economy_v2_conquest_k2 = k2;
        scenario.military_conflict_probability = 1.0;
        scenario.actors.push(vassalage_actor("great", 1000.0, 0.0, 50.0, 50.0, &["small"]));
        scenario.actors.push(vassalage_actor("small", 20.0, 0.0, 50.0, 50.0, &["great"]));
        let mut world = WorldState::with_seed("test".into(), 1430, 0);
        let mut great = vassalage_actor("great", 1000.0, 0.0, 50.0, 50.0, &["small"]);
        great.set_metric("military_quality", 100.0);
        world.actors.insert("great".into(), great);
        world.actors.insert("small".into(), vassalage_actor("small", 20.0, 0.0, 50.0, 50.0, &["great"]));
        (scenario, world)
    }

    fn fight(world: &mut WorldState, scenario: &Scenario, ticks: std::ops::Range<u32>) {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
        let mut log = EventLog::new();
        for t in ticks {
            world.tick = t;
            interactions::calculate_interactions(world, scenario, &mut log, &mut rng);
        }
    }

    /// Economy v2 (Ц7): three battles lost in a row to an overwhelming winner make the loser its
    /// vassal. Both ways: without the conquest rule nobody submits.
    #[test]
    fn economy_v2_lost_battles_make_a_vassal() {
        for (k2, expect) in [(Some(1), true), (None, false)] {
            let (scenario, mut world) = war_world(k2);
            fight(&mut world, &scenario, 3..40);
            let bound = world.vassalages.iter().any(|v| v.vassal_id == "small" && v.overlord_id == "great");
            assert_eq!(bound, expect, "conquest rule {k2:?}");
        }
    }

    /// Economy v2 (Ц7): overlord and vassal do not fight. Both ways: without the rule the same
    /// bound pair does.
    #[test]
    fn economy_v2_vassal_and_overlord_do_not_fight() {
        for (k2, peace) in [(Some(1), true), (None, false)] {
            let (scenario, mut world) = war_world(k2);
            world.vassalages.push(crate::core::Vassalage { vassal_id: "small".into(), overlord_id: "great".into(), formed_tick: 0 });
            fight(&mut world, &scenario, 3..40);
            assert_eq!(world.actors["small"].get_metric("military_size") == 20.0, peace, "conquest rule {k2:?}");
        }
    }

    /// Economy v2 (Ц7): `begins_conquest` breaks the pair's vassalage and forbids it again; then
    /// K₂ lost battles kill the target by the conquest path. Both ways: without the rule the
    /// milestone leaves the vassalage, and the conquest path keeps reading legitimacy.
    #[test]
    fn economy_v2_declared_conquest_breaks_the_bond_and_kills() {
        let run = |k2: Option<u32>| {
            let (mut scenario, mut world) = war_world(k2);
            let mut m = dated_milestone("assault", None, &[]);
            m.condition.condition_type = EventConditionType::Tick { tick: 2 };
            m.begins_conquest = Some(crate::core::BeginsConquest { attacker: "great".into(), target: "small".into() });
            scenario.milestone_events = vec![m];
            world.vassalages.push(crate::core::Vassalage { vassal_id: "small".into(), overlord_id: "great".into(), formed_tick: 0 });
            world.tick = 2;
            let mut log = EventLog::new();
            check_milestone_events(&mut world, &scenario, &mut log, &mut rand_chacha::ChaCha8Rng::seed_from_u64(0));
            let bond_after_milestone = !world.vassalages.is_empty();
            fight(&mut world, &scenario, 3..40);
            let rebound = !world.vassalages.is_empty();
            let conquered = world.conquered_by.get("small").cloned();
            // the conquest path: in danger while conquered, dead after its three ticks
            for t in 40..44 {
                world.tick = t;
                check_collapses(&mut world, &scenario, &mut log);
            }
            (bond_after_milestone, rebound, conquered, world.dead_actor_ids.contains("small"))
        };
        assert_eq!(run(Some(1)), (false, false, Some("great".to_string()), true), "broken, not rebound, conquered, dead");
        let (bond, _, conquered, dead) = run(None);
        assert!(bond && conquered.is_none() && !dead, "without the rule: the bond stays, nobody is conquered");
    }

    /// Economy v2 (Ц7): `begins_conquest` opens with the assault — one battle on the milestone's
    /// tick, without the roll for an attack; with K₂ = 1 an overwhelming conqueror takes the city
    /// that tick. Both ways: a milestone without `begins_conquest` fights nothing.
    #[test]
    fn economy_v2_declared_conquest_opens_with_an_assault() {
        let run = |declare: bool| {
            let (mut scenario, mut world) = war_world(Some(1));
            scenario.military_conflict_probability = 0.0; // no ordinary battle can happen
            let mut m = dated_milestone("assault", None, &[]);
            m.condition.condition_type = EventConditionType::Tick { tick: 5 };
            if declare { m.begins_conquest = Some(crate::core::BeginsConquest { attacker: "great".into(), target: "small".into() }); }
            scenario.milestone_events = vec![m];
            world.tick = 5;
            let mut log = EventLog::new();
            check_milestone_events(&mut world, &scenario, &mut log, &mut rand_chacha::ChaCha8Rng::seed_from_u64(3));
            check_collapses(&mut world, &scenario, &mut log);
            (world.actors.get("small").map(|a| a.get_metric("military_size")), world.dead_actor_ids.contains("small"))
        };
        let (_, dead) = run(true);
        assert!(dead, "the assault on the milestone's tick: S 1000 against 10, K₂ = 1 — the city falls that tick");
        assert_eq!(run(false), (Some(20.0), false), "without begins_conquest: no battle");
    }

    /// Economy v2 (Ц9): allies do not fight each other. Both ways: with the switch off the same
    /// allied pair does.
    #[test]
    fn economy_v2_allies_do_not_fight() {
        for (on, peace) in [(true, true), (false, false)] {
            let (mut scenario, mut world) = war_world(None);
            scenario.economy_v2_alliances = on;
            world.alliances.push(crate::core::Alliance { actor_ids: vec!["great".into(), "small".into()], common_enemy: None, trade_benefit: false, formed_tick: 0 });
            fight(&mut world, &scenario, 3..40);
            assert_eq!(world.actors["small"].get_metric("military_size") == 20.0, peace, "alliances {on}");
        }
    }

    /// Economy v2 (Ц6, Ц9): an ally's army is no threat. Both ways: without the alliance it is.
    #[test]
    fn economy_v2_an_ally_is_no_threat() {
        for (allied, threat) in [(true, 0.0), (false, 100.0 * 1000.0 / 1020.0)] {
            let (_, mut world) = war_world(None);
            if allied {
                world.alliances.push(crate::core::Alliance { actor_ids: vec!["great".into(), "small".into()], common_enemy: None, trade_benefit: false, formed_tick: 0 });
            }
            let t = pressure_threat(&world, "small").unwrap();
            assert!((t - threat).abs() < 1e-9, "allied {allied}: {t}");
        }
    }

    /// Economy v2 (Ц7, Ц9): the loser's allies join its side of the ratio — S 1000 against the
    /// small power's 10 overwhelms it, against 10 + an ally's 500 it does not, so no streak and no
    /// submission. Both ways: with the switch off the ally is not counted and the small power
    /// submits.
    #[test]
    fn economy_v2_allies_join_the_losers_side_of_the_ratio() {
        for (on, submits) in [(true, false), (false, true)] {
            let (mut scenario, mut world) = war_world(Some(1));
            scenario.economy_v2_alliances = on;
            world.actors.insert("ally".into(), vassalage_actor("ally", 1000.0, 0.0, 50.0, 50.0, &[]));
            world.alliances.push(crate::core::Alliance { actor_ids: vec!["small".into(), "ally".into()], common_enemy: None, trade_benefit: false, formed_tick: 0 });
            fight(&mut world, &scenario, 3..40);
            let bound = world.vassalages.iter().any(|v| v.vassal_id == "small" && v.overlord_id == "great");
            assert_eq!(bound, submits, "alliances {on}");
        }
    }

    /// Economy v2 (Ц9): the scenario's starting alliances enter the world on its first tick, once.
    /// Both ways: with the switch off the world has none.
    #[test]
    fn economy_v2_starting_alliances_enter_on_the_first_tick() {
        for on in [true, false] {
            let (mut scenario, mut world) = war_world(None);
            scenario.economy_v2_alliances = on;
            scenario.starting_alliances = vec![crate::core::StartingAlliance { actors: vec!["great".into(), "small".into()] }];
            let mut log = EventLog::new();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0);
            tick(&mut world, &scenario, &mut log, &mut rng);
            tick(&mut world, &scenario, &mut log, &mut rng);
            assert_eq!(world.alliances.len(), on as usize, "alliances {on}");
            assert_eq!(interactions::allied(&world, "great", "small"), on);
        }
    }

    /// Economy v2 (Ц9): an event's `leaves_alliance_as_enemy` — its target leaves every alliance
    /// and becomes the common enemy; an alliance left with one member ends. Both ways: with the
    /// switch off the event writes its metrics and the alliances stand.
    #[test]
    fn economy_v2_event_turns_an_alliance_against_a_member() {
        for on in [true, false] {
            let (mut scenario, mut world) = war_world(None);
            scenario.economy_v2_alliances = on;
            world.actors.insert("third".into(), vassalage_actor("third", 10.0, 0.0, 50.0, 50.0, &[]));
            world.alliances = vec![
                crate::core::Alliance { actor_ids: vec!["great".into(), "small".into(), "third".into()], common_enemy: None, trade_benefit: false, formed_tick: 0 },
                crate::core::Alliance { actor_ids: vec!["great".into(), "third".into()], common_enemy: None, trade_benefit: false, formed_tick: 0 },
            ];
            scenario.random_events = vec![crate::core::RandomEvent {
                id: "league_turns".into(),
                probability: 1.0,
                target: crate::core::EventTarget::Actor("great".into()),
                conditions: vec![],
                effects: HashMap::from([(crate::core::RelativeMetricRef::literal("actor:great.legitimacy"), -12.0)]),
                llm_context: String::new(),
                one_time: true,
                leaves_alliance_as_enemy: Some("great".into()),
                economy_v2_population_share: None,
            }];
            let mut log = EventLog::new();
            phase_random_events(&mut world, &scenario, &mut log, &mut rand_chacha::ChaCha8Rng::seed_from_u64(0));
            assert_eq!(world.actors["great"].get_metric("legitimacy"), 38.0, "the metrics are written either way");
            if on {
                assert_eq!(world.alliances.len(), 1, "the pair great–third ends");
                assert_eq!(world.alliances[0].actor_ids, vec!["small".to_string(), "third".to_string()]);
                assert_eq!(world.alliances[0].common_enemy.as_deref(), Some("great"));
                assert!(!interactions::allied(&world, "great", "small") && interactions::allied(&world, "small", "third"));
            } else {
                assert_eq!(world.alliances.len(), 2, "switch off: the alliances stand");
                assert!(interactions::allied(&world, "great", "small"));
            }
        }
    }

    /// Economy v2 (Ц7): tribute is paid only out of a non-negative treasury — the overlord gets
    /// what was paid. Both ways: without the floor the vassal's treasury goes below zero.
    #[test]
    fn economy_v2_tribute_stops_at_the_treasury_floor() {
        let run = |floor: bool| {
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut v = vassalage_actor("vassal", 10.0, 0.0, 50.0, 50.0, &[]);
            v.set_metric("treasury", 0.5);
            world.actors.insert("vassal".into(), v);
            world.actors.insert("lord".into(), vassalage_actor("lord", 100.0, 0.0, 50.0, 50.0, &[]));
            world.vassalages.push(crate::core::Vassalage { vassal_id: "vassal".into(), overlord_id: "lord".into(), formed_tick: 0 });
            let mut log = EventLog::new();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
            interactions::calculate_vassalage_interaction(&mut world, &mut log, &mut rng, floor);
            (world.actors["vassal"].get_metric("treasury"), world.actors["lord"].get_metric("treasury"))
        };
        // economic_output 50 → tribute 1.5–2.5, more than the vassal's 0.5
        assert_eq!(run(true), (0.0, 100.5), "floor: 0.5 paid, the rest lost at the floor");
        let (v, l) = run(false);
        assert!(v < 0.0 && l > 100.5 && (v + l - 100.5).abs() < 1e-9, "no floor: the vassal goes into debt for it");
    }

    /// Economy v2 (Ц4): the battle's outcome. Over a uniform grid of draws the attacker wins the
    /// share `S_a / (S_a + S_d)`, `S = army × quality / 100`. Both ways: with equal armies the side
    /// of higher quality wins more often, and swapping the qualities swaps the outcome; with equal
    /// qualities numbers decide; without quality (variant (б)) equal armies win half each.
    #[test]
    fn economy_v2_battle_outcome_reads_strength() {
        let side = |army: f64, quality: f64| {
            let mut a = vassalage_actor("x", army, 50.0, 50.0, 50.0, &[]);
            a.set_metric("military_quality", quality);
            a
        };
        let share = |a: &crate::core::Actor, d: &crate::core::Actor, with_quality: bool| {
            let n = 10_000;
            (0..n).filter(|i| interactions::resolve_battle(a, d, (*i as f64 + 0.5) / n as f64, with_quality, true).attacker_wins).count() as f64 / n as f64
        };
        let close = |x: f64, y: f64| (x - y).abs() < 1e-3;
        assert!(close(share(&side(100.0, 80.0), &side(100.0, 40.0), true), 2.0 / 3.0), "higher quality wins two in three");
        assert!(close(share(&side(100.0, 40.0), &side(100.0, 80.0), true), 1.0 / 3.0), "swapped qualities, swapped outcome");
        assert!(close(share(&side(100.0, 60.0), &side(50.0, 60.0), true), 2.0 / 3.0), "equal quality: numbers decide");
        assert!(close(share(&side(100.0, 80.0), &side(100.0, 40.0), false), 0.5), "without quality equal armies are even");
        // a victory over the weak is nearly free: S 80 against S 5 scales the winner's loss by 5 / 80
        let b = interactions::resolve_battle(&side(100.0, 80.0), &side(10.0, 50.0), 0.0, true, true);
        assert!(b.attacker_wins && (b.winner_scale - 5.0 / 80.0).abs() < 1e-12);
        assert_eq!(interactions::resolve_battle(&side(100.0, 80.0), &side(10.0, 50.0), 0.0, true, false).winner_scale, 1.0, "unscaled (variant (в))");
    }

    /// Economy v2 (Ц4): with the outcome on, the loser — whichever side — takes the 15–30 % army
    /// loss and the cohesion loss, the winner a scaled 5–15 %. Both ways: off, the defender always
    /// takes them and the attacker always loses 5–15 %.
    #[test]
    fn economy_v2_battle_loser_takes_the_losses() {
        let run = |outcome: bool, seed: u64| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_combat_outcome = outcome;
            scenario.military_conflict_probability = 1.0;
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            world.tick = 5;
            // the bigger army attacks; its quality is so low that it is the weaker side
            let mut a = vassalage_actor("a", 1000.0, 0.0, 50.0, 50.0, &["b"]);
            a.set_metric("military_quality", 1.0);
            let mut b = vassalage_actor("b", 100.0, 0.0, 50.0, 50.0, &["a"]);
            b.set_metric("military_quality", 100.0);
            world.actors.insert("a".into(), a);
            world.actors.insert("b".into(), b);
            let mut log = EventLog::new();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
            interactions::calculate_interactions(&mut world, &scenario, &mut log, &mut rng);
            let g = |id: &str, m: &str| world.actors[id].get_metric(m);
            (g("a", "military_size") / 1000.0, g("a", "cohesion"), g("b", "military_size") / 100.0, g("b", "cohesion"))
        };
        let fought = |r: (f64, f64, f64, f64)| r.0 < 1.0 || r.2 < 1.0;
        let seed = (0..200).find(|s| fought(run(false, *s))).expect("a battle in 200 seeds");
        let off = run(false, seed);
        assert!(off.1 > 45.0 && off.3 <= 40.0, "off: the defender loses cohesion");
        assert!((0.85..=0.95).contains(&off.0), "off: the attacker loses 5–15 %");
        // on: S_a = 10, S_d = 100 — the defender wins in ten of eleven; find a battle it won
        let seed = (0..200).find(|s| { let r = run(true, *s); fought(r) && r.1 <= 40.0 }).expect("a defender's win in 200 seeds");
        let on = run(true, seed);
        assert!((0.70..=0.85).contains(&on.0) && on.1 <= 40.0, "on: the losing attacker takes 15–30 % and the cohesion");
        // the winner's 5–15 % scaled by S_loser / S_winner = 10 / 100
        assert!((0.985..=0.995).contains(&on.2) && on.3 > 45.0, "on: the winner's loss is scaled, its cohesion kept");
    }

    /// Economy v2 (Ц8): cohesion is pulled toward `T_C` = the authored start + the tags' levels, from
    /// below as from above. Both ways: v1, or no r, leaves it alone.
    #[test]
    fn economy_v2_pulls_cohesion_toward_its_norm() {
        let run = |v2: bool, r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            scenario.economy_v2_cohesion_pull = r;
            scenario.actors.push(vassalage_actor("city", 50.0, 50.0, 50.0, 60.0, &[]));
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            world.actors.insert("city".into(), vassalage_actor("city", 50.0, 50.0, 50.0, 20.0, &[]));
            world.tag_levels.entry("cohesion".into()).or_default().insert("city".into(), std::collections::BTreeMap::from([("faction".to_string(), -10.0)]));
            pull_cohesion_to_target(&mut world, &scenario);
            world.actors["city"].get_metric("cohesion")
        };
        // T_C = 60 − 10 = 50; 20 + 0.12 × (50 − 20) = 23.6 — pulled up, which the decay rule never did
        assert!((run(true, Some(0.12)) - 23.6).abs() < 1e-9);
        assert_eq!(run(false, Some(0.12)), 20.0, "v1 does not pull");
        assert_eq!(run(true, None), 20.0, "no r, no pull");
    }

    /// Economy v2 (Ц10): population is pulled toward `P₀ × eo / T`. Both ways: v1 and no r leave it.
    #[test]
    fn economy_v2_pulls_population_toward_its_norm() {
        let run = |v2: bool, r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            scenario.economy_v2_population_pull = r;
            let mut base = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            base.set_metric("population", 1000.0);
            scenario.actors.push(base); // P₀ = 1000, T = 50
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.set_metric("population", 400.0);
            a.set_metric("economic_output", 25.0);
            world.actors.insert("city".into(), a);
            pull_population_to_norm(&mut world, &scenario);
            world.actors["city"].get_metric("population")
        };
        // N = 1000 × 25 / 50 = 500; 400 + 0.1 × (500 − 400) = 410
        assert!((run(true, Some(0.1)) - 410.0).abs() < 1e-9);
        assert_eq!(run(false, Some(0.1)), 400.0, "v1 does not pull");
        assert_eq!(run(true, None), 400.0, "no r, no pull");
    }

    /// Economy v2 (Ц10): under the population pull an event's population blow is its share of the
    /// people. Both ways: without the pull the authored number.
    #[test]
    fn economy_v2_population_events_take_a_share_under_the_pull() {
        let run = |r: Option<f64>, pop: f64| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_population_pull = r;
            scenario.random_events = vec![crate::core::RandomEvent {
                id: "famine".into(),
                probability: 1.0,
                target: crate::core::EventTarget::Actor("city".into()),
                conditions: vec![],
                effects: HashMap::from([(crate::core::RelativeMetricRef::literal("self.population"), -20.0)]),
                llm_context: String::new(),
                one_time: false,
                leaves_alliance_as_enemy: None,
                economy_v2_population_share: Some(0.05),
            }];
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.set_metric("population", pop);
            world.actors.insert("city".into(), a);
            let mut log = EventLog::new();
            phase_random_events(&mut world, &scenario, &mut log, &mut rand_chacha::ChaCha8Rng::seed_from_u64(0));
            world.actors["city"].get_metric("population")
        };
        assert!((run(Some(0.01), 40.0) - 38.0).abs() < 1e-9, "the pull: 5 % of 40");
        assert!((run(Some(0.01), 8000.0) - 7600.0).abs() < 1e-9, "the pull: 5 % of 8000");
        assert_eq!(run(None, 40.0), 20.0, "no pull: the authored −20");
    }

    /// Economy v2 (Ц10): under the population pull the economy-to-population deficit rule is not
    /// applied. Both ways: without the pull it takes people.
    #[test]
    fn economy_v2_population_pull_replaces_the_deficit_rules() {
        let run = |r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_population_pull = r;
            scenario.dependencies.push(crate::core::DependencyRule {
                id: "economic_output_to_population".into(),
                from: crate::core::MetricName::new("economic_output").unwrap(),
                to: crate::core::MetricName::new("population").unwrap(),
                coefficient: 0.125,
                threshold: Some(50.0),
                mode: crate::core::DependencyMode::DeficitProportional,
            });
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.set_metric("population", 1000.0);
            a.set_metric("economic_output", 0.0);
            world.actors.insert("city".into(), a);
            phase_apply_dependencies(&mut world, &scenario);
            world.actors["city"].get_metric("population")
        };
        assert_eq!(run(Some(0.01)), 1000.0, "the pull: the rule gives way");
        assert!(run(None) < 1000.0, "no pull: the rule takes people");
    }

    /// Economy v2 (Ц10): a split scales the seat's population base by the share it kept. Both ways:
    /// without the pull nothing is recorded.
    #[test]
    fn economy_v2_split_scales_the_population_base() {
        let run = |r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_population_pull = r;
            let mut parent = vassalage_actor("empire", 50.0, 50.0, 50.0, 50.0, &[]);
            parent.set_metric("population", 1000.0);
            parent.on_collapse = vec![
                crate::core::Successor { id: "empire".into(), weight: 0.45, keeps_seat: true },
                crate::core::Successor { id: "east".into(), weight: 0.55, keeps_seat: false },
            ];
            scenario.actors.push(parent.clone());
            let mut east = vassalage_actor("east", 50.0, 50.0, 50.0, 50.0, &[]);
            east.is_successor_template = true;
            scenario.actors.push(east);
            let mut m = dated_milestone("split", None, &[]);
            m.splits_actor = Some("empire".into());
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            world.actors.insert("empire".into(), parent);
            let mut log = EventLog::new();
            apply_seat_split(&mut world, &scenario, &m, &mut log);
            (world.population_base_scale.get("empire").copied(), population_base(&world, &scenario, "empire"))
        };
        let (scale, base) = run(Some(0.01));
        assert!((scale.unwrap() - 0.45).abs() < 1e-9 && (base.unwrap() - 450.0).abs() < 1e-9, "the seat keeps 45 % of P₀");
        assert_eq!(run(None).0, None, "no pull: nothing recorded");
    }

    /// Economy v2 (Ц8): with the cohesion pull, tags' `cohesion` modifiers are a level. Both ways:
    /// without it the tag adds every tick.
    #[test]
    fn economy_v2_gives_cohesion_tags_as_a_level() {
        let run = |r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_cohesion_pull = r;
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.actor_tags.insert("faction".into(), crate::core::ActorTag {
                metrics_modifier: HashMap::from([(crate::core::MetricName::new("cohesion").unwrap(), -2)]),
                spreads_via: vec![],
            });
            world.actors.insert("city".into(), a);
            let mut seen = Vec::new();
            for t in 0..4 {
                if t == 2 { world.actors.get_mut("city").unwrap().actor_tags.remove("faction"); }
                apply_actor_tags(&mut world, &scenario);
                seen.push(world.actors["city"].get_metric("cohesion"));
            }
            seen
        };
        assert_eq!(run(Some(0.12)), vec![48.0, 48.0, 50.0, 50.0], "level: −2 once, back on removal");
        assert_eq!(run(None), vec![48.0, 46.0, 46.0, 46.0], "rate: every tick while carried");
    }

    /// Economy v2 (Ц8): a rule reading cohesion measures against `T_C`, and the cohesion decay rule
    /// gives way to the pull. Both ways: without the pull both act as authored.
    #[test]
    fn economy_v2_measures_cohesion_readers_against_the_norm() {
        let run = |r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_cohesion_pull = r;
            let dep = |id: &str, to: &str, coefficient: f64, mode: crate::core::DependencyMode| crate::core::DependencyRule {
                id: id.into(),
                from: crate::core::MetricName::new("cohesion").unwrap(),
                to: crate::core::MetricName::new(to).unwrap(),
                coefficient,
                threshold: Some(50.0),
                mode,
            };
            scenario.dependencies.push(dep("cohesion_to_legitimacy", "legitimacy", 0.02, crate::core::DependencyMode::Deficit));
            scenario.dependencies.push(dep("cohesion_natural_decay", "cohesion", 0.12, crate::core::DependencyMode::Excess));
            // T_C = 40: the reader's threshold becomes 20, and cohesion 30 is above it
            scenario.actors.push(vassalage_actor("city", 50.0, 50.0, 50.0, 40.0, &[]));
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 30.0, &[]);
            a.set_metric("legitimacy", 50.0);
            world.actors.insert("city".into(), a);
            phase_apply_dependencies(&mut world, &scenario);
            let first = world.actors["city"].get_metric("legitimacy");
            world.actors.get_mut("city").unwrap().set_metric("cohesion", 80.0);
            phase_apply_dependencies(&mut world, &scenario);
            (first, world.actors["city"].get_metric("cohesion"))
        };
        assert_eq!(run(Some(0.12)), (50.0, 80.0), "30 is above 50 × 40 / 100; the decay rule is not applied");
        let (l, c) = run(None);
        // −(50 − 30) × 0.02 = −0.4; decay −(80 − 50) × 0.12 = −3.6
        assert!((l - 49.6).abs() < 1e-9 && (c - 76.4).abs() < 1e-9, "absolute threshold, decay applied: {l} {c}");
    }

    /// Economy v2 (Ц5): legitimacy is pulled toward `T_L` = the authored start + the tags' levels.
    /// Both ways: v1, or no r, leaves it alone.
    #[test]
    fn economy_v2_pulls_legitimacy_toward_its_norm() {
        let run = |v2: bool, r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            scenario.economy_v2_legitimacy_pull = r;
            scenario.actors.push(vassalage_actor("city", 50.0, 50.0, 40.0, 50.0, &[]));
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            world.actors.insert("city".into(), vassalage_actor("city", 50.0, 50.0, 10.0, 50.0, &[]));
            world.tag_levels.entry("legitimacy".into()).or_default().insert("city".into(), std::collections::BTreeMap::from([("court".to_string(), 4.0)]));
            pull_legitimacy_to_target(&mut world, &scenario);
            world.actors["city"].get_metric("legitimacy")
        };
        // T_L = 40 + 4 = 44; 10 + 0.1 × (44 − 10) = 13.4
        assert!((run(true, Some(0.1)) - 13.4).abs() < 1e-9, "pulled a tenth of the way to T_L");
        assert_eq!(run(false, Some(0.1)), 10.0, "v1 does not pull");
        assert_eq!(run(true, None), 10.0, "no r, no pull");
    }

    /// Economy v2 (Ц5): with the legitimacy pull, tags' `legitimacy` modifiers are a level —
    /// given once, taken back when the tag leaves. Both ways: without it the tag adds every tick.
    #[test]
    fn economy_v2_gives_legitimacy_tags_as_a_level() {
        let run = |r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_legitimacy_pull = r;
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.actor_tags.insert("court".into(), crate::core::ActorTag {
                metrics_modifier: HashMap::from([(crate::core::MetricName::new("legitimacy").unwrap(), 2)]),
                spreads_via: vec![],
            });
            world.actors.insert("city".into(), a);
            let mut seen = Vec::new();
            for t in 0..4 {
                if t == 2 { world.actors.get_mut("city").unwrap().actor_tags.remove("court"); }
                apply_actor_tags(&mut world, &scenario);
                seen.push(world.actors["city"].get_metric("legitimacy"));
            }
            seen
        };
        assert_eq!(run(Some(0.05)), vec![52.0, 52.0, 50.0, 50.0], "level: +2 once, back on removal");
        assert_eq!(run(None), vec![52.0, 54.0, 54.0, 54.0], "rate: every tick while carried");
    }

    /// Economy v2 (Ц5): a dependency rule reading legitimacy measures a fall below the actor's
    /// norm — threshold × `T_L` / 100. Both ways: without the pull the absolute threshold holds.
    #[test]
    fn economy_v2_measures_legitimacy_readers_against_the_norm() {
        let run = |r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_legitimacy_pull = r;
            scenario.dependencies.push(crate::core::DependencyRule {
                id: "legitimacy_to_cohesion".into(),
                from: crate::core::MetricName::new("legitimacy").unwrap(),
                to: crate::core::MetricName::new("cohesion").unwrap(),
                coefficient: 0.03,
                threshold: Some(50.0),
                mode: crate::core::DependencyMode::Deficit,
            });
            // the norm T_L = 40: the threshold becomes 20, and legitimacy 30 is above it
            scenario.actors.push(vassalage_actor("city", 50.0, 50.0, 40.0, 50.0, &[]));
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            world.actors.insert("city".into(), vassalage_actor("city", 50.0, 50.0, 30.0, 50.0, &[]));
            phase_apply_dependencies(&mut world, &scenario);
            world.actors["city"].get_metric("cohesion")
        };
        assert_eq!(run(Some(0.05)), 50.0, "30 is above 50 × 40 / 100 = 20");
        // −(50 − 30) × 0.03 = −0.6
        assert!((run(None) - 49.4).abs() < 1e-9, "absolute threshold 50");
    }

    /// Economy v2 (Ц2): a treasury below zero `n` ticks in a row costs the army `cut` of itself
    /// every tick from the n-th on, and the count resets once the treasury is back at zero or
    /// above. Both ways: with v2 off, or with no cut, the army is untouched.
    #[test]
    fn economy_v2_debt_shrinks_the_army() {
        let run = |v2: bool, cut: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            scenario.economy_v2_debt_ticks = Some(2);
            scenario.economy_v2_debt_cut = cut;
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 100.0, 50.0, 50.0, 50.0, &[]);
            a.set_metric("treasury", -1000.0);
            a.set_metric("population", 0.0); // no income: the treasury stays below zero
            world.actors.insert("city".into(), a);
            let mut armies = Vec::new();
            for _ in 0..3 {
                apply_treasury(&mut world, &scenario);
                armies.push(world.actors["city"].get_metric("military_size"));
            }
            // back above zero: the count resets
            world.actors.get_mut("city").unwrap().set_metric("treasury", 1.0e6);
            apply_treasury(&mut world, &scenario);
            (armies, world.debt_ticks.get("city").copied())
        };
        let (with, count) = run(true, Some(0.5));
        assert_eq!(with, vec![100.0, 50.0, 25.0], "the first tick of debt is free, then −50 % a tick");
        assert_eq!(count, None, "the count resets above zero");
        assert_eq!(run(false, Some(0.5)).0, vec![100.0; 3], "v1: no debt rule");
        assert_eq!(run(true, None).0, vec![100.0; 3], "no cut, no rule");
    }

    /// Economy v2 (Ц1 stage 3): a rule whose source is `economic_output` measures a fall below
    /// the actor's own norm — threshold × T / 100. A poor actor (T = 30) at 20 is not in deficit
    /// against «below 50»; it is against «below 50 % of 30 = 15» only under 15. Both ways: v1
    /// keeps the absolute 50, and a rule on another source is untouched.
    #[test]
    fn economy_v2_measures_economic_output_deficits_against_the_norm() {
        let rule = |from: &str| crate::core::DependencyRule {
            id: "r".into(),
            from: crate::core::MetricName::new(from).unwrap(),
            to: crate::core::MetricName::new("treasury").unwrap(),
            coefficient: 1.0,
            threshold: Some(50.0),
            mode: crate::core::DependencyMode::Deficit,
        };
        let run = |v2: bool, from: &str, eo: f64| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            let mut authored = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            authored.set_metric("economic_output", 30.0); // the norm: T = 30
            scenario.actors.push(authored);
            scenario.dependencies = vec![rule(from)];
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.set_metric("economic_output", eo);
            a.set_metric("cohesion", 20.0);
            // room above the zero floor (Ц2 stage 2), so only the threshold is measured
            a.set_metric("treasury", 100.0);
            world.actors.insert("city".into(), a);
            phase_apply_dependencies(&mut world, &scenario);
            world.actors["city"].get_metric("treasury") - 100.0
        };
        assert_eq!(run(true, "economic_output", 20.0), 0.0, "v2: 20 is above half the norm (15) — no deficit");
        assert_eq!(run(true, "economic_output", 10.0), -5.0, "v2: 10 is 5 below half the norm");
        assert_eq!(run(false, "economic_output", 20.0), -30.0, "v1: 30 below the absolute 50");
        assert_eq!(run(true, "cohesion", 20.0), -30.0, "another source keeps its absolute threshold");
    }

    /// Economy v2 (Ц2 stage 2): a loss other than the army's upkeep stops at the treasury's
    /// zero — a dependency draining the treasury takes what is there and no more, and takes
    /// nothing from a treasury already in debt. Both ways: v1 lets it go below zero.
    #[test]
    fn economy_v2_losses_stop_at_the_treasury_floor() {
        let rule = crate::core::DependencyRule {
            id: "drain".into(),
            from: crate::core::MetricName::new("cohesion").unwrap(),
            to: crate::core::MetricName::new("treasury").unwrap(),
            coefficient: 1.0,
            threshold: Some(50.0),
            mode: crate::core::DependencyMode::Deficit,
        };
        let run = |floor: bool, treasury: f64| {
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 20.0, &[]); // cohesion 20 → −30
            a.set_metric("treasury", treasury);
            apply_dependency_rule(&mut a, &rule, 0, 1.0, floor);
            a.get_metric("treasury")
        };
        assert_eq!(run(true, 10.0), 0.0, "v2: −30 against 10 takes 10 and stops at zero");
        assert_eq!(run(true, -5.0), -5.0, "v2: nothing more from a treasury in debt");
        assert_eq!(run(true, 100.0), 70.0, "v2: a loss within the treasury applies in full");
        assert_eq!(run(false, 10.0), -20.0, "v1: no floor");
    }

    /// Economy v2 (Ц2 stage 2): no money, no levy — an army in debt does not regrow toward its
    /// capacity. Both ways: a solvent army regrows, and v1 regrows in debt too.
    #[test]
    fn economy_v2_no_recruiting_in_debt() {
        let run = |solvent_only: bool, treasury: f64| {
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 2.0, 50.0, 50.0, 50.0, &[]);
            a.set_metric("population", 8000.0);
            a.set_metric("treasury", treasury);
            world.actors.insert("city".into(), a);
            interactions::apply_military_recovery(&mut world, solvent_only);
            world.actors["city"].get_metric("military_size")
        };
        assert_eq!(run(true, -1.0), 2.0, "v2: in debt, no recruiting");
        assert!(run(true, 1.0) > 2.0, "v2: solvent, the army regrows");
        assert!(run(false, -1.0) > 2.0, "v1: regrows in debt too");
    }

    /// Economy v2 (Ц2 stage 3): a state with population ≤ 1 for n ticks in a row collapses by the
    /// usual path — none of the three legitimacy/cohesion/pressure paths sees it. Both ways:
    /// v1 keeps the empty state alive, and a populated state lives.
    #[test]
    fn economy_v2_depopulated_state_collapses() {
        let run = |v2: bool, population: f64| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            scenario.economy_v2_depopulation_ticks = Some(2);
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            // healthy on the three classic paths: legitimacy, cohesion and pressure mid-scale
            let mut a = vassalage_actor("city", 10.0, 50.0, 50.0, 50.0, &[]);
            a.set_metric("population", population);
            world.actors.insert("city".into(), a);
            let mut log = EventLog::new();
            let mut alive = Vec::new();
            for _ in 0..3 {
                check_collapses(&mut world, &scenario, &mut log);
                alive.push(!world.dead_actor_ids.contains("city"));
            }
            alive
        };
        assert_eq!(run(true, 0.0), vec![true, false, false], "v2: dies on the second tick without people");
        assert_eq!(run(false, 0.0), vec![true, true, true], "v1: an empty state goes on living");
        assert_eq!(run(true, 50.0), vec![true, true, true], "v2: a populated state lives");
    }

    /// Economy v2 (Ц6): the threat is `100 × N / (N + own army)` over living neighbours at
    /// distance 1, and pressure is pulled toward it. Both ways: a dead or distant neighbour is no
    /// threat, and without v2 or without `r` nothing pulls.
    #[test]
    fn economy_v2_pulls_pressure_toward_the_threat() {
        let world_with = |dead_near: bool| {
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut city = vassalage_actor("city", 50.0, 10.0, 50.0, 50.0, &["near", "far"]);
            city.neighbors.iter_mut().find(|n| n.id == "far").unwrap().distance = 2;
            world.actors.insert("city".into(), city);
            world.actors.insert("near".into(), vassalage_actor("near", 150.0, 50.0, 50.0, 50.0, &[]));
            world.actors.insert("far".into(), vassalage_actor("far", 1000.0, 50.0, 50.0, 50.0, &[]));
            if dead_near { world.dead_actor_ids.insert("near".into()); }
            world
        };
        // N = 150 (the distant 1000 does not count), own 50 → 75
        assert_eq!(pressure_threat(&world_with(false), "city"), Some(75.0));
        assert_eq!(pressure_threat(&world_with(true), "city"), Some(0.0), "a dead neighbour is no threat");
        let pulled = |v2: bool, r: Option<f64>| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            scenario.economy_v2_pressure_pull = r;
            let mut world = world_with(false);
            pull_pressure_to_threat(&mut world, &scenario);
            world.actors["city"].get_metric("external_pressure")
        };
        // 10 + 0.1 × (75 − 10) = 16.5
        assert!((pulled(true, Some(0.1)) - 16.5).abs() < 1e-9);
        assert_eq!(pulled(false, Some(0.1)), 10.0, "v1 does not pull");
        assert_eq!(pulled(true, None), 10.0, "no r, no pull");
    }

    /// Economy v2 (Ц6 stage 2, items 2–3): a sea neighbour weighs half in N, and an army bound to
    /// us by vassalage (either way) is no threat. Both ways: on land and unbound, all count.
    #[test]
    fn economy_v2_threat_weighs_sea_half_and_skips_vassals() {
        let world_with = |sea: bool, bound: bool| {
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut city = vassalage_actor("city", 100.0, 10.0, 50.0, 50.0, &["land", "sea", "liege"]);
            if sea { city.neighbors.iter_mut().find(|n| n.id == "sea").unwrap().border_type = crate::core::BorderType::Sea; }
            world.actors.insert("city".into(), city);
            for id in ["land", "sea", "liege"] { world.actors.insert(id.into(), vassalage_actor(id, 100.0, 50.0, 50.0, 50.0, &[])); }
            if bound { world.vassalages.push(crate::core::Vassalage { vassal_id: "city".into(), overlord_id: "liege".into(), formed_tick: 0 }); }
            world
        };
        // N = 100 + 100 + 100, own 100 → 75
        assert_eq!(pressure_threat(&world_with(false, false), "city"), Some(75.0));
        // N = 100 + 50 + 100 → 100 × 250 / 350
        assert!((pressure_threat(&world_with(true, false), "city").unwrap() - 100.0 * 250.0 / 350.0).abs() < 1e-9, "sea weighs half");
        // N = 100 + 100 → 100 × 200 / 300
        assert!((pressure_threat(&world_with(false, true), "city").unwrap() - 100.0 * 200.0 / 300.0).abs() < 1e-9, "the overlord is no threat");
        // N = 100 + 50 → 60
        assert_eq!(pressure_threat(&world_with(true, true), "city"), Some(60.0));
    }

    /// Economy v2 (Ц6 stage 2, item 1): under the threat model combat and migration no longer write
    /// `external_pressure` — everything else they do stays. Both ways on the same seed, so the
    /// draws are the same and the difference is exactly the write: combat's 15–25 to the defender,
    /// migration's `(ep − 65) × 0.2` to the sink.
    #[test]
    fn economy_v2_war_enters_pressure_only_through_the_threat() {
        let run = |threat: bool, combat: bool, seed: u64| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = threat;
            scenario.economy_v2_pressure_pull = Some(0.05);
            scenario.military_conflict_probability = 1.0;
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            if combat {
                world.tick = 5;
                world.actors.insert("a".into(), vassalage_actor("a", 1000.0, 0.0, 50.0, 50.0, &["b"]));
                world.actors.insert("b".into(), vassalage_actor("b", 100.0, 10.0, 50.0, 50.0, &["a"]));
            } else {
                world.actors.insert("a".into(), vassalage_actor("a", 0.0, 90.0, 50.0, 20.0, &["b"]));
                world.actors.insert("b".into(), vassalage_actor("b", 0.0, 10.0, 50.0, 50.0, &["a"]));
            }
            let mut log = EventLog::new();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
            interactions::calculate_interactions(&mut world, &scenario, &mut log, &mut rng);
            (world.actors["b"].get_metric("external_pressure"), world.actors["b"].get_metric("military_size"), world.actors["b"].get_metric("population"))
        };
        let seed = (0..100).find(|s| run(false, true, *s).1 < 100.0).expect("a fight in 100 seeds");
        let (off, on) = (run(false, true, seed), run(true, true, seed));
        assert!((15.0..=25.0).contains(&(off.0 - on.0)), "combat's write: {} vs {}", off.0, on.0);
        assert_eq!(off.1, on.1, "the defender's losses stay");
        let (off, on) = (run(false, false, 1), run(true, false, 1));
        assert!((off.0 - on.0 - 5.0).abs() < 1e-9, "migration's write: {} vs {}", off.0, on.0);
        assert_eq!(off.2, on.2, "the migrants still arrive");
    }

    /// Economy v2 (Ц6): tags' `external_pressure` modifiers as a level — given once, taken back
    /// when the tag leaves. Both ways: with the switch off the tag adds every tick.
    #[test]
    fn economy_v2_gives_pressure_tags_as_a_level() {
        let run = |level: bool| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = true;
            scenario.economy_v2_pressure_tags_as_level = level;
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            let mut a = vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]);
            a.actor_tags.insert("frontier".into(), crate::core::ActorTag {
                metrics_modifier: HashMap::from([(crate::core::MetricName::new("external_pressure").unwrap(), 2)]),
                spreads_via: vec![],
            });
            world.actors.insert("city".into(), a);
            let mut seen = Vec::new();
            for t in 0..4 {
                if t == 2 { world.actors.get_mut("city").unwrap().actor_tags.remove("frontier"); }
                apply_actor_tags(&mut world, &scenario);
                seen.push(world.actors["city"].get_metric("external_pressure"));
            }
            seen
        };
        assert_eq!(run(true), vec![52.0, 52.0, 50.0, 50.0], "level: +2 once, back on removal");
        assert_eq!(run(false), vec![52.0, 54.0, 54.0, 54.0], "rate: every tick while carried");
    }

    /// B54: a milestone's `effects` apply on the tick it fires and only then. Dated tick 3,
    /// checked on ticks 0–6: the city's metrics move once, on tick 3, through
    /// `MetricRef::apply` (cohesion clamped at 0, treasury unbounded). Both ways: without
    /// the `effects` the same firing moves nothing.
    #[test]
    fn milestone_effects_apply_once_on_the_firing_tick() {
        let run = |with_effects: bool| {
            let mut scenario = empty_scenario();
            let mut m = dated_milestone("m", None, &["city"]);
            m.condition.condition_type = EventConditionType::Tick { tick: 3 };
            if with_effects {
                m.effects = HashMap::from([
                    (crate::core::MetricRef::literal("actor:city.cohesion"), -60.0),
                    (crate::core::MetricRef::literal("actor:city.treasury"), -200.0),
                ]);
            }
            scenario.milestone_events = vec![m];
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            world.actors.insert("city".into(), vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]));
            world.actors.get_mut("city").unwrap().set_metric("treasury", 100.0);
            let mut seen = Vec::new();
            for t in 0..7 {
                world.tick = t;
                let mut log = EventLog::new();
                check_milestone_events(&mut world, &scenario, &mut log, &mut rand_chacha::ChaCha8Rng::seed_from_u64(0));
                let c = &world.actors["city"];
                seen.push((c.get_metric("cohesion"), c.get_metric("treasury")));
            }
            seen
        };
        let with = run(true);
        assert_eq!(&with[..3], &[(50.0, 100.0); 3], "nothing moves before the firing tick");
        assert_eq!(&with[3..], &[(0.0, -100.0); 4], "applied once on tick 3: cohesion clamped at 0, treasury unbounded");
        assert_eq!(run(false), vec![(50.0, 100.0); 7], "without effects the firing moves nothing");
    }

    /// Economy v2 (Ц3): a milestone's `economy_v2_effects` apply once on its tick, only with v2
    /// on. Both ways: with v2 off the same firing leaves the army.
    #[test]
    fn milestone_v2_effects_apply_only_under_v2() {
        let run = |v2: bool| {
            let mut scenario = empty_scenario();
            scenario.features.economy_v2 = v2;
            let mut m = dated_milestone("m", None, &["city"]);
            m.condition.condition_type = EventConditionType::Tick { tick: 3 };
            m.economy_v2_effects = HashMap::from([(crate::core::MetricRef::literal("actor:city.military_size"), 60.0)]);
            scenario.milestone_events = vec![m];
            let mut world = WorldState::with_seed("test".into(), 1430, 0);
            world.actors.insert("city".into(), vassalage_actor("city", 50.0, 50.0, 50.0, 50.0, &[]));
            let mut seen = Vec::new();
            for t in 0..6 {
                world.tick = t;
                let mut log = EventLog::new();
                check_milestone_events(&mut world, &scenario, &mut log, &mut rand_chacha::ChaCha8Rng::seed_from_u64(0));
                seen.push(world.actors["city"].get_metric("military_size"));
            }
            seen
        };
        assert_eq!(run(true), vec![50.0, 50.0, 50.0, 110.0, 110.0, 110.0], "v2: +60 once on tick 3");
        assert_eq!(run(false), vec![50.0; 6], "v1: the firing moves nothing");
    }

    // ------------------------------------------------------------------
    // B44 stage 2: an absent actor in auto-deltas. It reads as 0.0; a ratio over a zero
    // denominator is a limit (±∞), 0 / 0 is skipped; a single condition keeps 0.0.
    // ------------------------------------------------------------------

    /// One auto-delta on `global:probe`, run through the engine's own phase. `byz` is
    /// present with `military_size` 50 unless `byz_alive` is false; `ott` is never
    /// present. Returns the authored delta that reached the global (noise 0).
    fn auto_delta_on_absent(
        byz_alive: bool,
        conditions: Vec<crate::core::DeltaCondition>,
        ratios: Vec<crate::core::DeltaConditionRatio>,
    ) -> f64 {
        let mut world = WorldState::with_seed("test".to_string(), 1430, 0);
        if byz_alive {
            world.actors.insert("byz".into(), vassalage_actor("byz", 50.0, 50.0, 50.0, 50.0, &[]));
        }
        let mut scenario = empty_scenario();
        scenario.auto_deltas = vec![crate::core::AutoDelta {
            metric: mr("global:probe", None),
            base: 0.0,
            conditions,
            ratio_conditions: ratios,
            noise: 0.0,
            actor_id: None,
        }];
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
        phase_auto_deltas(&mut world, &scenario, &mut rng);
        world.global_metrics.get("probe").copied().unwrap_or(0.0)
    }

    fn ratio(operator: ComparisonOperator, delta: f64) -> crate::core::DeltaConditionRatio {
        crate::core::DeltaConditionRatio {
            metric_a: mr("actor:byz.military_size", None),
            metric_b: mr("actor:ott.military_size", None),
            ratio: 0.25,
            operator,
            delta,
        }
    }

    /// A living numerator over a dead (or zero) denominator: the utmost superiority.
    /// `>` holds, `<` does not. Under the old skip both were off (0 reached the global).
    #[test]
    fn ratio_over_an_absent_denominator_is_infinite() {
        assert_eq!(auto_delta_on_absent(true, vec![], vec![ratio(ComparisonOperator::Greater, 1.0)]), 1.0, "`>` over a dead opponent holds");
        assert_eq!(auto_delta_on_absent(true, vec![], vec![ratio(ComparisonOperator::Less, 10.0)]), 0.0, "`<` over a dead opponent does not");
    }

    /// 0 / 0 is undefined and skipped, whichever the operator.
    #[test]
    fn ratio_zero_over_zero_is_skipped() {
        assert_eq!(auto_delta_on_absent(false, vec![], vec![ratio(ComparisonOperator::Greater, 1.0)]), 0.0, "0 / 0 with `>` is skipped");
        assert_eq!(auto_delta_on_absent(false, vec![], vec![ratio(ComparisonOperator::Less, 10.0)]), 0.0, "0 / 0 with `<` is skipped");
    }

    /// A single condition on an absent actor reads 0.0: `>` false, `<` true — declared.
    #[test]
    fn single_condition_on_an_absent_actor_reads_zero() {
        let cond = |operator, delta| crate::core::DeltaCondition { metric: mr("actor:ott.military_size", None), operator, value: 10.0, delta };
        assert_eq!(auto_delta_on_absent(true, vec![cond(ComparisonOperator::Greater, 1.0)], vec![]), 0.0, "`>` on an absent actor is false");
        assert_eq!(auto_delta_on_absent(true, vec![cond(ComparisonOperator::Less, 10.0)], vec![]), 10.0, "`<` on an absent actor is true");
    }

    #[test]
    fn eval_metric_condition_resolves_all_scopes() {
        let mut world = WorldState::with_seed("test".to_string(), 1430, 0);
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
        let mut world = WorldState::with_seed("test".to_string(), 1430, 0);
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
        let mut world = WorldState::with_seed("test".to_string(), 1477, 0);
        // Weak actor sitting inside the danger band, strong healthy neighbour.
        world.actors.insert("small".into(), vassalage_actor("small", 10.0, 78.0, 18.0, 22.0, &["big"]));
        world.actors.insert("big".into(), vassalage_actor("big", 100.0, 30.0, 60.0, 60.0, &["small"]));
        let mut log = EventLog::new();

        // Needs 3 consecutive ticks in band before forming.
        interactions::check_vassalage(&mut world, &mut log, true);
        assert!(world.vassalages.is_empty(), "must not form before 3 ticks");
        interactions::check_vassalage(&mut world, &mut log, true);
        assert!(world.vassalages.is_empty(), "must not form before 3 ticks");
        interactions::check_vassalage(&mut world, &mut log, true);

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
        interactions::calculate_vassalage_interaction(&mut world, &mut log, &mut rng, false);
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
        let mut world = WorldState::with_seed("test".to_string(), 1477, 0);
        world.actors.insert("small".into(), vassalage_actor("small", 10.0, 78.0, 18.0, 22.0, &["free", "v"]));
        world.actors.insert("free".into(), vassalage_actor("free", 50.0, 30.0, 60.0, 60.0, &["small"]));
        world.actors.insert("v".into(), vassalage_actor("v", 100.0, 30.0, 60.0, 60.0, &["small", "lord"]));
        world.actors.insert("lord".into(), vassalage_actor("lord", 200.0, 30.0, 60.0, 60.0, &["v"]));
        // "v" is already a vassal of a healthy "lord", so it stays bound.
        world.vassalages.push(crate::core::Vassalage { vassal_id: "v".into(), overlord_id: "lord".into(), formed_tick: 0 });
        let mut log = EventLog::new();

        for _ in 0..3 {
            interactions::check_vassalage(&mut world, &mut log, true);
        }

        // "small" submits — but to "free", not to the stronger vassal "v".
        assert_eq!(world.vassalages.len(), 2);
        let small_v = world.vassalages.iter().find(|v| v.vassal_id == "small").expect("small should be a vassal");
        assert_eq!(small_v.overlord_id, "free", "must not pick a vassal as overlord");
        assert_eq!(world.actors.get("free").unwrap().get_metric("expansion_count"), 1.0);
    }

    #[test]
    fn test_vassalage_revolt_conditions() {
        let mut world = WorldState::with_seed("test".to_string(), 1477, 0);
        world.actors.insert("small".into(), vassalage_actor("small", 10.0, 30.0, 60.0, 60.0, &["big"]));
        world.actors.insert("big".into(), vassalage_actor("big", 20.0, 30.0, 60.0, 60.0, &["small"]));
        world.vassalages.push(crate::core::Vassalage { vassal_id: "small".into(), overlord_id: "big".into(), formed_tick: 0 });
        let mut log = EventLog::new();

        // Stable: vassal weak (10 < 80% of 20) and overlord healthy.
        interactions::check_vassalage(&mut world, &mut log, true);
        assert_eq!(world.vassalages.len(), 1, "must stay bound while weak");

        // Revolt path 1: vassal's military catches up (>= 80% of overlord).
        world.actors.get_mut("small").unwrap().set_metric("military_size", 16.0); // 16 >= 20*0.8
        interactions::check_vassalage(&mut world, &mut log, true);
        assert!(world.vassalages.is_empty(), "vassal must revolt once strong enough");

        // Revolt path 2: overlord itself enters the FULL vassalage band (all three
        // metrics together), not merely one slipped metric.
        world.actors.get_mut("small").unwrap().set_metric("military_size", 5.0);
        world.vassalages.push(crate::core::Vassalage { vassal_id: "small".into(), overlord_id: "big".into(), formed_tick: 0 });
        // Only external_pressure in band — legitimacy/cohesion still healthy: must NOT revolt.
        let big = world.actors.get_mut("big").unwrap();
        big.set_metric("external_pressure", 75.0);
        interactions::check_vassalage(&mut world, &mut log, true);
        assert_eq!(world.vassalages.len(), 1, "single slipped metric must not free the vassal");
        // Now drive legitimacy and cohesion into band too → full band → revolt.
        let big = world.actors.get_mut("big").unwrap();
        big.set_metric("legitimacy", 18.0);
        big.set_metric("cohesion", 22.0);
        interactions::check_vassalage(&mut world, &mut log, true);
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
        let mut world = WorldState::with_seed("test".into(), 375, 0);
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
        let mut world = WorldState::with_seed("test".into(), 375, 0);
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
        let mut world = WorldState::with_seed("test".into(), 375, 0);
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
        let mut world = WorldState::with_seed("test".into(), 375, 0);
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
        let mut world = WorldState::with_seed("test".into(), 375, 0);
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
        let mut world = WorldState::with_seed("test".into(), 375, 0);
        world.actors.insert("parent".into(), parent);
        world.actors.insert("huns".into(), huns);
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        assert_eq!(neighbor_ids(&world, "huns"), ["parent"], "split: replacement undefined, list left as authored");
        assert_eq!(neighbor_ids(&world, "east"), ["huns"], "empty heir list still inherits");
        assert_eq!(neighbor_ids(&world, "west"), ["huns"]);
    }

    /// Absorption by a living heir: the border passes to it (B24). This test pinned the
    /// opposite until then — «absorption does not touch any list» — as the scope line
    /// of D₄ (docs/investigation_successor_edges.md §8 left the question open).
    #[test]
    fn absorption_passes_the_parents_edges_to_the_heir() {
        let heir = vassalage_actor("heir", 50.0, 30.0, 60.0, 60.0, &[]);
        let mut parent = doomed_actor("parent", &["heir"]);
        parent.neighbors = vassalage_actor("parent", 0.0, 0.0, 0.0, 0.0, &["huns"]).neighbors;
        let huns = vassalage_actor("huns", 120.0, 5.0, 60.0, 72.0, &["parent"]);
        let mut scenario = empty_scenario();
        scenario.actors = vec![parent.clone(), heir.clone(), huns.clone()];
        let mut world = WorldState::with_seed("test".into(), 375, 0);
        world.actors.insert("parent".into(), parent);
        world.actors.insert("heir".into(), heir);
        world.actors.insert("huns".into(), huns);
        let mut log = EventLog::new();

        kill(&mut world, &scenario, &mut log);

        assert_eq!(neighbor_ids(&world, "heir"), ["huns"], "the absorber gains the parent's edge");
        assert_eq!(neighbor_ids(&world, "huns"), ["heir"], "the neighbour names the absorber instead of the dead parent");
    }

    // ------------------------------------------------------------------
    // Mobilisation capacity and recovery (docs/investigation_military_source.md)
    // ------------------------------------------------------------------

    #[test]
    fn army_recovers_toward_capacity_but_never_above_it() {
        use crate::engine::interactions::{military_capacity, MILITARY_RECOVERY_RATE};
        let mut world = WorldState::with_seed("test".into(), 375, 0);
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

        interactions::apply_military_recovery(&mut world, false);

        let expected = 2.0 + (capacity - 2.0) * MILITARY_RECOVERY_RATE;
        assert!((world.actors["spent"].get_metric("military_size") - expected).abs() < 1e-9);
        assert_eq!(world.actors["over"].get_metric("military_size"), 350.0, "above capacity is left alone");
        assert_eq!(world.actors["empty"].get_metric("military_size"), 0.0, "no population, no recovery");
    }

    #[test]
    fn recovery_converges_to_capacity_and_stops() {
        use crate::engine::interactions::military_capacity;
        let mut world = WorldState::with_seed("test".into(), 375, 0);
        let mut a = vassalage_actor("a", 0.0, 30.0, 60.0, 60.0, &[]);
        a.set_metric("population", 250.0);
        let capacity = military_capacity(&a);
        world.actors.insert("a".into(), a);
        for _ in 0..400 {
            interactions::apply_military_recovery(&mut world, false);
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
            splits_actor: Some("parent".into()),
            after: None,
            group: None,
            requires_alive: vec![],
            effects: Default::default(),
            begins_conquest: None,
            economy_v2_effects: Default::default(),
        }];
        let mut world = WorldState::with_seed("test".into(), 375, 0);
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
                region_rank: crate::core::RegionRank::C,
                religion: crate::core::Religion::Orthodox,
                culture: crate::core::Culture::Slavic,
            }),
            splits_actor: None,
            after: None,
            group: None,
            requires_alive: vec![],
            effects: Default::default(),
            begins_conquest: None,
            economy_v2_effects: Default::default(),
        }];
        // Milan already names France on its own terms — that entry must survive as is.
        let mut milan_lists_france = vassalage_actor("milan", 50.0, 30.0, 60.0, 60.0, &["savoy"]);
        milan_lists_france.neighbors.push(Neighbor { id: "france".into(), distance: 2, border_type: BorderType::Land });
        let mut world = WorldState::with_seed("test".into(), 1477, 0);
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
        let mut world = WorldState::with_seed("test".into(), 375, 0);
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

