//! Spawn identity probe — stage 1 of B28 (docs/TRIAGE.md).
//!
//! The spawn path (`engine::phase_events`, `spawn_actor`) builds every spawned actor with
//! `RegionRank::C`, `Religion::Orthodox` and `Culture::Slavic`, whatever the content
//! says: France in milan enters the world as an Orthodox Slav, the Mamluks too.
//!
//! What reads these fields:
//! * rank — only `phase_region_ranks`; rank `C` has a zero bonus in all three scenarios
//!   (A21), so the pinned rank changes nothing today;
//! * religion / culture — `interactions::friction` (named `affinity` before B41), read by `effective_military` (army
//!   stretched by foreign neighbours) and by the combat roll (a strong attacker is less
//!   likely to strike an affine defender); culture also by cultural displacement, which
//!   never fires (A30).
//!
//! This probe EMULATES authored identities: right after the tick a spawned actor appears
//! (spawns happen in the events phase, after interactions), its religion and culture are
//! set to a historical candidate, before its first interaction. The candidates are this
//! probe's, not the author's — the question is whether the choice moves the world at all.
//!
//! Usage: [SPAWN_ONLY=<id>] cargo run --release --bin spawn_identity_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::{Culture, Religion};
use rand::SeedableRng;

fn candidate(id: &str) -> Option<(Religion, Culture)> {
    // SPAWN_ONLY=<id> restricts the substitution to one spawn, to attribute an effect.
    if let Ok(only) = std::env::var("SPAWN_ONLY") {
        if only != id {
            return None;
        }
    }
    match id {
        "wallachia" => Some((Religion::Orthodox, Culture::Latin)),
        "poland_lithuania" => Some((Religion::Catholic, Culture::Slavic)),
        "mamluks" => Some((Religion::Muslim, Culture::Arabic)),
        "france" => Some((Religion::Catholic, Culture::Latin)),
        _ => None,
    }
}

#[derive(Default)]
struct Tally {
    spawned: u32,
    died: u32,
    death_ticks: Vec<u32>,
    wars: u64,
    world_deaths: u64,
    victories: u32,
}

fn run(scenario: &str, strategy: Option<&str>, seed: u64, ticks: u32, authored: bool, spawn: &str) -> Tally {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, scenario.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    let strategy = strategy.map(|s| ScriptedStrategy::from_str(s, scenario));
    let mut t = Tally::default();
    for tick in 0..ticks {
        match &strategy {
            Some(s) => {
                play_scripted_tick(&mut st, s);
            }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let sc = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let ws = st.world_state.as_mut().unwrap();
        if authored {
            for (id, actor) in ws.actors.iter_mut() {
                if let Some((r, c)) = candidate(id) {
                    actor.religion = r;
                    actor.culture = c;
                }
            }
        }
        if t.spawned == 0 && ws.actors.contains_key(spawn) {
            t.spawned = 1;
        }
        if t.spawned == 1 && t.died == 0 && ws.dead_actor_ids.contains(spawn) {
            t.died = 1;
            t.death_ticks.push(tick);
        }
    }
    let ws = st.world_state.as_ref().unwrap();
    t.wars = st
        .event_log
        .events
        .iter()
        .filter(|e| e.id.starts_with("military_conflict_") && e.id.contains(spawn))
        .count() as u64;
    t.world_deaths = ws.dead_actors.len() as u64;
    t.victories = ws.victory_achieved as u32;
    t
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let cases: &[(&str, Option<&str>, &str)] = &[
        ("constantinople_1430", None, "wallachia"),
        ("constantinople_1430", Some("balanced"), "wallachia"),
        ("constantinople_1430", None, "poland_lithuania"),
        ("constantinople_1430", Some("balanced"), "poland_lithuania"),
        ("constantinople_1430", None, "mamluks"),
        ("constantinople_1430", Some("balanced"), "mamluks"),
        ("milan_1477", None, "france"),
        ("milan_1477", Some("aggressive"), "france"),
    ];
    println!("{:<20} {:<10} {:<17} {:<9} {:>8} {:>10} {:>14} {:>10} {:>12} {:>9}",
        "scenario", "world", "spawn", "identity", "spawned", "died", "death tick p50", "wars", "world deaths", "victory");
    for (sc, strat, spawn) in cases {
        for authored in [false, true] {
            let mut tot = Tally::default();
            for seed in 0..seeds {
                let t = run(sc, *strat, seed, ticks, authored, spawn);
                tot.spawned += t.spawned;
                tot.died += t.died;
                tot.death_ticks.extend(t.death_ticks);
                tot.wars += t.wars;
                tot.world_deaths += t.world_deaths;
                tot.victories += t.victories;
            }
            tot.death_ticks.sort();
            let p50 = tot.death_ticks.get(tot.death_ticks.len() / 2).map(|x| x.to_string()).unwrap_or("—".into());
            println!("{:<20} {:<10} {:<17} {:<9} {:>5}/{:<2} {:>7}/{:<2} {:>14} {:>10} {:>12} {:>6}/{:<2}",
                sc, strat.unwrap_or("none"), spawn, if authored { "authored" } else { "pinned" },
                tot.spawned, seeds, tot.died, seeds, p50, tot.wars, tot.world_deaths, tot.victories, seeds);
        }
    }
}
