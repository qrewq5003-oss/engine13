use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::core::{Event, EventType, WorldState, Scenario, Religion, Culture};
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

/// Effective military strength accounting for force projection through neighbors
pub fn effective_military(actor: &crate::core::Actor, neighbors: Vec<&crate::core::Actor>) -> f64 {
    let active_neighbors = neighbors.len().max(1);
    
    // Average affinity with all neighbors
    let avg_affinity: f64 = neighbors.iter()
        .map(|n| affinity(actor, n))
        .sum::<f64>() / active_neighbors as f64;
    
    // More foreign neighbors = more military stretched
    let divisor = (active_neighbors as f64 * avg_affinity).max(1.0);
    actor.metrics.military_size / divisor
}

/// Type of interaction between actors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InteractionType {
    Military,
    Trade,
    Diplomatic,
    Migration,
    Vassalage,
    Cultural,
}

/// Interaction between two actors
pub struct Interaction {
    pub actor_a: String,
    pub actor_b: String,
    pub interaction_type: InteractionType,
    pub intensity: f64,
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

        calculate_cultural_interaction(
            world, &actor_a_id, &actor_b_id, distance,
            current_tick, current_year, event_log, rng,
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

    pairs
}

/// Military interaction — conflict between neighbors
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

    let actor_a = world.actors.get(actor_a_id).unwrap();
    let actor_b = world.actors.get(actor_b_id).unwrap();

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
    let (attacker_id, defender_id, attacker_eff, defender_eff) = if eff_mil_a >= eff_mil_b {
        (actor_a_id.to_string(), actor_b_id.to_string(), eff_mil_a, eff_mil_b)
    } else {
        (actor_b_id.to_string(), actor_a_id.to_string(), eff_mil_b, eff_mil_a)
    };

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
    let attacker = world.actors.get(&attacker_id).unwrap();
    let defender = world.actors.get(&defender_id).unwrap();
    
