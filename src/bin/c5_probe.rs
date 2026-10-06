//! Economy project, Ц5 stage 1: legitimacy and the upper link of the cascade — diagnosis, no
//! pre-commitment (docs/economy_project_brief.md §9). Built with `--features census`. v2 with Ц1 and
//! Ц6 as in the content; every world, 30 seeds × 300 ticks.
//!
//! 1. Ц5's measure per actor: the share of living ticks with legitimacy at the floor (≤ 1) and at
//!    the ceiling (≥ 99) — over all living ticks and over the ticks without a crisis tag (a tag with a
//!    negative `legitimacy` modifier, brief §9 (б)).
//! 2. The writers of legitimacy and cohesion: the rate (asked per living actor-tick), the actor with
//!    the largest rate, and for dependency rules the state of the source (its value at the end of
//!    the previous tick, in bins of 10) at which a single write is largest.
//! 3. Counterfactuals: without `external_pressure_to_legitimacy`, without
//!    `siege_rally_cohesion_bonus`, without both — the share of living ticks with cohesion < 15 and
//!    with `economic_output` < T / 2, Rome's depopulation, deaths paired by seed against base, and
//!    Ц5's measure.
//! 4. For information (§9.6, no gate): the §9.2 rows.
//!
//! Usage: cargo run --release --features census --bin c5_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];
const P2L: &str = "external_pressure_to_legitimacy";
const RALLY: &str = "siege_rally_cohesion_bonus";

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Cf { Base, NoP2L, NoRally, NoBoth }

impl Cf {
    fn label(&self) -> &'static str {
        match self {
            Cf::Base => "base (v2 with Ц1 + Ц6)",
            Cf::NoP2L => "without external_pressure_to_legitimacy",
            Cf::NoRally => "without siege_rally_cohesion_bonus",
            Cf::NoBoth => "without both",
        }
    }
    fn drops(&self, id: &str) -> bool {
        match self {
            Cf::Base => false,
            Cf::NoP2L => id == P2L,
            Cf::NoRally => id == RALLY,
            Cf::NoBoth => id == P2L || id == RALLY,
        }
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

#[derive(Default, Clone)]
struct Acc {
    living: u64,
    floor: u64,
    ceiling: u64,
    calm: u64,
    calm_floor: u64,
    calm_ceiling: u64,
    coh_gate: u64,
    eo_low: u64,
}

#[derive(Default)]
struct Run {
    acc: BTreeMap<String, Acc>,
    /// (actor, metric, source) -> asked
    sources: BTreeMap<(String, String, String), f64>,
    /// (metric, dependency id, bin of the source value) -> (writes, asked)
    bins: BTreeMap<(String, String, i32), (u64, f64)>,
    deaths: u32,
    dead: BTreeMap<String, u32>,
    rome_zombie: bool,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

fn run(sc: &str, world: &str, cf: Cf, seed: u64, ticks: u32, with_sources: bool) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        let before = s.dependencies.len();
        s.dependencies.retain(|d| !cf.drops(&d.id));
        let dropped = before - s.dependencies.len();
        assert_eq!(dropped, match cf { Cf::Base => 0, Cf::NoBoth => 2, _ => 1 }, "{sc}: the rules to drop are in the content");
    }
    let from: BTreeMap<String, String> = st.current_scenario.as_ref().unwrap().dependencies.iter()
        .map(|d| (d.id.clone(), d.from.as_str().to_string())).collect();
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    // the state each dependency read: the actor's metrics at the end of the previous tick
    let mut prev: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let _ = census::take_writes();
    for _ in 0..ticks {
        let before: BTreeSet<String> = st.world_state.as_ref().unwrap().dead_actor_ids.iter().cloned().collect();
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
                if w.metric != "legitimacy" && w.metric != "cohesion" { continue; }
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                *r.sources.entry((w.actor.clone(), w.metric.clone(), src.clone())).or_default() += w.requested;
                if let Some(id) = src.strip_prefix("dependency ") {
                    if let Some(v) = from.get(id).and_then(|m| prev.get(&w.actor).and_then(|p| p.get(m))) {
                        let bin = ((v / 10.0).floor() as i32).clamp(0, 30);
                        let e = r.bins.entry((w.metric.clone(), id.to_string(), bin)).or_default();
                        e.0 += 1;
                        e.1 += w.requested;
                    }
                }
            }
        }
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) { r.dead.insert(d.clone(), t); }
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let l = a.get_metric("legitimacy");
            let crisis = a.actor_tags.values().any(|tag| tag.metrics_modifier.iter().any(|(m, v)| m.as_str() == "legitimacy" && *v < 0));
            let e = r.acc.entry(id.clone()).or_default();
            e.living += 1;
            if l <= 1.0 { e.floor += 1; }
            if l >= 99.0 { e.ceiling += 1; }
            if !crisis {
                e.calm += 1;
                if l <= 1.0 { e.calm_floor += 1; }
                if l >= 99.0 { e.calm_ceiling += 1; }
            }
            if a.get_metric("cohesion") < 15.0 { e.coh_gate += 1; }
            if engine13::engine::eo_target(ws, scn, id).is_some_and(|tg| a.get_metric("economic_output") < tg / 2.0) { e.eo_low += 1; }
            if id == "rome" && a.get_metric("population") <= 1.0 { r.rome_zombie = true; }
            prev.insert(id.clone(), a.metrics.iter().map(|(k, v)| (k.clone(), *v)).collect());
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

