//! Wallachia probe — acceptance of A31 (docs/TRIAGE.md).
//!
//! `wallachia_emerges` stood on `ottomans.military_size > 70` against a start of 180: a
//! tick-0 milestone disguised as a threshold. A31 writes it as `Tick { tick: 0 }` and
//! turns its text into a description of the start. The world must not move by a bit;
//! only the text and `is_key` of one log line change.
//!
//! Per run (4 constantinople worlds × seeds): the tick the milestone lands in the log,
//! the tick `wallachia` enters the world, and that line's `is_key` and description.
//! Diff the output before and after the edit: only the last two columns may differ.
//!
//! Usage: cargo run --release --bin wallachia_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    for strat in [None, Some("balanced"), Some("diplomacy"), Some("military")] {
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, "constantinople_1430".into()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let s = strat.map(|x| ScriptedStrategy::from_str(x, "constantinople_1430"));
            let (mut fired, mut spawned) = (None, None);
            for t in 0..ticks {
                match &s {
                    Some(s) => { play_scripted_tick(&mut st, s); }
                    None => {
                        let ws = st.world_state.as_mut().unwrap();
                        let scn = st.current_scenario.as_ref().unwrap();
                        engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                    }
                }
                if fired.is_none() {
                    fired = st.event_log.events.iter().find(|e| e.id == "wallachia_emerges")
                        .map(|e| (t, e.is_key, e.description.clone()));
                }
                if spawned.is_none() && st.world_state.as_ref().unwrap().actors.contains_key("wallachia") {
                    spawned = Some(t);
                }
            }
            let (ft, key, text) = match fired {
                Some((t, k, d)) => (t.to_string(), k.to_string(), d),
                None => ("—".into(), "—".into(), "—".into()),
            };
            println!("{:<10} seed {:>2}  fired {:>3}  spawned {:>3}  is_key {:<5}  {}",
                strat.unwrap_or("none"), seed, ft, spawned.map_or("—".into(), |t| t.to_string()), key, text);
        }
    }
}
