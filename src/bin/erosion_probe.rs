//! Erosion-trajectory probe for infrastructure task 11, stage 1 (design).
//!
//! Task 10 measured *that* the vassalage band is unreachable and *why* in outline:
//! the `external_pressure` window (70-85) opens at median tick 8-20, the
//! `legitimacy` window (10-25) at median tick 58-87, and they never coincide.
//! Task 11 proposes to fix the trigger by making pressure the *cause* of erosion,
//! and its stage 1 requires N (how long `ep` must hold) and delta (how hard it
//! erodes) to be *derived* from measured magnitudes rather than guessed -- plus an
//! analytic proof that eroded `leg`/`coh` reach their windows *before* `ep` crosses
//! 85 into `classic_collapse` territory.
//!
//! Neither number is derivable from what task 10 emitted. This probe measures the
//! four missing trajectory quantities, per actor, per run:
//!
//!   1. TRANSIT: ticks between `ep >= 70` and `ep > 85`. This is the entire time
//!      budget the erosion formula has to work with -- the collision risk the task
//!      statement names, quantified.
//!   2. HEIGHT: `legitimacy`/`cohesion` at the moment `ep` first reaches 70. This is
//!      the distance erosion must cover, and (with 1) it fixes delta.
//!   3. NATURAL SLOPE: how fast `leg`/`coh` already fall on their own after `ep`
//!      arrives -- the existing magnitude any designed delta must be stated against
//!      (the Milan 1477 calibration discipline: derive from what is there, don't
//!      invent a number).
//!   4. CEILING COUNTERFACTUAL: actor-ticks that would satisfy the band if the
//!      `ep <= 85` ceiling were dropped (`ep >= 70` AND leg 10-25 AND coh 15-30),
//!      including the longest consecutive streak (formation needs 3). This asks
//!      whether erosion is needed *at all*, or whether the ceiling alone is the
//!      blocker -- data before design, per task 10's discipline.
//!
//! Read-only: it drives `tick()` and reads metrics. No engine symbol is modified and
//! no RNG is drawn outside the engine, so the simulation it observes is the one the
//! baselines are built from.
//!
//! Usage:
//! ```bash
//! cargo run --release --bin erosion_probe -- <scenario> <ticks> <seed>
//! ```
//! Emits one `ACTOR<TAB>...` line per actor plus one `SUMMARY<TAB>...` line per run.

use engine13::{core::WorldState, engine::{tick, EventLog}, scenarios::registry};
use rand::SeedableRng;
use std::collections::HashMap;

const EP_LO: f64 = 70.0;
const EP_HI: f64 = 85.0;
const LEG_LO: f64 = 10.0;
const LEG_HI: f64 = 25.0;
const COH_LO: f64 = 15.0;
const COH_HI: f64 = 30.0;

/// Sustained-pressure durations to profile. A duration-gated erosion rule needs an
/// N; these are the candidates whose *state at trigger time* we measure.
const N_CANDIDATES: [u32; 4] = [3, 5, 10, 20];

