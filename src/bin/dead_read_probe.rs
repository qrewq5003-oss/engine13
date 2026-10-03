//! Dead-read census — stage 1 of B44 (docs/TRIAGE.md).
//!
//! Finds reads of absent actors where they happen, not by listing known ids: built with
//! `--features census`, `MetricRef::get`/`try_get` record every read of an actor that is
//! not in the world, at its call site (`core::census`). Condition sites add the operator
//! and the result the engine drew from the default.
//!
//! All three scenarios, no player and every scripted strategy, seeds × ticks. Two modes:
//! the engine as it is, and the counterfactual "any condition on an absent actor is
//! false" (the rule of milestones with `actor_id`). Prints:
//!
//! 1. every dead read, grouped by world, call site, content, key, operator and result;
//! 2. controls: the known federation pull must be found; no read of a *dead* actor before
//!    the first death; reads of never-present actors before it are listed apart;
//! 3. call sites met (for the static cross-check against the list in TRIAGE);
//! 4. counterfactual: which rows flip, and how the metrics they write move;
//! 5. the named cases: Byzantine pressure after the Ottomans die (which writer holds
//!    it), milestones fired over a fallen Byzantium, the federation's bands while she is
//!    dead (a global metric about a dead subject — not a dead read at all).
//!
//! Usage: cargo run --release --features census --bin dead_read_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use engine13::engine::trace;
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const WORLDS: &[(&str, &[&str])] = &[
    ("rome_375", &["none", "balanced", "influence", "wealth"]),
    ("constantinople_1430", &["none", "balanced", "diplomacy", "military"]),
    ("milan_1477", &["none", "aggressive"]),
];

/// (site, context, key, kind, test, result, target)
type RowKey = (String, String, String, &'static str, String, String, &'static str);

#[derive(Default)]
struct Agg {
    runs: BTreeSet<u64>,
    ticks: BTreeSet<(u64, u32)>,
    reads: u64,
}

#[derive(Default)]
struct World {
    rows: BTreeMap<RowKey, Agg>,
    sites: BTreeMap<String, u64>,
    // controls
    runs_without_death: u64,
    before_first_death_dead: u64,
    before_first_death_absent: BTreeMap<String, u64>,
    // metric means: key -> (sum, n) over alive ticks
    metrics: HashMap<String, (f64, u64)>,
    victories: u64,
    deaths: u64,
    // named cases (constantinople)
    ep_after_ottomans: Vec<f64>,
    ep_ticks_after_ottomans: u64,
    ep_block_sum: BTreeMap<usize, f64>,
    ep_dep_sum: BTreeMap<String, f64>,
    ep_observed_change: f64,
    ottomans_die_first: u64,
    // B44 stage 2: pressure on the Ottomans' neighbours once the Ottomans are gone.
    neighbour_ep: BTreeMap<String, (f64, u64)>,
    milestones_after_byz: BTreeMap<String, u64>,
    milestones_fired: BTreeMap<String, u64>,
    byz_dead_ticks: u64,
    byz_dead_fed80: u64,
    byz_dead_fed60: u64,
    byz_dead_runs_fed80: BTreeSet<u64>,
    byz_death_ticks: Vec<u32>,
}

