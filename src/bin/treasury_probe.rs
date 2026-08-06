//! Treasury-flow probe for infrastructure task 19 (ottoman treasury).
//!
//! The task statement claims the ottoman treasury is an inert stock: fed by an
//! engine formula nothing in the scenario authored, read by exactly two common-event
//! gates that are both saturated from tick 1. Every number in it was measured in a
//! **no-player** world, and §4 flags that as the one open hole: the calibration the
//! scenario is balanced against lives in `scripted` mode, and the statement's
//! expectation there ("the surplus only grows") is explicitly marked as reasoned,
//! not measured. This probe closes that hole and makes the whole measurement
//! reproducible rather than a one-off run.
//!
//! What it measures, per actor per run:
//!
//!   1. TREASURY TRAJECTORY: start / final / peak / trough, and the full per-tick
//!      series under `trace`.
//!   2. INCOME AND EXPENSE SEPARATELY. `apply_treasury` (`engine/mod.rs:642`) is the
//!      first thing phase 1 does, so its two terms are fully determined by the state
//!      this probe already holds *before* it calls `tick()`:
//!      `income = economic_output * population * 0.001` and
//!      `expense = military_size * 0.8`.
//!      They are reconstructed here rather than read out of the engine — the engine
//!      is not touched — and the reconstruction is self-checking (see 3).
//!   3. RESIDUAL: `(treasury_after - treasury_before) - (income - expense)`. This is
//!      the exact per-tick contribution of **everything that is not** `apply_treasury`:
//!      random events, the trade interaction, the `economic_output -> treasury`
//!      dependency, vassalage tribute, hardcoded milestone effects. Measuring the
//!      residual is stronger than tallying the event types the statement happens to
//!      name: a writer nobody enumerated still shows up in it.
//!   4. GATE OCCUPANCY: ticks spent above 300 (`mercenary_influx`'s gate) and below
//!      200 (`desertion`'s gate), the first tick each becomes true, and how many times
//!      each flips. A gate that never flips is a constant, not a gate — that is the
//!      whole quantitative claim of §3 of the task, and this is what tests it.
//!   5. THE `POWER_CAP` WINDOW (second pass). `Actor::power_projection` reads treasury
//!      as `min(treasury / 500, 1) * 0.20`, so the stock moves relevance scoring only
//!      **below 500** and is a constant above it. Measured: ticks spent below the cap,
//!      crossings in each direction (is it a threshold or a one-way door?), the stock's
//!      value at every actual Background/Foreground transition, and a counterfactual —
//!      would this actor's own `condition_power` / `low_power` verdict change if every
//!      treasury in the world were saturated? A zero divergence count is proof, not
//!      inference, that the stock cannot have moved that actor's relevance.
//!   6. `military_size` MINIMUM per run, for the separate job of re-measuring whether
//!      "ottomans ground to 0.00 in 27/30" still holds after PR #32 (task 18 changed
//!      the scenario and re-measured other aggregates, not this one).
//!
//! In `scripted` mode the driver replicates `sim.rs::run_scripted` exactly for the
//! non-Milan path: same strategy priority lists, same `apply_player_action` call
//! order, same `actions_per_tick` budget, same early termination on victory or
//! byzantium's death. `apply_player_action` draws no RNG, so a run here consumes the
//! same RNG stream as the corresponding `sim` run and is comparable to the calibration
//! rather than merely similar to it.
//!
//! Read-only: it drives `tick()` and `apply_player_action` and reads metrics. No engine
//! symbol is modified and no RNG is drawn outside them, so the simulation it observes is
//! the one the baselines are built from.
//!
//! Usage:
//! ```bash
//! cargo run --release --bin treasury_probe -- <scenario> <ticks> <mode> <strategy> <seeds> [trace_actor]
//! # mode = noplayer | scripted ; strategy is ignored for noplayer but must be present
//! cargo run --release --bin treasury_probe -- constantinople_1430 300 noplayer - 42,1,7
//! cargo run --release --bin treasury_probe -- constantinople_1430 300 scripted balanced 42 ottomans
//! ```
//! Emits `RUN` / `ACTOR` / `EVENT` (and `TRACE` when a trace actor is named) TSV lines.

use engine13::application::actions::{apply_player_action, PlayerActionInput};
use engine13::commands::AppState;
use engine13::{
    core::WorldState,
    engine::{tick, EventLog},
    scenarios::registry,
};
use rand::SeedableRng;
use std::collections::HashMap;

