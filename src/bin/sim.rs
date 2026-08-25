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
//! cargo run --bin sim rome_375 6 narrative_eval 42 live   # evaluate real narratives
//! cargo run --bin sim rome_375 6 narrative_eval 42 dry    # same, without calling the LLM
//! cargo run --bin sim rome_375 0 narrative_pack 42 live   # review pack, bound derived from scenario
//! cargo run --bin sim milan_1477 0 narrative_pack 42 dry  # inputs only, no LLM calls
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
            let seed: u64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(42);
            run_scripted(scenario_id, ticks, strategy, seed);
        },
        "narrative_eval" => {
            let seed: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(42);
            let live = args.get(5).map(|s| s.as_str()) != Some("dry");
            run_narrative_eval(scenario_id, ticks, seed, live)
        }
        "narrative_pack" => {
            let seed: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(42);
            let live = args.get(5).map(|s| s.as_str()) != Some("dry");
            run_narrative_pack(scenario_id, ticks, seed, live)
        }
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

        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
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

// ============================================================================
// Narrative Evaluation Mode
//
// Rewritten for task 31 plan item (D). What it used to do (§2.1 of
// docs/narrative_state_2026_08.md): score the literal string
// `"[Narrative for tick N would appear here]"`. Its printed verdict — always
// `Avg 1.0/4`, always `N of N ticks FAIL` — was a constant of that literal:
// only `not_generic` passed, because the placeholder happens not to open with a
// banned Russian phrase. No state of the world and no quality of narrative could
// change it.
//
// It is now a SMOKE TEST over real output, not a quality judge. §4.1 of the
// write-up measured what happens when this kind of regex scoring is trusted as a
// verdict — three of seven rules had to be thrown away as proximity artifacts —
// so the four checks here are deliberately narrow: they answer "did a real
// narrative come back, in the language and vocabulary of THIS scenario", not
// "is it good". Quality stays a human read of the review pack.
// ============================================================================

/// Consequence markers for causality check
const CONSEQUENCE_MARKERS: &[&str] = &[
    "поэтому", "в результате", "это привело", "тем временем",
    "после этого", "вслед за", "что повлекло", "следствием",
];

/// Generic opening phrases to avoid
const GENERIC_OPENINGS: &[&str] = &[
    "В это время", "Мир менялся", "Годы шли", "Империя стояла",
    "Время шло", "История продолжалась",
];

/// Word stems of an action's own display name, for the strategy-reflection check.
///
/// This replaces a hardcoded `ACTION_KEYWORDS` table that listed eight ids, six of
/// them rome's. Constantinople's and milan's action ids were absent from it, so the
/// check returned FAIL for those two scenarios on every tick regardless of the text
/// — the same rome-only assumption §2.2 found in the pack generator. The vocabulary
/// now comes from the scenario itself: the action's `name` is exactly the string the
/// chronicler is shown ("Действие игрока: Сбор ополчения"), so matching against its
/// stems asks whether the narrative picked up the action it was told about.
///
/// Six-character stems are a crude stand-in for Russian inflection ("ополчения" →
/// "ополче" matches "ополчение"). It misses aspect changes ("Вложить" vs
/// "вкладывают"), so a FAIL here is weak evidence and a PASS is strong evidence —
/// which is why this is a smoke test.
fn action_name_stems(name: &str) -> Vec<String> {
    name.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| w.chars().count() >= 5)
        .map(|w| w.chars().take(6).collect::<String>())
        .collect()
}

fn run_narrative_eval(scenario_id: &str, ticks: u32, seed: u64, live: bool) {
    use engine13::application::actions::{apply_player_action, PlayerActionInput};
    use engine13::commands::AppState;

    let scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");

    println!("Running narrative evaluation mode");
    println!(
        "  scenario: {}  seed: {}  ticks: {}  narrative: {}",
        scenario_id,
        seed,
        ticks,
        if live { "generated by the configured model" } else { "NOT requested (dry)" }
    );
    println!("  (smoke test over real output — not a quality verdict; quality is the review pack)");
    println!();

    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario
            .generation_mechanics
            .as_ref()
            .map(|g| g.patriarch_start_age)
            .unwrap_or(40) as u32;
        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
        });
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let mut state = AppState {
        world_state: Some(world),
        event_log: EventLog::new(),
        current_scenario: Some(scenario.clone()),
        rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
        narrative_memory: engine13::llm::NarrativeMemory::default(),
    };

    let strategy = ScriptedStrategy::from_str("balanced", scenario_id);
    let priority = strategy.priority_actions();
    let db = engine13::db::Db::open_in_memory().expect("in-memory db");
    let cfg = engine13::llm::get_llm_config();
    if live {
        eprintln!("[eval] provider={} model={}", cfg.provider, cfg.model);
    }

    let mut total_scores: Vec<u32> = Vec::new();
    let mut low_score_ticks: Vec<u32> = Vec::new();
    let mut llm_failures = 0u32;

    for tick_num in 0..ticks {
        let mut applied = 0u32;
        let mut actions_applied: Vec<String> = Vec::new();
        for action_id in &priority {
            if applied >= scenario.actions_per_tick {
                break;
            }
            let input = PlayerActionInput {
                action_id: action_id.to_string(),
                target_actor_id: None,
            };
            if apply_player_action(&mut state, &input).is_ok() {
                applied += 1;
                actions_applied.push(action_id.to_string());
            }
        }

        {
            let ws = state.world_state.as_mut().unwrap();
            let sc = state.current_scenario.as_ref().unwrap();
            let rng = state.rng.as_mut().unwrap();
            tick(ws, sc, &mut state.event_log, rng);
        }

        if actions_applied.is_empty() {
            continue;
        }

        // The narrative the player would actually read, through the canonical path.
        let ws = state.world_state.as_ref().unwrap();
        let snapshot = engine13::llm::build_snapshot(ws, &scenario, &state.event_log);
        let prompt = engine13::llm::generate_narrative_prompt(
            &snapshot,
            &scenario,
            &db,
            &state.narrative_memory,
        );

        let narrative_text = if live {
            match engine13::llm::generate_narrative_blocking(&prompt, &cfg, 5) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[eval] tick {} FAILED: {}", tick_num, e);
                    llm_failures += 1;
                    continue;
                }
            }
        } else {
            println!(
                "tick {:2}: prompt {} bytes, actions=[{}] — dry, LLM not called",
                tick_num,
                prompt.len(),
                actions_applied.join(", ")
            );
            continue;
        };

        let ws = state.world_state.as_ref().unwrap();
        let tick_result =
            evaluate_narrative_tick(&narrative_text, &actions_applied, ws, &scenario, tick_num);

        total_scores.push(tick_result.best_score);
        if tick_result.best_score < 3 {
            low_score_ticks.push(tick_num);
        }
        println!("{}", tick_result.output);
    }

    println!();
    println!("=== SUMMARY ===");
    if !live {
        println!("Dry run — no narrative was generated, nothing was scored.");
        return;
    }
    println!("Ticks evaluated: {}", total_scores.len());
    if llm_failures > 0 {
        println!("LLM failures (tick skipped): {}", llm_failures);
    }
    if !total_scores.is_empty() {
        let avg_score = total_scores.iter().sum::<u32>() as f64 / total_scores.len() as f64;
        println!("Avg best-action score: {:.1}/4", avg_score);
        println!(
            "Ticks with score < 3: {} (ticks: {:?})",
            low_score_ticks.len(),
            low_score_ticks
        );
    }
}

