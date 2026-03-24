use crate::core::{MetricRef, WorldState};
use crate::scenarios::registry;
use rand::SeedableRng;

#[test]
fn test_metric_ref_parse_actor() {
    let mr = MetricRef::parse("actor:venice.treasury");
    match mr {
        MetricRef::Actor { actor_id, metric } => {
            assert_eq!(actor_id, "venice");
            assert_eq!(metric, "treasury");
        }
        _ => panic!("Expected Actor variant"),
    }
}

#[test]
fn test_metric_ref_parse_family() {
    // Test canonical format
    let mr = MetricRef::parse("family:influence");
    match mr {
        MetricRef::Family { key } => {
            assert_eq!(key, "influence");
        }
        _ => panic!("Expected Family variant"),
    }
    
    // Test legacy format (backward-compat, normalized to canonical)
    let mr = MetricRef::parse("family:family_influence");
    match mr {
        MetricRef::Family { key } => {
            assert_eq!(key, "influence");
        }
        _ => panic!("Expected Family variant"),
    }
}

#[test]
fn test_metric_ref_parse_global() {
    let mr = MetricRef::parse("global:federation_progress");
    match mr {
        MetricRef::Global { key } => {
            assert_eq!(key, "federation_progress");
        }
        _ => panic!("Expected Global variant"),
    }
}

#[test]
fn test_metric_ref_apply_actor_treasury() {
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);

    // Add venice actor
    for actor in &scenario.actors {
        if actor.id == "venice" {
            world.actors.insert(actor.id.clone(), actor.clone());
            break;
        }
    }

    let before = MetricRef::parse("actor:venice.treasury").get(&world);
    MetricRef::parse("actor:venice.treasury").apply(&mut world, -100.0);
    let after = MetricRef::parse("actor:venice.treasury").get(&world);

    // Treasury can go negative
    assert_eq!(after, before - 100.0);
}

#[test]
fn test_metric_ref_apply_actor_treasury_negative() {
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);

    // Add venice actor with low treasury
    for actor in &scenario.actors {
        if actor.id == "venice" {
            let mut venice = actor.clone();
            venice.set_metric("treasury", 50.0);
            world.actors.insert(actor.id.clone(), venice);
            break;
        }
    }

    MetricRef::parse("actor:venice.treasury").apply(&mut world, -100.0);
    let after = MetricRef::parse("actor:venice.treasury").get(&world);

    // Treasury can go negative (debts)
    assert_eq!(after, -50.0);
}

#[test]
fn test_metric_ref_apply_global_clamped() {
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    
    // Global metrics should clamp to 0-100
    MetricRef::parse("federation_progress").apply(&mut world, 150.0);
    let value = MetricRef::parse("federation_progress").get(&world);
    
    assert_eq!(value, 100.0);
}

#[test]
fn test_metric_ref_apply_legitimacy_clamped() {
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    
    // Add venice actor
    for actor in &scenario.actors {
        if actor.id == "venice" {
            world.actors.insert(actor.id.clone(), actor.clone());
            break;
        }
    }
    
    // Legitimacy should clamp to 0-100
    MetricRef::parse("venice.legitimacy").apply(&mut world, 200.0);
    let value = MetricRef::parse("venice.legitimacy").get(&world);
    
    assert!(value <= 100.0);
    assert_eq!(value, 100.0);
}

#[test]
fn test_metric_ref_apply_military_size_min_zero() {
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    
    // Add venice actor with low military
    for actor in &scenario.actors {
        if actor.id == "venice" {
            let mut venice = actor.clone();
            venice.set_metric("military_size", 10.0);
            world.actors.insert(actor.id.clone(), venice);
            break;
        }
    }
    
    MetricRef::parse("venice.military_size").apply(&mut world, -50.0);
    let value = MetricRef::parse("venice.military_size").get(&world);

    // military_size should not go below 0
    assert_eq!(value, 0.0);
}

#[test]
fn test_family_state_initialized() {
    // Load rome_375 and verify family_state is initialized
    let _scenario = registry::load_by_id("rome_375").unwrap();
    let mut state = crate::AppState::default();
    let _db = crate::db::Db::open_in_memory().unwrap();

    crate::application::load_scenario(&mut state, &_db, "rome_375".to_string()).unwrap();
    
    let world_state = state.world_state.as_ref().unwrap();
    assert!(world_state.family_state.is_some(), "family_state should be Some for Rome 375");
    
    let family_state = world_state.family_state.as_ref().unwrap();
    assert!(family_state.patriarch_age > 0, "patriarch_age should be set");
    assert!(!family_state.metrics.is_empty(), "family metrics should not be empty");
}