/// `mercenary_influx` fires only above this treasury; `desertion` only below
/// MERCENARY_GATE's counterpart. Both live in `events/common.rs` — duplicated here
/// as observation thresholds, not as engine behaviour.
const MERCENARY_GATE: f64 = 300.0;
const DESERTION_GATE: f64 = 200.0;

/// `Actor::power_projection`'s treasury cap (`core/actor.rs:173`). Treasury enters
/// relevance scoring as `min(treasury / 500, 1) * 0.20`, so **below** this value the
/// stock moves `power_projection` and above it contributes a constant 0.20. This is
/// the live window the stock has left after the two common-event gates saturate, and
/// measuring it is the whole point of the second pass.
const POWER_CAP: f64 = 500.0;
const W_MIL: f64 = 0.45;
const W_QUALITY: f64 = 0.35;
const W_TREASURY: f64 = 0.20;

/// `apply_treasury`'s two coefficients (`engine/mod.rs:647-648`). Duplicated for the
/// reconstruction in (2) above; the residual in (3) is what proves the duplication
/// is faithful — a wrong constant here would show up as a constant nonzero residual.
const INCOME_COEFF: f64 = 0.001;
const UPKEEP_PER_UNIT: f64 = 0.8;

/// `power_projection` recomputed here rather than called, so the same function can be
/// evaluated **counterfactually** — with every actor's treasury term forced to its
/// saturated value. Verified against `Actor::power_projection` by construction: same
/// weights, same normalisations, same clamps (`core/actor.rs:171-188`).
fn power_projection(mil: f64, quality: f64, treasury: f64, max_mil: f64, saturate: bool) -> f64 {
    let mil_norm = if max_mil > 0.0 { (mil / max_mil).clamp(0.0, 1.0) } else { 0.0 };
    let quality_norm = (quality / 100.0).clamp(0.0, 1.0);
    let treasury_norm = if saturate {
        1.0
    } else {
        (treasury / POWER_CAP).clamp(0.0, 1.0)
    };
    (mil_norm * W_MIL + quality_norm * W_QUALITY + treasury_norm * W_TREASURY) * 100.0
}

/// Scripted priority lists, copied verbatim from
/// `sim.rs::ScriptedStrategy::priority_actions`. Kept as a copy rather than shared:
/// `sim.rs` is a binary, its strategy enum is private to it, and making it public
/// would be an engine-visible change in a task whose acceptance is byte-identical sim.
fn priority_actions(scenario_id: &str, strategy: &str) -> Vec<&'static str> {
    if scenario_id == "rome_375" {
        return match strategy {
            "influence" | "influence_heavy" => vec![
                "build_reputation",
                "support_city",
                "fund_defense",
                "back_administration",
                "expand_network",
                "educate_family",
                "invest_wealth",
                "gather_information",
                "lay_low",
            ],
            "wealth" | "wealth_heavy" => vec![
                "lay_low",
                "invest_wealth",
                "gather_information",
                "expand_network",
                "educate_family",
                "support_city",
                "back_administration",
                "build_reputation",
                "fund_defense",
            ],
            _ => vec![
                "expand_network",
                "build_reputation",
                "support_city",
                "back_administration",
                "fund_defense",
                "lay_low",
                "invest_wealth",
                "gather_information",
                "educate_family",
            ],
        };
    }
    if scenario_id == "milan_1477" {
        return vec![
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
        ];
    }
    match strategy {
        "diplomacy" | "diplomatic" => vec![
            "venice_diplomacy",
            "genoa_financial_aid",
            "milan_bankers",
            "venice_trade_deal",
            "genoa_galata_garrison",
            "venice_naval_support",
            "genoa_mercenaries",
            "milan_condottieri",
        ],
        "military" | "military_heavy" => vec![
            "venice_naval_support",
            "genoa_mercenaries",
            "milan_condottieri",
            "genoa_galata_garrison",
            "venice_diplomacy",
            "genoa_financial_aid",
            "milan_bankers",
            "venice_trade_deal",
        ],
        _ => vec![
            "venice_diplomacy",
            "genoa_financial_aid",
            "milan_bankers",
            "venice_naval_support",
            "genoa_mercenaries",
            "milan_condottieri",
            "venice_trade_deal",
            "genoa_galata_garrison",
        ],
    }
}

