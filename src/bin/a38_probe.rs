//! A38 stage 1 — the protagonist's legitimacy when the bot plays the full game.
//!
//! No scripted strategy uses `milan_legitimacy`, the one action that raises Byzantium's
//! legitimacy (+8; cost: Milan's treasury −25 and legitimacy −5; available while
//! `milan.legitimacy > 60`). Every constantinople measurement so far was made in a game
//! where nobody supports the protagonist's legitimacy — and legitimacy 0 is the heart of
//! the «zombie Byzantium» of A37. This probe adds the action to each strategy **in memory**
//! (the committed strategies are not touched) through the library's own loop
//! (`play_scripted_priorities_tick`, no copy of it) and compares:
//!
//! - Byzantium's legitimacy at ticks 46 and 150 (p10/p50/p90 over games where she lives);
//! - Byzantium's falls by the two classes of B46 (under Ottoman pressure / after the
//!   Ottomans died);
//! - the A10 row: victories, tick p10/p50/p90, wins on ticks 40–43;
//! - the B46 endings;
//! - how often the bot actually took `milan_legitimacy`.
//!
//! Placements: `base` (the strategy as committed), `second` (right after the strategy's
//! lead action — the chosen placement) and `last` (sensitivity).
//!
//! Usage: cargo run --release --bin a38_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_priorities_tick, ScriptedStrategy};
use rand::SeedableRng;

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let sc = "constantinople_1430";
    println!("# A38 stage 1: {sc}, {seeds} seeds × {ticks} ticks; `milan_legitimacy` added in memory\n");
    println!("| world | placement | leg@46 p10/50/90 (alive) | leg@150 p10/50/90 (alive) | falls: under pressure · after Ottomans · never | victories, tick p10/50/90, on 40–43 | survived_alone · fell_federation · historical | milan_legitimacy taken (games / times, turn p10/50/90) |");
    println!("|---|---|---|---|---|---|---|---|");
    for world in ["balanced", "diplomacy", "military"] {
        let base = ScriptedStrategy::from_str(world, sc).priority_actions();
        let mut second = base.clone();
        second.insert(1, "milan_legitimacy");
        let mut last = base.clone();
        last.push("milan_legitimacy");
        for (label, prio) in [("base", base), ("second", second), ("last", last)] {
            let (mut l46, mut l150) = (Vec::new(), Vec::new());
            let (mut under, mut after, mut never) = (0, 0, 0);
            let mut wins = Vec::new();
            let (mut sa, mut ff, mut hist) = (0, 0, 0);
            let (mut took_games, mut took) = (0, 0u32);
            let mut took_ticks: Vec<f64> = Vec::new();
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                let mut fell: Option<bool> = None;
                let mut won = None;
                let mut took_here = 0;
                for _ in 0..ticks {
                    let turn = play_scripted_priorities_tick(&mut st, &prio);
                    let n = turn.applied.iter().filter(|a| **a == "milan_legitimacy").count() as u32;
                    if n > 0 { took_ticks.push(st.world_state.as_ref().unwrap().tick as f64 - 1.0); }
                    took_here += n;
                    let ws = st.world_state.as_ref().unwrap();
                    let leg = ws.actors.get("byzantium").map(|a| a.get_metric("legitimacy"));
                    if ws.tick == 46 { if let Some(l) = leg { l46.push(l); } }
                    if ws.tick == 150 { if let Some(l) = leg { l150.push(l); } }
                    if fell.is_none() && ws.dead_actor_ids.contains("byzantium") {
                        fell = Some(ws.dead_actor_ids.contains("ottomans"));
                    }
                    if won.is_none() && ws.victory_achieved { won = Some(ws.tick); }
                }
                let ws = st.world_state.as_ref().unwrap();
                match fell { Some(false) => under += 1, Some(true) => after += 1, None => never += 1 }
                if let Some(t) = won { wins.push(t as f64); }
                let f = &ws.milestone_events_fired;
                if f.iter().any(|m| m == "outcome_survived_alone") { sa += 1; }
                if f.iter().any(|m| m == "outcome_fell_federation") { ff += 1; }
                if f.iter().any(|m| m == "outcome_historical") { hist += 1; }
                if took_here > 0 { took_games += 1; }
                took += took_here;
            }
            // Victory ticks are `world.tick` after the turn, as in `victory_probe`'s loop
            // index + 1; the 40–43 window is counted on the turn index for comparability.
            let on_4043 = wins.iter().filter(|t| (41.0..=44.0).contains(*t)).count();
            let w: Vec<f64> = wins.iter().map(|t| t - 1.0).collect();
            println!("| {world} | {label} | {} ({}) | {} ({}) | {under} · {after} · {never} | {}, {}, {on_4043} | {sa} · {ff} · {hist} | {took_games} / {took}, {} |",
                q(&l46), l46.len(), q(&l150), l150.len(), wins.len(), q(&w), q(&took_ticks));
        }
    }
}