    let pressure_mod = (attacker.metrics.external_pressure / 100.0) * 0.2;
    let military_mod = if attacker.metrics.military_size > defender.metrics.military_size * 1.5 {
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

    #[cfg(debug_assertions)]
    eprintln!("[INTERACTION] Military: {} vs {}", attacker_id, defender_id);

    // Apply losses
    let attacker_loss = 0.05 + rng.gen::<f64>() * 0.10; // 5-15%
    let defender_loss = 0.15 + rng.gen::<f64>() * 0.15;   // 15-30%
    let cohesion_loss = 10.0 + rng.gen::<f64>() * 10.0;  // 10-20
    let pressure_gain = 15.0 + rng.gen::<f64>() * 10.0;  // 15-25

    if let Some(attacker_actor) = world.actors.get_mut(&attacker_id) {
        attacker_actor.metrics.military_size *= 1.0 - attacker_loss;
    }

    if let Some(defender_actor) = world.actors.get_mut(&defender_id) {
        defender_actor.metrics.military_size *= 1.0 - defender_loss;
        defender_actor.metrics.cohesion = (defender_actor.metrics.cohesion - cohesion_loss).max(0.0);
        defender_actor.metrics.external_pressure += pressure_gain;
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
fn calculate_trade_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    border_type: crate::core::BorderType,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    rng: &mut ChaCha8Rng,
) {
    let actor_a = world.actors.get(actor_a_id).unwrap();
    let actor_b = world.actors.get(actor_b_id).unwrap();

    // Condition: external_pressure_avg < 60, economic_output_both > 20
    let external_pressure_avg = (actor_a.metrics.external_pressure + actor_b.metrics.external_pressure) / 2.0;
    if external_pressure_avg >= 60.0 {
        return;
    }
    if actor_a.metrics.economic_output < 20.0 || actor_b.metrics.economic_output < 20.0 {
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
    let bonus = (actor_a.metrics.economic_output + actor_b.metrics.economic_output) * 0.002 * distance_modifier * sea_bonus;

    // Apply treasury gain
    if let Some(actor) = world.actors.get_mut(actor_a_id) {
        actor.metrics.treasury += bonus;
    }
    if let Some(actor) = world.actors.get_mut(actor_b_id) {
        actor.metrics.treasury += bonus;
    }

    // Set cooldown
    world.interaction_cooldowns.insert(trade_key, current_tick);

    #[cfg(debug_assertions)]
    eprintln!("[INTERACTION] Trade: {} vs {}", actor_a_id, actor_b_id);

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
fn calculate_diplomatic_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    rng: &mut ChaCha8Rng,
) {
    let actor_a = world.actors.get(actor_a_id).unwrap();
    let actor_b = world.actors.get(actor_b_id).unwrap();

    // Condition: legitimacy_diff > 15
    let legitimacy_diff = (actor_a.metrics.legitimacy - actor_b.metrics.legitimacy).abs();
    if legitimacy_diff <= 15.0 {
        return;
    }

    // Stronger actor influences weaker
    let (influencer_id, influenced_id) = if actor_a.metrics.legitimacy > actor_b.metrics.legitimacy {
        (actor_a_id.to_string(), actor_b_id.to_string())
    } else {
        (actor_b_id.to_string(), actor_a_id.to_string())
    };

    let influence = (legitimacy_diff * 0.1) / distance as f64;

    if let Some(influenced) = world.actors.get_mut(&influenced_id) {
        influenced.metrics.cohesion = (influenced.metrics.cohesion + influence).min(100.0);
    }

    #[cfg(debug_assertions)]
    eprintln!("[INTERACTION] Diplomatic: {} vs {}", actor_a_id, actor_b_id);

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
fn calculate_migration_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    border_type: crate::core::BorderType,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    rng: &mut ChaCha8Rng,
) {
    // Condition: border Land, external_pressure > 65, cohesion < 40
    if border_type != crate::core::BorderType::Land {
        return;
    }

    // Find the pressuring actor (high external_pressure, low cohesion)
    let pressuring_id = {
        let actor_a = world.actors.get(actor_a_id).unwrap();
        let actor_b = world.actors.get(actor_b_id).unwrap();

        if actor_a.metrics.external_pressure > 65.0 && actor_a.metrics.cohesion < 40.0 {
            Some(actor_a_id.to_string())
        } else if actor_b.metrics.external_pressure > 65.0 && actor_b.metrics.cohesion < 40.0 {
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
    let pressuring_pop = {
        let pressuring = world.actors.get(&pressuring_id).unwrap();
        pressuring.metrics.population
    };

    let pressuring_pressure = {
        let pressuring = world.actors.get(&pressuring_id).unwrap();
        pressuring.metrics.external_pressure
    };

    let pressure_transfer = (pressuring_pressure - 65.0) * 0.2 / distance as f64;
    let pop_loss_ratio = 0.01;
    let pop_gain_ratio = 0.005;

    if let Some(pressuring) = world.actors.get_mut(&pressuring_id) {
        pressuring.metrics.population *= 1.0 - pop_loss_ratio;
    }

    if let Some(neighbor) = world.actors.get_mut(&neighbor_id) {
        neighbor.metrics.external_pressure += pressure_transfer;
        let pop_gain = pressuring_pop * pop_gain_ratio;
        neighbor.metrics.population += pop_gain;
    }

    #[cfg(debug_assertions)]
    eprintln!("[INTERACTION] Migration: {} vs {}", actor_a_id, actor_b_id);

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
    let actor_a = world.actors.get(actor_a_id).unwrap();
    let actor_b = world.actors.get(actor_b_id).unwrap();

    // Calculate shared tags
    let shared_tags: Vec<&String> = actor_a.tags.iter()
        .filter(|t| actor_b.tags.contains(t))
        .collect();

    let shared_count = shared_tags.len() as f64;
    let bonus = shared_count * 0.5 / distance as f64;
    let malus = if shared_count == 0.0 && distance == 1 { 0.5 } else { 0.0 };

    // Apply cohesion changes
    let cohesion_change = bonus - malus;

    if let Some(actor) = world.actors.get_mut(actor_a_id) {
        actor.metrics.cohesion = (actor.metrics.cohesion + cohesion_change).clamp(0.0, 100.0);
    }
    if let Some(actor) = world.actors.get_mut(actor_b_id) {
        actor.metrics.cohesion = (actor.metrics.cohesion + cohesion_change).clamp(0.0, 100.0);
    }

    #[cfg(debug_assertions)]
    eprintln!("[INTERACTION] Cultural: {} vs {}", actor_a_id, actor_b_id);

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
