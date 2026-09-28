//! Federation probe — stage 1 of A10, redone after the owner's correction (docs/TRIAGE.md).
//!
//! The first pass (PR #164) counted wins over a dead Byzantium: the pull that holds the
//! federation down stands on `byzantium.external_pressure > 70`, and dies with her. Since
//! A10 step 1 the victory requires a living Byzantium; this probe counts everything only
//! while she lives. Levers, each an edit of the scenario held in memory (the engine is
//! not touched):
//!
//! (b) the world pulls the federation down — the negative terms of the federation
//!     auto-delta (Byzantine pressure > 70 → −2, Ottoman army > 220 → −3) × 1.5 / 2 / 3;
//! (c) the start rush: every action effect on `federation_progress` × 0.5;
//! (d) the start rush: the allies' starting treasuries (venice, genoa, milan) × 0.5;
//! and (c), (d) each paired with every (b).
//!
//! Per world: the tick federation first reaches 80 with Byzantium alive (p10/p50/p90, n);
//! the share of Byzantium's living ticks with federation ≥ 80; the victory under the real
//! rule; and the victory without its Ottoman condition — ≥ 80 for the sustained ticks
//! from `minimum_tick`, Byzantium alive.
//!
//! Usage: cargo run --release --bin federation_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::MetricRef;
use rand::SeedableRng;

#[derive(Clone, Copy)]
struct Lever { pull: f64, effects: f64, treasury: f64 }

fn apply(sc: &mut engine13::core::Scenario, l: Lever) {
    let fed = MetricRef::literal("global:federation_progress");
    for ad in sc.auto_deltas.iter_mut().filter(|d| d.metric == fed) {
        for c in ad.conditions.iter_mut().filter(|c| c.delta < 0.0) {
            c.delta *= l.pull;
        }
    }
    for a in sc.patron_actions.iter_mut() {
        if let Some(v) = a.effects.get_mut(&fed) {
            *v *= l.effects;
        }
    }
    for a in sc.actors.iter_mut().filter(|a| ["venice", "genoa", "milan"].contains(&a.id.as_str())) {
        if let Some(t) = a.metrics.get_mut("treasury") {
            *t *= l.treasury;
        }
    }
}

fn pct(v: &[u32], q: f64) -> String {
    if v.is_empty() { return "—".into(); }
    let mut s = v.to_vec();
    s.sort();
    s[((s.len() - 1) as f64 * q).round() as usize].to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let mut levers: Vec<(String, Lever)> = vec![("base".into(), Lever { pull: 1.0, effects: 1.0, treasury: 1.0 })];
    for p in [1.5, 2.0, 3.0] { levers.push((format!("(b)×{p}"), Lever { pull: p, effects: 1.0, treasury: 1.0 })); }
    levers.push(("(c)".into(), Lever { pull: 1.0, effects: 0.5, treasury: 1.0 }));
    levers.push(("(d)".into(), Lever { pull: 1.0, effects: 1.0, treasury: 0.5 }));
    for p in [1.5, 2.0, 3.0] {
        levers.push((format!("(c)+(b)×{p}"), Lever { pull: p, effects: 0.5, treasury: 1.0 }));
        levers.push((format!("(d)+(b)×{p}"), Lever { pull: p, effects: 1.0, treasury: 0.5 }));
    }
    println!("{:<13} {:<10} {:>24} {:>14} {:>16} {:>30}", "lever", "world", "fed≥80 first p10/50/90 (n)", "≥80 share", "victory p50 (n)", "w/o Ottoman p10/50/90 (n)");
    for (name, lever) in &levers {
        for strat in [None, Some("balanced"), Some("diplomacy"), Some("military")] {
            let (mut first, mut win, mut bare) = (Vec::new(), Vec::new(), Vec::new());
            let (mut above, mut alive_ticks) = (0u64, 0u64);
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, "constantinople_1430".into()).unwrap();
                apply(st.current_scenario.as_mut().unwrap(), *lever);
                // Starting treasuries live in the world too: re-take the edited actors.
                let edited: Vec<engine13::core::Actor> = st.current_scenario.as_ref().unwrap().actors.clone();
                for a in edited {
                    if let Some(w) = st.world_state.as_mut().unwrap().actors.get_mut(&a.id) { w.metrics = a.metrics.clone(); }
                }
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                let s = strat.map(|x| ScriptedStrategy::from_str(x, "constantinople_1430"));
                let vc = st.current_scenario.as_ref().unwrap().victory_condition.clone().unwrap();
                let (mut f, mut w, mut b, mut streak) = (None, None, None, 0u32);
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
                    if w.is_none() && ws.victory_achieved { w = Some(t); }
                    if !ws.actors.contains_key("byzantium") { continue; }
                    let v = vc.metric.get(ws);
                    alive_ticks += 1;
                    if v >= vc.threshold { above += 1; }
                    if f.is_none() && v >= vc.threshold { f = Some(t); }
                    streak = if v >= vc.threshold { streak + 1 } else { 0 };
                    if b.is_none() && t >= vc.minimum_tick && streak >= vc.sustained_ticks_required { b = Some(t); }
                }
                if let Some(x) = f { first.push(x); }
                if let Some(x) = w { win.push(x); }
                if let Some(x) = b { bare.push(x); }
            }
            println!("{:<13} {:<10} {:>24} {:>13.1}% {:>16} {:>30}", name, strat.unwrap_or("none"),
                format!("{}/{}/{} ({})", pct(&first, 0.1), pct(&first, 0.5), pct(&first, 0.9), first.len()),
                100.0 * above as f64 / alive_ticks.max(1) as f64,
                format!("{} ({})", pct(&win, 0.5), win.len()),
                format!("{}/{}/{} ({})", pct(&bare, 0.1), pct(&bare, 0.5), pct(&bare, 0.9), bare.len()));
        }
    }
}
