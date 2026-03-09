//! Headless simulation binary for batch testing and balance tuning
//!
//! Usage:
//! ```bash
//! cargo run --bin sim constantinople_1430 50 42
//! cargo run --bin sim constantinople_1430 1000 42  # 1000 ticks for balance testing
//! cargo run --bin sim constantinople_1430 50 batch  # batch mode: 100 runs with seeds 0-99
//! cargo run --bin sim constantinople_1430 25 scripted balanced  # scripted mode with balanced strategy
//! cargo run --bin sim constantinople_1430 25 scripted diplomacy  # diplomacy-heavy strategy
//! cargo run --bin sim constantinople_1430 25 scripted military  # military-heavy strategy
//! cargo run --bin sim rome_375 50 batch  # Rome batch mode
//! cargo run --bin sim rome_375 50 scripted balanced  # Rome scripted balanced
//! cargo run --bin sim rome_375 50 scripted influence  # Rome scripted influence-focused
//! cargo run --bin sim rome_375 50 scripted wealth  # Rome scripted wealth-focused
//! ```

use engine13::{
    core::{Event, EventType, WorldState, NarrativeStatus},
    engine::{tick, EventLog},
    scenarios::registry,
};
use rand::SeedableRng;
use std::collections::{HashMap, HashSet};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("constantinople_1430");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50);
    let mode = args.get(3).map(|s| s.as_str()).unwrap_or("42");
    let submode = args.get(4).map(|s| s.as_str());

    println!("=== ENGINE13 HEADLESS SIMULATION ===");
    println!("Scenario: {}", scenario_id);
    println!("Ticks: {}", ticks);

    match mode {
        "batch" => run_batch(scenario_id, ticks),
        "scripted" => {
            let strategy = submode.unwrap_or("balanced");
            run_scripted(scenario_id, ticks, strategy);
        },
        _ => {
            let seed: u64 = mode.parse().unwrap_or(42);
            println!("Seed: {}", seed);
            println!();
            run_single(scenario_id, ticks, seed);
        }
    }
}

fn run_single(scenario_id: &str, ticks: u32, seed: u64) {
    let scenario = registry::load_by_id(scenario_id)
        .expect("Unknown scenario");

    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);

    // Initialize actors from scenario
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    // Initialize family_state for family-based scenarios (e.g., Rome 375)
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario.generation_mechanics
            .as_ref()
            .map(|g| g.patriarch_start_age)
            .unwrap_or(40) as u32;

        // Normalize keys: strip "family:" prefix then "family_" prefix (MetricRef expects just "knowledge", "wealth", etc.)
        let mut normalized_metrics = HashMap::new();
        for (key, value) in initial_metrics {
            // First strip "family:" prefix if present
            let key1 = key.strip_prefix("family:").unwrap_or(key);
            // Then strip "family_" prefix if present (MetricRef does this internally)
            let normalized_key = key1.strip_prefix("family_").unwrap_or(key1);
            normalized_metrics.insert(normalized_key.to_string(), *value);
        }

        world.family_state = Some(engine13::core::FamilyState {
            metrics: normalized_metrics,
            patriarch_age,
        });
    }

    // Set generation_mechanics from scenario
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let mut stats = SimStats::default();
    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);

    for tick_num in 0..ticks {
        tick(&mut world, &scenario, &mut event_log, &mut rng);
        let events: Vec<Event> = event_log.events.iter()
            .filter(|e| e.tick == tick_num)
            .cloned()
            .collect();
        stats.record(tick_num, &world, &events, &scenario);

        // Progress indicator every 10 ticks
        if (tick_num + 1) % 10 == 0 {
            eprintln!("Progress: tick {}/{}", tick_num + 1, ticks);
        }
    }

    stats.print_report(&scenario);
}

