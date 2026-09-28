//! Absorption edges probe — stage 1 of B24 (docs/TRIAGE.md).
//!
//! When a power dies with one heir that is already alive (absorption: `savoy → milan`,
//! `byzantium → ottomans`), `check_collapses` credits the heir and touches no edges: the
//! absorber does not get the dead power's neighbours, and the neighbours keep a dangling
//! reference to the dead id. A fresh sole heir (D₄) gets both — its parent's list and
//! the parent's place in every living neighbour's list.
//!
//! This probe EMULATES the D₄ rule for absorptions: right after the tick an absorption
//! happens, the absorber gains the dead power's edges it lacks, and every living actor
//! replaces the dead id with the absorber (or drops it, if it already lists the absorber).
//! It reports how often absorption happens, how many dangling references the living
//! carry, and what the rule changes in the world.
//!
//! Usage: cargo run --release --bin absorb_edges_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;
use std::collections::HashSet;

#[derive(Default)]
struct Tally {
    absorptions: u64,
    edges_gained: u64,
    dangling_actor_ticks: u64,
    wars: u64,
    deaths: u64,
    wins: u32,
}

fn run(sc: &str, strat: Option<&str>, seed: u64, ticks: u32, rewire: bool) -> Tally {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    let strategy = strat.map(|s| ScriptedStrategy::from_str(s, sc));
    let mut t = Tally::default();
    for _ in 0..ticks {
        let alive_before: HashSet<String> = st.world_state.as_ref().unwrap().actors.keys().cloned().collect();
        let dead_before = st.world_state.as_ref().unwrap().dead_actors.len();
        match &strategy {
            Some(s) => {
                play_scripted_tick(&mut st, s);
            }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let ws = st.world_state.as_mut().unwrap();
        let newly_dead: Vec<(String, Vec<String>)> = ws.dead_actors[dead_before..]
            .iter()
            .map(|d| (d.id.clone(), d.successor_ids.iter().map(|s| s.id.clone()).collect()))
            .collect();
        for (dead, heirs) in newly_dead {
            if heirs.len() != 1 || heirs[0] == dead || !alive_before.contains(&heirs[0]) || !ws.actors.contains_key(&heirs[0]) {
                continue;
            }
            let heir = heirs[0].clone();
            t.absorptions += 1;
            if !rewire {
                continue;
            }
            // Edges the dead power had toward the living, from the living side.
            let mut inherited: Vec<engine13::core::Neighbor> = Vec::new();
            let mut ids: Vec<String> = ws.actors.keys().cloned().collect();
            ids.sort();
            for id in &ids {
                if *id == heir {
                    continue;
                }
                let other = ws.actors.get_mut(id).unwrap();
                if let Some(n) = other.neighbors.iter().find(|n| n.id == dead).cloned() {
                    if other.neighbors.iter().any(|n| n.id == heir) {
                        other.neighbors.retain(|n| n.id != dead);
                    } else {
                        for n in other.neighbors.iter_mut() {
                            if n.id == dead {
                                n.id = heir.clone();
                            }
                        }
                        inherited.push(engine13::core::Neighbor { id: id.clone(), distance: n.distance, border_type: n.border_type });
                    }
                }
            }
            let h = ws.actors.get_mut(&heir).unwrap();
            h.neighbors.retain(|n| n.id != dead);
            for n in inherited {
                if !h.neighbors.iter().any(|m| m.id == n.id) {
                    h.neighbors.push(n);
                    t.edges_gained += 1;
                }
            }
        }
        let dead_ids = &ws.dead_actor_ids;
        t.dangling_actor_ticks += ws.actors.values().filter(|a| a.neighbors.iter().any(|n| dead_ids.contains(&n.id))).count() as u64;
    }
    let ws = st.world_state.as_ref().unwrap();
    t.wars = st.event_log.events.iter().filter(|e| e.id.starts_with("military_conflict_")).count() as u64;
    t.deaths = ws.dead_actors.len() as u64;
    t.wins = ws.victory_achieved as u32;
    t
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let worlds: &[(&str, Option<&str>)] = &[
        ("rome_375", None), ("rome_375", Some("balanced")),
        ("constantinople_1430", None), ("constantinople_1430", Some("balanced")),
        ("constantinople_1430", Some("diplomacy")), ("constantinople_1430", Some("military")),
        ("milan_1477", None), ("milan_1477", Some("aggressive")),
    ];
    println!("{:<20} {:<10} {:<7} {:>11} {:>12} {:>15} {:>10} {:>9} {:>8}",
        "scenario", "world", "rule", "absorb/game", "edges/absorb", "dangling a-t/g", "wars/game", "deaths/g", "victory");
    for (sc, strat) in worlds {
        for rewire in [false, true] {
            let mut s = Tally::default();
            for seed in 0..seeds {
                let t = run(sc, *strat, seed, ticks, rewire);
                s.absorptions += t.absorptions; s.edges_gained += t.edges_gained;
                s.dangling_actor_ticks += t.dangling_actor_ticks; s.wars += t.wars; s.deaths += t.deaths; s.wins += t.wins;
            }
            let n = seeds as f64;
            println!("{:<20} {:<10} {:<7} {:>11.2} {:>12.2} {:>15.1} {:>10.1} {:>9.2} {:>5}/{}",
                sc, strat.unwrap_or("none"), if rewire { "rewire" } else { "engine" },
                s.absorptions as f64 / n, s.edges_gained as f64 / (s.absorptions.max(1) as f64),
                s.dangling_actor_ticks as f64 / n, s.wars as f64 / n, s.deaths as f64 / n, s.wins, seeds);
        }
    }
}
