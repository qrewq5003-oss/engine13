//! Economy project, Ц1 stage 3 and the debt rule again (docs/economy_project_brief.md §9).
//! Measurement only for the thresholds; the debt rule's x is chosen by the owner's
//! pre-commitment. Built with `--features census`. Every world, 30 seeds × 300 ticks.
//!
//! Ц1 stage 3 (v2): a dependency rule whose source is `economic_output` measures a fall below the
//! actor's own norm — its threshold is `threshold × T / 100`, `T` = the `economic_output` target.
//! Compared with v2 before the stage (absolute thresholds, `census::set_eo_absolute_thresholds`)
//! and with v1, paired by seed:
//! 1. for each of the three rules (`economic_output_to_treasury` < 50,
//!    `economic_output_to_population` < 50, `low_economic_output_to_population_decay` < 15): the share
//!    of living ticks it fires, per actor;
//! 2. deaths paired against v2 before and against v1;
//! 3. the Ц1 measure and the tiers (as after stage 2);
//! 4. the treasury of the chronic debtors of Ц2 stage 1 by source, before and after.
//!
//! Then the debt rule again (x = 0.05, 0.10, 0.20) with the owner's pre-commitment: no actor in debt
//! on more than 20 % of its living ticks and the p90 debt spell ≤ 12 ticks, in every world; the
//! remaining chronic debtors and their treasury by source; the §9.2 rows.
//!
//! Usage: cargo run --release --features census --bin c1s3_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const RULES: [&str; 3] = ["economic_output_to_treasury", "economic_output_to_population", "low_economic_output_to_population_decay"];
const DEBTORS: [&str; 8] = ["huns", "ostrogoths", "saxons", "mantua", "urbino", "ferrara", "trebizond", "wallachia"];
const CUTS: [f64; 3] = [0.05, 0.10, 0.20];

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
enum Model { V1, V2Before, V2(Option<f64>) }

impl Model {
    fn label(&self) -> String {
        match self {
            Model::V1 => "v1".into(),
            Model::V2Before => "v2 before (absolute)".into(),
            Model::V2(None) => "v2 relative".into(),
            Model::V2(Some(x)) => format!("v2 relative, debt x = {:.0} %", x * 100.0),
        }
    }
}

#[derive(Default)]
struct Run {
    // actor -> (living ticks, ticks each rule fired)
    fires: BTreeMap<String, (u64, [u64; 3])>,
    eo: BTreeMap<String, Vec<f64>>,
    debt: BTreeMap<String, (u64, u64)>,
    spells: Vec<f64>,
    // (actor, source) -> treasury asked
    treasury: BTreeMap<(String, String), f64>,
    deaths: u32,
    key_dead: BTreeMap<String, bool>,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

fn run(sc: &str, world: &str, v: Model, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = v != Model::V1;
        s.economy_v2_debt_cut = if let Model::V2(x) = v { x } else { None };
    }
    census::set_eo_absolute_thresholds(v == Model::V2Before);
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut open: BTreeMap<String, u32> = BTreeMap::new();
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
        let writes = census::take_writes();
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        let mut fired: BTreeMap<&str, [bool; 3]> = BTreeMap::new();
        for w in &writes {
            let Some(src) = w.source.as_deref() else { continue };
            for (i, rule) in RULES.iter().enumerate() {
                if src == format!("dependency {rule}") { fired.entry(w.actor.as_str()).or_default()[i] = true; }
            }
            if w.metric == "treasury" && DEBTORS.contains(&w.actor.as_str()) {
                *r.treasury.entry((w.actor.clone(), src.to_string())).or_default() += w.requested;
            }
        }
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let e = r.fires.entry(id.clone()).or_default();
            e.0 += 1;
            if let Some(f) = fired.get(id.as_str()) { for (n, hit) in e.1.iter_mut().zip(f) { if *hit { *n += 1; } } }
            r.eo.entry(id.clone()).or_default().push(a.get_metric("economic_output"));
            let d = r.debt.entry(id.clone()).or_default();
            d.0 += 1;
            if a.get_metric("treasury") < 0.0 {
                d.1 += 1;
                *open.entry(id.clone()).or_default() += 1;
            } else if let Some(n) = open.remove(id) {
                r.spells.push(n as f64);
            }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    r.spells.extend(open.values().map(|n| *n as f64));
    census::set_eo_absolute_thresholds(false);
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    for k in key_actors(sc) { r.key_dead.insert(k.to_string(), ws.dead_actor_ids.contains(*k)); }
    r.outcomes = ws.milestone_events_fired.iter().filter(|m| m.starts_with("outcome_")).cloned().collect();
    r.stab = ws.milestone_events_fired.iter().any(|m| m == "milan_regency_stabilizes");
    r.deep = ws.milestone_events_fired.iter().any(|m| m == "milan_regency_crisis_deepens");
    r
}

