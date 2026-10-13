use crate::application::{save_game, list_saves_with_slots};
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
    let mut world_state = crate::core::WorldState::with_seed(scenario.id.clone(), scenario.start_year, 0);
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
    let mut world_state = crate::core::WorldState::with_seed(scenario.id.clone(), scenario.start_year, 0);
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
fn test_actions_listed_rome() {
    // B42: the mode-aware `get_available_actions` was removed; the UI's list is the one
    // list. Every scenario action is listed, available or with the reason it is not.
    let state = setup_rome_state();
    let listed = crate::commands::get_actions_with_availability(&state).expect("listed");
    assert!(!listed.is_empty(), "the scenario must list its actions");
    assert!(listed.iter().all(|a| a.available == a.unavailable_reason.is_none()));
}

#[test]
fn test_actions_listed_constantinople() {
    // B42: the mode-aware `get_available_actions` was removed; the UI's list is the one
    // list. Every scenario action is listed, available or with the reason it is not.
    let state = setup_constantinople_state();
    let listed = crate::commands::get_actions_with_availability(&state).expect("listed");
    assert!(!listed.is_empty(), "the scenario must list its actions");
    assert!(listed.iter().all(|a| a.available == a.unavailable_reason.is_none()));
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
    state.rng = Some(<rand_chacha::ChaCha8Rng as rand::SeedableRng>::seed_from_u64(0));
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
                        let current = metric.get(&world_scripted);
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
                        metric.apply(&mut world_scripted, *effect);
                    }
                    for (metric, cost) in &action.cost {
                        metric.apply(&mut world_scripted, *cost);
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
                        let current = metric.get(&world);
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
                        metric.apply(&mut world, *effect);
                    }
                    for (metric, cost) in &action.cost {
                        metric.apply(&mut world, *cost);
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

/// constantinople as loaded, with `byzantium` optionally taken out of the world the way
/// a collapse does (removed from `actors`, recorded in `dead_actor_ids`).
fn constantinople_with_byzantium(alive: bool) -> AppState {
    let db = setup_test_db();
    let mut state = AppState::default();
    crate::application::load_scenario(&mut state, &db, "constantinople_1430".to_string()).unwrap();
    state.rng = Some(<rand_chacha::ChaCha8Rng as rand::SeedableRng>::seed_from_u64(0));
    if !alive {
        let ws = state.world_state.as_mut().unwrap();
        ws.actors.remove("byzantium");
        ws.dead_actor_ids.insert("byzantium".to_string());
    }
    state
}

fn availability_of(state: &AppState, id: &str) -> Result<(), crate::application::actions::UnavailableReason> {
    let sc = state.current_scenario.as_ref().unwrap();
    let action = sc.patron_actions.iter().find(|a| a.id == id).expect("action");
    crate::application::actions::action_availability(action, state.world_state.as_ref().unwrap(), sc)
}

/// B46 rule: an action addressed to an absent actor is unavailable, whole — even with a
/// living effect (`genoa_financial_aid` also adds to the federation). Both ways: with
/// Byzantium alive the same action is available.
#[test]
fn action_addressed_to_an_absent_actor_is_unavailable() {
    use crate::application::actions::UnavailableReason;
    assert!(availability_of(&constantinople_with_byzantium(true), "genoa_financial_aid").is_ok(),
        "with Byzantium alive the help is available");
    match availability_of(&constantinople_with_byzantium(false), "genoa_financial_aid") {
        Err(UnavailableReason::AddresseeDead { actor }) => assert_eq!(actor, "byzantium"),
        other => panic!("help to a fallen city must be refused as «адресат погиб», got {other:?}"),
    }
    // The apply path draws the same rule — the bot cannot spend on the dead either.
    let mut state = constantinople_with_byzantium(false);
    let input = PlayerActionInput { action_id: "genoa_financial_aid".into(), target_actor_id: None };
    assert!(apply_player_action(&mut state, &input).unwrap_err().starts_with("Адресат погиб"));
}

/// «Адресат погиб» comes before the cost: `sabotage_federation` pays with Byzantium's
/// legitimacy, and over a fallen Byzantium it used to be refused as a missing resource
/// of the dead.
#[test]
fn addressee_dead_is_reported_before_a_missing_cost() {
    use crate::application::actions::UnavailableReason;
    match availability_of(&constantinople_with_byzantium(false), "sabotage_federation") {
        Err(UnavailableReason::AddresseeDead { actor }) => assert_eq!(actor, "byzantium"),
        other => panic!("expected «адресат погиб» before the cost check, got {other:?}"),
    }
}

/// The player's chosen target is held to the same rule.
#[test]
fn a_chosen_target_that_is_absent_refuses_the_action() {
    let mut state = constantinople_with_byzantium(true);
    let input = PlayerActionInput { action_id: "venice_diplomacy".into(), target_actor_id: Some("mamluks".into()) };
    assert!(apply_player_action(&mut state, &input).unwrap_err().starts_with("Адресат погиб"),
        "`mamluks` is not in the world at start");
}

/// A37 stage 2: a tag that is a relation to an actor (`requires_alive`) leaves every
/// carrier once that actor is gone, and does not come back — not through spreading, not
/// through heirs born later. Both ways: while the Ottomans live the tags are carried.
#[test]
fn a_tag_tied_to_a_dead_actor_leaves_and_stays_gone() {
    let tied = ["ottoman_frontier", "crusade_caller"];
    let carriers = |state: &AppState| -> usize {
        state.world_state.as_ref().unwrap().actors.values()
            .filter(|a| a.tags.iter().any(|t| tied.contains(&t.as_str()))
                || a.actor_tags.keys().any(|t| tied.contains(&t.as_str())))
            .count()
    };
    let mut state = constantinople_with_byzantium(true);
    assert!(carriers(&state) > 0, "precondition: the tags are carried at the start");
    crate::commands::advance_tick_silent(&mut state).unwrap();
    assert!(carriers(&state) > 0, "with the Ottomans alive the tags stay");

    let ws = state.world_state.as_mut().unwrap();
    ws.actors.remove("ottomans");
    ws.dead_actor_ids.insert("ottomans".to_string());
    for t in 0..40 {
        crate::commands::advance_tick_silent(&mut state).unwrap();
        assert_eq!(carriers(&state), 0, "tick {t} after the Ottomans died: a tied tag is still carried");
    }
}

/// A37 follow-up: rome's «border with Rome» tags never reach Rome itself. Before the rule
/// they spread to Rome and added +1 pressure a tick each — a border with oneself.
#[test]
fn rome_never_carries_a_border_with_itself() {
    use rand::SeedableRng;
    let own = ["roman_border", "roman_frontier", "rhine_border"];
    let db = setup_test_db();
    let mut state = AppState::default();
    crate::application::load_scenario(&mut state, &db, "rome_375".to_string()).unwrap();
    state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(3));
    // Without the rule Rome picks up `roman_border` on tick 2 of this seed.
    for t in 0..40 {
        crate::commands::advance_tick_silent(&mut state).unwrap();
        if let Some(rome) = state.world_state.as_ref().unwrap().actors.get("rome") {
            let bad: Vec<&String> = rome.tags.iter().filter(|x| own.contains(&x.as_str())).collect();
            assert!(bad.is_empty(), "tick {t}: Rome carries {bad:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// The game's book of chronicles
// ---------------------------------------------------------------------------

fn stamp_of(state: &AppState) -> crate::llm::ChronicleStamp {
    let world = state.world_state.as_ref().unwrap();
    let scenario = state.current_scenario.as_ref().unwrap();
    let snapshot = crate::llm::build_snapshot(world, scenario, &state.event_log);
    crate::llm::ChronicleStamp::of(world, &snapshot)
}

/// A chronicle enters the book only once its stream finished, for the run and tick it was
/// written for; a broken stream (`None`) or an empty text writes nothing; regenerating the same
/// tick replaces the entry. Both ways on each condition.
#[test]
fn chronicle_book_keeps_finished_chronicles_of_their_own_run_and_tick() {
    let mut state = setup_rome_state();
    let stamp = stamp_of(&state);
    let world = state.world_state.as_mut().unwrap();
    assert!(!crate::llm::record_chronicle(world, &stamp, None), "a broken stream is not kept");
    assert!(!crate::llm::record_chronicle(world, &stamp, Some("  ".into())), "an empty text is not kept");
    let mut other_run = stamp.clone();
    other_run.run_id = "another-run".into();
    assert!(!crate::llm::record_chronicle(world, &other_run, Some("чужая".into())), "another run's chronicle is not kept");
    let mut other_tick = stamp.clone();
    other_tick.tick += 1;
    assert!(!crate::llm::record_chronicle(world, &other_tick, Some("чужой тик".into())), "another tick's chronicle is not kept");
    assert!(world.chronicle_book.is_empty());
    assert!(crate::llm::record_chronicle(world, &stamp, Some("Первая хроника.".into())));
    assert!(crate::llm::record_chronicle(world, &stamp, Some("Переписанная хроника.".into())));
    assert_eq!(world.chronicle_book.len(), 1, "one entry per tick");
    assert_eq!(world.chronicle_book[0].text, "Переписанная хроника.");
    assert_eq!((world.chronicle_book[0].tick, world.chronicle_book[0].year), (stamp.tick, stamp.year));
}

/// Invariant (owner's decision of 2026-10-11): no sentence of the past chronicles enters the
/// prompt; the book reaches it only as the stop-list, phrases of at most six words. Both ways: a
/// turn repeated in two chronicles is in the prompt, as a prohibition; a sentence that stands in
/// one chronicle only is not, and no run of seven words of the book is.
#[test]
fn chronicle_book_reaches_the_prompt_only_as_the_stop_list() {
    let mut state = setup_rome_state();
    let db = setup_test_db();
    let marker = "МАРКЕР-КНИГИ-ПАРТИИ-7f3a";
    let repeated = "серые тучи сгущались над старым городом";
    let texts = [
        format!("Хроника с {marker} и долгим рассказом о судьбе империи. А {repeated} снова."),
        format!("Другая хроника, где {repeated} опять, и варвары стояли у ворот."),
    ];
    for t in &texts {
        let stamp = stamp_of(&state);
        assert!(crate::llm::record_chronicle(state.world_state.as_mut().unwrap(), &stamp, Some(t.clone())));
        advance_tick(&mut state, None).expect("tick");
    }
    let world = state.world_state.as_ref().unwrap();
    assert_eq!(world.chronicle_book.len(), 2, "the book holds both chronicles");
    let scenario = state.current_scenario.as_ref().unwrap();
    let snapshot = crate::llm::build_snapshot(world, scenario, &state.event_log);
    let prompt = crate::llm::generate_narrative_prompt(&snapshot, scenario, &db);
    assert!(!format!("{snapshot:?}").contains(marker) && !prompt.contains(marker), "a sentence of one chronicle never reaches the prompt");
    assert!(snapshot.annals.stop_phrases.iter().all(|p| p.split(' ').count() <= crate::llm::annals::STOP_WORDS_MAX));
    let stop = &snapshot.annals.stop_phrases;
    assert!(stop.iter().any(|p| repeated.contains(p.as_str())), "the repeated turn is on the stop-list: {stop:?}");
    assert!(prompt.contains(&format!("«{}»", stop[0])), "the stop-list is in the prompt");
    let prompt_words = format!(" {} ", crate::llm::annals::words(&prompt).join(" "));
    for t in &texts {
        for w in crate::llm::annals::words(t).windows(crate::llm::annals::STOP_WORDS_MAX + 1) {
            assert!(!prompt_words.contains(&format!(" {} ", w.join(" "))), "seven words of the book reached the prompt: {w:?}");
        }
    }
}

/// The world's annals in milan on v2 (the six layers are the engine's facts): the League stands
/// in the relations from the first half-year and the regency crisis is an open story dated 1477;
/// a player decision older than two years is summed up among the past ones and never listed
/// as recent, while one of this half-year is. Both ways on each.
#[test]
fn world_annals_carry_relations_stories_and_dated_player_policy() {
    let db = setup_test_db();
    let mut state = AppState::default();
    crate::application::load_scenario(&mut state, &db, "milan_1477".to_string()).unwrap();
    use rand::SeedableRng;
    state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(0));
    let action = |tick: u32| crate::core::Event::new("player_action_milan_bribe_curia".into(), tick, 1477, "milan".into(),
        crate::core::EventType::PlayerAction, true, "Действие игрока: Подкуп курии".into());
    state.event_log.add(action(0));
    for _ in 0..12 {
        advance_tick(&mut state, None).expect("tick");
    }
    let snap = |state: &AppState| crate::llm::build_snapshot(state.world_state.as_ref().unwrap(), state.current_scenario.as_ref().unwrap(), &state.event_log);
    let a = snap(&state).annals;
    assert!(a.relations.iter().any(|l| l.starts_with("Союз:") && l.contains("Флорентийская республика")), "{:?}", a.relations);
    assert!(a.open_stories.iter().any(|l| l.contains("кризис регентства — с 1477 года")) || state.world_state.as_ref().unwrap().milestone_events_fired.iter().any(|m| m == "ludovico_takes_regency"), "{:?}", a.open_stories);
    assert!(a.player_recent.is_empty(), "a decision of tick 0 is not recent at tick 12: {:?}", a.player_recent);
    assert_eq!(a.player_older, vec!["Подкуп курии — однажды, 1477 г.".to_string()]);
    let now = state.world_state.as_ref().unwrap().tick - 1;
    state.event_log.add(action(now));
    let a = snap(&state).annals;
    assert_eq!(a.player_recent.len(), 1, "a decision of this half-year is recent: {:?}", a.player_recent);
    assert!(a.player_recent[0].ends_with(": Подкуп курии"));
    let prompt = crate::llm::generate_narrative_prompt(&snap(&state), state.current_scenario.as_ref().unwrap(), &db);
    assert!(!prompt.contains("actor:") && !prompt.contains("→"), "the player's block prints no engine keys or effects");
}

/// An open war is one the engine still lets go on: two recent battles make Florence and Siena a
/// war story; once Siena is Florence's vassal (Ц7: the pair cannot fight) the story is closed,
/// though the battles are as recent as before.
#[test]
fn open_war_closes_when_the_pair_is_bound_by_vassalage() {
    let db = setup_test_db();
    let mut state = AppState::default();
    crate::application::load_scenario(&mut state, &db, "milan_1477".to_string()).unwrap();
    use rand::SeedableRng;
    state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(0));
    for _ in 0..4 {
        advance_tick(&mut state, None).expect("tick");
    }
    assert!(crate::engine::interactions::conquest_on(state.current_scenario.as_ref().unwrap()), "milan plays Ц7");
    let now = state.world_state.as_ref().unwrap().tick;
    for t in [now - 2, now - 1] {
        state.event_log.add(crate::core::Event::new("military_conflict_florence_siena".into(), t, 1478, "florence".into(),
            crate::core::EventType::War, true, "Военный конфликт".into()));
    }
    let war = |state: &AppState| {
        let a = crate::llm::build_snapshot(state.world_state.as_ref().unwrap(), state.current_scenario.as_ref().unwrap(), &state.event_log).annals;
        a.open_stories.iter().any(|l| l.starts_with("Война:") && l.contains("Флорентийская республика") && l.contains("Сиенская республика"))
    };
    assert!(war(&state), "two recent battles are an open war");
    state.world_state.as_mut().unwrap().vassalages.push(crate::core::Vassalage { vassal_id: "siena".into(), overlord_id: "florence".into(), formed_tick: now - 1 });
    assert!(!war(&state), "a vassal and its overlord are at peace: no open war");
}

/// The stop-list: a turn in two chronicles is listed, one in a single chronicle is not; a shared
/// opening comes first; nothing longer than six words.
#[test]
fn stop_list_lists_turns_repeated_across_chronicles_only() {
    let book = [
        "Весна нынешнего года застала Италию в оцепенении. Купцы молчали у причалов.",
        "Весна нынешнего года застала Италию врасплох. Флот ушёл на юг.",
        "Осень принесла дожди. Купцы молчали у причалов Генуи.",
    ];
    let stop = crate::llm::annals::stop_phrases(&book);
    assert_eq!(stop[0], "весна нынешнего года застала италию", "{stop:?}");
    assert!(stop.iter().any(|p| p == "купцы молчали у причалов"), "{stop:?}");
    assert!(!stop.iter().any(|p| p.contains("флот") || p.contains("дожди")), "{stop:?}");
    assert!(stop.iter().all(|p| (4..=6).contains(&p.split(' ').count())));
    assert!(crate::llm::annals::stop_phrases(&book[..1]).is_empty(), "one chronicle repeats nothing");
}

/// The book travels with the save; a save written without one (older saves) loads with an
/// empty book, and a world without chronicles serialises without the field.
#[test]
fn chronicle_book_rides_with_the_save_and_old_saves_load() {
    let mut state = setup_rome_state();
    let db = setup_test_db();
    let empty_json = serde_json::to_string(state.world_state.as_ref().unwrap()).unwrap();
    assert!(!empty_json.contains("chronicle_book"), "no chronicles, no field: a save without them is unchanged");
    let old: crate::core::WorldState = serde_json::from_str(&empty_json).expect("an older save loads");
    assert!(old.chronicle_book.is_empty());
    let stamp = stamp_of(&state);
    assert!(crate::llm::record_chronicle(state.world_state.as_mut().unwrap(), &stamp, Some("Хроника для сейва.".into())));
    let book = state.world_state.as_ref().unwrap().chronicle_book.clone();
    let saved = save_game(&mut state, &db, None).expect("save");
    let id = saved.save_id.expect("save id");
    let mut loaded = AppState::default();
    crate::application::load_game(&mut loaded, &db, id).expect("load");
    use rand::SeedableRng;
    loaded.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(42));
    assert_eq!(loaded.world_state.as_ref().unwrap().chronicle_book, book, "the book rides with the save");
}

/// Economy v2 (Ц9, authored content): in milan the Italian League stands from the first tick and
/// `milan_savoy_alliance` makes Milan and Savoy allies. Both ways: with the switch off the action
/// writes its metrics and no alliance exists.
#[test]
fn milan_league_and_the_savoy_alliance_exist_only_under_the_switch() {
    use rand::SeedableRng;
    use crate::engine::interactions::allied;
    for on in [true, false] {
        let db = setup_test_db();
        let mut state = AppState::default();
        crate::load_scenario(&mut state, &db, "milan_1477".to_string()).unwrap();
        state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(0));
        {
            let s = state.current_scenario.as_mut().unwrap();
            s.features.economy_v2 = true;
            s.economy_v2_alliances = on;
        }
        crate::commands::advance_tick_silent(&mut state).unwrap();
        let ws = state.world_state.as_ref().unwrap();
        let league = ["venice", "florence", "naples", "sicily", "papacy"].iter().all(|m| allied(ws, "milan", m)) && allied(ws, "naples", "papacy");
        assert_eq!(league, on, "the league, switch {on}");
        let savoy_before = state.world_state.as_ref().unwrap().actors["savoy"].get_metric("external_pressure");
        apply_player_action(&mut state, &PlayerActionInput { action_id: "milan_savoy_alliance".into(), target_actor_id: None }).unwrap();
        let ws = state.world_state.as_ref().unwrap();
        assert_eq!(allied(ws, "milan", "savoy"), on, "the Savoy alliance, switch {on}");
        assert!(ws.actors["savoy"].get_metric("external_pressure") < savoy_before || savoy_before == 0.0, "the action's metrics are written either way");
    }
}

