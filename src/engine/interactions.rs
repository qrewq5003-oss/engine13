use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::core::{ActorTag, BorderType, ConditionActor, Event, EventType, InteractionRule, TagSpreadType, Vassalage, WorldState, Scenario, Religion, Culture};
use crate::engine::EventLog;

/// Cultural affinity between two cultures (0.0 = hostile, 1.0 = identical)
pub fn cultural_affinity(a_culture: &Culture, b_culture: &Culture) -> f64 {
    use Culture::*;
    match (a_culture, b_culture) {
        (x, y) if x == y => 0.2,  // Same culture has low friction

        // Latin
        (Latin, Greek) | (Greek, Latin)       => 0.4,
        (Latin, Germanic) | (Germanic, Latin) => 0.3,
        (Latin, Slavic) | (Slavic, Latin)     => 0.7,
        (Latin, Arabic) | (Arabic, Latin)     => 0.8,
        (Latin, Turkic) | (Turkic, Latin)     => 1.0,
        (Latin, Persian) | (Persian, Latin)   => 0.9,
        (Latin, Indian) | (Indian, Latin)     => 1.0,
        (Latin, EastAsian) | (EastAsian, Latin) => 1.0,

        // Greek
        (Greek, Slavic) | (Slavic, Greek)     => 0.3,
        (Greek, Germanic) | (Germanic, Greek) => 0.8,
        (Greek, Arabic) | (Arabic, Greek)     => 0.7,
        (Greek, Turkic) | (Turkic, Greek)     => 0.9,
        (Greek, Persian) | (Persian, Greek)   => 0.8,
        (Greek, Indian) | (Indian, Greek)     => 1.0,
        (Greek, EastAsian) | (EastAsian, Greek) => 1.0,

        // Slavic
        (Slavic, Germanic) | (Germanic, Slavic) => 0.7,
        (Slavic, Arabic) | (Arabic, Slavic)     => 0.9,
        (Slavic, Turkic) | (Turkic, Slavic)     => 0.8,
        (Slavic, Persian) | (Persian, Slavic)   => 0.9,
        (Slavic, Indian) | (Indian, Slavic)     => 1.0,
        (Slavic, EastAsian) | (EastAsian, Slavic) => 1.0,

        // Germanic
        (Germanic, Arabic) | (Arabic, Germanic) => 0.9,
        (Germanic, Turkic) | (Turkic, Germanic) => 1.0,
        (Germanic, Persian) | (Persian, Germanic) => 0.9,
        (Germanic, Indian) | (Indian, Germanic) => 1.0,
        (Germanic, EastAsian) | (EastAsian, Germanic) => 1.0,

        // Arabic
        (Arabic, Turkic) | (Turkic, Arabic)   => 0.4,
        (Arabic, Persian) | (Persian, Arabic) => 0.4,
        (Arabic, Indian) | (Indian, Arabic)   => 0.7,
        (Arabic, EastAsian) | (EastAsian, Arabic) => 0.9,

        // Turkic
        (Turkic, Persian) | (Persian, Turkic) => 0.5,
        (Turkic, Indian) | (Indian, Turkic)   => 0.8,
        (Turkic, EastAsian) | (EastAsian, Turkic) => 0.7,

        // Persian
        (Persian, Indian) | (Indian, Persian) => 0.6,
        (Persian, EastAsian) | (EastAsian, Persian) => 0.9,

        // Indian
        (Indian, EastAsian) | (EastAsian, Indian) => 0.7,

        _ => 0.8,
    }
}

/// Religious modifier for interactions (-0.2 = harmonious, +0.3 = hostile)
pub fn religious_modifier(a_religion: &Religion, b_religion: &Religion) -> f64 {
    use Religion::*;
    match (a_religion, b_religion) {
        (x, y) if x == y                          => -0.2,  // Same religion
        (Catholic, Orthodox) | (Orthodox, Catholic) => 0.2,
        (Catholic, Muslim) | (Muslim, Catholic)     => 0.3,
        (Orthodox, Muslim) | (Muslim, Orthodox)     => 0.3,
        (Buddhist, Muslim) | (Muslim, Buddhist)     => 0.2,
        (Hindu, Muslim) | (Muslim, Hindu)           => 0.3,
        _                                           => 0.0,
    }
}

/// Overall affinity between two actors (0.0 = hostile, 1.0 = allied)
pub fn affinity(a: &crate::core::Actor, b: &crate::core::Actor) -> f64 {
    let base = cultural_affinity(&a.culture, &b.culture);
    let modifier = religious_modifier(&a.religion, &b.religion);
    (base + modifier).clamp(0.0, 1.0)
}

/// Below this `military_size` a defender has nothing left to fight with, and a
/// "battle" against it is a phantom: the attacker still pays the full 5–15% loss
/// for storming an empty field.
///
/// Combat losses are multiplicative (`mil * (1 - loss)`), so a beaten army never
/// reaches exactly `0.0` — it decays asymptotically. An `== 0.0` test would never
/// fire; the cut-off has to be an epsilon. This is the same threshold
/// `src/bin/combat_probe.rs` uses to classify "fight against an already-empty
/// army", so the set of fights this guard removes is exactly the set the
/// investigation measured (81–95% of all fights).
pub const MIN_DEFENSIBLE_MILITARY: f64 = 0.01;

/// Effective military strength accounting for force projection through neighbors
pub fn effective_military(actor: &crate::core::Actor, neighbors: Vec<&crate::core::Actor>) -> f64 {
    let active_neighbors = neighbors.len().max(1);
    
    // Average affinity with all neighbors
    let avg_affinity: f64 = neighbors.iter()
        .map(|n| affinity(actor, n))
        .sum::<f64>() / active_neighbors as f64;

    // More foreign neighbors = more military stretched
    let divisor = (active_neighbors as f64 * avg_affinity).max(1.0);
    actor.get_metric("military_size") / divisor
}

/// Type of interaction between actors
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum InteractionType {
    Military,
    Trade,
    Diplomatic,
    Migration,
    Vassalage,
    Cultural,
}