fn paired(a: &[Run], b: &[Run]) -> String {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| y.deaths as f64 - x.deaths as f64).collect();
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
    println!("# Ц1 stage 3 (relative eo thresholds) and the debt rule again, {seeds} seeds × {ticks} ticks per world\n");
    let mut fire_rows = Vec::new();
    let mut death_rows = Vec::new();
    let mut c1_rows = Vec::new();
    let mut treas: BTreeMap<String, BTreeMap<(String, String), f64>> = BTreeMap::new();
    let mut treas_games: BTreeMap<String, u64> = BTreeMap::new();
    let mut debt_rows = Vec::new();
    let mut constraint_rows = Vec::new();
    let mut verdict: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    let mut chronic: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let v1: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::V1, s, ticks)).collect();
            let before: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::V2Before, s, ticks)).collect();
            let after: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::V2(None), s, ticks)).collect();
            death_rows.push(format!("| {sc} | {world} | {} | {} | {} | {} | {} |",
                v1.iter().map(|r| r.deaths).sum::<u32>(), before.iter().map(|r| r.deaths).sum::<u32>(), after.iter().map(|r| r.deaths).sum::<u32>(),
                paired(&before, &after), paired(&v1, &after)));
            // rule firing per actor, before → after
            for (label, runs) in [("before", &before), ("after", &after)] {
                let mut per: BTreeMap<String, (u64, [u64; 3])> = BTreeMap::new();
                for r in runs.iter() { for (k, v) in &r.fires { let e = per.entry(k.clone()).or_default(); e.0 += v.0; for i in 0..3 { e.1[i] += v.1[i]; } } }
                for (i, rule) in RULES.iter().enumerate() {
                    let mut shares: Vec<(String, f64)> = per.iter().map(|(k, v)| (k.clone(), 100.0 * v.1[i] as f64 / v.0.max(1) as f64)).collect();
                    shares.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                    let all: Vec<f64> = shares.iter().map(|x| x.1).collect();
                    let over = all.iter().filter(|x| **x > 50.0).count();
                    let top: Vec<String> = shares.iter().take(4).map(|(k, v)| format!("{k} {v:.0} %")).collect();
                    fire_rows.push(format!("| {sc} | {world} | {rule} | {label} | {} | {over} / {} | {} |", q(&all), all.len(), top.join(", ")));
                }
            }
            // Ц1 measure and tiers, after
            let mut pool: BTreeMap<String, Vec<f64>> = BTreeMap::new();
            for r in &after { for (k, x) in &r.eo { pool.entry(k.clone()).or_default().extend(x); } }
            let share = |x: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * x.iter().filter(|y| pr(**y)).count() as f64 / x.len().max(1) as f64;
            let passing = pool.values().filter(|x| share(x, &|y| y >= 99.0) < 20.0 && share(x, &|y| y <= 1.0) < 20.0).count();
            let tm: Vec<f64> = tiers(sc).iter().map(|t| { let x: Vec<f64> = t.iter().filter_map(|a| pool.get(*a)).flatten().copied().collect(); pct(&x, 0.5) }).collect();
            c1_rows.push(format!("| {sc} | {world} | {passing} / {} | {} | {} |", pool.len(),
                tm.iter().map(|m| format!("{m:.0}")).collect::<Vec<_>>().join(" › "), if tm.windows(2).all(|w| w[0] > w[1]) { "yes" } else { "no" }));
            for (label, runs) in [("before", &before), ("after", &after)] {
                let e = treas.entry(label.into()).or_default();
                for r in runs.iter() { for (k, x) in &r.treasury { *e.entry(k.clone()).or_default() += x; } }
                *treas_games.entry(label.into()).or_default() += runs.len() as u64;
            }
            // debt rule
            for x in std::iter::once(None).chain(CUTS.iter().map(|c| Some(*c))) {
                let v = Model::V2(x);
                let runs: Vec<Run> = if x.is_none() { (0..seeds).map(|s| run(sc, world, v, s, ticks)).collect() } else { (0..seeds).map(|s| run(sc, world, v, s, ticks)).collect() };
                let mut per: BTreeMap<String, (u64, u64)> = BTreeMap::new();
                for r in &runs { for (k, d) in &r.debt { let e = per.entry(k.clone()).or_default(); e.0 += d.0; e.1 += d.1; } }
                let mut shares: Vec<(String, f64)> = per.iter().map(|(k, d)| (k.clone(), 100.0 * d.1 as f64 / d.0.max(1) as f64)).collect();
                shares.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let over: Vec<String> = shares.iter().filter(|s| s.1 > 20.0).map(|(k, v)| format!("{k} {v:.0} %")).collect();
                let spells: Vec<f64> = runs.iter().flat_map(|r| r.spells.iter().copied()).collect();
                let p90 = pct(&spells, 0.9);
                if let Some(c) = x {
                    verdict.entry(format!("{:.0} %", c * 100.0)).or_default().push((shares.first().map_or(0.0, |s| s.1), p90));
                    if c == 0.20 { chronic.insert(format!("{sc} {world}"), over.clone()); }
                }
                debt_rows.push(format!("| {sc} | {world} | {} | {} | {:.0} / {p90:.0} | {} |", v.label(),
                    if over.is_empty() { "—".into() } else { over.join(", ") }, pct(&spells, 0.5), paired(&after, &runs)));
                if x == Some(0.10) || x.is_none() {
                    let wins: Vec<f64> = runs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
                    let mut perw: BTreeMap<u32, u32> = BTreeMap::new();
                    for t in &wins { *perw.entry(*t as u32).or_default() += 1; }
                    let top = perw.values().max().copied().unwrap_or(0);
                    let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                    let wins_s = if wins.is_empty() { "0".into() } else { format!("{} ({}; 40–43: {on}; top {:.0} %)", wins.len(), q(&wins), 100.0 * top as f64 / wins.len() as f64) };
                    let keys = key_actors(sc).iter().map(|k| format!("{k} {}", runs.iter().filter(|r| r.key_dead[*k]).count())).collect::<Vec<_>>().join(", ");
                    let mut oc: BTreeMap<String, u32> = BTreeMap::new();
                    for r in &runs { for o in &r.outcomes { *oc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                    let multi = runs.iter().filter(|r| r.outcomes.len() > 1).count();
                    let outc = if oc.is_empty() { "—".into() } else { format!("{} (>1: {multi})", oc.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")) };
                    let fork = if sc == "milan_1477" { format!("{} / {}", runs.iter().filter(|r| r.stab).count(), runs.iter().filter(|r| r.deep).count()) } else { "—".into() };
                    let split = if sc == "rome_375" { runs.iter().filter(|r| r.split40).count().to_string() } else { "—".into() };
                    constraint_rows.push(format!("| {sc} | {world} | {} | {} | {keys} | {wins_s} | {split} | {outc} | {fork} |", v.label(), runs.iter().map(|r| r.deaths).sum::<u32>()));
                }
            }
        }
    }
    println!("## 1. Deaths: v1, v2 before (absolute thresholds), v2 after (relative), paired by seed\n");
    println!("| scenario | world | v1 | v2 before | v2 after | after − before per seed | after − v1 per seed |");
    println!("|---|---|---|---|---|---|---|");
    for r in death_rows { println!("{r}"); }
    println!("\n## 2. Where the three rules fire: share of living ticks per actor (p10/50/90 over actors), actors over 50 %, the top four\n");
    println!("| scenario | world | rule | v2 | per actor p10/50/90 | actors > 50 % | most |");
    println!("|---|---|---|---|---|---|---|");
    for r in fire_rows { println!("{r}"); }
    println!("\n## 3. Ц1 measure and tiers after the stage (v2 relative)\n");
    println!("| scenario | world | actors with ceiling and floor < 20 % | tiers | ordered |");
    println!("|---|---|---|---|---|");
    for r in c1_rows { println!("{r}"); }
    println!("\n## 4. The chronic debtors' treasury by source, per game (all worlds of their scenario)\n");
    println!("| actor | v2 | largest sources |");
    println!("|---|---|---|");
    for label in ["before", "after"] {
        let m = &treas[label];
        for a in DEBTORS {
            let mut v: Vec<(String, f64)> = m.iter().filter(|((x, _), _)| x == a).map(|((_, s), x)| (s.clone(), *x)).collect();
            // per game of that actor's scenario: count games of its scenario
            let games = (seeds * worlds(match a { "huns" | "ostrogoths" | "saxons" => "rome_375", "mantua" | "urbino" | "ferrara" => "milan_1477", _ => "constantinople_1430" }).len() as u64) as f64;
            v.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap());
            let cells: Vec<String> = v.iter().filter(|x| x.1.abs() / games > 1.0).take(5).map(|(s, x)| format!("{s} {:+.0}", x / games)).collect();
            println!("| {a} | {label} | {} |", cells.join(", "));
        }
    }
    let _ = treas_games;
    println!("\n## 5. The debt rule again (relative thresholds on)\n");
    println!("| scenario | world | variant | actors in debt > 20 % of living ticks | debt spell median / p90 | deaths paired vs v2 relative without the rule |");
    println!("|---|---|---|---|---|---|");
    for r in debt_rows { println!("{r}"); }
    println!("\n## 6. §9.2 constraints (for information)\n");
    println!("| scenario | world | variant | deaths | key deaths | wins | split on 40 | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in constraint_rows { println!("{r}"); }
    println!("\n## 7. The owner's pre-commitment for the debt rule\n");
    println!("| x | worlds where no actor is in debt > 20 % | worlds with p90 spell ≤ 12 | worst share | worst p90 | passes |");
    println!("|---|---|---|---|---|---|");
    for x in CUTS {
        let key = format!("{:.0} %", x * 100.0);
        let v = &verdict[&key];
        let a = v.iter().filter(|s| s.0 <= 20.0).count();
        let b = v.iter().filter(|s| s.1 <= 12.0).count();
        println!("| {key} | {a} / {} | {b} / {} | {:.0} % | {:.0} | {} |", v.len(), v.len(),
            v.iter().map(|s| s.0).fold(0.0, f64::max), v.iter().map(|s| s.1).fold(0.0, f64::max), if a == v.len() && b == v.len() { "yes" } else { "no" });
    }
    println!("\n## 8. Chronic debtors left at x = 20 % (in debt > 20 % of living ticks)\n");
    for (w, list) in chronic { println!("- {w}: {}", if list.is_empty() { "none".into() } else { list.join(", ") }); }
}