#[derive(Default, Clone)]
struct Trace {
    // 1. transit
    t_ep70: Option<u32>,
    t_ep85: Option<u32>,
    // 2. height at the moment pressure arrives
    leg_at_ep70: Option<f64>,
    coh_at_ep70: Option<f64>,
    // 3. natural slope after pressure arrives (leg/coh sampled at t_ep70 + 20/40)
    leg_at_p20: Option<f64>,
    leg_at_p40: Option<f64>,
    coh_at_p20: Option<f64>,
    coh_at_p40: Option<f64>,
    // leg's own descent through its window
    t_leg_band: Option<u32>,
    t_leg_below: Option<u32>,
    coh_at_leg_band: Option<f64>,
    ep_at_leg_band: Option<f64>,
    // 4. ceiling counterfactual
    both_lc_ticks: u32,     // leg in band AND coh in band (any ep)
    no_ceiling_ticks: u32,  // ep >= 70 AND leg in band AND coh in band
    no_ceiling_streak: u32, // current
    no_ceiling_best: u32,   // longest consecutive -- 3 forms a vassalage
    // sustained pressure
    ep_streak: u32,
    // state at the tick ep has been >= 70 for exactly N ticks
    leg_at_n: HashMap<u32, f64>,
    coh_at_n: HashMap<u32, f64>,
    alive_ticks: u32,
    // 5. OVERLORD AVAILABILITY -- the ceiling on what any trigger redesign can
    // achieve. `check_vassalage` only binds an actor whose strongest neighbour (by
    // military_size / distance, ANY distance) satisfies vassal_mil < 0.8 * overlord_mil.
    // Task 10 showed the world demilitarizes; if that test passes for nobody, a fixed
    // trigger buys nothing and the redesign is futile before it is designed.
    // Sampled over the ticks legitimacy spends inside its own window -- the moment a
    // working trigger would actually be looking for an overlord.
    overlord_ticks: u32,    // actor-ticks in leg window WITH a qualifying overlord
    leg_window_ticks: u32,  // actor-ticks in leg window at all
    best_overlord_mil: f64,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("rome_375");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(400);
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(42);

    let scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");
    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    let mut traces: HashMap<String, Trace> = HashMap::new();

    for _t in 0..ticks {
        tick(&mut world, &scenario, &mut event_log, &mut rng);
        let now = world.tick;

        for (id, actor) in &world.actors {
            let ep = actor.get_metric("external_pressure");
            let leg = actor.get_metric("legitimacy");
            let coh = actor.get_metric("cohesion");
            let tr = traces.entry(id.clone()).or_default();
            tr.alive_ticks += 1;

            // --- 1/2. pressure arrival and the height it finds -----------------
            if ep >= EP_LO && tr.t_ep70.is_none() {
                tr.t_ep70 = Some(now);
                tr.leg_at_ep70 = Some(leg);
                tr.coh_at_ep70 = Some(coh);
            }
            if ep > EP_HI && tr.t_ep85.is_none() {
                tr.t_ep85 = Some(now);
            }

            // --- 3. natural slope, sampled at fixed offsets after arrival -------
            if let Some(t0) = tr.t_ep70 {
                if now == t0 + 20 {
                    tr.leg_at_p20 = Some(leg);
                    tr.coh_at_p20 = Some(coh);
                }
                if now == t0 + 40 {
                    tr.leg_at_p40 = Some(leg);
                    tr.coh_at_p40 = Some(coh);
                }
            }

            // legitimacy's own descent through its window
            if (LEG_LO..=LEG_HI).contains(&leg) && tr.t_leg_band.is_none() {
                tr.t_leg_band = Some(now);
                tr.coh_at_leg_band = Some(coh);
                tr.ep_at_leg_band = Some(ep);
            }
            if leg < LEG_LO && tr.t_leg_below.is_none() && tr.t_leg_band.is_some() {
                tr.t_leg_below = Some(now);
            }

            // --- 4. ceiling counterfactual --------------------------------------
            let g_leg = (LEG_LO..=LEG_HI).contains(&leg);
            let g_coh = (COH_LO..=COH_HI).contains(&coh);
            if g_leg && g_coh {
                tr.both_lc_ticks += 1;
            }
            if ep >= EP_LO && g_leg && g_coh {
                tr.no_ceiling_ticks += 1;
                tr.no_ceiling_streak += 1;
                tr.no_ceiling_best = tr.no_ceiling_best.max(tr.no_ceiling_streak);
            } else {
                tr.no_ceiling_streak = 0;
            }

            // --- 5. is there anyone to submit TO? --------------------------------
            // Mirrors check_vassalage step 3 verbatim: strongest neighbour by
            // military_size scaled by distance (no distance limit), then the
            // "clearly stronger overlord" test. Note both sides at 0.00 army fails
            // it (0 >= 0), so a fully demilitarized neighbourhood binds nobody.
            if (LEG_LO..=LEG_HI).contains(&leg) {
                tr.leg_window_ticks += 1;
                let mut best: Option<(String, f64)> = None;
                for n in &actor.neighbors {
                    if n.id == *id { continue; }
                    let Some(nb) = world.actors.get(&n.id) else { continue };
                    let pressure = nb.get_metric("military_size") / n.distance.max(1) as f64;
                    if best.as_ref().map(|(_, p)| pressure > *p).unwrap_or(true) {
                        best = Some((n.id.clone(), pressure));
                    }
                }
                if let Some((oid, _)) = best {
                    let omil = world.actors.get(&oid).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
                    let vmil = actor.get_metric("military_size");
                    if vmil < omil * 0.8 {
                        tr.overlord_ticks += 1;
                        tr.best_overlord_mil = tr.best_overlord_mil.max(omil);
                    }
                }
            }

            // --- sustained pressure: state at the N-th consecutive tick ----------
            if ep >= EP_LO {
                tr.ep_streak += 1;
                for n in N_CANDIDATES {
                    if tr.ep_streak == n {
                        tr.leg_at_n.insert(n, leg);
                        tr.coh_at_n.insert(n, coh);
                    }
                }
            } else {
                tr.ep_streak = 0;
            }
        }
    }