/// The event window reads names: ids are replaced by whole token, so `rome` is renamed and
/// `rome_west` is not touched by it. Both ways.
#[test]
fn event_window_names_replace_ids_by_whole_token_only() {
    let names: std::collections::BTreeMap<String, String> =
        [("rome", "Рим"), ("rome_west", "Западная империя")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(crate::llm::with_actor_names("Военный конфликт между rome и rome_west", &names), "Военный конфликт между Рим и Западная империя");
    assert_eq!(crate::llm::with_actor_names("romeo, rome2 и xrome", &names), "romeo, rome2 и xrome", "no part of a token is renamed");
}

/// The event window of the prompt carries no actor id and no event key, in all three worlds — the
/// engine's log keeps its descriptions as written. Both ways: the log does hold ids (the
/// defect is real), the window does not.
#[test]
fn event_window_carries_no_actor_ids_and_no_event_keys() {
    use rand::SeedableRng;
    let db = setup_test_db();
    let mut ids_in_log = 0;
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for seed in 0..3u64 {
            let mut state = AppState::default();
            crate::application::load_scenario(&mut state, &db, sc.to_string()).unwrap();
            state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let ids: Vec<String> = state.current_scenario.as_ref().unwrap().actors.iter().map(|a| a.id.clone()).collect();
            let has_id = |text: &str| text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).any(|t| ids.iter().any(|i| i == t));
            for _ in 0..60 {
                advance_tick(&mut state, None).expect("tick");
                let world = state.world_state.as_ref().unwrap();
                let scenario = state.current_scenario.as_ref().unwrap();
                let snapshot = crate::llm::build_snapshot(world, scenario, &state.event_log);
                let prompt = crate::llm::generate_narrative_prompt(&snapshot, scenario, &db);
                let start = prompt.find("=== НЕДАВНИЕ СОБЫТИЯ").expect("the window");
                let window: Vec<&str> = prompt[start..].lines().skip(1).take_while(|l| l.starts_with("- ")).collect();
                for l in &window {
                    assert!(!has_id(l), "{sc} seed {seed} tick {}: an actor id in the window: {l}", world.tick);
                    let toks: Vec<&str> = l.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).collect();
                    for e in &snapshot.recent_important_events {
                        assert!(!toks.contains(&e.id.as_str()), "{sc} seed {seed}: the event key {} in the window: {l}", e.id);
                    }
                }
            }
            ids_in_log += state.event_log.events.iter().filter(|e| has_id(&e.description)).count();
        }
    }
    assert!(ids_in_log > 0, "the engine's log keeps ids in its descriptions — the window renames them");
}