#[allow(dead_code)]
/// Interaction between two actors
pub struct Interaction {
    pub actor_a: String,
    pub actor_b: String,
    pub interaction_type: InteractionType,
    pub intensity: f64,
}

/// Apply a data-driven interaction rule to an actor pair
/// Order: distance → border → cooldown → conditions → effects
// Interaction rules are driven by scenario data; the parameter list mirrors the
// full interaction context and is intentionally wide. Splitting it into a struct
// would obscure the call sites without changing behavior.
#[allow(clippy::too_many_arguments)]
pub fn apply_interaction_rule(
    world: &mut WorldState,
    source_id: &str,
    target_id: &str,
    distance: u32,
    border_type: &BorderType,
    rule: &InteractionRule,
    current_tick: u32,
    _current_year: i32,
    _event_log: &mut EventLog,
) {
    // 1. Distance check
    if distance > rule.max_distance {
        return;
    }

    // 2. Border type — unknown value = panic (not silent pass-through)
    if let Some(ref bt) = rule.border_type {
        let matches = match bt.as_str() {
            "land" => *border_type == BorderType::Land,
            "sea"  => *border_type == BorderType::Sea,
            other  => panic!("InteractionRule '{}': invalid border_type '{}'", rule.id, other),
        };
        if !matches {
            return;
        }
    }

    // 3. Cooldown — symmetric key
    let cooldown_key = {
        let (a, b) = if source_id < target_id {
            (source_id, target_id)
        } else {
            (target_id, source_id)
        };
        format!("rule_{}_{}_{}", rule.id, a, b)
    };
    if rule.cooldown_ticks > 0 {
        if let Some(&last_tick) = world.interaction_cooldowns.get(&cooldown_key) {
            if current_tick.saturating_sub(last_tick) < rule.cooldown_ticks {
                return;
            }
        }
    }

    // 4. Conditions — sequential check, order from file
    for cond in &rule.conditions {
        let actor_id = match cond.actor {
            ConditionActor::Source => source_id,
            ConditionActor::Target => target_id,
        };
        let actor = match world.actors.get(actor_id) {
            Some(a) => a,
            None => return,
        };
        let val = actor.get_metric(&cond.metric);
        let passes = cond.operator.evaluate(val, cond.value);
        if !passes {
            return;
        }
    }

    // 5. Effects — flat delta, sequential apply
    let mut total_abs_delta: f64 = 0.0;
    for effect in &rule.effects {
        let actor_id = match effect.actor {
            ConditionActor::Source => source_id.to_string(),
            ConditionActor::Target => target_id.to_string(),
        };
        if let Some(actor) = world.actors.get_mut(&actor_id) {
            actor.add_metric(&effect.metric, effect.delta);
            total_abs_delta += effect.delta.abs();
        }
    }

    // 6. Cooldown set
    if rule.cooldown_ticks > 0 {
        world.interaction_cooldowns.insert(cooldown_key, current_tick);
    }

    // 7. Event logging
    if let Some(ref _event_type_str) = rule.event_type {
        if total_abs_delta >= rule.event_threshold {
            // TODO: map event_type_str → EventType in PR H when real rules exist
        }
    }
}

/// Calculate all interactions between neighboring actors
pub fn calculate_interactions(
    world: &mut WorldState,
    scenario: &Scenario,
    event_log: &mut EventLog,
    rng: &mut ChaCha8Rng,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    // Get all actor pairs that are neighbors
    let actor_pairs = get_neighbor_pairs(world);

    for (actor_a_id, actor_b_id, distance, border_type) in actor_pairs {
        // Skip if either actor is dead
        if !world.actors.contains_key(&actor_a_id) || !world.actors.contains_key(&actor_b_id) {
            continue;
        }

        // Clone border_type for reuse across multiple interaction types
        let bt = border_type.clone();

        // Calculate all six interaction types sequentially
        calculate_military_interaction(
            world, &actor_a_id, &actor_b_id, distance, bt.clone(),
            current_tick, current_year, event_log, rng, scenario,
        );

        calculate_trade_interaction(
            world, &actor_a_id, &actor_b_id, distance, bt.clone(),
            current_tick, current_year, event_log, rng,
        );

        calculate_diplomatic_interaction(
            world, &actor_a_id, &actor_b_id, distance,
            current_tick, current_year, event_log, rng,
        );

        calculate_migration_interaction(
            world, &actor_a_id, &actor_b_id, distance, bt.clone(),
            current_tick, current_year, event_log, rng,
        );

        // Data-driven rules (empty for Rome/Constantinople by default)
        // Order in TOML = order of application = part of simulation logic
        for rule in &scenario.interaction_rules {
            apply_interaction_rule(
                world, &actor_a_id, &actor_b_id, distance, &bt,
                rule, current_tick, current_year, event_log,
            );
        }

        calculate_cultural_interaction(
            world, &actor_a_id, &actor_b_id, distance,
            current_tick, current_year, event_log, rng,
        );

        spread_actor_tags(
            world, scenario, &actor_a_id, &actor_b_id, distance, &bt,
            current_tick, rng, event_log,
        );
    }
}

/// Get all neighbor pairs from actor neighbor lists
fn get_neighbor_pairs(world: &WorldState) -> Vec<(String, String, u32, crate::core::BorderType)> {
    let mut pairs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (actor_id, actor) in &world.actors {
        for neighbor in &actor.neighbors {
            if world.actors.contains_key(&neighbor.id) {
                // Create sorted pair key to avoid duplicates
                let pair_key = if actor_id < &neighbor.id {
                    format!("{}-{}", actor_id, neighbor.id)
                } else {
                    format!("{}-{}", neighbor.id, actor_id)
                };

                if !seen.contains(&pair_key) {
                    seen.insert(pair_key.clone());
                    let (a, b) = if actor_id < &neighbor.id {
                        (actor_id.clone(), neighbor.id.clone())
                    } else {
                        (neighbor.id.clone(), actor_id.clone())
                    };
                    pairs.push((a, b, neighbor.distance, neighbor.border_type.clone()));
                }
            }
        }
    }

    // Sort by (actor_a_id, actor_b_id) for a deterministic iteration order.
    // `world.actors` is a HashMap, so the collection order above is randomized
    // per process; without this sort the order in which pairs consume `rng`
    // (military rolls, etc.) varies run-to-run, breaking fixed-seed reproducibility.
    pairs.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));

    pairs
}

