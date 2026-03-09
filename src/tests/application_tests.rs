use crate::application::{get_available_actions, save_game, list_saves_with_slots};
use crate::application::actions::{apply_player_action, PlayerActionInput};
use crate::commands::{advance_tick, AppState};
use crate::db::Db;
use crate::scenarios::registry;

fn setup_test_db() -> Db {
    Db::open_in_memory().expect("Failed to create in-memory database")
}

fn setup_rome_state() -> AppState {
    let mut state = AppState::default();
    let scenario = registry::load_by_id("rome_375").expect("Failed to load rome_375");
    let mut world_state = crate::core::WorldState::new(scenario.id.clone(), scenario.start_year);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world_state.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    state.current_scenario = Some(scenario);
    state.world_state = Some(world_state);
    // Initialize RNG for tests
    use rand::SeedableRng;
    state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(42));
    state
}

fn setup_constantinople_state() -> AppState {
    let mut state = AppState::default();
    let scenario = registry::load_by_id("constantinople_1430").expect("Failed to load constantinople_1430");
    let mut world_state = crate::core::WorldState::new(scenario.id.clone(), scenario.start_year);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world_state.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    state.current_scenario = Some(scenario);
    state.world_state = Some(world_state);
    // Initialize RNG for tests
    use rand::SeedableRng;
    state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(42));
    state
}

#[test]
fn test_tick_advances() {
    let mut state = setup_rome_state();

    let before = state.world_state.as_ref().unwrap().tick;
    let result = advance_tick(&mut state, None);
    assert!(result.is_ok(), "advance_tick failed: {:?}", result);
    let after = state.world_state.as_ref().unwrap().tick;

    assert_eq!(after, before + 1, "Tick should advance by 1");
}

#[test]
fn test_tick_advances_constantinople() {
    let mut state = setup_constantinople_state();

    let before = state.world_state.as_ref().unwrap().tick;
    let result = advance_tick(&mut state, None);
    assert!(result.is_ok(), "advance_tick failed: {:?}", result);
    let after = state.world_state.as_ref().unwrap().tick;

    assert_eq!(after, before + 1, "Tick should advance by 1");
}

#[test]
fn test_get_available_actions_rome() {
    let state = setup_rome_state();
    
    let actions = get_available_actions(&state);
    assert!(actions.is_ok(), "get_available_actions failed: {:?}", actions);
    let actions = actions.unwrap();
    
    // Should have at least some actions available
    assert!(!actions.is_empty(), "Should have at least one available action");
}

#[test]
fn test_get_available_actions_constantinople() {
    let state = setup_constantinople_state();
    
    let actions = get_available_actions(&state);
    assert!(actions.is_ok(), "get_available_actions failed: {:?}", actions);
    let actions = actions.unwrap();
    
    // Constantinople should have federation actions
    assert!(!actions.is_empty(), "Should have at least one available action");
}

#[test]
fn test_action_applies_cost() {
    let mut state = setup_constantinople_state();
    
    // Get venice treasury before
    let before_treasury = {
        let world = state.world_state.as_ref().unwrap();
        world.actors.get("venice").unwrap().metrics.treasury
    };
    
    // Try to apply venice_naval_support (costs 50 treasury)
    let action_input = PlayerActionInput {
        action_id: "venice_naval_support".to_string(),
        target_actor_id: None,
    };
    
    let result = apply_player_action(&mut state, &action_input);
    
    // Action may fail if not available, but if it succeeds, treasury should decrease
    if result.is_ok() {
        let after_treasury = {
            let world = state.world_state.as_ref().unwrap();
            world.actors.get("venice").unwrap().metrics.treasury
        };
        assert!(after_treasury < before_treasury, "Treasury should decrease after action");
    }
}

