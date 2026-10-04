//! A46 stage 2 — census of readers: what saturation switches off (docs/TRIAGE.md). Measurement
//! only.
//!
//! Metrics: `economic_output`, `external_pressure`, `legitimacy`, `military_quality`,
//! `cohesion`, and rome's family `knowledge` and `influence`.
//!
//! **Readers, by construction.** The static list is built from the scenarios themselves and
//! from the engine's source: dependency rules with the metric as `from`; auto-delta conditions
//! and ratio valves; actions' `available_if` and cost; milestones, rank conditions, random-
//! event gates (the scenario's and the common pool); the victory; status indicators, key
//! metrics and the global-metric panel; and every engine line reading the metric by name
//! (`get_metric("…")`). **Measured** with `--features census`, at the point of evaluation:
//! threshold readers through the occupancy counter (living actors only — an evaluation on an
//! absent actor is not counted), dependency rules through their own annotation (active or
//! not, source at the boundary), formula readers through the read counter (share of reads
//! at the boundary, ≥ 99 or ≤ 1 — there the formula sees a constant), UI readers by the share
//! of living ticks spent in their single most frequent band.
//!
//! **Cross-check:** a call site the read counter finds that the static list does not name is
//! printed as UNLISTED — the list is incomplete.
//!
//! Classes for threshold readers, per world: always on (> 99 % true), always off (< 1 %),
//! live. A mechanism held on or off **in every world** is listed in the summary.
//!
//! Plus milan's legitimacy decomposed by source (the write sink of A37): the mirror case,
//! actors whose legitimacy rises.
//!
//! Usage: cargo run --release --features census --bin a46_readers_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::{census, ActionCondition, EventConditionType, MetricRef, RelativeMetricRef, Scenario};
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const METRICS: &[&str] = &["economic_output", "external_pressure", "legitimacy", "military_quality", "cohesion", "knowledge", "influence"];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

fn name_of(m: &MetricRef) -> String {
    match m {
        MetricRef::Actor { metric, .. } => metric.as_str().to_string(),
        MetricRef::Family { key } => key.as_str().trim_start_matches("family_").to_string(),
        MetricRef::Global { key } => key.as_str().to_string(),
    }
}

fn rel_name(m: &RelativeMetricRef) -> String {
    match m {
        RelativeMetricRef::SelfRelative(n) => n.as_str().to_string(),
        RelativeMetricRef::Absolute(r) => name_of(r),
    }
}

