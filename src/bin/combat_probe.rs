//! Combat diagnostic probe for infrastructure task 5 (combat imbalance).
//!
//! Read-only: touches no engine code. It reconstructs every military conflict
//! from the event log (`calculate_military_interaction` always records one, and
//! encodes the attacker/defender roles in the event id) and recomputes the
//! `effective_military` inputs with the engine's own public functions, so what
//! it reports is what the engine actually did, not a re-implementation of it.
//!
//! Usage:
//! ```bash
//! cargo run --bin combat_probe constantinople_1430 300 42
//! ```

use engine13::{
    core::WorldState,
    engine::{interactions::{affinity, effective_military}, tick, EventLog},
    scenarios::registry,
};
use rand::SeedableRng;
use std::collections::HashMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("constantinople_1430");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
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

    println!("=== COMBAT PROBE: {} (seed {}, {} ticks) ===\n", scenario_id, seed, ticks);

    // ---- Part 1: static tick-0 picture of who can fight whom -----------------
    // Combat requires distance == 1; everything else never reaches the dice roll.
    println!("--- Distance-1 pairs (the only pairs combat can occur between) ---");
    let mut ids: Vec<String> = world.actors.keys().cloned().collect();
    ids.sort();
    let mut seen = std::collections::HashSet::new();
    let mut combat_pairs: Vec<(String, String)> = Vec::new();
    for id in &ids {
        let actor = &world.actors[id];
        for n in &actor.neighbors {
            if !world.actors.contains_key(&n.id) {
                continue;
            }
            let key = if *id < n.id { (id.clone(), n.id.clone()) } else { (n.id.clone(), id.clone()) };
            if !seen.insert(key.clone()) {
                continue;
            }
            if n.distance == 1 {
                combat_pairs.push(key);
            }
        }
    }
    if combat_pairs.is_empty() {
        println!("  (none — this scenario has no military combat at all)");
    }
    for (a, b) in &combat_pairs {
        let (ea, da, na, aa) = eff_breakdown(&world, a);
        let (eb, db, nb, ab) = eff_breakdown(&world, b);
        let attacker = if ea >= eb { a } else { b };
        println!(
            "  {a} vs {b}\n    {a:12} mil={:6.1}  neighbors={na}  avg_affinity={aa:.2}  divisor={da:.2}  eff_mil={ea:7.2}\n    {b:12} mil={:6.1}  neighbors={nb}  avg_affinity={ab:.2}  divisor={db:.2}  eff_mil={eb:7.2}\n    -> attacker (higher eff_mil): {attacker}",
            world.actors[a].get_metric("military_size"),
            world.actors[b].get_metric("military_size"),
        );
    }
    println!();

    // ---- Part 2: run the sim, reconstruct every conflict ---------------------
    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);

    // (attacker, defender) -> (count, attacker_mil_lost, defender_mil_lost)
    let mut per_pair: HashMap<(String, String), (u32, f64, f64)> = HashMap::new();
    let mut conflicts: Vec<(u32, String, String, f64, f64, f64, f64)> = Vec::new();
    let mut seen_events = 0usize;

    for t in 0..ticks {
        let before: HashMap<String, f64> = world
            .actors
            .iter()
            .map(|(id, a)| (id.clone(), a.get_metric("military_size")))
            .collect();

        tick(&mut world, &scenario, &mut event_log, &mut rng);

        let new_events: Vec<_> = event_log.events[seen_events..].to_vec();
        seen_events = event_log.events.len();

        for e in &new_events {
            let Some(rest) = e.id.strip_prefix("military_conflict_") else { continue };
            // event.actor_id is the attacker; the remainder of the id is the defender
            let attacker = e.actor_id.clone();
            let Some(defender) = rest.strip_prefix(&format!("{}_", attacker)) else { continue };
            let defender = defender.to_string();

            let a_before = before.get(&attacker).copied().unwrap_or(0.0);
            let d_before = before.get(&defender).copied().unwrap_or(0.0);
            let a_after = world.actors.get(&attacker).map(|a| a.get_metric("military_size")).unwrap_or(0.0);
            let d_after = world.actors.get(&defender).map(|a| a.get_metric("military_size")).unwrap_or(0.0);

            let entry = per_pair.entry((attacker.clone(), defender.clone())).or_insert((0, 0.0, 0.0));
            entry.0 += 1;
            entry.1 += a_before - a_after;
            entry.2 += d_before - d_after;

            conflicts.push((t, attacker, defender, a_before, a_after, d_before, d_after));
        }
    }

    println!("--- Every military conflict, by (attacker -> defender) ---");
    let mut pairs: Vec<_> = per_pair.iter().collect();
    pairs.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    for ((atk, def), (count, atk_lost, def_lost)) in &pairs {
        println!(
            "  {atk:12} -> {def:12}  fights={count:3}  attacker_mil_lost={atk_lost:8.2}  defender_mil_lost={def_lost:8.2}"
        );
    }
    if pairs.is_empty() {
        println!("  (no military conflicts occurred)");
    }
    println!();

    // The core measurement: fights where the defender had no army left to fight
    // with. The attacker still pays 5-15% of its own military for each of these.
    // Same constant the engine's termination guard uses, so this count stays the
    // measurement of what that guard removes rather than drifting from it.
    const EMPTY: f64 = engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY;
    let zombie: Vec<_> = conflicts.iter().filter(|(_, _, _, _, _, db, _)| *db < EMPTY).collect();
    let zombie_cost: f64 = zombie.iter().map(|(_, _, _, ab, aa, _, _)| ab - aa).sum();
    let first_zombie = zombie.first().map(|(t, ..)| *t);
    println!("--- Fights against an already-empty army (defender military_size < {EMPTY}) ---");
    println!("  such fights:            {} of {} total", zombie.len(), conflicts.len());
    println!(
        "  first one at tick:      {}",
        first_zombie.map(|t| t.to_string()).unwrap_or_else(|| "n/a".into())
    );
    println!("  military the attackers destroyed of their OWN army in them: {zombie_cost:.2}");
    println!();

    let full = args.get(4).map(|s| s == "full").unwrap_or(false);
    println!("--- Conflict timeline{} ---", if full { "" } else { " (first 12 and last 4; pass 'full' as arg 4 for all)" });
    let show: Vec<_> = conflicts
        .iter()
        .enumerate()
        .filter(|(i, _)| full || *i < 12 || *i >= conflicts.len().saturating_sub(4))
        .collect();
    for (i, (t, atk, def, ab, aa, db, da)) in show {
        println!(
            "  #{i:<3} tick {t:3}  {atk} -> {def}   attacker mil {ab:7.2} -> {aa:7.2} ({:+.1}%)   defender mil {db:7.2} -> {da:7.2} ({:+.1}%)",
            if *ab > 0.0 { (aa - ab) / ab * 100.0 } else { 0.0 },
            if *db > 0.0 { (da - db) / db * 100.0 } else { 0.0 },
        );
    }
    println!();

    println!("--- Final military_size ---");
    for id in &ids {
        if let Some(a) = world.actors.get(id) {
            println!("  {id:12} {:8.2}", a.get_metric("military_size"));
        } else {
            println!("  {id:12}  (dead)");
        }
    }
}

/// Recompute what the engine's `effective_military` sees for one actor:
/// (eff_mil, divisor, neighbor_count, avg_affinity).
fn eff_breakdown(world: &WorldState, id: &str) -> (f64, f64, usize, f64) {
    let actor = &world.actors[id];
    let neighbors: Vec<&engine13::core::Actor> = actor
        .neighbors
        .iter()
        .filter_map(|n| world.actors.get(&n.id))
        .collect();
    let n = neighbors.len().max(1);
    let avg_aff: f64 = neighbors.iter().map(|x| affinity(actor, x)).sum::<f64>() / n as f64;
    let divisor = (n as f64 * avg_aff).max(1.0);
    (effective_military(actor, neighbors), divisor, n, avg_aff)
}
