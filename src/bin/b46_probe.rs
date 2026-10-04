//! B46 stage 1 — the fall of Constantinople and the scenario's endings (docs/TRIAGE.md).
//!
//! Measured after B44 stage 2 and the «адресат погиб» rule, so nothing here is propped up
//! by help to a dead city. Nothing is changed. Per constantinople world, seeds × ticks:
//!
//! 1. Byzantium's fall: how often, on which tick, and the federation at that moment —
//!    split by class (owner, after #176): "under Ottoman pressure" (the Ottomans alive at
//!    the fall) and "after the Ottomans died" (the zombie artefact of A37/A38);
//! 2. tick 46 (1453, the historical fall): Byzantium alive or not, federation then;
//! 3. every `outcome_*` milestone and `constantinople_holds`: fired, tick, with Byzantium
//!    alive or dead at the firing;
//! 4. outcomes that fire together in one game (they read as mutually exclusive endings);
//! 5. the mode: when the game goes to `Consequences`, and games where the city fell but the
//!    mode never changed;
//! 6. acceptance of the B46 endings (stage 2): outcomes per game, victory + outcome pairs,
//!    each ending's tick and condition, `constantinople_falls` and the mode in every fallen
//!    game, milestones fired over a dead city, `constantinople_holds` after `final_assault`.
//!
//! Ticks are `world.tick` after the turn; the tick a condition was checked on is one less.
//!
//! Usage: cargo run --release --bin b46_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::GameMode;
use rand::SeedableRng;
use std::collections::BTreeMap;

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

#[derive(Default)]
struct Run {
    byz_fall: Option<(u32, f64, bool)>, // tick, federation, Ottomans already dead
    at46: Option<(bool, f64)>,
    fired: BTreeMap<String, (u32, bool)>, // milestone -> (tick, Byzantium alive)
    consequences: Option<u32>,
    end_mode: String,
    victory: Option<u32>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let sc = "constantinople_1430";
    println!("# B46 stage 1: {sc}, {seeds} seeds × {ticks} ticks\n");
    for world in ["none", "balanced", "diplomacy", "military"] {
        let mut runs = Vec::new();
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
            let mut r = Run::default();
            for _ in 0..ticks {
                match &strategy {
                    Some(s) => { play_scripted_tick(&mut st, s); }
                    None => {
                        let ws = st.world_state.as_mut().unwrap();
                        let scn = st.current_scenario.as_ref().unwrap();
                        engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                    }
                }
                let ws = st.world_state.as_ref().unwrap();
                let fed = ws.global_metrics.get("federation_progress").copied().unwrap_or(0.0);
                let byz_alive = ws.actors.contains_key("byzantium");
                if r.byz_fall.is_none() && ws.dead_actor_ids.contains("byzantium") {
                    r.byz_fall = Some((ws.tick, fed, ws.dead_actor_ids.contains("ottomans")));
                }
                if ws.tick == 46 { r.at46 = Some((byz_alive, fed)); }
                for m in &ws.milestone_events_fired {
                    r.fired.entry(m.clone()).or_insert((ws.tick, byz_alive));
                }
                if r.victory.is_none() && ws.victory_achieved { r.victory = Some(ws.tick); }
                if r.consequences.is_none() && ws.game_mode == GameMode::Consequences {
                    r.consequences = Some(ws.tick);
                }
            }
            r.end_mode = format!("{:?}", st.world_state.as_ref().unwrap().game_mode);
            runs.push(r);
        }

        println!("## {world}\n");
        println!("| fall class | runs | tick p10/50/90 | federation at fall p10/50/90 |");
        println!("|---|---|---|---|");
        for (label, after) in [("under Ottoman pressure", false), ("after the Ottomans died", true)] {
            let f: Vec<&(u32, f64, bool)> = runs.iter().filter_map(|r| r.byz_fall.as_ref()).filter(|f| f.2 == after).collect();
            println!("| {label} | {} | {} | {} |", f.len(),
                q(&f.iter().map(|f| f.0 as f64).collect::<Vec<_>>()), q(&f.iter().map(|f| f.1).collect::<Vec<_>>()));
        }
        let standing = runs.iter().filter(|r| r.byz_fall.is_none()).count();
        println!("| never fell | {standing} | — | — |\n");