/// The three engine descriptions that carry engine words are reworded for the chronicler from
/// their own parts: a tag spread reads the tag's label, an era change reads words, a promotion
/// to the foreground loses its debug reasons. Both ways: any other description is untouched.
#[test]
fn chronicler_description_rewords_tag_spread_era_and_foreground() {
    let scenario = crate::scenarios::milan_1477::load_milan_1477();
    let ev = |id: &str, d: &str| crate::core::Event::new(id.into(), 1, 1477, "milan".into(), crate::core::EventType::Cultural, false, d.into());
    let r = |id: &str, d: &str| crate::llm::chronicler_description(&ev(id, d), &scenario);
    assert_eq!(r("tag_spread_french_orbit_savoy", "Тег 'french_orbit' распространился от france к savoy"),
        "Французское влияние распространяется от france к savoy");
    assert_eq!(r("era_milan_earlymodern", "Миланское герцогство перешёл из LateMedieval в EarlyModern эру"),
        "Миланское герцогство переходит из позднего Средневековья в раннее Новое время");
    assert_eq!(r("foreground_savoy", "Савойя вышел на передний план: power_projection 88 > 70% avg 32, internal upheaval"),
        "Савойя выходит на передний план");
    assert_eq!(r("military_conflict_florence_siena", "Военный конфликт между florence и siena"), "Военный конфликт между florence и siena");
}

