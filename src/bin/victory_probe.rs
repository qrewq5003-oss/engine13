//! Victory probe — the A10 premise, measured on whatever bot the engine has.
//!
//! A10 was framed on "federation saturates by tick 4, victory is a timer at tick 42
//! without the Ottoman gate". B42 found that the scripted player bought actions it could
//! not afford (the apply path did not check cost); this probe re-measures the premise.
//!
//! constantinople: first tick `federation_progress >= 80`; victory tick under the real
//! rule; and the victory tick the rule would give without its extra condition
//! (`ottomans.military_size < 40`) — federation at or above the threshold for the
//! required sustained ticks, not before `minimum_tick`. rome: victory tick.
//!
//! The last column is the spread of the real victory, for the A10 criteria (B44 stage 2
//! re-measures them): p10 / p50 / p90 and how many wins land on ticks 40–43.
//!
//! And, for constantinople, the state at each victory (A10's criterion as refined after A37
//! stage 2: without a player there is no victory **while the Ottomans live**): per seed, the
//! tick and whether the Ottomans are dead and Byzantium alive at that moment.
//!
//! Usage: cargo run --release --bin victory_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::MetricRef;
use rand::SeedableRng;

fn p50(mut v: Vec<u32>) -> String {
    if v.is_empty() {
        return "—".into();
    }
    v.sort();
    v[v.len() / 2].to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let worlds: &[(&str, Option<&str>)] = &[
        ("constantinople_1430", None), ("constantinople_1430", Some("balanced")),
        ("constantinople_1430", Some("diplomacy")), ("constantinople_1430", Some("military")),
        ("rome_375", Some("balanced")), ("rome_375", Some("influence")), ("rome_375", Some("wealth")),
    ];
    println!("{:<20} {:<10} {:>22} {:>22} {:>28} {:>30}", "scenario", "world", "fed >= 80 first (n)", "victory tick (n)", "victory w/o extra cond (n)", "victory p10/50/90, on 40–43");
    for (sc, strat) in worlds {
        let (mut fed, mut win, mut bare) = (Vec::new(), Vec::new(), Vec::new());
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let strategy = strat.map(|s| ScriptedStrategy::from_str(s, sc));
            let vc = st.current_scenario.as_ref().unwrap().victory_condition.clone();
            let fed_ref = MetricRef::literal("global:federation_progress");
            let (mut f, mut w, mut b, mut streak) = (None, None, None, 0u32);
            let mut at_win = None;
            for t in 0..ticks {
                match &strategy {
                    Some(s) => { play_scripted_tick(&mut st, s); }
                    None => {
                        let ws = st.world_state.as_mut().unwrap();
                        let scn = st.current_scenario.as_ref().unwrap();
                        engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                    }
                }
                let ws = st.world_state.as_ref().unwrap();
                if sc.starts_with("constantinople") {
                    let v = fed_ref.get(ws);
                    if f.is_none() && v >= 80.0 { f = Some(t); }
                    if let Some(vc) = &vc {
                        streak = if vc.metric.get(ws) >= vc.threshold { streak + 1 } else { 0 };
                        if b.is_none() && t >= vc.minimum_tick && streak >= vc.sustained_ticks_required {
                            b = Some(t);
                        }
                    }
                }
                if w.is_none() && ws.victory_achieved {
                    w = Some(t);
                    at_win = Some((ws.dead_actor_ids.contains("ottomans"), ws.actors.contains_key("byzantium")));
                }
            }
            if let Some(x) = f { fed.push(x); }
            if let Some(x) = w { win.push(x); }
            if let (Some(t), Some((ott_dead, byz_alive)), true) = (w, at_win, sc.starts_with("constantinople")) {
                if strat.is_none() || ott_dead {
                    eprintln!("victory-state {sc} {} seed {seed} tick {t} ottomans_dead={ott_dead} byzantium_alive={byz_alive}", strat.unwrap_or("none"));
                }
            }
            if let Some(x) = b { bare.push(x); }
        }
        let fmt = |v: &Vec<u32>| format!("{} ({}/{})", p50(v.clone()), v.len(), seeds);
        let spread = {
            let mut s = win.clone();
            s.sort();
            let q = |p: f64| s.get(((s.len().max(1) - 1) as f64 * p).round() as usize).map_or("—".into(), |x| x.to_string());
            format!("{}/{}/{}, {}", q(0.1), q(0.5), q(0.9), s.iter().filter(|t| (40..=43).contains(*t)).count())
        };
        println!("{:<20} {:<10} {:>22} {:>22} {:>28} {:>30}", sc, strat.unwrap_or("none"), fmt(&fed), fmt(&win), fmt(&bare), spread);
    }
}
