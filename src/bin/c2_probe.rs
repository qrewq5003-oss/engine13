//! Economy project, Ц2 stage 1 (docs/economy_project_brief.md §9). Measurement only.
//! Built with `--features census`. Every world of the three scenarios, 30 seeds × 300 ticks.
//!
//! 1. **Diagnosis: why v2 kills more in rome.** v2 as shipped after Ц1 stage 2 against v2 with
//!    the revived `economic_output` readers silenced in groups, paired by seed with t:
//!    (а) the three deficit rules (`economic_output_to_treasury`, `economic_output_to_population`,
//!    `low_economic_output_to_population_decay`) removed from the scenario in memory;
//!    (б) `famine` muted — it still rolls and is logged, its effects are not applied
//!    (`census::set_muted_event`), so the random stream is the same; (в) rome's auto-delta
//!    conditions «`rome.economic_output` less than …» removed (the auto-delta's own noise roll
//!    stays); (г) all three.
//! 2. **The debt rule** (v2, third part): treasury below zero `economy_v2_debt_ticks` = 4 ticks in
//!    a row → the army loses x of itself every tick until the treasury is back at zero or above;
//!    x = 0.05, 0.10, 0.20 against v2 without the rule. Per x: the share of living ticks in debt
//!    per actor, debt spells (median, p90), army cuts per game, Byzantium's army (military) and
//!    Milan's (aggressive), the Ottomans' treasury, deaths paired, the §9.2 rows; and the owner's
//!    pre-commitment: the smallest x with no actor in debt on more than 20 % of its living ticks
//!    and the p90 debt spell ≤ 12 ticks, in every world.
//!
//! Usage: cargo run --release --features census --bin c2_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const DEFICIT_RULES: [&str; 3] = ["economic_output_to_treasury", "economic_output_to_population", "low_economic_output_to_population_decay"];
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

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if s.is_empty() { return f64::NAN; }
    s[((s.len() - 1) as f64 * p).round() as usize]
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Cfg {
    deficit_off: bool,
    famine_off: bool,
    rome_conds_off: bool,
    cut: Option<f64>,
}

const BASE: Cfg = Cfg { deficit_off: false, famine_off: false, rome_conds_off: false, cut: None };

