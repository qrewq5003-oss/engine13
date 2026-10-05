//! The chain behind Rome's depopulation in v2 (docs/economy_project_brief.md §9.7, owner's
//! hypothesis): pressure at the ceiling → legitimacy at the floor → the cycle to the cohesion
//! floor (A19) → economy far below its norm (v2) → depopulation (Ц2). Measurement only.
//! Built with `--features census`. v2 as at `2a5b436`; every world, 30 seeds × 300 ticks.
//!
//! For Rome, Byzantium and the three most suffering actors of each scenario (the largest share
//! of living ticks with `economic_output` below half its target T, Rome and Byzantium apart):
//! legitimacy and cohesion sources per living tick (asked), and four worlds:
//! - base (v2 as is);
//! - (а) pressure held at 50: every actor's `external_pressure` set to 50 at the end of each tick,
//!   so the rules reading it in the next tick's dependency phase see ≈ 50 and the `excess` rules
//!   from pressure stay silent (tags and auto-deltas still move it within the tick);
//! - (б) `external_pressure_to_legitimacy` removed from the scenario;
//! - (в) `cohesion_to_economic_output` removed from the scenario.
//!
//! Per world: the share of living ticks at the legitimacy floor (≤ 1), at the cohesion floor
//! (≤ 1) and in the cohesion collapse gate (< 15), the share with `economic_output` < T / 2, Rome's
//! depopulation (games where Rome reaches population ≤ 1, its population on tick 299), and deaths
//! paired by seed against base (all and key actors).
//!
//! Usage: cargo run --release --features census --bin chain_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Cf { Base, PressureAt50, NoPressureToLegit, NoCohesionToEo }

impl Cf {
    fn label(&self) -> &'static str {
        match self {
            Cf::Base => "base",
            Cf::PressureAt50 => "(а) pressure held at 50",
            Cf::NoPressureToLegit => "(б) no external_pressure_to_legitimacy",
            Cf::NoCohesionToEo => "(в) no cohesion_to_economic_output",
        }
    }
}

#[derive(Default, Clone)]
struct Acc {
    living: u64,
    legit_floor: u64,
    coh_floor: u64,
    coh_gate: u64,
    eo_low: u64,
}

#[derive(Default)]
struct Run {
    acc: BTreeMap<String, Acc>,
    // (actor, metric, source) -> asked
    sources: BTreeMap<(String, String, String), f64>,
    deaths: u32,
    key_dead: BTreeMap<String, bool>,
    rome_zombie: bool,
    rome_pop_end: Option<f64>,
}

