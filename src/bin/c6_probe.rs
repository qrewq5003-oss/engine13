//! Economy project, Ц6 stage 1: pressure from a real threat (docs/economy_project_brief.md §9).
//! Measurement only; nothing is chosen. Built with `--features census`. v2 as at `797e535`;
//! every world of the three scenarios, 30 seeds × 300 ticks.
//!
//! Models: base (v2 as is); (а) tags' `external_pressure` modifiers as a level; (б) (а) plus the
//! pull toward the threat `T_p = 100 × N / (N + own army)` (`engine::pressure_threat`, N = armies
//! of living neighbours at distance 1) at r = 0.03, 0.05, 0.10.
//!
//! Printed: the `b / r` table of the pressure writers (asked per living actor-tick, in base); the
//! Ц6 measure (ceiling ≥ 99 under 30 % of living ticks; the drop of a neighbour's pressure in the
//! 10 ticks after an actor dies); meaningfulness (the correlation of pressure with T_p over
//! actor-ticks; of the ticks at pressure ≥ 85, the share with a stronger armed neighbourhood than
//! one's own army, T_p > 50); the cascade (legitimacy ≤ 1, cohesion < 15, `economic_output` < T/2,
//! Rome's depopulation); where deaths move (paired by seed against base, per actor); the
//! historical profile (West Rome's deaths, Byzantium's fall tick without a player, Milan with the
//! bot); and A10, A35, the split, B46 outcomes, the regency fork for information.
//!
//! Usage: cargo run --release --features census --bin c6_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const PULLS: [f64; 3] = [0.03, 0.05, 0.10];
const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if s.is_empty() { return f64::NAN; }
    s[((s.len() - 1) as f64 * p).round() as usize]
}

fn q(v: &[f64]) -> String {
    if v.is_empty() { return "—".into(); }
    format!("{:.0}/{:.0}/{:.0}", pct(v, 0.1), pct(v, 0.5), pct(v, 0.9))
}

#[derive(Clone, Copy, PartialEq)]
enum Model { Base, TagsLevel, Pull(f64) }

impl Model {
    fn label(&self) -> String {
        match self {
            Model::Base => "base (797e535)".into(),
            Model::TagsLevel => "(а) tags as level".into(),
            Model::Pull(r) => format!("(б) tags as level + pull r = {r:.2}"),
        }
    }
}