/// A threshold reader: the occupancy keys it answers to.
struct Reader { metric: String, kind: &'static str, id: String, ctx_prefix: String, ctx_suffix: String }

/// A UI reader: the metric and its bands (ascending lower bounds).
struct Ui { metric: String, kind: &'static str, id: String, key: MetricRef, bounds: Vec<f64> }

fn readers(sc: &Scenario) -> (Vec<Reader>, Vec<Ui>) {
    let mut r = Vec::new();
    let mut push = |metric: String, kind: &'static str, id: String, p: String, s: String| {
        if METRICS.contains(&metric.as_str()) { r.push(Reader { metric, kind, id, ctx_prefix: p, ctx_suffix: s }); }
    };
    for d in &sc.dependencies {
        push(d.from.as_str().to_string(), "dependency", format!("{} ({:?} {:?})", d.id, d.mode, d.threshold), format!("dependency {}", d.id), String::new());
    }
    for (i, ad) in sc.auto_deltas.iter().enumerate() {
        for c in &ad.conditions {
            push(name_of(&c.metric), "auto-delta condition", format!("[{i}] {} if {} {:?} {}", ad.metric, c.metric, c.operator, c.value),
                format!("auto_delta[{i}] {} | if {}", ad.metric, c.metric), String::new());
        }
        for rc in &ad.ratio_conditions {
            for m in [&rc.metric_a, &rc.metric_b] {
                push(name_of(m), "auto-delta ratio", format!("[{i}] {} if {} / {} {:?} {}", ad.metric, rc.metric_a, rc.metric_b, rc.operator, rc.ratio),
                    format!("auto_delta[{i}] {} | ratio {} / {}", ad.metric, rc.metric_a, rc.metric_b), String::new());
            }
        }
    }
    for a in &sc.patron_actions {
        if let ActionCondition::Metric { metric, operator, value } = &a.available_if {
            push(name_of(metric), "action available_if", format!("{} {:?} {}", a.id, operator, value), format!("action {} | available_if {}", a.id, metric), String::new());
        }
        for m in a.cost.keys() {
            push(name_of(m), "action cost", format!("{} cost {}", a.id, m), format!("action {} | cost {}", a.id, m), String::new());
        }
    }
    for m in &sc.milestone_events {
        if let EventConditionType::Metric { metric, operator, value, .. } = &m.condition.condition_type {
            push(name_of(metric), "milestone", format!("{} {:?} {}", m.id, operator, value), format!("milestone {}", m.id), String::new());
        }
    }
    for rc in &sc.rank_conditions {
        if let EventConditionType::Metric { metric, operator, value, .. } = &rc.condition.condition_type {
            push(name_of(metric), "rank condition", format!("{} {:?} {}", rc.region_id, operator, value), format!("rank_condition {}", rc.region_id), String::new());
        }
    }
    let mut events = engine13::events::common_events();
    events.extend(sc.random_events.iter().cloned());
    for e in &events {
        for c in &e.conditions {
            push(rel_name(&c.metric), "event gate", format!("{} if {} {:?} {}", e.id, c.metric, c.operator, c.value), format!("event {} @", e.id), format!("| if {}", c.metric));
        }
    }
    if let Some(v) = &sc.victory_condition {
        push(name_of(&v.metric), "victory", format!("{} >= {}", v.metric, v.threshold), format!("victory | {}", v.metric), String::new());
    }
    let mut ui = Vec::new();
    for i in &sc.status_indicators {
        ui.push(Ui { metric: name_of(&i.metric), kind: "status indicator", id: i.label.clone(), key: i.metric.clone(), bounds: i.thresholds.iter().map(|t| t.0).collect() });
    }
    for k in &sc.narrative_config.key_metrics {
        ui.push(Ui { metric: name_of(&k.metric), kind: "key metric (chronicler)", id: k.label.clone(), key: k.metric.clone(), bounds: k.bands.iter().map(|b| b.0).collect() });
    }
    for d in &sc.global_metrics_display {
        let mut b = vec![f64::MIN];
        b.extend(d.thresholds.iter().map(|t| t.below));
        ui.push(Ui { metric: name_of(&d.metric), kind: "global panel", id: d.label.clone(), key: d.metric.clone(), bounds: b });
    }
    ui.retain(|u| METRICS.contains(&u.metric.as_str()));
    (r, ui)
}

/// Engine lines that read a watched metric by name: `get_metric("…")` outside test modules.
fn engine_literal_sites() -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(es) = std::fs::read_dir(d) {
            for e in es.flatten() {
                let p = e.path();
                if p.is_dir() { walk(&p, out) } else if p.extension().is_some_and(|x| x == "rs") { out.push(p) }
            }
        }
    }
    let mut files = Vec::new();
    for d in ["src/engine", "src/application", "src/llm", "src/core"] { walk(std::path::Path::new(d), &mut files); }
    files.push("src/commands.rs".into());
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        let fp = f.to_string_lossy().replace('\\', "/");
        for (i, line) in src.lines().enumerate() {
            if line.contains("#[cfg(test)]") { break; }
            for m in METRICS {
                if line.contains(&format!("get_metric(\"{m}\")")) { out.insert((format!("{fp}:{}", i + 1), m.to_string())); }
            }
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let literal = engine_literal_sites();
    census::enable();
    census::enable_occupancy();
    census::occupancy_live_only(true);
    census::enable_reads();
    let mut out: Vec<String> = Vec::new();
    let mut summary: Vec<String> = Vec::new();
    let mut held: Vec<String> = Vec::new();
    let mut unlisted: BTreeMap<(String, String), (u64, u64)> = BTreeMap::new();
    let mut per_metric: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // readers, live
    let mut milan_legit: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new(); // (actor, source) -> asked
    for sc_id in ["rome_375", "constantinople_1430", "milan_1477"] {
        let scenario = engine13::scenarios::registry::load_by_id(sc_id).unwrap();
        let (rs, ui) = readers(&scenario);
        let ws_list = worlds(sc_id);
        let mut occ: Vec<census::Occupancy> = Vec::new();
        let mut reads: Vec<BTreeMap<(String, String), (u64, u64)>> = Vec::new();
        let mut ui_bands: Vec<HashMap<usize, Vec<u64>>> = Vec::new();
        for world in ws_list {
            let _ = census::take_occupancy();
            let _ = census::take_reads();
            let mut bands: HashMap<usize, Vec<u64>> = HashMap::new();
            if sc_id == "milan_1477" { census::enable_writes(); census::watch_all_metrics(true); }
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, sc_id.to_string()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, sc_id));
                let _ = census::take_writes();
                for _ in 0..ticks {
                    match &strategy {
                        Some(s) => { play_scripted_tick(&mut st, s); }
                        None => {
                            let ws = st.world_state.as_mut().unwrap();
                            let scn = st.current_scenario.as_ref().unwrap();
                            engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                        }
                    }
                    let _ = census::take();
                    let ws = st.world_state.as_ref().unwrap();
                    for (ui_i, u) in ui.iter().enumerate() {
                        let Some(v) = u.key.try_get(ws) else { continue };
                        let band = u.bounds.iter().rposition(|b| v >= *b).unwrap_or(0);
                        let e = bands.entry(ui_i).or_insert_with(|| vec![0; u.bounds.len().max(1)]);
                        if band < e.len() { e[band] += 1; }
                    }
                    if sc_id == "milan_1477" {
                        for w in census::take_writes() {
                            if w.metric == "legitimacy" && ws.actors.contains_key(&w.actor) {
                                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                                let e = milan_legit.entry((w.actor.clone(), src)).or_default();
                                e.0 += w.requested; e.1 += w.applied;
                            }
                        }
                    }
                }
            }
            if sc_id == "milan_1477" { census::watch_all_metrics(false); }
            occ.push(census::take_occupancy());
            reads.push(census::take_reads());
            ui_bands.push(bands);
        }
        // threshold readers
        out.push(format!("## {sc_id}\n\n### Threshold readers (share of evaluations true, living actors; per world: {})\n", ws_list.join(" · ")));
        out.push("| metric | kind | reader | per world | class |".into());
        out.push("|---|---|---|---|---|".into());
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for r in &rs {
            let key = format!("{}|{}|{}", r.kind, r.id, r.metric);
            if !seen.insert(key) { continue; }
            let mut cells = Vec::new();
            let mut classes = Vec::new();
            for o in &occ {
                let (t, n) = o.iter().filter(|((c, _), _)| c.starts_with(&r.ctx_prefix) && c.ends_with(&r.ctx_suffix) && (r.kind != "dependency" || c == &r.ctx_prefix))
                    .fold((0u64, 0u64), |a, (_, v)| (a.0 + v.0, a.1 + v.1));
                if n == 0 { cells.push("—".to_string()); classes.push("unread"); continue; }
                let p = 100.0 * t as f64 / n as f64;
                cells.push(format!("{p:.1} %"));
                classes.push(if p > 99.0 { "always on" } else if p < 1.0 { "always off" } else { "live" });
            }
            let read: Vec<&&str> = classes.iter().filter(|c| **c != "unread").collect();
            let class = if read.is_empty() {
                if r.kind == "event gate" { "not reached (an earlier gate of the event cuts it off)".to_string() } else { "never evaluated".to_string() }
            }
                else if read.iter().all(|c| **c == "always on") { "ALWAYS ON".into() }
                else if read.iter().all(|c| **c == "always off") { "ALWAYS OFF".into() }
                else if read.iter().all(|c| **c == "live") { "live".into() }
                else { classes.join(" / ") };
            let pm = per_metric.entry(r.metric.clone()).or_default();
            pm.0 += 1;
            if class == "live" { pm.1 += 1; }
            if class == "ALWAYS ON" || class == "ALWAYS OFF" || class.starts_with("never") || class.starts_with("not reached") {
                held.push(format!("| {sc_id} | {} | {} | {} | {class} |", r.metric, r.kind, r.id));
            }
            let extra = if r.kind == "dependency" {
                let b: Vec<String> = occ.iter().map(|o| o.iter().filter(|((c, _), _)| c == &format!("{} source", r.ctx_prefix))
                    .fold((0u64, 0u64), |a, (_, v)| (a.0 + v.0, a.1 + v.1))).map(|(t, n)| if n == 0 { "—".into() } else { format!("{:.0} %", 100.0 * t as f64 / n as f64) }).collect();
                format!(" (source at boundary: {})", b.join(" · "))
            } else { String::new() };
            out.push(format!("| {} | {} | {}{extra} | {} | {class} |", r.metric, r.kind, r.id, cells.join(" · ")));
        }
        // formula readers
        out.push("\n### Formula readers (engine call sites reading the metric; share of reads at the boundary ≥ 99 or ≤ 1)\n".to_string());
        out.push("| site | metric | reads per world | at boundary per world | listed |".into());
        out.push("|---|---|---|---|---|".into());
        let mut sites: BTreeSet<(String, String)> = BTreeSet::new();
        for rd in &reads { sites.extend(rd.keys().cloned()); }
        for (site, metric) in &sites {
            let rc: Vec<String> = reads.iter().map(|rd| rd.get(&(site.clone(), metric.clone())).map_or("—".into(), |v| v.0.to_string())).collect();
            let bc: Vec<String> = reads.iter().map(|rd| rd.get(&(site.clone(), metric.clone())).map_or("—".into(), |v| format!("{:.0} %", 100.0 * v.1 as f64 / v.0.max(1) as f64))).collect();
            let short = site.trim_start_matches("src/").to_string();
            let line = std::fs::read_to_string(site.split(':').next().unwrap()).ok()
                .and_then(|s| s.lines().nth(site.rsplit(':').next().unwrap().parse::<usize>().unwrap_or(1) - 1).map(|l| l.trim().to_string()))
                .unwrap_or_default();
            let generic = !line.contains("get_metric(\"");
            let listed = if literal.contains(&(site.clone(), metric.clone())) { "literal" } else if generic { "content-driven" } else { "UNLISTED" };
            if listed == "UNLISTED" {
                let e = unlisted.entry((site.clone(), metric.clone())).or_default();
                for rd in &reads { if let Some(v) = rd.get(&(site.clone(), metric.clone())) { e.0 += v.0; e.1 += v.1; } }
            }
            out.push(format!("| {short} | {metric} | {} | {} | {listed} |", rc.join(" · "), bc.join(" · ")));
            if listed == "literal" {
                let pm = per_metric.entry(metric.clone()).or_default();
                pm.0 += 1;
                let all_bound = reads.iter().all(|rd| rd.get(&(site.clone(), metric.clone())).is_none_or(|v| v.0 == 0 || v.1 as f64 / v.0 as f64 > 0.99));
                if all_bound {
                    held.push(format!("| {sc_id} | {metric} | engine formula | {short} | sees a constant (> 99 % of reads at the boundary in every world) |"));
                } else { pm.1 += 1; }
            }
        }
        // UI readers
        out.push("\n### UI readers (share of living ticks in the single most frequent band)\n".to_string());
        out.push("| metric | kind | reader | per world | |".into());
        out.push("|---|---|---|---|---|".into());
        for (i, u) in ui.iter().enumerate() {
            let cells: Vec<f64> = ui_bands.iter().map(|b| b.get(&i).map_or(0.0, |v| { let s: u64 = v.iter().sum(); 100.0 * *v.iter().max().unwrap_or(&0) as f64 / s.max(1) as f64 })).collect();
            let constant = cells.iter().all(|c| *c > 99.0);
            let pm = per_metric.entry(u.metric.clone()).or_default();
            pm.0 += 1;
            if !constant { pm.1 += 1; } else { held.push(format!("| {sc_id} | {} | {} | {} | shows one band (> 99 % of ticks in every world) |", u.metric, u.kind, u.id)); }
            out.push(format!("| {} | {} | {} | {} | {} |", u.metric, u.kind, u.id, cells.iter().map(|c| format!("{c:.0} %")).collect::<Vec<_>>().join(" · "), if constant { "constant" } else { "" }));
        }
        out.push(String::new());
    }
    summary.push(format!("# A46 stage 2 — readers of saturated metrics, {seeds} seeds × {ticks} ticks per world\n"));
    summary.push("## Readers per metric (static list; «live» = varies in every world: threshold readers 1–99 % true, formula sites ≤ 99 % of reads at the boundary somewhere, UI not stuck in one band)\n".into());
    summary.push("| metric | readers | live |".into());
    summary.push("|---|---|---|".into());
    for (m, (n, l)) in &per_metric { summary.push(format!("| {m} | {n} | {l} |")); }
    summary.push("\n## Mechanisms the saturation holds on or off in every world\n".into());
    summary.push("| scenario | metric | kind | reader | state |".into());
    summary.push("|---|---|---|---|---|".into());
    summary.extend(held);
    summary.push(format!("\n## Cross-check: call sites reading a watched metric that the static list does not name: {}\n", unlisted.len()));
    for ((s, m), (n, b)) in &unlisted { summary.push(format!("- UNLISTED {s} {m}: {n} reads, {:.0} % at the boundary", 100.0 * *b as f64 / (*n).max(1) as f64)); }
    summary.push("\n## milan: legitimacy by source (asked over living ticks, both worlds; actors sorted by net)\n".into());
    let mut by_actor: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
    for ((a, s), (asked, _)) in &milan_legit { by_actor.entry(a.clone()).or_default().push((s.clone(), *asked)); }
    let mut actors: Vec<(String, f64)> = by_actor.iter().map(|(a, v)| (a.clone(), v.iter().map(|x| x.1).sum())).collect();
    actors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    for (a, net) in actors {
        let mut v = by_actor[&a].clone();
        v.sort_by(|x, y| y.1.abs().partial_cmp(&x.1.abs()).unwrap());
        let top: Vec<String> = v.iter().take(6).map(|(s, x)| format!("{} {:+.0}", s.trim_start_matches("src/"), x)).collect();
        summary.push(format!("- **{a}** net asked {net:+.0}: {}", top.join(", ")));
    }
    for l in summary { println!("{l}"); }
    println!();
    for l in out { println!("{l}"); }
}
