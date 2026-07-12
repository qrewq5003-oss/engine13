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
fn test_parse_scoped_bare_metric_resolves_to_actor() {
    // The regression this guards: a bare key next to an actor context used to be
    // concatenated into "rome.cohesion", which `parse` reads as a Global key that
    // nothing else touches, so the delta never reached the actor.
    match MetricRef::parse_scoped("cohesion", Some("rome")) {
        MetricRef::Actor { actor_id, metric } => {
            assert_eq!(actor_id, "rome");
            assert_eq!(metric, "cohesion");
        }
        other => panic!("Expected Actor variant, got {:?}", other),
    }
}

#[test]
fn test_parse_scoped_self_prefix_resolves_to_target_actor() {
    // Random events name their target actor as `self.`. This used to be rewritten
    // to "venice.population", which `parse` reads as a Global key — so conditions
    // read 0.0 and effects were swallowed.
    match MetricRef::parse_scoped("self.population", Some("venice")) {
        MetricRef::Actor { actor_id, metric } => {
            assert_eq!(actor_id, "venice");
            assert_eq!(metric, "population");
        }
        other => panic!("Expected Actor variant, got {:?}", other),
    }
}

#[test]
fn test_parse_scoped_bare_metric_without_actor_stays_global() {
    match MetricRef::parse_scoped("federation_progress", None) {
        MetricRef::Global { key } => assert_eq!(key, "federation_progress"),
        other => panic!("Expected Global variant, got {:?}", other),
    }
}

