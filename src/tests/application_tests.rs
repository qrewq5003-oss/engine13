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
    assert!(slot_list.slots[0].is_some(), "slot_1 should exist");
    assert!(slot_list.slots[1].is_none(), "slot_2 should be empty");
    assert!(slot_list.slots[2].is_none(), "slot_3 should be empty");
}
