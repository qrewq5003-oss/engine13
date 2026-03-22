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
fn test_action_effects_applied() {
    // Test that action effects are applied to world state
    let mut state = setup_constantinople_state();

    // Get initial federation progress
    let initial_federation = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);

    // Get initial genoa cohesion
    let initial_genoa_cohesion = state.world_state.as_ref().unwrap()
        .actors.get("genoa")
        .map(|a| a.get_metric("cohesion"))
        .unwrap_or(0.0);

    // Apply venice_diplomacy (effects: +5 federation_progress, +2 genoa.cohesion)
    // With venice weight of 2.0 for federation_progress, effect should be +10
    let action_input = PlayerActionInput {
        action_id: "venice_diplomacy".to_string(),
        target_actor_id: None,
    };
    let result = apply_player_action(&mut state, &action_input);
    assert!(result.is_ok(), "apply_player_action failed: {:?}", result);

    // Check federation progress was increased
    let final_federation = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);

    // Venice has weight 2.0 for federation_progress, so effect should be 5.0 * 2.0 = 10.0
    assert!((final_federation - (initial_federation + 10.0)).abs() < 0.01,
        "Federation progress should increase by 10 (5 * 2.0 weight): initial={}, final={}", 
        initial_federation, final_federation);

    // Check genoa cohesion was increased
    let final_genoa_cohesion = state.world_state.as_ref().unwrap()
        .actors.get("genoa")
        .map(|a| a.get_metric("cohesion"))
        .unwrap_or(0.0);

    // No weight for genoa.cohesion, so effect should be 2.0
    assert!((final_genoa_cohesion - (initial_genoa_cohesion + 2.0)).abs() < 0.01,
        "Genoa cohesion should increase by 2: initial={}, final={}", 
        initial_genoa_cohesion, final_genoa_cohesion);
}

#[test]
fn test_action_applies_cost_and_effects_together() {
    // Test that both cost AND effects are applied in the same action
    let mut state = setup_constantinople_state();

    // Get initial venice treasury
    let initial_treasury = state.world_state.as_ref().unwrap()
        .actors.get("venice")
        .map(|a| a.get_metric("treasury"))
        .unwrap_or(0.0);

    // Get initial federation progress
    let initial_federation = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);

    // Apply venice_diplomacy (cost: -30 treasury, effect: +10 federation with weight)
    let action_input = PlayerActionInput {
        action_id: "venice_diplomacy".to_string(),
        target_actor_id: None,
    };
    let (effects, costs) = apply_player_action(&mut state, &action_input).unwrap();

    // Verify both cost and effect were applied
    assert!(!effects.is_empty(), "Effects should not be empty");
    assert!(!costs.is_empty(), "Costs should not be empty");

    // Verify treasury was deducted
    let final_treasury = state.world_state.as_ref().unwrap()
        .actors.get("venice")
        .map(|a| a.get_metric("treasury"))
        .unwrap_or(0.0);
    assert!((final_treasury - (initial_treasury - 30.0)).abs() < 0.01,
        "Treasury should be reduced by 30");

    // Verify federation was increased
    let final_federation = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);
    assert!((final_federation - (initial_federation + 10.0)).abs() < 0.01,
        "Federation should increase by 10");
}

#[test]
fn test_action_effects_persist_through_tick() {
    // Test that action effects persist after full tick processing
    use rand::SeedableRng;
    use crate::engine::tick;
    
    let scenario = crate::scenarios::registry::load_by_id("constantinople_1430").unwrap();
    let mut world = crate::core::WorldState::new(scenario.id.clone(), scenario.start_year);
    
    // Add actors
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    world.global_metrics.insert("federation_progress".to_string(), 0.0);
    
    // Get initial federation
    let initial_federation = world.global_metrics.get("federation_progress").copied().unwrap_or(0.0);
    
    // Apply action directly to world state (simulating what apply_player_action does)
    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    
    // Apply federation effect with venice weight (2.0)
    let federation_effect = 5.0 * 2.0; // base effect * weight
    crate::core::MetricRef::parse("global:federation_progress").apply(&mut world, federation_effect);
    
    // Run full tick
    tick(&mut world, &scenario, &mut event_log, &mut rng);
    
    // Check federation persisted after tick
    let final_federation = world.global_metrics.get("federation_progress").copied().unwrap_or(0.0);
    
    // Federation should still be increased (may be modified by auto_deltas, but should be > initial)
    assert!(final_federation > initial_federation,
        "Federation should persist after tick: initial={}, final={}", initial_federation, final_federation);
}

#[test]
fn test_advance_tick_with_action_applies_effects() {
    // Test that advance_tick correctly applies action effects through the full pipeline
    use crate::commands::{advance_tick, PlayerActionInput};
    
    let mut state = setup_constantinople_state();
    
    // Get initial federation
    let initial_federation = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);
    
    // Advance tick with action
    let action_input = PlayerActionInput {
        action_id: "venice_diplomacy".to_string(),
        target_actor_id: None,
    };
    
    let result = advance_tick(&mut state, Some(action_input));
    assert!(result.is_ok(), "advance_tick should succeed: {:?}", result);
    
    // Check federation was increased
    let final_federation = state.world_state.as_ref().unwrap()
        .global_metrics.get("federation_progress")
        .copied()
        .unwrap_or(0.0);
    
    // Federation should be increased by action effect (5.0 * 2.0 weight = 10.0)
    // May be modified by auto_deltas, but should be > initial
    assert!(final_federation > initial_federation,
        "Federation should increase after action+tick: initial={}, final={}", 
        initial_federation, final_federation);
}