#[derive(Default)]
struct Run {
    // per actor: (living, ceiling, legit floor, coh < 15, eo < T/2)
    acc: BTreeMap<String, [u64; 5]>,
    // correlation sums over actor-ticks: n, Σx, Σy, Σxx, Σyy, Σxy (x = ep, y = T_p)
    corr: [f64; 6],
    high: (u64, u64),
    // after a neighbour dies: (ep at death, ep 10 ticks later)
    drops: Vec<(f64, f64)>,
    // source -> asked, and per actor
    sources: BTreeMap<(String, String), f64>,
    dead: BTreeMap<String, u32>,
    death_tick: BTreeMap<String, u32>,
    deaths: u32,
    rome_zombie: bool,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

fn run(sc: &str, world: &str, m: Model, seed: u64, ticks: u32, with_sources: bool) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        s.economy_v2_pressure_tags_as_level = m != Model::Base;
        s.economy_v2_pressure_pull = if let Model::Pull(r) = m { Some(r) } else { None };
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    // pending: (due tick, actor, ep at death)
    let mut pending: Vec<(u32, String, f64)> = Vec::new();
    let _ = census::take_writes();
    for _ in 0..ticks {
        let before: std::collections::BTreeSet<String> = st.world_state.as_ref().unwrap().dead_actor_ids.iter().cloned().collect();
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let writes = census::take_writes();
        let scn = st.current_scenario.as_ref().unwrap();
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        if with_sources {
            for w in &writes {
                if w.metric == "external_pressure" && ws.actors.contains_key(&w.actor) {
                    let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                    *r.sources.entry((w.actor.clone(), src)).or_default() += w.requested;
                }
            }
        }
        // newly dead → watch living neighbours at distance 1
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) {
            *r.dead.entry(d.clone()).or_default() += 1;
            r.death_tick.entry(d.clone()).or_insert(t);
            for (id, a) in &ws.actors {
                if ws.dead_actor_ids.contains(id) { continue; }
                if a.neighbors.iter().any(|n| n.distance == 1 && &n.id == d) {
                    pending.push((t + 10, id.clone(), a.get_metric("external_pressure")));
                }
            }
        }
        pending.retain(|(due, id, ep0)| {
            if *due != t { return true; }
            if let Some(a) = ws.actors.get(id).filter(|_| !ws.dead_actor_ids.contains(id)) {
                r.drops.push((*ep0, a.get_metric("external_pressure")));
            }
            false
        });
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let ep = a.get_metric("external_pressure");
            let e = r.acc.entry(id.clone()).or_default();
            e[0] += 1;
            if ep >= 99.0 { e[1] += 1; }
            if a.get_metric("legitimacy") <= 1.0 { e[2] += 1; }
            if a.get_metric("cohesion") < 15.0 { e[3] += 1; }
            if engine13::engine::eo_target(ws, scn, id).is_some_and(|tg| a.get_metric("economic_output") < tg / 2.0) { e[4] += 1; }
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) {
                r.corr[0] += 1.0; r.corr[1] += ep; r.corr[2] += tp; r.corr[3] += ep * ep; r.corr[4] += tp * tp; r.corr[5] += ep * tp;
                if ep >= 85.0 { r.high.1 += 1; if tp > 50.0 { r.high.0 += 1; } }
            }
            if id == "rome" && a.get_metric("population") <= 1.0 { r.rome_zombie = true; }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    r.outcomes = ws.milestone_events_fired.iter().filter(|m| m.starts_with("outcome_")).cloned().collect();
    r.stab = ws.milestone_events_fired.iter().any(|m| m == "milan_regency_stabilizes");
    r.deep = ws.milestone_events_fired.iter().any(|m| m == "milan_regency_crisis_deepens");
    r
}