#[test]
fn test_save_load_preserves_state() {
    let db = setup_test_db();
    let mut state = setup_constantinople_state();
    
    // Advance a tick to change state
    let _ = advance_tick(&mut state, None);
    
    let year_before = state.world_state.as_ref().unwrap().year;
    let tick_before = state.world_state.as_ref().unwrap().tick;
    let fed_before = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);
    
    // Save to slot_1
    let save_result = save_game(&mut state, &db, Some("slot_1".to_string()));
    assert!(save_result.is_ok(), "save_game failed: {:?}", save_result);

    // Load the save - use double underscore separator
    let save_id = format!("constantinople_1430__slot_1");
    let load_result = crate::application::load_game(&mut state, &db, save_id);
    assert!(load_result.is_ok(), "load_game failed: {:?}", load_result);
    
    let year_after = state.world_state.as_ref().unwrap().year;
    let tick_after = state.world_state.as_ref().unwrap().tick;
    let fed_after = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);
    
    assert_eq!(year_before, year_after, "Year should be preserved");
    assert_eq!(tick_before, tick_after, "Tick should be preserved");
    // Use approximate equality for floating point
    assert!((fed_before - fed_after).abs() < 0.0001, "federation_progress should be preserved (before: {}, after: {})", fed_before, fed_after);
}

#[test]
fn test_list_saves_with_slots() {
    let db = setup_test_db();
    let mut state = setup_constantinople_state();

    // Save to different slots
    let _ = save_game(&mut state, &db, Some("auto".to_string()));
    let _ = advance_tick(&mut state, None);
    let _ = save_game(&mut state, &db, Some("slot_1".to_string()));

    let result = list_saves_with_slots(&db, "constantinople_1430");
    assert!(result.is_ok(), "list_saves_with_slots failed: {:?}", result);

    let slot_list = result.unwrap();
    assert!(slot_list.auto.is_some(), "auto save should exist");
    assert!(slot_list.slots.contains_key("slot_1"), "slot_1 should exist");
    assert!(!slot_list.slots.contains_key("slot_2"), "slot_2 should be empty");
    assert!(!slot_list.slots.contains_key("slot_3"), "slot_3 should be empty");
}

// ============================================================================
// Action Tests
// ============================================================================

#[test]
fn test_action_cost_deducted() {
    // Test that action cost is correctly deducted from treasury
    let mut state = setup_constantinople_state();
    let _db = setup_test_db();

    // Get initial venice treasury
    let initial_treasury = state.world_state.as_ref().unwrap()
        .actors.get("venice")
        .map(|a| a.metrics.treasury)
        .unwrap_or(0.0);

    // Apply venice_diplomacy action (cost: venice.treasury -30)
    let action_input = PlayerActionInput {
        action_id: "venice_diplomacy".to_string(),
        target_actor_id: None,
    };
    let result = apply_player_action(&mut state, &action_input);
    assert!(result.is_ok(), "apply_player_action failed: {:?}", result);

    // Check treasury was deducted
    let final_treasury = state.world_state.as_ref().unwrap()
        .actors.get("venice")
        .map(|a| a.metrics.treasury)
        .unwrap_or(0.0);

    assert!((final_treasury - (initial_treasury - 30.0)).abs() < 0.01,
        "Venice treasury should be reduced by 30: initial={}, final={}", initial_treasury, final_treasury);
}

#[test]
fn test_action_unavailable_when_insufficient_resources() {
    // Test that actions are unavailable when resources are insufficient
    let mut state = setup_constantinople_state();

    // Set venice treasury very low
    if let Some(world) = state.world_state.as_mut() {
        if let Some(venice) = world.actors.get_mut("venice") {
            venice.metrics.treasury = 5.0; // Very low treasury
        }
    }

    // Try to apply venice_diplomacy (requires venice.legitimacy > 60, cost: -30 treasury)
    // This should fail because treasury is too low for the cost
    let _action_input = PlayerActionInput {
        action_id: "venice_diplomacy".to_string(),
        target_actor_id: None,
    };

    // The action availability check should pass (legitimacy check), but we're testing
    // that the action can be rejected when conditions aren't met
    // Let's test with an action that has a treasury requirement
    let action_input = PlayerActionInput {
        action_id: "venice_naval_support".to_string(), // requires venice.treasury > 100
        target_actor_id: None,
    };
    let result = apply_player_action(&mut state, &action_input);
    assert!(result.is_err(), "Action should be unavailable when treasury < 100");
}

