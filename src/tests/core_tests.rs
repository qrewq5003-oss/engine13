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
    let mr = MetricRef::parse("family:family_influence");
    match mr {
        MetricRef::Family { key } => {
            assert_eq!(key, "family_influence");
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
            venice.metrics.treasury = 50.0;
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
            venice.metrics.military_size = 10.0;
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
    let scenario = registry::load_by_id("rome_375").unwrap();
    let mut state = crate::AppState::default();
    let db = crate::db::Db::open_in_memory().unwrap();
    
    crate::application::load_scenario(&mut state, &db, "rome_375".to_string()).unwrap();
    
    let world_state = state.world_state.as_ref().unwrap();
    assert!(world_state.family_state.is_some(), "family_state should be Some for Rome 375");
    
    let family_state = world_state.family_state.as_ref().unwrap();
    assert!(family_state.patriarch_age > 0, "patriarch_age should be set");
    assert!(!family_state.metrics.is_empty(), "family metrics should not be empty");
}

#[test]
fn test_family_state_none_for_constantinople() {
    // Load constantinople_1430 and verify family_state is None
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut state = crate::AppState::default();
    let db = crate::db::Db::open_in_memory().unwrap();
    
    crate::application::load_scenario(&mut state, &db, "constantinople_1430".to_string()).unwrap();
    
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
    
    // Set federation_progress = 100 (high enough to stay above 80 after auto_deltas), tick = 25
    // Note: MetricRef strips "global:" prefix when storing
    world.global_metrics.insert("federation_progress".to_string(), 100.0);
    world.tick = 25;
    
    // Set byzantium.external_pressure = 90 (above threshold 85)
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.metrics.external_pressure = 90.0;
    }
    
    // Run check_victory_condition via tick
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // victory_achieved should be false because pressure is too high
    assert!(!world.victory_achieved, "Victory should not be achieved when pressure > 85");
    
    // Lower pressure to 70
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.metrics.external_pressure = 70.0;
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
    
    // Set victory conditions: federation = 100 (high enough to stay above 80), pressure = 70, tick = 25
    world.global_metrics.insert("federation_progress".to_string(), 100.0);
    world.tick = 25;
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.metrics.external_pressure = 70.0;
    }
    
    // Run 2 ticks - should accumulate sustained ticks
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    assert_eq!(world.victory_sustained_ticks, 2, "Should have 2 sustained ticks");
    
    // Raise pressure above threshold
    if let Some(byz) = world.actors.get_mut("byzantium") {
        byz.metrics.external_pressure = 90.0;
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
    
    // Set family_state with patriarch_age at end age
    let gen = scenario.generation_mechanics.as_ref().unwrap();
    world.family_state = Some(crate::core::FamilyState {
        metrics: [("family:family_influence".to_string(), 60.0)].into(),
        patriarch_age: gen.patriarch_end_age,
    });
    
    let initial_influence = world.family_state.as_ref().unwrap().metrics.get("family:family_influence").copied().unwrap_or(0.0);
    
    // Run tick - should trigger generation transfer
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // Check patriarch_age reset to start age
    assert_eq!(world.family_state.as_ref().unwrap().patriarch_age, gen.patriarch_start_age, "Patriarch age should reset");
    
    // Check family_influence reduced by inheritance coefficient (0.85)
    let final_influence = world.family_state.as_ref().unwrap().metrics.get("family:family_influence").copied().unwrap_or(0.0);
    let expected = initial_influence * 0.85;
    assert!((final_influence - expected).abs() < 0.1, "Family influence should be reduced by inheritance coefficient");
}

#[test]
fn test_initial_family_metrics_loaded() {
    // Load rome_375
    let scenario = registry::load_by_id("rome_375").unwrap();
    let mut state = crate::AppState::default();
    let db = crate::db::Db::open_in_memory().unwrap();
    
    crate::application::load_scenario(&mut state, &db, "rome_375".to_string()).unwrap();
    
    let world_state = state.world_state.as_ref().unwrap();
    assert!(world_state.family_state.is_some(), "family_state should be Some for Rome 375");
    
    let family_state = world_state.family_state.as_ref().unwrap();
    
    // Check that initial metrics are loaded
    assert!(family_state.metrics.contains_key("family:family_influence"), "Should have family_influence");
    assert!(family_state.metrics.contains_key("family:family_knowledge"), "Should have family_knowledge");
    assert!(family_state.metrics.contains_key("family:family_wealth"), "Should have family_wealth");
    assert!(family_state.metrics.contains_key("family:family_connections"), "Should have family_connections");
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
