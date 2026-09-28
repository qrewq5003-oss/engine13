//! Saturation probe — stage 1 of B29 (docs/TRIAGE.md).
//!
//! `rome.economic_output` was recorded at the 100 ceiling from tick 6 (task 17), which
//! makes any rule gated on it inert, and `economic_output` saturated in all three
//! scenarios (72–94 % of actor-ticks, task 24). This probe re-measures that on today's
//! engine: the share of living actor-ticks at the ceiling, the first tick a named actor
//! reaches it, and which traced writers keep pushing into it (`engine::trace` — the
//! auto-delta and dependency rows the engine emits for the value it just used).
//!
//! Usage: cargo run --release --bin saturation_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::engine::trace;
use rand::SeedableRng;
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let worlds: &[(&str, Option<&str>, &str)] = &[
        ("rome_375", None, "rome"), ("rome_375", Some("balanced"), "rome"),
        ("constantinople_1430", None, "byzantium"), ("constantinople_1430", Some("balanced"), "byzantium"),
        ("milan_1477", None, "milan"), ("milan_1477", Some("aggressive"), "milan"),
    ];
    for (sc, strat, key) in worlds {
        let (mut at_ceiling, mut living) = (0u64, 0u64);
        let mut firsts: Vec<u32> = Vec::new();
        let mut key_at: u64 = 0;
        let mut key_ticks: u64 = 0;
        let mut pushes: BTreeMap<String, (u64, f64)> = BTreeMap::new(); // writer -> (times it pushed a ceiling value up, mass)
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let strategy = strat.map(|s| ScriptedStrategy::from_str(s, sc));
            let dep_names: Vec<String> = st.current_scenario.as_ref().unwrap().dependencies.iter()
                .map(|d| format!("dep {}→{}", d.from.as_str(), d.to.as_str())).collect();
            let _ = dep_names;
            trace::enable();
            let mut first: Option<u32> = None;
            for t in 0..ticks {
                match &strategy {
                    Some(s) => { play_scripted_tick(&mut st, s); }
                    None => {
                        let ws = st.world_state.as_mut().unwrap();
                        let scn = st.current_scenario.as_ref().unwrap();
                        engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                    }
                }
                for r in trace::take_dependencies() {
                    if r.rule.ends_with("economic_output") && r.to_before >= 100.0 && r.delta > 0.0 {
                        let e = pushes.entry(format!("dependency {}", r.rule)).or_default();
                        e.0 += 1; e.1 += r.delta;
                    }
                }
                for r in trace::take_auto_deltas() {
                    if r.metric.ends_with("economic_output") && r.applied > 0.0 {
                        let e = pushes.entry(format!("auto_delta #{} {}", r.index, r.metric)).or_default();
                        e.0 += 1; e.1 += r.applied;
                    }
                }
                let ws = st.world_state.as_ref().unwrap();
                for a in ws.actors.values() {
                    living += 1;
                    if a.get_metric("economic_output") >= 100.0 { at_ceiling += 1; }
                }
                if let Some(k) = ws.actors.get(*key) {
                    key_ticks += 1;
                    if k.get_metric("economic_output") >= 100.0 {
                        key_at += 1;
                        if first.is_none() { first = Some(t); }
                    }
                }
            }
            trace::disable();
            if let Some(f) = first { firsts.push(f); }
        }
        firsts.sort();
        println!("== {sc} {} : eo at ceiling {:.1} % of living actor-ticks; {key} at ceiling {:.1} % of its ticks, first tick p50 {} ({} / {seeds} reach it)",
            strat.unwrap_or("none"), 100.0 * at_ceiling as f64 / living.max(1) as f64,
            100.0 * key_at as f64 / key_ticks.max(1) as f64,
            firsts.get(firsts.len() / 2).map(|x| x.to_string()).unwrap_or("—".into()), firsts.len());
        let mut v: Vec<_> = pushes.into_iter().collect();
        v.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap());
        for (k, (n, m)) in v.iter().take(4) {
            println!("     {k}: {n} rows, mass {m:.0}");
        }
    }
}
