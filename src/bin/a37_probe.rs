//! A37 stage 1 — what holds `external_pressure` at the ceiling, and what the siege-rally
//! rule does to Byzantium (docs/TRIAGE.md). Measurement only.
//!
//! Built with `--features census`. Every write to an actor's `external_pressure` or
//! `cohesion` is recorded where it happens (`core::census` write sink): call site, the
//! source the writing site names (auto-delta, dependency rule, tag, event, action), the
//! delta asked for and the delta that landed after any clamp. Found by enumeration of
//! writes, not by listing suspects — and checked: each tick the landed deltas of every
//! watched actor must sum to the tick's change, or the decomposition is incomplete.
//!
//! 1. Pressure by source — Byzantium, Serbia, Trebizond, Hungary (opponent: the Ottomans)
//!    and Rome (opponent: the Huns), before and after the opponent dies; asked and landed.
//!    Positive control: auto-delta [2] of Byzantium (base +2.125) must be found.
//! 2. Rally — Byzantium's cohesion by source per living tick, and the share of living
//!    ticks with pressure above 69 (where pressure is, net, good for the city).
//! 3. Counterfactuals in memory, one at a time: (a) the rally rule removed; (b) the rule
//!    reads `min(pressure, 80)`; plus (c) if `c` is passed: tags' `external_pressure`
//!    modifiers removed. Per world: Byzantium's falls by the two B46 classes, Ottoman and
//!    Rome deaths, the A10 row, the B46 endings, the share of living ticks at the ceiling.
//!
//! Usage: cargo run --release --features census --bin a37_probe -- [seeds] [ticks] [decompose|cf]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap};

const RALLY: &str = "siege_rally_cohesion_bonus";

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

fn new_state(sc: &str, seed: u64) -> engine13::AppState {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    st
}

fn step(st: &mut engine13::AppState, strategy: &Option<ScriptedStrategy>) {
    match strategy {
        Some(s) => { play_scripted_tick(st, s); }
        None => {
            let ws = st.world_state.as_mut().unwrap();
            let sc = st.current_scenario.as_ref().unwrap();
            engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
        }
    }
}

fn worlds(sc: &str) -> &'static [&'static str] {
    if sc == "rome_375" { &["none", "balanced", "influence", "wealth"] } else { &["none", "balanced", "diplomacy", "military"] }
}