fn paired(a: &[Run], b: &[Run], f: impl Fn(&Run) -> f64) -> String {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| f(y) - f(x)).collect();
    let n = d.len() as f64;
    let mean = d.iter().sum::<f64>() / n;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
    format!("{mean:+.2} (t {t:+.1})")
}

fn pool(runs: &[Run]) -> BTreeMap<String, Acc> {
    let mut p: BTreeMap<String, Acc> = BTreeMap::new();
    for r in runs {
        for (k, a) in &r.acc {
            let e = p.entry(k.clone()).or_default();
            e.living += a.living; e.floor += a.floor; e.ceiling += a.ceiling; e.calm += a.calm;
            e.calm_floor += a.calm_floor; e.calm_ceiling += a.calm_ceiling; e.coh_gate += a.coh_gate; e.eo_low += a.eo_low;
        }
    }
    p
}

fn share(x: u64, n: u64) -> f64 { 100.0 * x as f64 / n.max(1) as f64 }

/// (actors passing on calm ticks, actors with calm ticks, the failing ones)
fn c5_measure(p: &BTreeMap<String, Acc>) -> (usize, usize, Vec<String>) {
    let calm: Vec<(&String, &Acc)> = p.iter().filter(|(_, a)| a.calm > 0).collect();
    let failing: Vec<String> = calm.iter().filter(|(_, a)| share(a.calm_floor, a.calm) >= 20.0 || share(a.calm_ceiling, a.calm) >= 20.0)
        .map(|(k, a)| format!("{k} ({:.0} / {:.0} %)", share(a.calm_floor, a.calm), share(a.calm_ceiling, a.calm))).collect();
    (calm.len() - failing.len(), calm.len(), failing)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    println!("# Ц5 stage 1 — legitimacy and the upper link of the cascade, v2 with Ц1 + Ц6, {seeds} seeds × {ticks} ticks per world\n");
    let cfs = [Cf::Base, Cf::NoP2L, Cf::NoRally, Cf::NoBoth];
    let mut measure_rows = Vec::new();
    let mut actor_rows = Vec::new();
    let mut writer_rows = Vec::new();
    let mut cf_rows = Vec::new();
    let mut death_rows = Vec::new();
    let mut info_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let mut src_pool: BTreeMap<(String, String, String), f64> = BTreeMap::new();
        let mut bin_pool: BTreeMap<(String, String, i32), (u64, f64)> = BTreeMap::new();
        let mut living_pool: BTreeMap<String, u64> = BTreeMap::new();
        for world in worlds(sc) {
            let base: Vec<Run> = (0..seeds).map(|s| run(sc, world, Cf::Base, s, ticks, true)).collect();
            for rr in &base {
                for (k, x) in &rr.sources { *src_pool.entry(k.clone()).or_default() += x; }
                for (k, x) in &rr.bins { let e = bin_pool.entry(k.clone()).or_default(); e.0 += x.0; e.1 += x.1; }
                for (k, a) in &rr.acc { *living_pool.entry(k.clone()).or_default() += a.living; }
            }
            // per-actor detail on base
            let p = pool(&base);
            let mut v: Vec<(&String, &Acc)> = p.iter().collect();
            v.sort_by(|a, b| (share(b.1.floor, b.1.living) + share(b.1.ceiling, b.1.living)).partial_cmp(&(share(a.1.floor, a.1.living) + share(a.1.ceiling, a.1.living))).unwrap());
            let cells: Vec<String> = v.iter().take(8).map(|(k, a)| format!("{k} {:.0} / {:.0} % (calm {:.0} / {:.0} %, crisis ticks {:.0} %)",
                share(a.floor, a.living), share(a.ceiling, a.living), share(a.calm_floor, a.calm), share(a.calm_ceiling, a.calm), 100.0 - share(a.calm, a.living))).collect();
            actor_rows.push(format!("| {sc} | {world} | {} |", cells.join("; ")));
            for cf in cfs {
                let fresh: Vec<Run>;
                let runs: &[Run] = if cf == Cf::Base { &base } else { fresh = (0..seeds).map(|s| run(sc, world, cf, s, ticks, false)).collect(); &fresh };
                let p = pool(runs);
                let tot = p.values().fold(Acc::default(), |mut s, a| {
                    s.living += a.living; s.floor += a.floor; s.ceiling += a.ceiling; s.coh_gate += a.coh_gate; s.eo_low += a.eo_low; s
                });
                let (pass, n, failing) = c5_measure(&p);
                measure_rows.push(format!("| {sc} | {world} | {} | {:.0} % | {:.0} % | {pass} / {n} | {} |", cf.label(),
                    share(tot.floor, tot.living), share(tot.ceiling, tot.living), if failing.is_empty() { "—".into() } else { failing.join(", ") }));
                let rome = if sc == "rome_375" { format!("{} / {seeds}", runs.iter().filter(|r| r.rome_zombie).count()) } else { "—".into() };
                cf_rows.push(format!("| {sc} | {world} | {} | {:.0} % | {:.0} % | {:.0} % | {rome} |", cf.label(),
                    share(tot.floor, tot.living), share(tot.coh_gate, tot.living), share(tot.eo_low, tot.living)));
                let keys: Vec<String> = KEY.iter().filter(|k| base[0].acc.contains_key(**k)).map(|k| {
                    format!("{k} {}→{} ({})", base.iter().filter(|r| r.dead.contains_key(*k)).count(), runs.iter().filter(|r| r.dead.contains_key(*k)).count(),
                        paired(&base, runs, |r| if r.dead.contains_key(*k) { 1.0 } else { 0.0 }))
                }).collect();
                death_rows.push(format!("| {sc} | {world} | {} | {} | {} | {} |", cf.label(), runs.iter().map(|r| r.deaths).sum::<u32>(),
                    paired(&base, runs, |r| r.deaths as f64), keys.join("; ")));
                let wins: Vec<f64> = runs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
                let mut perw: BTreeMap<u32, u32> = BTreeMap::new();
                for t in &wins { *perw.entry(*t as u32).or_default() += 1; }
                let top = perw.values().max().copied().unwrap_or(0);
                let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let wins_s = if wins.is_empty() { "0".into() } else { format!("{} ({}; 40–43: {on}; top {:.0} %)", wins.len(), q(&wins), 100.0 * top as f64 / wins.len() as f64) };
                let mut oc: BTreeMap<String, u32> = BTreeMap::new();
                for r in runs { for o in &r.outcomes { *oc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                let outc = if oc.is_empty() { "—".into() } else { oc.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ") };
                let fork = if sc == "milan_1477" { format!("{} / {}", runs.iter().filter(|r| r.stab).count(), runs.iter().filter(|r| r.deep).count()) } else { "—".into() };
                let split = if sc == "rome_375" { runs.iter().filter(|r| r.split40).count().to_string() } else { "—".into() };
                let hist = match (sc, *world) {
                    ("rome_375", _) => format!("Rome dies {} / {seeds}", runs.iter().filter(|r| r.dead.contains_key("rome")).count()),
                    ("constantinople_1430", "none") => {
                        let ft: Vec<f64> = runs.iter().filter_map(|r| r.dead.get("byzantium").map(|t| *t as f64)).collect();
                        format!("Byzantium falls {} / {seeds}, tick {}", ft.len(), q(&ft))
                    }
                    ("milan_1477", _) => format!("Milan dies {} / {seeds}", runs.iter().filter(|r| r.dead.contains_key("milan")).count()),
                    _ => "—".into(),
                };
                info_rows.push(format!("| {sc} | {world} | {} | {hist} | {wins_s} | {split} | {outc} | {fork} |", cf.label()));
            }
        }
        // writers of legitimacy and cohesion, base, pooled over the scenario's worlds
        let total_living: u64 = living_pool.values().sum();
        for metric in ["legitimacy", "cohesion"] {
            let mut by_src: BTreeMap<String, (f64, f64, String)> = BTreeMap::new();
            for ((actor, m, src), x) in &src_pool {
                if m != metric { continue; }
                let e = by_src.entry(src.clone()).or_insert((0.0, 0.0, String::new()));
                e.0 += x;
                let per_actor = x / living_pool.get(actor).copied().unwrap_or(1).max(1) as f64;
                if per_actor.abs() > e.1.abs() { e.1 = per_actor; e.2 = actor.clone(); }
            }
            let mut v: Vec<(String, (f64, f64, String))> = by_src.into_iter().collect();
            v.sort_by(|a, b| b.1 .0.abs().partial_cmp(&a.1 .0.abs()).unwrap());
            for (src, (sum, max, who)) in v.iter().take(12) {
                let state = src.strip_prefix("dependency ").map(|id| {
                    let bins: Vec<(i32, (u64, f64))> = bin_pool.iter().filter(|((m, d, _), _)| m == metric && d == id).map(|((_, _, b), x)| (*b, *x)).collect();
                    let writes: u64 = bins.iter().map(|b| b.1 .0).sum();
                    bins.iter().filter(|b| b.1 .0 * 50 >= writes).max_by(|a, b| (a.1 .1 / a.1 .0 as f64).abs().partial_cmp(&(b.1 .1 / b.1 .0 as f64).abs()).unwrap())
                        .map(|(b, (n, x))| format!("{}–{}: {:+.2} a write ({:.0} % of writes)", b * 10, b * 10 + 10, x / *n as f64, 100.0 * *n as f64 / writes.max(1) as f64))
                        .unwrap_or_else(|| "—".into())
                }).unwrap_or_else(|| "—".into());
                writer_rows.push(format!("| {sc} | {metric} | {src} | {:+.3} | {max:+.3} ({who}) | {state} |", sum / total_living.max(1) as f64));
            }
        }
    }
    println!("## 1. Ц5's measure (crisis tag = a tag with a negative legitimacy modifier; calm = ticks without one)\n");
    println!("| scenario | world | model | legitimacy ≤ 1, all actor-ticks | ≥ 99 | actors passing on calm ticks (floor and ceiling < 20 %) | failing (calm floor / ceiling) |");
    println!("|---|---|---|---|---|---|---|");
    for r in measure_rows.iter().filter(|r| r.contains(Cf::Base.label())) { println!("{r}"); }
    println!("\n### Actors with the most ticks at the floor or the ceiling (base; all ticks floor / ceiling)\n");
    println!("| scenario | world | actors |");
    println!("|---|---|---|");
    for r in actor_rows { println!("{r}"); }
    println!("\n## 2. Writers of legitimacy and cohesion (base, pooled over the scenario's worlds; asked per living actor-tick)\n");
    println!("For a dependency rule, the state is its source's value at the end of the previous tick, in bins of 10 (bins with ≥ 2 % of the writes): the bin with the largest write.\n");
    println!("| scenario | metric | writer | mean per actor-tick | largest per-actor mean (actor) | source state at the largest write |");
    println!("|---|---|---|---|---|---|");
    for r in writer_rows { println!("{r}"); }
    println!("\n## 3. Counterfactuals\n");
    println!("| scenario | world | model | legitimacy ≤ 1 | cohesion < 15 | eo < T / 2 | Rome depopulated (games) |");
    println!("|---|---|---|---|---|---|---|");
    for r in cf_rows { println!("{r}"); }
    println!("\n### Deaths paired by seed against base\n");
    println!("| scenario | world | model | deaths | per seed | key actors (games, paired) |");
    println!("|---|---|---|---|---|---|");
    for r in death_rows { println!("{r}"); }
    println!("\n### Ц5's measure under each counterfactual\n");
    println!("| scenario | world | model | legitimacy ≤ 1 | ≥ 99 | actors passing on calm ticks | failing |");
    println!("|---|---|---|---|---|---|---|");
    for r in measure_rows { println!("{r}"); }
    println!("\n## 4. For information (§9.2, no gate)\n");
    println!("| scenario | world | model | key actor | wins | split on 40 | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in info_rows { println!("{r}"); }
}