fn paired(a: &[Run], b: &[Run], f: impl Fn(&Run) -> f64) -> (f64, f64) {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| f(y) - f(x)).collect();
    let n = d.len() as f64;
    let mean = d.iter().sum::<f64>() / n;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
    (mean, t)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    println!("# Ц6 stage 1 — pressure from a real threat, {seeds} seeds × {ticks} ticks per world\n");
    let models: Vec<Model> = [Model::Base, Model::TagsLevel].into_iter().chain(PULLS.iter().map(|r| Model::Pull(*r))).collect();
    let mut br_rows = Vec::new();
    let mut measure_rows = Vec::new();
    let mut cascade_rows = Vec::new();
    let mut death_rows = Vec::new();
    let mut hist_rows = Vec::new();
    let mut info_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let mut src_pool: BTreeMap<(String, String), f64> = BTreeMap::new();
        let mut living_pool: BTreeMap<String, u64> = BTreeMap::new();
        for world in worlds(sc) {
            let base: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::Base, s, ticks, true)).collect();
            for rr in &base {
                for (k, x) in &rr.sources { *src_pool.entry(k.clone()).or_default() += x; }
                for (k, a) in &rr.acc { *living_pool.entry(k.clone()).or_default() += a[0]; }
            }
            for m in &models {
                let runs: Vec<Run> = if *m == Model::Base { (0..seeds).map(|s| run(sc, world, *m, s, ticks, false)).collect() } else { (0..seeds).map(|s| run(sc, world, *m, s, ticks, false)).collect() };
                let mut pool: BTreeMap<String, [u64; 5]> = BTreeMap::new();
                for rr in &runs { for (k, a) in &rr.acc { let e = pool.entry(k.clone()).or_default(); for (x, y) in e.iter_mut().zip(a) { *x += y; } } }
                let tot: [u64; 5] = pool.values().fold([0; 5], |mut s, a| { for (x, y) in s.iter_mut().zip(a) { *x += y; } s });
                let p = |x: u64, n: u64| 100.0 * x as f64 / n.max(1) as f64;
                let under30 = pool.values().filter(|a| p(a[1], a[0]) < 30.0).count();
                let c = runs.iter().fold([0.0; 6], |mut s, rr| { for (x, y) in s.iter_mut().zip(&rr.corr) { *x += y; } s });
                let (n, sx, sy, sxx, syy, sxy) = (c[0], c[1], c[2], c[3], c[4], c[5]);
                let corr = (n * sxy - sx * sy) / ((n * sxx - sx * sx).sqrt() * (n * syy - sy * sy).sqrt());
                let (hi, hn) = runs.iter().fold((0, 0), |s, rr| (s.0 + rr.high.0, s.1 + rr.high.1));
                let drops: Vec<f64> = runs.iter().flat_map(|rr| rr.drops.iter().map(|(a, b)| a - b)).collect();
                let fell10 = runs.iter().flat_map(|rr| rr.drops.iter()).filter(|(a, b)| b <= &(a - 10.0)).count();
                measure_rows.push(format!("| {sc} | {world} | {} | {:.0} % | {under30} / {} | {} cases: drop p10/50/90 {}; fell ≥ 10 in {:.0} % | {corr:.2} | {:.0} % of {hn} |",
                    m.label(), p(tot[1], tot[0]), pool.len(), drops.len(), q(&drops), 100.0 * fell10 as f64 / drops.len().max(1) as f64, 100.0 * hi as f64 / hn.max(1) as f64));
                let rome = if sc == "rome_375" { format!("{} / {seeds}", runs.iter().filter(|rr| rr.rome_zombie).count()) } else { "—".into() };
                cascade_rows.push(format!("| {sc} | {world} | {} | {:.0} % | {:.0} % | {:.0} % | {rome} |", m.label(), p(tot[2], tot[0]), p(tot[3], tot[0]), p(tot[4], tot[0])));
                // deaths per actor, paired against base
                let (dm, dt) = paired(&base, &runs, |rr| rr.deaths as f64);
                let mut actors: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                for rr in base.iter().chain(runs.iter()) { actors.extend(rr.dead.keys().cloned()); }
                let mut moves: Vec<(String, i64)> = actors.iter().map(|a| {
                    let b: u32 = base.iter().map(|rr| rr.dead.get(a).copied().unwrap_or(0)).sum();
                    let x: u32 = runs.iter().map(|rr| rr.dead.get(a).copied().unwrap_or(0)).sum();
                    (format!("{a} {b}→{x}"), x as i64 - b as i64)
                }).collect();
                moves.sort_by(|a, b| b.1.abs().cmp(&a.1.abs()));
                let keys: Vec<String> = KEY.iter().filter(|k| actors.contains(**k) || base[0].acc.contains_key(**k)).map(|k| {
                    let (km, kt) = paired(&base, &runs, |rr| rr.dead.get(*k).copied().unwrap_or(0) as f64);
                    format!("{k} {km:+.2} (t {kt:+.1})")
                }).collect();
                death_rows.push(format!("| {sc} | {world} | {} | {} | {dm:+.2} (t {dt:+.1}) | {} | {} |", m.label(), runs.iter().map(|rr| rr.deaths).sum::<u32>(),
                    keys.join("; "), moves.iter().filter(|x| x.1 != 0).take(6).map(|x| x.0.clone()).collect::<Vec<_>>().join(", ")));
                // historical profile
                let hist = match sc {
                    "rome_375" => format!("Rome dies in {} / {seeds}", runs.iter().filter(|rr| rr.dead.contains_key("rome")).count()),
                    "constantinople_1430" if *world == "none" => {
                        let ft: Vec<f64> = runs.iter().filter_map(|rr| rr.death_tick.get("byzantium").map(|t| *t as f64)).collect();
                        format!("Byzantium falls in {} / {seeds}, tick p10/50/90 {}", ft.len(), q(&ft))
                    }
                    "milan_1477" if *world == "aggressive" => format!("Milan dies in {} / {seeds}", runs.iter().filter(|rr| rr.dead.contains_key("milan")).count()),
                    _ => String::new(),
                };
                if !hist.is_empty() { hist_rows.push(format!("| {sc} | {world} | {} | {hist} |", m.label())); }
                let wins: Vec<f64> = runs.iter().filter_map(|rr| rr.win.map(|t| t as f64)).collect();
                let mut perw: BTreeMap<u32, u32> = BTreeMap::new();
                for t in &wins { *perw.entry(*t as u32).or_default() += 1; }
                let top = perw.values().max().copied().unwrap_or(0);
                let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let wins_s = if wins.is_empty() { "0".into() } else { format!("{} ({}; 40–43: {on}; top {:.0} %)", wins.len(), q(&wins), 100.0 * top as f64 / wins.len() as f64) };
                let mut oc: BTreeMap<String, u32> = BTreeMap::new();
                for rr in &runs { for o in &rr.outcomes { *oc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                let multi = runs.iter().filter(|rr| rr.outcomes.len() > 1).count();
                let outc = if oc.is_empty() { "—".into() } else { format!("{} (>1: {multi})", oc.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")) };
                let fork = if sc == "milan_1477" { format!("{} / {}", runs.iter().filter(|rr| rr.stab).count(), runs.iter().filter(|rr| rr.deep).count()) } else { "—".into() };
                let split = if sc == "rome_375" { runs.iter().filter(|rr| rr.split40).count().to_string() } else { "—".into() };
                info_rows.push(format!("| {sc} | {world} | {} | {wins_s} | {split} | {outc} | {fork} |", m.label()));
            }
        }
        // b / r table: per source, mean asked per living actor-tick (all actors) and the largest actor mean
        let total_living: u64 = living_pool.values().sum();
        let mut by_src: BTreeMap<String, (f64, f64, String)> = BTreeMap::new();
        for ((actor, src), x) in &src_pool {
            let e = by_src.entry(src.clone()).or_insert((0.0, 0.0, String::new()));
            e.0 += x;
            let per_actor = x / living_pool.get(actor).copied().unwrap_or(1).max(1) as f64;
            if per_actor.abs() > e.1.abs() { e.1 = per_actor; e.2 = actor.clone(); }
        }
        let mut v: Vec<(String, (f64, f64, String))> = by_src.into_iter().collect();
        v.sort_by(|a, b| b.1 .1.abs().partial_cmp(&a.1 .1.abs()).unwrap());
        for (src, (sum, max, who)) in v.iter().filter(|x| x.1 .1.abs() >= 0.02).take(14) {
            let mean = sum / total_living.max(1) as f64;
            br_rows.push(format!("| {sc} | {src} | {mean:+.3} | {max:+.3} ({who}) | {:+.1} / {:+.1} / {:+.1} |", max / 0.03, max / 0.05, max / 0.10));
        }
    }
    println!("## 1. Pressure writers in base: asked per living actor-tick, and the shift b / r of the largest per-actor rate\n");
    println!("| scenario | writer | mean over all actor-ticks | largest per-actor mean (actor) | b / r at r = 0.03 / 0.05 / 0.10 |");
    println!("|---|---|---|---|---|");
    for r in br_rows { println!("{r}"); }
    println!("\n## 2. The Ц6 measure and meaningfulness\n");
    println!("| scenario | world | model | at the ceiling (≥ 99) | actors under 30 % | after a neighbour dies (pressure then − 10 ticks later) | corr(pressure, T_p) | pressure ≥ 85 with T_p > 50 |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in measure_rows { println!("{r}"); }
    println!("\n## 3. The cascade (share of living actor-ticks)\n");
    println!("| scenario | world | model | legitimacy ≤ 1 | cohesion < 15 | eo < T / 2 | Rome depopulated (games) |");
    println!("|---|---|---|---|---|---|---|");
    for r in cascade_rows { println!("{r}"); }
    println!("\n## 4. Where deaths move: paired by seed against base\n");
    println!("| scenario | world | model | deaths | per seed | key actors | largest moves (deaths base → model) |");
    println!("|---|---|---|---|---|---|---|");
    for r in death_rows { println!("{r}"); }
    println!("\n## 5. Historical profile\n");
    println!("| scenario | world | model | |");
    println!("|---|---|---|---|");
    for r in hist_rows { println!("{r}"); }
    println!("\n## 6. For information: A10, A35, the split, B46 outcomes, the regency fork\n");
    println!("| scenario | world | model | wins | split on 40 | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|");
    for r in info_rows { println!("{r}"); }
}
