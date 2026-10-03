//! B48 — `successor_state`: a tag five rome heirs carry and no tag file defines.
//!
//! The shared-tag count (`interactions.rs`, `calculate_diplomatic_interaction` /
//! `calculate_cultural_interaction`) reads `actor.tags` and pays cohesion for a tag two
//! actors share — so the undefined tag earns a bonus while having no definition. Two
//! counterfactuals in memory, against the scenario as it is:
//!
//! - (a) the tag removed from the five templates (`visigoth_kingdom`, `ostrogoth_kingdom`,
//!   `late_sassanids`, `vandal_kingdom_africa`, `frankish_kingdom`);
//! - (b) the tag defined with no modifier and no spreading — it takes part only in the
//!   shared-tag count. This is what is in effect today, so (b) must match the scenario
//!   **byte for byte**; a difference would mean the undefined tag is read somewhere else.
//!
//! Per rome world: Rome's deaths, the split (tick, count), the family's victories, the
//! share of Rome's living ticks at the pressure ceiling; and for (b), how many runs are
//! identical to the base (a hash of every actor's metrics and tags, every tick).
//!
//! Usage: cargo run --release --bin b48_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;
use std::hash::{Hash, Hasher};

const HEIRS: &[&str] = &["visigoth_kingdom", "ostrogoth_kingdom", "late_sassanids", "vandal_kingdom_africa", "frankish_kingdom"];

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

struct Run { rome_death: Option<u32>, split: Option<u32>, win: Option<u32>, at_ceiling: (u64, u64), hash: u64, metric_hash: u64, heirs_with_tag: u32 }

fn run(world: &str, seed: u64, ticks: u32, variant: char) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, "rome_375".into()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let sc = st.current_scenario.as_mut().unwrap();
        match variant {
            'a' => {
                for a in sc.actors.iter_mut().filter(|a| HEIRS.contains(&a.id.as_str())) {
                    a.tags.retain(|t| t != "successor_state");
                }
            }
            'b' => {
                let def: engine13::core::TagDefinition =
                    toml::from_str("id = \"successor_state\"\nmetrics_modifier = {}\nspreads_via = []").unwrap();
                sc.tag_definitions.push(def);
            }
            _ => {}
        }
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, "rome_375"));
    let mut r = Run { rome_death: None, split: None, win: None, at_ceiling: (0, 0), hash: 0, metric_hash: 0, heirs_with_tag: 0 };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // metrics only: (a) changes the tag lists by construction, the question is the world
    let mut hm = std::collections::hash_map::DefaultHasher::new();
    for _ in 0..ticks {
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let sc = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            let a = &ws.actors[id];
            id.hash(&mut h);
            let mut m: Vec<(&String, &f64)> = a.metrics.iter().collect();
            m.sort_by(|x, y| x.0.cmp(y.0));
            id.hash(&mut hm);
            for (k, v) in m { k.hash(&mut h); v.to_bits().hash(&mut h); k.hash(&mut hm); v.to_bits().hash(&mut hm); }
            let mut t = a.tags.clone();
            t.sort();
            t.hash(&mut h);
        }
        if r.split.is_none() && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split = Some(ws.tick - 1); }
        if r.rome_death.is_none() && ws.dead_actor_ids.contains("rome") { r.rome_death = Some(ws.tick - 1); }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(ws.tick - 1); }
        if let Some(rome) = ws.actors.get("rome") {
            r.at_ceiling.1 += 1;
            if rome.get_metric("external_pressure") >= 100.0 { r.at_ceiling.0 += 1; }
        }
    }
    let ws = st.world_state.as_ref().unwrap();
    r.heirs_with_tag = ws.actors.values().filter(|a| a.tags.iter().any(|t| t == "successor_state")).count() as u32;
    r.hash = h.finish();
    r.metric_hash = hm.finish();
    r
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("# B48: rome_375, {seeds} seeds × {ticks} ticks\n");
    println!("| world | variant | Rome dies (tick p10/50/90) | split on tick 40 | family victories (tick p10/50/90) | Rome at ceiling | heirs carrying the tag at the end | runs identical to base: all state / metrics only |");
    println!("|---|---|---|---|---|---|---|---|");
    for world in ["none", "balanced", "influence", "wealth"] {
        let base: Vec<Run> = (0..seeds).map(|s| run(world, s, ticks, 'x')).collect();
        for (v, label) in [('x', "base"), ('a', "(a) removed"), ('b', "(b) defined, no modifier")] {
            let rs: Vec<Run> = if v == 'x' { (0..seeds).map(|s| run(world, s, ticks, 'x')).collect() } else { (0..seeds).map(|s| run(world, s, ticks, v)).collect() };
            let deaths: Vec<f64> = rs.iter().filter_map(|r| r.rome_death.map(|t| t as f64)).collect();
            let split40 = rs.iter().filter(|r| r.split == Some(40)).count();
            let wins: Vec<f64> = rs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
            let (c, n) = rs.iter().fold((0, 0), |acc, r| (acc.0 + r.at_ceiling.0, acc.1 + r.at_ceiling.1));
            let tagged: u32 = rs.iter().map(|r| r.heirs_with_tag).sum();
            let same = rs.iter().zip(&base).filter(|(a, b)| a.hash == b.hash).count();
            let same_m = rs.iter().zip(&base).filter(|(a, b)| a.metric_hash == b.metric_hash).count();
            println!("| {world} | {label} | {} ({}) | {split40} / {seeds} | {} ({}) | {:.1} % | {tagged} | {same} / {same_m} of {seeds} |",
                deaths.len(), q(&deaths), wins.len(), q(&wins), 100.0 * c as f64 / n.max(1) as f64);
        }
    }
}
