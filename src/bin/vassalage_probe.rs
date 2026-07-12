//! Vassalage / conquest diagnostic probe for infrastructure task 10.
//!
//! Answers two questions task 2 could only answer for a pre-#22/#23 engine:
//!   1. Is the vassalage band (external_pressure 70-85 AND legitimacy 10-25 AND
//!      cohesion 15-30) reachable at all, now that the combat guard (#22) stopped
//!      phantom fights from grinding cohesion down?
//!   2. How many actors does `conquest_collapse` (#23) kill that would otherwise
//!      have lived forever -- and does it kill them *before* they could have
//!      reached the band?
//!
//! Band membership is NOT re-implemented here: `check_vassalage` inserts an actor
//! into `world.vassalage_warning_ticks` exactly when its own `in_vassalage_band`
//! returns true, so that map is the engine's own verdict, read back after each
//! tick. The per-gate breakdown below *is* computed from metrics, but only to
//! explain *why* the engine's verdict came out the way it did -- never to decide it.
//!
//! Deliberately depends on no symbol introduced by #22/#23 (hence the local
//! `NO_ARMY` rather than importing `interactions::MIN_DEFENSIBLE_MILITARY`, which
//! it equals on main). That keeps the binary compilable on the pre-guard trees, so
//! the same measurement can be run across all three engine generations and
//! compared like for like.
//!
//! Usage:
//! ```bash
//! cargo run --release --bin vassalage_probe -- <scenario> <ticks> <seed>
//! ```
//! Emits a `SUMMARY<TAB>...` line per run for aggregation across seeds.

use engine13::{core::WorldState, engine::{tick, EventLog}, scenarios::registry};
use rand::SeedableRng;
use std::collections::{HashMap, HashSet};

/// Same value as `interactions::MIN_DEFENSIBLE_MILITARY` on main; kept local so
/// this probe still builds on trees from before that constant existed.
const NO_ARMY: f64 = 0.01;

/// The three collapse predicates, verbatim from `engine::check_collapses`.
/// Re-stated here only to attribute a death to a path after the fact -- the engine
/// records *that* an actor died, never *why*.
#[derive(Default, Clone, Copy)]
struct Danger {
    classic: bool,
    internal: bool,
    conquest: bool,
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

    // --- accumulators ------------------------------------------------------
    // Band membership, per the engine's own `vassalage_warning_ticks`.
    let mut band_actor_ticks = 0u64;
    let mut band_actors: HashSet<String> = HashSet::new();
    let mut max_band_streak: HashMap<String, u32> = HashMap::new();

    // Per-gate diagnosis: which of the three gates is the one that fails?
    let mut gate_ep = 0u64; // ep in 70..=85
    let mut gate_leg = 0u64; // leg in 10..=25
    let mut gate_coh = 0u64; // coh in 15..=30
    let mut ep_and_leg = 0u64; // the pair task 2 called "measure-zero together"
    let mut coh_when_ep_and_leg: Vec<f64> = Vec::new(); // ...and what cohesion was doing then
    let mut two_of_three = 0u64;
    let mut live_actor_ticks = 0u64;

    // Timing: the first tick each gate opens for each actor. If `ep` opens early
    // (on its monotone climb) and `legitimacy` only opens much later, the two
    // windows cannot overlap in time -- which is *why* the band is measure-zero,
    // as opposed to merely observing that it is.
    let mut first_ep: HashMap<String, u32> = HashMap::new();
    let mut first_leg: HashMap<String, u32> = HashMap::new();
    let mut first_coh: HashMap<String, u32> = HashMap::new();

    // Last-seen danger flags, for attributing each death to a collapse path.
    let mut last_danger: HashMap<String, Danger> = HashMap::new();
    let mut deaths: Vec<(String, u32, Danger)> = Vec::new();
    let mut seen_dead: HashSet<String> = HashSet::new();

    for _t in 0..ticks {
        tick(&mut world, &scenario, &mut event_log, &mut rng);

        // Engine's verdict on band membership this tick.
        for (id, streak) in &world.vassalage_warning_ticks {
            band_actor_ticks += 1;
            band_actors.insert(id.clone());
            let e = max_band_streak.entry(id.clone()).or_insert(0);
            *e = (*e).max(*streak);
        }

        // Gate diagnosis + danger flags, over living actors.
        for (id, actor) in &world.actors {
            let ep = actor.get_metric("external_pressure");
            let leg = actor.get_metric("legitimacy");
            let coh = actor.get_metric("cohesion");
            let mil = actor.get_metric("military_size");

            live_actor_ticks += 1;
            let g_ep = (70.0..=85.0).contains(&ep);
            let g_leg = (10.0..=25.0).contains(&leg);
            let g_coh = (15.0..=30.0).contains(&coh);
            gate_ep += g_ep as u64;
            gate_leg += g_leg as u64;
            gate_coh += g_coh as u64;
            if g_ep && g_leg {
                ep_and_leg += 1;
                coh_when_ep_and_leg.push(coh);
            }
            if [g_ep, g_leg, g_coh].iter().filter(|b| **b).count() == 2 {
                two_of_three += 1;
            }
            if g_ep { first_ep.entry(id.clone()).or_insert(world.tick); }
            if g_leg { first_leg.entry(id.clone()).or_insert(world.tick); }
            if g_coh { first_coh.entry(id.clone()).or_insert(world.tick); }

            let besieged = actor.neighbors.iter().any(|n| {
                n.distance == 1
                    && world
                        .actors
                        .get(&n.id)
                        .map(|nb| nb.get_metric("military_size") >= NO_ARMY)
                        .unwrap_or(false)
            });
            last_danger.insert(
                id.clone(),
                Danger {
                    classic: leg < 10.0 && coh < 15.0 && ep > 85.0,
                    internal: leg < 5.0 && coh < 8.0,
                    conquest: mil < NO_ARMY && leg < 10.0 && ep > 85.0 && besieged,
                },
            );
        }

        // Newly dead actors: attribute to the path that was live when last seen.
        for d in &world.dead_actors {
            if seen_dead.insert(d.id.clone()) {
                let danger = last_danger.get(&d.id).copied().unwrap_or_default();
                deaths.push((d.id.clone(), d.tick_death, danger));
            }
        }
    }

