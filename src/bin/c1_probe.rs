//! Economy project, Ц1 stage 1 (docs/economy_project_brief.md §9). Measurement only.
//!
//! Economy v2, first part (`ScenarioFeatures::economy_v2`, switched on in memory): tags'
//! `economic_output` modifiers are a level, and the treasury income coefficient is the
//! refitted one (`Scenario::economy_v2_income_coefficient`, variant (д′) of A46). Every
//! world of the three scenarios, v1 against v2 on the same seeds. Built with
//! `--features census` (the A37 write sink and the treasury parts):
//!
//! 1. `economic_output` per actor: share of living ticks at the ceiling (≥ 99) and the floor
//!    (≤ 1), median; the Ц1 measure — both shares under 20 % for every actor;
//! 2. the spread of actor medians in each scenario (≥ 20 points), and the approved tiers
//!    (brief §9.5): each tier's median above the next tier's, per world;
//! 3. what pulls `economic_output` down: asked writes per living actor-tick by source;
//! 4. the income check: median total income per game, v2 / v1 (the (д′) refit was made in
//!    the world before A4 and A8);
//! 5. the §9.2 constraints, printed for information: A10, A35, the split on tick 40, B46
//!    outcomes per game, the regency fork, deaths (key and all) paired by seed with t.
//!
//! Living actors, tick-end values. Successor actors are counted in the shares but are in no
//! tier.
//!
//! Usage: cargo run --release --features census --bin c1_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

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

/// The tiers approved by the owner (brief §9.5), richest first.
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

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