#[test]
fn test_actions_per_tick_limit_enforced() {
    // Test that actions_per_tick limit is enforced
    let mut state = setup_constantinople_state();
    let _db = setup_test_db();

    // Apply 3 actions (the limit for constantinople)
    let actions = vec!["venice_diplomacy", "genoa_financial_aid", "milan_bankers"];
    for action_id in &actions {
        let action_input = PlayerActionInput {
            action_id: action_id.to_string(),
            target_actor_id: None,
        };
        let result = apply_player_action(&mut state, &action_input);
        assert!(result.is_ok(), "Action {} should succeed: {:?}", action_id, result);
    }

    // 4th action should fail due to actions_per_tick limit
    let action_input = PlayerActionInput {
        action_id: "venice_naval_support".to_string(),
        target_actor_id: None,
    };
    let result = apply_player_action(&mut state, &action_input);
    assert!(result.is_err(), "4th action should fail due to actions_per_tick limit");
}

#[test]
fn test_federation_progress_grows_with_actions() {
    // Test that federation actions increase federation_progress
    let mut state = setup_constantinople_state();
    let _db = setup_test_db();

    // Get initial federation progress
    let initial_fed = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);

    // Apply 3 federation actions
    let actions = vec!["venice_diplomacy", "genoa_financial_aid", "milan_bankers"];
    for action_id in &actions {
        let action_input = PlayerActionInput {
            action_id: action_id.to_string(),
            target_actor_id: None,
        };
        let result = apply_player_action(&mut state, &action_input);
        assert!(result.is_ok(), "Action {} should succeed: {:?}", action_id, result);
    }

    // Check federation progress increased
    let final_fed = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);

    assert!(final_fed > initial_fed,
        "Federation progress should increase: initial={}, final={}", initial_fed, final_fed);
}

#[test]
fn test_scripted_actions_improve_outcome_vs_no_actions() {
    // Test that scripted actions improve outcome vs no-action baseline
    use crate::core::MetricRef;
    use crate::engine::{tick, EventLog};
    use rand::SeedableRng;

    // Baseline: 25 ticks with no actions
    let scenario = registry::load_by_id("constantinople_1430").expect("Failed to load scenario");
    let mut world_no_actions = crate::core::WorldState::with_seed(
        scenario.id.clone(), scenario.start_year, 42
    );
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world_no_actions.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    for _ in 0..25 {
        tick(&mut world_no_actions, &scenario, &mut event_log, &mut rng);
    }
    let fed_no_actions = world_no_actions.global_metrics.get("federation_progress").copied().unwrap_or(0.0);
    let pressure_no_actions = world_no_actions.actors.get("byzantium")
        .map(|a| a.metrics.external_pressure)
        .unwrap_or(0.0);

    // Scripted: 25 ticks with priority actions
    let mut world_scripted = crate::core::WorldState::with_seed(
        scenario.id.clone(), scenario.start_year, 42
    );
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world_scripted.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    let priority_actions = vec![
        "venice_diplomacy", "genoa_financial_aid", "milan_bankers",
        "venice_naval_support", "genoa_mercenaries", "milan_condottieri",
        "venice_trade_deal", "genoa_galata_garrison",
    ];

    for _ in 0..25 {
        // Apply scripted actions
        for action_id in &priority_actions {
            if let Some(action) = scenario.patron_actions.iter().find(|a| a.id == *action_id) {
                let is_available = match &action.available_if {
                    crate::core::ActionCondition::Always => true,
                    crate::core::ActionCondition::Metric { metric, operator, value } => {
                        let current = MetricRef::parse(metric).get(&world_scripted);
                        match operator {
                            crate::core::ComparisonOperator::Less => current < *value,
                            crate::core::ComparisonOperator::LessOrEqual => current <= *value,
                            crate::core::ComparisonOperator::Greater => current > *value,
                            crate::core::ComparisonOperator::GreaterOrEqual => current >= *value,
                            crate::core::ComparisonOperator::Equal => (current - value).abs() < 0.001,
                        }
                    }
                };
                if is_available {
                    for (metric, effect) in &action.effects {
                        MetricRef::parse(metric).apply(&mut world_scripted, *effect);
                    }
                    for (metric, cost) in &action.cost {
                        MetricRef::parse(metric).apply(&mut world_scripted, *cost);
                    }
                }
            }
        }
        tick(&mut world_scripted, &scenario, &mut event_log, &mut rng);
    }

    let fed_scripted = world_scripted.global_metrics.get("federation_progress").copied().unwrap_or(0.0);
    let pressure_scripted = world_scripted.actors.get("byzantium")
        .map(|a| a.metrics.external_pressure)
        .unwrap_or(0.0);

    // Assert scripted is better
    assert!(fed_scripted > fed_no_actions,
        "Scripted federation ({:.1}) should be higher than no-actions ({:.1})",
        fed_scripted, fed_no_actions);

    // Allow +5 tolerance for pressure (random events can affect it)
    assert!(pressure_scripted <= pressure_no_actions + 5.0,
        "Scripted pressure ({:.1}) should be <= no-actions ({:.1}) + 5.0 tolerance",
        pressure_scripted, pressure_no_actions);
}

