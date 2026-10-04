//! A46 stage 3 — `economic_output`: who writes it, and what changes if the ceiling stops
//! swallowing it (docs/TRIAGE.md). Measurement only; every variant lives in memory.
//!
//! `decompose`: every write to an actor's `economic_output` (the A37 write sink, all
//! metrics), per scenario pooled over its worlds, living actors only — inflow and outflow
//! asked by source (dependency rules, auto-deltas, tags, events, actions), and the clamp: the
//! inflow a tick's ceiling cut takes away is shared among that tick's positive writers in
//! proportion to what each asked.
//!
//! `cf`: counterfactuals, one at a time — (a) the main writer × 0.5, (b) × 0.25 (the main
//! writer is the tag channel — every tag's `economic_output` modifier, scaled through the
//! census hook `tag_modifier`, since the modifiers are integers), (c) an
//! outflow proportional to the level (a Linear dependency `economic_output → economic_output`
//! with coefficient −k), k chosen per scenario so that the equilibrium against the measured
//! mean positive inflow alone lies at 70: k = mean asked inflow per living actor-tick / 70.
//! Readers' thresholds are not touched. Per variant and world: the share of living ticks at
//! the ceiling / floor, how many of the readers held on or off in the base become live,
//! `famine` / `trade_boom` / `silk_road_caravan`, treasury and population at ticks 50 and 150,
//! deaths, the split, the family's wins, the A10 row, the B46 endings, Milan in `aggressive`.
//!
//! Usage: cargo run --release --features census --bin a46_eco_probe -- [seeds] [ticks] [decompose|cf] [writer,…]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap};

#[path = "shared/readers.rs"]
mod shared_readers;

const SCENARIOS: &[&str] = &["rome_375", "constantinople_1430", "milan_1477"];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

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

fn source_of(w: &census::Write) -> String {
    w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file().trim_start_matches("src/"), w.location.line()))
}

/// Per scenario: source → (asked in, asked out, cut attributed), and the mean asked inflow per
/// living actor-tick.
fn decompose(seeds: u64, ticks: u32, print: bool) -> BTreeMap<String, f64> {
    census::enable_writes();
    census::watch_all_metrics(true);
    let mut mean_inflow = BTreeMap::new();
    for sc in SCENARIOS {
        let mut by: BTreeMap<String, (f64, f64, f64)> = BTreeMap::new();
        let (mut actor_ticks, mut total_in, mut total_cut) = (0u64, 0.0, 0.0);
        for world in worlds(sc) {
            for seed in 0..seeds {
                let mut st = new_state(sc, seed);
                let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, sc));
                let _ = census::take_writes();
                for _ in 0..ticks {
                    step(&mut st, &strategy);
                    let ws = st.world_state.as_ref().unwrap();
                    actor_ticks += ws.actors.len() as u64;
                    let mut per_actor: HashMap<String, Vec<census::Write>> = HashMap::new();
                    for w in census::take_writes() {
                        if w.metric == "economic_output" && ws.actors.contains_key(&w.actor) && w.source.as_deref() != Some("seat_split") {
                            per_actor.entry(w.actor.clone()).or_default().push(w);
                        }
                    }
                    for (_, ws_) in per_actor {
                        let cut: f64 = ws_.iter().map(|w| (w.requested - w.applied).max(0.0)).sum();
                        let pos: f64 = ws_.iter().map(|w| w.requested.max(0.0)).sum();
                        total_cut += cut;
                        total_in += pos;
                        for w in &ws_ {
                            let e = by.entry(source_of(w)).or_default();
                            if w.requested > 0.0 { e.0 += w.requested; if pos > 0.0 { e.2 += cut * w.requested / pos; } }
                            if w.requested < 0.0 { e.1 += -w.requested; }
                        }
                    }
                }
            }
        }
        mean_inflow.insert(sc.to_string(), total_in / actor_ticks.max(1) as f64);
        if print {
            println!("## {sc} — economic_output by writer (pooled over the worlds; living actors)\n");
            println!("mean asked inflow per living actor-tick {:+.3}; inflow cut at the ceiling {:.1} %\n", total_in / actor_ticks.max(1) as f64, 100.0 * total_cut / total_in.max(1e-9));
            println!("| writer | asked inflow | share of inflow | asked outflow | inflow cut by the ceiling (attributed) |");
            println!("|---|---|---|---|---|");
            let mut rows: Vec<(&String, &(f64, f64, f64))> = by.iter().collect();
            rows.sort_by(|a, b| (b.1.0 + b.1.1).partial_cmp(&(a.1.0 + a.1.1)).unwrap());
            for (s, (i, o, c)) in rows.iter().take(14) {
                let cut = if *i > 0.0 { format!("{:.1} %", 100.0 * c / i) } else { "—".into() };
                println!("| {s} | {i:+.0} | {:.1} % | {:+.0} | {cut} |", 100.0 * i / total_in.max(1e-9), -o);
            }
            println!();
        }
    }
    census::watch_all_metrics(false);
    mean_inflow
}

