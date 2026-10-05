//! Economy project, Ц1 stage 2 (docs/economy_project_brief.md §9). Measurement only.
//!
//! Economy v2, second part: `economic_output` is pulled toward its target each tick,
//! `eo += r × (T − eo)`, `T` = the actor's authored starting `economic_output` + its tags'
//! current levels (`engine::eo_target`). Variants in memory: v1, v2 without the pull (stage 1)
//! and v2 with r = 0.03, 0.05, 0.10. Every world of the three scenarios, the same seeds.
//! Built with `--features census`.
//!
//! Per r: the Ц1 measure (actors with both ceiling ≥ 99 and floor ≤ 1 under 20 % of living
//! ticks), the tiers (brief §9.5) per world, the interquartile range of actor medians, the share
//! of living actor-ticks with |eo − T| > 10, the income per game v2 / v1, the §9.2 rows, and
//! what pushes the Ottomans' `economic_output` up. The owner's pre-commitment is evaluated at
//! the end: the smallest r with ≥ 80 % of actors passing in every world, the tiers ordered in
//! every world and |eo − T| > 10 on ≥ 10 % of living actor-ticks (every world).
//!
//! `refit <r>` instead runs the A46 income refit for that r: per scenario, the coefficient whose
//! median total income per game (10 seeds, all worlds) matches v1 — a proportional step from
//! the current value, then a secant step.
//!
//! Usage: cargo run --release --features census --bin c1s2_probe -- [seeds] [ticks] [refit <r>]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const PULLS: [f64; 3] = [0.03, 0.05, 0.10];

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

fn tiers(sc: &str) -> Vec<Vec<&'static str>> {
    match sc {
        "rome_375" => vec![
            vec!["guptas", "sassanids", "eastern_jin"],
            vec!["rome", "kushans", "armenia"],
            vec!["berbers", "franks", "visigoths", "burgundians", "vandals", "alamanni", "ostrogoths", "saxons"],
            vec!["huns"],
        ],
        "constantinople_1430" => vec![
            vec!["venice", "milan", "genoa"],
            vec!["ottomans", "papacy", "hungary"],
            vec!["trebizond", "serbia", "byzantium"],
        ],
        _ => vec![
            vec!["venice", "florence", "milan"],
            vec!["genoa", "naples", "papacy"],
            vec!["sicily", "ferrara", "bologna"],
            vec!["siena", "urbino", "savoy", "mantua"],
        ],
    }
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn pct_at(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f64 * p).round() as usize]
}

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    format!("{:.0}/{:.0}/{:.0}", pct_at(v, 0.1), pct_at(v, 0.5), pct_at(v, 0.9))
}

#[derive(Clone, Copy, PartialEq)]
enum V { V1, V2(Option<f64>) }

impl V {
    fn label(&self) -> String {
        match self {
            V::V1 => "v1".into(),
            V::V2(None) => "v2, no pull".into(),
            V::V2(Some(r)) => format!("v2, r = {r:.2}"),
        }
    }
}