struct NarrativeEvalResult {
    best_score: u32,
    output: String,
}

fn evaluate_narrative_tick(
    narrative: &str,
    actions: &[String],
    world: &WorldState,
    scenario: &engine13::core::Scenario,
    tick_num: u32,
) -> NarrativeEvalResult {
    let mut output = String::new();
    output.push_str(&format!("=== NARRATIVE EVAL: tick {} ===\n", tick_num));
    output.push_str(&format!("Actions this tick: {}\n", actions.join(", ")));
    output.push_str(&format!(
        "Narrative: {} chars, {} paragraphs\n\n",
        narrative.chars().count(),
        narrative.split("\n\n").filter(|p| !p.trim().is_empty()).count()
    ));

    let mut best_score = 0u32;

    for action_id in actions {
        let mut score = 0u32;
        let mut action_output = String::new();
        let action_name = scenario
            .patron_actions
            .iter()
            .find(|a| &a.id == action_id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| action_id.clone());
        action_output.push_str(&format!("  Action: {} (\"{}\")\n", action_id, action_name));

        let (c1_pass, c1_match) = check_action_type_reflected(narrative, &action_name);
        if c1_pass { score += 1; }
        action_output.push_str(&format!(
            "    action_type_reflected:      {}  [matched: \"{}\"]\n",
            if c1_pass { "PASS" } else { "FAIL" },
            c1_match.unwrap_or_else(|| "none".to_string())
        ));

        let (c2_pass, c2_match) = check_consequence_marker(narrative);
        if c2_pass { score += 1; }
        action_output.push_str(&format!(
            "    consequence_marker_present: {}  [matched: \"{}\"]\n",
            if c2_pass { "PASS" } else { "FAIL" },
            c2_match.unwrap_or_else(|| "none".to_string())
        ));

        let c3_pass = check_not_generic(narrative);
        if c3_pass { score += 1; }
        action_output.push_str(&format!(
            "    not_generic:                {}\n",
            if c3_pass { "PASS" } else { "FAIL" }
        ));

        let (c4_pass, c4_match) = check_actor_mentioned(narrative, world);
        if c4_pass { score += 1; }
        action_output.push_str(&format!(
            "    actor_mentioned:            {}  [matched: \"{}\"]\n",
            if c4_pass { "PASS" } else { "FAIL" },
            c4_match.unwrap_or_else(|| "none".to_string())
        ));

        action_output.push_str(&format!("    Score: {}/4\n\n", score));

        if score > best_score {
            best_score = score;
        }

        output.push_str(&action_output);
    }

    output.push_str(&format!(
        "Tick result: {} (best action score: {}/4)\n",
        if best_score >= 3 { "PASS" } else { "FAIL" },
        best_score
    ));

    NarrativeEvalResult { best_score, output }
}

/// Does the narrative pick up the vocabulary of the action it was told about?
/// Matches against stems of the action's own display name — see `action_name_stems`.
fn check_action_type_reflected(narrative: &str, action_name: &str) -> (bool, Option<String>) {
    let narrative_lower = narrative.to_lowercase();
    for stem in action_name_stems(action_name) {
        if narrative_lower.contains(&stem) {
            return (true, Some(stem));
        }
    }
    (false, None)
}

fn check_consequence_marker(narrative: &str) -> (bool, Option<String>) {
    for marker in CONSEQUENCE_MARKERS.iter() {
        if narrative.contains(marker) {
            return (true, Some(marker.to_string()));
        }
    }
    (false, None)
}

fn check_not_generic(narrative: &str) -> bool {
    let words: Vec<&str> = narrative.split_whitespace().take(15).collect();
    let opening = words.join(" ");

    for generic in GENERIC_OPENINGS.iter() {
        if opening.contains(generic) {
            return false;
        }
    }
    true
}

fn check_actor_mentioned(narrative: &str, world: &WorldState) -> (bool, Option<String>) {
    // Sorted: `world.actors` is a HashMap, so an unsorted scan would report a
    // different "matched" name per process even when the verdict is the same.
    // Same class as the five sites plan item (C) fixed in `llm/mod.rs`.
    let mut names: Vec<&str> = world.actors.values().map(|a| a.name.as_str()).collect();
    names.sort_unstable();
    for name in names {
        if narrative.contains(name) {
            return (true, Some(name.to_string()));
        }
    }
    (false, None)
}