#[derive(Default)]
struct Run {
    deaths: u32,
    key_dead: BTreeMap<String, bool>,
    // per actor: (living ticks, ticks in debt), and debt spell lengths
    debt: BTreeMap<String, (u64, u64)>,
    spells: Vec<f64>,
    cuts: u32,
    army_byz: BTreeMap<usize, f64>,
    army_milan: BTreeMap<usize, f64>,
    ott_treasury: BTreeMap<usize, f64>,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

const MARKS: [usize; 5] = [25, 50, 100, 150, 299];

/// Which army series of a run to print.
type ArmyOf = fn(&Run) -> &BTreeMap<usize, f64>;

fn run(sc: &str, world: &str, c: Cfg, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        s.economy_v2_debt_cut = c.cut;
        if c.deficit_off {
            s.dependencies.retain(|d| !DEFICIT_RULES.contains(&d.id.as_str()));
        }
        if c.rome_conds_off {
            for ad in s.auto_deltas.iter_mut() {
                ad.conditions.retain(|cond| !(cond.metric.to_string() == "actor:rome.economic_output"
                    && matches!(cond.operator, engine13::core::ComparisonOperator::Less)));
            }
        }
    }
    census::set_muted_event(c.famine_off.then(|| "famine".to_string()));
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
        r.cuts += census::take_writes().iter().filter(|w| w.source.as_deref() == Some("debt")).count() as u32;
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick as usize - 1;
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let e = r.debt.entry(id.clone()).or_default();
            e.0 += 1;
            if a.get_metric("treasury") < 0.0 {
                e.1 += 1;
                *open.entry(id.clone()).or_default() += 1;
            } else if let Some(n) = open.remove(id) {
                r.spells.push(n as f64);
            }
        }
        if MARKS.contains(&t) {
            if let Some(b) = ws.actors.get("byzantium").filter(|_| !ws.dead_actor_ids.contains("byzantium")) { r.army_byz.insert(t, b.get_metric("military_size")); }
            if let Some(m) = ws.actors.get("milan").filter(|_| !ws.dead_actor_ids.contains("milan")) { r.army_milan.insert(t, m.get_metric("military_size")); }
            if let Some(o) = ws.actors.get("ottomans").filter(|_| !ws.dead_actor_ids.contains("ottomans")) { r.ott_treasury.insert(t, o.get_metric("treasury")); }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t as u32); }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    r.spells.extend(open.values().map(|n| *n as f64));
    census::set_muted_event(None);
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
    println!("# Ц2 stage 1 — diagnosis of v2's deaths, and the debt rule, {seeds} seeds × {ticks} ticks per world\n");

    let groups: [(&str, Cfg); 4] = [
        ("(а) deficit rules off", Cfg { deficit_off: true, ..BASE }),
        ("(б) famine muted", Cfg { famine_off: true, ..BASE }),
        ("(в) rome's eo-less conditions off", Cfg { rome_conds_off: true, ..BASE }),
        ("(г) all three", Cfg { deficit_off: true, famine_off: true, rome_conds_off: true, cut: None }),
    ];
    let mut diag = Vec::new();
    let mut debt_rows = Vec::new();
    let mut army_rows = Vec::new();
    let mut ott_rows = Vec::new();
    let mut constraint_rows = Vec::new();
    let mut verdict: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let base: Vec<Run> = (0..seeds).map(|s| run(sc, world, BASE, s, ticks)).collect();
            let cells: Vec<String> = groups.iter().map(|(_, c)| {
                let runs: Vec<Run> = (0..seeds).map(|s| run(sc, world, *c, s, ticks)).collect();
                paired(&base, &runs)
            }).collect();
            diag.push(format!("| {sc} | {world} | {} | {} |", base.iter().map(|r| r.deaths).sum::<u32>(), cells.join(" | ")));
            for cut in std::iter::once(None).chain(CUTS.iter().map(|x| Some(*x))) {
                let label = cut.map_or("v2 stage 2 (no rule)".to_string(), |x| format!("x = {:.0} %", x * 100.0));
                let runs: Vec<Run> = if cut.is_none() { (0..seeds).map(|s| run(sc, world, BASE, s, ticks)).collect() } else { (0..seeds).map(|s| run(sc, world, Cfg { cut, ..BASE }, s, ticks)).collect() };
                let mut per: BTreeMap<String, (u64, u64)> = BTreeMap::new();
                for r in &runs { for (k, v) in &r.debt { let e = per.entry(k.clone()).or_default(); e.0 += v.0; e.1 += v.1; } }
                let mut shares: Vec<(String, f64)> = per.iter().map(|(k, v)| (k.clone(), 100.0 * v.1 as f64 / v.0.max(1) as f64)).collect();
                shares.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let worst: Vec<String> = shares.iter().take(3).map(|(k, v)| format!("{k} {v:.0} %")).collect();
                let over = shares.iter().filter(|x| x.1 > 20.0).count();
                let spells: Vec<f64> = runs.iter().flat_map(|r| r.spells.iter().copied()).collect();
                let p90 = pct(&spells, 0.9);
                let cuts: Vec<f64> = runs.iter().map(|r| r.cuts as f64).collect();
                if cut.is_some() { verdict.entry(label.clone()).or_default().push((shares.first().map_or(0.0, |x| x.1), p90)); }
                debt_rows.push(format!("| {sc} | {world} | {label} | {} | {over} | {:.0} / {p90:.0} | {} | {} |",
                    worst.join(", "), pct(&spells, 0.5), q(&cuts), paired(&base, &runs)));
                if sc == "constantinople_1430" && *world == "military" || sc == "milan_1477" && *world == "aggressive" {
                    let (name, f): (&str, ArmyOf) = if sc == "milan_1477" { ("milan", |r| &r.army_milan) } else { ("byzantium", |r| &r.army_byz) };
                    let cells: Vec<String> = MARKS.iter().map(|t| q(&runs.iter().filter_map(|r| f(r).get(t).copied()).collect::<Vec<_>>())).collect();
                    army_rows.push(format!("| {sc} {world} | {name} | {label} | {} |", cells.join(" | ")));
                }
                if sc == "constantinople_1430" {
                    let cells: Vec<String> = MARKS.iter().map(|t| q(&runs.iter().filter_map(|r| r.ott_treasury.get(t).copied()).collect::<Vec<_>>())).collect();
                    ott_rows.push(format!("| {world} | {label} | {} |", cells.join(" | ")));
                }
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
                constraint_rows.push(format!("| {sc} | {world} | {label} | {} | {keys} | {wins_s} | {split} | {outc} | {fork} |", runs.iter().map(|r| r.deaths).sum::<u32>()));
            }
        }
    }
    println!("## 1. Diagnosis: deaths with the revived readers silenced, paired by seed against v2 (Ц1 stage 2)\n");
    println!("| scenario | world | deaths, v2 | {} |", groups.iter().map(|g| g.0).collect::<Vec<_>>().join(" | "));
    println!("|---|---|---|---|---|---|---|");
    for r in diag { println!("{r}"); }
    println!("\n## 2. The debt rule (4 ticks below zero → army −x per tick)\n");
    println!("| scenario | world | variant | most in debt (share of living ticks) | actors over 20 % | debt spell median / p90 | army cuts per game p10/50/90 | deaths paired vs v2 without the rule |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in debt_rows { println!("{r}"); }
    println!("\n## 3. Armies: Byzantium (military) and Milan (aggressive), p10/50/90 at ticks {MARKS:?}\n");
    println!("| world | actor | variant | {} |", MARKS.iter().map(|t| format!("tick {t}")).collect::<Vec<_>>().join(" | "));
    println!("|---|---|---|{}", "---|".repeat(MARKS.len()));
    for r in army_rows { println!("{r}"); }
    println!("\n## 4. The Ottomans' treasury, p10/50/90 at ticks {MARKS:?} (its sink belongs to Ц3)\n");
    println!("| world | variant | {} |", MARKS.iter().map(|t| format!("tick {t}")).collect::<Vec<_>>().join(" | "));
    println!("|---|---|{}", "---|".repeat(MARKS.len()));
    for r in ott_rows { println!("{r}"); }
    println!("\n## 5. §9.2 constraints (for information)\n");
    println!("| scenario | world | variant | deaths | key deaths | wins | split on 40 | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in constraint_rows { println!("{r}"); }
    println!("\n## 6. The owner's pre-commitment: no actor in debt > 20 % of living ticks, p90 spell ≤ 12, every world\n");
    println!("| variant | worlds passing the share | worlds passing p90 | worst share | worst p90 | passes |");
    println!("|---|---|---|---|---|---|");
    for x in CUTS {
        let label = format!("x = {:.0} %", x * 100.0);
        let v = &verdict[&label];
        let a = v.iter().filter(|x| x.0 <= 20.0).count();
        let b = v.iter().filter(|x| x.1 <= 12.0).count();
        let ws = v.iter().map(|x| x.0).fold(0.0, f64::max);
        let wp = v.iter().map(|x| x.1).fold(0.0, f64::max);
        println!("| {label} | {a} / {} | {b} / {} | {ws:.0} % | {wp:.0} | {} |", v.len(), v.len(), if a == v.len() && b == v.len() { "yes" } else { "no" });
    }
}
