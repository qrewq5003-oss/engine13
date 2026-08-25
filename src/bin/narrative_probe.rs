//! Narrative probe — задача 31.
//!
//! Инструмент ОЦЕНКИ, не правка. Он не трогает движок и не меняет контент:
//! он воспроизводит ровно тот путь, по которому игрок получает текст в UI:
//!
//!     advance tick  ->  build_snapshot(world, scenario, event_log)
//!                   ->  generate_narrative_prompt(snapshot, scenario, db, memory)
//!                   ->  POST /v1/chat/completions
//!
//! (см. `application::narrative::cmd_get_narrative` и `App.tsx::handleAdvanceTick`,
//! который зовёт `refreshNarrative` КАЖДЫЙ тик).
//!
//! Режимы:
//!   dry   — полная партия, собрать промпты по всем тикам, без обращения к LLM
//!   live  — то же + сгенерировать текст на лестнице сэмплов (пары соседних полугодий)
//!
//! Usage:
//!   cargo run --release --bin narrative_probe -- <scenario> <ticks> <strategy> <seed> <dry|live> [outdir]

use engine13::{
    application::actions::{apply_player_action, PlayerActionInput},
    commands::AppState,
    core::WorldState,
    engine::{tick, EventLog},
    scenarios::registry,
};
use rand::SeedableRng;

// ---------------------------------------------------------------------------
// Scripted strategies — копия приоритетов из src/bin/sim.rs, чтобы промпт
// строился на партии, в которой игрок реально что-то делает.
// ---------------------------------------------------------------------------
fn priority_actions(scenario_id: &str, strategy: &str) -> Vec<&'static str> {
    match (scenario_id, strategy) {
        ("constantinople_1430", "diplomacy") => vec![
            "venice_diplomacy", "genoa_financial_aid", "milan_bankers",
            "venice_trade_deal", "genoa_galata_garrison", "venice_naval_support",
            "genoa_mercenaries", "milan_condottieri",
        ],
        ("constantinople_1430", "military") => vec![
            "venice_naval_support", "genoa_mercenaries", "milan_condottieri",
            "genoa_galata_garrison", "venice_diplomacy", "genoa_financial_aid",
            "milan_bankers", "venice_trade_deal",
        ],
        ("constantinople_1430", _) => vec![
            "venice_diplomacy", "genoa_financial_aid", "milan_bankers",
            "venice_naval_support", "genoa_mercenaries", "milan_condottieri",
            "venice_trade_deal", "genoa_galata_garrison",
        ],
        ("rome_375", "influence") => vec![
            "build_reputation", "expand_network", "back_administration",
            "gather_information", "invest_wealth", "lay_low",
        ],
        ("rome_375", "wealth") => vec![
            "invest_wealth", "gather_information", "expand_network",
            "build_reputation", "back_administration", "lay_low",
        ],
        ("rome_375", _) => vec![
            "build_reputation", "invest_wealth", "expand_network",
            "gather_information", "back_administration", "lay_low",
        ],
        ("milan_1477", _) => vec![
            "milan_raise_troops", "milan_marriage_alliance", "milan_papal_favor",
            "milan_hire_condottieri", "milan_tax_reform", "milan_patronage",
        ],
        _ => vec![],
    }
}

struct TurnRecord {
    tick: u32,
    year: i32,
    half_year: String,
    victory: bool,
    prompt: String,
    actions: Vec<String>,
    foreground: Vec<String>,
    dead: Vec<String>,
    events_shown: Vec<String>,
    narrative: Option<String>,
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let scenario_id = a.get(1).cloned().unwrap_or_else(|| "rome_375".into());
    let ticks: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(150);
    let strategy = a.get(3).cloned().unwrap_or_else(|| "balanced".into());
    let seed: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(42);
    let mode = a.get(5).cloned().unwrap_or_else(|| "dry".into());
    let outdir = a.get(6).cloned().unwrap_or_else(|| "/tmp/narrative_probe".into());

    std::fs::create_dir_all(&outdir).expect("mkdir outdir");