/// Military interaction — conflict between neighbors
#[allow(clippy::too_many_arguments)]
fn calculate_military_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    border_type: crate::core::BorderType,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    rng: &mut ChaCha8Rng,
    scenario: &Scenario,
) {
    // No military conflicts in first 3 ticks (stabilization period)
    if world.tick < 3 {
        return;
    }

    let actor_a = match world.actors.get(actor_a_id) {
        Some(a) => a,
        None => return,
    };
    let actor_b = match world.actors.get(actor_b_id) {
        Some(a) => a,
        None => return,
    };

    // Condition: both alive, distance == 1
    if distance != 1 {
        return;
    }

    // Get all neighbors for force projection calculation
    let actor_a_neighbors: Vec<&crate::core::Actor> = actor_a.neighbors.iter()
        .filter_map(|n| world.actors.get(&n.id))
        .collect();
    let actor_b_neighbors: Vec<&crate::core::Actor> = actor_b.neighbors.iter()
        .filter_map(|n| world.actors.get(&n.id))
        .collect();

    // Calculate effective military with force projection
    let eff_mil_a = effective_military(actor_a, actor_a_neighbors);
    let eff_mil_b = effective_military(actor_b, actor_b_neighbors);

    // Determine stronger (attacker) and weaker (defender) actor by effective military
    let (attacker_id, defender_id, _attacker_eff, _defender_eff) = if eff_mil_a >= eff_mil_b {
        (actor_a_id.to_string(), actor_b_id.to_string(), eff_mil_a, eff_mil_b)
    } else {
        (actor_b_id.to_string(), actor_a_id.to_string(), eff_mil_b, eff_mil_a)
    };

    // Termination condition: a side with no army left is not a belligerent.
    // Without this the fight has no end — `military_mod` (+0.15) is *always* true
    // against an empty army, so the attacker keeps rolling every time the cooldown
    // expires and keeps paying 5–15% per fight, destroying itself on a battlefield
    // the enemy left long ago. Checked on the defender: an actor with no military
    // has an `effective_military` of 0 (the divisor is `.max(1.0)`, never zero), so
    // it is always the one assigned the defender role above.
    let defender_military = world
        .actors
        .get(&defender_id)
        .map(|a| a.get_metric("military_size"))
        .unwrap_or(0.0);
    if defender_military < MIN_DEFENSIBLE_MILITARY {
        return;
    }

    // Check cooldown
    let cooldown_key = format!("{}_vs_{}", attacker_id, defender_id);
    if let Some(&last_tick) = world.interaction_cooldowns.get(&cooldown_key) {
        if current_tick - last_tick < 5 {
            return;
        }
    }

    // Get base probability from scenario based on connection type
    let base_prob = match border_type {
        crate::core::BorderType::Land => scenario.military_conflict_probability,
        crate::core::BorderType::Sea => scenario.naval_conflict_probability,
    };

    // Calculate modifiers
    let attacker = match world.actors.get(&attacker_id) {
        Some(a) => a,
        None => return,
    };
    let defender = match world.actors.get(&defender_id) {
        Some(a) => a,
        None => return,
    };

    let pressure_mod = (attacker.get_metric("external_pressure") / 100.0) * 0.2;
    let military_mod = if attacker.get_metric("military_size") > defender.get_metric("military_size") * 1.5 {
        0.15
    } else {
        0.0
    };

    // Get affinity between actors
    let affinity_mod = affinity(attacker, defender);
    
    // Calculate final probability (capped at 0.8)
    let final_prob = (base_prob + pressure_mod + military_mod * (1.0 - affinity_mod * 0.5)).min(0.8);

    // Roll for interaction
    let roll: f64 = rng.gen();
    if roll > final_prob {
        return;
    }

    // Apply losses
    let attacker_loss = 0.05 + rng.gen::<f64>() * 0.10; // 5-15%
    let defender_loss = 0.15 + rng.gen::<f64>() * 0.15;   // 15-30%
    let cohesion_loss = 10.0 + rng.gen::<f64>() * 10.0;  // 10-20
    let pressure_gain = 15.0 + rng.gen::<f64>() * 10.0;  // 15-25

    if let Some(attacker_actor) = world.actors.get_mut(&attacker_id) {
        let mil = attacker_actor.get_metric("military_size");
        attacker_actor.set_metric("military_size", mil * (1.0 - attacker_loss));
    }

    if let Some(defender_actor) = world.actors.get_mut(&defender_id) {
        let mil = defender_actor.get_metric("military_size");
        defender_actor.set_metric("military_size", mil * (1.0 - defender_loss));
        let coh = defender_actor.get_metric("cohesion");
        defender_actor.set_metric("cohesion", (coh - cohesion_loss).max(0.0));
        defender_actor.add_metric("external_pressure", pressure_gain);
    }

    // Set cooldown
    world.interaction_cooldowns.insert(cooldown_key, current_tick);

    // Record event
    let intensity = final_prob;
    if should_record_event(&InteractionType::Military, intensity) {
        let event = Event::new(
            format!("military_conflict_{}_{}", attacker_id, defender_id),
            current_tick,
            current_year,
            attacker_id.clone(),
            EventType::War,
            true,
            format!("Военный конфликт между {} и {}", attacker_id, defender_id),
        );
        event_log.add(event);
    }
}

