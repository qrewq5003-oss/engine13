//! Condition occupancy — how often each authored condition is true, measured where the
//! engine evaluates it (docs/TRIAGE.md, re-check of the items lost from the old task file).
//!
//! Built with `--features census`: `core::census` counts every annotated condition —
//! auto-delta conditions and ratio valves, milestones, rank conditions, random-event gates,
//! action availability — at the moment of evaluation. A tick-boundary snapshot would be
//! wrong for some of them: the treasury moves inside the tick (`apply_treasury`) right
//! before the auto-deltas read it.
//!
//! Per scenario and world (no player and each scripted strategy), seeds × ticks: for every
//! (content, test) the share of evaluations that were true and how many there were. A
//! condition whose actor is absent counts too (it reads 0.0, or `false` with `actor_id`).
//!
//! Usage: cargo run --release --features census --bin condition_occupancy_probe -- [seeds] [ticks] [filter]
//! `filter` keeps rows whose content or test contains it (e.g. `treasury`, `ratio`).

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const WORLDS: &[(&str, &[&str])] = &[
    ("rome_375", &["none", "balanced", "influence", "wealth"]),
    ("constantinople_1430", &["none", "balanced", "diplomacy", "military"]),
    ("milan_1477", &["none", "aggressive"]),
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let filter = args.get(3).cloned().unwrap_or_default();
    println!("# condition occupancy: {seeds} seeds × {ticks} ticks per world; filter `{filter}`\n");
    for (scenario, worlds) in WORLDS {
        // (content, test) -> per world (true, evaluated)
        let mut table: BTreeMap<(String, String), Vec<(u64, u64)>> = BTreeMap::new();
        for (wi, world) in worlds.iter().enumerate() {
            census::enable_occupancy();
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, scenario.to_string()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, scenario));
                for _ in 0..ticks {
                    match &strategy {
                        Some(s) => { play_scripted_tick(&mut st, s); }
                        None => {
                            let ws = st.world_state.as_mut().unwrap();
                            let sc = st.current_scenario.as_ref().unwrap();
                            engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
                        }
                    }
                }
            }
            for (k, v) in census::take_occupancy() {
                if !filter.is_empty() && !k.0.contains(&filter) && !k.1.contains(&filter) {
                    continue;
                }
                table.entry(k).or_insert_with(|| vec![(0, 0); worlds.len()])[wi] = v;
            }
        }
        println!("## {scenario} — true % (evaluations) per world: {}\n", worlds.join(" · "));
        println!("| content | test | {} |", worlds.join(" | "));
        println!("|---|---|{}", "---|".repeat(worlds.len()));
        for ((ctx, test), cells) in &table {
            let c: Vec<String> = cells.iter()
                .map(|(t, n)| if *n == 0 { "—".into() } else { format!("{:.1} % ({n})", 100.0 * *t as f64 / *n as f64) })
                .collect();
            println!("| {ctx} | {test} | {} |", c.join(" | "));
        }
        println!();
    }
}