#[derive(Clone)]
struct ActorTrace {
    start: Option<f64>,
    final_v: f64,
    peak: f64,
    trough: f64,
    income_sum: f64,
    expense_sum: f64,
    residual_sum: f64,
    action_sum: f64,
    ticks_alive: u32,
    above_gate: u32,
    below_gate: u32,
    first_above: Option<u32>,
    first_below: Option<u32>,
    flips_above: u32,
    flips_below: u32,
    prev_above: Option<bool>,
    prev_below: Option<bool>,
    mil_min: f64,
    mil_final: f64,
    spawned: bool,
    // --- POWER_CAP window (second pass) --------------------------------------
    below_cap: u32,
    first_below_cap: Option<u32>,
    first_at_cap: Option<u32>,
    cap_up: u32,
    cap_down: u32,
    prev_at_cap: Option<bool>,
    /// actor-ticks where the actor's own `condition_power` / `low_power` verdict
    /// differs between the real world and the treasury-saturated counterfactual
    cf_divergent: u32,
}

impl ActorTrace {
    fn new(start: Option<f64>, spawned: bool) -> Self {
        let s = start.unwrap_or(0.0);
        Self {
            start,
            final_v: s,
            peak: s,
            trough: s,
            income_sum: 0.0,
            expense_sum: 0.0,
            residual_sum: 0.0,
            action_sum: 0.0,
            ticks_alive: 0,
            above_gate: 0,
            below_gate: 0,
            first_above: None,
            first_below: None,
            flips_above: 0,
            flips_below: 0,
            prev_above: None,
            prev_below: None,
            mil_min: f64::MAX,
            mil_final: 0.0,
            spawned,
            below_cap: 0,
            first_below_cap: None,
            first_at_cap: None,
            cap_up: 0,
            cap_down: 0,
            prev_at_cap: None,
            cf_divergent: 0,
        }
    }
}

struct RunResult {
    ticks_run: u32,
    victory: bool,
    victory_tick: Option<u32>,
    byz_dead_tick: Option<u32>,
    traces: HashMap<String, ActorTrace>,
    events: HashMap<(String, String), u32>,
}

