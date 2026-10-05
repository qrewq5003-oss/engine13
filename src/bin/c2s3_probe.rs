//! Economy project, Ц2 stage 3: states without people, and the debt rule by the corrected
//! pre-commitment (docs/economy_project_brief.md §9). Every world of the three scenarios,
//! 30 seeds × 300 ticks. No census feature needed.
//!
//! 1. **Zombie census, v1 and v2:** actors living with population ≤ 1 — how many, which, how many
//!    ticks per world. Present in v1 → a v1 defect too (recorded in column B, not fixed in v1).
//! 2. **«Depopulation is collapse»** (v2, `economy_v2_depopulation_ticks` = 4): population ≤ 1 four
//!    ticks in a row collapses the state by the usual path. Deaths paired against v2 at
//!    `c2b6a10` (rule off), key actors, the split on tick 40, B46 outcomes.
//! 3. **The debt rule again** (x = 0.05, 0.10, 0.20) with depopulation on, by the corrected
//!    pre-commitment: (1) every world — the p90 debt spell ≤ 12 ticks, zombie ticks
//!    (population ≤ 1) not counted; (2) worlds without a player — no actor in debt on more than
//!    20 % of its living ticks; (3) played worlds — the share only printed; (4) no key actor's
//!    deaths rising significantly (t < 3) against v2 at `c2b6a10`.
//!
//! Usage: cargo run --release --bin c2s3_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;
use std::collections::BTreeMap;

const CUTS: [f64; 3] = [0.05, 0.10, 0.20];
const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];
const DEPOP: u32 = 4;

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

#[derive(Clone, Copy, PartialEq)]
enum Model { V1, V2, Depop(Option<f64>) }

impl Model {
    fn label(&self) -> String {
        match self {
            Model::V1 => "v1".into(),
            Model::V2 => "v2 (c2b6a10)".into(),
            Model::Depop(None) => "v2 + depopulation".into(),
            Model::Depop(Some(x)) => format!("v2 + depopulation, debt x = {:.0} %", x * 100.0),
        }
    }
}

#[derive(Default)]
struct Run {
    // actor -> zombie ticks (population ≤ 1 while alive)
    zombies: BTreeMap<String, u32>,
    // actor -> (counted ticks, ticks in debt), zombie ticks not counted
    debt: BTreeMap<String, (u64, u64)>,
    spells: Vec<f64>,
    deaths: u32,
    key_dead: BTreeMap<String, bool>,
    split40: bool,
    outcomes: Vec<String>,
}

fn run(sc: &str, world: &str, m: Model, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = m != Model::V1;
        s.economy_v2_depopulation_ticks = matches!(m, Model::Depop(_)).then_some(DEPOP);
        s.economy_v2_debt_cut = if let Model::Depop(x) = m { x } else { None };
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut open: BTreeMap<String, u32> = BTreeMap::new();
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
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            if a.get_metric("population") <= 1.0 {
                *r.zombies.entry(id.clone()).or_default() += 1;
                if let Some(n) = open.remove(id) { r.spells.push(n as f64); }
                continue;
            }
            let d = r.debt.entry(id.clone()).or_default();
            d.0 += 1;
            if a.get_metric("treasury") < 0.0 {
                d.1 += 1;
                *open.entry(id.clone()).or_default() += 1;
            } else if let Some(n) = open.remove(id) {
                r.spells.push(n as f64);
            }
        }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    r.spells.extend(open.values().map(|n| *n as f64));
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    for k in KEY {
        if st.current_scenario.as_ref().unwrap().actors.iter().any(|a| a.id == k && !a.is_successor_template) {
            r.key_dead.insert(k.to_string(), ws.dead_actor_ids.contains(k));
        }
    }
    r.outcomes = ws.milestone_events_fired.iter().filter(|m| m.starts_with("outcome_")).cloned().collect();
    r
}

fn paired(a: &[Run], b: &[Run], f: impl Fn(&Run) -> f64) -> (f64, f64) {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| f(y) - f(x)).collect();
    let n = d.len() as f64;
    let mean = d.iter().sum::<f64>() / n;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else if mean > 0.0 { f64::INFINITY } else { 0.0 };
    (mean, t)
}