/// Trade interaction — economic exchange
#[allow(clippy::too_many_arguments)]
fn calculate_trade_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    border_type: crate::core::BorderType,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    _rng: &mut ChaCha8Rng,
) {
    let actor_a = match world.actors.get(actor_a_id) {
        Some(a) => a,
        None => return,
    };
    let actor_b = match world.actors.get(actor_b_id) {
        Some(a) => a,
        None => return,
    };

    // Condition: external_pressure_avg < 60, economic_output_both > 20
    let external_pressure_avg = (actor_a.get_metric("external_pressure") + actor_b.get_metric("external_pressure")) / 2.0;
    if external_pressure_avg >= 60.0 {
        return;
    }
    if actor_a.get_metric("economic_output") < 20.0 || actor_b.get_metric("economic_output") < 20.0 {
        return;
    }

    // Check 3-tick cooldown
    let trade_key = format!("trade_{}_{}", actor_a_id, actor_b_id);
    if let Some(&last_tick) = world.interaction_cooldowns.get(&trade_key) {
        if current_tick - last_tick < 3 {
            return;
        }
    }

    // Calculate bonus
    let distance_modifier = match distance {
        1 => 1.0,
        2 => 0.7,
        _ => 0.4,
    };
    let sea_bonus = if border_type == crate::core::BorderType::Sea { 1.5 } else { 1.0 };
    let bonus = (actor_a.get_metric("economic_output") + actor_b.get_metric("economic_output")) * 0.002 * distance_modifier * sea_bonus;

    // Apply treasury gain
    if let Some(actor) = world.actors.get_mut(actor_a_id) {
        actor.add_metric("treasury", bonus);
    }
    if let Some(actor) = world.actors.get_mut(actor_b_id) {
        actor.add_metric("treasury", bonus);
    }

    // Set cooldown
    world.interaction_cooldowns.insert(trade_key, current_tick);

    // Record event if significant
    if should_record_event(&InteractionType::Trade, bonus) {
        let event = Event::new(
            format!("trade_{}_{}", actor_a_id, actor_b_id),
            current_tick,
            current_year,
            actor_a_id.to_string(),
            EventType::Trade,
            false,
            format!("Торговое взаимодействие между {} и {}", actor_a_id, actor_b_id),
        );
        event_log.add(event);
    }
}

/// Diplomatic interaction — legitimacy influence
#[allow(clippy::too_many_arguments)]
fn calculate_diplomatic_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    _rng: &mut ChaCha8Rng,
) {
    let actor_a = match world.actors.get(actor_a_id) {
        Some(a) => a,
        None => return,
    };
    let actor_b = match world.actors.get(actor_b_id) {
        Some(a) => a,
        None => return,
    };

    // Condition: legitimacy_diff > 15
    let legitimacy_diff = (actor_a.get_metric("legitimacy") - actor_b.get_metric("legitimacy")).abs();
    if legitimacy_diff <= 15.0 {
        return;
    }

    // Stronger actor influences weaker
    let (influencer_id, influenced_id) = if actor_a.get_metric("legitimacy") > actor_b.get_metric("legitimacy") {
        (actor_a_id.to_string(), actor_b_id.to_string())
    } else {
        (actor_b_id.to_string(), actor_a_id.to_string())
    };

    let influence = (legitimacy_diff * 0.1) / distance as f64;

    if let Some(influenced) = world.actors.get_mut(&influenced_id) {
        let coh = influenced.get_metric("cohesion");
        influenced.set_metric("cohesion", (coh + influence).min(100.0));
    }

    // Record event if significant
    if should_record_event(&InteractionType::Diplomatic, influence) {
        let event = Event::new(
            format!("diplomatic_{}_{}", influencer_id, influenced_id),
            current_tick,
            current_year,
            influencer_id.clone(),
            EventType::Diplomatic,
            false,
            format!("{} оказывает влияние на {}", influencer_id, influenced_id),
        );
        event_log.add(event);
    }
}

/// Migration interaction — population pressure
#[allow(clippy::too_many_arguments)]
fn calculate_migration_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    border_type: crate::core::BorderType,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    _rng: &mut ChaCha8Rng,
) {
    // Condition: border Land, external_pressure > 65, cohesion < 40
    if border_type != crate::core::BorderType::Land {
        return;
    }

    // Find the pressuring actor (high external_pressure, low cohesion)
    let pressuring_id = {
        let actor_a = match world.actors.get(actor_a_id) {
            Some(a) => a,
            None => return,
        };
        let actor_b = match world.actors.get(actor_b_id) {
            Some(a) => a,
            None => return,
        };

        if actor_a.get_metric("external_pressure") > 65.0 && actor_a.get_metric("cohesion") < 40.0 {
            Some(actor_a_id.to_string())
        } else if actor_b.get_metric("external_pressure") > 65.0 && actor_b.get_metric("cohesion") < 40.0 {
            Some(actor_b_id.to_string())
        } else {
            None
        }
    };

    let pressuring_id = match pressuring_id {
        Some(id) => id,
        None => return,
    };

    let neighbor_id = if pressuring_id == actor_a_id {
        actor_b_id.to_string()
    } else {
        actor_a_id.to_string()
    };

    // Apply migration effects
    let pressuring_pop = match world.actors.get(&pressuring_id) {
        Some(p) => p.get_metric("population"),
        None => return,
    };

    let pressuring_pressure = match world.actors.get(&pressuring_id) {
        Some(p) => p.get_metric("external_pressure"),
        None => return,
    };

    let pressure_transfer = (pressuring_pressure - 65.0) * 0.2 / distance as f64;
    let pop_loss_ratio = 0.01;
    let pop_gain_ratio = 0.005;

    if let Some(pressuring) = world.actors.get_mut(&pressuring_id) {
        let pop = pressuring.get_metric("population");
        pressuring.set_metric("population", pop * (1.0 - pop_loss_ratio));
    }

    if let Some(neighbor) = world.actors.get_mut(&neighbor_id) {
        neighbor.add_metric("external_pressure", pressure_transfer);
        let pop_gain = pressuring_pop * pop_gain_ratio;
        neighbor.add_metric("population", pop_gain);
    }

    // Record event if significant
    if should_record_event(&InteractionType::Migration, pressure_transfer) {
        let event = Event::new(
            format!("migration_{}_{}", pressuring_id, neighbor_id),
            current_tick,
            current_year,
            pressuring_id.clone(),
            EventType::Migration,
            false,
            format!("Миграция из {} в {}", pressuring_id, neighbor_id),
        );
        event_log.add(event);
    }
}

