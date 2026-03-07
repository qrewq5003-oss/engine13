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
