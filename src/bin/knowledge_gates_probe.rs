//! Read-only probe for задача 17: the `family_knowledge > 40 / > 50` gates.
//!
//! Задача 16 §3.1 established that задача 15 pushed `knowledge` across two
//! thresholds that no run had ever reached (max 28.7 before, 82.7 after), and that
//! three `auto_deltas` rules of rome are gated on them. It could not say whether
//! anything followed, for two reasons this probe removes:
//!
//!   1. `sim` never prints `rome.economic_output` per tick — and one of the three
//!      rules writes *only* there (`base: 0.0`, single condition, `noise: 0.0`).
//!   2. The value the engine tests is `MetricRef::literal("family:family_knowledge")
//!      .get(world)`, which goes through `canonical_family_key`. Reading the report
//!      column is not the same measurement.
//!
//! What this probe CANNOT do, and does not pretend to: observe state *between*
//! engine phases. Every `phase_*` is private; only `pub fn tick` is exported, and
//! nine phases run after `auto_deltas` (the `20.0` legitimacy floor among them).
//! Separating "delta never applied" from "delta applied and absorbed" needs the
//! counterfactual builds, not this binary. See the задача 17 statement.
//!
//! The run is a byte-faithful replica of `sim rome_375 <ticks> scripted wealth`:
//! same seed, same priority list, same action loop, same `tick` call. `--validate`
//! checks that replication against the numbers задача 16 measured, so a drift in
//! the replica cannot masquerade as a finding.
//!
//! Usage:
//!   knowledge_gates_probe [ticks] [--validate]

use engine13::application::actions::{apply_player_action, PlayerActionInput};
use engine13::commands::AppState;
use engine13::core::{MetricRef, WorldState};
use engine13::engine::{tick, EventLog};
use engine13::scenarios::registry;
use rand::SeedableRng;

/// `ScriptedStrategy::RomeWealth` priority list, copied verbatim from
/// `sim.rs`. Replicated rather than imported: `sim.rs` is a binary, not a library
/// target, so there is nothing to import. Any drift here breaks `--validate`.
const ROME_WEALTH_PRIORITY: &[&str] = &[
    "lay_low",
    "invest_wealth",
    "gather_information",
    "expand_network",
    "educate_family",
    "support_city",
    "back_administration",
    "build_reputation",
    "fund_defense",
];

const SEED: u64 = 42;

/// The two thresholds under investigation, and the rules gated on them.
const GATE_LOW: f64 = 40.0; // rome.legitimacy +0.1 ; rome.economic_output +0.3
const GATE_HIGH: f64 = 50.0; // family_knowledge +0.1 (the feedback loop)

struct TickRow {
    tick: u32,
    knowledge: f64,
    economic_output: f64,
    legitimacy: f64,
    cohesion: f64,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ticks: u32 = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let validate = args.iter().any(|a| a == "--validate");

    let scenario = registry::load_by_id("rome_375").expect("rome_375 must load");
    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, SEED);

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
        rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(SEED)),
        narrative_memory: engine13::llm::NarrativeMemory::default(),
    };

    // The value the engine tests, reached the way the engine reaches it.
    let knowledge_ref = MetricRef::literal("family:family_knowledge");
    let econ_ref = MetricRef::literal("actor:rome.economic_output");
    let legit_ref = MetricRef::literal("actor:rome.legitimacy");
    let cohesion_ref = MetricRef::literal("actor:rome.cohesion");

    let mut rows: Vec<TickRow> = Vec::with_capacity(ticks as usize);
    let mut applied_total = 0u32;
    let mut rejected_total = 0u32;
    let mut gate_low_tick: Option<u32> = None;
    let mut gate_high_tick: Option<u32> = None;

    println!("# knowledge gates probe — rome_375, scripted wealth, seed {SEED}, {ticks} ticks");
    println!("# knowledge read as MetricRef::literal(\"family:family_knowledge\").get(world)");
    println!("# gate_low = knowledge > {GATE_LOW}  gate_high = knowledge > {GATE_HIGH}");
    println!("tick\tknowledge\tgate_low\tgate_high\teconomic_output\tlegitimacy\tcohesion");

    for tick_num in 0..ticks {
        let mut applied_this_tick = 0u32;
        for action_id in ROME_WEALTH_PRIORITY {
            if applied_this_tick >= state.current_scenario.as_ref().unwrap().actions_per_tick {
                break;
            }
            let input = PlayerActionInput {
                action_id: (*action_id).to_string(),
                target_actor_id: None,
            };
            match apply_player_action(&mut state, &input) {
                Ok(_) => applied_this_tick += 1,
                Err(_) => rejected_total += 1,
            }
        }
        applied_total += applied_this_tick;

        let world_state = state.world_state.as_mut().unwrap();
        let scenario_ref = state.current_scenario.as_ref().unwrap();
        let rng = state.rng.as_mut().unwrap();
        tick(world_state, scenario_ref, &mut state.event_log, rng);

        let w = state.world_state.as_ref().unwrap();
        let row = TickRow {
            tick: tick_num,
            knowledge: knowledge_ref.get(w),
            economic_output: econ_ref.get(w),
            legitimacy: legit_ref.get(w),
            cohesion: cohesion_ref.get(w),
        };

        if row.knowledge > GATE_LOW && gate_low_tick.is_none() {
            gate_low_tick = Some(row.tick);
        }
        if row.knowledge > GATE_HIGH && gate_high_tick.is_none() {
            gate_high_tick = Some(row.tick);
        }

        println!(
            "{}\t{:.4}\t{}\t{}\t{:.4}\t{:.4}\t{:.4}",
            row.tick,
            row.knowledge,
            u8::from(row.knowledge > GATE_LOW),
            u8::from(row.knowledge > GATE_HIGH),
            row.economic_output,
            row.legitimacy,
            row.cohesion,
        );
        rows.push(row);

        if state.world_state.as_ref().unwrap().victory_achieved {
            println!("# early termination: victory at tick {tick_num}");
            break;
        }
    }

    summary(&rows, gate_low_tick, gate_high_tick, applied_total, rejected_total);

    if validate {
        validate_replication(&rows, applied_total, rejected_total);
    }
}