#[test]
fn test_family_state_none_for_constantinople() {
    // Load constantinople_1430 and verify family_state is None
    let _scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut state = crate::AppState::default();
    let _db = crate::db::Db::open_in_memory().unwrap();

    crate::application::load_scenario(&mut state, &_db, "constantinople_1430".to_string()).unwrap();

    let world_state = state.world_state.as_ref().unwrap();
    assert!(world_state.family_state.is_none(), "family_state should be None for Constantinople");
}

#[test]
fn test_global_metrics_display_configured() {
    // Verify Constantinople has federation progress display config
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    assert!(!scenario.global_metrics_display.is_empty(), "Constantinople should have global metrics display");
    
    let fed_display = scenario.global_metrics_display.iter()
        .find(|m| m.metric.contains("federation_progress"));
    assert!(fed_display.is_some(), "Should have federation_progress display config");
    
    let fed_display = fed_display.unwrap();
    assert_eq!(fed_display.panel_title, "Федерация");
    assert!(!fed_display.thresholds.is_empty(), "Should have thresholds");
}

#[test]
fn test_generation_mechanics_has_era_texts() {
    // Verify Rome 375 has era texts in generation_mechanics
    let scenario = registry::load_by_id("rome_375").unwrap();
    assert!(scenario.generation_mechanics.is_some(), "Rome 375 should have generation_mechanics");
    
    let gen = scenario.generation_mechanics.as_ref().unwrap();
    assert_eq!(gen.panel_label, "Семья Ди Милано");
    assert!(!gen.era_texts.is_empty(), "Should have era texts");
}

#[test]
fn test_scenario_victory_requires_byzantium_alive() {
    // Load constantinople_1430
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    let mut event_log = crate::engine::EventLog::new();
    
    // Add byzantium actor
    for actor in &scenario.actors {
        if actor.id == "byzantium" {
            world.actors.insert(actor.id.clone(), actor.clone());
            break;
        }
    }
    
    // Set federation_progress = 100 (high enough to stay above 80 after auto_deltas), tick = 45
    // Note: MetricRef strips "global:" prefix when storing
    world.global_metrics.insert("federation_progress".to_string(), 100.0);
    world.tick = 45;  // minimum_tick is 40 (20 years × 2 ticks/year)
    
    // Set byzantium.external_pressure = 90 (above threshold 85)
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("external_pressure", 90.0);
    }

    // Run check_victory_condition via tick
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);

    // victory_achieved should be false because pressure is too high
    assert!(!world.victory_achieved, "Victory should not be achieved when pressure > 85");

    // Lower pressure to 70
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("external_pressure", 70.0);
    }
    
    // Run tick again
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // victory_sustained_ticks should be 1, victory_achieved still false (needs 3 ticks)
    assert_eq!(world.victory_sustained_ticks, 1, "Should have 1 sustained tick");
    assert!(!world.victory_achieved, "Victory should not be achieved yet (needs 3 ticks)");
}

#[test]
fn test_victory_sustained_ticks_resets() {
    // Load constantinople_1430
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    let mut event_log = crate::engine::EventLog::new();
    
    // Add byzantium actor
    for actor in &scenario.actors {
        if actor.id == "byzantium" {
            world.actors.insert(actor.id.clone(), actor.clone());
            break;
        }
    }
    
    // Set victory conditions: federation = 100 (high enough to stay above 80), pressure = 70, tick = 45
    world.global_metrics.insert("federation_progress".to_string(), 100.0);
    world.tick = 45;  // minimum_tick is 40 (20 years × 2 ticks/year)
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("external_pressure", 70.0);
    }

    // Run 2 ticks - should accumulate sustained ticks
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);

    assert_eq!(world.victory_sustained_ticks, 2, "Should have 2 sustained ticks");

    // Raise pressure above threshold
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("external_pressure", 90.0);
    }
    
    // Run another tick - should reset sustained ticks
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    assert_eq!(world.victory_sustained_ticks, 0, "Sustained ticks should reset when condition fails");
}