// ============================================================================
// Narrative Review Pack Generator
//
// Rewritten for task 31 plan item (D). What it used to do, and why none of it
// could be trusted (see docs/narrative_state_2026_08.md §2.2):
//   - it wrote the literal "[LLM UNAVAILABLE - narrative would appear here]"
//     into every case, so the pack never contained a narrative;
//   - it built the metrics table from `world` *after* the loop, so every case
//     showed the final tick's state and the final year, not the case's;
//   - it read `world.actors.get("rome")` unconditionally, so for
//     constantinople_1430 and milan_1477 the whole table was 0.0;
//   - "Player actions this tick" and "Key events" were string literals;
//   - `MAX_TICKS = 60` was inherited, and task 29 moved rome's generation
//     period to 33 years = 66 ticks, silently making case 5 unreachable.
//
// The rewrite: one deterministic pass records per-tick state *and the exact
// prompt the chronicler would receive* (via `llm::build_snapshot` +
// `llm::generate_narrative_prompt` — the same path the app uses, made
// reproducible by plan item (C)); cases are then selected from that recorded
// series, so a case's table is by construction the state at the case's tick;
// and the narrative for the selected ticks is generated through the shared
// `llm::generate_narrative_blocking`.
// ============================================================================

/// One recorded half-year of a pack run.
struct PackTurn {
    tick: u32,
    year: i32,
    half_year: String,
    /// Scenario's own `narrative_config.key_metrics`, resolved at this tick.
    key_metrics: Vec<(String, f64)>,
    /// Actors the engine currently considers in danger, sorted.
    collapse_warnings: Vec<String>,
    dead_actors: Vec<String>,
    victory_achieved: bool,
    victory_sustained: u32,
    generation_transfer: bool,
    actions_applied: Vec<String>,
    /// Event ids the chronicler is actually shown (first five of the prompt).
    events_shown: Vec<String>,
    prompt: String,
}