#[derive(Default)]
struct Run {
    eo: BTreeMap<String, Vec<f64>>,
    // asked writes to economic_output by source, and living actor-ticks
    sources: BTreeMap<String, f64>,
    actor_ticks: u64,
    income: f64,
    deaths: u32,
    key_dead: BTreeMap<String, bool>,
    win: Option<u32>,
    win_ottomans_alive: bool,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

fn run(sc: &str, world: &str, v2: bool, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    st.current_scenario.as_mut().unwrap().features.economy_v2 = v2;
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
        let t = ws.tick - 1;
        for w in census::take_writes() {
            if w.metric == "economic_output" && ws.actors.contains_key(&w.actor) {
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                *r.sources.entry(src).or_default() += w.requested;
            }
        }
        r.income += census::take_treasury_parts().iter().map(|p| p.1).sum::<f64>();
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) {
                continue;
            }
            r.actor_ticks += 1;
            r.eo.entry(id.clone()).or_default().push(ws.actors[id].get_metric("economic_output"));
        }
        if r.win.is_none() && ws.victory_achieved {
            r.win = Some(t);
            r.win_ottomans_alive = ws.actors.contains_key("ottomans") && !ws.dead_actor_ids.contains("ottomans");
        }
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
    format!("{mean:+.2} ± {sd:.2} (t {t:+.1})")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    census::enable_treasury_parts();
    println!("# Ц1 stage 1 — economy v2 (tags' economic_output as a level + refitted income), {seeds} seeds × {ticks} ticks per world\n");

    let mut actor_rows = Vec::new();
    let mut world_rows = Vec::new();
    let mut tier_rows = Vec::new();
    let mut source_rows = Vec::new();
    let mut income_rows = Vec::new();
    let mut constraint_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let mut inc = (Vec::new(), Vec::new());
        let mut pooled_sources: [BTreeMap<String, f64>; 2] = [BTreeMap::new(), BTreeMap::new()];
        let mut pooled_ticks = [0u64; 2];
        for world in worlds(sc) {
            let v1: Vec<Run> = (0..seeds).map(|s| run(sc, world, false, s, ticks)).collect();
            let v2: Vec<Run> = (0..seeds).map(|s| run(sc, world, true, s, ticks)).collect();
            for r in &v1 { inc.0.push(r.income); }
            for r in &v2 { inc.1.push(r.income); }
            for (i, runs) in [&v1, &v2].iter().enumerate() {
                for r in runs.iter() {
                    pooled_ticks[i] += r.actor_ticks;
                    for (k, x) in &r.sources { *pooled_sources[i].entry(k.clone()).or_default() += x; }
                }
            }
            // per-actor distribution
            let pool = |runs: &[Run]| {
                let mut m: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for r in runs { for (k, v) in &r.eo { m.entry(k.clone()).or_default().extend(v); } }
                m
            };
            let (p1, p2) = (pool(&v1), pool(&v2));
            let share = |v: &[f64], pred: &dyn Fn(f64) -> bool| 100.0 * v.iter().filter(|x| pred(**x)).count() as f64 / v.len().max(1) as f64;
            let mut medians2: Vec<(String, f64)> = Vec::new();
            let mut ok = 0;
            for (id, v) in &p2 {
                let (c2, f2) = (share(v, &|x| x >= 99.0), share(v, &|x| x <= 1.0));
                let m2 = median(&mut v.clone());
                let v1v = p1.get(id).cloned().unwrap_or_default();
                let (c1, f1, m1) = (share(&v1v, &|x| x >= 99.0), share(&v1v, &|x| x <= 1.0), median(&mut v1v.clone()));
                medians2.push((id.clone(), m2));
                if c2 < 20.0 && f2 < 20.0 { ok += 1; }
                actor_rows.push(format!("| {sc} | {world} | {id} | {c1:.0} % / {f1:.0} % / {m1:.0} | {c2:.0} % / {f2:.0} % / {m2:.0} | {} |",
                    if c2 < 20.0 && f2 < 20.0 { "yes" } else { "no" }));
            }
            let meds: Vec<f64> = medians2.iter().map(|x| x.1).filter(|x| x.is_finite()).collect();
            let spread = meds.iter().cloned().fold(f64::MIN, f64::max) - meds.iter().cloned().fold(f64::MAX, f64::min);
            world_rows.push(format!("| {sc} | {world} | {ok} / {} | {spread:.0} | {} |", p2.len(), q(&meds)));
            // tiers: pooled living ticks of the tier's actors
            let tier_meds: Vec<f64> = tiers(sc).iter().map(|t| {
                let mut v: Vec<f64> = t.iter().filter_map(|a| p2.get(*a)).flatten().copied().collect();
                median(&mut v)
            }).collect();
            let ordered = tier_meds.windows(2).all(|w| w[0] > w[1]);
            let tier_meds1: Vec<f64> = tiers(sc).iter().map(|t| {
                let mut v: Vec<f64> = t.iter().filter_map(|a| p1.get(*a)).flatten().copied().collect();
                median(&mut v)
            }).collect();
            tier_rows.push(format!("| {sc} | {world} | {} | {} | {} |",
                tier_meds1.iter().map(|m| format!("{m:.0}")).collect::<Vec<_>>().join(" › "),
                tier_meds.iter().map(|m| format!("{m:.0}")).collect::<Vec<_>>().join(" › "),
                if ordered { "yes" } else { "no" }));
            // §9.2 rows, v1 → v2
            let wins = |runs: &[Run]| {
                let w: Vec<f64> = runs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
                let mut per: BTreeMap<u32, u32> = BTreeMap::new();
                for t in &w { *per.entry(*t as u32).or_default() += 1; }
                let top = per.values().max().copied().unwrap_or(0);
                let on = w.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let ott = runs.iter().filter(|r| r.win.is_some() && r.win_ottomans_alive).count();
                if w.is_empty() { "0".to_string() } else {
                    format!("{} ({}; 40–43: {on}; top {:.0} %; Ottomans alive {ott})", w.len(), q(&w), 100.0 * top as f64 / w.len() as f64)
                }
            };
            let outc = |runs: &[Run]| {
                let mut m: BTreeMap<String, u32> = BTreeMap::new();
                for r in runs { for o in &r.outcomes { *m.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                let multi = runs.iter().filter(|r| r.outcomes.len() > 1).count();
                if m.is_empty() { "—".to_string() } else {
                    format!("{} (>1 per game: {multi})", m.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "))
                }
            };
            let keys = |runs: &[Run]| key_actors(sc).iter().map(|k| format!("{k} {}", runs.iter().filter(|r| r.key_dead[*k]).count())).collect::<Vec<_>>().join(", ");
            let fork = |runs: &[Run]| if sc == "milan_1477" {
                format!("{} / {}", runs.iter().filter(|r| r.stab).count(), runs.iter().filter(|r| r.deep).count())
            } else { "—".into() };
            let split = |runs: &[Run]| if sc == "rome_375" { runs.iter().filter(|r| r.split40).count().to_string() } else { "—".into() };
            let deaths = |runs: &[Run]| runs.iter().map(|r| r.deaths).sum::<u32>();
            constraint_rows.push(format!("| {sc} | {world} | {} → {} | {} | {} → {} | {} → {} | {} → {} | {} → {} | {} → {} |",
                deaths(&v1), deaths(&v2), paired(&v1, &v2, |r| r.deaths as f64), keys(&v1), keys(&v2),
                wins(&v1), wins(&v2), split(&v1), split(&v2), outc(&v1), outc(&v2), fork(&v1), fork(&v2)));
        }
        let m1 = median(&mut inc.0.clone());
        let m2 = median(&mut inc.1.clone());
        income_rows.push(format!("| {sc} | {m1:.0} | {m2:.0} | {:.3} |", m2 / m1));
        for (i, label) in ["v1", "v2"].iter().enumerate() {
            let mut v: Vec<(String, f64)> = pooled_sources[i].iter().map(|(k, x)| (k.clone(), x / pooled_ticks[i].max(1) as f64)).collect();
            v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            let down: Vec<String> = v.iter().filter(|x| x.1 < -0.005).take(10).map(|(k, x)| format!("{k} {x:+.3}")).collect();
            let up: f64 = v.iter().filter(|x| x.1 > 0.0).map(|x| x.1).sum();
            let dn: f64 = v.iter().filter(|x| x.1 < 0.0).map(|x| x.1).sum();
            source_rows.push(format!("| {sc} | {label} | {up:+.3} | {dn:+.3} | {} |", down.join(", ")));
        }
    }

    println!("## 1. Ц1 measure per scenario and world (v2): actors with ceiling < 20 % and floor < 20 %, spread of medians\n");
    println!("| scenario | world | actors passing | spread of medians (max − min) | actor medians p10/50/90 |");
    println!("|---|---|---|---|---|");
    for r in world_rows { println!("{r}"); }
    println!("\n## 2. Tiers (brief §9.5): median of each tier's pooled living ticks, richest first\n");
    println!("| scenario | world | v1 | v2 | v2 ordered |");
    println!("|---|---|---|---|---|");
    for r in tier_rows { println!("{r}"); }
    println!("\n## 3. What moves economic_output: asked writes per living actor-tick, pooled over the worlds\n");
    println!("| scenario | model | inflow | outflow | outflows by source (largest first) |");
    println!("|---|---|---|---|---|");
    for r in source_rows { println!("{r}"); }
    println!("\n## 4. Income: median total income per game, all worlds\n");
    println!("| scenario | v1 | v2 | v2 / v1 |");
    println!("|---|---|---|---|");
    for r in income_rows { println!("{r}"); }
    println!("\n## 5. §9.2 constraints, v1 → v2 (for information)\n");
    println!("| scenario | world | deaths | deaths paired per seed (v2 − v1) | key deaths | wins (A10 / A35) | split on 40 | outcomes | regency stabilizes / deepens |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in constraint_rows { println!("{r}"); }
    println!("\n## 6. Per actor: economic_output ceiling / floor / median, v1 and v2\n");
    println!("| scenario | world | actor | v1 | v2 | passes |");
    println!("|---|---|---|---|---|---|");
    for r in actor_rows { println!("{r}"); }
}
