//! What the authored auto-deltas actually contribute to `external_pressure`, per actor.
//!
//! Written for stage 2 of queue item E, where the criterion asks whether a spawn given
//! edges receives pressure "comparable to the authored actors". That comparison cannot be
//! made from the content: no actor's pressure is one readable number. `byzantium` has four
//! blocks (base `2.125` in one, `0.0` in three) carrying eight conditions and two ratio
//! conditions between them; the three "simple" actors each pair their base with a negative
//! ratio condition keyed on the ottoman army — the very quantity the edit moves. Reading a
//! base out of `auto_deltas.toml` and calling it "the authored pressure" would compare the
//! wrong thing (docs/investigation_dead_authored_content.md §16.5).
//!
//! So it is measured, and measured from `engine::trace`: the engine emits the number it
//! used, at the point it used it. Nothing here recomputes a delta — the constraint that
//! produced three hand-written copies of the dependency arithmetic in `budget_probe`.
//!
//! Two quantities are reported separately, because they answer different questions:
//!   * `authored` — base plus every satisfied condition's own delta: what the content asked;
//!   * `applied`  — that plus this tick's noise: what the world felt.
//!
//! The criterion asks about intent, the ratified gates react to what was applied.
//!
//! Usage: pressure_probe <scenario> <ticks> <seed_from> <seed_count>

use engine13::{core::WorldState, engine::{tick, trace, EventLog}, scenarios::registry};
use rand::SeedableRng;
use std::collections::BTreeMap;

fn quartiles(v: &mut [f64]) -> (f64, f64, f64) {
    if v.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    (q(0.25), q(0.5), q(0.75))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let scenario_id = a.get(1).map(|s| s.as_str()).unwrap_or("constantinople_1430");
    let ticks: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed_from: u64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let seed_count: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(30);

    let scenario = registry::load_by_id(scenario_id).expect("unknown scenario");

    // Which blocks write `external_pressure`, and to whom. Taken from the metric key the
    // block carries, not from a list of names.
    let targets: BTreeMap<usize, String> = scenario
        .auto_deltas
        .iter()
        .enumerate()
        .filter_map(|(i, ad)| {
            let key = ad.metric.to_string();
            key.ends_with(".external_pressure").then(|| {
                let actor = key
                    .trim_start_matches("actor:")
                    .trim_end_matches(".external_pressure")
                    .to_string();
                (i, actor)
            })
        })
        .collect();

    println!("=== PRESSURE PROBE: {scenario_id} ({ticks} ticks, seeds {seed_from}..{})", seed_from + seed_count - 1);
    println!("blocks writing external_pressure: {}\n", targets.len());

    // Per actor, per seed: the summed contribution over the whole run.
    let mut authored_by_actor: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut applied_by_actor: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut fired_by_block: BTreeMap<usize, usize> = BTreeMap::new();
    let mut ticks_by_block: BTreeMap<usize, usize> = BTreeMap::new();

    for seed in seed_from..seed_from + seed_count {
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for actor in &scenario.actors {
            if !actor.is_successor_template {
                world.actors.insert(actor.id.clone(), actor.clone());
            }
        }
        if let Some(ref im) = scenario.initial_family_metrics {
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(im),
                patriarch_age: scenario
                    .generation_mechanics
                    .as_ref()
                    .map(|g| g.patriarch_start_age)
                    .unwrap_or(40) as u32,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut log = EventLog::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let mut authored: BTreeMap<String, f64> = BTreeMap::new();
        let mut applied: BTreeMap<String, f64> = BTreeMap::new();

        trace::enable();
        for _ in 0..ticks {
            tick(&mut world, &scenario, &mut log, &mut rng);
            for row in trace::take_auto_deltas() {
                let Some(actor) = targets.get(&row.index) else { continue };
                *authored.entry(actor.clone()).or_default() += row.authored;
                *applied.entry(actor.clone()).or_default() += row.applied;
                *ticks_by_block.entry(row.index).or_default() += 1;
                // "Fired" means a condition moved the block off its base — the question the
                // content asks, and not the same as "the block ran".
                if (row.authored - row.base).abs() > f64::EPSILON {
                    *fired_by_block.entry(row.index).or_default() += 1;
                }
            }
        }
        trace::disable();

        for (actor, v) in authored {
            authored_by_actor.entry(actor).or_default().push(v);
        }
        for (actor, v) in applied {
            applied_by_actor.entry(actor).or_default().push(v);
        }
    }

    println!(
        "--- summed contribution to external_pressure over {ticks} ticks, by seed (q25 / median / q75) ---"
    );
    println!("{:16} {:>30} {:>30}", "actor", "authored", "applied");
    for (actor, v) in authored_by_actor.iter() {
        let mut au = v.clone();
        let mut ap = applied_by_actor.get(actor).cloned().unwrap_or_default();
        let (a25, a50, a75) = quartiles(&mut au);
        let (p25, p50, p75) = quartiles(&mut ap);
        println!(
            "{actor:16} {:>9.1} {:>9.1} {:>9.1} {:>10.1} {:>9.1} {:>9.1}",
            a25, a50, a75, p25, p50, p75
        );
    }

    println!("\n--- per block: how often a condition moved it off its base ---");
    for (i, actor) in &targets {
        let n = fired_by_block.get(i).copied().unwrap_or(0);
        let t = ticks_by_block.get(i).copied().unwrap_or(0);
        let base = scenario.auto_deltas[*i].base;
        println!(
            "  block #{i:<3} -> {actor:14} base {base:>6.3} | moved off base {:>6.2}% of block-ticks",
            if t > 0 { 100.0 * n as f64 / t as f64 } else { 0.0 }
        );
    }
}