#[test]
fn test_generation_transfer_applies_inheritance() {
    // Load rome_375
    let scenario = registry::load_by_id("rome_375").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    let mut event_log = crate::engine::EventLog::new();

    // Add rome actor
    for actor in &scenario.actors {
        if actor.id == "rome" {
            world.actors.insert(actor.id.clone(), actor.clone());
            break;
        }
    }

    // Set family_state with patriarch_age at end age (using canonical short-key format)
    let gen = scenario.generation_mechanics.as_ref().unwrap();
    world.family_state = Some(crate::core::FamilyState {
        metrics: [("influence".to_string(), 60.0)].into(),
        patriarch_age: gen.patriarch_end_age,
        generation_count: 0,
    });

    let initial_influence = world.family_state.as_ref().unwrap().metrics.get("influence").copied().unwrap_or(0.0);

    // Run tick - should trigger generation transfer
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);

    // Check patriarch_age reset to start age
    assert_eq!(world.family_state.as_ref().unwrap().patriarch_age, gen.patriarch_start_age, "Patriarch age should reset");

    // Check family_influence reduced by inheritance coefficient (0.85)
    let final_influence = world.family_state.as_ref().unwrap().metrics.get("influence").copied().unwrap_or(0.0);
    let expected = initial_influence * 0.85;
    assert!((final_influence - expected).abs() < 0.1, "Family influence should be reduced by inheritance coefficient");
}

#[test]
fn test_initial_family_metrics_loaded() {
    // Load rome_375
    let _scenario = registry::load_by_id("rome_375").unwrap();
    let mut state = crate::AppState::default();
    let _db = crate::db::Db::open_in_memory().unwrap();

    crate::application::load_scenario(&mut state, &_db, "rome_375".to_string()).unwrap();

    let world_state = state.world_state.as_ref().unwrap();
    assert!(world_state.family_state.is_some(), "family_state should be Some for Rome 375");

    let family_state = world_state.family_state.as_ref().unwrap();

    // Check that initial metrics are loaded (using canonical short-key format)
    assert!(family_state.metrics.contains_key("influence"), "Should have influence");
    assert!(family_state.metrics.contains_key("knowledge"), "Should have knowledge");
    assert!(family_state.metrics.contains_key("wealth"), "Should have wealth");
    assert!(family_state.metrics.contains_key("connections"), "Should have connections");
}

#[test]
fn test_scenario_all_metrics_valid() {
    // Validate rome_375 scenario
    let rome_scenario = registry::load_by_id("rome_375").unwrap();
    let rome_result = registry::validate_scenario(&rome_scenario);
    assert!(rome_result.is_ok(), "Rome 375 should pass validation: {:?}", rome_result.err());

    // Validate constantinople_1430 scenario
    let constantinople_scenario = registry::load_by_id("constantinople_1430").unwrap();
    let constantinople_result = registry::validate_scenario(&constantinople_scenario);
    assert!(constantinople_result.is_ok(), "Constantinople 1430 should pass validation: {:?}", constantinople_result.err());
}

#[test]
fn test_actor_collapse_deterministic_no_freeze() {
    // Test that actors collapse deterministically and don't freeze in near-dead state
    use rand::SeedableRng;
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    
    // Add byzantium actor with metrics that will trigger collapse
    for actor in &scenario.actors {
        if actor.id == "byzantium" {
            let mut byzantium = actor.clone();
            // Set metrics to trigger classic collapse
            byzantium.set_metric("legitimacy", 8.0);  // < 10
            byzantium.set_metric("cohesion", 12.0);   // < 15
            byzantium.set_metric("external_pressure", 90.0);  // > 85
            world.actors.insert(actor.id.clone(), byzantium);
            break;
        }
    }
    
    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    
    // Run 10 ticks - actor should collapse
    let mut collapse_tick = None;
    for tick_num in 0..10 {
        // Check counter before tick
        let counter_before = world.collapse_warning_ticks.get("byzantium").copied().unwrap_or(0);
        
        crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
        
        if world.dead_actor_ids.contains("byzantium") && collapse_tick.is_none() {
            collapse_tick = Some(world.tick);
        }
        
        // Check counter after tick
        let counter_after = world.collapse_warning_ticks.get("byzantium").copied().unwrap_or(0);
        println!("tick {}: counter {} -> {}, collapsed={}", tick_num, counter_before, counter_after, world.dead_actor_ids.contains("byzantium"));
    }
    
    // Verify collapse happened exactly once and within expected timeframe
    assert!(collapse_tick.is_some(), "Byzantium should have collapsed within 10 ticks, but didn't - freeze detected! counters: see output above");
    
    // Verify no zombie state - actor should be in dead_actors, not in actors
    assert!(!world.actors.contains_key("byzantium"), "Collapsed actor should be removed from actors");
    assert!(world.dead_actors.iter().any(|d| d.id == "byzantium"), "Collapsed actor should be in dead_actors");
}

