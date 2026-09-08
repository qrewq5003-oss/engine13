//! Successor-entry probe — stage 1 of the successor-subsystem task.
//!
//! Two open items live in the same twenty lines of `engine::check_collapses`:
//!
//!   * constantinople_1430 declares five `ottoman_*` heirs in `on_collapse` and has a
//!     template for none of them — the engine skips them silently
//!     (`docs/narrative_state_2026_08.md` §15.8 п.4);
//!   * `split_metrics_for_successor` is called with the *heir's own template*
//!     (`&scenario_actor.metrics`), not the dead parent's metrics, so the
//!     architecture's split formula (`ENGINE13_ARCHITECTURE.md`, «Формула раскола»:
//!     every line is `родитель × …`) multiplies authored starting values instead
//!     (`docs/investigation_pressure_ratchet_rome.md` §12 `(D₁)`).
//!
//! This probe measures both on the current tree, model-free: it drives `tick()`
//! and reads the world, nothing else. For every death it records the parent's
//! final metrics and each declared heir's outcome (born / phantom / absorbed by a
//! living power); for every birth it records what the heir was actually born
//! with, next to the raw template and next to what the architecture formula
//! would have produced from the parent's final metrics. `FATE` rows follow each
//! born heir to its own death, if any.
//!
//! Read-only: no engine symbol is modified and no RNG is drawn outside the
//! engine, so the simulation observed is the one the baselines are built from.
//!
//! Usage:
//! ```bash
//! cargo run --release --bin successor_probe -- <scenario> <ticks> <seed>
//! ```

use engine13::{core::WorldState, engine::{tick, EventLog}, scenarios::registry};
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap, HashSet};

const KEYS: [&str; 8] = [
    "external_pressure", "cohesion", "legitimacy", "population",
    "military_size", "military_quality", "economic_output", "treasury",
];

