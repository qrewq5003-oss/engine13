//! Economy project, Ц2 stage 2: debt is unpaid soldiers' pay (docs/economy_project_brief.md §9).
//! Built with `--features census`. Every world of the three scenarios, 30 seeds × 300 ticks.
//!
//! v2 (Ц2 stage 2): only the army's upkeep takes a treasury below zero — events, milestone
//! effects and dependency rules stop at zero; an actor in debt does not recruit (no levy
//! without money). Then the debt rule (4 ticks below zero → the army loses x a tick) for
//! x = 0.05, 0.10, 0.20. Compared, paired by seed, with v2 before the stage (no floor,
//! recruiting in debt — `census::set_debt_as_pay_off`, i.e. v2 at `bad8f63`).
//!
//! Per x: the share of living ticks in debt per actor, debt spells, armies (Byzantium in
//! military, Milan with the bot, the Huns, the Ostrogoths), what each source lost at the zero
//! floor per actor, deaths (key actors and all) paired with t, the §9.2 rows; and the owner's
//! pre-commitment: no actor in debt on more than 20 % of its living ticks, the p90 spell ≤ 12
//! ticks, and no key actor's deaths rising significantly (t < 3), in every world.
//!
//! Usage: cargo run --release --features census --bin c2s2_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const CUTS: [f64; 3] = [0.05, 0.10, 0.20];
const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];
const ARMIES: [(&str, &str, &str); 4] = [
    ("constantinople_1430", "military", "byzantium"),
    ("milan_1477", "aggressive", "milan"),
    ("rome_375", "none", "huns"),
    ("rome_375", "none", "ostrogoths"),
];
const MARKS: [usize; 5] = [10, 25, 50, 100, 200];

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
enum Model { Before, After(Option<f64>) }

impl Model {
    fn label(&self) -> String {
        match self {
            Model::Before => "v2 before (bad8f63)".into(),
            Model::After(None) => "pay rules, no debt rule".into(),
            Model::After(Some(x)) => format!("pay rules, debt x = {:.0} %", x * 100.0),
        }
    }
}

#[derive(Default)]
struct Run {
    debt: BTreeMap<String, (u64, u64)>,
    spells: Vec<f64>,
    armies: BTreeMap<(String, usize), f64>,
    floor: BTreeMap<(String, String), f64>,
    deaths: u32,
    key_dead: BTreeMap<String, bool>,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

fn run(sc: &str, world: &str, m: Model, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        s.economy_v2_debt_cut = if let Model::After(x) = m { x } else { None };
    }
    census::set_debt_as_pay_off(m == Model::Before);
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut open: BTreeMap<String, u32> = BTreeMap::new();
    let _ = census::take_floor_losses();
    for _ in 0..ticks {
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let _ = census::take_writes();
        for (a, s, x) in census::take_floor_losses() {
            let s = s.split(' ').take(2).collect::<Vec<_>>().join(" ");
            *r.floor.entry((a, s)).or_default() += x;
        }
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick as usize - 1;
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let d = r.debt.entry(id.clone()).or_default();
            d.0 += 1;
            if a.get_metric("treasury") < 0.0 {
                d.1 += 1;
                *open.entry(id.clone()).or_default() += 1;
            } else if let Some(n) = open.remove(id) {
                r.spells.push(n as f64);
            }
        }
        if MARKS.contains(&t) {
            for (s2, w2, actor) in ARMIES {
                if s2 == sc && w2 == world {
                    if let Some(a) = ws.actors.get(actor).filter(|_| !ws.dead_actor_ids.contains(actor)) {
                        r.armies.insert((actor.to_string(), t), a.get_metric("military_size"));
                    }
                }
            }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t as u32); }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    r.spells.extend(open.values().map(|n| *n as f64));
    census::set_debt_as_pay_off(false);
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    for k in KEY {
        if st.current_scenario.as_ref().unwrap().actors.iter().any(|a| a.id == k && !a.is_successor_template) {
            r.key_dead.insert(k.to_string(), ws.dead_actor_ids.contains(k));
        }
    }
    r.outcomes = ws.milestone_events_fired.iter().filter(|m| m.starts_with("outcome_")).cloned().collect();
    r.stab = ws.milestone_events_fired.iter().any(|m| m == "milan_regency_stabilizes");
    r.deep = ws.milestone_events_fired.iter().any(|m| m == "milan_regency_crisis_deepens");
    r
}