        let alive46: Vec<f64> = runs.iter().filter_map(|r| r.at46).filter(|a| a.0).map(|a| a.1).collect();
        let dead46: Vec<f64> = runs.iter().filter_map(|r| r.at46).filter(|a| !a.0).map(|a| a.1).collect();
        println!("tick 46 (1453): Byzantium alive {}/{seeds} (federation p10/50/90 {}), fallen {} (federation {})\n",
            alive46.len(), q(&alive46), dead46.len(), q(&dead46));

        println!("| milestone | fired | tick p10/50/90 | with Byzantium alive | with Byzantium dead |");
        println!("|---|---|---|---|---|");
        let mut ids: Vec<&String> = runs.iter().flat_map(|r| r.fired.keys()).collect();
        ids.sort();
        ids.dedup();
        for id in ids.iter().filter(|i| i.starts_with("outcome_") || i.as_str() == "constantinople_holds") {
            let f: Vec<&(u32, bool)> = runs.iter().filter_map(|r| r.fired.get(*id)).collect();
            println!("| {id} | {} | {} | {} | {} |", f.len(), q(&f.iter().map(|f| f.0 as f64).collect::<Vec<_>>()),
                f.iter().filter(|f| f.1).count(), f.iter().filter(|f| !f.1).count());
        }
        let mut pairs: BTreeMap<String, usize> = BTreeMap::new();
        for r in &runs {
            let o: Vec<&String> = r.fired.keys().filter(|k| k.starts_with("outcome_")).collect();
            for i in 0..o.len() {
                for j in i + 1..o.len() {
                    *pairs.entry(format!("{} + {}", o[i], o[j])).or_default() += 1;
                }
            }
        }
        let per_run: BTreeMap<usize, usize> = runs.iter().fold(BTreeMap::new(), |mut m, r| {
            *m.entry(r.fired.keys().filter(|k| k.starts_with("outcome_")).count()).or_default() += 1;
            m
        });
        println!("\noutcomes per game (count → games): {per_run:?}");
        for (p, n) in &pairs { println!("  together: {p} — {n} games"); }

        let cons: Vec<f64> = runs.iter().filter_map(|r| r.consequences.map(|t| t as f64)).collect();
        let fell_no_switch = runs.iter().filter(|r| r.byz_fall.is_some() && r.consequences.is_none()).count();
        let mut ends: BTreeMap<String, usize> = BTreeMap::new();
        for r in &runs { *ends.entry(r.end_mode.clone()).or_default() += 1; }
        println!("\nmode: Consequences in {}/{seeds} (tick p10/50/90 {}); city fell but mode never changed: {fell_no_switch}; end modes {ends:?}\n",
            cons.len(), q(&cons));

