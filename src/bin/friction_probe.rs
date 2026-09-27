//! Friction probe — stage 1 of B41 (docs/TRIAGE.md).
//!
//! `interactions::affinity` is read in two opposite senses. Its tables are a friction
//! scale (same culture 0.2 "low friction", Latin–Turkic 1.0, same religion −0.2), and
//! the army-stretch reader (`effective_military`) and milan's content read it that way.
//! The war roll (`attempt_military_conflict`) reads it as closeness —
//! `military_mod * (1.0 - affinity * 0.5)` — so the most alike fight the most.
//!
//! The probe prints a per-world summary of what a war-roll change can move: military
//! conflicts, how many of them the strong-attacker bonus could apply to, deaths and
//! victory. Run it on the engine as is and with the roll reversed (a local, uncommitted
//! patch) and compare; the probe itself does not touch the engine.
//!
//! Usage: cargo run --release --bin friction_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let worlds: &[(&str, Option<&str>)] = &[
        ("rome_375", None),
        ("rome_375", Some("balanced")),
        ("constantinople_1430", None),
        ("constantinople_1430", Some("balanced")),
        ("constantinople_1430", Some("diplomacy")),
        ("constantinople_1430", Some("military")),
        ("milan_1477", None),
        ("milan_1477", Some("aggressive")),
    ];
    println!("{:<20} {:<10} {:>10} {:>12} {:>10}", "scenario", "world", "wars/game", "deaths/game", "victory");
    for (sc, strat) in worlds {
        let (mut wars, mut deaths, mut wins) = (0u64, 0u64, 0u32);
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let strategy = strat.map(|s| ScriptedStrategy::from_str(s, sc));
            for _ in 0..ticks {
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
            }
            wars += st.event_log.events.iter().filter(|e| e.id.starts_with("military_conflict_")).count() as u64;
            let ws = st.world_state.as_ref().unwrap();
            deaths += ws.dead_actors.len() as u64;
            wins += ws.victory_achieved as u32;
        }
        println!("{:<20} {:<10} {:>10.1} {:>12.2} {:>7}/{}", sc, strat.unwrap_or("none"),
            wars as f64 / seeds as f64, deaths as f64 / seeds as f64, wins, seeds);
    }
}