/// Why a case slot has no tick.
enum CaseOutcome {
    Found(usize),
    NotReached,
    /// The scenario cannot produce this case at all — it declares no such mechanic.
    NotApplicable(&'static str),
}

fn run_narrative_pack(scenario_id: &str, max_ticks_arg: u32, seed: u64, live: bool) {
    use engine13::application::actions::{apply_player_action, PlayerActionInput};
    use engine13::commands::AppState;

    let scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");

    // ------------------------------------------------------------------
    // Bound. Derived from measurement, not inherited.
    //
    // The old `MAX_TICKS = 60` had no relation to any scenario value, and task 29
    // silently invalidated it: restoring rome's authored `generation_length` of 33
    // years moved the generation transfer to tick 64, past the bound, so that case
    // became permanently unreachable and nobody noticed.
    //
    // The bound is therefore re-derived here from what the cases actually need,
    // measured on this branch (`narrative_pack <scenario> 400 ... dry`, §11.2):
    //
    //   binding case            measured first tick
    //   rome generation transfer  64 (stable across seeds 42/1/7 — fixed period)
    //   rome first collapse warn  77
    //   milan first collapse warn 127
    //   constantinople victory    86 / 97 / 113 / 120 / 164 / 203
    //                             (seeds 13 / 1 / 99 / 3 / 42 / 7)
    //
    // The widest is constantinople's victory at 203, so the default carries ~18%
    // headroom above it. The generation period is kept as an explicit floor so a
    // future scenario with a longer period cannot be silently truncated the way
    // task 29 truncated this tool.
    // ------------------------------------------------------------------
    const MEASURED_TICKS: u32 = 240;
    let generation_floor = scenario
        .generation_length
        .map(|years| years * 2 + 4)
        .unwrap_or(0);
    let derived_ticks = MEASURED_TICKS.max(generation_floor);
    let max_ticks = if max_ticks_arg > 0 { max_ticks_arg } else { derived_ticks };

    println!("Generating narrative review pack");
    println!(
        "  scenario: {}  seed: {}  bound: {} ticks ({})",
        scenario_id,
        seed,
        max_ticks,
        if max_ticks_arg > 0 { "from argument" } else { "derived from scenario" }
    );
    println!(
        "  generation_length: {}",
        scenario
            .generation_length
            .map(|y| format!("{} years = {} ticks", y, y * 2))
            .unwrap_or_else(|| "none (scenario has no generation mechanic)".to_string())
    );
    println!();

    // ------------------------------------------------------------------
    // World init — identical to run_scripted, so the pack reflects a game
    // someone is actually playing rather than an idle world.
    // ------------------------------------------------------------------
    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario
            .generation_mechanics
            .as_ref()
            .map(|g| g.patriarch_start_age)
            .unwrap_or(40) as u32;
        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
        });
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let mut state = AppState {
        world_state: Some(world),
        event_log: EventLog::new(),
        current_scenario: Some(scenario.clone()),
        rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
        narrative_memory: engine13::llm::NarrativeMemory::default(),
    };

    let strategy = ScriptedStrategy::from_str("balanced", scenario_id);
    let priority = strategy.priority_actions();
    let db = engine13::db::Db::open_in_memory().expect("in-memory db");

    let mut turns: Vec<PackTurn> = Vec::new();
    let mut seen_transfer_ticks: HashSet<u32> = HashSet::new();

    for tick_num in 0..max_ticks {
        let mut applied = 0u32;
        let mut actions_applied: Vec<String> = Vec::new();
        for action_id in &priority {
            if applied >= scenario.actions_per_tick {
                break;
            }
            let input = PlayerActionInput {
                action_id: action_id.to_string(),
                target_actor_id: None,
            };
            if apply_player_action(&mut state, &input).is_ok() {
                applied += 1;
                actions_applied.push(action_id.to_string());
            }
        }

        {
            let ws = state.world_state.as_mut().unwrap();
            let sc = state.current_scenario.as_ref().unwrap();
            let rng = state.rng.as_mut().unwrap();
            tick(ws, sc, &mut state.event_log, rng);
        }

        let ws = state.world_state.as_ref().unwrap();
        let snapshot = engine13::llm::build_snapshot(ws, &scenario, &state.event_log);
        let prompt = engine13::llm::generate_narrative_prompt(
            &snapshot,
            &scenario,
            &db,
            &state.narrative_memory,
        );

        // Key metrics: the scenario's own declared narrative metrics, resolved at
        // THIS tick. Sorted for the same reason the prompt sorts them — the source
        // is a HashMap and the pack has to be reproducible run to run.
        let mut key_metrics: Vec<(String, f64)> = snapshot
            .key_metrics
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        key_metrics.sort_by(|a, b| a.0.cmp(&b.0));

        let mut collapse_warnings: Vec<String> =
            ws.collapse_warning_ticks.keys().cloned().collect();
        collapse_warnings.sort();

        let generation_transfer = state
            .event_log
            .events
            .iter()
            .any(|e| e.id == "generation_transfer" && e.tick == tick_num && seen_transfer_ticks.insert(tick_num));

        turns.push(PackTurn {
            tick: tick_num,
            year: snapshot.year,
            half_year: snapshot.half_year.display_name().to_string(),
            key_metrics,
            collapse_warnings,
            dead_actors: snapshot.dead_actors.clone(),
            victory_achieved: snapshot.victory_achieved,
            victory_sustained: ws.victory_sustained_ticks,
            generation_transfer,
            actions_applied,
            events_shown: snapshot
                .recent_important_events
                .iter()
                .take(5)
                .map(|e| e.id.clone())
                .collect(),
            prompt,
        });
    }

    // ------------------------------------------------------------------
    // Case selection — from the recorded series, by engine-generic signals.
    //
    // No case predicate names an actor, a metric or a scenario. "Crisis" is
    // whatever the engine itself flags via `collapse_warning_ticks`; "winning"
    // and "victory" are the engine's own `victory_sustained_ticks` /
    // `victory_achieved`. A scenario that declares no victory condition or no
    // generation mechanic reports those slots as *not applicable*, which is a
    // different statement from "not reached" and the old tool could not make it.
    // ------------------------------------------------------------------
    let has_victory = scenario.victory_condition.is_some();
    let has_generations = scenario.generation_mechanics.is_some();
    // "Winning but not yet won" needs the victory condition to require holding for
    // more than one tick. rome_375 declares `sustained_ticks_required: 1`, so the
    // engine flips `victory_achieved` in the same tick the condition first holds and
    // that state cannot exist there — not applicable, not "not reached".
    let sustained_required = scenario
        .victory_condition
        .as_ref()
        .map(|vc| vc.sustained_ticks_required)
        .unwrap_or(0);
    // "Early" is 20 ticks = 10 game years, absolute. It must NOT be a fraction of the
    // bound: at the 400-tick bound used to derive the default, a quarter-of-the-run
    // window called tick 77 "early game" in rome.
    const EARLY_WINDOW_TICKS: u32 = 20;

    let case_names = [
        "Ранняя партия, устойчиво",
        "Ранняя партия, кризис",
        "Середина партии, игрок ведёт",
        "Первое предупреждение о коллапсе",
        "Смена поколения",
        "Момент победы",
    ];

    let mut outcomes: Vec<CaseOutcome> = Vec::with_capacity(6);

    // 1: earliest tick from 3 onward with nothing in danger and nobody dead.
    outcomes.push(
        turns
            .iter()
            .position(|t| t.tick >= 3 && t.collapse_warnings.is_empty() && t.dead_actors.is_empty())
            .map(CaseOutcome::Found)
            .unwrap_or(CaseOutcome::NotReached),
    );

    // 2: first danger signal, if it lands in the first quarter of the run.
    outcomes.push(
        match turns.iter().position(|t| !t.collapse_warnings.is_empty()) {
            Some(i) if turns[i].tick < EARLY_WINDOW_TICKS => CaseOutcome::Found(i),
            _ => CaseOutcome::NotReached,
        },
    );

    // 3: victory condition currently satisfied, not yet sustained long enough.
    outcomes.push(if !has_victory {
        CaseOutcome::NotApplicable("сценарий не объявляет victory_condition")
    } else if sustained_required <= 1 {
        CaseOutcome::NotApplicable(
            "victory_condition требует удержания 1 тик — состояние «условие держится, но победы ещё нет» невозможно",
        )
    } else {
        turns
            .iter()
            .position(|t| t.victory_sustained >= 1 && !t.victory_achieved)
            .map(CaseOutcome::Found)
            .unwrap_or(CaseOutcome::NotReached)
    });

    // 4: the first half-year the engine flags anyone as in danger, whenever it happens.
    //
    // An earlier draft used the *peak* number of actors under warning at once. That is
    // bound-relative — at a 400-tick bound the peak landed on tick 333 in constantinople
    // and 364 in milan, centuries past the scenario's premise, and two packs built with
    // different bounds would not be comparable. "First warning" is absolute.
    // It coincides with case 2 when the first warning happens to be early; that is not a
    // duplicate, the two slots ask different questions.
    outcomes.push(
        turns
            .iter()
            .position(|t| !t.collapse_warnings.is_empty())
            .map(CaseOutcome::Found)
            .unwrap_or(CaseOutcome::NotReached),
    );

    // 5: the half-year after a generation transfer fired.
    outcomes.push(if !has_generations {
        CaseOutcome::NotApplicable("сценарий не объявляет generation_mechanics")
    } else {
        match turns.iter().position(|t| t.generation_transfer) {
            Some(i) if i + 1 < turns.len() => CaseOutcome::Found(i + 1),
            Some(i) => CaseOutcome::Found(i),
            None => CaseOutcome::NotReached,
        }
    });

    // 6: the half-year victory was achieved.
    outcomes.push(if !has_victory {
        CaseOutcome::NotApplicable("сценарий не объявляет victory_condition")
    } else {
        turns
            .iter()
            .position(|t| t.victory_achieved)
            .map(CaseOutcome::Found)
            .unwrap_or(CaseOutcome::NotReached)
    });

    // ------------------------------------------------------------------
    // Narrative for the selected ticks. This is the part that used to be a
    // string literal.
    // ------------------------------------------------------------------
    let mut narratives: HashMap<usize, String> = HashMap::new();
    if live {
        let cfg = engine13::llm::get_llm_config();
        eprintln!("[pack] provider={} model={}", cfg.provider, cfg.model);
        let mut wanted: Vec<usize> = outcomes
            .iter()
            .filter_map(|o| match o {
                CaseOutcome::Found(i) => Some(*i),
                _ => None,
            })
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        for i in wanted {
            match engine13::llm::generate_narrative_blocking(&turns[i].prompt, &cfg, 5) {
                Ok(text) => {
                    eprintln!("[pack] tick {} -> {} chars", turns[i].tick, text.chars().count());
                    narratives.insert(i, text);
                }
                Err(e) => {
                    eprintln!("[pack] tick {} FAILED: {}", turns[i].tick, e);
                    narratives.insert(i, format!("[LLM ERROR] {}", e));
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Markdown
    // ------------------------------------------------------------------
    let mut md = String::new();
    md.push_str("# Narrative Manual Review Pack\n\n");
    md.push_str(&format!(
        "**Сценарий:** `{}` · **сид:** `{}` · **граница:** `{}` тиков · **стратегия:** `{}`\n\n",
        scenario_id,
        seed,
        max_ticks,
        strategy.name()
    ));
    md.push_str(&format!(
        "**Нарратив:** {}\n\n",
        if live {
            "сгенерирован моделью из конфига (`llm::generate_narrative_blocking`)"
        } else {
            "НЕ запрашивался — пакет собран в режиме `dry`, перезапустить с `live`"
        }
    ));
    md.push_str(
        "Метрики каждого кейса — это состояние мира **на тике этого кейса**, не в конце партии, \
         и это `narrative_config.key_metrics` самого сценария, а не фиксированный список.\n\n",
    );
    md.push_str("**Инструкция:** заполнить чек-листы вручную после чтения каждого нарратива.\n\n");
    // The command that regenerates this file, verbatim. This pack's predecessor sat in
    // docs/ for five months while the engine moved out from under it and nobody could
    // tell it had gone stale (§2.3); a file that carries its own reproduction line can
    // at least be re-run by whoever next doubts it.
    md.push_str(&format!(
        "**Пересобрать:** `cargo run --release --bin sim -- {} {} narrative_pack {} {}`\n\n",
        scenario_id,
        if max_ticks_arg > 0 { max_ticks_arg.to_string() } else { "0".to_string() },
        seed,
        if live { "live" } else { "dry" }
    ));
    md.push_str("---\n\n");

    for (idx, outcome) in outcomes.iter().enumerate() {
        md.push_str(&format!("## Case {}: {}\n\n", idx + 1, case_names[idx]));
        let i = match outcome {
            CaseOutcome::NotApplicable(reason) => {
                md.push_str(&format!(
                    "**[НЕПРИМЕНИМ К ЭТОМУ СЦЕНАРИЮ — {}]**\n\nЭто не «не достигнут»: \
                     кейс не может возникнуть здесь ни при какой партии.\n\n---\n\n",
                    reason
                ));
                continue;
            }
            CaseOutcome::NotReached => {
                md.push_str(&format!(
                    "**[НЕ ДОСТИГНУТ за {} тиков]**\n\n---\n\n",
                    max_ticks
                ));
                continue;
            }
            CaseOutcome::Found(i) => *i,
        };
        let t = &turns[i];

        md.push_str(&format!(
            "**Тик:** {} · **год:** {} ({})\n\n",
            t.tick, t.year, t.half_year
        ));

        md.push_str("**Ключевые метрики сценария на этом тике:**\n\n");
        md.push_str("| Метрика | Значение |\n|---|---|\n");
        for (k, v) in &t.key_metrics {
            md.push_str(&format!("| `{}` | {:.1} |\n", k, v));
        }
        md.push('\n');

        md.push_str("**Состояние мира:**\n\n");
        md.push_str(&format!(
            "- под угрозой коллапса: {}\n",
            if t.collapse_warnings.is_empty() {
                "никого".to_string()
            } else {
                t.collapse_warnings.join(", ")
            }
        ));
        md.push_str(&format!(
            "- павшие: {}\n",
            if t.dead_actors.is_empty() {
                "никого".to_string()
            } else {
                t.dead_actors.join(", ")
            }
        ));
        md.push_str(&format!(
            "- победа: {} (условие держится {} тиков подряд)\n",
            if t.victory_achieved { "достигнута" } else { "нет" },
            t.victory_sustained
        ));
        md.push('\n');

        md.push_str(&format!(
            "**Действия игрока в этом полугодии:** {}\n\n",
            if t.actions_applied.is_empty() {
                "нет".to_string()
            } else {
                t.actions_applied.join(", ")
            }
        ));
        md.push_str(&format!(
            "**События, показанные летописцу:** {}\n\n",
            if t.events_shown.is_empty() {
                "нет".to_string()
            } else {
                t.events_shown.join(", ")
            }
        ));

        md.push_str("**Сгенерированный нарратив:**\n\n");
        match narratives.get(&i) {
            Some(text) => {
                md.push_str(text);
                md.push_str("\n\n");
            }
            None => md.push_str("_(режим `dry` — нарратив не запрашивался)_\n\n"),
        }

        md.push_str("---\n\n");
        md.push_str("### Чек-лист (заполнить вручную)\n\n");
        md.push_str("- [ ] Фактическая точность — нет событий, которых нет в снапшоте\n");
        md.push_str("- [ ] Нет выдуманного краха / смерти / победы\n");
        md.push_str("- [ ] Мир сначала — текст не открывается игроком\n");
        md.push_str("- [ ] Голос сценария\n");
        md.push_str("- [ ] Стратегия игрока отражена как причина\n");
        md.push_str("- [ ] Повтор относительно предыдущего полугодия низкий\n");
        md.push_str("- [ ] Объём абзацев\n\n");
        md.push_str("**Заметки рецензента:** _(заполнить)_\n\n");
        md.push_str("**Оценка: /7**\n\n");
        md.push_str("---\n\n");
    }

    let path = format!("docs/narrative_review_pack_{}.md", scenario_id);
    std::fs::write(&path, &md).unwrap_or_else(|e| panic!("Failed to write {}: {}", path, e));

    // Diagnostics: the measured series behind the verdicts, so "НЕ ДОСТИГНУТ" is
    // readable without re-running anything.
    println!();
    println!("=== ИЗМЕРЕННЫЙ РЯД (основание вердиктов) ===");
    println!(
        "  первое предупреждение о коллапсе: {}",
        turns
            .iter()
            .find(|t| !t.collapse_warnings.is_empty())
            .map(|t| format!("тик {} (год {}): {}", t.tick, t.year, t.collapse_warnings.join(", ")))
            .unwrap_or_else(|| "не было".to_string())
    );
    println!(
        "  максимум одновременно под угрозой: {}",
        turns.iter().map(|t| t.collapse_warnings.len()).max().unwrap_or(0)
    );
    println!(
        "  первая гибель актора:             {}",
        turns
            .iter()
            .find(|t| !t.dead_actors.is_empty())
            .map(|t| format!("тик {} (год {}): {}", t.tick, t.year, t.dead_actors.join(", ")))
            .unwrap_or_else(|| "не было".to_string())
    );
    println!(
        "  условие победы впервые держится:  {}",
        turns
            .iter()
            .find(|t| t.victory_sustained >= 1)
            .map(|t| format!("тик {} (год {}), требуется удержать {} тик(ов)", t.tick, t.year, sustained_required))
            .unwrap_or_else(|| "ни разу".to_string())
    );
    println!(
        "  победа достигнута:                {}",
        turns
            .iter()
            .find(|t| t.victory_achieved)
            .map(|t| format!("тик {} (год {})", t.tick, t.year))
            .unwrap_or_else(|| "нет".to_string())
    );
    println!(
        "  смена поколения:                  {}",
        turns
            .iter()
            .find(|t| t.generation_transfer)
            .map(|t| format!("тик {} (год {})", t.tick, t.year))
            .unwrap_or_else(|| "не было".to_string())
    );
    println!();
    println!("Generated {}", path);
    for (idx, outcome) in outcomes.iter().enumerate() {
        match outcome {
            CaseOutcome::Found(i) => println!(
                "  Case {}: tick {} (год {})  {}",
                idx + 1,
                turns[*i].tick,
                turns[*i].year,
                case_names[idx]
            ),
            CaseOutcome::NotReached => {
                println!("  Case {}: НЕ ДОСТИГНУТ            {}", idx + 1, case_names[idx])
            }
            CaseOutcome::NotApplicable(r) => println!(
                "  Case {}: неприменим ({})  {}",
                idx + 1,
                r,
                case_names[idx]
            ),
        }
    }
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
    #[allow(clippy::type_complexity)]
    let mut actor_finals: HashMap<String, (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>)> = HashMap::new();

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
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
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

        // Collected for every scenario, not just rome: задача 18 tests a threshold
        // stated per actor ("an actor collapses that never collapsed in baseline"),
        // and constantinople never emitted the list. Rome's own copy below is left
        // exactly where it was, so rome's output stays byte-identical.
        if scenario_id != "rome_375" {
            for dead_actor in &world.dead_actors {
                collapsed_actors_all.push(dead_actor.id.clone());
            }
        }

        // Per-actor final metrics, same reason: задача 18's acceptance criterion
        // asks for their distributions and no run mode emitted them.
        if scenario_id != "rome_375" {
            for actor in world.actors.values() {
                let e = actor_finals.entry(actor.id.clone()).or_default();
                e.0.push(actor.get_metric("military_size"));
                e.1.push(actor.get_metric("cohesion"));
                e.2.push(actor.get_metric("legitimacy"));
                e.3.push(actor.get_metric("external_pressure"));
            }
        }

        // Rome-specific stats
        if scenario_id == "rome_375" {
            if let Some(rome) = world.actors.get("rome") {
                rome_military_final.push(rome.get_metric("military_size"));
                rome_cohesion_final.push(rome.get_metric("cohesion"));
                rome_legitimacy_final.push(rome.get_metric("legitimacy"));
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

    // Per-actor collapse frequencies. Rome prints its own copy inside the
    // rome-specific block below, so it is excluded here and its output is
    // unchanged — it is the byte-for-byte control for задача 18.
    //
    // Reporting only: this reads stats the batch already collected. Задача 18
    // needs per-actor frequencies to test its rejection threshold ("an actor
    // collapses that never collapsed in baseline"), and no run mode emitted them.
    if scenario_id != "rome_375" && !collapsed_actors_all.is_empty() {
        let mut actor_counts: HashMap<String, u32> = HashMap::new();
        for actor_id in &collapsed_actors_all {
            *actor_counts.entry(actor_id.clone()).or_insert(0) += 1;
        }
        let mut sorted_actors: Vec<_> = actor_counts.iter().collect();
        sorted_actors.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

        println!();
        println!("Collapsed actors (runs out of 100):");
        for (actor_id, count) in sorted_actors {
            println!("  - {}: {}", actor_id, count);
        }
    }

    if scenario_id != "rome_375" && !actor_finals.is_empty() {
        let mean = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
        let mut names: Vec<&String> = actor_finals.keys().collect();
        names.sort();
        println!();
        println!("Final metrics per actor (mean over surviving runs): mil / coh / leg / ep");
        for name in names {
            let m = &actor_finals[name];
            println!(
                "  - {:<12} {:>7.1} {:>7.1} {:>7.1} {:>7.1}   (n={})",
                name, mean(&m.0), mean(&m.1), mean(&m.2), mean(&m.3), m.0.len()
            );
        }
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
            sorted_actors.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            
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
    MilanAggressive,
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
        } else if scenario_id == "milan_1477" {
            // Milan 1477 - only one strategy defined so far (aggressive/expansionist),
            // used to test whether military_size can grow at all under real
            // player-like action-taking (see ENGINE13_SCENARIO3_DESIGN.md,
            // "Найдено при плейтесте C/D" for why this was needed).
            ScriptedStrategy::MilanAggressive
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
            // New priority: exit resource loop early, get to outcome actions
            ScriptedStrategy::RomeBalanced => vec![
                "expand_network",      // FIRST: get connections for build_reputation (uses starting wealth 50)
                "build_reputation",    // PRIMARY: convert to influence (needs connections > 15)
                "support_city",        // SECONDARY: influence + cohesion (needs wealth > 15)
                "back_administration", // TERTIARY: legitimacy + more connections
                "fund_defense",        // LATE: influence + military_quality
                "lay_low",             // Only when influence is high enough to spare
                "invest_wealth",       // Only when connections are high
                "gather_information",  // Only when wealth is high (knowledge has legitimacy bridge now)
                "educate_family",      // Lowest priority: knowledge has no direct sink
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
            // Milan 1477 - aggressive/expansionist: raise troops (military_size),
            // pressure neighbours, destabilize Naples, hire condottieri
            // (military_quality), keep treasury flowing to afford gated actions.
            ScriptedStrategy::MilanAggressive => vec![
                "milan_raise_troops",
                "milan_pressure_genoa",
                "incite_baronial_revolt",
                "milan_hire_condottieri",
                "milan_hire_urbino_condottieri",
                "milan_lease_genoese_fleet",
                "milan_banking_deal_florence",
                "milan_bribe_curia",
                "milan_court_patronage",
                "milan_diplomacy_ferrara",
                "milan_marriage_venice",
                "milan_marriage_naples",
                "call_papal_arbitration",
                "milan_savoy_alliance",
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
            ScriptedStrategy::MilanAggressive => "aggressive",
        }
    }
}

fn run_scripted(scenario_id: &str, ticks: u32, strategy_str: &str, seed: u64) {
    use engine13::application::actions::{apply_player_action, PlayerActionInput, get_available_actions};
    use engine13::commands::AppState;

    let strategy = ScriptedStrategy::from_str(strategy_str, scenario_id);

    println!("Running scripted mode with {} strategy (seed {})", strategy.name(), seed);
    println!();

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

        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
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
        rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
        narrative_memory: engine13::llm::NarrativeMemory::default(),
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
            rome_legitimacy_start = rome.get_metric("legitimacy");
            rome_cohesion_start = rome.get_metric("cohesion");
            rome_military_start = rome.get_metric("military_size");
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
            .map(|a| a.get_metric("external_pressure"))
            .unwrap_or(0.0);
        let _sustained_before = state.world_state.as_ref().unwrap().victory_sustained_ticks;

        // Apply scripted actions before tick using same path as UI
        let mut applied_this_tick = 0u32;
        let mut rejected_this_tick = 0u32;
        let mut actions_applied = Vec::new();

        if scenario_id == "milan_1477" {
            // Disciplined strategy: milan_raise_troops (the only military_size
            // growth lever) always gets first claim on treasury. Everything
            // else is discretionary - only funded from the surplus left over
            // above milan_raise_troops' own gate (treasury > 70), never at
            // its expense. See ENGINE13_SCENARIO3_DESIGN.md, "Найдено при
            // плейтесте C/D" for why the earlier naive priority-list strategy
            // (spend on everything every tick) couldn't sustain growth.
            const RAISE_TROOPS_GATE: f64 = 70.0;
            let raise_input = PlayerActionInput {
                action_id: "milan_raise_troops".to_string(),
                target_actor_id: None,
            };
            let treasury_before = state.world_state.as_ref().unwrap()
                .actors.get("milan").map(|a| a.get_metric("treasury")).unwrap_or(0.0);
            if treasury_before > RAISE_TROOPS_GATE {
                match apply_player_action(&mut state, &raise_input) {
                    Ok(_) => {
                        applied_this_tick += 1;
                        actions_applied.push("milan_raise_troops");
                        *actions_by_type.entry("milan_raise_troops").or_insert(0) += 1;
                    }
                    Err(_) => rejected_this_tick += 1,
                }
            }

            for action_id in priority_actions.iter().filter(|id| **id != "milan_raise_troops") {
                if applied_this_tick >= scenario.actions_per_tick {
                    break;
                }
                let treasury_now = state.world_state.as_ref().unwrap()
                    .actors.get("milan").map(|a| a.get_metric("treasury")).unwrap_or(0.0);
                let surplus = treasury_now - RAISE_TROOPS_GATE;
                if surplus <= 0.0 {
                    break; // preserve the reserve - no discretionary spend below it
                }
                let cost = scenario.patron_actions.iter()
                    .find(|a| a.id == *action_id)
                    .and_then(|a| a.cost.get(&engine13::core::MetricRef::literal("actor:milan.treasury")))
                    .map(|c| -c) // cost values are negative deltas
                    .unwrap_or(f64::MAX);
                if cost > surplus {
                    continue; // can't afford this one without dipping into the reserve
                }

                let action_input = PlayerActionInput {
                    action_id: action_id.to_string(),
                    target_actor_id: None,
                };
                match apply_player_action(&mut state, &action_input) {
                    Ok(_) => {
                        applied_this_tick += 1;
                        actions_applied.push(*action_id);
                        *actions_by_type.entry(*action_id).or_insert(0) += 1;
                    }
                    Err(_) => rejected_this_tick += 1,
                }
            }
        } else {
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
                    }
                    Err(_) => {
                        rejected_this_tick += 1;
                    }
                }
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
            let leg_before = world.actors.get("rome").map(|a| a.get_metric("legitimacy")).unwrap_or(0.0);
            let coh_before = world.actors.get("rome").map(|a| a.get_metric("cohesion")).unwrap_or(0.0);

            let inf_after = world.family_state.as_ref().and_then(|f| f.metrics.get("influence")).copied().unwrap_or(0.0);
            let know_after = world.family_state.as_ref().and_then(|f| f.metrics.get("knowledge")).copied().unwrap_or(0.0);
            let wea_after = world.family_state.as_ref().and_then(|f| f.metrics.get("wealth")).copied().unwrap_or(0.0);
            let con_after = world.family_state.as_ref().and_then(|f| f.metrics.get("connections")).copied().unwrap_or(0.0);
            let leg_after = world.actors.get("rome").map(|a| a.get_metric("legitimacy")).unwrap_or(0.0);
            let coh_after = world.actors.get("rome").map(|a| a.get_metric("cohesion")).unwrap_or(0.0);

            println!("tick {:2}: influence {:6.1}->{:6.1}  knowledge {:5.1}->{:5.1}  wealth {:7.1}->{:7.1}  connections {:6.1}->{:6.1}  legitimacy {:5.1}->{:5.1}  cohesion {:5.1}->{:5.1}  actions=[{}]  applied={} rejected={}",
                tick_num, inf_before, inf_after, know_before, know_after, wea_before, wea_after, con_before, con_after, leg_before, leg_after, coh_before, coh_after,
                actions_applied.join(", "), applied_this_tick, rejected_this_tick);
        } else if scenario_id == "milan_1477" {
            let world = state.world_state.as_ref().unwrap();
            let milan = world.actors.get("milan");
            let mil_after = milan.map(|a| a.get_metric("military_size")).unwrap_or(0.0);
            let treasury_after = milan.map(|a| a.get_metric("treasury")).unwrap_or(0.0);
            let leg_after = milan.map(|a| a.get_metric("legitimacy")).unwrap_or(0.0);
            let ec_after = milan.map(|a| a.get_metric("expansion_count")).unwrap_or(0.0);

            println!("tick {:2}: milan.military_size={:6.2}  treasury={:7.1}  legitimacy={:5.1}  expansion_count={:.0}  actions=[{}]  applied={} rejected={}",
                tick_num, mil_after, treasury_after, leg_after, ec_after,
                actions_applied.join(", "), applied_this_tick, rejected_this_tick);
        } else {
            // Constantinople output
            let _fed_before = state.world_state.as_ref().unwrap()
                .global_metrics.get("federation_progress").copied().unwrap_or(0.0);
            let _pressure_before = state.world_state.as_ref().unwrap()
                .actors.get("byzantium")
                .map(|a| a.get_metric("external_pressure"))
                .unwrap_or(0.0);
            let _sustained_before = state.world_state.as_ref().unwrap().victory_sustained_ticks;

            let fed_after = state.world_state.as_ref().unwrap()
                .global_metrics.get("federation_progress").copied().unwrap_or(0.0);
            let pressure_after = state.world_state.as_ref().unwrap()
                .actors.get("byzantium")
                .map(|a| a.get_metric("external_pressure"))
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
        let rome_legitimacy_final = rome_final.map(|a| a.get_metric("legitimacy")).unwrap_or(0.0);
        let rome_cohesion_final = rome_final.map(|a| a.get_metric("cohesion")).unwrap_or(0.0);
        let rome_military_final = rome_final.map(|a| a.get_metric("military_size")).unwrap_or(0.0);
        
        let family_total_start = family_influence_start + family_wealth_start + family_knowledge_start + family_connections_start;
        let family_total_final = family_influence_final + family_wealth_final + family_knowledge_final + family_connections_final;
        let family_total_delta = family_total_final - family_total_start;

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
        println!();
        println!("Secondary diagnostic:");
        println!("  family_total: {:5.1} -> {:5.1} (delta: {:+.1})", family_total_start, family_total_final, family_total_delta);
        println!();
        println!("Actions applied by type:");
        let mut sorted_actions: Vec<_> = actions_by_type.iter().collect();
        sorted_actions.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (action_id, count) in sorted_actions {
            println!("  - {}: {}", action_id, count);
        }
    } else {
        // Constantinople summary
        let fed_final = world.global_metrics.get("federation_progress").copied().unwrap_or(0.0);
        let byz_final = world.actors.get("byzantium")
            .map(|a| a.get_metric("external_pressure"))
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
        sorted_actions.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
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
            self.byzantium_pressure.push(byz.get_metric("external_pressure"));
            self.byzantium_alive.push(!world.dead_actor_ids.contains("byzantium"));
        }

        // Rome-specific tracking
        if scenario.id == "rome_375" {
            if let Some(rome) = world.actors.get("rome") {
                self.rome_military_timeline.push(rome.get_metric("military_size"));
                self.rome_cohesion_timeline.push(rome.get_metric("cohesion"));
                self.rome_legitimacy_timeline.push(rome.get_metric("legitimacy"));
            }
            
            if let Some(ref family) = world.family_state {
                // Canonical runtime keys, the same space every seeding path
                // produces (`normalize_family_metrics`) and the only one
                // `MetricRef::Family` reads. Spelled raw (`family_influence`),
                // these four missed every time and `.unwrap_or(&0.0)` printed
                // `0.0` for any world state — the other two read paths of this
                // same binary (`run_scripted`, lines ~1168 and ~1243) were
                // already canonical, so one report was true and this one was not.
                self.family_influence_timeline.push(*family.metrics.get("influence").unwrap_or(&0.0));
                self.family_knowledge_timeline.push(*family.metrics.get("knowledge").unwrap_or(&0.0));
                self.family_wealth_timeline.push(*family.metrics.get("wealth").unwrap_or(&0.0));
                self.family_connections_timeline.push(*family.metrics.get("connections").unwrap_or(&0.0));
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
            
            // Count generation transitions by exact event_id
            if event.id == "generation_transfer" {
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
        
        // Count generation transitions by exact event_id
        for event in events {
            if event.id == "generation_transfer" {
                self.generation_transitions += 1;
            }
        }
    }
}