#[allow(clippy::too_many_arguments)]
fn run(
    scenario_id: &str,
    ticks: u32,
    scripted: bool,
    strategy: &str,
    seed: u64,
    trace_actor: Option<&str>,
) -> RunResult {
    let scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");

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

    let mut traces: HashMap<String, ActorTrace> = world
        .actors
        .iter()
        .map(|(id, a)| (id.clone(), ActorTrace::new(Some(a.get_metric("treasury")), false)))
        .collect();

    let mut state = AppState {
        world_state: Some(world),
        event_log: EventLog::new(),
        current_scenario: Some(scenario.clone()),
        rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
        narrative_memory: engine13::llm::NarrativeMemory::default(),
    };

    let priorities = priority_actions(scenario_id, strategy);
    let mut victory_tick = None;
    let mut byz_dead_tick = None;
    let mut ticks_run = 0u32;
    let mut deaths_seen = 0u32;
    let mut prev_status: HashMap<String, bool> = state
        .world_state
        .as_ref()
        .unwrap()
        .actors
        .iter()
        .map(|(id, a)| {
            (
                id.clone(),
                a.narrative_status == engine13::core::NarrativeStatus::Foreground,
            )
        })
        .collect();

    for _ in 0..ticks {
        // --- player actions, replicating sim.rs::run_scripted's non-Milan path ----
        let mut pre_action: HashMap<String, f64> = HashMap::new();
        if scripted {
            for (id, a) in &state.world_state.as_ref().unwrap().actors {
                pre_action.insert(id.clone(), a.get_metric("treasury"));
            }
            let per_tick = state
                .current_scenario
                .as_ref()
                .unwrap()
                .actions_per_tick;
            let mut applied = 0u32;
            if scenario_id == "milan_1477" {
                // Milan's disciplined strategy, replicated from sim.rs: raise_troops
                // gets first claim on treasury, everything else only from the surplus
                // above its own gate.
                const RAISE_TROOPS_GATE: f64 = 70.0;
                let treasury_before = state
                    .world_state
                    .as_ref()
                    .unwrap()
                    .actors
                    .get("milan")
                    .map(|a| a.get_metric("treasury"))
                    .unwrap_or(0.0);
                if treasury_before > RAISE_TROOPS_GATE {
                    let input = PlayerActionInput {
                        action_id: "milan_raise_troops".to_string(),
                        target_actor_id: None,
                    };
                    if apply_player_action(&mut state, &input).is_ok() {
                        applied += 1;
                    }
                }
                for action_id in priorities.iter().filter(|id| **id != "milan_raise_troops") {
                    if applied >= per_tick {
                        break;
                    }
                    let treasury_now = state
                        .world_state
                        .as_ref()
                        .unwrap()
                        .actors
                        .get("milan")
                        .map(|a| a.get_metric("treasury"))
                        .unwrap_or(0.0);
                    let surplus = treasury_now - RAISE_TROOPS_GATE;
                    if surplus <= 0.0 {
                        break;
                    }
                    let cost = state
                        .current_scenario
                        .as_ref()
                        .unwrap()
                        .patron_actions
                        .iter()
                        .find(|a| a.id == *action_id)
                        .and_then(|a| {
                            a.cost
                                .get(&engine13::core::MetricRef::literal("actor:milan.treasury"))
                        })
                        .map(|c| -c)
                        .unwrap_or(f64::MAX);
                    if cost > surplus {
                        continue;
                    }
                    let input = PlayerActionInput {
                        action_id: action_id.to_string(),
                        target_actor_id: None,
                    };
                    if apply_player_action(&mut state, &input).is_ok() {
                        applied += 1;
                    }
                }
            } else {
                for action_id in &priorities {
                    if applied >= per_tick {
                        break;
                    }
                    let input = PlayerActionInput {
                        action_id: action_id.to_string(),
                        target_actor_id: None,
                    };
                    if apply_player_action(&mut state, &input).is_ok() {
                        applied += 1;
                    }
                }
            }
        }

        // --- state that `apply_treasury` will read at the top of the next tick ----
        let mut pre_tick: HashMap<String, (f64, f64, f64)> = HashMap::new();
        for (id, a) in &state.world_state.as_ref().unwrap().actors {
            pre_tick.insert(
                id.clone(),
                (
                    a.get_metric("treasury"),
                    a.get_metric("economic_output") * a.get_metric("population") * INCOME_COEFF,
                    a.get_metric("military_size") * UPKEEP_PER_UNIT,
                ),
            );
        }

        {
            let world_state = state.world_state.as_mut().unwrap();
            let scenario_ref = state.current_scenario.as_ref().unwrap();
            let rng = state.rng.as_mut().unwrap();
            tick(world_state, scenario_ref, &mut state.event_log, rng);
        }
        ticks_run += 1;

        let world = state.world_state.as_ref().unwrap();
        let now = world.tick;

        // --- relevance scoring, real and treasury-saturated ----------------------
        // `check_relevance_thresholds` runs in phase 6 and nothing writes `treasury`
        // or `military_size` after phase 3, so the post-tick state this reads is the
        // state it scored on — except on a tick where an actor collapsed (phase 7),
        // which changes both `max_military_size` and `world.actors.len()`. Those ticks
        // are flagged in the output rather than silently averaged in.
        let max_mil = world
            .actors
            .values()
            .map(|a| a.get_metric("military_size"))
            .fold(1.0_f64, f64::max);
        let n_actors = world.actors.len().max(1) as f64;
        let (mut sum_pp, mut sum_pp_cf, mut below_cap_now) = (0.0, 0.0, 0u32);
        for a in world.actors.values() {
            let (m, q, t) = (
                a.get_metric("military_size"),
                a.get_metric("military_quality"),
                a.get_metric("treasury"),
            );
            sum_pp += power_projection(m, q, t, max_mil, false);
            sum_pp_cf += power_projection(m, q, t, max_mil, true);
            if t < POWER_CAP {
                below_cap_now += 1;
            }
        }
        let avg_pp = sum_pp / n_actors;
        let avg_pp_cf = sum_pp_cf / n_actors;
        let died_this_tick = world.dead_actor_ids.len() as u32 != deaths_seen;
        deaths_seen = world.dead_actor_ids.len() as u32;

        for (id, a) in &world.actors {
            let tr = a.get_metric("treasury");
            let mil = a.get_metric("military_size");
            let (before, income, expense) = pre_tick
                .get(id)
                .copied()
                .unwrap_or((0.0, 0.0, 0.0)); // actor spawned inside this tick
            let spawned_now = !pre_tick.contains_key(id);
            let residual = (tr - before) - (income - expense);

            let e = traces
                .entry(id.clone())
                .or_insert_with(|| ActorTrace::new(None, spawned_now));
            if scripted {
                if let Some(pa) = pre_action.get(id) {
                    e.action_sum += before - pa;
                }
            }
            e.final_v = tr;
            e.peak = e.peak.max(tr);
            e.trough = e.trough.min(tr);
            e.income_sum += income;
            e.expense_sum += expense;
            if !spawned_now {
                e.residual_sum += residual;
            }
            e.ticks_alive += 1;
            e.mil_min = e.mil_min.min(mil);
            e.mil_final = mil;

            let above = tr > MERCENARY_GATE;
            let below = tr < DESERTION_GATE;
            if above {
                e.above_gate += 1;
                if e.first_above.is_none() {
                    e.first_above = Some(now);
                }
            }
            if below {
                e.below_gate += 1;
                if e.first_below.is_none() {
                    e.first_below = Some(now);
                }
            }
            if e.prev_above.is_some_and(|p| p != above) {
                e.flips_above += 1;
            }
            if e.prev_below.is_some_and(|p| p != below) {
                e.flips_below += 1;
            }
            e.prev_above = Some(above);
            e.prev_below = Some(below);

            // --- POWER_CAP window: is the stock still moving power_projection? ----
            let at_cap = tr >= POWER_CAP;
            if !at_cap {
                e.below_cap += 1;
                if e.first_below_cap.is_none() {
                    e.first_below_cap = Some(now);
                }
            } else if e.first_at_cap.is_none() {
                e.first_at_cap = Some(now);
            }
            if let Some(prev) = e.prev_at_cap {
                if prev != at_cap {
                    if at_cap {
                        e.cap_up += 1;
                    } else {
                        e.cap_down += 1;
                    }
                }
            }
            e.prev_at_cap = Some(at_cap);

            // --- counterfactual: would this actor's relevance verdict change if
            // every treasury were saturated? `condition_power` for a background
            // actor, `low_power` for a foreground one — the only two places the
            // stock enters relevance at all.
            let pp = power_projection(
                a.get_metric("military_size"),
                a.get_metric("military_quality"),
                tr,
                max_mil,
                false,
            );
            let pp_cf = power_projection(
                a.get_metric("military_size"),
                a.get_metric("military_quality"),
                tr,
                max_mil,
                true,
            );
            let background = a.narrative_status == engine13::core::NarrativeStatus::Background;
            let verdict = if background {
                pp > avg_pp * 0.7
            } else {
                pp < avg_pp * 0.4
            };
            let verdict_cf = if background {
                pp_cf > avg_pp_cf * 0.7
            } else {
                pp_cf < avg_pp_cf * 0.4
            };
            if verdict != verdict_cf {
                e.cf_divergent += 1;
                println!(
                    "CFDIV\t{}\t{}\t{}\tbackground={}\ttreasury={:.2}\tat_cap={}\tpp={:.2}\tavg={:.2}\tpp_cf={:.2}\tavg_cf={:.2}\treal={}\tcf={}\tdeath_tick={}",
                    seed, id, now, background, tr, at_cap, pp, avg_pp, pp_cf, avg_pp_cf,
                    verdict, verdict_cf, died_this_tick
                );
            }

            // --- every actual status transition, with the stock at that moment ----
            let was = prev_status.get(id).copied();
            let is_fg = a.narrative_status == engine13::core::NarrativeStatus::Foreground;
            if was.is_some_and(|w| w != is_fg) {
                println!(
                    "STATUS\t{}\t{}\t{}\t{}\ttreasury={:.2}\tat_cap={}\tbelow_cap_actors={}\tpp={:.2}\tavg70={:.2}\tcond_power={}\tcond_power_cf={}\tdeath_tick={}",
                    seed,
                    id,
                    now,
                    if is_fg { "promoted" } else { "demoted" },
                    tr,
                    at_cap,
                    below_cap_now,
                    pp,
                    avg_pp * 0.7,
                    pp > avg_pp * 0.7,
                    pp_cf > avg_pp_cf * 0.7,
                    died_this_tick
                );
            }
            prev_status.insert(id.clone(), is_fg);

            if trace_actor == Some(id.as_str()) {
                println!(
                    "TRACE\t{}\t{}\t{}\ttreasury={:.2}\tincome={:.2}\texpense={:.2}\tresidual={:+.2}\teo={:.1}\tpop={:.1}\tmil={:.2}",
                    seed, id, now, tr, income, expense, residual,
                    a.get_metric("economic_output"),
                    a.get_metric("population"),
                    mil
                );
            }
        }

        if world.victory_achieved && victory_tick.is_none() {
            victory_tick = Some(now);
        }
        if byz_dead_tick.is_none() && world.dead_actor_ids.iter().any(|id| id.contains("byzantium"))
        {
            byz_dead_tick = Some(now);
        }

        // Early termination — identical to sim.rs::run_scripted. Only applied in
        // scripted mode: the no-player baseline runs the full horizon.
        if scripted
            && (world.victory_achieved
                || (scenario_id != "rome_375"
                    && world.dead_actor_ids.iter().any(|id| id.contains("byzantium"))))
        {
            break;
        }
    }

    let mut events: HashMap<(String, String), u32> = HashMap::new();
    for ev in &state.event_log.events {
        // `metrics_<actor>_<tick>` is bookkeeping emitted by `record_metric_changes`
        // once per actor per tick (`engine/mod.rs:1649`), not a content event. It is
        // the only id family excluded, and it is excluded by construction, not by
        // judgement about which events matter.
        if ev.id.starts_with("metrics_") {
            continue;
        }
        *events.entry((ev.actor_id.clone(), ev.id.clone())).or_insert(0) += 1;
    }

    let world = state.world_state.as_ref().unwrap();
    RunResult {
        ticks_run,
        victory: world.victory_achieved,
        victory_tick,
        byz_dead_tick,
        traces,
        events,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("constantinople_1430");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let mode = args.get(3).map(|s| s.as_str()).unwrap_or("noplayer");
    let strategy = args.get(4).map(|s| s.as_str()).unwrap_or("balanced");
    let seeds: Vec<u64> = args
        .get(5)
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_else(|| vec![42]);
    let trace_actor = args.get(6).map(|s| s.as_str());
    let scripted = mode == "scripted";

    for seed in seeds {
        let r = run(scenario_id, ticks, scripted, strategy, seed, trace_actor);

        println!(
            "RUN\t{}\t{}\t{}\t{}\tticks_run={}\tvictory={}\tvictory_tick={}\tbyz_dead_tick={}",
            scenario_id,
            mode,
            if scripted { strategy } else { "-" },
            seed,
            r.ticks_run,
            r.victory,
            r.victory_tick.map(|t| t.to_string()).unwrap_or_else(|| "-".into()),
            r.byz_dead_tick.map(|t| t.to_string()).unwrap_or_else(|| "-".into()),
        );

        let mut ids: Vec<&String> = r.traces.keys().collect();
        ids.sort();
        for id in ids {
            let t = &r.traces[id];
            let observed = t.final_v - t.start.unwrap_or(0.0);
            let predicted = t.income_sum - t.expense_sum + t.action_sum;
            println!(
                "ACTOR\t{}\t{}\tstart={}\tfinal={:.2}\tpeak={:.2}\ttrough={:.2}\tincome={:.2}\texpense={:.2}\taction={:+.2}\tresidual={:+.2}\tobserved_delta={:.2}\tcheck={:+.4}\tticks={}\tabove300={}\tbelow200={}\tfirst_above300={}\tfirst_below200={}\tflips300={}\tflips200={}\tmil_min={:.2}\tmil_final={:.2}\tspawned={}\tbelow500={}\tfirst_below500={}\tfirst_at500={}\tcap_up={}\tcap_down={}\tcf_divergent={}",
                seed,
                id,
                t.start.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "-".into()),
                t.final_v,
                t.peak,
                t.trough,
                t.income_sum,
                t.expense_sum,
                t.action_sum,
                t.residual_sum,
                observed,
                observed - (predicted + t.residual_sum),
                t.ticks_alive,
                t.above_gate,
                t.below_gate,
                t.first_above.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                t.first_below.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                t.flips_above,
                t.flips_below,
                if t.mil_min == f64::MAX { 0.0 } else { t.mil_min },
                t.mil_final,
                t.spawned,
                t.below_cap,
                t.first_below_cap.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                t.first_at_cap.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                t.cap_up,
                t.cap_down,
                t.cf_divergent,
            );
        }

        let mut evs: Vec<(&(String, String), &u32)> = r.events.iter().collect();
        evs.sort();
        for ((actor, ev), n) in evs {
            println!("EVENT\t{}\t{}\t{}\t{}", seed, actor, ev, n);
        }
    }
}
