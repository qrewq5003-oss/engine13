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
//! Stage 2 (`lever` mode): the lever itself, varied in memory, one at a time — (a) as is
//! (control: must repeat stage 1's one-shot), (b) no −5 legitimacy cost (Milan's treasury
//! alone limits it, like the other allies' actions), (c) the cost kept, Milan's gate lowered
//! from > 60 to > 40. Placement: in balanced and diplomacy right after `venice_diplomacy`, in
//! military last. Per variant and world (no player too — it must not move): Byzantium's
//! legitimacy at ticks 46 / 150, uses per game, Byzantium's falls by the B46 classes, Ottoman
//! deaths, the A10 row with its refined criterion (no win without a player while the
//! Ottomans live), the B46 endings, and Milan's own legitimacy, treasury and deaths.
//!
//! Usage: cargo run --release --bin a38_probe -- [seeds] [ticks] [lever]

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

fn lever(seeds: u64, ticks: u32) {
    use engine13::core::{ActionCondition, MetricRef};
    let sc = "constantinople_1430";
    let milan_leg = MetricRef::literal("actor:milan.legitimacy");
    println!("# A38 stage 2: {sc}, {seeds} seeds × {ticks} ticks\n");
    for world in ["balanced", "diplomacy", "military"] {
        let mut p = ScriptedStrategy::from_str(world, sc).priority_actions();
        match p.iter().position(|a| *a == "venice_diplomacy") {
            Some(i) if world != "military" => p.insert(i + 1, "milan_legitimacy"),
            _ => p.push("milan_legitimacy"),
        }
        let at = p.iter().position(|a| *a == "milan_legitimacy").unwrap();
        println!("placement {world}: {} of {} — {}", at + 1, p.len(), p.join(", "));
    }
    println!();
    println!("| variant | world | Byzantium legitimacy @46 / @150 p10/50/90 (alive) | uses per game (games) | falls: under pressure · after Ottomans · never | Ottomans die | victories, tick p10/50/90, on 40–43; no-player wins with Ottomans alive | endings: held · fell+fed · fell · none | Milan @150: legitimacy, treasury p10/50/90 (alive); Milan dies |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for (variant, label) in [('a', "(a) as is"), ('b', "(b) no legitimacy cost"), ('c', "(c) gate > 40")] {
        for world in ["none", "balanced", "diplomacy", "military"] {
            let prio: Option<Vec<&'static str>> = (world != "none").then(|| {
                let mut p = ScriptedStrategy::from_str(world, sc).priority_actions();
                match p.iter().position(|a| *a == "venice_diplomacy") {
                    Some(i) if world != "military" => p.insert(i + 1, "milan_legitimacy"),
                    _ => p.push("milan_legitimacy"),
                }
                p
            });
            let (mut l46, mut l150, mut ml, mut mt) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            let (mut under, mut after, mut never, mut ott, mut milan_dead) = (0, 0, 0, 0, 0);
            let (mut wins, mut np_alive_wins) = (Vec::new(), 0);
            let (mut held, mut ff, mut fell, mut none) = (0, 0, 0, 0);
            let (mut uses, mut use_games) = (0u32, 0);
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                {
                    let a = st.current_scenario.as_mut().unwrap().patron_actions.iter_mut().find(|a| a.id == "milan_legitimacy").unwrap();
                    match variant {
                        'b' => { a.cost.remove(&milan_leg); }
                        'c' => { if let ActionCondition::Metric { value, .. } = &mut a.available_if { *value = 40.0; } }
                        _ => {}
                    }
                }
                let (mut fall, mut won, mut here) = (None, None, 0u32);
                for _ in 0..ticks {
                    match &prio {
                        Some(p) => { here += play_scripted_priorities_tick(&mut st, p).applied.iter().filter(|a| **a == "milan_legitimacy").count() as u32; }
                        None => {
                            let ws = st.world_state.as_mut().unwrap();
                            let scn = st.current_scenario.as_ref().unwrap();
                            engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                        }
                    }
                    let ws = st.world_state.as_ref().unwrap();
                    let leg = ws.actors.get("byzantium").map(|a| a.get_metric("legitimacy"));
                    if ws.tick == 46 { if let Some(l) = leg { l46.push(l); } }
                    if ws.tick == 150 {
                        if let Some(l) = leg { l150.push(l); }
                        if let Some(m) = ws.actors.get("milan") { ml.push(m.get_metric("legitimacy")); mt.push(m.get_metric("treasury")); }
                    }
                    if fall.is_none() && ws.dead_actor_ids.contains("byzantium") { fall = Some(ws.dead_actor_ids.contains("ottomans")); }
                    if won.is_none() && ws.victory_achieved {
                        won = Some(ws.tick as f64 - 1.0);
                        if world == "none" && !ws.dead_actor_ids.contains("ottomans") { np_alive_wins += 1; }
                    }
                }
                let ws = st.world_state.as_ref().unwrap();
                match fall { Some(false) => under += 1, Some(true) => after += 1, None => never += 1 }
                if ws.dead_actor_ids.contains("ottomans") { ott += 1; }
                if ws.dead_actor_ids.contains("milan") { milan_dead += 1; }
                if let Some(t) = won { wins.push(t); }
                let f = &ws.milestone_events_fired;
                let has = |m: &str| f.iter().any(|x| x == m);
                if has("outcome_survived_alone") { held += 1 } else if has("outcome_fell_federation") { ff += 1 } else if has("outcome_historical") { fell += 1 } else { none += 1 }
                uses += here;
                if here > 0 { use_games += 1; }
            }
            let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
            println!("| {label} | {world} | {} / {} ({}) | {:.1} ({use_games}) | {under} · {after} · {never} | {ott} | {}, {}, {on}; {np_alive_wins} | {held} · {ff} · {fell} · {none} | {}, {} ({}); {milan_dead} |",
                q(&l46), q(&l150), l150.len(), uses as f64 / seeds as f64, wins.len(), q(&wins), q(&ml), q(&mt), ml.len());
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    if args.get(3).map(|s| s.as_str()) == Some("lever") {
        return lever(seeds, ticks);
    }
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