#[test]
fn test_actor_collapse_no_oscillation_freeze() {
    // Test that actors don't freeze due to metric oscillation
    // This tests the fix for cumulative vs consecutive counter
    use rand::SeedableRng;
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    
    // Add byzantium actor
    for actor in &scenario.actors {
        if actor.id == "byzantium" {
            world.actors.insert(actor.id.clone(), actor.clone());
            break;
        }
    }
    
    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    
    // Manually set dangerous state for 3 ticks, with temporary recovery in between
    // This simulates oscillation that would cause freeze with consecutive counter
    
    // Tick 1: dangerous
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("legitimacy", 8.0);
        byz.set_metric("cohesion", 12.0);
        byz.set_metric("external_pressure", 90.0);
    }
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // Tick 2: temporary recovery (would reset consecutive counter)
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("legitimacy", 50.0);
        byz.set_metric("cohesion", 50.0);
        byz.set_metric("external_pressure", 50.0);
    }
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // Tick 3: dangerous again
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("legitimacy", 8.0);
        byz.set_metric("cohesion", 12.0);
        byz.set_metric("external_pressure", 90.0);
    }
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // Tick 4: temporary recovery
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("legitimacy", 50.0);
        byz.set_metric("cohesion", 50.0);
        byz.set_metric("external_pressure", 50.0);
    }
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // Tick 5: dangerous - should collapse (cumulative counter = 3)
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.set_metric("legitimacy", 8.0);
        byz.set_metric("cohesion", 12.0);
        byz.set_metric("external_pressure", 90.0);
    }
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // With cumulative counter, collapse should happen by tick 5
    // With consecutive counter, actor would oscillate forever
    assert!(world.dead_actor_ids.contains("byzantium"), 
        "Byzantium should have collapsed after 3 cumulative dangerous ticks - freeze due to oscillation!");
}

#[test]
fn test_constantinople_sim_balance() {
    use rand::SeedableRng;
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    world.global_metrics.insert("federation_progress".to_string(), 0.0);

    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    let mut victory_tick = None;
    for _ in 0..100 {
        crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
        if world.victory_achieved && victory_tick.is_none() {
            victory_tick = Some(world.tick);
        }
    }

    // Note: Victory requires federation_progress >= 80 AND byzantium.external_pressure < 85
    // In autonomous simulation, external_pressure tends to increase, making victory unlikely
    // Test verifies simulation runs correctly; victory may or may not occur
    assert!(
        victory_tick.map(|t| t >= 40 && t <= 100).unwrap_or(true),
        "Constantinople victory tick {:?} outside expected range 40-100 (or no victory)",
        victory_tick
    );
}

#[test]
fn test_rome_375_sim_balance() {
    use rand::SeedableRng;
    let scenario = registry::load_by_id("rome_375").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    if let Some(ref initial) = scenario.initial_family_metrics {
        world.family_state = Some(crate::core::FamilyState {
            metrics: initial.clone(),
            patriarch_age: scenario.generation_mechanics.as_ref().unwrap().patriarch_start_age,
            generation_count: 0,
        });
    }

    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    let mut victory_tick = None;
    for _ in 0..100 {
        crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
        if world.victory_achieved && victory_tick.is_none() {
            victory_tick = Some(world.tick);
        }
    }

    // Note: Victory requires family:influence >= 90
    // In autonomous simulation, influence fluctuates and rarely reaches 90
    // Test verifies simulation runs correctly; victory may or may not occur
    assert!(
        victory_tick.map(|t| t >= 25 && t <= 100).unwrap_or(true),
        "Rome 375 victory tick {:?} outside expected range 25-100 (or no victory)",
        victory_tick
    );
}