/// Cultural interaction — shared tags influence
#[allow(clippy::too_many_arguments)]
fn calculate_cultural_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    _rng: &mut ChaCha8Rng,
) {
    let actor_a = match world.actors.get(actor_a_id) {
        Some(a) => a,
        None => return,
    };
    let actor_b = match world.actors.get(actor_b_id) {
        Some(a) => a,
        None => return,
    };

    // Calculate shared tags
    let shared_tags: Vec<&String> = actor_a.tags.iter()
        .filter(|t| actor_b.tags.contains(t))
        .collect();

    let shared_count = shared_tags.len() as f64;
    let bonus = shared_count * 0.01 / distance as f64;
    let malus = if shared_count == 0.0 && distance == 1 { 0.05 } else { 0.0 };

    // Apply cohesion changes, capped at ±0.05 per interaction
    let cohesion_change = (bonus - malus).clamp(-0.05, 0.05);

    if let Some(actor) = world.actors.get_mut(actor_a_id) {
        let coh = actor.get_metric("cohesion");
        actor.set_metric("cohesion", (coh + cohesion_change).clamp(0.0, 100.0));
    }
    if let Some(actor) = world.actors.get_mut(actor_b_id) {
        let coh = actor.get_metric("cohesion");
        actor.set_metric("cohesion", (coh + cohesion_change).clamp(0.0, 100.0));
    }

    // Record event rarely — only if cohesion changed > 3.0
    if should_record_event(&InteractionType::Cultural, cohesion_change.abs()) {
        let event = Event::new(
            format!("cultural_{}_{}", actor_a_id, actor_b_id),
            current_tick,
            current_year,
            actor_a_id.to_string(),
            EventType::Cultural,
            false,
            format!("Культурное взаимодействие между {} и {}", actor_a_id, actor_b_id),
        );
        event_log.add(event);
    }

    // Cultural displacement: stronger culture pressures weaker neighbor
    if distance <= 2 {
        let actor_a = match world.actors.get(actor_a_id) {
            Some(a) => a,
            None => return,
        };
        let actor_b = match world.actors.get(actor_b_id) {
            Some(a) => a,
            None => return,
        };
        let a_power = cultural_power(actor_a);
        let b_power = cultural_power(actor_b);

        if a_power > b_power * 1.5 {
            let delta = a_power - b_power;
            apply_cultural_pressure(world, actor_a_id, actor_b_id, delta, current_tick, current_year, event_log);
        } else if b_power > a_power * 1.5 {
            let delta = b_power - a_power;
            apply_cultural_pressure(world, actor_b_id, actor_a_id, delta, current_tick, current_year, event_log);
        }
    }
}

// ============================================================================
// Vassalage
// ============================================================================

/// External pressure at or above which an actor counts as "under pressure" for both
/// `apply_pressure_erosion` and the pressure member of the vassalage band.
pub const PRESSURE_THRESHOLD: f64 = 70.0;

/// Consecutive ticks of sustained pressure required before it starts eroding cohesion
/// (and before the pressure member of the vassalage band opens). Derived from the
/// measured 70→85 transit (5/8/1.5 ticks, max 16): N=10 means the trigger answers to a
/// *sustained* state, not to a spike passing through.
pub const PRESSURE_TICKS_REQUIRED: u32 = 10;

/// Floor that pressure erosion drives cohesion towards — the midpoint of the band's own
/// cohesion window (15–30). Pressure alone can therefore never push cohesion into the
/// `classic_collapse` gate (`cohesion < 15`): submission is the outcome of pressure,
/// death is the outcome of violence.
pub const COHESION_FLOOR: f64 = 22.5;

/// Per-tick fraction of the distance to `COHESION_FLOOR` removed by sustained pressure.
/// Sized so the resulting equilibrium (`COHESION_FLOOR + drift/r`) lands inside the
/// band's cohesion window against the worst measured natural drift (+1.53/tick).
pub const EROSION_RATE: f64 = 0.25;

/// Sustained pressure erodes cohesion (see `engine::phase_pressure_erosion`).
///
/// Cohesion is the one band metric with no downward force left in the engine: it was
/// only ever pushed down by a defender's `cohesion_loss` in combat, and the phantom-fight
/// guard legitimately removed most of those fights. Without this law the third band gate
/// hangs on a metric that nothing can lower, and the band is unreachable by construction.
///
/// The law makes pressure the *cause* of weakness instead of its co-occupant: hold
/// `external_pressure >= PRESSURE_THRESHOLD` for `PRESSURE_TICKS_REQUIRED` consecutive
/// ticks and cohesion decays towards `COHESION_FLOOR`, proportionally to the distance
/// remaining. Self-limiting: the pull is zero at the floor and `max(0.0, ..)` keeps it
/// from *healing* an actor already below it. Combat is still free to push cohesion under
/// the floor — erosion simply stops helping, it never subtracts upwards.
///
/// Skips actors that are already vassals: submission is the *answer* to pressure, so a
/// vassal that kept eroding would make vassalage a death sentence with a title rather
/// than an alternative to disintegration.
///
/// Consumes no RNG.
pub fn apply_pressure_erosion(world: &mut WorldState) {
    let vassal_ids: std::collections::HashSet<String> =
        world.vassalages.iter().map(|v| v.vassal_id.clone()).collect();

    for (actor_id, actor) in world.actors.iter_mut() {
        if actor.get_metric("external_pressure") < PRESSURE_THRESHOLD {
            world.pressure_ticks.remove(actor_id);
            continue;
        }
        let counter = world.pressure_ticks.entry(actor_id.clone()).or_insert(0);
        *counter += 1;
        if *counter < PRESSURE_TICKS_REQUIRED || vassal_ids.contains(actor_id) {
            continue;
        }
        let coh = actor.get_metric("cohesion");
        let erosion = EROSION_RATE * (coh - COHESION_FLOOR).max(0.0);
        if erosion > 0.0 {
            actor.add_metric("cohesion", -erosion);
        }
    }
}