    let f = |o: Option<f64>| o.map(|v| format!("{v:.2}")).unwrap_or_else(|| "na".into());
    let u = |o: Option<u32>| o.map(|v| v.to_string()).unwrap_or_else(|| "na".into());

    let mut ids: Vec<&String> = traces.keys().collect();
    ids.sort();
    for id in ids {
        let tr = &traces[id];
        // transit: ticks ep spends inside [70, 85]
        let transit = match (tr.t_ep70, tr.t_ep85) {
            (Some(a), Some(b)) => (b - a).to_string(),
            (Some(_), None) => "never_exits".into(),
            _ => "na".into(),
        };
        // leg dwell inside its own window
        let leg_dwell = match (tr.t_leg_band, tr.t_leg_below) {
            (Some(a), Some(b)) => (b - a).to_string(),
            (Some(_), None) => "still_in".into(),
            _ => "na".into(),
        };
        let row: Vec<String> = vec![
            "ACTOR".into(),
            scenario_id.into(),
            seed.to_string(),
            id.to_string(),
            u(tr.t_ep70),
            u(tr.t_ep85),
            transit,
            f(tr.leg_at_ep70),
            f(tr.coh_at_ep70),
            f(tr.leg_at_p20),
            f(tr.leg_at_p40),
            f(tr.coh_at_p20),
            f(tr.coh_at_p40),
            u(tr.t_leg_band),
            u(tr.t_leg_below),
            leg_dwell,
            f(tr.coh_at_leg_band),
            f(tr.ep_at_leg_band),
            tr.both_lc_ticks.to_string(),
            tr.no_ceiling_ticks.to_string(),
            tr.no_ceiling_best.to_string(),
            f(tr.leg_at_n.get(&3).copied()),
            f(tr.leg_at_n.get(&5).copied()),
            f(tr.leg_at_n.get(&10).copied()),
            f(tr.leg_at_n.get(&20).copied()),
            f(tr.coh_at_n.get(&5).copied()),
            f(tr.coh_at_n.get(&10).copied()),
            f(tr.coh_at_n.get(&20).copied()),
            tr.leg_window_ticks.to_string(),
            tr.overlord_ticks.to_string(),
            format!("{:.1}", tr.best_overlord_mil),
        ];
        println!("{}", row.join("\t"));
    }

    let tot_no_ceiling: u32 = traces.values().map(|t| t.no_ceiling_ticks).sum();
    let tot_both_lc: u32 = traces.values().map(|t| t.both_lc_ticks).sum();
    let best_streak: u32 = traces.values().map(|t| t.no_ceiling_best).max().unwrap_or(0);
    let would_form = traces.values().filter(|t| t.no_ceiling_best >= 3).count();
    println!(
        "SUMMARY\t{scenario_id}\t{seed}\t{ticks}\t{tot_both_lc}\t{tot_no_ceiling}\t{best_streak}\t{would_form}\t{}",
        world.vassalages.len()
    );
}
