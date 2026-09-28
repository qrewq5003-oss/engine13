//! Rome arc probe — acceptance of A12 and the A11 re-measure (docs/TRIAGE.md).
//!
//! A12 moved the split of the empire to a date (tick 40, 395). The split shrinks Rome
//! (population × its share, army × share × 0.7, …), so it now hits at tick 40 instead
//! of 103–138, and Rome's mortality — A11's subject (the West fell in 476, tick 202) —
//! may move. Per rome world: the split tick, whether `rome_east` is alive right after it,
//! how often Rome (the western seat) dies and when, and victories.
//!
//! Usage: cargo run --release --bin rome_arc_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;

fn p50(mut v: Vec<u32>) -> String {
    if v.is_empty() { return "—".into(); }
    v.sort();
    v[v.len() / 2].to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("{:<10} {:>16} {:>14} {:>22} {:>16} {:>14}", "world", "split tick (n)", "east alive", "rome dies: n / tick p50", "rome death year", "victory (n)");
    for strat in [None, Some("balanced"), Some("influence"), Some("wealth")] {
        let (mut split, mut east_ok, mut death, mut wins) = (Vec::new(), 0u32, Vec::new(), Vec::new());
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, "rome_375".into()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let s = strat.map(|x| ScriptedStrategy::from_str(x, "rome_375"));
            let (mut sp, mut dt, mut w) = (None, None, None);
            for t in 0..ticks {
                match &s {
                    Some(s) => { play_scripted_tick(&mut st, s); }
                    None => {
                        let ws = st.world_state.as_mut().unwrap();
                        let scn = st.current_scenario.as_ref().unwrap();
                        engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                    }
                }
                let ws = st.world_state.as_ref().unwrap();
                if sp.is_none() && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") {
                    sp = Some(t);
                    if ws.actors.contains_key("rome_east") { east_ok += 1; }
                }
                if dt.is_none() && ws.dead_actor_ids.contains("rome") { dt = Some(t); }
                if w.is_none() && ws.victory_achieved { w = Some(t); }
            }
            if let Some(x) = sp { split.push(x); }
            if let Some(x) = dt { death.push(x); }
            if let Some(x) = w { wins.push(x); }
        }
        let year = if death.is_empty() { "—".to_string() } else {
            let mut d = death.clone(); d.sort(); format!("{}", 375 + d[d.len() / 2] / 2)
        };
        println!("{:<10} {:>16} {:>11}/{:<2} {:>22} {:>16} {:>14}",
            strat.unwrap_or("none"), format!("{} ({})", p50(split.clone()), split.len()), east_ok, seeds,
            format!("{} / {}", death.len(), p50(death.clone())), year, format!("{} ({})", p50(wins.clone()), wins.len()));
    }
}