fn fmt(m: &HashMap<String, f64>) -> String {
    KEYS.iter()
        .map(|k| format!("{}={:.1}", k, m.get(*k).copied().unwrap_or(0.0)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Verbatim reproduction of `engine::split_metrics_for_successor` (private), so
/// the probe can state what the formula yields on a *given* input. Applied to the
/// template it must reproduce the engine's own birth values exactly — that is the
/// probe's self-check (`BIRTH` rows print `formula_template` next to `born`).
fn split(parent: &HashMap<String, f64>, weight: f64) -> HashMap<String, f64> {
    let mut m = parent.clone();
    let g = |m: &HashMap<String, f64>, k: &str| m.get(k).copied().unwrap_or(0.0);
    m.insert("military_size".into(), g(&m, "military_size") * weight * 0.7);
    m.insert("military_quality".into(), g(&m, "military_quality") * 0.8);
    m.insert("economic_output".into(), g(&m, "economic_output") * 0.7);
    m.insert("population".into(), g(&m, "population") * weight);
    m.insert("treasury".into(), g(&m, "treasury") * weight * 0.5);
    m.insert("external_pressure".into(), (g(&m, "external_pressure") * 1.3).min(100.0));
    m.insert("cohesion".into(), 20.0);
    m.insert("legitimacy".into(), 30.0);
    engine13::core::actor::ensure_default_metrics(&mut m);
    m
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("rome_375");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(42);

    let scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");
    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario.generation_mechanics.as_ref()
            .map(|g| g.patriarch_start_age).unwrap_or(40) as u32;
        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
        });
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let templates: HashMap<&str, &engine13::core::Actor> =
        scenario.actors.iter().map(|a| (a.id.as_str(), a)).collect();

    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);

    let mut alive: HashSet<String> = world.actors.keys().cloned().collect();
    let mut seen_deaths = 0usize;
    let mut born_tick: BTreeMap<String, u32> = BTreeMap::new();
    let mut n_births = 0usize;
    let mut n_phantom = 0usize;
    let mut n_absorbed = 0usize;
    let mut n_dead = 0usize;

    for t in 0..ticks {
        tick(&mut world, &scenario, &mut event_log, &mut rng);
        let now: HashSet<String> = world.actors.keys().cloned().collect();

        // Deaths processed this tick, in the order the engine recorded them.
        let new_dead: Vec<engine13::core::DeadActor> =
            world.dead_actors[seen_deaths..].to_vec();
        seen_deaths = world.dead_actors.len();

        let mut born_this_tick: Vec<String> = now.difference(&alive).cloned().collect();
        born_this_tick.sort();

        for d in &new_dead {
            let declared: Vec<String> = d.successor_ids.iter().map(|s| s.id.clone()).collect();
            let mut created = vec![];
            let mut phantom = vec![];
            let mut absorbed = vec![];
            let mut dead = vec![];
            for s in &d.successor_ids {
                if born_this_tick.contains(&s.id) {
                    created.push(s.id.clone());
                } else if world.actors.contains_key(&s.id) {
                    absorbed.push(s.id.clone());
                } else if world.dead_actor_ids.contains(&s.id) {
                    // Heir died before its parent: skipped by the dead-guard (stage 2);
                    // before it, this was the resurrection path.
                    dead.push(s.id.clone());
                } else {
                    phantom.push(s.id.clone());
                }
            }
            n_phantom += phantom.len();
            n_absorbed += absorbed.len();
            n_dead += dead.len();
            println!(
                "DEATH\t{}\t{}\t{}\t{}\t{}\tdeclared=[{}]\tcreated=[{}]\tphantom=[{}]\tabsorbed=[{}]\tdead=[{}]\ttemplate_missing=[{}]",
                scenario_id, seed, t, d.id, fmt(&d.final_metrics),
                declared.join(","), created.join(","), phantom.join(","), absorbed.join(","), dead.join(","),
                declared.iter().filter(|id| !templates.contains_key(id.as_str()))
                    .cloned().collect::<Vec<_>>().join(","),
            );
            for s in &d.successor_ids {
                if let Some(a) = world.actors.get(&s.id) {
                    if !born_this_tick.contains(&s.id) { continue; }
                    let tpl = templates.get(s.id.as_str()).expect("born heir has a template");
                    n_births += 1;
                    born_tick.insert(s.id.clone(), t);
                    println!(
                        "BIRTH\t{}\t{}\t{}\t{}\tparent={}\tweight={}\tn_succ={}\tneighbors={}\tborn: {}\ttemplate: {}\tformula_template: {}\tformula_parent: {}",
                        scenario_id, seed, t, s.id, d.id, s.weight, d.successor_ids.len(),
                        a.neighbors.len(),
                        fmt(&a.metrics), fmt(&tpl.metrics),
                        fmt(&split(&tpl.metrics, s.weight)),
                        fmt(&split(&d.final_metrics, s.weight)),
                    );
                }
            }
        }
        // Births not tied to a death this tick are milestone spawns. Printed with the
        // spawn's own edge count and how many of the listed neighbours name it back
        // (the spawn-edge task's criterion).
        for id in &born_this_tick {
            if born_tick.contains_key(id) { continue; }
            if let Some(a) = world.actors.get(id) {
                let back = a.neighbors.iter().filter(|n| {
                    world.actors.get(&n.id).map(|o| o.neighbors.iter().any(|m| &m.id == id)).unwrap_or(false)
                }).count();
                println!("SPAWN\t{}\t{}\t{}\t{}\tneighbors={}\tlisted_back_by={}", scenario_id, seed, t, id, a.neighbors.len(), back);
            }
        }
        alive = now;
    }

    for (id, bt) in &born_tick {
        // State of a still-living heir at the end of the run: the only way to see
        // whether it ever fought or was pressed once it stopped dying (stage D₄).
        if let Some(a) = world.actors.get(id) {
            println!("ALIVE\t{}\t{}\t{}\tborn={}\tneighbors={}\t{}", scenario_id, seed, id, bt, a.neighbors.len(), fmt(&a.metrics));
        }
        let death = world.dead_actors.iter().find(|d| &d.id == id).map(|d| d.tick_death);
        println!(
            "FATE\t{}\t{}\t{}\tborn={}\tdeath={}\tlifetime={}",
            scenario_id, seed, id, bt,
            death.map(|d| d.to_string()).unwrap_or_else(|| "-".into()),
            death.map(|d| (d - bt).to_string()).unwrap_or_else(|| format!("{}+", ticks - bt)),
        );
    }
    println!(
        "SUMMARY\t{}\t{}\tdeaths={}\tbirths={}\tphantom={}\tabsorbed={}\tdead={}\talive_end={}",
        scenario_id, seed, world.dead_actors.len(), n_births, n_phantom, n_absorbed, n_dead, world.actors.len()
    );
}