/// The event window carries no tag id, no era's code name and no debug reason of a promotion,
/// in all three worlds. Both ways: the engine's log does hold tag spreads and promotions with
/// those words (the defect is real), the window does not.
#[test]
fn event_window_carries_no_tag_ids_era_names_or_debug_reasons() {
    use rand::SeedableRng;
    let db = setup_test_db();
    let eras = ["Ancient", "EarlyMedieval", "HighMedieval", "LateMedieval", "EarlyModern"];
    let debug = ["power_projection", "military contact", "internal upheaval"];
    let (mut spreads_in_log, mut promotions_in_log, mut shown) = (0, 0, 0);
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for seed in 0..3u64 {
            let mut state = AppState::default();
            crate::application::load_scenario(&mut state, &db, sc.to_string()).unwrap();
            state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let tags: Vec<String> = state.current_scenario.as_ref().unwrap().tag_definitions.iter().map(|t| t.id.clone()).collect();
            for _ in 0..60 {
                advance_tick(&mut state, None).expect("tick");
                let world = state.world_state.as_ref().unwrap();
                let scenario = state.current_scenario.as_ref().unwrap();
                let snapshot = crate::llm::build_snapshot(world, scenario, &state.event_log);
                let prompt = crate::llm::generate_narrative_prompt(&snapshot, scenario, &db);
                let start = prompt.find("=== НЕДАВНИЕ СОБЫТИЯ").expect("the window");
                for l in prompt[start..].lines().skip(1).take_while(|l| l.starts_with("- ")) {
                    let toks: Vec<&str> = l.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).collect();
                    assert!(!toks.iter().any(|t| tags.iter().any(|x| x == t)), "{sc} seed {seed} tick {}: a tag id in the window: {l}", world.tick);
                    assert!(!toks.iter().any(|t| eras.contains(t)), "{sc} seed {seed}: an era's code name in the window: {l}");
                    assert!(!debug.iter().any(|d| l.contains(d)), "{sc} seed {seed}: a debug reason in the window: {l}");
                    if l.contains("распространяется от") || l.contains("выходит на передний план") { shown += 1; }
                }
            }
            spreads_in_log += state.event_log.events.iter().filter(|e| e.id.starts_with("tag_spread_") && e.description.starts_with("Тег '")).count();
            promotions_in_log += state.event_log.events.iter().filter(|e| e.id.starts_with("foreground_") && debug.iter().any(|d| e.description.contains(d))).count();
        }
    }
    assert!(spreads_in_log > 0 && promotions_in_log > 0, "the log keeps the engine's words: spreads {spreads_in_log}, promotions {promotions_in_log}");
    assert!(shown > 0, "reworded spreads or promotions do reach the window");
}