#[derive(Default)]
struct Run {
    eo: BTreeMap<String, Vec<f64>>,
    off_target: (u64, u64),
    income: f64,
    ott_sources: BTreeMap<String, f64>,
    ott_ticks: u64,
    deaths: u32,
    key_dead: BTreeMap<String, bool>,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

thread_local! {
    static COEF: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
}

fn run(sc: &str, world: &str, v: V, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = v != V::V1;
        s.economy_v2_eo_pull = if let V::V2(r) = v { r } else { None };
        if let Some(c) = COEF.with(|c| c.get()) { s.economy_v2_income_coefficient = Some(c); }
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let _ = census::take_writes();
    let _ = census::take_treasury_parts();
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
        let scn = st.current_scenario.as_ref().unwrap();
        let t = ws.tick - 1;
        let ott_alive = ws.actors.contains_key("ottomans") && !ws.dead_actor_ids.contains("ottomans");
        for w in census::take_writes() {
            if w.metric == "economic_output" && w.actor == "ottomans" && ott_alive {
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                *r.ott_sources.entry(src).or_default() += w.requested;
            }
        }
        if ott_alive { r.ott_ticks += 1; }
        r.income += census::take_treasury_parts().iter().map(|p| p.1).sum::<f64>();
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) {
                continue;
            }
            let eo = ws.actors[id].get_metric("economic_output");
            r.eo.entry(id.clone()).or_default().push(eo);
            if let Some(target) = engine13::engine::eo_target(ws, scn, id) {
                r.off_target.1 += 1;
                if (eo - target.clamp(0.0, 100.0)).abs() > 10.0 { r.off_target.0 += 1; }
            }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    for k in key_actors(sc) { r.key_dead.insert(k.to_string(), ws.dead_actor_ids.contains(*k)); }
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    census::enable_treasury_parts();
    if args.get(3).is_some_and(|a| a == "refit") {
        let r: f64 = args.get(4).and_then(|s| s.parse().ok()).expect("refit <r>");
        let med = |sc: &str, v: V| {
            let mut x: Vec<f64> = worlds(sc).iter().flat_map(|w| (0..10).map(move |s| (w, s))).map(|(w, s)| run(sc, w, v, s, ticks).income).collect();
            median(&mut x)
        };
        for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
            let target = med(sc, V::V1);
            let c0 = engine13::scenarios::registry::load_by_id(sc).unwrap().economy_v2_income_coefficient.unwrap_or(0.001);
            COEF.with(|c| c.set(Some(c0)));
            let i0 = med(sc, V::V2(Some(r)));
            let c1 = c0 * target / i0;
            COEF.with(|c| c.set(Some(c1)));
            let i1 = med(sc, V::V2(Some(r)));
            let c2 = c1 + (target - i1) * (c1 - c0) / (i1 - i0);
            COEF.with(|c| c.set(Some(c2)));
            let i2 = med(sc, V::V2(Some(r)));
            COEF.with(|c| c.set(None));
            println!("{sc} r = {r}: v1 {target:.0}; c0 {c0:.6} → {i0:.0} ({:.3}); c1 {c1:.6} → {i1:.0} ({:.3}); c2 {c2:.6} → {i2:.0} ({:.3})",
                i0 / target, i1 / target, i2 / target);
        }
        return;
    }
    let variants: Vec<V> = std::iter::once(V::V2(None)).chain(PULLS.iter().map(|r| V::V2(Some(*r)))).collect();
    println!("# Ц1 stage 2 — economic_output pulled toward its target, {seeds} seeds × {ticks} ticks per world\n");

    // per variant: per world (passing share, tiers ok, off-target share)
    let mut verdict: BTreeMap<String, Vec<(String, f64, bool, f64)>> = BTreeMap::new();
    let mut measure_rows = Vec::new();
    let mut tier_rows = Vec::new();
    let mut income_rows = Vec::new();
    let mut constraint_rows = Vec::new();
    let mut ott_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let mut income: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut ott: BTreeMap<String, (BTreeMap<String, f64>, u64)> = BTreeMap::new();
        for world in worlds(sc) {
            let base: Vec<Run> = (0..seeds).map(|s| run(sc, world, V::V1, s, ticks)).collect();
            income.entry("v1".into()).or_default().extend(base.iter().map(|r| r.income));
            for v in &variants {
                let runs: Vec<Run> = (0..seeds).map(|s| run(sc, world, *v, s, ticks)).collect();
                income.entry(v.label()).or_default().extend(runs.iter().map(|r| r.income));
                if sc == "constantinople_1430" {
                    let e = ott.entry(v.label()).or_default();
                    for r in &runs { e.1 += r.ott_ticks; for (k, x) in &r.ott_sources { *e.0.entry(k.clone()).or_default() += x; } }
                }
                let mut pool: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for r in &runs { for (k, x) in &r.eo { pool.entry(k.clone()).or_default().extend(x); } }
                let share = |x: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * x.iter().filter(|y| pr(**y)).count() as f64 / x.len().max(1) as f64;
                let passing = pool.values().filter(|x| share(x, &|y| y >= 99.0) < 20.0 && share(x, &|y| y <= 1.0) < 20.0).count();
                let pass_share = 100.0 * passing as f64 / pool.len().max(1) as f64;
                let meds: Vec<f64> = pool.values().map(|x| median(&mut x.clone())).collect();
                let iqr = pct_at(&meds, 0.75) - pct_at(&meds, 0.25);
                let (off, n) = runs.iter().fold((0, 0), |a, r| (a.0 + r.off_target.0, a.1 + r.off_target.1));
                let off_share = 100.0 * off as f64 / n.max(1) as f64;
                let tier_meds: Vec<f64> = tiers(sc).iter().map(|t| {
                    let mut x: Vec<f64> = t.iter().filter_map(|a| pool.get(*a)).flatten().copied().collect();
                    median(&mut x)
                }).collect();
                let ordered = tier_meds.windows(2).all(|w| w[0] > w[1]);
                verdict.entry(v.label()).or_default().push((format!("{sc} {world}"), pass_share, ordered, off_share));
                measure_rows.push(format!("| {sc} | {world} | {} | {passing} / {} ({pass_share:.0} %) | {iqr:.0} | {} | {off_share:.1} % |",
                    v.label(), pool.len(), q(&meds)));
                tier_rows.push(format!("| {sc} | {world} | {} | {} | {} |", v.label(),
                    tier_meds.iter().map(|m| format!("{m:.0}")).collect::<Vec<_>>().join(" › "), if ordered { "yes" } else { "no" }));
                // §9.2
                let wins: Vec<f64> = runs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
                let mut per: BTreeMap<u32, u32> = BTreeMap::new();
                for t in &wins { *per.entry(*t as u32).or_default() += 1; }
                let top = per.values().max().copied().unwrap_or(0);
                let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let wins_s = if wins.is_empty() { "0".into() } else { format!("{} ({}; 40–43: {on}; top {:.0} %)", wins.len(), q(&wins), 100.0 * top as f64 / wins.len() as f64) };
                let keys = key_actors(sc).iter().map(|k| format!("{k} {}", runs.iter().filter(|r| r.key_dead[*k]).count())).collect::<Vec<_>>().join(", ");
                let mut oc: BTreeMap<String, u32> = BTreeMap::new();
                for r in &runs { for o in &r.outcomes { *oc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                let multi = runs.iter().filter(|r| r.outcomes.len() > 1).count();
                let outc = if oc.is_empty() { "—".into() } else { format!("{} (>1: {multi})", oc.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")) };
                let fork = if sc == "milan_1477" { format!("{} / {}", runs.iter().filter(|r| r.stab).count(), runs.iter().filter(|r| r.deep).count()) } else { "—".into() };
                let split = if sc == "rome_375" { runs.iter().filter(|r| r.split40).count().to_string() } else { "—".into() };
                constraint_rows.push(format!("| {sc} | {world} | {} | {} | {} | {keys} | {wins_s} | {split} | {outc} | {fork} |",
                    v.label(), runs.iter().map(|r| r.deaths).sum::<u32>(), paired(&base, &runs, |r| r.deaths as f64)));
            }
        }
        let v1m = median(&mut income["v1"].clone());
        for v in &variants {
            let m = median(&mut income[&v.label()].clone());
            income_rows.push(format!("| {sc} | {} | {:.3} |", v.label(), m / v1m));
        }
        for (label, (src, n)) in &ott {
            let mut x: Vec<(String, f64)> = src.iter().map(|(k, v)| (k.clone(), v / (*n).max(1) as f64)).collect();
            x.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());
            let cells: Vec<String> = x.iter().filter(|y| y.1.abs() >= 0.005).take(8).map(|(k, v)| format!("{k} {v:+.3}")).collect();
            ott_rows.push(format!("| {label} | {} |", cells.join(", ")));
        }
    }

    println!("## 1. Ц1 measure, spread, distance from the target (v2 variants)\n");
    println!("| scenario | world | variant | actors with ceiling and floor < 20 % | IQR of actor medians | actor medians p10/50/90 | living actor-ticks with \\|eo − T\\| > 10 |");
    println!("|---|---|---|---|---|---|---|");
    for r in measure_rows { println!("{r}"); }
    println!("\n## 2. Tiers (brief §9.5), median per tier, richest first\n");
    println!("| scenario | world | variant | tiers | ordered |");
    println!("|---|---|---|---|---|");
    for r in tier_rows { println!("{r}"); }
    println!("\n## 3. Income per game, v2 / v1 (median of totals, all worlds)\n");
    println!("| scenario | variant | v2 / v1 |");
    println!("|---|---|---|");
    for r in income_rows { println!("{r}"); }
    println!("\n## 4. The Ottomans' economic_output: asked writes per living tick (constantinople, all worlds)\n");
    println!("| variant | sources |");
    println!("|---|---|");
    for r in ott_rows { println!("{r}"); }
    println!("\n## 5. §9.2 constraints (for information)\n");
    println!("| scenario | world | variant | deaths | deaths paired vs v1 per seed | key deaths | wins | split on 40 | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for r in constraint_rows { println!("{r}"); }
    println!("\n## 6. The owner's pre-commitment\n");
    println!("| variant | worlds with ≥ 80 % of actors passing | worlds with the tiers ordered | worlds with \\|eo − T\\| > 10 on ≥ 10 % | passes |");
    println!("|---|---|---|---|---|");
    for v in &variants {
        let rows = &verdict[&v.label()];
        let a = rows.iter().filter(|x| x.1 >= 80.0).count();
        let b = rows.iter().filter(|x| x.2).count();
        let c = rows.iter().filter(|x| x.3 >= 10.0).count();
        let n = rows.len();
        println!("| {} | {a} / {n} | {b} / {n} | {c} / {n} | {} |", v.label(), if a == n && b == n && c == n { "yes" } else { "no" });
    }
}
