//! Split duration probe — A12 (docs/TRIAGE.md).
//!
//! `rome_splits` fires once `rome.cohesion < 30` has held for `duration` ticks (today 20);
//! the owner's target is a split around 395, i.e. tick ~40. The lever is the duration.
//! Three measurements per rome world, on the honest bot (B42):
//!
//! 1. split tick as a function of duration. Before the split every duration plays the
//!    same world, so one run with the split disabled gives the firing tick for all of
//!    them: the first tick the running dip reaches `duration` ticks;
//! 2. the share of splits on a temporary dip: in that same no-split world, the dip the
//!    split would have fired on either lasts, or cohesion climbs back above 30 — a dip
//!    that ends within `TEMPORARY` ticks is counted as temporary;
//! 3. victories before and after the split, from real runs at each duration.
//!
//! The duration is changed in the scenario held in memory; the engine is not touched.
//!
//! Usage: cargo run --release --bin split_duration_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;

const DURATIONS: &[u32] = &[3, 5, 8, 10, 12, 15, 20];
const TEMPORARY: u32 = 40; // a dip that ends within 20 years

fn start(strat: Option<&str>, seed: u64, duration: u32) -> (engine13::AppState, Option<ScriptedStrategy>) {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, "rome_375".into()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    for m in st.current_scenario.as_mut().unwrap().milestone_events.iter_mut() {
        if m.id == "rome_splits" {
            m.condition.duration = Some(duration);
        }
    }
    (st, strat.map(|s| ScriptedStrategy::from_str(s, "rome_375")))
}

fn step(st: &mut engine13::AppState, s: &Option<ScriptedStrategy>) {
    match s {
        Some(s) => { play_scripted_tick(st, s); }
        None => {
            let ws = st.world_state.as_mut().unwrap();
            let scn = st.current_scenario.as_ref().unwrap();
            engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
        }
    }
}

fn p50(mut v: Vec<u32>) -> String {
    if v.is_empty() { return "—".into(); }
    v.sort();
    v[v.len() / 2].to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    for strat in [None, Some("balanced"), Some("influence"), Some("wealth")] {
        println!("== rome_375 / {} / {seeds} seeds × {ticks}", strat.unwrap_or("no player"));
        // (1)+(2): the no-split world — cohesion series per seed.
        let series: Vec<Vec<f64>> = (0..seeds).map(|seed| {
            let (mut st, s) = start(strat, seed, 1_000_000);
            (0..ticks).map(|_| {
                step(&mut st, &s);
                st.world_state.as_ref().unwrap().actors.get("rome").map(|r| r.get_metric("cohesion")).unwrap_or(100.0)
            }).collect()
        }).collect();
        println!("  {:>8} {:>18} {:>16} {:>26}", "duration", "split tick p50 (n)", "temporary dips", "victory: before / after / none");
        for &d in DURATIONS {
            let (mut fire, mut temporary) = (Vec::new(), 0u32);
            for s in &series {
                let mut run = 0u32;
                for (t, c) in s.iter().enumerate() {
                    run = if *c < 30.0 { run + 1 } else { 0 };
                    if run >= d {
                        fire.push(t as u32);
                        // how long does this dip last in the world without a split?
                        let dip_start = t + 1 - run as usize;
                        let len = s[dip_start..].iter().take_while(|c| **c < 30.0).count() as u32;
                        if dip_start + (len as usize) < s.len() && len < TEMPORARY {
                            temporary += 1;
                        }
                        break;
                    }
                }
            }
            // (3): real runs at this duration.
            let (mut before, mut after, mut none) = (0u32, 0u32, 0u32);
            for seed in 0..seeds {
                let (mut st, s) = start(strat, seed, d);
                let (mut split_at, mut win_at) = (None, None);
                for t in 0..ticks {
                    step(&mut st, &s);
                    let ws = st.world_state.as_ref().unwrap();
                    if split_at.is_none() && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { split_at = Some(t); }
                    if win_at.is_none() && ws.victory_achieved { win_at = Some(t); }
                }
                match (win_at, split_at) {
                    (None, _) => none += 1,
                    (Some(w), Some(sp)) if w >= sp => after += 1,
                    _ => before += 1,
                }
            }
            println!("  {:>8} {:>18} {:>13}/{:<2} {:>12} / {} / {}",
                d, format!("{} ({}/{})", p50(fire.clone()), fire.len(), seeds), temporary, fire.len(), before, after, none);
        }
    }
}