fn decompose(seeds: u64, ticks: u32) {
    let mut positive = false;
    let mut mismatches = 0u64;
    let mut checked = 0u64;
    let mut worst = 0.0f64;
    for sc in ["constantinople_1430", "rome_375"] {
        let (targets, opponent): (&[&str], &str) = if sc == "rome_375" { (&["rome"], "huns") } else { (&["byzantium", "serbia", "trebizond", "hungary"], "ottomans") };
        for world in worlds(sc) {
            // (actor, phase, source) -> (asked, landed); phase: before/after the opponent dies
            let mut ep: BTreeMap<(String, &str, String), (f64, f64)> = BTreeMap::new();
            let mut ep_ticks: BTreeMap<(String, &str), u64> = BTreeMap::new();
            let mut coh: BTreeMap<String, (f64, f64)> = BTreeMap::new();
            let (mut byz_ticks, mut byz_over69, mut byz_at100) = (0u64, 0u64, 0u64);
            for seed in 0..seeds {
                let mut st = new_state(sc, seed);
                let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, sc));
                census::enable_writes();
                let _ = census::take_writes();
                for _ in 0..ticks {
                    let before: HashMap<String, (f64, f64)> = st.world_state.as_ref().unwrap().actors.iter()
                        .map(|(id, a)| (id.clone(), (a.get_metric("external_pressure"), a.get_metric("cohesion")))).collect();
                    let opp_dead = st.world_state.as_ref().unwrap().dead_actor_ids.contains(opponent);
                    step(&mut st, &strategy);
                    let writes = census::take_writes();
                    let ws = st.world_state.as_ref().unwrap();
                    // completeness: landed deltas sum to the change, for every actor alive throughout
                    let mut landed: HashMap<(&str, &str), f64> = HashMap::new();
                    for w in &writes { *landed.entry((w.actor.as_str(), w.metric.as_str())).or_default() += w.applied; }
                    for (id, (e0, c0)) in &before {
                        let Some(a) = ws.actors.get(id) else { continue };
                        for (m, v0) in [("external_pressure", *e0), ("cohesion", *c0)] {
                            let d = a.get_metric(m) - v0;
                            let s = landed.get(&(id.as_str(), m)).copied().unwrap_or(0.0);
                            checked += 1;
                            let err = (d - s).abs();
                            worst = worst.max(err);
                            if err > 1e-6 { mismatches += 1; }
                        }
                    }
                    let phase = if opp_dead { "after" } else { "before" };
                    for t in targets.iter() {
                        if before.contains_key(*t) && ws.actors.contains_key(*t) {
                            *ep_ticks.entry((t.to_string(), phase)).or_default() += 1;
                        }
                    }
                    for w in &writes {
                        if !targets.contains(&w.actor.as_str()) || !before.contains_key(&w.actor) { continue; }
                        let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file().trim_start_matches("src/"), w.location.line()));
                        if w.metric == "external_pressure" {
                            let e = ep.entry((w.actor.clone(), phase, src.clone())).or_default();
                            e.0 += w.requested; e.1 += w.applied;
                            if sc == "constantinople_1430" && w.actor == "byzantium" && src.starts_with("auto_delta[2] ") { positive = true; }
                        } else if w.actor == "byzantium" {
                            let e = coh.entry(src).or_default();
                            e.0 += w.requested; e.1 += w.applied;
                        }
                    }
                    if let Some(b) = ws.actors.get("byzantium") {
                        byz_ticks += 1;
                        let p = b.get_metric("external_pressure");
                        if p > 69.0 { byz_over69 += 1; }
                        if p >= 100.0 { byz_at100 += 1; }
                    }
                }
            }
            println!("## {sc} / {world} — external_pressure by source (Σ asked · Σ landed · share of the positive asked)\n");
            let mut keys: Vec<(String, &str)> = ep_ticks.keys().cloned().collect();
            keys.sort();
            for (actor, phase) in keys {
                let n = ep_ticks[&(actor.clone(), phase)];
                let rows: Vec<(&String, &(f64, f64))> = ep.iter().filter(|((a, p, _), _)| *a == actor && *p == phase).map(|((_, _, s), v)| (s, v)).collect();
                let pos: f64 = rows.iter().map(|(_, v)| v.0.max(0.0)).sum();
                let net_asked: f64 = rows.iter().map(|(_, v)| v.0).sum();
                let net_landed: f64 = rows.iter().map(|(_, v)| v.1).sum();
                println!("### {actor}, {phase} {opponent} die — {n} living ticks; per tick: asked {:+.2}, landed {:+.3}", net_asked / n as f64, net_landed / n as f64);
                let mut rows = rows;
                rows.sort_by(|a, b| b.1.0.abs().partial_cmp(&a.1.0.abs()).unwrap());
                for (s, (asked, landed)) in rows.iter().take(12) {
                    let share = if *asked > 0.0 && pos > 0.0 { format!("{:.1} %", 100.0 * asked / pos) } else { "—".into() };
                    println!("  {s:<70} {:>+10.1} · {:>+9.1} · {share}  ({:+.3}/tick)", asked, landed, asked / n as f64);
                }
            }
            if sc == "constantinople_1430" {
                println!("\n### byzantium cohesion by source (per living tick: asked · landed); living ticks {byz_ticks}, pressure > 69 in {:.1} %, at 100 in {:.1} %",
                    100.0 * byz_over69 as f64 / byz_ticks.max(1) as f64, 100.0 * byz_at100 as f64 / byz_ticks.max(1) as f64);
                let mut rows: Vec<(&String, &(f64, f64))> = coh.iter().collect();
                rows.sort_by(|a, b| b.1.0.abs().partial_cmp(&a.1.0.abs()).unwrap());
                for (s, (asked, landed)) in rows.iter().take(14) {
                    println!("  {s:<70} {:>+8.3} · {:>+8.3}", asked / byz_ticks.max(1) as f64, landed / byz_ticks.max(1) as f64);
                }
            }
            println!();
        }
    }
    println!("completeness: {mismatches} of {checked} actor-metric-ticks where landed writes ≠ change (worst {worst:.2e})");
    println!("positive control (auto_delta[2] of byzantium found): {}", if positive { "FOUND" } else { "MISSING — decomposition broken" });
}

#[derive(Clone, Copy, PartialEq)]
enum Cf { Base, NoRally, Cap80, NoTagEp }

