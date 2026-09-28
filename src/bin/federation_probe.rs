//! Federation probe — stage 1 of A10 (docs/TRIAGE.md).
//!
//! Victory in constantinople is `federation_progress >= 80` from `minimum_tick` 40 (plus
//! `ottomans.military_size < 40`). Even the honest bot (B42) brings the federation to 80
//! by tick 4, so without the Ottoman condition the victory is a timer on tick ~40. The
//! owner asked for two levers, measured separately, with no balance edit committed:
//!
//! (a) the ally's treasury really limits — three actions carry their payment in
//!     `effects` (venice_naval_support −50, genoa_financial_aid −70, milan_condottieri
//!     −80), which availability does not check; the emulation moves that payment into
//!     `cost`, so the one availability rule refuses an ally who cannot pay;
//! (b) the world pulls the federation down — the federation auto-delta already has two
//!     negative terms (Byzantine pressure > 70 → −2, Ottoman army > 220 → −3); the
//!     emulation scales them ×3 and ×10 to see whether this lever moves anything.
//!
//! Both edit the scenario held in memory; the engine is not touched. Per world: the tick
//! federation first reaches 80 (p10 / p50 / p90, n), the victory under the real rule, and
//! the victory the rule would give without its Ottoman condition.
//!
//! Usage: cargo run --release --bin federation_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::MetricRef;
use rand::SeedableRng;

#[derive(Clone, Copy)]
enum Lever { None, CostBinding, WorldPull(f64) }

fn apply(scenario: &mut engine13::core::Scenario, lever: Lever) {
    match lever {
        Lever::None => {}
        Lever::CostBinding => {
            for a in scenario.patron_actions.iter_mut() {
                let Some(src) = a.source_actor_id.clone() else { continue };
                let key = MetricRef::literal(&format!("actor:{src}.treasury"));
                if let Some(v) = a.effects.get(&key).copied() {
                    if v < 0.0 {
                        a.effects.remove(&key);
                        *a.cost.entry(key).or_insert(0.0) += v;
                    }
                }
            }
        }
        Lever::WorldPull(k) => {
            let fed = MetricRef::literal("global:federation_progress");
            for ad in scenario.auto_deltas.iter_mut().filter(|d| d.metric == fed) {
                for c in ad.conditions.iter_mut().filter(|c| c.delta < 0.0) {
                    c.delta *= k;
                }
            }
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
    let levers = [("base", Lever::None), ("(a) cost", Lever::CostBinding), ("(b) ×3", Lever::WorldPull(3.0)), ("(b) ×10", Lever::WorldPull(10.0))];
    println!("{:<9} {:<10} {:>22} {:>20} {:>28}", "lever", "world", "fed≥80: p10/p50/p90 (n)", "victory p50 (n)", "w/o Ottoman: p10/p50/p90 (n)");
    for (name, lever) in levers {
        for strat in [None, Some("balanced"), Some("diplomacy"), Some("military")] {
            let (mut fed, mut win, mut bare) = (Vec::new(), Vec::new(), Vec::new());
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, "constantinople_1430".into()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                apply(st.current_scenario.as_mut().unwrap(), lever);
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
                    let v = vc.metric.get(ws);
                    if f.is_none() && v >= vc.threshold { f = Some(t); }
                    streak = if v >= vc.threshold { streak + 1 } else { 0 };
                    if b.is_none() && t >= vc.minimum_tick && streak >= vc.sustained_ticks_required { b = Some(t); }
                    if w.is_none() && ws.victory_achieved { w = Some(t); }
                }
                if let Some(x) = f { fed.push(x); }
                if let Some(x) = w { win.push(x); }
                if let Some(x) = b { bare.push(x); }
            }
            println!("{:<9} {:<10} {:>22} {:>20} {:>28}", name, strat.unwrap_or("none"),
                format!("{}/{}/{} ({})", pct(&fed, 0.1), pct(&fed, 0.5), pct(&fed, 0.9), fed.len()),
                format!("{} ({})", pct(&win, 0.5), win.len()),
                format!("{}/{}/{} ({})", pct(&bare, 0.1), pct(&bare, 0.5), pct(&bare, 0.9), bare.len()));
        }
    }
}
