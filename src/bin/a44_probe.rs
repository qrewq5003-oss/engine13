//! A44 stage 1 — the eras' authored fields the engine does not read (docs/TRIAGE.md).
//! Measurement only; nothing is chosen.
//!
//! `EraDefinition.unlocks_tags` and `auto_delta_modifier` are authored in all three scenarios
//! and read by nothing. Built with `--features census`, the probe connects them in memory
//! (`census::set_era_counterfactual`):
//!
//! - (а) `unlocks_tags` **granted** to an actor on the tick it enters the era. The content
//!   says so: in rome each era's `from_tags` lists the previous era's unlocks (`heavy_cavalry`,
//!   `feudalism` → high_medieval; `crossbow`, `guilds`, `banking` → late_medieval; …), a chain
//!   that only works if entering an era hands its tags over. «Available to spread» would do
//!   nothing: no tag is defined for any unlocked id in its scenario, and an undefined tag never
//!   spreads (`interactions.rs`, «No definition = no spreading»); no tag uses `requires_era`;
//! - (б) `auto_delta_modifier` scales every auto-delta whose target is an actor by the modifier
//!   of that actor's current era (the whole applied delta, noise included); an era without the
//!   field (serde default 0) means 1. Family and global targets have no era and are left alone;
//! - (в) both.
//!
//! Per scenario, world and variant: the eras actually entered (actors, tick p10/50/90), deaths
//! (all, and of the key actors), rome's split on tick 40, the A10 row (constantinople wins,
//! tick p10/50/90, on 40–43, wins with the Ottomans alive), the A35 row (rome wins, tick
//! p10/50/90, largest share on one tick), the B46 outcomes, milan's metrics at the end of
//! the aggressive world, and the share of living actor-ticks at the ceiling (≥ 99) for
//! `economic_output` and `external_pressure`.
//!
//! Usage: cargo run --release --features census --bin a44_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

fn key_actors(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["rome"],
        "milan_1477" => &["milan"],
        _ => &["byzantium", "ottomans"],
    }
}

const VARIANTS: &[(&str, bool, bool)] = &[("base", false, false), ("(а) unlocks", true, false), ("(б) modifier", false, true), ("(в) both", true, true)];
const MILAN_METRICS: &[&str] = &["legitimacy", "cohesion", "economic_output", "external_pressure", "military_size", "treasury"];

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
    // era name -> ticks of entry (one per actor)
    eras: BTreeMap<String, Vec<u32>>,
    deaths: u32,
    key_deaths: BTreeMap<String, u32>,
    split40: bool,
    win: Option<u32>,
    win_ottomans_alive: bool,
    outcomes: Vec<String>,
    milan_end: Vec<f64>,
    ceil: [(u64, u64); 2],
}

