//! Family arc probe — A2 (docs/TRIAGE.md).
//!
//! rome's family milestones: `family_rises` (influence ≥ 60) and `family_falls` («the
//! family lost everything», influence < 5), and the victory (influence ≥ 90). With the
//! family starting at 0/0/0/0, `family_falls` fired on tick 0 in every game; with any
//! start it fired in 30 of 30, because the family's influence melts on its own. Per rome
//! world: p10 / p50 / p90 tick and count of each of the three.
//!
//! Usage: cargo run --release --bin family_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;

fn pct(v: &[u32]) -> String {
    if v.is_empty() { return "— (0)".into(); }
    let mut s = v.to_vec();
    s.sort();
    let q = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{}/{}/{} ({})", q(0.1), q(0.5), q(0.9), s.len())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("{:<10} {:>22} {:>22} {:>22}", "world", "family_rises", "family_falls", "victory");
    for strat in [None, Some("balanced"), Some("influence"), Some("wealth")] {
        let (mut rises, mut falls, mut wins) = (Vec::new(), Vec::new(), Vec::new());
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, "rome_375".into()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let s = strat.map(|x| ScriptedStrategy::from_str(x, "rome_375"));
            let (mut r, mut f, mut w) = (None, None, None);
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
                if r.is_none() && ws.milestone_events_fired.iter().any(|m| m == "family_rises") { r = Some(t); }
                if f.is_none() && ws.milestone_events_fired.iter().any(|m| m == "family_falls") { f = Some(t); }
                if w.is_none() && ws.victory_achieved { w = Some(t); }
            }
            if let Some(x) = r { rises.push(x); }
            if let Some(x) = f { falls.push(x); }
            if let Some(x) = w { wins.push(x); }
        }
        println!("{:<10} {:>22} {:>22} {:>22}", strat.unwrap_or("none"), pct(&rises), pct(&falls), pct(&wins));
    }
}