/// The vassalage danger band — an actor that is seriously pressured but not yet
/// collapsing. The `legitimacy` (10–25) and `cohesion` (15–30) members sit strictly
/// *above* the collapse thresholds (`legitimacy < 10`, `cohesion < 15`): an actor in this
/// band is a candidate to submit to a stronger neighbour rather than disintegrate.
///
/// The pressure member is a **state, not an instantaneous window**: pressure held at
/// `PRESSURE_THRESHOLD` for `PRESSURE_TICKS_REQUIRED` consecutive ticks. The old
/// `70..=85` window could not work — `external_pressure` is a fast early variable that
/// crosses it in 1.5–8 ticks and then saturates at 100, so it had left the window ~60
/// ticks before `legitimacy` ever entered its own. There is deliberately no ceiling:
/// once pressure is the *cause* of the weakness (`apply_pressure_erosion`), `ep > 85` is
/// no longer a disqualification from submitting — it is the reason for it.
fn in_vassalage_band(
    actor: &crate::core::Actor,
    pressure_ticks: &std::collections::HashMap<String, u32>,
) -> bool {
    let leg = actor.get_metric("legitimacy");
    let coh = actor.get_metric("cohesion");
    let under_pressure = pressure_ticks
        .get(&actor.id)
        .is_some_and(|t| *t >= PRESSURE_TICKS_REQUIRED);
    under_pressure && (10.0..=25.0).contains(&leg) && (15.0..=30.0).contains(&coh)
}

/// Vassalage formation, dissolution (revolt) and cleanup. Runs parallel to
/// `check_collapses` (see `engine::phase_vassalage`).
///
/// Three sequential steps:
/// 1. Prune relationships whose vassal or overlord is no longer alive.
/// 2. Dissolve (revolt) where the vassal's military has caught up to the overlord's
///    (`vassal_mil >= overlord_mil * 0.8`, the `cultural_power` comparison shape) OR
///    the overlord has itself entered the full vassalage band (`in_vassalage_band`:
///    pressure sustained for `PRESSURE_TICKS_REQUIRED` ticks AND legitimacy 10–25 AND
///    cohesion 15–30).
/// 3. Form new relationships for actors that have spent 3 consecutive ticks in the
///    band, submitting to the neighbour projecting the most military pressure.
///
/// Hierarchy is forbidden: a vassal can never also be an overlord. Overlord
/// attribution is computed on the fly here and never stored as history.
///
/// Consumes no RNG — formation and revolt are fully deterministic — so scenarios
/// that never form a vassalage see an unchanged simulation.
pub fn check_vassalage(world: &mut WorldState, event_log: &mut EventLog) {
    let current_tick = world.tick;
    let current_year = world.year;

    // --- 1. Prune dead relationships ---------------------------------------
    world
        .vassalages
        .retain(|v| world.actors.contains_key(&v.vassal_id) && world.actors.contains_key(&v.overlord_id));

    // --- 2. Revolt / exit --------------------------------------------------
    let mut revolts: Vec<(String, String)> = Vec::new(); // (vassal_id, overlord_id)
    for v in &world.vassalages {
        let vassal_mil = world
            .actors
            .get(&v.vassal_id)
            .map(|a| a.get_metric("military_size"))
            .unwrap_or(0.0);
        let overlord = world.actors.get(&v.overlord_id);
        let overlord_mil = overlord.map(|a| a.get_metric("military_size")).unwrap_or(0.0);
        // "Overlord itself crosses the band" = the full three-metric vassalage band
        // (external_pressure 70–85, legitimacy 10–25, cohesion 15–30), i.e. the
        // overlord has become as weak as something that would itself submit — not a
        // single slipped metric.
        let overlord_in_band = overlord
            .map(|o| in_vassalage_band(o, &world.pressure_ticks))
            .unwrap_or(false);

        let vassal_strong_enough = vassal_mil >= overlord_mil * 0.8;
        if vassal_strong_enough || overlord_in_band {
            revolts.push((v.vassal_id.clone(), v.overlord_id.clone()));
        }
    }
    if !revolts.is_empty() {
        let revolt_set: std::collections::HashSet<(String, String)> = revolts.iter().cloned().collect();
        world
            .vassalages
            .retain(|v| !revolt_set.contains(&(v.vassal_id.clone(), v.overlord_id.clone())));
        for (vassal_id, overlord_id) in &revolts {
            let event = Event::new(
                format!("vassalage_end_{}_{}", overlord_id, vassal_id),
                current_tick,
                current_year,
                vassal_id.clone(),
                EventType::Diplomatic,
                true,
                format!("{} вышел из-под власти {}", vassal_id, overlord_id),
            );
            event_log.add(event);
        }
    }

    // --- 3. Formation ------------------------------------------------------
    // Hierarchy guard sets: an actor bound as a vassal can never be an overlord and
    // vice versa. Kept mutable so relationships formed earlier in this same tick are
    // respected by later candidates.
    let mut vassal_ids: std::collections::HashSet<String> =
        world.vassalages.iter().map(|v| v.vassal_id.clone()).collect();
    let mut overlord_ids: std::collections::HashSet<String> =
        world.vassalages.iter().map(|v| v.overlord_id.clone()).collect();

    // Deterministic order: `world.actors` is a HashMap. Formation order feeds the
    // `vassalages` Vec order, which drives RNG-consuming tribute iteration in
    // `calculate_vassalage_interaction`, so candidates must be processed sorted.
    let mut actor_ids: Vec<String> = world.actors.keys().cloned().collect();
    actor_ids.sort();

    for actor_id in &actor_ids {
        // Update the band counter for every actor each tick.
        let in_band = world
            .actors
            .get(actor_id)
            .map(|a| in_vassalage_band(a, &world.pressure_ticks))
            .unwrap_or(false);
        if !in_band {
            world.vassalage_warning_ticks.remove(actor_id);
            continue;
        }
        let counter = world.vassalage_warning_ticks.entry(actor_id.clone()).or_insert(0);
        *counter += 1;
        if *counter < 3 {
            continue;
        }

        // Hierarchy guard: candidate must be free — not already a vassal, and (the
        // explicit rule) not already someone's overlord.
        if vassal_ids.contains(actor_id) || overlord_ids.contains(actor_id) {
            continue;
        }

        // Attribution: strongest neighbour by military pressure (military_size scaled
        // down by distance), tie-broken by lexicographic id for determinism. The
        // overlord must itself be free of vassalage so it does not gain a vassal
        // while being one (no hierarchy).
        let overlord_id = {
            let actor = match world.actors.get(actor_id) {
                Some(a) => a,
                None => continue,
            };
            let mut neighbor_ids: Vec<(String, u32)> =
                actor.neighbors.iter().map(|n| (n.id.clone(), n.distance)).collect();
            neighbor_ids.sort_by(|a, b| a.0.cmp(&b.0));

            let mut best: Option<(String, f64)> = None;
            for (nid, distance) in &neighbor_ids {
                if nid == actor_id || !world.actors.contains_key(nid) || vassal_ids.contains(nid) {
                    continue;
                }
                let mil = world.actors.get(nid).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
                let pressure = mil / (*distance).max(1) as f64;
                if best.as_ref().map(|(_, p)| pressure > *p).unwrap_or(true) {
                    best = Some((nid.clone(), pressure));
                }
            }
            match best {
                Some((id, _)) => id,
                None => continue,
            }
        };

        // Only submit to a clearly stronger overlord. If the candidate is already
        // ~as strong militarily it would revolt next tick — don't churn.
        let vassal_mil = world.actors.get(actor_id).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
        let overlord_mil = world.actors.get(&overlord_id).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
        if vassal_mil >= overlord_mil * 0.8 {
            continue;
        }

        world.vassalages.push(Vassalage {
            vassal_id: actor_id.clone(),
            overlord_id: overlord_id.clone(),
            formed_tick: current_tick,
        });
        // Shared expansion counter (also incremented on collapse+heir absorption in
        // `check_collapses`); consumed by the coalition trigger in task D.
        if let Some(overlord) = world.actors.get_mut(&overlord_id) {
            overlord.add_metric("expansion_count", 1.0);
        }
        world.vassalage_warning_ticks.remove(actor_id);
        vassal_ids.insert(actor_id.clone());
        overlord_ids.insert(overlord_id.clone());

        let event = Event::new(
            format!("vassalage_form_{}_{}", overlord_id, actor_id),
            current_tick,
            current_year,
            actor_id.clone(),
            EventType::Diplomatic,
            true,
            format!("{} признал сюзеренитет {}", actor_id, overlord_id),
        );
        event_log.add(event);
    }
}

