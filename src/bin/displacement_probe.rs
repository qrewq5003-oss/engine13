//! Displacement probe — stage 1 of A30 + B37 (docs/TRIAGE.md).
//!
//! Cultural displacement never fires: progress peaks at 28.4 of 100 in every world. The
//! arithmetic says why — a cultural interaction adds `min(delta × 0.05, 3.0)` and every
//! tick subtracts 5.0 (`phase_actor_tags`), so one aggressor at the cap still loses 2 a
//! tick; only several aggressors at once can accumulate. This probe reports, per world:
//! displacements per game, games with one, the peak progress, and how many
//! displacements the `.take(3)` of transferred tags decided by hash order (the aggressor
//! had more than three candidate tags — B37). Run it on the engine as is and with the
//! decay lowered (a local, uncommitted patch) to see what reviving the mechanic does.
//!
//! Usage: cargo run --release --bin displacement_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let worlds: &[(&str, Option<&str>)] = &[
        ("rome_375", None), ("rome_375", Some("balanced")),
        ("constantinople_1430", None), ("constantinople_1430", Some("balanced")),
        ("milan_1477", None), ("milan_1477", Some("aggressive")),
    ];
    println!("{:<20} {:<10} {:>13} {:>12} {:>9} {:>14} {:>10} {:>9} {:>8}",
        "scenario", "world", "displ./game", "games w/ ≥1", "peak", "hash-decided", "wars/game", "deaths/g", "victory");
    for (sc, strat) in worlds {
        let (mut displ, mut games, mut hash, mut wars, mut deaths, mut wins) = (0u64, 0u32, 0u64, 0u64, 0u64, 0u32);
        let mut peak: f64 = 0.0;
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let strategy = strat.map(|s| ScriptedStrategy::from_str(s, sc));
            let mut here = 0u64;
            for _ in 0..ticks {
                let before = st.event_log.events.len();
                // Aggressors' tags before the tick, to judge whether `.take(3)` chose.
                let tags_before: std::collections::HashMap<String, Vec<String>> = st.world_state.as_ref().unwrap()
                    .actors.iter().map(|(id, a)| (id.clone(), a.actor_tags.keys().cloned().collect())).collect();
                let target_tags_before = tags_before.clone();
                match &strategy {
                    Some(s) => { play_scripted_tick(&mut st, s); }
                    None => {
                        let ws = st.world_state.as_mut().unwrap();
                        let scn = st.current_scenario.as_ref().unwrap();
                        engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                    }
                }
                let ws = st.world_state.as_ref().unwrap();
                for v in ws.cultural_displacement_progress.values() {
                    peak = peak.max(*v);
                }
                for e in &st.event_log.events[before..] {
                    if let Some(target) = e.id.strip_prefix("cultural_displacement_") {
                        here += 1;
                        let aggressor = e.description.rsplit(' ').next().unwrap_or("");
                        let a_tags = tags_before.get(aggressor).cloned().unwrap_or_default();
                        let t_tags = target_tags_before.get(target).cloned().unwrap_or_default();
                        let candidates = a_tags.iter().filter(|t| !t_tags.contains(t)).count();
                        if candidates > 3 {
                            hash += 1;
                        }
                    }
                }
            }
            displ += here;
            if here > 0 { games += 1; }
            wars += st.event_log.events.iter().filter(|e| e.id.starts_with("military_conflict_")).count() as u64;
            let ws = st.world_state.as_ref().unwrap();
            deaths += ws.dead_actors.len() as u64;
            wins += ws.victory_achieved as u32;
        }
        let n = seeds as f64;
        println!("{:<20} {:<10} {:>13.2} {:>9}/{:<2} {:>9.1} {:>14} {:>10.1} {:>9.2} {:>5}/{}",
            sc, strat.unwrap_or("none"), displ as f64 / n, games, seeds, peak, hash, wars as f64 / n, deaths as f64 / n, wins, seeds);
    }
}