fn run_batch(scenario_id: &str, ticks: u32) {
    println!("Running batch mode: 100 runs with seeds 0-99");
    println!();

    let scenario = registry::load_by_id(scenario_id)
        .expect("Unknown scenario");

    // Scenario-specific batch stats
    let mut collapses: Vec<u32> = vec![];
    let mut victories: Vec<u32> = vec![];
    let mut events_per_run: Vec<u32> = vec![];
    
    // Rome-specific stats
    let mut rome_military_final: Vec<f64> = vec![];
    let mut rome_cohesion_final: Vec<f64> = vec![];
    let mut rome_legitimacy_final: Vec<f64> = vec![];
    let mut family_influence_final: Vec<f64> = vec![];
    let mut generation_transitions_per_run: Vec<u32> = vec![];
    let mut foreground_shifts_per_run: Vec<u32> = vec![];
    let mut collapsed_actors_all: Vec<String> = vec![];

    for seed in 0..100u64 {
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);

        // Initialize actors from scenario
        for actor in &scenario.actors {
            if !actor.is_successor_template {
                world.actors.insert(actor.id.clone(), actor.clone());
            }
        }

        // Initialize family_state for family-based scenarios (e.g., Rome 375)
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario.generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40) as u32;

            world.family_state = Some(engine13::core::FamilyState {
                metrics: initial_metrics.clone(),
                patriarch_age,
            });
        }

        // Set generation_mechanics from scenario
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut stats = BatchStats::default();
        let mut event_log = EventLog::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        
        let mut prev_foreground: HashSet<String> = world.actors.values()
            .filter(|a| a.narrative_status == NarrativeStatus::Foreground)
            .map(|a| a.id.clone())
            .collect();

        for tick_num in 0..ticks {
            tick(&mut world, &scenario, &mut event_log, &mut rng);
            let events: Vec<Event> = event_log.events.iter()
                .filter(|e| e.tick == tick_num)
                .cloned()
                .collect();
            stats.record(tick_num, &world, &events);

            // Count foreground shifts
            let current_foreground: HashSet<String> = world.actors.values()
                .filter(|a| a.narrative_status == NarrativeStatus::Foreground)
                .map(|a| a.id.clone())
                .collect();
            let shifts: usize = current_foreground.symmetric_difference(&prev_foreground).count();
            stats.foreground_shifts += shifts as u32;
            prev_foreground = current_foreground;

            // Stop early if victory or collapse
            if world.victory_achieved || world.dead_actor_ids.iter().any(|id| id.contains("byzantium")) {
                break;
            }
        }

        if let Some(t) = stats.collapse_tick { collapses.push(t); }
        if let Some(t) = stats.victory_tick { victories.push(t); }
        events_per_run.push(stats.random_events_fired);
        
        // Rome-specific stats
        if scenario_id == "rome_375" {
            if let Some(rome) = world.actors.get("rome") {
                rome_military_final.push(rome.metrics.military_size);
                rome_cohesion_final.push(rome.metrics.cohesion);
                rome_legitimacy_final.push(rome.metrics.legitimacy);
            }
            if let Some(ref family) = world.family_state {
                family_influence_final.push(*family.metrics.get("influence").unwrap_or(&0.0));
            }
            generation_transitions_per_run.push(stats.generation_transitions);
            foreground_shifts_per_run.push(stats.foreground_shifts);
            
            for dead_actor in &world.dead_actors {
                collapsed_actors_all.push(dead_actor.id.clone());
            }
        }
    }

    let collapse_pct = collapses.len() as f64 / 100.0 * 100.0;
    let victory_pct = victories.len() as f64 / 100.0 * 100.0;
    let early_collapses = collapses.iter().filter(|&&t| t < 10).count();
    let mid_collapses = collapses.iter().filter(|&&t| t < 20).count();

    let mut sorted_collapses = collapses.clone(); sorted_collapses.sort();
    let mut sorted_victories = victories.clone(); sorted_victories.sort();
    let median_collapse = sorted_collapses.get(sorted_collapses.len() / 2).copied().unwrap_or(0);
    let median_victory = sorted_victories.get(sorted_victories.len() / 2).copied().unwrap_or(0);
    let avg_events = events_per_run.iter().sum::<u32>() as f64 / 100.0;

    println!("=== SIMULATION REPORT (100 runs, {} ticks each) ===", ticks);
    println!("Ticks completed: {}", ticks);
    println!("Random events fired (avg): {:.1}", avg_events);
    
    // Common collapse/victory stats
    if !collapses.is_empty() {
        println!("Collapses: {} runs ({:.0}%)", collapses.len(), collapse_pct);
        println!("  median collapse tick: {}", median_collapse);
        println!("  collapses before tick 10: {}", early_collapses);
        println!("  collapses before tick 20: {}", mid_collapses);
    }
    if !victories.is_empty() {
        println!("Victory achieved: {} runs ({:.0}%)", victories.len(), victory_pct);
        println!("  median victory tick: {}", median_victory);
    }

    // Rome-specific summary
    if scenario_id == "rome_375" {
        println!();
        println!("=== BALANCE REPORT: ROME 375 (100 runs, {} ticks each, no-player) ===", ticks);
        println!("This report reflects autonomous world behavior without player actions.");
        println!();
        
        let avg_rome_military = rome_military_final.iter().sum::<f64>() / rome_military_final.len() as f64;
        let avg_rome_cohesion = rome_cohesion_final.iter().sum::<f64>() / rome_cohesion_final.len() as f64;
        let avg_rome_legitimacy = rome_legitimacy_final.iter().sum::<f64>() / rome_legitimacy_final.len() as f64;
        
        println!("Rome core metrics (final avg):");
        println!("  military_size:   {:.1}", avg_rome_military);
        println!("  cohesion:        {:.1}", avg_rome_cohesion);
        println!("  legitimacy:      {:.1}", avg_rome_legitimacy);
        
        if !family_influence_final.is_empty() {
            let avg_family_influence = family_influence_final.iter().sum::<f64>() / family_influence_final.len() as f64;
            println!();
            println!("Family metrics (final avg):");
            println!("  family_influence: {:.1}", avg_family_influence);
        }
        
        let avg_gen_transitions = generation_transitions_per_run.iter().sum::<u32>() as f64 / 100.0;
        let avg_foreground_shifts = foreground_shifts_per_run.iter().sum::<u32>() as f64 / 100.0;
        
        println!();
        println!("Dynamics (avg per run):");
        println!("  generation transitions: {:.1}", avg_gen_transitions);
        println!("  foreground shifts:      {:.1}", avg_foreground_shifts);
        
        // Most common collapsed actors
        if !collapsed_actors_all.is_empty() {
            let mut actor_counts: HashMap<String, u32> = HashMap::new();
            for actor_id in &collapsed_actors_all {
                *actor_counts.entry(actor_id.clone()).or_insert(0) += 1;
            }
            let mut sorted_actors: Vec<_> = actor_counts.iter().collect();
            sorted_actors.sort_by(|a, b| b.1.cmp(a.1));
            
            println!();
            println!("Most common collapsed actors:");
            for (actor_id, count) in sorted_actors.iter().take(5) {
                println!("  - {}: {} runs", actor_id, count);
            }
        }
    }
}