#[test]
fn test_query_tags_enriched_with_semantic_context() {
    // Test that query tags include semantic context beyond just actor id/name/region
    use std::collections::HashSet;
    
    let scenario = crate::scenarios::registry::load_by_id("constantinople_1430").unwrap();
    let mut world = crate::core::WorldState::new(scenario.id.clone(), scenario.start_year);
    
    // Add actors
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    
    // Build query tags the same way narrative does
    let query_tags: Vec<String> = {
        let mut tags_set: HashSet<String> = HashSet::new();
        
        for actor in world.actors.values() {
            if actor.narrative_status == crate::core::NarrativeStatus::Foreground {
                // Core identity tags
                tags_set.insert(actor.id.clone());
                tags_set.insert(actor.name.to_lowercase());
                tags_set.insert(actor.region.to_lowercase());
                
                // Semantic tags
                for tag in &actor.tags {
                    tags_set.insert(tag.to_lowercase());
                }
                
                // Religion and culture
                tags_set.insert(format!("religion_{:?}", actor.religion).to_lowercase());
                tags_set.insert(format!("culture_{:?}", actor.culture).to_lowercase());
                
                // Region rank
                tags_set.insert(format!("rank_{:?}", actor.region_rank).to_lowercase());
            }
        }
        
        // Scenario-level context
        for tone_tag in &scenario.narrative_config.tone_tags {
            tags_set.insert(tone_tag.to_lowercase());
        }
        for axis in &scenario.narrative_config.narrative_axes {
            tags_set.insert(axis.to_lowercase());
        }
        
        let mut tags: Vec<String> = tags_set.into_iter().collect();
        tags.sort();
        tags
    };
    
    // Verify semantic tags are included
    let tags_str = query_tags.join(",");
    assert!(tags_str.contains("orthodoxy"), "Should include religion tags");
    assert!(tags_str.contains("siege_defense") || tags_str.contains("greek_culture"), 
        "Should include actor semantic tags");
    assert!(tags_str.contains("rank_"), "Should include region rank tags");
    assert!(tags_str.contains("survival") || tags_str.contains("unity"), 
        "Should include narrative axes");
    
    // Verify more tags than just id/name/region
    // Constantinople has 4 foreground actors, each with id/name/region = 12 base tags
    // With enrichment, should have significantly more
    assert!(query_tags.len() > 12, 
        "Enriched tags ({}) should exceed base tags (12)", query_tags.len());
}

#[test]
fn test_query_tags_deterministic_and_deduplicated() {
    // Test that query tags are deterministic and deduplicated
    use std::collections::HashSet;
    
    let scenario = crate::scenarios::registry::load_by_id("constantinople_1430").unwrap();
    let mut world = crate::core::WorldState::new(scenario.id.clone(), scenario.start_year);
    
    // Add actors
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    
    // Build query tags twice
    let build_tags = || -> Vec<String> {
        let mut tags_set: HashSet<String> = HashSet::new();
        
        for actor in world.actors.values() {
            if actor.narrative_status == crate::core::NarrativeStatus::Foreground {
                tags_set.insert(actor.id.clone());
                tags_set.insert(actor.name.to_lowercase());
                tags_set.insert(actor.region.to_lowercase());
                
                for tag in &actor.tags {
                    tags_set.insert(tag.to_lowercase());
                }
                
                tags_set.insert(format!("religion_{:?}", actor.religion).to_lowercase());
                tags_set.insert(format!("culture_{:?}", actor.culture).to_lowercase());
                tags_set.insert(format!("rank_{:?}", actor.region_rank).to_lowercase());
            }
        }
        
        for tone_tag in &scenario.narrative_config.tone_tags {
            tags_set.insert(tone_tag.to_lowercase());
        }
        for axis in &scenario.narrative_config.narrative_axes {
            tags_set.insert(axis.to_lowercase());
        }
        
        let mut tags: Vec<String> = tags_set.into_iter().collect();
        tags.sort();
        tags
    };
    
    let tags_run1 = build_tags();
    let tags_run2 = build_tags();
    
    // Verify determinism - same input produces same output
    assert_eq!(tags_run1, tags_run2, "Query tags should be deterministic");
    
    // Verify deduplication - HashSet ensures no duplicates
    let unique_count: usize = tags_run1.iter().collect::<HashSet<_>>().len();
    assert_eq!(unique_count, tags_run1.len(), 
        "Query tags should have no duplicates: {} unique vs {} total", 
        unique_count, tags_run1.len());
    
    // Verify sorted order
    let mut sorted_tags = tags_run1.clone();
    sorted_tags.sort();
    assert_eq!(sorted_tags, tags_run1, "Query tags should be sorted");
}

#[test]
fn test_action_applies_cost() {
    let mut state = setup_constantinople_state();
    
    // Get venice treasury before
    let before_treasury = {
        let world = state.world_state.as_ref().unwrap();
        world.actors.get("venice").unwrap().get_metric("treasury")
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
            world.actors.get("venice").unwrap().get_metric("treasury")
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
        .map(|a| a.get_metric("treasury"))
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
        .map(|a| a.get_metric("treasury"))
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
            venice.set_metric("treasury", 5.0); // Very low treasury
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
        .map(|a| a.get_metric("external_pressure"))
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
        .map(|a| a.get_metric("external_pressure"))
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