/// Per-tick vassal tribute: 3–5% of the vassal's `economic_output` moves from the
/// vassal's `treasury` to the overlord's `treasury` (symmetric, routed through
/// `treasury` rather than `economic_output` directly — the trade-interaction
/// precedent). Iterates the stable `vassalages` Vec so RNG is consumed
/// deterministically for a fixed seed.
pub fn calculate_vassalage_interaction(
    world: &mut WorldState,
    event_log: &mut EventLog,
    rng: &mut ChaCha8Rng,
) {
    let current_tick = world.tick;
    let current_year = world.year;

    let relationships: Vec<(String, String)> = world
        .vassalages
        .iter()
        .map(|v| (v.vassal_id.clone(), v.overlord_id.clone()))
        .collect();

    for (vassal_id, overlord_id) in relationships {
        let econ = match world.actors.get(&vassal_id) {
            Some(a) => a.get_metric("economic_output"),
            None => continue,
        };
        if !world.actors.contains_key(&overlord_id) {
            continue;
        }

        let rate = 0.03 + rng.gen::<f64>() * 0.02; // 3–5%
        let tribute = econ * rate;

        if let Some(vassal) = world.actors.get_mut(&vassal_id) {
            vassal.add_metric("treasury", -tribute);
        }
        if let Some(overlord) = world.actors.get_mut(&overlord_id) {
            overlord.add_metric("treasury", tribute);
        }

        if should_record_event(&InteractionType::Vassalage, tribute) {
            let event = Event::new(
                format!("vassalage_tribute_{}_{}", overlord_id, vassal_id),
                current_tick,
                current_year,
                overlord_id.clone(),
                EventType::Diplomatic,
                false,
                format!("{} выплатил дань {} ({:.1})", vassal_id, overlord_id, tribute),
            );
            event_log.add(event);
        }
    }
}

/// Determine if an interaction should be recorded as an event
fn should_record_event(interaction_type: &InteractionType, intensity: f64) -> bool {
    match interaction_type {
        InteractionType::Military => true,           // always record
        InteractionType::Trade => intensity > 5.0,
        InteractionType::Diplomatic => intensity > 5.0,
        InteractionType::Migration => intensity > 5.0,
        InteractionType::Vassalage => intensity > 3.0,
        InteractionType::Cultural => intensity > 3.0,
    }
}

/// Calculate cultural power of an actor for displacement comparison
fn cultural_power(actor: &crate::core::Actor) -> f64 {
    let base = actor.get_metric("legitimacy") * 0.3
        + actor.get_metric("cohesion") * 0.3
        + actor.get_metric("economic_output") * 0.2;
    let tag_bonus = actor.actor_tags.len() as f64 * 2.0;
    base + tag_bonus
}