fn counterfactuals(seeds: u64, ticks: u32, with_c: bool) {
    let mut cfs = vec![(Cf::Base, "base"), (Cf::NoRally, "(a) rule removed"), (Cf::Cap80, "(b) min(pressure, 80)")];
    if with_c { cfs.push((Cf::NoTagEp, "(c) tags' pressure modifiers removed")); }
    println!("| scenario | world | variant | Byzantium falls: under pressure · after Ottomans · never | Ottomans die | Rome dies | victories, tick p10/50/90, on 40–43 | endings: held · fell+fed · fell · none | at ceiling (share of living ticks) |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for sc in ["constantinople_1430", "rome_375"] {
        for world in worlds(sc) {
            for (cf, label) in &cfs {
                let (mut under, mut after, mut never, mut ott, mut rome) = (0, 0, 0, 0, 0);
                let mut wins = Vec::new();
                let (mut held, mut ff, mut fell, mut none) = (0, 0, 0, 0);
                let mut ceil: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
                // A37 stage 2: the ceiling after the Ottomans died, for the three the tied tags held.
                let mut ceil_after: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
                for seed in 0..seeds {
                    let mut st = new_state(sc, seed);
                    {
                        let s = st.current_scenario.as_mut().unwrap();
                        match cf {
                            Cf::NoRally => s.dependencies.retain(|d| d.id != RALLY),
                            Cf::NoTagEp => {
                                for t in s.tag_definitions.iter_mut() { t.metrics_modifier.retain(|k, _| k.as_str() != "external_pressure"); }
                            }
                            _ => {}
                        }
                    }
                    if *cf == Cf::NoTagEp {
                        for a in st.world_state.as_mut().unwrap().actors.values_mut() {
                            for t in a.actor_tags.values_mut() { t.metrics_modifier.retain(|k, _| k.as_str() != "external_pressure"); }
                        }
                    }
                    census::set_dependency_cap((*cf == Cf::Cap80).then(|| (RALLY.to_string(), 80.0)));
                    let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, sc));
                    let mut fall: Option<bool> = None;
                    let mut won = None;
                    for _ in 0..ticks {
                        step(&mut st, &strategy);
                        let ws = st.world_state.as_ref().unwrap();
                        if fall.is_none() && ws.dead_actor_ids.contains("byzantium") { fall = Some(ws.dead_actor_ids.contains("ottomans")); }
                        if won.is_none() && ws.victory_achieved { won = Some(ws.tick as f64 - 1.0); }
                        if ws.dead_actor_ids.contains("ottomans") {
                            for a in ["byzantium", "serbia", "trebizond"] {
                                if let Some(x) = ws.actors.get(a) {
                                    let e = ceil_after.entry(a).or_default();
                                    e.1 += 1;
                                    if x.get_metric("external_pressure") >= 100.0 { e.0 += 1; }
                                }
                            }
                        }
                        for a in ["byzantium", "serbia", "trebizond", "hungary", "rome"] {
                            if let Some(x) = ws.actors.get(a) {
                                let e = ceil.entry(a).or_default();
                                e.1 += 1;
                                if x.get_metric("external_pressure") >= 100.0 { e.0 += 1; }
                            }
                        }
                    }
                    census::set_dependency_cap(None);
                    let ws = st.world_state.as_ref().unwrap();
                    if sc == "constantinople_1430" {
                        match fall { Some(false) => under += 1, Some(true) => after += 1, None => never += 1 }
                        let f = &ws.milestone_events_fired;
                        let has = |m: &str| f.iter().any(|x| x == m);
                        if has("outcome_survived_alone") { held += 1 } else if has("outcome_fell_federation") { ff += 1 } else if has("outcome_historical") { fell += 1 } else { none += 1 }
                    }
                    if ws.dead_actor_ids.contains("ottomans") { ott += 1; }
                    if ws.dead_actor_ids.contains("rome") { rome += 1; }
                    if let Some(t) = won { wins.push(t); }
                }
                let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let mut c: Vec<String> = ceil.iter().map(|(a, (h, n))| format!("{a} {:.0} %", 100.0 * *h as f64 / (*n).max(1) as f64)).collect();
                if !ceil_after.is_empty() {
                    let a: Vec<String> = ceil_after.iter().map(|(a, (h, n))| format!("{a} {:.0} % of {n}", 100.0 * *h as f64 / (*n).max(1) as f64)).collect();
                    c.push(format!("after Ottomans: {}", a.join(", ")));
                }
                let falls = if sc == "constantinople_1430" { format!("{under} · {after} · {never}") } else { "—".into() };
                let ends = if sc == "constantinople_1430" { format!("{held} · {ff} · {fell} · {none}") } else { "—".into() };
                println!("| {sc} | {world} | {label} | {falls} | {ott} | {} | {}, {}, {on} | {ends} | {} |",
                    if sc == "rome_375" { rome.to_string() } else { "—".into() }, wins.len(), q(&wins), c.join(", "));
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    match args.get(3).map(|s| s.as_str()).unwrap_or("decompose") {
        "cf" => counterfactuals(seeds, ticks, false),
        "cfc" => counterfactuals(seeds, ticks, true),
        _ => decompose(seeds, ticks),
    }
}
