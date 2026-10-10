//! The chronicler's memory, measured (owner's decision after the switch to v2, 2026-10-11).
//! Measurement only: the engine and the prompt are not touched.
//!
//! milan_1477 on v2 (as the content stands), with no player and in `aggressive`. The product's path,
//! as `cmd_get_narrative` runs it after every tick: `build_snapshot` → `generate_narrative_prompt` →
//! the live model (`generate_narrative_blocking`, the config of `get_llm_config`) → the game's book
//! (`record_chronicle`). Consecutive chronicles of one game are written to `<outdir>`, each with
//! its prompt and the engine's facts at that moment, so the input can be checked against the world:
//! v2's vassalages, alliances and the league, the player's actions over the whole game against the
//! five in the prompt, the fallen and the milestones.
//!
//! Usage:
//!   chronicle_probe scan <world> <seeds>                  — per seed: when v2's facts appear
//!   chronicle_probe live <world> <seed> <first> <n> <outdir> — n chronicles from tick `first`
//!   chronicle_probe dry  <world> <seed> <first> <n> <outdir> — the same, prompts only

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;
use std::fmt::Write as _;

const SC: &str = "milan_1477";

fn new_game(world: &str, seed: u64) -> (engine13::AppState, engine13::db::Db, Option<ScriptedStrategy>) {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, SC.to_string()).unwrap();
    assert!(st.current_scenario.as_ref().unwrap().features.economy_v2, "milan must play on v2");
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, SC));
    (st, db, strategy)
}

fn advance(st: &mut engine13::AppState, strategy: &Option<ScriptedStrategy>) {
    match strategy {
        Some(s) => { play_scripted_tick(st, s); }
        None => {
            let ws = st.world_state.as_mut().unwrap();
            let scn = st.current_scenario.as_ref().unwrap();
            engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
        }
    }
}