    // Actors the conquest path deliberately spares: no army, no authority,
    // saturated pressure -- but no armed neighbour to finish them. Pre-#23 these
    // were immortal; post-#23 they still are, by design. Counting them measures
    // the residue the `besieged` clause leaves behind.
    let mut immortal_residue: Vec<String> = Vec::new();
    for (id, actor) in &world.actors {
        let ep = actor.get_metric("external_pressure");
        let leg = actor.get_metric("legitimacy");
        let mil = actor.get_metric("military_size");
        let besieged = actor.neighbors.iter().any(|n| {
            n.distance == 1
                && world
                    .actors
                    .get(&n.id)
                    .map(|nb| nb.get_metric("military_size") >= NO_ARMY)
                    .unwrap_or(false)
        });
        if mil < NO_ARMY && leg < 10.0 && ep > 85.0 && !besieged {
            immortal_residue.push(id.clone());
        }
    }
    immortal_residue.sort();

    let d_classic = deaths.iter().filter(|(_, _, d)| d.classic).count();
    let d_internal = deaths.iter().filter(|(_, _, d)| d.internal && !d.classic).count();
    // Deaths that exist *only* because of #23: no other path was live.
    let d_conquest_only = deaths
        .iter()
        .filter(|(_, _, d)| d.conquest && !d.classic && !d.internal)
        .count();
    let d_unattributed = deaths
        .iter()
        .filter(|(_, _, d)| !d.conquest && !d.classic && !d.internal)
        .count();

    let coh_min = coh_when_ep_and_leg.iter().cloned().fold(f64::INFINITY, f64::min);
    let coh_max = coh_when_ep_and_leg.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // --- human-readable ----------------------------------------------------
    println!("=== VASSALAGE/CONQUEST PROBE: {scenario_id} (seed {seed}, {ticks} ticks) ===");
    println!("live actor-ticks observed: {live_actor_ticks}");
    println!();
    println!("--- Q1: is the vassalage band reachable? (engine's own vassalage_warning_ticks) ---");
    println!("  actor-ticks in band:      {band_actor_ticks}");
    println!("  distinct actors in band:  {} {:?}", band_actors.len(), {
        let mut v: Vec<_> = band_actors.iter().cloned().collect();
        v.sort();
        v
    });
    println!("  longest band streak:      {} (formation needs 3 consecutive)",
        max_band_streak.values().max().copied().unwrap_or(0));
    println!("  vassalages formed:        {}", world.vassalages.len());
    println!();
    println!("--- Which gate blocks the band? (actor-ticks passing each, of {live_actor_ticks}) ---");
    println!("  external_pressure 70-85:  {gate_ep}");
    println!("  legitimacy       10-25:   {gate_leg}");
    println!("  cohesion         15-30:   {gate_coh}");
    println!("  ep AND leg together:      {ep_and_leg}   <- task 2 called this the hard part");
    if !coh_when_ep_and_leg.is_empty() {
        println!(
            "    cohesion during those:  min={coh_min:.2} max={coh_max:.2}  (needs 15-30 to close the band)"
        );
    }
    println!("  exactly 2 of 3 gates:     {two_of_three}");
    println!();
    println!("--- WHY: when does each gate first open, per actor? (median first tick) ---");
    let median = |m: &HashMap<String, u32>| -> String {
        if m.is_empty() { return "never".into(); }
        let mut v: Vec<u32> = m.values().copied().collect();
        v.sort_unstable();
        format!("{} (n={})", v[v.len() / 2], v.len())
    };
    println!("  external_pressure 70-85 opens at: {}", median(&first_ep));
    println!("  legitimacy       10-25 opens at: {}", median(&first_leg));
    println!("  cohesion         15-30 opens at: {}", median(&first_coh));
    println!();
    println!("--- Q2: mortality, by collapse path ---");
    println!("  deaths total:             {}", deaths.len());
    println!("  ... classic  (leg/coh/ep):     {d_classic}");
    println!("  ... internal (leg/coh):        {d_internal}");
    println!("  ... conquest ONLY (#23 added): {d_conquest_only}");
    println!("  ... unattributed:              {d_unattributed}");
    for (id, t, d) in &deaths {
        let path = if d.classic { "classic" } else if d.internal { "internal" }
            else if d.conquest { "CONQUEST" } else { "?" };
        println!("      tick {t:3}  {id:24} {path}");
    }
    println!();
    println!("  immortal residue (no army/authority, pressured, but NOT besieged -> spared):");
    println!("      {} {:?}", immortal_residue.len(), immortal_residue);
    println!();

    // --- machine-readable --------------------------------------------------
    println!(
        "SUMMARY\t{scenario_id}\t{seed}\t{ticks}\t{band_actor_ticks}\t{}\t{}\t{}\t{}\t{d_classic}\t{d_internal}\t{d_conquest_only}\t{}\t{ep_and_leg}\t{}\t{gate_ep}\t{gate_leg}\t{gate_coh}",
        band_actors.len(),
        max_band_streak.values().max().copied().unwrap_or(0),
        world.vassalages.len(),
        deaths.len(),
        immortal_residue.len(),
        if coh_when_ep_and_leg.is_empty() { "n/a".to_string() } else { format!("{coh_min:.1}-{coh_max:.1}") },
    );
}