fn key_worst_t(base: &[Run], runs: &[Run]) -> (f64, String) {
    let mut worst = f64::NEG_INFINITY;
    let mut cells = Vec::new();
    for k in KEY {
        if runs.first().is_some_and(|r| r.key_dead.contains_key(k)) {
            let (mean, t) = paired(base, runs, |r| if r.key_dead[k] { 1.0 } else { 0.0 });
            worst = worst.max(t);
            cells.push(format!("{k} {} → {} (t {t:+.1})",
                base.iter().filter(|r| r.key_dead[k]).count(), runs.iter().filter(|r| r.key_dead[k]).count()));
            let _ = mean;
        }
    }
    (worst, cells.join("; "))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("# Ц2 stage 3 — zombies, depopulation as collapse, and the debt rule by the corrected pre-commitment, {seeds} seeds × {ticks} ticks per world\n");
    let mut zombie_rows = Vec::new();
    let mut depop_rows = Vec::new();
    let mut debt_rows = Vec::new();
    let mut depop_key_ok = true;
    // x -> list of (world is no-player, p90, worst share, worst key t)
    let mut verdict: BTreeMap<String, Vec<(bool, f64, f64, f64)>> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let v1: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::V1, s, ticks)).collect();
            let v2: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::V2, s, ticks)).collect();
            for (label, runs) in [("v1", &v1), ("v2", &v2)] {
                let mut per: BTreeMap<String, (u32, u32)> = BTreeMap::new(); // actor -> (games, ticks)
                for r in runs.iter() { for (k, n) in &r.zombies { let e = per.entry(k.clone()).or_default(); e.0 += 1; e.1 += n; } }
                let games_with = runs.iter().filter(|r| !r.zombies.is_empty()).count();
                let mut v: Vec<(String, (u32, u32))> = per.into_iter().collect();
                v.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
                let list: Vec<String> = v.iter().take(8).map(|(k, (g, t))| format!("{k} {g} games / {:.0} ticks per game", *t as f64 / *g as f64)).collect();
                zombie_rows.push(format!("| {sc} | {world} | {label} | {games_with} / {seeds} | {} |", if list.is_empty() { "—".into() } else { list.join(", ") }));
            }
            let depop: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::Depop(None), s, ticks)).collect();
            let (dm, dt) = paired(&v2, &depop, |r| r.deaths as f64);
            let (kt, kcells) = key_worst_t(&v2, &depop);
            if kt >= 3.0 { depop_key_ok = false; }
            let outc = |runs: &[Run]| {
                let mut m: BTreeMap<String, u32> = BTreeMap::new();
                for r in runs { for o in &r.outcomes { *m.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                let multi = runs.iter().filter(|r| r.outcomes.len() > 1).count();
                if m.is_empty() { "—".to_string() } else { format!("{} (>1: {multi})", m.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")) }
            };
            let split = |runs: &[Run]| if sc == "rome_375" { runs.iter().filter(|r| r.split40).count().to_string() } else { "—".into() };
            depop_rows.push(format!("| {sc} | {world} | {} → {} | {dm:+.2} (t {dt:+.1}) | {kcells} | {} → {} | {} → {} |",
                v2.iter().map(|r| r.deaths).sum::<u32>(), depop.iter().map(|r| r.deaths).sum::<u32>(),
                split(&v2), split(&depop), outc(&v2), outc(&depop)));
            for x in std::iter::once(None).chain(CUTS.iter().map(|c| Some(*c))) {
                let m = Model::Depop(x);
                let runs: Vec<Run> = if x.is_none() { (0..seeds).map(|s| run(sc, world, m, s, ticks)).collect() } else { (0..seeds).map(|s| run(sc, world, m, s, ticks)).collect() };
                let mut per: BTreeMap<String, (u64, u64)> = BTreeMap::new();
                for r in &runs { for (k, d) in &r.debt { let e = per.entry(k.clone()).or_default(); e.0 += d.0; e.1 += d.1; } }
                let mut shares: Vec<(String, f64)> = per.iter().map(|(k, d)| (k.clone(), 100.0 * d.1 as f64 / d.0.max(1) as f64)).collect();
                shares.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let over: Vec<String> = shares.iter().filter(|s| s.1 > 20.0).map(|(k, v)| format!("{k} {v:.0} %")).collect();
                let spells: Vec<f64> = runs.iter().flat_map(|r| r.spells.iter().copied()).collect();
                let p90 = pct(&spells, 0.9);
                let (kt, kcells) = key_worst_t(&v2, &runs);
                if let Some(c) = x {
                    verdict.entry(format!("{:.0} %", c * 100.0)).or_default().push((*world == "none", p90, shares.first().map_or(0.0, |s| s.1), kt));
                }
                debt_rows.push(format!("| {sc} | {world} | {} | {} | {:.0} / {p90:.0} | {kcells} |", m.label(),
                    if over.is_empty() { "—".into() } else { over.join(", ") }, pct(&spells, 0.5)));
            }
        }
    }
    println!("## 1. Zombies: actors living with population ≤ 1\n");
    println!("| scenario | world | model | games with a zombie | zombies (games, ticks per such game) |");
    println!("|---|---|---|---|---|");
    for r in zombie_rows { println!("{r}"); }
    println!("\n## 2. Depopulation as collapse (population ≤ 1 for {DEPOP} ticks), against v2 at c2b6a10\n");
    println!("| scenario | world | deaths | paired per seed | key actors (deaths before → after, t) | split on 40 | outcomes |");
    println!("|---|---|---|---|---|---|---|");
    for r in depop_rows { println!("{r}"); }
    println!("\n## 3. The debt rule with depopulation on (zombie ticks not counted)\n");
    println!("| scenario | world | model | actors in debt > 20 % of counted ticks | spell median / p90 | key actors vs v2 at c2b6a10 |");
    println!("|---|---|---|---|---|---|");
    for r in debt_rows { println!("{r}"); }
    println!("\n## 4. The pre-commitment\n");
    println!("Depopulation alone keeps every key actor's deaths at t < 3 in every world: {}\n", if depop_key_ok { "yes" } else { "no" });
    println!("| x | (1) worlds with p90 ≤ 12 | (2) no-player worlds with no actor > 20 % | (4) worlds with key t < 3 | worst p90 | worst no-player share | worst key t | passes |");
    println!("|---|---|---|---|---|---|---|---|");
    for x in CUTS {
        let key = format!("{:.0} %", x * 100.0);
        let v = &verdict[&key];
        let a = v.iter().filter(|s| s.1 <= 12.0).count();
        let np: Vec<&(bool, f64, f64, f64)> = v.iter().filter(|s| s.0).collect();
        let b = np.iter().filter(|s| s.2 <= 20.0).count();
        let c = v.iter().filter(|s| s.3 < 3.0).count();
        let pass = a == v.len() && b == np.len() && c == v.len() && depop_key_ok;
        println!("| {key} | {a} / {} | {b} / {} | {c} / {} | {:.0} | {:.0} % | {:.1} | {} |", v.len(), np.len(), v.len(),
            v.iter().map(|s| s.1).fold(0.0, f64::max), np.iter().map(|s| s.2).fold(0.0, f64::max), v.iter().map(|s| s.3).fold(f64::NEG_INFINITY, f64::max),
            if pass { "yes" } else { "no" });
    }
}