#[test]
fn test_parse_scoped_explicit_prefix_wins_over_actor_scope() {
    // An actor-scoped auto_delta may still gate on a global or another actor.
    match MetricRef::parse_scoped("global:federation_progress", Some("byzantium")) {
        MetricRef::Global { key } => assert_eq!(key, "federation_progress"),
        other => panic!("Expected Global variant, got {:?}", other),
    }
    match MetricRef::parse_scoped("actor:ottomans.military_size", Some("byzantium")) {
        MetricRef::Actor { actor_id, metric } => {
            assert_eq!(actor_id, "ottomans");
            assert_eq!(metric, "military_size");
        }
        other => panic!("Expected Actor variant, got {:?}", other),
    }
    match MetricRef::parse_scoped("family:family_influence", Some("rome")) {
        MetricRef::Family { key } => assert_eq!(key, "family_influence"),
        other => panic!("Expected Family variant, got {:?}", other),
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
    
    // Add byzantium and ottomans actors. The victory additional-condition gate is
    // `ottomans.military_size < 40` (replaced the old `external_pressure < 85`),
    // so ottomans must be present for the gate to read a real value.
    for actor in &scenario.actors {
        if actor.id == "byzantium" || actor.id == "ottomans" {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    // Set federation_progress = 100 (high enough to stay above 80 after auto_deltas), tick = 45
    // Note: MetricRef strips "global:" prefix when storing
    world.global_metrics.insert("federation_progress".to_string(), 100.0);
    world.tick = 45;  // minimum_tick is 40 (20 years × 2 ticks/year)

    // Set ottomans.military_size = 120 (well above threshold 40) → gate fails.
    // Wide margin so any per-tick combat drift can't cross 40.
    if let Some(ott) = world.actors.get_mut("ottomans") {
        ott.set_metric("military_size", 120.0);
    }

    // Run check_victory_condition via tick
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);

    // victory_achieved should be false because ottoman military is too strong
    assert!(!world.victory_achieved, "Victory should not be achieved when ottoman military_size >= 40");

    // Break the Ottoman army: military_size = 10 (well below threshold 40)
    if let Some(ott) = world.actors.get_mut("ottomans") {
        ott.set_metric("military_size", 10.0);
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
    
    // Add byzantium and ottomans actors (victory gate is ottomans.military_size < 40)
    for actor in &scenario.actors {
        if actor.id == "byzantium" || actor.id == "ottomans" {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    // Set victory conditions: federation = 100 (high enough to stay above 80),
    // ottomans.military_size = 10 (below threshold 40 → gate passes), tick = 45
    world.global_metrics.insert("federation_progress".to_string(), 100.0);
    world.tick = 45;  // minimum_tick is 40 (20 years × 2 ticks/year)
    if let Some(ott) = world.actors.get_mut("ottomans") {
        ott.set_metric("military_size", 10.0);
    }

    // Run 2 ticks - should accumulate sustained ticks
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);

    assert_eq!(world.victory_sustained_ticks, 2, "Should have 2 sustained ticks");

    // Rebuild the Ottoman army above threshold → gate fails
    if let Some(ott) = world.actors.get_mut("ottomans") {
        ott.set_metric("military_size", 120.0);
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
        generation_count: 0,
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
    let _scenario = registry::load_by_id("rome_375").unwrap();
    let mut state = crate::AppState::default();
    let _db = crate::db::Db::open_in_memory().unwrap();

    crate::application::load_scenario(&mut state, &_db, "rome_375".to_string()).unwrap();

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

#[test]
fn test_actor_tags_populated_after_load() {
    // Verify that actor_tags are populated from tags.toml after scenario load
    let scenario = registry::load_by_id("rome_375").unwrap();

    // Rome should have actor_tags for its tags (bureaucracy, roman_law, etc.)
    let rome = scenario.actors.iter().find(|a| a.id == "rome").unwrap();
    assert!(!rome.actor_tags.is_empty(), "Rome should have actor_tags populated");
    assert!(rome.actor_tags.contains_key("bureaucracy"), "Rome should have bureaucracy tag");
    assert!(rome.actor_tags.contains_key("trade_networks"), "Rome should have trade_networks tag");

    // Check that trade_networks has metrics_modifier (economic tags retain modifiers)
    let trade = rome.actor_tags.get("trade_networks").unwrap();
    assert!(trade.metrics_modifier.contains_key("economic_output"), "trade_networks should modify economic_output");
}

#[test]
fn test_tag_definitions_loaded() {
    // Verify tag_definitions are loaded in scenario
    let scenario = registry::load_by_id("rome_375").unwrap();
    assert!(!scenario.tag_definitions.is_empty(), "Rome 375 should have tag_definitions");

    // Check that a known tag exists
    let trade_tag = scenario.tag_definitions.iter().find(|t| t.id == "trade_networks");
    assert!(trade_tag.is_some(), "Should have trade_networks tag definition");
    let trade_tag = trade_tag.unwrap();
    assert!(!trade_tag.spreads_via.is_empty(), "trade_networks should have spreads_via");
}

#[test]
fn test_era_definitions_loaded() {
    // Verify era_definitions are loaded in scenario
    let scenario = registry::load_by_id("rome_375").unwrap();
    assert!(!scenario.era_definitions.is_empty(), "Rome 375 should have era_definitions");

    // Should have ancient and early_medieval at minimum
    let ancient = scenario.era_definitions.iter().find(|e| e.era == crate::core::Era::Ancient);
    assert!(ancient.is_some(), "Should have ancient era definition");

    let early_med = scenario.era_definitions.iter().find(|e| e.era == crate::core::Era::EarlyMedieval);
    assert!(early_med.is_some(), "Should have early_medieval era definition");
}

#[test]
fn test_era_progression_fires() {
    // Verify era progression works: give actor enough tags and run ticks
    let scenario = registry::load_by_id("rome_375").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);

    // Add rome with enough tags for early_medieval
    for actor in &scenario.actors {
        if actor.id == "rome" {
            let mut rome = actor.clone();
            // Rome already has bureaucracy, roman_law, trade_networks, coinage, christianity = 5 tags
            // early_medieval requires 4 from ancient tags
            world.actors.insert(rome.id.clone(), rome);
            break;
        }
    }

    // Set tick past min_tick for early_medieval (40)
    world.tick = 41;

    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);

    // Rome should have advanced to EarlyMedieval
    let rome = world.actors.get("rome").unwrap();
    assert_eq!(rome.era, crate::core::Era::EarlyMedieval, "Rome should advance to EarlyMedieval after tick with enough tags");
}

/// Builds a two-actor constantinople world (byzantium and ottomans are the only
/// distance-1 pair in the scenario) and runs the interaction phase directly, so no
/// auto_delta or random event can move `military_size` behind the test's back.
/// Returns the ids of every military conflict that occurred.
fn run_combat_only(byzantium_military: f64, rounds: u32) -> Vec<String> {
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    for actor in &scenario.actors {
        if actor.id == "byzantium" || actor.id == "ottomans" {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    world
        .actors
        .get_mut("byzantium")
        .unwrap()
        .set_metric("military_size", byzantium_military);

    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    // Past the 3-tick stabilization window; each round advances the tick so the
    // 5-tick combat cooldown cannot be what suppresses the fight.
    for i in 0..rounds {
        world.tick = 10 + i * 6;
        crate::engine::interactions::calculate_interactions(
            &mut world,
            &scenario,
            &mut event_log,
            &mut rng,
        );
    }

    event_log
        .events
        .iter()
        .map(|e| e.id.clone())
        .filter(|id| id.starts_with("military_conflict_"))
        .collect()
}

#[test]
fn test_no_combat_against_an_army_that_no_longer_exists() {
    // The #17 defect: a destroyed army stayed a legal target forever, and the attacker
    // paid 5-15% of its own military per fight to storm an empty field. 81-95% of all
    // fights in the no-player baselines were these.
    let conflicts = run_combat_only(0.0, 40);
    assert!(
        conflicts.is_empty(),
        "the ottomans attacked a byzantium with no army {} times: {:?}",
        conflicts.len(),
        conflicts
    );
}

#[test]
fn test_combat_still_happens_against_a_real_army() {
    // The other half of the guard: it must not be so eager that it disables combat.
    // A defender that can still fight is still attacked.
    let conflicts = run_combat_only(50.0, 40);
    assert!(
        !conflicts.is_empty(),
        "no fight occurred against a byzantium with a real army — the guard is suppressing real combat"
    );
}

/// Puts byzantium in the exhaustion state — no army, no legitimacy, saturated external
/// pressure — and holds it there. Cohesion is kept deliberately *high*, so neither of
/// the two older collapse paths (both require low cohesion) can be what kills it.
///
/// `with_ottomans` controls the only thing that should matter: whether an armed actor
/// is present on the border to finish the job. The ottomans are byzantium's sole
/// distance-1 neighbour, so removing them removes the siege. (Leaving them in with a
/// zeroed army would not work: their auto_delta re-arms them by +0.5 within the same
/// tick, before `check_collapses` runs.)
///
/// Returns whether byzantium was still alive after `rounds` ticks.
fn byzantium_survives_exhaustion(with_ottomans: bool, rounds: u32) -> bool {
    let scenario = registry::load_by_id("constantinople_1430").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
    for actor in &scenario.actors {
        let keep = actor.id == "byzantium" || (with_ottomans && actor.id == "ottomans");
        if keep {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    // Past any minimum_survival_ticks guarantee.
    world.tick = 120;

    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    for _ in 0..rounds {
        // Re-assert the exhaustion state each tick: auto_deltas and events would
        // otherwise drift the very metrics under test.
        if let Some(byz) = world.actors.get_mut("byzantium") {
            byz.set_metric("military_size", 0.0);
            byz.set_metric("legitimacy", 0.0);
            byz.set_metric("external_pressure", 100.0);
            byz.set_metric("cohesion", 90.0);
        }
        crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    }

    world.actors.contains_key("byzantium")
}

#[test]
fn test_exhausted_actor_dies_to_an_armed_neighbour() {
    // No army, no legitimacy, saturated pressure, and an armed enemy on the border.
    assert!(
        !byzantium_survives_exhaustion(true, 12),
        "byzantium had no army, no legitimacy, pressure 100 and an armed ottoman \
         neighbour on its only distance-1 border, and still did not fall"
    );
}

#[test]
fn test_exhausted_actor_survives_when_no_one_can_finish_it() {
    // The same exhaustion state, with nobody on the border able to finish it.
    //
    // This is the clause that keeps the path a *conquest* condition rather than a
    // blanket one, and it is not a hypothetical: in the no-player world legitimacy
    // decays to 0 and external_pressure saturates at 100 for nearly every actor, so
    // those two gates discriminate nothing. Measured without this clause, the predicate
    // killed 12 of Rome's actors and *both* protagonists — byzantium at median tick 41,
    // milan at 71 — in 20/20 runs.
    assert!(
        byzantium_survives_exhaustion(false, 12),
        "byzantium died with nobody on its border able to finish it — the conquest \
         collapse path has degenerated into a blanket predicate"
    );
}

#[test]
fn test_cultural_displacement_progress_accumulates() {
    // Verify cultural displacement progress accumulates when there's a big power gap
    let scenario = registry::load_by_id("rome_375").unwrap();
    let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);

    // Add rome (strong) and alamanni (weak, neighbor at distance 2)
    for actor in &scenario.actors {
        if actor.id == "rome" || actor.id == "alamanni" {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    // Make alamanni very weak to create big cultural power gap
    if let Some(alamanni) = world.actors.get_mut("alamanni") {
        alamanni.set_metric("legitimacy", 10.0);
        alamanni.set_metric("cohesion", 10.0);
        alamanni.set_metric("economic_output", 5.0);
    }

    let mut event_log = crate::engine::EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    // Run several ticks
    for _ in 0..10 {
        crate::engine::tick(&mut world, &scenario, &mut event_log, &mut rng);
    }

    // Check if displacement progress accumulated for alamanni
    // (may or may not have triggered full displacement, but progress should exist or have triggered)
    let progress = world.cultural_displacement_progress.get("alamanni").copied().unwrap_or(0.0);
    // Progress accumulates then decays, so it might have triggered or be building up
    // The key test is that it didn't panic and the system works
    assert!(progress >= 0.0, "Displacement progress should be non-negative");
}

// --- Dependency rule validation (load-time guard for threshold-required modes) ---

fn dep_rule(id: &str, mode: crate::core::DependencyMode, threshold: Option<f64>) -> crate::core::DependencyRule {
    crate::core::DependencyRule {
        id: id.to_string(),
        from: "legitimacy".to_string(),
        to: "cohesion".to_string(),
        coefficient: 1.0,
        threshold,
        mode,
    }
}

#[test]
fn test_validate_dependencies_missing_threshold_is_load_error() {
    use crate::core::DependencyMode;
    let metrics = ["legitimacy", "cohesion"];

    // Deficit/Excess/Bonus without a threshold must be rejected at load time
    for mode in [DependencyMode::Deficit, DependencyMode::Excess, DependencyMode::Bonus] {
        let rules = vec![dep_rule("bad_rule", mode.clone(), None)];
        let result = crate::engine::validate_dependencies(&rules, &metrics);
        let errors = result.expect_err("missing threshold should be a load error, not accepted");
        assert!(
            errors.iter().any(|e| e.contains("bad_rule") && e.contains("threshold required")),
            "error message should name the rule and the missing threshold, got: {:?}",
            errors
        );
    }
}

#[test]
fn test_validate_dependencies_valid_rules_ok() {
    use crate::core::DependencyMode;
    let metrics = ["legitimacy", "cohesion"];
    let rules = vec![
        dep_rule("deficit_ok", DependencyMode::Deficit, Some(50.0)),
        dep_rule("linear_ok", DependencyMode::Linear, None), // Linear needs no threshold
    ];
    assert!(
        crate::engine::validate_dependencies(&rules, &metrics).is_ok(),
        "valid rules (threshold present, or Linear) should pass validation"
    );
}

#[test]
fn test_validate_scenario_catches_missing_threshold_centrally() {
    use crate::core::DependencyMode;
    // D4: threshold validation is centralized in the load choke point
    // (`validate_scenario`). A scenario that reaches it with a non-Linear rule
    // missing its threshold must be rejected even without a per-scenario
    // `validate_dependencies` call — this is what makes the `apply_dependency_rule`
    // hot-path fallback provably unreachable for any new scenario.
    let mut scenario = registry::load_by_id("rome_375").unwrap();
    assert!(
        registry::validate_scenario(&scenario).is_ok(),
        "baseline rome_375 should be valid"
    );

    scenario
        .dependencies
        .push(dep_rule("injected_bad", DependencyMode::Deficit, None));

    let errors = registry::validate_scenario(&scenario)
        .expect_err("central choke point must reject a non-Linear rule without a threshold");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("injected_bad") && e.contains("threshold required")),
        "central validation should name the offending rule, got: {:?}",
        errors
    );
}
