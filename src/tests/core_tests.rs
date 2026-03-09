use crate::core::{MetricRef, WorldState};
use crate::scenarios::registry;

#[test]
fn test_metric_ref_parse_actor() {
    let mr = MetricRef::parse("venice.treasury");
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
    let mr = MetricRef::parse("family_influence");
    match mr {
        MetricRef::Family { key } => {
            assert_eq!(key, "family_influence");
        }
        _ => panic!("Expected Family variant"),
    }
}

#[test]
fn test_metric_ref_parse_global() {
    let mr = MetricRef::parse("federation_progress");
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
    
    let before = MetricRef::parse("venice.treasury").get(&world);
    MetricRef::parse("venice.treasury").apply(&mut world, -100.0);
    let after = MetricRef::parse("venice.treasury").get(&world);
    
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
    
    MetricRef::parse("venice.treasury").apply(&mut world, -100.0);
    let after = MetricRef::parse("venice.treasury").get(&world);
    
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