fn run_world(scenario: &str, world: &str, seeds: u64, ticks: u32, uniform_false: bool) -> World {
    let mut w = World::default();
    census::set_uniform_false(uniform_false);
    for seed in 0..seeds {
        let db = engine13::db::Db::open_in_memory().unwrap();
        let mut st = engine13::AppState::default();
        engine13::load_scenario(&mut st, &db, scenario.to_string()).unwrap();
        st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
        let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, scenario));
        census::enable();
        trace::enable();
        let _ = census::take();
        let mut first_death: Option<u32> = None;
        let mut byz_death: Option<u32> = None;
        let mut fired_seen: BTreeSet<String> = BTreeSet::new();
        for t in 0..ticks {
            let ep_before = st.world_state.as_ref().unwrap().actors.get("byzantium")
                .and_then(|a| a.metrics.get("external_pressure").copied());
            let ott_dead_before = st.world_state.as_ref().unwrap().dead_actor_ids.contains("ottomans");
            match &strategy {
                Some(s) => { play_scripted_tick(&mut st, s); }
                None => {
                    let ws = st.world_state.as_mut().unwrap();
                    let sc = st.current_scenario.as_ref().unwrap();
                    engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
                }
            }
            let ws = st.world_state.as_ref().unwrap();
            let reads = census::take();
            let blocks = trace::take_auto_deltas();
            let deps = trace::take_dependencies();

            for r in &reads {
                let site = format!("{}:{}", r.location.file(), r.location.line());
                *w.sites.entry(site.clone()).or_default() += 1;
                let (test, result, used) = match &r.condition {
                    Some(c) => (c.test.clone(), c.result.to_string(), c.used.to_string()),
                    None => ("(value)".into(), "0.0".into(), "0.0".into()),
                };
                let kind = if r.dead { "dead" } else { "absent" };
                let _ = used;
                // Whom the content writes: an auto-delta onto an absent actor is a no-op
                // whatever its condition says.
                let target = match r.context.strip_prefix("auto_delta[").and_then(|x| x.split("] ").nth(1)).and_then(|x| x.split(" | ").next()) {
                    Some(m) if m.starts_with("actor:") => {
                        let id = m.trim_start_matches("actor:").split('.').next().unwrap_or("");
                        if ws.actors.contains_key(id) { "live actor" } else { "absent actor (no-op)" }
                    }
                    Some(_) => "global/family",
                    None => "—",
                };
                let a = w.rows.entry((site, r.context.clone(), r.key.clone(), kind, test, result, target)).or_default();
                a.runs.insert(seed);
                a.ticks.insert((seed, t));
                a.reads += 1;
                if first_death.is_none() {
                    if r.dead { w.before_first_death_dead += 1; } else {
                        *w.before_first_death_absent.entry(format!("{} | {}", r.context, r.key)).or_default() += 1;
                    }
                }
            }
            if first_death.is_none() && !ws.dead_actor_ids.is_empty() { first_death = Some(t); }
            if byz_death.is_none() && ws.dead_actor_ids.contains("byzantium") { byz_death = Some(t); }

            for m in &ws.milestone_events_fired {
                if fired_seen.insert(m.clone()) {
                    *w.milestones_fired.entry(m.clone()).or_default() += 1;
                    if byz_death.is_some() {
                        *w.milestones_after_byz.entry(m.clone()).or_default() += 1;
                    }
                }
            }

            // Rome once the Huns are gone (B44 stage 2: the ratio relief returns).
            if ws.dead_actor_ids.contains("huns") {
                if let Some(v) = ws.actors.get("rome").and_then(|a| a.metrics.get("external_pressure")) {
                    let e = w.neighbour_ep.entry("rome (huns dead)".to_string()).or_default();
                    e.0 += v; e.1 += 1;
                }
            }
            if ott_dead_before {
                for n in ["byzantium", "serbia", "trebizond", "hungary"] {
                    if let Some(v) = ws.actors.get(n).and_then(|a| a.metrics.get("external_pressure")) {
                        let e = w.neighbour_ep.entry(n.to_string()).or_default();
                        e.0 += v; e.1 += 1;
                    }
                }
            }
            // Byzantine pressure while the Ottomans are already dead and Byzantium lives.
            if ott_dead_before {
                if let (Some(before), Some(after)) = (ep_before, ws.actors.get("byzantium").and_then(|a| a.metrics.get("external_pressure").copied())) {
                    w.ep_after_ottomans.push(after);
                    w.ep_ticks_after_ottomans += 1;
                    w.ep_observed_change += after - before;
                    for b in blocks.iter().filter(|b| b.metric == "actor:byzantium.external_pressure") {
                        *w.ep_block_sum.entry(b.index).or_default() += b.applied;
                    }
                    for d in deps.iter().filter(|d| d.actor == "byzantium" && d.rule.contains("external_pressure")) {
                        *w.ep_dep_sum.entry(d.rule.clone()).or_default() += d.delta;
                    }
                }
            }

            if scenario == "constantinople_1430" && byz_death.is_some() {
                w.byz_dead_ticks += 1;
                let fed = ws.global_metrics.get("federation_progress").copied().unwrap_or(0.0);
                if fed >= 80.0 { w.byz_dead_fed80 += 1; w.byz_dead_runs_fed80.insert(seed); }
                else if fed >= 60.0 { w.byz_dead_fed60 += 1; }
            }

            for (k, v) in &ws.global_metrics {
                let e = w.metrics.entry(format!("global:{k}")).or_default();
                e.0 += v; e.1 += 1;
            }
            if let Some(fs) = &ws.family_state {
                for (k, v) in &fs.metrics {
                    let e = w.metrics.entry(format!("family:{k}")).or_default();
                    e.0 += v; e.1 += 1;
                }
            }
            for (id, a) in &ws.actors {
                for (k, v) in &a.metrics {
                    let e = w.metrics.entry(format!("actor:{id}.{k}")).or_default();
                    e.0 += v; e.1 += 1;
                }
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        if ws.dead_actor_ids.is_empty() { w.runs_without_death += 1; }
        if ws.victory_achieved { w.victories += 1; }
        w.deaths += ws.dead_actor_ids.len() as u64;
        if let Some(t) = byz_death { w.byz_death_ticks.push(t); }
        if ws.dead_actor_ids.contains("ottomans") && !ws.dead_actor_ids.contains("byzantium") {
            w.ottomans_die_first += 1;
        }
        trace::disable();
    }
    census::set_uniform_false(false);
    w
}