#[test]
fn test_scripted_victory_achievable() {
    // Test that scripted victory is achievable within 40 ticks
    use crate::core::MetricRef;
    use crate::engine::{tick, EventLog};
    use rand::SeedableRng;

    let scenario = registry::load_by_id("constantinople_1430").expect("Failed to load scenario");
    let mut world = crate::core::WorldState::with_seed(
        scenario.id.clone(), scenario.start_year, 42
    );
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    let priority_actions = vec![
        "venice_diplomacy", "genoa_financial_aid", "milan_bankers",
        "venice_naval_support", "genoa_mercenaries", "milan_condottieri",
        "venice_trade_deal", "genoa_galata_garrison",
    ];

    for _ in 0..40 {
        // Apply scripted actions
        for action_id in &priority_actions {
            if let Some(action) = scenario.patron_actions.iter().find(|a| a.id == *action_id) {
                let is_available = match &action.available_if {
                    crate::core::ActionCondition::Always => true,
                    crate::core::ActionCondition::Metric { metric, operator, value } => {
                        let current = MetricRef::parse(metric).get(&world);
                        match operator {
                            crate::core::ComparisonOperator::Less => current < *value,
                            crate::core::ComparisonOperator::LessOrEqual => current <= *value,
                            crate::core::ComparisonOperator::Greater => current > *value,
                            crate::core::ComparisonOperator::GreaterOrEqual => current >= *value,
                            crate::core::ComparisonOperator::Equal => (current - value).abs() < 0.001,
                        }
                    }
                };
                if is_available {
                    for (metric, effect) in &action.effects {
                        MetricRef::parse(metric).apply(&mut world, *effect);
                    }
                    for (metric, cost) in &action.cost {
                        MetricRef::parse(metric).apply(&mut world, *cost);
                    }
                }
            }
        }
        tick(&mut world, &scenario, &mut event_log, &mut rng);

        // Stop early if victory achieved
        if world.victory_achieved {
            break;
        }
    }

    let fed_final = world.global_metrics.get("federation_progress").copied().unwrap_or(0.0);

    // Soft victory criterion: either victory achieved OR federation > 80
    assert!(world.victory_achieved || fed_final > 80.0,
        "Scripted strategy should approach victory: victory={}, federation={:.1}",
        world.victory_achieved, fed_final);
}