/// Paired difference b − a per seed: (mean, t).
fn paired(a: &[Run], b: &[Run], f: impl Fn(&Run) -> f64) -> (f64, f64) {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| f(y) - f(x)).collect();
    let n = d.len() as f64;
    let mean = d.iter().sum::<f64>() / n;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else if mean > 0.0 { f64::INFINITY } else { 0.0 };
    (mean, t)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_floor_losses();
    // the floor losses take the label of the current write source, which is kept only while
    // the write sink is on
    census::enable_writes();
    println!("# Ц2 stage 2 — debt as unpaid pay, and the debt rule again, {seeds} seeds × {ticks} ticks per world\n");
    let models: Vec<Model> = std::iter::once(Model::After(None)).chain(CUTS.iter().map(|x| Model::After(Some(*x)))).collect();
    let mut debt_rows = Vec::new();
    let mut army_rows = Vec::new();
    let mut floor_rows: BTreeMap<String, BTreeMap<(String, String), f64>> = BTreeMap::new();
    let mut floor_games: BTreeMap<String, f64> = BTreeMap::new();
    let mut death_rows = Vec::new();
    let mut constraint_rows = Vec::new();
    let mut verdict: BTreeMap<String, Vec<(f64, f64, f64)>> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let before: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::Before, s, ticks)).collect();
            for m in std::iter::once(Model::Before).chain(models.iter().copied()) {
                let runs: Vec<Run> = if m == Model::Before { (0..seeds).map(|s| run(sc, world, m, s, ticks)).collect() } else { (0..seeds).map(|s| run(sc, world, m, s, ticks)).collect() };
                let mut per: BTreeMap<String, (u64, u64)> = BTreeMap::new();
                for r in &runs { for (k, d) in &r.debt { let e = per.entry(k.clone()).or_default(); e.0 += d.0; e.1 += d.1; } }
                let mut shares: Vec<(String, f64)> = per.iter().map(|(k, d)| (k.clone(), 100.0 * d.1 as f64 / d.0.max(1) as f64)).collect();
                shares.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let over: Vec<String> = shares.iter().filter(|s| s.1 > 20.0).map(|(k, v)| format!("{k} {v:.0} %")).collect();
                let spells: Vec<f64> = runs.iter().flat_map(|r| r.spells.iter().copied()).collect();
                let p90 = pct(&spells, 0.9);
                // key deaths paired against before
                let mut key_cells = Vec::new();
                let mut worst_t: f64 = f64::NEG_INFINITY;
                for k in KEY {
                    if runs.first().is_some_and(|r| r.key_dead.contains_key(k)) {
                        let (mean, t) = paired(&before, &runs, |r| if r.key_dead[k] { 1.0 } else { 0.0 });
                        worst_t = worst_t.max(t);
                        key_cells.push(format!("{k} {} → {} ({mean:+.2}, t {t:+.1})",
                            before.iter().filter(|r| r.key_dead[k]).count(), runs.iter().filter(|r| r.key_dead[k]).count()));
                    }
                }
                let (dm, dt) = paired(&before, &runs, |r| r.deaths as f64);
                if let Model::After(Some(x)) = m {
                    verdict.entry(format!("{:.0} %", x * 100.0)).or_default().push((shares.first().map_or(0.0, |s| s.1), p90, worst_t));
                }
                debt_rows.push(format!("| {sc} | {world} | {} | {} | {:.0} / {p90:.0} |", m.label(),
                    if over.is_empty() { "—".into() } else { over.join(", ") }, pct(&spells, 0.5)));
                death_rows.push(format!("| {sc} | {world} | {} | {} | {dm:+.2} (t {dt:+.1}) | {} |", m.label(),
                    runs.iter().map(|r| r.deaths).sum::<u32>(), key_cells.join("; ")));
                for (s2, w2, actor) in ARMIES {
                    if s2 == sc && w2 == *world {
                        let cells: Vec<String> = MARKS.iter().map(|t| q(&runs.iter().filter_map(|r| r.armies.get(&(actor.to_string(), *t)).copied()).collect::<Vec<_>>())).collect();
                        army_rows.push(format!("| {sc} {world} | {actor} | {} | {} |", m.label(), cells.join(" | ")));
                    }
                }
                if m != Model::Before {
                    let e = floor_rows.entry(format!("{sc} | {}", m.label())).or_default();
                    for r in &runs { for (k, x) in &r.floor { *e.entry(k.clone()).or_default() += x; } }
                    *floor_games.entry(format!("{sc} | {}", m.label())).or_default() += runs.len() as f64;
                }
                if m == Model::Before || m == Model::After(None) || m == Model::After(Some(0.10)) {
                    let wins: Vec<f64> = runs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
                    let mut perw: BTreeMap<u32, u32> = BTreeMap::new();
                    for t in &wins { *perw.entry(*t as u32).or_default() += 1; }
                    let top = perw.values().max().copied().unwrap_or(0);
                    let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                    let wins_s = if wins.is_empty() { "0".into() } else { format!("{} ({}; 40–43: {on}; top {:.0} %)", wins.len(), q(&wins), 100.0 * top as f64 / wins.len() as f64) };
                    let mut oc: BTreeMap<String, u32> = BTreeMap::new();
                    for r in &runs { for o in &r.outcomes { *oc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                    let multi = runs.iter().filter(|r| r.outcomes.len() > 1).count();
                    let outc = if oc.is_empty() { "—".into() } else { format!("{} (>1: {multi})", oc.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")) };
                    let fork = if sc == "milan_1477" { format!("{} / {}", runs.iter().filter(|r| r.stab).count(), runs.iter().filter(|r| r.deep).count()) } else { "—".into() };
                    let split = if sc == "rome_375" { runs.iter().filter(|r| r.split40).count().to_string() } else { "—".into() };
                    constraint_rows.push(format!("| {sc} | {world} | {} | {wins_s} | {split} | {outc} | {fork} |", m.label()));
                }
            }
        }
    }
    println!("## 1. Debt: actors in debt on more than 20 % of living ticks, debt spells\n");
    println!("| scenario | world | model | actors in debt > 20 % | spell median / p90 |");
    println!("|---|---|---|---|---|");
    for r in debt_rows { println!("{r}"); }
    println!("\n## 2. Deaths paired against v2 before (bad8f63): all, and key actors (deaths before → after, mean per seed, t)\n");
    println!("| scenario | world | model | deaths | all, per seed | key actors |");
    println!("|---|---|---|---|---|---|");
    for r in death_rows { println!("{r}"); }
    println!("\n## 3. Armies, p10/50/90 at ticks {MARKS:?}\n");
    println!("| world | actor | model | {} |", MARKS.iter().map(|t| format!("tick {t}")).collect::<Vec<_>>().join(" | "));
    println!("|---|---|---|{}", "---|".repeat(MARKS.len()));
    for r in army_rows { println!("{r}"); }
    println!("\n## 4. Lost at the zero floor, per game, by actor and source (largest)\n");
    println!("| scenario | model | lost at the floor |");
    println!("|---|---|---|");
    for (k, m) in &floor_rows {
        let n = floor_games[k];
        let mut v: Vec<(String, f64)> = m.iter().map(|((a, s), x)| (format!("{a} · {s}"), x / n)).collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let cells: Vec<String> = v.iter().filter(|x| x.1 >= 1.0).take(10).map(|(k, x)| format!("{k} {x:.0}")).collect();
        println!("| {k} | {} |", cells.join(", "));
    }
    println!("\n## 5. §9.2 constraints (for information)\n");
    println!("| scenario | world | model | wins | split on 40 | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|");
    for r in constraint_rows { println!("{r}"); }
    println!("\n## 6. The owner's pre-commitment\n");
    println!("| x | worlds where no actor is in debt > 20 % | worlds with p90 spell ≤ 12 | worlds with every key actor's deaths t < 3 | worst share | worst p90 | worst t | passes |");
    println!("|---|---|---|---|---|---|---|---|");
    for x in CUTS {
        let key = format!("{:.0} %", x * 100.0);
        let v = &verdict[&key];
        let a = v.iter().filter(|s| s.0 <= 20.0).count();
        let b = v.iter().filter(|s| s.1 <= 12.0).count();
        let c = v.iter().filter(|s| s.2 < 3.0).count();
        println!("| {key} | {a} / {n} | {b} / {n} | {c} / {n} | {:.0} % | {:.0} | {:.1} | {} |",
            v.iter().map(|s| s.0).fold(0.0, f64::max), v.iter().map(|s| s.1).fold(0.0, f64::max), v.iter().map(|s| s.2).fold(f64::NEG_INFINITY, f64::max),
            if a == v.len() && b == v.len() && c == v.len() { "yes" } else { "no" }, n = v.len());
    }
}