/// Scripted strategy for Constantinople and Rome
enum ScriptedStrategy {
    Balanced,
    Diplomacy,
    Military,
    RomeBalanced,
    RomeInfluence,
    RomeWealth,
}

impl ScriptedStrategy {
    fn from_str(s: &str, scenario_id: &str) -> Self {
        // Rome-specific strategies
        if scenario_id == "rome_375" {
            match s.to_lowercase().as_str() {
                "influence" | "influence_heavy" => ScriptedStrategy::RomeInfluence,
                "wealth" | "wealth_heavy" => ScriptedStrategy::RomeWealth,
                _ => ScriptedStrategy::RomeBalanced,
            }
        } else {
            // Constantinople strategies
            match s.to_lowercase().as_str() {
                "diplomacy" | "diplomatic" => ScriptedStrategy::Diplomacy,
                "military" | "military_heavy" => ScriptedStrategy::Military,
                _ => ScriptedStrategy::Balanced,
            }
        }
    }
    
    fn priority_actions(&self) -> Vec<&'static str> {
        match self {
            // Constantinople strategies
            ScriptedStrategy::Balanced => vec![
                "venice_diplomacy",
                "genoa_financial_aid",
                "milan_bankers",
                "venice_naval_support",
                "genoa_mercenaries",
                "milan_condottieri",
                "venice_trade_deal",
                "genoa_galata_garrison",
            ],
            ScriptedStrategy::Diplomacy => vec![
                "venice_diplomacy",
                "genoa_financial_aid",
                "milan_bankers",
                "venice_trade_deal",
                "genoa_galata_garrison",
                "venice_naval_support",
                "genoa_mercenaries",
                "milan_condottieri",
            ],
            ScriptedStrategy::Military => vec![
                "venice_naval_support",
                "genoa_mercenaries",
                "milan_condottieri",
                "genoa_galata_garrison",
                "venice_diplomacy",
                "genoa_financial_aid",
                "milan_bankers",
                "venice_trade_deal",
            ],
            // Rome strategies - using actual IDs from rome_375.rs
            // Note: Many actions have availability gates (e.g., family_wealth > 10)
            // Only gather_information and lay_low are available unconditionally on tick 0
            ScriptedStrategy::RomeBalanced => vec![
                "gather_information",  // Always available, +knowledge -wealth
                "lay_low",             // Always available, +wealth -influence
                "expand_network",      // wealth > 10, +connections -wealth
                "educate_family",      // wealth > 10, +knowledge -wealth
                "build_reputation",    // connections > 15, +influence -wealth
                "invest_wealth",       // wealth > 20, +wealth -connections
                "support_city",        // wealth > 15, +influence +economic_output +cohesion -wealth
                "back_administration", // connections > 15, +connections +legitimacy -wealth
                "fund_defense",        // wealth > 20, +influence +military_quality -wealth
            ],
            ScriptedStrategy::RomeInfluence => vec![
                "build_reputation",    // Priority: influence-focused
                "support_city",
                "fund_defense",
                "back_administration",
                "expand_network",
                "educate_family",
                "invest_wealth",
                "gather_information",
                "lay_low",
            ],
            ScriptedStrategy::RomeWealth => vec![
                "lay_low",             // Priority: wealth accumulation first
                "invest_wealth",
                "gather_information",
                "expand_network",
                "educate_family",
                "support_city",
                "back_administration",
                "build_reputation",
                "fund_defense",
            ],
        }
    }
    
    fn name(&self) -> &'static str {
        match self {
            ScriptedStrategy::Balanced => "balanced",
            ScriptedStrategy::Diplomacy => "diplomacy",
            ScriptedStrategy::Military => "military",
            ScriptedStrategy::RomeBalanced => "balanced",
            ScriptedStrategy::RomeInfluence => "influence",
            ScriptedStrategy::RomeWealth => "wealth",
        }
    }
}