    let scenario = registry::load_by_id(&scenario_id).expect("Unknown scenario");
    let db = engine13::db::Db::open_in_memory().expect("in-memory db");

    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario
            .generation_mechanics
            .as_ref()
            .map(|g| g.patriarch_start_age)
            .unwrap_or(40) as u32;
        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
        });
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let mut state = AppState {
        world_state: Some(world),
        event_log: EventLog::new(),
        current_scenario: Some(scenario.clone()),
        rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
        narrative_memory: engine13::llm::NarrativeMemory::default(),
    };

    let prio = priority_actions(&scenario_id, &strategy);
    let mut records: Vec<TurnRecord> = Vec::new();
    let mut victory_tick: Option<u32> = None;

    for tick_num in 0..ticks {
        // --- player actions, тем же путём, что и UI ---
        let mut applied = 0u32;
        let mut actions_applied: Vec<String> = Vec::new();

        if scenario_id == "milan_1477" {
            const GATE: f64 = 70.0;
            let treasury = state.world_state.as_ref().unwrap()
                .actors.get("milan").map(|a| a.get_metric("treasury")).unwrap_or(0.0);
            if treasury > GATE {
                let input = PlayerActionInput { action_id: "milan_raise_troops".into(), target_actor_id: None };
                if apply_player_action(&mut state, &input).is_ok() {
                    applied += 1;
                    actions_applied.push("milan_raise_troops".into());
                }
            }
            for id in prio.iter().filter(|i| **i != "milan_raise_troops") {
                if applied >= scenario.actions_per_tick { break; }
                let now = state.world_state.as_ref().unwrap()
                    .actors.get("milan").map(|a| a.get_metric("treasury")).unwrap_or(0.0);
                if now - GATE <= 0.0 { break; }
                let input = PlayerActionInput { action_id: id.to_string(), target_actor_id: None };
                if apply_player_action(&mut state, &input).is_ok() {
                    applied += 1;
                    actions_applied.push(id.to_string());
                }
            }
        } else {
            for id in &prio {
                if applied >= scenario.actions_per_tick { break; }
                let input = PlayerActionInput { action_id: id.to_string(), target_actor_id: None };
                if apply_player_action(&mut state, &input).is_ok() {
                    applied += 1;
                    actions_applied.push(id.to_string());
                }
            }
        }

        // --- advance tick ---
        {
            let ws = state.world_state.as_mut().unwrap();
            let sc = state.current_scenario.as_ref().unwrap();
            let rng = state.rng.as_mut().unwrap();
            tick(ws, sc, &mut state.event_log, rng);
        }

        // --- ровно то, что делает cmd_get_narrative ---
        let ws = state.world_state.as_ref().unwrap();
        let snapshot = engine13::llm::build_snapshot(ws, &scenario, &state.event_log);
        let prompt = engine13::llm::generate_narrative_prompt(
            &snapshot, &scenario, &db, &state.narrative_memory,
        );

        if snapshot.victory_achieved && victory_tick.is_none() {
            victory_tick = Some(tick_num);
        }

        records.push(TurnRecord {
            tick: tick_num,
            year: snapshot.year,
            half_year: format!("{:?}", snapshot.half_year),
            victory: snapshot.victory_achieved,
            prompt,
            actions: actions_applied,
            foreground: snapshot.foreground_actors.clone(),
            dead: snapshot.dead_actors.clone(),
            events_shown: snapshot.recent_important_events.iter().take(5)
                .map(|e| e.id.clone()).collect(),
            narrative: None,
        });
    }

    // ------------------------------------------------------------------
    // Model-free измерения по промптам (LLM не нужен)
    // ------------------------------------------------------------------
    let mut identical_pairs = 0usize;
    let mut diff_bytes: Vec<usize> = Vec::new();
    for w in records.windows(2) {
        if w[0].prompt == w[1].prompt { identical_pairs += 1; }
        diff_bytes.push(byte_diff(&w[0].prompt, &w[1].prompt));
    }
    let events_frozen_from = records.iter().position(|r| r.events_shown.len() >= 5);
    let distinct_event_sets: std::collections::HashSet<String> = records.iter()
        .map(|r| r.events_shown.join("|")).collect();

    println!("=== NARRATIVE PROBE: {} / {} / seed {} ===", scenario_id, strategy, seed);
    println!("ticks: {}", records.len());
    println!("victory tick: {:?}", victory_tick);
    println!("identical adjacent prompts: {}/{}", identical_pairs, records.len().saturating_sub(1));
    if !diff_bytes.is_empty() {
        let avg = diff_bytes.iter().sum::<usize>() as f64 / diff_bytes.len() as f64;
        let maxd = diff_bytes.iter().max().unwrap();
        let avg_len = records.iter().map(|r| r.prompt.len()).sum::<usize>() as f64 / records.len() as f64;
        println!("prompt len avg: {:.0} bytes", avg_len);
        println!("adjacent prompt diff: avg {:.1} bytes, max {} bytes ({:.2}% of prompt)",
            avg, maxd, avg / avg_len * 100.0);
    }
    println!("distinct 'НЕДАВНИЕ СОБЫТИЯ' sets over the whole game: {}", distinct_event_sets.len());
    println!("first tick with 5 events shown: {:?}", events_frozen_from);
    let last = records.last().unwrap();
    println!("last tick events shown: {:?}", last.events_shown);
    println!("first tick events shown: {:?}", records[0].events_shown);

    // ------------------------------------------------------------------
    // live: генерация текста на лестнице сэмплов (соседние пары)
    // ------------------------------------------------------------------
    if mode == "live" {
        let n = records.len() as u32;
        let mut samples: Vec<u32> = vec![0, 1, 5, 6, 20, 21];
        if n > 52 { samples.extend_from_slice(&[50, 51]); }
        if n > 102 { samples.extend_from_slice(&[100, 101]); }
        if let Some(vt) = victory_tick {
            if vt >= 1 { samples.push(vt - 1); }
            samples.push(vt);
        } else if n >= 2 {
            samples.push(n - 2);
            samples.push(n - 1);
        }
        samples.retain(|t| *t < n);
        samples.sort_unstable();
        samples.dedup();

        let cfg = engine13::llm::get_llm_config();
        eprintln!("[live] provider={} model={} samples={:?}", cfg.provider, cfg.model, samples);

        for t in &samples {
            let idx = *t as usize;
            let prompt = records[idx].prompt.clone();
            // Ретраи и сам запрос живут в `llm::generate_narrative_blocking` —
            // общий путь с `sim`'s narrative_eval / narrative_pack. Своей копии
            // запроса у пробника больше нет.
            match engine13::llm::generate_narrative_blocking(&prompt, &cfg, 5) {
                Ok(text) => {
                    eprintln!("[live] tick {} -> {} chars", t, text.chars().count());
                    records[idx].narrative = Some(text);
                }
                Err(e) => {
                    eprintln!("[live] tick {} FAILED: {}", t, e);
                    records[idx].narrative = Some(format!("[LLM ERROR] {}", e));
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // dump
    // ------------------------------------------------------------------
    let base = format!("{}/{}__{}__seed{}", outdir, scenario_id, strategy, seed);

    let mut md = String::new();
    md.push_str(&format!("# narrative probe: {} / {} / seed {}\n\n", scenario_id, strategy, seed));
    md.push_str(&format!("- ticks: {}\n- victory tick: {:?}\n- identical adjacent prompts: {}/{}\n\n",
        records.len(), victory_tick, identical_pairs, records.len().saturating_sub(1)));
    for r in &records {
        if r.narrative.is_none() { continue; }
        md.push_str(&format!("\n---\n\n## tick {} — {} год, {}\n\n", r.tick, r.year, r.half_year));
        md.push_str(&format!("- victory_achieved: {}\n", r.victory));
        md.push_str(&format!("- действия игрока в этом тике: {:?}\n", r.actions));
        md.push_str(&format!("- foreground: {:?}\n", r.foreground));
        md.push_str(&format!("- павшие: {:?}\n", r.dead));
        md.push_str(&format!("- события в промпте: {:?}\n\n", r.events_shown));
        md.push_str("### narrative\n\n");
        md.push_str(r.narrative.as_ref().unwrap());
        md.push('\n');
    }
    std::fs::write(format!("{}.narrative.md", base), md).expect("write md");

    // все промпты — для сверки и диффов
    let mut pd = String::new();
    for r in &records {
        pd.push_str(&format!("\n\n########## TICK {} (year {}, {}) ##########\n", r.tick, r.year, r.half_year));
        pd.push_str(&r.prompt);
    }
    std::fs::write(format!("{}.prompts.txt", base), pd).expect("write prompts");

    // машинно-читаемая сводка
    let mut csv = String::from("tick,year,half_year,victory,n_actions,n_foreground,n_dead,events\n");
    for r in &records {
        csv.push_str(&format!("{},{},{},{},{},{},{},\"{}\"\n",
            r.tick, r.year, r.half_year, r.victory, r.actions.len(),
            r.foreground.len(), r.dead.len(), r.events_shown.join("|")));
    }
    std::fs::write(format!("{}.turns.csv", base), csv).expect("write csv");

    println!("written: {}.narrative.md / .prompts.txt / .turns.csv", base);
}

/// грубая мера различия: число байт, не совпавших при позиционном сравнении
fn byte_diff(a: &str, b: &str) -> usize {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    let common = ab.len().min(bb.len());
    let mut d = ab.len().max(bb.len()) - common;
    for i in 0..common {
        if ab[i] != bb[i] { d += 1; }
    }
    d
}