fn summary(
    rows: &[TickRow],
    gate_low_tick: Option<u32>,
    gate_high_tick: Option<u32>,
    applied: u32,
    rejected: u32,
) {
    let last = rows.last().expect("at least one tick");
    let max_knowledge = rows.iter().map(|r| r.knowledge).fold(f64::MIN, f64::max);
    let max_econ = rows.iter().map(|r| r.economic_output).fold(f64::MIN, f64::max);
    let min_econ = rows.iter().map(|r| r.economic_output).fold(f64::MAX, f64::min);
    let econ_at_100 = rows.iter().filter(|r| r.economic_output >= 99.95).count();
    let legit_distinct = {
        let mut v: Vec<String> = rows.iter().map(|r| format!("{:.1}", r.legitimacy)).collect();
        v.sort();
        v.dedup();
        v.len()
    };

    println!();
    println!("# --- summary ---");
    println!("# ticks run:              {}", rows.len());
    println!("# actions applied/rejected: {applied}/{rejected}");
    println!(
        "# gate_low  (>{GATE_LOW}) opens at tick: {}",
        gate_low_tick.map_or("never".to_string(), |t| t.to_string())
    );
    println!(
        "# gate_high (>{GATE_HIGH}) opens at tick: {}",
        gate_high_tick.map_or("never".to_string(), |t| t.to_string())
    );
    println!("# knowledge  final {:.4}  max {max_knowledge:.4}", last.knowledge);
    println!(
        "# economic_output final {:.4}  min {min_econ:.4}  max {max_econ:.4}  ticks at ceiling {econ_at_100}",
        last.economic_output
    );
    println!("# legitimacy final {:.4}  distinct printed values {legit_distinct}", last.legitimacy);
    println!("# cohesion   final {:.4}", last.cohesion);
}

/// The replica must reproduce задача 16's measured run, or nothing measured here
/// means anything. Numbers from `docs/investigation_rome_arc.md` §3, cell
/// `rome_375 300 scripted wealth` at `4338a1c`.
fn validate_replication(rows: &[TickRow], applied: u32, rejected: u32) {
    let last = rows.last().expect("at least one tick");
    let checks: [(&str, f64, f64, f64); 4] = [
        ("ticks", rows.len() as f64, 300.0, 0.0),
        ("actions applied", applied as f64, 600.0, 0.0),
        ("actions rejected", rejected as f64, 2.0, 0.0),
        ("knowledge final", last.knowledge, 82.7, 0.05),
    ];

    println!();
    println!("# --- replication check against задача 16 §3 ---");
    let mut ok = true;
    for (name, actual, expected, tol) in checks {
        let pass = (actual - expected).abs() <= tol;
        ok &= pass;
        println!(
            "# {:<18} actual {:>10.4}  expected {:>8.4}  {}",
            name,
            actual,
            expected,
            if pass { "OK" } else { "MISMATCH" }
        );
    }
    println!(
        "# replication: {}",
        if ok {
            "OK — probe reproduces the measured run"
        } else {
            "FAILED — probe does not reproduce the run; findings below are void"
        }
    );
    if !ok {
        std::process::exit(1);
    }
}