#[derive(Default)]
struct WorldOut {
    eo_ticks: u64, eo_ceil: u64, eo_floor: u64,
    events: BTreeMap<&'static str, u64>,
    treasury50: Vec<f64>, treasury150: Vec<f64>, pop50: Vec<f64>, pop150: Vec<f64>,
    deaths: BTreeMap<&'static str, u64>, deaths_total: u64,
    split40: u64, wins: Vec<f64>, np_alive_wins: u64,
    endings: [u64; 4],
    milan: Vec<(f64, f64, f64, f64)>, // legitimacy, treasury, military_size, eo at 150
    occ: census::Occupancy,
}

fn run_world(sc: &str, world: &str, seeds: u64, ticks: u32, variant: char, writers: &[String], k: f64) -> WorldOut {
    let mut o = WorldOut::default();
    let _ = census::take_occupancy();
    for seed in 0..seeds {
        let mut st = new_state(sc, seed);
        {
            let s = st.current_scenario.as_mut().unwrap();
            match variant {
                'a' | 'b' if !writers.is_empty() => {
                    let f = if variant == 'a' { 0.5 } else { 0.25 };
                    for d in s.dependencies.iter_mut().filter(|d| writers.contains(&d.id)) { d.coefficient *= f; }
                }
                'c' => {
                    let rule: engine13::core::DependencyRule = toml::from_str(&format!(
                        "id = \"a46_economic_output_proportional_outflow\"\nfrom = \"economic_output\"\nto = \"economic_output\"\ncoefficient = {}\nmode = \"linear\"", -k)).unwrap();
                    s.dependencies.push(rule);
                }
                _ => {}
            }
        }
        census::set_tag_scale(match variant {
            'a' if writers.is_empty() => Some(("economic_output".to_string(), 0.5)),
            'b' if writers.is_empty() => Some(("economic_output".to_string(), 0.25)),
            _ => None,
        });
        let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
        let mut won = None;
        for _ in 0..ticks {
            step(&mut st, &strategy);
            let _ = census::take();
            let ws = st.world_state.as_ref().unwrap();
            for a in ws.actors.values() {
                let v = a.get_metric("economic_output");
                o.eo_ticks += 1;
                if v >= 99.0 { o.eo_ceil += 1; }
                if v <= 1.0 { o.eo_floor += 1; }
            }
            if ws.tick == 50 || ws.tick == 150 {
                for a in ws.actors.values() {
                    let (t, p) = (a.get_metric("treasury"), a.get_metric("population"));
                    if ws.tick == 50 { o.treasury50.push(t); o.pop50.push(p); } else { o.treasury150.push(t); o.pop150.push(p); }
                }
                if ws.tick == 150 && sc == "milan_1477" && world == "aggressive" {
                    if let Some(m) = ws.actors.get("milan") {
                        o.milan.push((m.get_metric("legitimacy"), m.get_metric("treasury"), m.get_metric("military_size"), m.get_metric("economic_output")));
                    }
                }
            }
            if won.is_none() && ws.victory_achieved {
                won = Some(ws.tick as f64 - 1.0);
                if world == "none" && sc == "constantinople_1430" && !ws.dead_actor_ids.contains("ottomans") { o.np_alive_wins += 1; }
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        for e in &st.event_log.events {
            for id in ["famine", "trade_boom", "silk_road_caravan"] { if e.id == id { *o.events.entry(id).or_default() += 1; } }
        }
        for a in ["rome", "byzantium", "ottomans", "milan"] { if ws.dead_actor_ids.contains(a) { *o.deaths.entry(a).or_default() += 1; } }
        o.deaths_total += ws.dead_actor_ids.len() as u64;
        if sc == "rome_375" && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { o.split40 += 1; }
        if let Some(t) = won { o.wins.push(t); }
        if sc == "constantinople_1430" {
            let f = &ws.milestone_events_fired;
            let has = |m: &str| f.iter().any(|x| x == m);
            let i = if has("outcome_survived_alone") { 0 } else if has("outcome_fell_federation") { 1 } else if has("outcome_historical") { 2 } else { 3 };
            o.endings[i] += 1;
        }
    }
    o.occ = census::take_occupancy();
    o
}

fn cf(seeds: u64, ticks: u32, writers: Vec<String>) {
    let inflow = decompose(seeds.min(10), ticks, false);
    println!("# A46 stage 3 — economic_output counterfactuals, {seeds} seeds × {ticks} ticks per world\n");
    println!("main writer scaled in (a), (b): {}\n", if writers.is_empty() { "the tag channel — every tag's economic_output modifier".to_string() } else { writers.join(", ") });
    println!("(c) k per scenario = mean asked inflow per living actor-tick / 70 (measured on 10 seeds of the base):");
    let ks: BTreeMap<String, f64> = inflow.iter().map(|(s, i)| (s.clone(), i / 70.0)).collect();
    for (s, k) in &ks { println!("- {s}: inflow {:+.3} → k = {k:.5}", inflow[s]); }
    println!();
    census::enable();
    census::enable_occupancy();
    census::occupancy_live_only(true);
    for sc in SCENARIOS {
        let scenario = engine13::scenarios::registry::load_by_id(sc).unwrap();
        let (readers, _) = shared_readers::readers(&scenario, &["economic_output"]);
        println!("## {sc}\n");
        println!("| variant | world | eo at ceiling / floor | eo readers held in base → live now | famine · trade_boom · silk_road | treasury @50 / @150 p10/50/90 | population @50 / @150 p10/50/90 | deaths: rome · byz · ott · milan · all | split 40 | wins, tick p10/50/90, on 40–43; np wins with Ottomans alive | endings held·fed·fell·none | Milan @150 aggressive: leg, treasury, army, eo (p50) |");
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
        let mut base_held: HashMap<String, Vec<usize>> = HashMap::new();
        // variant -> reader index -> worlds where it came alive
        let mut revived: BTreeMap<char, BTreeMap<usize, Vec<String>>> = BTreeMap::new();
        for (variant, label) in [('x', "base"), ('a', "(a) × 0.5"), ('b', "(b) × 0.25"), ('c', "(c) proportional outflow")] {
            for world in worlds(sc) {
                let o = run_world(sc, world, seeds, ticks, variant, &writers, ks[*sc]);
                // readers: share true per reader in this world
                let mut held_idx = Vec::new();
                let mut live_now = 0;
                for (i, r) in readers.iter().enumerate() {
                    let (t, n) = o.occ.iter().filter(|((c, _), _)| c.starts_with(&r.ctx_prefix) && c.ends_with(&r.ctx_suffix) && (r.kind != "dependency" || c == &r.ctx_prefix))
                        .fold((0u64, 0u64), |a, (_, v)| (a.0 + v.0, a.1 + v.1));
                    if n == 0 { continue; }
                    let p = 100.0 * t as f64 / n as f64;
                    if !(1.0..=99.0).contains(&p) { held_idx.push(i); }
                    if variant != 'x' && base_held.get(*world).is_some_and(|h| h.contains(&i)) && (1.0..=99.0).contains(&p) {
                        live_now += 1;
                        revived.entry(variant).or_default().entry(i).or_default().push(format!("{world} {p:.0} %"));
                    }
                }
                if variant == 'x' { base_held.insert(world.to_string(), held_idx); }
                let held_base = base_held.get(*world).map_or(0, |h| h.len());
                let pct = |x: u64| 100.0 * x as f64 / o.eo_ticks.max(1) as f64;
                let on = o.wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let d = |a: &str| o.deaths.get(a).copied().unwrap_or(0);
                let ev = |e: &str| o.events.get(e).copied().unwrap_or(0);
                let milan = if o.milan.is_empty() { "—".into() } else {
                    let med = |f: fn(&(f64, f64, f64, f64)) -> f64| { let mut v: Vec<f64> = o.milan.iter().map(f).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] };
                    format!("{:.0}, {:.0}, {:.0}, {:.0}", med(|m| m.0), med(|m| m.1), med(|m| m.2), med(|m| m.3))
                };
                println!("| {label} | {world} | {:.1} % / {:.1} % | {held_base} → {} | {} · {} · {} | {} / {} | {} / {} | {} · {} · {} · {} · {} | {} | {}, {}, {on}; {} | {} | {milan} |",
                    pct(o.eo_ceil), pct(o.eo_floor), if variant == 'x' { "—".to_string() } else { live_now.to_string() },
                    ev("famine"), ev("trade_boom"), ev("silk_road_caravan"),
                    q(&o.treasury50), q(&o.treasury150), q(&o.pop50), q(&o.pop150),
                    d("rome"), d("byzantium"), d("ottomans"), d("milan"), o.deaths_total,
                    if *sc == "rome_375" { format!("{} / {seeds}", o.split40) } else { "—".into() },
                    o.wins.len(), q(&o.wins), o.np_alive_wins,
                    if *sc == "constantinople_1430" { format!("{}·{}·{}·{}", o.endings[0], o.endings[1], o.endings[2], o.endings[3]) } else { "—".into() });
            }
        }
        println!();
        for (v, m) in &revived {
            for (i, ws) in m {
                println!("- revived in {v}: {} {} — {}", readers[*i].kind, readers[*i].id, ws.join(", "));
            }
        }
        println!();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    match args.get(3).map(|s| s.as_str()).unwrap_or("decompose") {
        "cf" => {
            // default: the tag channel (decomposition: 92–97 % of the inflow in every scenario)
            let writers: Vec<String> = args.get(4).map(|s| s.split(',').map(|x| x.to_string()).collect()).unwrap_or_default();
            cf(seeds, ticks, writers);
        }
        _ => { println!("# A46 stage 3 — economic_output decomposition, {seeds} seeds × {ticks} ticks per world\n"); decompose(seeds, ticks, true); }
    }
}