        // 6. B46 acceptance.
        let endings = ["outcome_survived_alone", "outcome_fell_federation", "outcome_historical"];
        let n_end = |r: &Run| endings.iter().filter(|e| r.fired.contains_key(**e)).count();
        let mut dist: BTreeMap<usize, usize> = BTreeMap::new();
        for r in &runs { *dist.entry(n_end(r)).or_default() += 1; }
        let won_and: Vec<String> = endings.iter().map(|e| format!("{e} {}", runs.iter().filter(|r| r.victory.is_some() && r.fired.contains_key(*e)).count())).collect();
        println!("B46 · endings per game (count → games): {dist:?}; victory + ending: {}; victories {}", won_and.join(", "), runs.iter().filter(|r| r.victory.is_some()).count());
        // survived_alone: checked on tick 47 only (since A8 — the state after the siege tick;
        // `ws.tick` 48 after the turn), Byzantium alive, never after a victory
        let sa: Vec<&Run> = runs.iter().filter(|r| r.fired.contains_key("outcome_survived_alone")).collect();
        let sa_bad_tick = sa.iter().filter(|r| r.fired["outcome_survived_alone"].0 != 48).count();
        // Alive at the check: milestones run before collapses in a turn, so a city that falls
        // later in the same turn reads «dead» after it — count those apart.
        let sa_dead = sa.iter().filter(|r| !r.fired["outcome_survived_alone"].1
            && r.byz_fall.map(|f| f.0) != Some(r.fired["outcome_survived_alone"].0)).count();
        let sa_same_turn = sa.iter().filter(|r| r.byz_fall.map(|f| f.0) == Some(r.fired["outcome_survived_alone"].0)).count();
        let sa_after_win = sa.iter().filter(|r| r.victory.is_some_and(|v| v < r.fired["outcome_survived_alone"].0)).count();
        println!("B46 · survived_alone {} (checked off tick 47: {sa_bad_tick}; over a dead city: {sa_dead}; city fell later in the same turn: {sa_same_turn}; after a victory: {sa_after_win})", sa.len());
        // fall endings: on the fall tick or the next, city dead, fell_federation iff federation at the fall ≥ 80
        let mut fall_bad = 0; let mut iff_bad = 0; let mut alive_bad = 0;
        for r in &runs {
            for e in ["outcome_fell_federation", "outcome_historical"] {
                if let Some((t, alive)) = r.fired.get(e) {
                    if *alive { alive_bad += 1; }
                    match r.byz_fall { Some((ft, _, _)) if *t == ft || *t == ft + 1 => {}, _ => fall_bad += 1 }
                }
            }
            if let Some((_, fed, _)) = r.byz_fall {
                let ff = r.fired.contains_key("outcome_fell_federation");
                let ended_on_fall = ff || r.fired.contains_key("outcome_historical");
                if ended_on_fall && ff != (fed >= 80.0) { iff_bad += 1; }
            }
        }
        let fall_endings = runs.iter().filter(|r| r.fired.contains_key("outcome_fell_federation") || r.fired.contains_key("outcome_historical")).count();
        println!("B46 · fall endings {fall_endings} (not on the fall tick or next: {fall_bad}; over a living city: {alive_bad}; fell_federation ≠ (federation at fall ≥ 80): {iff_bad})");
        // constantinople_falls and the mode
        let fallen: Vec<&Run> = runs.iter().filter(|r| r.byz_fall.is_some()).collect();
        let cf_missing = fallen.iter().filter(|r| !r.fired.contains_key("constantinople_falls")).count();
        let not_cons = fallen.iter().filter(|r| r.consequences.is_none()).count();
        let not_cons_won = fallen.iter().filter(|r| r.consequences.is_none() && r.victory.is_some()).count();
        let unfallen_without_ending = runs.iter().filter(|r| r.byz_fall.is_none() && n_end(r) == 0).count();
        println!("B46 · fallen {} — constantinople_falls missing {cf_missing}; mode not Consequences {not_cons} (of them won earlier: {not_cons_won}); never fell and no ending: {unfallen_without_ending}", fallen.len());
        for m in ["final_assault", "mehmed_rises", "mehmed_accelerates"] {
            let f: Vec<&Run> = runs.iter().filter(|r| r.fired.contains_key(m)).collect();
            // as for survived_alone: a city that falls later in the firing turn reads «dead» after it
            let same_turn = |r: &&Run| r.byz_fall.map(|b| b.0) == Some(r.fired[m].0);
            let dead = f.iter().filter(|r| !r.fired[m].1 && !same_turn(r)).count();
            let st = f.iter().filter(|r| same_turn(r)).count();
            println!("B46 · {m}: fired {}, over a dead city {dead}; city fell later in the same turn {st}", f.len());
        }
        let holds: Vec<&Run> = runs.iter().filter(|r| r.fired.contains_key("constantinople_holds")).collect();
        let holds_bad = holds.iter().filter(|r| {
            let h = r.fired["constantinople_holds"];
            let after_assault = r.fired.get("final_assault").is_some_and(|fa| fa.0 <= h.0);
            !(after_assault && h.1)
        }).count();
        println!("B46 · constantinople_holds: fired {} (without a prior final_assault or over a dead city: {holds_bad})\n", holds.len());
    }
}
