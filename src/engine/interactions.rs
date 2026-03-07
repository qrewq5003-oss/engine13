use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::core::{Event, EventType, WorldState, Scenario};
use crate::engine::EventLog;

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
            current_tick, current_year, event_log, rng,
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

        calculate_vassalage_interaction(
            world, &actor_a_id, &actor_b_id, distance,
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
) {
    let actor_a = world.actors.get(actor_a_id).unwrap();
    let actor_b = world.actors.get(actor_b_id).unwrap();

    // Condition: both alive, distance == 1, border Land
    if distance != 1 {
        return;
    }

    // Calculate probability
    let external_pressure_avg = (actor_a.metrics.external_pressure + actor_b.metrics.external_pressure) / 2.0;
    let military_ratio = if actor_a.metrics.military_size > actor_b.metrics.military_size {
        actor_a.metrics.military_size / actor_b.metrics.military_size.max(1.0)
    } else {
        actor_b.metrics.military_size / actor_a.metrics.military_size.max(1.0)
    };
    let land_bonus = if border_type == crate::core::BorderType::Land { 1.0 } else { 0.4 };

    let probability = (external_pressure_avg / 100.0) * military_ratio.min(3.0) * land_bonus;

    // Roll for interaction
    let roll: f64 = rng.gen();
    if roll >= probability || probability <= 0.3 {
        return;
    }

    // Determine stronger and weaker actor
    let (stronger_id, weaker_id) = if actor_a.metrics.military_size >= actor_b.metrics.military_size {
        (actor_a_id.to_string(), actor_b_id.to_string())
    } else {
        (actor_b_id.to_string(), actor_a_id.to_string())
    };

    // Apply losses
    let stronger_loss = 0.05 + rng.gen::<f64>() * 0.10; // 5-15%
    let weaker_loss = 0.15 + rng.gen::<f64>() * 0.15;   // 15-30%
    let cohesion_loss = 10.0 + rng.gen::<f64>() * 10.0;  // 10-20
    let pressure_gain = 15.0 + rng.gen::<f64>() * 10.0;  // 15-25

    if let Some(stronger) = world.actors.get_mut(&stronger_id) {
        stronger.metrics.military_size *= 1.0 - stronger_loss;
    }

    if let Some(weaker) = world.actors.get_mut(&weaker_id) {
        weaker.metrics.military_size *= 1.0 - weaker_loss;
        weaker.metrics.cohesion = (weaker.metrics.cohesion - cohesion_loss).max(0.0);
        weaker.metrics.external_pressure += pressure_gain;
    }

    // Record event
    let intensity = probability;
    if should_record_event(&InteractionType::Military, intensity) {
        let event = Event::new(
            format!("military_conflict_{}_{}", stronger_id, weaker_id),
            current_tick,
            current_year,
            stronger_id.clone(),
            EventType::War,
            true,
            format!("Военный конфликт между {} и {}", stronger_id, weaker_id),
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

/// Vassalage interaction — power projection
fn calculate_vassalage_interaction(
    world: &mut WorldState,
    actor_a_id: &str,
    actor_b_id: &str,
    distance: u32,
    current_tick: u32,
    current_year: i32,
    event_log: &mut EventLog,
    rng: &mut ChaCha8Rng,
) {
    // Condition: power_diff > 2.0, distance == 1
    if distance != 1 {
        return;
    }

    let actor_a = world.actors.get(actor_a_id).unwrap();
    let actor_b = world.actors.get(actor_b_id).unwrap();

    // Calculate power projection (simplified: military_size * legitimacy / 100)
    let power_a = actor_a.metrics.military_size * actor_a.metrics.legitimacy / 100.0;
    let power_b = actor_b.metrics.military_size * actor_b.metrics.legitimacy / 100.0;
    let power_diff = (power_a - power_b).abs();

    if power_diff <= 2.0 {
        return;
    }

    // Determine dominant and weak actor
    let (dominant_id, weak_id) = if power_a > power_b {
        (actor_a_id.to_string(), actor_b_id.to_string())
    } else {
        (actor_b_id.to_string(), actor_a_id.to_string())
    };

    let legitimacy_loss = power_diff * 0.5;
    let cohesion_loss = power_diff * 0.3;
    let economic_gain = 1.0; // tribute

    let weak_legitimacy_before = {
        let weak = world.actors.get(&weak_id).unwrap();
        weak.metrics.legitimacy
    };
    let _weak_legitimacy_before = weak_legitimacy_before;

    if let Some(weak) = world.actors.get_mut(&weak_id) {
        weak.metrics.legitimacy = (weak.metrics.legitimacy - legitimacy_loss).max(0.0);
        weak.metrics.cohesion = (weak.metrics.cohesion - cohesion_loss).max(0.0);
    }

    if let Some(dominant) = world.actors.get_mut(&dominant_id) {
        dominant.metrics.economic_output += economic_gain;
    }

    // Record event if legitimacy dropped below 30 for the first time
    let _weak_legitimacy_after = {
        let weak = world.actors.get(&weak_id).unwrap();
        weak.metrics.legitimacy
    };

    if should_record_event(&InteractionType::Vassalage, legitimacy_loss) {
        let event = Event::new(
            format!("vassalage_{}_{}", dominant_id, weak_id),
            current_tick,
            current_year,
            dominant_id.clone(),
            EventType::Diplomatic,
            false,
            format!("{} устанавливает вассальную зависимость над {}", dominant_id, weak_id),
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