fn run(sc: &str, world: &str, cf: Cf, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        match cf {
            Cf::NoPressureToLegit => s.dependencies.retain(|d| d.id != "external_pressure_to_legitimacy"),
            Cf::NoCohesionToEo => s.dependencies.retain(|d| d.id != "cohesion_to_economic_output"),
            _ => {}
        }
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
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
        for w in census::take_writes() {
            if w.metric == "legitimacy" || w.metric == "cohesion" {
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                *r.sources.entry((w.actor.clone(), w.metric.clone(), src)).or_default() += w.requested;
            }
        }
        let scn = st.current_scenario.as_ref().unwrap().clone();
        let ws = st.world_state.as_mut().unwrap();
        let mut ids: Vec<String> = ws.actors.keys().cloned().collect();
        ids.sort();
        for id in &ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let e = r.acc.entry(id.clone()).or_default();
            e.living += 1;
            if a.get_metric("legitimacy") <= 1.0 { e.legit_floor += 1; }
            if a.get_metric("cohesion") <= 1.0 { e.coh_floor += 1; }
            if a.get_metric("cohesion") < 15.0 { e.coh_gate += 1; }
            if let Some(t) = engine13::engine::eo_target(ws, &scn, id) {
                if a.get_metric("economic_output") < t / 2.0 { e.eo_low += 1; }
            }
            if id == "rome" && a.get_metric("population") <= 1.0 { r.rome_zombie = true; }
        }
        if cf == Cf::PressureAt50 {
            for a in ws.actors.values_mut() { a.set_metric("external_pressure", 50.0); }
        }
    }
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    for k in KEY {
        if st.current_scenario.as_ref().unwrap().actors.iter().any(|a| a.id == k && !a.is_successor_template) {
            r.key_dead.insert(k.to_string(), ws.dead_actor_ids.contains(k));
        }
    }
    r.rome_pop_end = ws.actors.get("rome").filter(|_| !ws.dead_actor_ids.contains("rome")).map(|a| a.get_metric("population"));
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    println!("# The chain behind Rome's depopulation in v2 (2a5b436), {seeds} seeds × {ticks} ticks per world\n");
    let cfs = [Cf::Base, Cf::PressureAt50, Cf::NoPressureToLegit, Cf::NoCohesionToEo];
    let mut share_rows = Vec::new();
    let mut world_rows = Vec::new();
    let mut source_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        // runs per (world, cf)
        let mut all: BTreeMap<(String, &'static str), Vec<Run>> = BTreeMap::new();
        for world in worlds(sc) {
            for cf in cfs {
                all.insert((world.to_string(), cf.label()), (0..seeds).map(|s| run(sc, world, cf, s, ticks)).collect());
            }
        }
        // the three most suffering actors (base, pooled over worlds): largest share eo < T/2
        let mut pool: BTreeMap<String, Acc> = BTreeMap::new();
        for world in worlds(sc) {
            for r in &all[&(world.to_string(), Cf::Base.label())] {
                for (k, a) in &r.acc { let e = pool.entry(k.clone()).or_default(); e.living += a.living; e.eo_low += a.eo_low; }
            }
        }
        let mut suffering: Vec<(String, f64)> = pool.iter().filter(|(k, _)| k.as_str() != "rome" && k.as_str() != "byzantium")
            .map(|(k, a)| (k.clone(), a.eo_low as f64 / a.living.max(1) as f64)).collect();
        suffering.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let mut watch: BTreeSet<String> = suffering.iter().take(3).map(|x| x.0.clone()).collect();
        for k in ["rome", "byzantium"] { if pool.contains_key(k) { watch.insert(k.to_string()); } }
        for world in worlds(sc) {
            let base = &all[&(world.to_string(), Cf::Base.label())];
            for cf in cfs {
                let runs = &all[&(world.to_string(), cf.label())];
                for id in &watch {
                    let mut a = Acc::default();
                    for r in runs.iter() { if let Some(x) = r.acc.get(id) { a.living += x.living; a.legit_floor += x.legit_floor; a.coh_floor += x.coh_floor; a.coh_gate += x.coh_gate; a.eo_low += x.eo_low; } }
                    if a.living == 0 { continue; }
                    let p = |x: u64| 100.0 * x as f64 / a.living as f64;
                    share_rows.push(format!("| {sc} | {world} | {id} | {} | {:.0} % | {:.0} % | {:.0} % | {:.0} % |", cf.label(), p(a.legit_floor), p(a.coh_floor), p(a.coh_gate), p(a.eo_low)));
                }
                let keys: Vec<String> = KEY.iter().filter(|k| runs[0].key_dead.contains_key(**k)).map(|k| format!("{k} {} → {} {}",
                    base.iter().filter(|r| r.key_dead[*k]).count(), runs.iter().filter(|r| r.key_dead[*k]).count(),
                    paired(base, runs, |r| if r.key_dead[*k] { 1.0 } else { 0.0 }))).collect();
                let rome = if sc == "rome_375" {
                    let mut pops: Vec<f64> = runs.iter().filter_map(|r| r.rome_pop_end).collect();
                    pops.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    format!("{} / {seeds}; p50 {:.0}", runs.iter().filter(|r| r.rome_zombie).count(), pops.get(pops.len() / 2).copied().unwrap_or(f64::NAN))
                } else { "—".into() };
                world_rows.push(format!("| {sc} | {world} | {} | {} | {} | {rome} | {} |", cf.label(), runs.iter().map(|r| r.deaths).sum::<u32>(), paired(base, runs, |r| r.deaths as f64), keys.join("; ")));
            }
            // sources in base for the watched actors
            for id in &watch {
                for metric in ["legitimacy", "cohesion"] {
                    let mut agg: BTreeMap<String, f64> = BTreeMap::new();
                    let mut living = 0u64;
                    for r in base.iter() {
                        living += r.acc.get(id).map_or(0, |a| a.living);
                        for ((a, m, s), x) in &r.sources { if a == id && m == metric { *agg.entry(s.clone()).or_default() += x; } }
                    }
                    if living == 0 { continue; }
                    let mut v: Vec<(String, f64)> = agg.into_iter().map(|(k, x)| (k, x / living as f64)).collect();
                    v.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());
                    let cells: Vec<String> = v.iter().filter(|x| x.1.abs() >= 0.01).take(7).map(|(k, x)| format!("{k} {x:+.2}")).collect();
                    source_rows.push(format!("| {sc} | {world} | {id} | {metric} | {} |", cells.join(", ")));
                }
            }
        }
    }
    println!("## 1. Floors and the economy below half its norm, per watched actor (share of its living ticks)\n");
    println!("| scenario | world | actor | world model | legitimacy ≤ 1 | cohesion ≤ 1 | cohesion < 15 | eo < T / 2 |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in share_rows { println!("{r}"); }
    println!("\n## 2. Deaths and Rome's depopulation, paired by seed against base\n");
    println!("| scenario | world | model | deaths | paired per seed | Rome population ≤ 1 (games); population on tick 299 | key actors |");
    println!("|---|---|---|---|---|---|---|");
    for r in world_rows { println!("{r}"); }
    println!("\n## 3. Legitimacy and cohesion sources per living tick (asked), base\n");
    println!("| scenario | world | actor | metric | sources |");
    println!("|---|---|---|---|---|");
    for r in source_rows { println!("{r}"); }
}