/// Apply cultural pressure from aggressor to target
/// Accumulates displacement progress; at 100.0 triggers culture change
fn apply_cultural_pressure(
    world: &mut WorldState,
    aggressor_id: &str,
    target_id: &str,
    pressure_delta: f64,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
) {
    // Cap progress gain at 3.0 per tick
    let progress_gain = (pressure_delta * 0.05).min(3.0);

    let entry = world.cultural_displacement_progress
        .entry(target_id.to_string())
        .or_insert(0.0);
    *entry += progress_gain;

    // Check if displacement threshold reached
    if *entry >= 100.0 {
        // Get aggressor's culture and tags before mutable borrow
        let aggressor_culture = world.actors.get(aggressor_id)
            .map(|a| a.culture.clone());
        let aggressor_tags: Vec<(String, ActorTag)> = world.actors.get(aggressor_id)
            .map(|a| {
                a.actor_tags.iter()
                    .filter(|(id, _)| {
                        // Only transfer tags the target doesn't already have
                        !world.actors.get(target_id)
                            .map(|t| t.tags.contains(id))
                            .unwrap_or(true)
                    })
                    .take(3)
                    .map(|(id, tag)| (id.clone(), tag.clone()))
                    .collect()
            })
            .unwrap_or_default();

        if let (Some(culture), Some(target)) = (aggressor_culture, world.actors.get_mut(target_id)) {
            target.culture = culture;

            // Transfer tags
            for (tag_id, actor_tag) in aggressor_tags {
                if !target.tags.contains(&tag_id) {
                    target.tags.push(tag_id.clone());
                    target.actor_tags.insert(tag_id, actor_tag);
                }
            }

            let event = Event::new(
                format!("cultural_displacement_{}", target_id),
                current_tick,
                current_year,
                target_id.to_string(),
                EventType::Cultural,
                true,
                format!("{} попал под культурное доминирование {}", target_id, aggressor_id),
            );
            event_log.add(event);
        }

        // Reset progress
        world.cultural_displacement_progress.insert(target_id.to_string(), 0.0);
    }
}

/// Spread tags between neighboring actors via interaction types
#[allow(clippy::too_many_arguments)]
pub fn spread_actor_tags(
    world: &mut WorldState,
    scenario: &Scenario,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    border_type: &BorderType,
    current_tick: u32,
    rng: &mut ChaCha8Rng,
    event_log: &mut EventLog,
) {
    // Build tag definition lookup
    let tag_def_map: std::collections::HashMap<&str, &crate::core::TagDefinition> =
        scenario.tag_definitions.iter().map(|t| (t.id.as_str(), t)).collect();

    // Try spreading in both directions
    try_spread_direction(world, &tag_def_map, scenario, actor_a_id, actor_b_id, distance, border_type, current_tick, rng, event_log);
    try_spread_direction(world, &tag_def_map, scenario, actor_b_id, actor_a_id, distance, border_type, current_tick, rng, event_log);
}

/// Try spreading tags from source to target
#[allow(clippy::too_many_arguments)]
fn try_spread_direction(
    world: &mut WorldState,
    tag_def_map: &std::collections::HashMap<&str, &crate::core::TagDefinition>,
    _scenario: &Scenario,
    source_id: &str,
    target_id: &str,
    _distance: u32,
    border_type: &BorderType,
    current_tick: u32,
    rng: &mut ChaCha8Rng,
    event_log: &mut EventLog,
) {
    // Collect tags to spread (avoid borrow issues)
    let source_tags: Vec<String> = world.actors.get(source_id)
        .map(|a| a.tags.clone())
        .unwrap_or_default();

    let target_tags: Vec<String> = world.actors.get(target_id)
        .map(|a| a.tags.clone())
        .unwrap_or_default();

    let target_era = world.actors.get(target_id)
        .map(|a| a.era.clone())
        .unwrap_or_default();

    for tag_id in &source_tags {
        // Skip if target already has this tag
        if target_tags.contains(tag_id) {
            continue;
        }

        // Look up tag definition
        let tag_def = match tag_def_map.get(tag_id.as_str()) {
            Some(td) => td,
            None => continue, // No definition = no spreading
        };

        // Check era requirement
        if let Some(ref required_era) = tag_def.requires_era {
            if target_era < *required_era {
                continue;
            }
        }

        // Check if any spreads_via channel is active
        let can_spread = tag_def.spreads_via.iter().any(|via| {
            match via {
                TagSpreadType::Trade => {
                    let a_econ = world.actors.get(source_id).map(|a| a.get_metric("economic_output")).unwrap_or(0.0);
                    let b_econ = world.actors.get(target_id).map(|a| a.get_metric("economic_output")).unwrap_or(0.0);
                    a_econ > 20.0 && b_econ > 20.0
                }
                TagSpreadType::War => {
                    let a_mil = world.actors.get(source_id).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
                    let b_mil = world.actors.get(target_id).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
                    a_mil > b_mil * 1.3
                }
                TagSpreadType::Culture => {
                    *border_type == BorderType::Land
                }
                TagSpreadType::Migration => {
                    world.actors.get(target_id).map(|a| a.get_metric("external_pressure")).unwrap_or(0.0) > 50.0
                }
                TagSpreadType::Conquest => {
                    let a_mil = world.actors.get(source_id).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
                    let b_mil = world.actors.get(target_id).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
                    a_mil > b_mil * 1.5
                }
            }
        });

        if !can_spread {
            continue;
        }

        // Cooldown check
        let (id_a, id_b) = if source_id < target_id {
            (source_id, target_id)
        } else {
            (target_id, source_id)
        };
        let cooldown_key = format!("tag_{}_{}_{}", tag_id, id_a, id_b);
        if tag_def.spread_cooldown_ticks > 0 {
            if let Some(&last_tick) = world.tag_spread_cooldowns.get(&cooldown_key) {
                if current_tick.saturating_sub(last_tick) < tag_def.spread_cooldown_ticks {
                    continue;
                }
            }
        }

        // Probability roll
        let roll: f64 = rng.gen();
        if roll > tag_def.spread_chance {
            continue;
        }

        // Apply spread: add tag and ActorTag to target
        if let Some(target) = world.actors.get_mut(target_id) {
            if !target.tags.contains(tag_id) {
                target.tags.push(tag_id.clone());
                target.actor_tags.insert(tag_id.clone(), ActorTag {
                    metrics_modifier: tag_def.metrics_modifier.clone(),
                    spreads_via: tag_def.spreads_via.clone(),
                });
            }
        }

        // Set cooldown
        if tag_def.spread_cooldown_ticks > 0 {
            world.tag_spread_cooldowns.insert(cooldown_key, current_tick);
        }

        // Log event
        let event = Event::new(
            format!("tag_spread_{}_{}", tag_id, target_id),
            current_tick,
            world.year,
            target_id.to_string(),
            EventType::Cultural,
            false,
            format!("Тег '{}' распространился от {} к {}", tag_id, source_id, target_id),
        );
        event_log.add(event);
    }
}
