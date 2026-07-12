//! Task 12, stage 1 — read-only audit for the `submission` design.
//!
//! Answers, per actor, WITHOUT touching the engine (runs `tick()` and reads metrics;
//! consumes no RNG of its own):
//!
//!   Q1  Who actually stands under SUSTAINED pressure (`ep >= 70` held N ticks)?
//!       Task 11 fell over because it never asked this and eroded the patrons.
//!   Q2  Does `legitimacy` of those actors reach the band window 10-25 — and WHEN,
//!       relative to the scenario's victory tick? This decides whether `legitimacy`
//!       stays a band member (it is what keeps healthy patrons out of the band).
//!   Q3  How long is pressure actually sustained, and does it ever lift? (Derives the
//!       submission accumulation rate and tells us whether decay is even exercised.)
//!   Q4  Would a qualifying overlord exist (vassal_mil < 0.8 * overlord_mil, any
//!       distance) at the moment the proposed band opens?
//!   Q5  Tribute exposure: economic_output of the candidates (tribute = 3-5% of it,
//!       per tick, off the vassal's treasury) and their treasury — because patron
//!       action availability is gated on treasury (`actions.rs:213`).
//!
//! Usage: submission_probe <scenario> [ticks] [seed]

use engine13::{
    core::WorldState,
    engine::{tick, EventLog},
    scenarios::registry,
};
use rand::SeedableRng;
use std::collections::HashMap;

const EP_THRESHOLD: f64 = 70.0;
const N_SUSTAIN: u32 = 10; // task 11's confirmed trigger
const LEG_LO: f64 = 10.0;
const LEG_HI: f64 = 25.0;

#[derive(Default, Clone)]
struct Trace {
    alive_ticks: u32,
    // Q1: sustained pressure
    pressure_ticks: u32,      // current consecutive run of ep >= 70
    sustained_ticks: u32,     // actor-ticks with pressure_ticks >= N
    longest_run: u32,
    t_sustained: Option<u32>, // first tick pressure_ticks reached N
    drops: u32,               // times ep fell back below 70 after a sustained run
    // Q2: legitimacy window
    t_leg_window: Option<u32>,
    leg_window_ticks: u32,
    // proposed band = sustained pressure AND leg in 10..=25
    t_band: Option<u32>,
    band_ticks: u32,
    band_with_overlord: u32,
    // Q5
    econ_at_band: Option<f64>,
    treas_at_band: Option<f64>,
    leg_min: f64,
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
    if let Some(ref m) = scenario.initial_family_metrics {
        let age = scenario
            .generation_mechanics
            .as_ref()
            .map(|g| g.patriarch_start_age)
            .unwrap_or(40) as u32;
        world.family_state = Some(engine13::core::FamilyState {
            metrics: m.clone(),
            patriarch_age: age,
            generation_count: 0,
        });
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    // Successor actors appear mid-run (spawn/on_collapse), so entries are created lazily.
    let mut traces: HashMap<String, Trace> = HashMap::new();

    for _ in 0..ticks {
        tick(&mut world, &scenario, &mut event_log, &mut rng);
        let now = world.tick;

        // Snapshot military for overlord attribution (same shape as check_vassalage
        // step 3: strongest neighbour by military scaled by distance, any distance).
        let mil: HashMap<String, f64> = world
            .actors
            .iter()
            .map(|(id, a)| (id.clone(), a.get_metric("military_size")))
            .collect();

        let mut ids: Vec<String> = world.actors.keys().cloned().collect();
        ids.sort();
        for id in &ids {
            let actor = &world.actors[id];
            let ep = actor.get_metric("external_pressure");
            let leg = actor.get_metric("legitimacy");
            let tr = traces.entry(id.clone()).or_insert_with(|| Trace {
                leg_min: f64::MAX,
                ..Default::default()
            });
            tr.alive_ticks += 1;
            if leg < tr.leg_min {
                tr.leg_min = leg;
            }

            // Q1/Q3: sustained-pressure counter, exactly as the design would keep it.
            if ep >= EP_THRESHOLD {
                tr.pressure_ticks += 1;
                tr.longest_run = tr.longest_run.max(tr.pressure_ticks);
                if tr.pressure_ticks >= N_SUSTAIN {
                    tr.sustained_ticks += 1;
                    if tr.t_sustained.is_none() {
                        tr.t_sustained = Some(now);
                    }
                }
            } else {
                if tr.pressure_ticks >= N_SUSTAIN {
                    tr.drops += 1;
                }
                tr.pressure_ticks = 0;
            }

            // Q2: legitimacy window
            let leg_in = (LEG_LO..=LEG_HI).contains(&leg);
            if leg_in {
                tr.leg_window_ticks += 1;
                if tr.t_leg_window.is_none() {
                    tr.t_leg_window = Some(now);
                }
            }

            // Proposed band: sustained pressure AND legitimacy window
            let sustained = tr.pressure_ticks >= N_SUSTAIN;
            if sustained && leg_in {
                tr.band_ticks += 1;
                if tr.t_band.is_none() {
                    tr.t_band = Some(now);
                    tr.econ_at_band = Some(actor.get_metric("economic_output"));
                    tr.treas_at_band = Some(actor.get_metric("treasury"));
                }
                // Q4: does a qualifying overlord exist?
                let my_mil = mil.get(id).copied().unwrap_or(0.0);
                let mut best: Option<(String, f64)> = None;
                let mut nb: Vec<(String, u32)> =
                    actor.neighbors.iter().map(|n| (n.id.clone(), n.distance)).collect();
                nb.sort();
                for (nid, dist) in &nb {
                    if !mil.contains_key(nid) {
                        continue;
                    }
                    let p = mil[nid] / (*dist).max(1) as f64;
                    if best.as_ref().map(|(_, bp)| p > *bp).unwrap_or(true) {
                        best = Some((nid.clone(), p));
                    }
                }
                if let Some((oid, _)) = best {
                    if my_mil < 0.8 * mil.get(&oid).copied().unwrap_or(0.0) {
                        tr.band_with_overlord += 1;
                    }
                }
            }
        }
    }

    println!("=== SUBMISSION PROBE: {scenario_id} (seed {seed}, {ticks} ticks, no-player) ===");
    println!(
        "{:<22} {:>6} {:>7} {:>6} {:>6} {:>7} {:>6} {:>6} {:>6} {:>7} {:>8}",
        "actor", "alive", "sustTk", "tSust", "drops", "legWin", "tLeg", "BAND", "tBand", "w/lord", "econ@band"
    );
    let mut ids: Vec<&String> = traces.keys().collect();
    ids.sort();
    for id in ids {
        let t = &traces[id];
        let f = |o: Option<u32>| o.map(|v| v.to_string()).unwrap_or("-".into());
        println!(
            "{:<22} {:>6} {:>7} {:>6} {:>6} {:>7} {:>6} {:>6} {:>6} {:>7} {:>8}",
            id,
            t.alive_ticks,
            t.sustained_ticks,
            f(t.t_sustained),
            t.drops,
            t.leg_window_ticks,
            f(t.t_leg_window),
            t.band_ticks,
            f(t.t_band),
            t.band_with_overlord,
            t.econ_at_band.map(|v| format!("{v:.1}")).unwrap_or("-".into()),
        );
    }
}