fn run_scripted(scenario_id: &str, ticks: u32, strategy_str: &str) {
    use engine13::application::actions::{apply_player_action, PlayerActionInput, get_available_actions};
    use engine13::commands::AppState;

    let strategy = ScriptedStrategy::from_str(strategy_str, scenario_id);
    
    println!("Running scripted mode with {} strategy", strategy.name());
    println!();

    let scenario = registry::load_by_id(scenario_id)
        .expect("Unknown scenario");

    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, 42);

    // Initialize actors from scenario
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    // Initialize family_state for family-based scenarios (e.g., Rome 375)
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario.generation_mechanics
            .as_ref()
            .map(|g| g.patriarch_start_age)
            .unwrap_or(40) as u32;

        // Normalize keys: strip "family:" prefix then "family_" prefix (MetricRef expects just "knowledge", "wealth", etc.)
        let mut normalized_metrics = HashMap::new();
        for (key, value) in initial_metrics {
            // First strip "family:" prefix if present
            let key1 = key.strip_prefix("family:").unwrap_or(key);
            // Then strip "family_" prefix if present (MetricRef does this internally)
            let normalized_key = key1.strip_prefix("family_").unwrap_or(key1);
            normalized_metrics.insert(normalized_key.to_string(), *value);
        }

        world.family_state = Some(engine13::core::FamilyState {
            metrics: normalized_metrics,
            patriarch_age,
        });
    }

    // Set generation_mechanics from scenario
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    // Set up application state for using apply_player_action
    let mut state = AppState {
        world_state: Some(world),
        event_log: EventLog::new(),
        current_scenario: Some(scenario.clone()),
        rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(42)),
    };

    // ========================================================================
    // Task 0: Validate init before any scripted logic
    // ========================================================================
    if scenario_id == "rome_375" {
        // Check family_state
        let world_state = state.world_state.as_ref().unwrap();
        let family_state_ok = world_state.family_state.is_some();
        let generation_ok = world_state.generation_mechanics.is_some();
        
        eprintln!("Rome scripted init validation:");
        eprintln!("  family_state present: {}", family_state_ok);
        eprintln!("  generation_mechanics present: {}", generation_ok);
        
        if !family_state_ok {
            eprintln!("ERROR: Rome scripted init: family_state is missing - aborting");
            return;
        }
        
        // Check available actions on tick 0
        let available_actions = get_available_actions(&state).unwrap_or_default();
        let available_ids: Vec<&str> = available_actions.iter().map(|a| a.id.as_str()).collect();
        eprintln!("  tick0 available actions: {:?}", available_ids);
        
        if available_actions.is_empty() {
            eprintln!("ERROR: Rome scripted init: no available actions on tick 0 - aborting");
            return;
        }
        
        // Task 2: Verify priority IDs match actual available actions
        let priority_actions = strategy.priority_actions();
        let matching_ids: Vec<_> = priority_actions.iter()
            .filter(|pid| available_ids.contains(pid))
            .collect();
        
        eprintln!("  priority actions matching available: {}/{}", matching_ids.len(), priority_actions.len());
        
        if matching_ids.is_empty() {
            eprintln!("WARNING: No priority action IDs match available actions on tick 0");
            eprintln!("  Strategies may collapse into identical paths under current action economy");
        }
    }
    // ========================================================================

    // Track scripted stats
    let mut total_actions_applied = 0u32;
    let mut total_actions_rejected = 0u32;
    let mut max_federation = 0.0;
    let mut actions_by_type: HashMap<&str, u32> = HashMap::new();
    
    // Rome-specific tracking
    let mut family_influence_start = 0.0;
    let mut family_wealth_start = 0.0;
    let mut family_knowledge_start = 0.0;
    let mut family_connections_start = 0.0;
    let mut rome_legitimacy_start = 0.0;
    let mut rome_cohesion_start = 0.0;
    let mut rome_military_start = 0.0;
    
    // Metric flow tracking for Rome
    let mut influence_gained_from_actions = 0.0;
    let mut influence_lost_from_action_costs = 0.0;
    let mut wealth_gained_from_actions = 0.0;
    let mut wealth_lost_from_action_costs = 0.0;

    // Capture initial family metrics for Rome
    if scenario_id == "rome_375" {
        let world_state = state.world_state.as_ref().unwrap();
        if let Some(ref family) = world_state.family_state {
            family_influence_start = *family.metrics.get("influence").unwrap_or(&0.0);
            family_wealth_start = *family.metrics.get("wealth").unwrap_or(&0.0);
            family_knowledge_start = *family.metrics.get("knowledge").unwrap_or(&0.0);
            family_connections_start = *family.metrics.get("connections").unwrap_or(&0.0);
        }
        if let Some(rome) = world_state.actors.get("rome") {
            rome_legitimacy_start = rome.metrics.legitimacy;
            rome_cohesion_start = rome.metrics.cohesion;
            rome_military_start = rome.metrics.military_size;
        }
    }

    let priority_actions = strategy.priority_actions();

    println!("=== SCRIPTED SIMULATION: {} ===", strategy.name().to_uppercase());

    for tick_num in 0..ticks {
        // Capture before values
        let fed_before = state.world_state.as_ref().unwrap()
            .global_metrics.get("federation_progress").copied().unwrap_or(0.0);
        let pressure_before = state.world_state.as_ref().unwrap()
            .actors.get("byzantium")
            .map(|a| a.metrics.external_pressure)
            .unwrap_or(0.0);
        let _sustained_before = state.world_state.as_ref().unwrap().victory_sustained_ticks;

        // Apply scripted actions before tick using same path as UI
        let mut applied_this_tick = 0u32;
        let mut rejected_this_tick = 0u32;
        let mut actions_applied = Vec::new();
        
        // Track metric changes from actions for Rome
        let mut influence_delta_this_tick = 0.0;
        let mut wealth_delta_this_tick = 0.0;

        for action_id in &priority_actions {
            if applied_this_tick >= scenario.actions_per_tick {
                break;
            }

            // Try to apply action through application layer
            let action_input = PlayerActionInput {
                action_id: action_id.to_string(),
                target_actor_id: None,
            };

            match apply_player_action(&mut state, &action_input) {
                Ok(_) => {
                    applied_this_tick += 1;
                    actions_applied.push(*action_id);
                    *actions_by_type.entry(*action_id).or_insert(0) += 1;
                    
                    // Track metric flow for Rome
                    if scenario_id == "rome_375" {
                        // Get action details to track effects/costs
                        if let Some(action) = state.current_scenario.as_ref().unwrap().patron_actions.iter().find(|a| a.id == *action_id) {
                            for (metric, delta) in &action.effects {
                                // Normalize metric key: strip "family:" and "family_" prefixes
                                let normalized = metric.strip_prefix("family:").unwrap_or(metric).strip_prefix("family_").unwrap_or(metric);
                                if normalized == "influence" {
                                    influence_delta_this_tick += delta;
                                }
                                if normalized == "wealth" {
                                    wealth_delta_this_tick += delta;
                                }
                            }
                            for (metric, cost) in &action.cost {
                                // Normalize metric key
                                let normalized = metric.strip_prefix("family:").unwrap_or(metric).strip_prefix("family_").unwrap_or(metric);
                                if normalized == "influence" {
                                    influence_delta_this_tick += cost; // cost is negative
                                }
                                if normalized == "wealth" {
                                    wealth_delta_this_tick += cost; // cost is negative
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    rejected_this_tick += 1;
                }
            }
        }
        
        // Accumulate flow tracking
        if scenario_id == "rome_375" {
            if influence_delta_this_tick > 0.0 {
                influence_gained_from_actions += influence_delta_this_tick;
            } else {
                influence_lost_from_action_costs += influence_delta_this_tick.abs();
            }
            if wealth_delta_this_tick > 0.0 {
                wealth_gained_from_actions += wealth_delta_this_tick;
            } else {
                wealth_lost_from_action_costs += wealth_delta_this_tick.abs();
            }
        }

        total_actions_applied += applied_this_tick;
        total_actions_rejected += rejected_this_tick;

        // Run tick
        let world_state = state.world_state.as_mut().unwrap();
        let scenario_ref = state.current_scenario.as_ref().unwrap();
        let rng = state.rng.as_mut().unwrap();
        tick(world_state, scenario_ref, &mut state.event_log, rng);

        // Print tick summary - Rome-specific vs Constantinople-specific
        if scenario_id == "rome_375" {
            let world = state.world_state.as_ref().unwrap();
            let inf_before = world.family_state.as_ref().and_then(|f| f.metrics.get("influence")).copied().unwrap_or(0.0);
            let know_before = world.family_state.as_ref().and_then(|f| f.metrics.get("knowledge")).copied().unwrap_or(0.0);
            let wea_before = world.family_state.as_ref().and_then(|f| f.metrics.get("wealth")).copied().unwrap_or(0.0);
            let con_before = world.family_state.as_ref().and_then(|f| f.metrics.get("connections")).copied().unwrap_or(0.0);
            let leg_before = world.actors.get("rome").map(|a| a.metrics.legitimacy).unwrap_or(0.0);
            let coh_before = world.actors.get("rome").map(|a| a.metrics.cohesion).unwrap_or(0.0);
            
            let inf_after = world.family_state.as_ref().and_then(|f| f.metrics.get("influence")).copied().unwrap_or(0.0);
            let know_after = world.family_state.as_ref().and_then(|f| f.metrics.get("knowledge")).copied().unwrap_or(0.0);
            let wea_after = world.family_state.as_ref().and_then(|f| f.metrics.get("wealth")).copied().unwrap_or(0.0);
            let con_after = world.family_state.as_ref().and_then(|f| f.metrics.get("connections")).copied().unwrap_or(0.0);
            let leg_after = world.actors.get("rome").map(|a| a.metrics.legitimacy).unwrap_or(0.0);
            let coh_after = world.actors.get("rome").map(|a| a.metrics.cohesion).unwrap_or(0.0);
            
            println!("tick {:2}: influence {:6.1}->{:6.1}  knowledge {:5.1}->{:5.1}  wealth {:7.1}->{:7.1}  connections {:6.1}->{:6.1}  legitimacy {:5.1}->{:5.1}  cohesion {:5.1}->{:5.1}  actions=[{}]  applied={} rejected={}",
                tick_num, inf_before, inf_after, know_before, know_after, wea_before, wea_after, con_before, con_after, leg_before, leg_after, coh_before, coh_after,
                actions_applied.join(", "), applied_this_tick, rejected_this_tick);
        } else {
            // Constantinople output
            let _fed_before = state.world_state.as_ref().unwrap()
                .global_metrics.get("federation_progress").copied().unwrap_or(0.0);
            let _pressure_before = state.world_state.as_ref().unwrap()
                .actors.get("byzantium")
                .map(|a| a.metrics.external_pressure)
                .unwrap_or(0.0);
            let _sustained_before = state.world_state.as_ref().unwrap().victory_sustained_ticks;

            let fed_after = state.world_state.as_ref().unwrap()
                .global_metrics.get("federation_progress").copied().unwrap_or(0.0);
            let pressure_after = state.world_state.as_ref().unwrap()
                .actors.get("byzantium")
                .map(|a| a.metrics.external_pressure)
                .unwrap_or(0.0);
            let sustained_after = state.world_state.as_ref().unwrap().victory_sustained_ticks;

            // Track max federation
            if fed_after > max_federation {
                max_federation = fed_after;
            }

            println!("tick {:2}: fed {:5.1}->{:5.1}  pressure {:5.1}->{:5.1}  sustained={}  actions=[{}]  applied={} rejected={}",
                tick_num, fed_before, fed_after, pressure_before, pressure_after, sustained_after,
                actions_applied.join(", "), applied_this_tick, rejected_this_tick);
        }

        // Stop early if victory or collapse
        let world = state.world_state.as_ref().unwrap();
        if world.victory_achieved || (scenario_id != "rome_375" && world.dead_actor_ids.iter().any(|id| id.contains("byzantium"))) {
            if scenario_id == "rome_375" {
                println!("Early termination: victory={}", world.victory_achieved);
            } else {
                println!("Early termination: victory={} byzantium_dead={}",
                    world.victory_achieved,
                    world.dead_actor_ids.iter().any(|id| id.contains("byzantium")));
            }
            break;
        }
    }

    // Print final summary
    let world = state.world_state.as_ref().unwrap();
    
    // Rome-specific summary
    if scenario_id == "rome_375" {
        let family_influence_final = world.family_state.as_ref()
            .and_then(|f| f.metrics.get("influence"))
            .copied()
            .unwrap_or(0.0);
        let family_wealth_final = world.family_state.as_ref()
            .and_then(|f| f.metrics.get("wealth"))
            .copied()
            .unwrap_or(0.0);
        let family_knowledge_final = world.family_state.as_ref()
            .and_then(|f| f.metrics.get("knowledge"))
            .copied()
            .unwrap_or(0.0);
        let family_connections_final = world.family_state.as_ref()
            .and_then(|f| f.metrics.get("connections"))
            .copied()
            .unwrap_or(0.0);
        
        let rome_final = world.actors.get("rome");
        let rome_legitimacy_final = rome_final.map(|a| a.metrics.legitimacy).unwrap_or(0.0);
        let rome_cohesion_final = rome_final.map(|a| a.metrics.cohesion).unwrap_or(0.0);
        let rome_military_final = rome_final.map(|a| a.metrics.military_size).unwrap_or(0.0);
        
        let family_total_start = family_influence_start + family_wealth_start + family_knowledge_start + family_connections_start;
        let family_total_final = family_influence_final + family_wealth_final + family_knowledge_final + family_connections_final;
        let family_total_delta = family_total_final - family_total_start;
        
        // Calculate auto_delta influence loss (net delta minus action contributions)
        let influence_net_delta = family_influence_final - family_influence_start;
        let influence_from_actions = influence_gained_from_actions - influence_lost_from_action_costs;
        let influence_from_auto_deltas = influence_net_delta - influence_from_actions;
        
        let wealth_net_delta = family_wealth_final - family_wealth_start;
        let wealth_from_actions = wealth_gained_from_actions - wealth_lost_from_action_costs;
        let wealth_from_auto_deltas = wealth_net_delta - wealth_from_actions;

        println!();
        println!("=== SCRIPTED STRATEGY: {} (ROME 375) ===", strategy.name().to_uppercase());
        println!("Ticks completed:       {}", world.tick);
        println!("Total actions applied: {}", total_actions_applied);
        println!("Total actions rejected: {}", total_actions_rejected);
        println!();
        println!("=== ROME OUTCOME SUMMARY ===");
        println!("Victory achieved:      {}", if world.victory_achieved { "yes" } else { "no" });
        println!("Victory tick:          {}", if world.victory_achieved { format!("{}", world.tick) } else { "n/a".to_string() });
        println!();
        println!("Family metrics:");
        println!("  influence:   {:5.1} -> {:5.1}  (delta: {:+5.1})", family_influence_start, family_influence_final, family_influence_final - family_influence_start);
        println!("  knowledge:   {:5.1} -> {:5.1}  (delta: {:+5.1})", family_knowledge_start, family_knowledge_final, family_knowledge_final - family_knowledge_start);
        println!("  wealth:      {:5.1} -> {:5.1}  (delta: {:+5.1})", family_wealth_start, family_wealth_final, family_wealth_final - family_wealth_start);
        println!("  connections: {:5.1} -> {:5.1}  (delta: {:+5.1})", family_connections_start, family_connections_final, family_connections_final - family_connections_start);
        println!();
        println!("Rome core metrics:");
        println!("  legitimacy:  {:5.1} -> {:5.1}", rome_legitimacy_start, rome_legitimacy_final);
        println!("  cohesion:    {:5.1} -> {:5.1}", rome_cohesion_start, rome_cohesion_final);
        println!("  military:    {:5.1} -> {:5.1}", rome_military_start, rome_military_final);
        
        // Metric flow diagnostics
        println!();
        println!("=== ROME METRIC FLOW: INFLUENCE ===");
        println!("gained from actions:        {:+.1}", influence_gained_from_actions);
        println!("lost from action costs:     {:+.1}", -influence_lost_from_action_costs);
        println!("lost/gained from auto_deltas: {:+.1}", influence_from_auto_deltas);
        println!("net delta:                  {:+.1}", influence_net_delta);
        
        println!();
        println!("=== ROME METRIC FLOW: WEALTH ===");
        println!("gained from actions:        {:+.1}", wealth_gained_from_actions);
        println!("lost from action costs:     {:+.1}", -wealth_lost_from_action_costs);
        println!("gained/lost from auto_deltas: {:+.1}", wealth_from_auto_deltas);
        println!("net delta:                  {:+.1}", wealth_net_delta);
        
        // Secondary diagnostic: family_total
        println!();
        println!("Secondary diagnostic:");
        println!("  family_total: {:5.1} -> {:5.1} (delta: {:+.1})", family_total_start, family_total_final, family_total_delta);
        println!("  (family_total is a resource proxy, not the primary Rome success metric)");

        // Check if strategies collapse
        if total_actions_applied == 0 || (total_actions_rejected > 0 && total_actions_applied < 10) {
            println!();
            println!("WARNING: Rome scripted strategies may collapse into nearly identical paths");
            println!("  under current action economy. Check:");
            println!("  - actions_per_tick limit");
            println!("  - availability gates");
            println!("  - whether enough actions are actually available");
        }

        println!();
        println!("Actions applied by type:");
        let mut sorted_actions: Vec<_> = actions_by_type.iter().collect();
        sorted_actions.sort_by(|a, b| b.1.cmp(a.1));
        for (action_id, count) in sorted_actions {
            println!("  - {}: {}", action_id, count);
        }
    } else {
        // Constantinople summary
        let fed_final = world.global_metrics.get("federation_progress").copied().unwrap_or(0.0);
        let byz_final = world.actors.get("byzantium")
            .map(|a| a.metrics.external_pressure)
            .unwrap_or(0.0);
        let byz_dead = world.dead_actor_ids.iter().any(|id| id.contains("byzantium"));

        println!();
        println!("=== SCRIPTED STRATEGY: {} ===", strategy.name().to_uppercase());
        println!("Victory achieved:      {}", if world.victory_achieved { "yes" } else { "no" });
        println!("Victory tick:          {}", if world.victory_achieved { format!("{}", world.tick) } else { "not achieved".to_string() });
        println!("Federation progress:   {:5.1} -> {:5.1}  (max: {:5.1})", 0.0, fed_final, max_federation);
        println!("Byzantium pressure:    {:5.1} -> {:5.1}", 0.0, byz_final);
        println!("Byzantium collapsed:   {}", if byz_dead { "yes" } else { "no" });
        println!("Total actions applied: {}", total_actions_applied);
        println!("Total actions rejected: {}", total_actions_rejected);
        println!();
        println!("Actions applied by type:");
        let mut sorted_actions: Vec<_> = actions_by_type.iter().collect();
        sorted_actions.sort_by(|a, b| b.1.cmp(a.1));
        for (action_id, count) in sorted_actions {
            println!("  - {}: {}", action_id, count);
        }
    }
}

#[derive(Default)]
struct SimStats {
    pub federation_progress: Vec<f64>,
    pub byzantium_pressure: Vec<f64>,
    pub byzantium_alive: Vec<bool>,
    pub random_events_fired: u32,
    pub military_conflicts: u32,
    pub collapses: Vec<String>,
    
    // Rome-specific stats
    pub rome_military_timeline: Vec<f64>,
    pub rome_cohesion_timeline: Vec<f64>,
    pub rome_legitimacy_timeline: Vec<f64>,
    pub family_influence_timeline: Vec<f64>,
    pub family_knowledge_timeline: Vec<f64>,
    pub family_wealth_timeline: Vec<f64>,
    pub family_connections_timeline: Vec<f64>,
    pub generation_transitions: u32,
    pub foreground_shifts: u32,
    pub prev_foreground: HashSet<String>,
}

impl SimStats {
    fn record(&mut self, _tick: u32, world: &WorldState, events: &[Event], scenario: &engine13::core::Scenario) {
        // Track federation progress
        self.federation_progress.push(
            world.global_metrics.get("federation_progress").copied().unwrap_or(0.0)
        );

        // Track Byzantium status
        if let Some(byz) = world.actors.get("byzantium") {
            self.byzantium_pressure.push(byz.metrics.external_pressure);
            self.byzantium_alive.push(!world.dead_actor_ids.contains("byzantium"));
        }
        
        // Rome-specific tracking
        if scenario.id == "rome_375" {
            if let Some(rome) = world.actors.get("rome") {
                self.rome_military_timeline.push(rome.metrics.military_size);
                self.rome_cohesion_timeline.push(rome.metrics.cohesion);
                self.rome_legitimacy_timeline.push(rome.metrics.legitimacy);
            }
            
            if let Some(ref family) = world.family_state {
                self.family_influence_timeline.push(*family.metrics.get("family_influence").unwrap_or(&0.0));
                self.family_knowledge_timeline.push(*family.metrics.get("family_knowledge").unwrap_or(&0.0));
                self.family_wealth_timeline.push(*family.metrics.get("family_wealth").unwrap_or(&0.0));
                self.family_connections_timeline.push(*family.metrics.get("family_connections").unwrap_or(&0.0));
            }
            
            // Count foreground shifts
            let current_foreground: HashSet<String> = world.actors.values()
                .filter(|a| a.narrative_status == NarrativeStatus::Foreground)
                .map(|a| a.id.clone())
                .collect();
            let shifts: usize = current_foreground.symmetric_difference(&self.prev_foreground).count();
            self.foreground_shifts += shifts as u32;
            self.prev_foreground = current_foreground;
        }

        // Count events by type
        for event in events {
            match event.event_type {
                EventType::Threshold => self.random_events_fired += 1,
                EventType::War => self.military_conflicts += 1,
                EventType::Collapse => self.collapses.push(event.actor_id.clone()),
                _ => {}
            }
            
            // Count generation transitions
            if event.id.contains("generation") && event.event_type == EventType::Threshold {
                self.generation_transitions += 1;
            }
        }
    }

    fn print_report(&self, scenario: &engine13::core::Scenario) {
        println!();
        println!("=== SIMULATION REPORT ===");
        println!("Ticks completed: {}", self.federation_progress.len());
        println!("Random events fired: {}", self.random_events_fired);
        println!("Military conflicts: {}", self.military_conflicts);
        println!("Foreground shifts: {}", self.foreground_shifts);
        println!("Generation transitions: {}", self.generation_transitions);

        if !self.collapses.is_empty() {
            println!("Collapsed actors: {}", self.collapses.join(", "));
        }
        
        // Scenario-specific summary
        if scenario.id == "rome_375" {
            println!();
            println!("=== ROME 375 METRICS ===");
            
            // Rome core metrics timeline (every 5 ticks)
            if !self.rome_military_timeline.is_empty() {
                println!();
                println!("Rome core metrics timeline:");
                for i in (0..self.rome_military_timeline.len()).step_by(5) {
                    let mil = self.rome_military_timeline.get(i).copied().unwrap_or(0.0);
                    let coh = self.rome_cohesion_timeline.get(i).copied().unwrap_or(0.0);
                    let leg = self.rome_legitimacy_timeline.get(i).copied().unwrap_or(0.0);
                    println!("tick {:3}: military={:6.1}  cohesion={:5.1}  legitimacy={:5.1}", i, mil, coh, leg);
                }
                
                // Final values
                if let Some(&last) = self.rome_military_timeline.last() {
                    let coh = self.rome_cohesion_timeline.last().copied().unwrap_or(0.0);
                    let leg = self.rome_legitimacy_timeline.last().copied().unwrap_or(0.0);
                    println!("tick {:3}: military={:6.1}  cohesion={:5.1}  legitimacy={:5.1} [FINAL]", 
                        self.rome_military_timeline.len() - 1, last, coh, leg);
                }
            }
            
            // Family metrics final
            if !self.family_influence_timeline.is_empty() {
                println!();
                println!("Family metrics (final):");
                let inf = self.family_influence_timeline.last().copied().unwrap_or(0.0);
                let kno = self.family_knowledge_timeline.last().copied().unwrap_or(0.0);
                let wea = self.family_wealth_timeline.last().copied().unwrap_or(0.0);
                let con = self.family_connections_timeline.last().copied().unwrap_or(0.0);
                println!("  influence:   {:5.1}", inf);
                println!("  knowledge:   {:5.1}", kno);
                println!("  wealth:      {:5.1}", wea);
                println!("  connections: {:5.1}", con);
            }
        } else {
            // Constantinople / other scenarios
            if let Some(final_fed) = self.federation_progress.last() {
                println!("Federation final: {:.1}", final_fed);
            }
            let max_fed = self.federation_progress.iter().cloned().fold(0.0_f64, f64::max);
            println!("Federation max: {:.1}", max_fed);

            if let Some(&survived) = self.byzantium_alive.last() {
                println!("Byzantium survived: {}", survived);
            }
            let max_pressure = self.byzantium_pressure.iter().cloned().fold(0.0_f64, f64::max);
            println!("Byzantium max pressure: {:.1}", max_pressure);
        }
    }
}

#[derive(Default)]
struct BatchStats {
    pub collapse_tick: Option<u32>,
    pub victory_tick: Option<u32>,
    pub random_events_fired: u32,
    pub generation_transitions: u32,
    pub foreground_shifts: u32,
}

impl BatchStats {
    fn record(&mut self, tick: u32, world: &WorldState, events: &[Event]) {
        // Check for Byzantium collapse
        if self.collapse_tick.is_none()
            && world.dead_actor_ids.iter().any(|a| a.contains("byzantium")) {
            self.collapse_tick = Some(tick);
        }
        // Check for victory
        if self.victory_tick.is_none() && world.victory_achieved {
            self.victory_tick = Some(tick);
        }
        // Count random events (filter out threshold events from phase_events)
        self.random_events_fired += events.iter()
            .filter(|e| matches!(e.event_type, EventType::Threshold))
            .filter(|e| {
                !e.id.starts_with("foreground_")
                    && !e.id.starts_with("metrics_")
                    && !e.id.starts_with("rank_")
                    && !e.id.starts_with("milestone_")
                    && !e.id.starts_with("game_mode_")
                    && !e.id.starts_with("relevance_")
            })
            .count() as u32;
        
        // Count generation transitions
        for event in events {
            if event.id.contains("generation") && event.event_type == EventType::Threshold {
                self.generation_transitions += 1;
            }
        }
    }
}