fn run(sc: &str, world: &str, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut era_of: BTreeMap<String, String> = BTreeMap::new();
    for (id, a) in &st.world_state.as_ref().unwrap().actors {
        era_of.insert(id.clone(), format!("{:?}", a.era));
    }
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
        let t = ws.tick - 1;
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            let a = &ws.actors[id];
            let e = format!("{:?}", a.era);
            match era_of.get(id) {
                Some(old) if *old == e => {}
                Some(_) => { r.eras.entry(e.clone()).or_default().push(t); era_of.insert(id.clone(), e); }
                None => { era_of.insert(id.clone(), e); }
            }
            for (i, m) in ["economic_output", "external_pressure"].iter().enumerate() {
                r.ceil[i].1 += 1;
                if a.get_metric(m) >= 99.0 { r.ceil[i].0 += 1; }
            }
        }
        if ws.milestone_events_fired.iter().any(|m| m == "rome_splits") && !r.split40 && t == 40 {
            r.split40 = true;
        }
        if r.win.is_none() && ws.victory_achieved {
            r.win = Some(t);
            r.win_ottomans_alive = ws.actors.contains_key("ottomans") && !ws.dead_actor_ids.contains("ottomans");
        }
    }
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    for k in key_actors(sc) {
        if ws.dead_actor_ids.contains(*k) { r.key_deaths.insert(k.to_string(), 1); }
    }
    r.outcomes = ws.milestone_events_fired.iter().filter(|m| m.starts_with("outcome_")).cloned().collect();
    if let Some(m) = ws.actors.get("milan") {
        r.milan_end = MILAN_METRICS.iter().map(|k| m.get_metric(k)).collect();
    }
    r
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("# A44 stage 1 — eras' `unlocks_tags` and `auto_delta_modifier`, {seeds} seeds × {ticks} ticks per world\n");

    println!("## 1. As authored\n");
    println!("| scenario | era | min_tick | requires_tags of from_tags | auto_delta_modifier | unlocks_tags | unlocked ids defined as tags in the scenario |");
    println!("|---|---|---|---|---|---|---|");
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let s = engine13::scenarios::registry::load_by_id(sc).unwrap();
        for e in &s.era_definitions {
            let defined: Vec<&String> = e.unlocks_tags.iter().filter(|t| s.tag_definitions.iter().any(|d| &d.id == *t)).collect();
            println!("| {sc} | {:?} | {} | {} of {:?} | {} | {:?} | {:?} |", e.era, e.min_tick, e.requires_tags, e.from_tags, e.auto_delta_modifier, e.unlocks_tags, defined);
        }
    }

    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        println!("\n## {sc}\n");
        println!("| world | variant | eras entered: actors (tick p10/50/90) | deaths | key deaths | split on 40 | wins (tick p10/50/90) | wins: on 40–43 / largest share on one tick / Ottomans alive | outcomes | eo / ep at ceiling |");
        println!("|---|---|---|---|---|---|---|---|---|---|");
        let mut milan_rows = Vec::new();
        for world in worlds(sc) {
            for (label, unlocks, modifier) in VARIANTS {
                census::set_era_counterfactual(*unlocks, *modifier);
                let runs: Vec<Run> = (0..seeds).map(|s| run(sc, world, s, ticks)).collect();
                let mut eras: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for r in &runs {
                    for (e, ts) in &r.eras {
                        eras.entry(e.clone()).or_default().extend(ts.iter().map(|t| *t as f64));
                    }
                }
                let eras_s: Vec<String> = eras.iter().map(|(e, ts)| format!("{e} {} ({})", ts.len(), q(ts))).collect();
                let deaths: u32 = runs.iter().map(|r| r.deaths).sum();
                let keys: Vec<String> = key_actors(sc).iter().map(|k| format!("{k} {}", runs.iter().filter(|r| r.key_deaths.contains_key(*k)).count())).collect();
                let split = runs.iter().filter(|r| r.split40).count();
                let wins: Vec<f64> = runs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
                let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let mut per: BTreeMap<u32, u32> = BTreeMap::new();
                for t in &wins { *per.entry(*t as u32).or_default() += 1; }
                let top = per.values().max().copied().unwrap_or(0);
                let ott = runs.iter().filter(|r| r.win.is_some() && r.win_ottomans_alive).count();
                let mut outc: BTreeMap<String, u32> = BTreeMap::new();
                for r in &runs { for o in &r.outcomes { *outc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                let outc_s: Vec<String> = outc.iter().map(|(k, v)| format!("{k} {v}")).collect();
                let (ce, ne) = runs.iter().fold((0, 0), |a, r| (a.0 + r.ceil[0].0, a.1 + r.ceil[0].1));
                let (cp, np) = runs.iter().fold((0, 0), |a, r| (a.0 + r.ceil[1].0, a.1 + r.ceil[1].1));
                let share = if wins.is_empty() { "—".to_string() } else { format!("{:.0} %", 100.0 * top as f64 / wins.len() as f64) };
                println!("| {world} | {label} | {} | {deaths} | {} | {split} | {} ({}) | {on} / {share} / {ott} | {} | {:.1} % / {:.1} % |",
                    if eras_s.is_empty() { "—".into() } else { eras_s.join(", ") }, keys.join(", "), wins.len(), q(&wins),
                    if outc_s.is_empty() { "—".into() } else { outc_s.join(", ") },
                    100.0 * ce as f64 / ne.max(1) as f64, 100.0 * cp as f64 / np.max(1) as f64);
                if sc == "milan_1477" && *world == "aggressive" {
                    let alive: Vec<&Run> = runs.iter().filter(|r| !r.milan_end.is_empty()).collect();
                    let cells: Vec<String> = (0..MILAN_METRICS.len()).map(|i| q(&alive.iter().map(|r| r.milan_end[i]).collect::<Vec<_>>())).collect();
                    milan_rows.push(format!("| {label} | {} / {seeds} | {} |", alive.len(), cells.join(" | ")));
                }
            }
        }
        if sc == "milan_1477" {
            println!("\n### milan, aggressive: Milan's metrics on the last tick, p10/50/90 (Milan alive)\n");
            println!("| variant | Milan alive | {} |", MILAN_METRICS.join(" | "));
            println!("|---|---|{}", "---|".repeat(MILAN_METRICS.len()));
            for row in milan_rows { println!("{row}"); }
        }
    }
    census::set_era_counterfactual(false, false);
}