/// The engine's v2 facts and the game's history at this moment, as lines.
fn facts(st: &engine13::AppState) -> Vec<String> {
    let ws = st.world_state.as_ref().unwrap();
    let name = |id: &str| ws.actors.get(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());
    let mut out = Vec::new();
    for v in &ws.vassalages {
        out.push(format!("vassal: {} ({}) → overlord {} ({}), since tick {}", v.vassal_id, name(&v.vassal_id), v.overlord_id, name(&v.overlord_id), v.formed_tick));
    }
    for a in &ws.alliances {
        let mut m = a.actor_ids.clone();
        m.sort();
        out.push(format!("alliance: [{}] against {:?}, since tick {}", m.join(", "), a.common_enemy, a.formed_tick));
    }
    for (loser, winner) in &ws.conquered_by { out.push(format!("conquered: {loser} by {winner}")); }
    for d in &ws.dead_actors { out.push(format!("dead: {} ({}) tick {}", d.id, d.name, d.tick_death)); }
    let acts: Vec<String> = st.event_log.events.iter().filter(|e| matches!(e.event_type, engine13::core::EventType::PlayerAction)).map(|e| format!("{}@{}", e.id, e.tick)).collect();
    out.push(format!("player actions so far: {} — {}", acts.len(), acts.join(", ")));
    let mut ms: Vec<&String> = ws.milestone_events_fired.iter().collect();
    ms.sort();
    out.push(format!("milestones fired: {}", ms.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    let v2ev: Vec<String> = st.event_log.events.iter().filter(|e| e.id.starts_with("vassalage_") || e.id == "italian_league_against_milan" || e.id.starts_with("assault_")).map(|e| format!("{}@{}", e.id, e.tick)).collect();
    out.push(format!("v2 events in the log: {}", v2ev.join(", ")));
    out
}

fn scan(world: &str, seeds: u64) {
    println!("| seed | Florence → Siena (tick) | other vassalages | league turns on Milan | deaths |");
    println!("|---|---|---|---|---|");
    for seed in 0..seeds {
        let (mut st, _db, strategy) = new_game(world, seed);
        let mut vass: Vec<(String, u32)> = Vec::new();
        for _ in 0..300 {
            advance(&mut st, &strategy);
            let ws = st.world_state.as_ref().unwrap();
            for v in &ws.vassalages {
                let k = format!("{}→{}", v.overlord_id, v.vassal_id);
                if !vass.iter().any(|(x, _)| *x == k) { vass.push((k, v.formed_tick)); }
            }
        }
        let league = st.event_log.events.iter().find(|e| e.id == "italian_league_against_milan").map(|e| e.tick);
        let fs = vass.iter().find(|(k, _)| k == "florence→siena").map(|x| x.1);
        let other: Vec<String> = vass.iter().filter(|(k, _)| k != "florence→siena").map(|(k, t)| format!("{k}@{t}")).collect();
        println!("| {seed} | {fs:?} | {} | {league:?} | {} |", other.join(", "), st.world_state.as_ref().unwrap().dead_actors.len());
    }
}

fn chronicles(world: &str, seed: u64, first: u32, n: u32, outdir: &str, live: bool) {
    let (mut st, db, strategy) = new_game(world, seed);
    let cfg = engine13::llm::get_llm_config();
    if live { eprintln!("[chronicle] provider={} model={}", cfg.provider, cfg.model); }
    std::fs::create_dir_all(outdir).unwrap();
    let mut book_md = format!("# milan_1477 / {world} / seed {seed} — chronicles {first}…{}\n\nmodel: {} ({})\n", first + n - 1, cfg.model, cfg.provider);
    for _ in 0..first { advance(&mut st, &strategy); }
    for i in 0..n {
        advance(&mut st, &strategy);
        let (prompt, stamp) = {
            let ws = st.world_state.as_ref().unwrap();
            let scn = st.current_scenario.as_ref().unwrap();
            let snapshot = engine13::llm::build_snapshot(ws, scn, &st.event_log);
            (engine13::llm::generate_narrative_prompt(&snapshot, scn, &db), engine13::llm::ChronicleStamp::of(ws, &snapshot))
        };
        let text = if live {
            match engine13::llm::generate_narrative_blocking(&prompt, &cfg, 5) {
                Ok(t) => Some(t),
                Err(e) => { eprintln!("[chronicle] tick {} FAILED: {e}", stamp.tick); None }
            }
        } else { None };
        let written = engine13::llm::record_chronicle(st.world_state.as_mut().unwrap(), &stamp, text.clone());
        let f = facts(&st);
        let tag = format!("{outdir}/{world}_s{seed}_{i:02}_t{}", stamp.tick);
        std::fs::write(format!("{tag}.prompt.txt"), &prompt).unwrap();
        std::fs::write(format!("{tag}.facts.txt"), f.join("\n")).unwrap();
        if let Some(t) = &text { std::fs::write(format!("{tag}.text.txt"), t).unwrap(); }
        let _ = write!(book_md, "\n---\n\n## tick {} — {} год, {} (in the book: {written})\n\n### facts\n\n{}\n\n### chronicle\n\n{}\n",
            stamp.tick, stamp.year, stamp.half_year, f.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n"), text.as_deref().unwrap_or("(dry)"));
        eprintln!("[chronicle] {world} seed {seed} tick {} done", stamp.tick);
    }
    let book = &st.world_state.as_ref().unwrap().chronicle_book;
    let _ = write!(book_md, "\n---\n\nbook: {} entries\n", book.len());
    std::fs::write(format!("{outdir}/{world}_s{seed}.md"), book_md).unwrap();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("scan") => scan(&a[2], a[3].parse().unwrap()),
        Some(m @ ("live" | "dry")) => chronicles(&a[2], a[3].parse().unwrap(), a[4].parse().unwrap(), a[5].parse().unwrap(), &a[6], m == "live"),
        _ => eprintln!("usage: chronicle_probe scan|live|dry …"),
    }
}