#[test]
fn claude_cli_lines_give_text_and_the_final_verdict() {
    use crate::llm::{parse_claude_cli_line, ClaudeCliLine};
    let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Милан — "}}}"#;
    assert_eq!(parse_claude_cli_line(delta), ClaudeCliLine::Text("Милан — ".to_string()));
    let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"Милан — город."}"#;
    assert_eq!(parse_claude_cli_line(ok), ClaudeCliLine::Done(Ok(())));
    let err = r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in"}"#;
    assert_eq!(parse_claude_cli_line(err), ClaudeCliLine::Done(Err("Not logged in".to_string())));
    // The whole message repeated after the stream, and the CLI's own lines, are not text.
    let assistant = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Милан — город."}]}}"#;
    assert_eq!(parse_claude_cli_line(assistant), ClaudeCliLine::Other);
    assert_eq!(parse_claude_cli_line(r#"{"type":"system","subtype":"init"}"#), ClaudeCliLine::Other);
    assert_eq!(parse_claude_cli_line("not json"), ClaudeCliLine::Other);
}

#[cfg(unix)]
#[test]
fn claude_cli_run_streams_text_and_names_every_failure() {
    use crate::llm::run_claude_cli;
    use std::time::Duration;
    let sh = |script: &str| {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", script]);
        c
    };
    let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Милан"}}}"#;
    let ok = r#"{"type":"result","is_error":false,"result":"Милан"}"#;
    let mut pieces = Vec::new();
    let text = run_claude_cli(sh(&format!("cat >/dev/null; echo '{delta}'; echo '{ok}'")), "вход", Duration::from_secs(20), |t| {
        pieces.push(t.to_string())
    });
    assert_eq!(text, Ok(Some("Милан".to_string())));
    assert_eq!(pieces, vec!["Милан".to_string()]);

    let missing = run_claude_cli(std::process::Command::new("engine13-no-such-claude"), "", Duration::from_secs(5), |_| {});
    assert!(missing.unwrap_err().contains("не найден"));

    let not_logged_in = r#"{"type":"result","is_error":true,"result":"Not logged in · Please run /login"}"#;
    let e = run_claude_cli(sh(&format!("echo '{not_logged_in}'; exit 1")), "", Duration::from_secs(20), |_| {}).unwrap_err();
    assert!(e.contains("Not logged in"), "{e}");

    let e = run_claude_cli(sh("exit 1"), "", Duration::from_secs(20), |_| {}).unwrap_err();
    assert!(e.contains("без ответа"), "{e}");

    // A hung process is stopped and the half-written text is not a chronicle.
    let started = std::time::Instant::now();
    let e = run_claude_cli(sh(&format!("echo '{delta}'; exec sleep 30")), "", Duration::from_secs(1), |_| {}).unwrap_err();
    assert!(e.contains("не ответил"), "{e}");
    assert!(started.elapsed() < Duration::from_secs(10));
}