fn mean(m: &HashMap<String, (f64, u64)>, k: &str) -> String {
    // Family metrics are stored unprefixed (`family:family_wealth` → `wealth`).
    let k = match k.strip_prefix("family:") {
        Some(f) => format!("family:{}", f.strip_prefix("family_").unwrap_or(f)),
        None => k.to_string(),
    };
    m.get(&k).map_or("—".into(), |(s, n)| format!("{:.2}", s / *n as f64))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("# dead-read census: {seeds} seeds × {ticks} ticks per world\n");

    let mut all_sites: BTreeMap<String, u64> = BTreeMap::new();
    let mut positive = false;
    for (scenario, worlds) in WORLDS {
        let runs: Vec<(World, World)> = worlds.iter()
            .map(|world| (run_world(scenario, world, seeds, ticks, false), run_world(scenario, world, seeds, ticks, true)))
            .collect();
        // Content table: one row per (site, content, key, kind, test, result, target),
        // one column per world — `runs/ticks` in which the read happened.
        let mut keys: BTreeSet<&RowKey> = BTreeSet::new();
        for (b, _) in &runs { keys.extend(b.rows.keys()); }
        println!("## {scenario}: dead reads — runs/ticks per world ({})\n", worlds.join(" · "));
        println!("| site | content | key | kind | test | result | writes to | {} |", worlds.join(" | "));
        println!("|{}", "---|".repeat(7 + worlds.len()));
        for k in &keys {
            let cells: Vec<String> = runs.iter().map(|(b, _)| b.rows.get(*k).map_or("—".into(), |a| format!("{}/{}", a.runs.len(), a.ticks.len()))).collect();
            let site = k.0.trim_start_matches("src/");
            println!("| {site} | {} | {} | {} | {} | {} | {} | {} |", k.1, k.2, k.3, k.4, k.5, k.6, cells.join(" | "));
        }
        println!();
        for (world, (base, cf)) in worlds.iter().zip(&runs) {
            println!("### {scenario} / {world}");
            println!("deaths {} · victories {} · runs without a death {}/{}", base.deaths, base.victories, base.runs_without_death, seeds);
            println!("control: dead reads before the first death = {} (must be 0)", base.before_first_death_dead);
            for (k, n) in &base.before_first_death_absent {
                println!("control: never-present reads before the first death: {k} ×{n}");
            }
            for (_, ctx, key, _, test, result, _) in base.rows.keys() {
                if *scenario == "constantinople_1430" && *world == "none" && ctx.contains("global:federation_progress")
                    && key == "actor:byzantium.external_pressure" && test == "Greater 70" && result == "false" {
                    positive = true;
                }
            }
            for (s, n) in &base.sites { *all_sites.entry(s.clone()).or_default() += n; }

            // Counterfactual: rows whose condition was true on an absent actor flip.
            let flipped: Vec<&RowKey> = base.rows.keys().filter(|k| k.5 == "true" && !k.4.contains("insufficient")).collect();
            println!("\ncounterfactual (absent → false): {} row(s) flip · deaths {} → {} · victories {} → {}", flipped.len(), base.deaths, cf.deaths, base.victories, cf.victories);
            let mut targets: BTreeSet<String> = BTreeSet::new();
            for k in &flipped {
                println!("  flips: {} | {} | {} {} | writes to {}", k.1, k.2, k.4, k.3, k.6);
                if let Some(rest) = k.1.strip_prefix("auto_delta[") {
                    if let Some(m) = rest.split("] ").nth(1).and_then(|s| s.split(" | ").next()) { targets.insert(m.to_string()); }
                }
            }
            for t in &targets {
                println!("  mean {t}: {} → {}", mean(&base.metrics, t), mean(&cf.metrics, t));
            }

            if *scenario == "constantinople_1430" {
                let ep = &base.ep_after_ottomans;
                let ep_mean = if ep.is_empty() { "—".into() } else { format!("{:.2}", ep.iter().sum::<f64>() / ep.len() as f64) };
                println!("\ncase: Byzantine pressure after the Ottomans die (Byzantium alive): runs {}, ticks {}, mean {ep_mean}, observed Δ total {:.2}",
                    base.ottomans_die_first, base.ep_ticks_after_ottomans, base.ep_observed_change);
                for (i, s) in &base.ep_block_sum { println!("  auto_delta[{i}] applied Σ {s:.2}"); }

                for (r, s) in &base.ep_dep_sum { println!("  dependency {r} Σ {s:.2}"); }
                let mut d = base.byz_death_ticks.clone();
                d.sort();
                println!("case: Byzantium dies in {}/{seeds} runs, tick p50 {}", d.len(), d.get(d.len() / 2).map_or("—".into(), |t| t.to_string()));
                println!("case: federation bands with Byzantium dead: {} dead ticks, ≥ 80 («готова») {} ticks in {} runs, 60–80 {} ticks",
                    base.byz_dead_ticks, base.byz_dead_fed80, base.byz_dead_runs_fed80.len(), base.byz_dead_fed60);
                for (m, n) in &base.milestones_fired {
                    println!("case: milestone {m}: fired {n}/{seeds}, of them after Byzantium's death {}", base.milestones_after_byz.get(m).copied().unwrap_or(0));
                }
            }
            for (n, (s, c)) in &base.neighbour_ep {
                println!("case: {n} external_pressure (after the opponent died): mean {:.2} over {c} living ticks", s / *c as f64);
            }
            println!();
        }
    }
    println!("## call sites met with an absent actor (all worlds)");
    for (s, n) in &all_sites { println!("  {s}  ×{n}"); }
    println!("\npositive control (federation pull on byzantium.external_pressure > 70, constantinople / none): {}",
        if positive { "FOUND" } else { "MISSING — the census is broken" });
}
