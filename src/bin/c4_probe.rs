//! Economy project, Ц4 stage 1: a battle with an outcome (docs/economy_project_brief.md §9).
//! Built with `--features census`. Every world, 30 seeds × 300 ticks.
//!
//! Models: base = v2 as in the content (`e84341c`, no outcome); (a) the outcome on
//! (`economy_v2_combat_outcome`): strength `army × quality / 100`, winner by `S_a / (S_a + S_d)`,
//! the loser takes 15–30 % and the cohesion, the winner 5–15 % × min(1, S_l / S_w); (b) strength
//! without quality (`census::set_combat_quality(false)`) — the measure checked the other way;
//! (c) the winner's loss unscaled (`census::set_combat_loss_scaled(false)`) — what removing the
//! sink gives.
//!
//! Ц4's measure (pre-commitment): among battles with the army ratio in 0.8–1.25 and a quality gap
//! of at least 10, the side of higher quality wins ≥ 60 % — over all 10 worlds and per scenario;
//! (b) must give about 50 %, else the measure is empty. Stop rule: Ц1, Ц5, Ц6 (the corrected
//! "ceiling without a threat") at (a). For information: the Ottoman army (median on ticks 40–50,
//! the fall on ticks 2–5 by write source, also in v1), `mehmed_accelerates`, the armies of strong
//! powers and their neighbours' threat, Milan's battles, deaths paired against base, §9.2.
//!
//! Usage: cargo run --release --features census --bin c4_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];
const STRONG: [&str; 4] = ["rome", "ottomans", "venice", "milan"];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Model { V1, Base, A, B, C }

impl Model {
    fn label(&self) -> &'static str {
        match self {
            Model::V1 => "v1",
            Model::Base => "base (v2, e84341c)",
            Model::A => "(a) outcome",
            Model::B => "(b) no quality",
            Model::C => "(c) winner's loss unscaled",
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

fn share(x: u64, n: u64) -> f64 { 100.0 * x as f64 / n.max(1) as f64 }

#[derive(Default, Clone)]
struct Acc {
    living: u64,
    calm: u64,
    calm_floor: u64,
    calm_ceiling: u64,
    ceiling: u64,
    ceiling_no_threat: u64,
}

#[derive(Default)]
struct Run {
    acc: BTreeMap<String, Acc>,
    legit: BTreeMap<String, Vec<f64>>,
    eo: BTreeMap<String, Vec<f64>>,
    corr: [f64; 6],
    declines: Vec<(bool, u8)>,
    battles: Vec<census::Battle>,
    /// ottomans: army per tick (alive), and military_size writes by source on ticks 0–5
    ott_army: Vec<(u32, f64)>,
    ott_sources: BTreeMap<String, f64>,
    mehmed_tick: Option<u32>,
    /// strong powers: (army samples, neighbours' T_p samples)
    strong: BTreeMap<String, (Vec<f64>, Vec<f64>)>,
    ott_over_220: (u64, u64),
    vassal_ticks: u64,
    deaths: u32,
    dead: BTreeMap<String, u32>,
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
        assert!(!s.economy_v2_combat_outcome, "base is the content as at e84341c");
        s.features.economy_v2 = m != Model::V1;
        s.economy_v2_combat_outcome = matches!(m, Model::A | Model::B | Model::C);
    }
    census::set_combat_quality(m != Model::B);
    census::set_combat_loss_scaled(m != Model::C);
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    type Open = (String, u32, f64, f64, bool, f64, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    let _ = census::take_writes();
    let _ = census::take_battles();
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
        r.battles.extend(census::take_battles());
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        if t <= 5 {
            for w in &writes {
                if w.actor == "ottomans" && w.metric == "military_size" {
                    let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                    *r.ott_sources.entry(src).or_default() += w.applied;
                }
            }
        }
        if r.mehmed_tick.is_none() && ws.milestone_events_fired.iter().any(|mm| mm == "mehmed_accelerates") { r.mehmed_tick = Some(t); }
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) { r.dead.insert(d.clone(), t); }
        open.retain_mut(|(id, deadline, goal, drop, below, tp_new, tp_max, reached)| {
            let Some(a) = ws.actors.get(id).filter(|_| !ws.dead_actor_ids.contains(id)) else { return false };
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) { *tp_max = tp_max.max(tp); }
            if a.get_metric("external_pressure") <= *goal { *reached = true; }
            if t < *deadline { return true; }
            r.declines.push((*reached, if *below { 0 } else if *tp_max >= *tp_new + *drop / 2.0 { 1 } else { 2 }));
            false
        });
        r.vassal_ticks += ws.vassalages.len() as u64;
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let l = a.get_metric("legitimacy");
            let ep = a.get_metric("external_pressure");
            let tp = engine13::engine::pressure_threat(ws, id);
            let crisis = a.actor_tags.values().any(|tag| tag.metrics_modifier.iter().any(|(mm, v)| mm.as_str() == "legitimacy" && *v < 0));
            let e = r.acc.entry(id.clone()).or_default();
            e.living += 1;
            if !crisis {
                e.calm += 1;
                if l <= 1.0 { e.calm_floor += 1; }
                if l >= 99.0 { e.calm_ceiling += 1; }
            }
            if ep >= 99.0 {
                e.ceiling += 1;
                if tp.is_some_and(|x| x < 90.0) { e.ceiling_no_threat += 1; }
            }
            r.legit.entry(id.clone()).or_default().push(l);
            r.eo.entry(id.clone()).or_default().push(a.get_metric("economic_output"));
            if let Some(tp) = tp {
                r.corr[0] += 1.0; r.corr[1] += ep; r.corr[2] += tp; r.corr[3] += ep * ep; r.corr[4] += tp * tp; r.corr[5] += ep * tp;
                if let (Some(p), Some(e0)) = (prev_tp.get(id), prev_ep.get(id)) {
                    let drop = p - tp;
                    if drop >= 20.0 { open.push((id.clone(), t + 10, e0 - drop / 2.0, drop, e0 - drop / 2.0 < tp, tp, f64::MIN, false)); }
                }
                prev_tp.insert(id.clone(), tp);
            }
            prev_ep.insert(id.clone(), ep);
            if STRONG.contains(&id.as_str()) {
                let e = r.strong.entry(id.clone()).or_default();
                e.0.push(a.get_metric("military_size"));
                for nb in a.neighbors.iter().filter(|n| n.distance == 1 && !ws.dead_actor_ids.contains(&n.id)) {
                    if let Some(x) = engine13::engine::pressure_threat(ws, &nb.id) { e.1.push(x); }
                }
            }
            if id == "ottomans" {
                let army = a.get_metric("military_size");
                r.ott_army.push((t, army));
                r.ott_over_220.1 += 1;
                if army > 220.0 { r.ott_over_220.0 += 1; }
            }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|mm| mm == "rome_splits") { r.split40 = true; }
    }
    census::set_combat_quality(true);
    census::set_combat_loss_scaled(true);
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    r.outcomes = ws.milestone_events_fired.iter().filter(|mm| mm.starts_with("outcome_")).cloned().collect();
    r.stab = ws.milestone_events_fired.iter().any(|mm| mm == "milan_regency_stabilizes");
    r.deep = ws.milestone_events_fired.iter().any(|mm| mm == "milan_regency_crisis_deepens");
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

/// Ц4's measure over a set of battles: (battles counted, won by the side of higher quality)
fn c4_measure<'a>(battles: impl Iterator<Item = &'a census::Battle>) -> (u64, u64) {
    let mut n = 0;
    let mut won = 0;
    for b in battles {
        if b.army_defender <= 0.0 { continue; }
        let ratio = b.army_attacker / b.army_defender;
        if !(0.8..=1.25).contains(&ratio) || (b.quality_attacker - b.quality_defender).abs() < 10.0 { continue; }
        n += 1;
        if (b.quality_attacker > b.quality_defender) == b.attacker_won { won += 1; }
    }
    (n, won)
}

#[derive(Default)]
struct Stop {
    c1: [bool; 3],
    c5: [bool; 2],
    c6: [bool; 2],
    c6_decline: (u64, u64),
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    census::enable_battles();
    println!("# Ц4 stage 1 — a battle with an outcome, {seeds} seeds × {ticks} ticks per world\n");
    let models = [Model::Base, Model::A, Model::B, Model::C];
    let mut c4: BTreeMap<(String, &str), (u64, u64)> = BTreeMap::new();
    // (model, gap bin) -> (battles, won by higher quality, Σ of the (a) formula's p for that side)
    let mut gaps: BTreeMap<(&str, u8), (u64, u64, f64)> = BTreeMap::new();
    let mut battle_rows = Vec::new();
    let mut stop: BTreeMap<&str, Vec<Stop>> = BTreeMap::new();
    let mut stop_rows = Vec::new();
    let mut ott_rows = Vec::new();
    let mut ott_src_rows = Vec::new();
    let mut mehmed_rows = Vec::new();
    let mut strong_rows = Vec::new();
    let mut milan_rows = Vec::new();
    let mut death_rows = Vec::new();
    let mut info_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let base: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::Base, s, ticks)).collect();
            if sc == "constantinople_1430" && *world == "none" {
                let v1: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::V1, s, ticks)).collect();
                for (label, runs) in [("v1", &v1), ("base (v2, e84341c)", &base)] {
                    let mut src: BTreeMap<String, f64> = BTreeMap::new();
                    for rr in runs.iter() { for (k, x) in &rr.ott_sources { *src.entry(k.clone()).or_default() += x / seeds as f64; } }
                    let army = |tk: u32| -> f64 { let v: Vec<f64> = runs.iter().filter_map(|rr| rr.ott_army.iter().find(|x| x.0 == tk).map(|x| x.1)).collect(); v.iter().sum::<f64>() / v.len().max(1) as f64 };
                    let mut v: Vec<(String, f64)> = src.into_iter().collect();
                    v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                    ott_src_rows.push(format!("| {label} | {:.0} → {:.0} → {:.0} → {:.0} | {} |", army(0), army(2), army(3), army(5),
                        v.iter().map(|(k, x)| format!("{k} {x:+.1}")).collect::<Vec<_>>().join("; ")));
                }
            }
            for m in models {
                let fresh: Vec<Run>;
                let runs: &[Run] = if m == Model::Base { &base } else { fresh = (0..seeds).map(|s| run(sc, world, m, s, ticks)).collect(); &fresh };
                let label = m.label();
                // Ц4 measure
                if m != Model::Base {
                    let (n, won) = c4_measure(runs.iter().flat_map(|rr| rr.battles.iter()));
                    let e = c4.entry((sc.to_string(), label)).or_default();
                    e.0 += n; e.1 += won;
                    for b in runs.iter().flat_map(|rr| rr.battles.iter()) {
                        if b.army_defender <= 0.0 { continue; }
                        let ratio = b.army_attacker / b.army_defender;
                        let gap = (b.quality_attacker - b.quality_defender).abs();
                        if !(0.8..=1.25).contains(&ratio) || gap < 10.0 { continue; }
                        let hq_att = b.quality_attacker > b.quality_defender;
                        let (s_a, s_d) = (b.army_attacker * b.quality_attacker / 100.0, b.army_defender * b.quality_defender / 100.0);
                        let p_att = if s_a + s_d > 0.0 { s_a / (s_a + s_d) } else { 0.5 };
                        let bin = if gap < 20.0 { 0 } else if gap < 30.0 { 1 } else { 2 };
                        let e = gaps.entry((label, bin)).or_default();
                        e.0 += 1;
                        e.1 += (hq_att == b.attacker_won) as u64;
                        e.2 += if hq_att { p_att } else { 1.0 - p_att };
                    }
                    let all: Vec<&census::Battle> = runs.iter().flat_map(|rr| rr.battles.iter()).collect();
                    let att = all.iter().filter(|b| b.attacker_won).count();
                    battle_rows.push(format!("| {sc} | {world} | {label} | {} | {:.0} % | {n} | {:.0} % |", all.len(), share(att as u64, all.len() as u64), share(won, n)));
                }
                // stop rule
                let mut pool: BTreeMap<String, Acc> = BTreeMap::new();
                for rr in runs { for (k, a) in &rr.acc { let e = pool.entry(k.clone()).or_default(); e.living += a.living; e.calm += a.calm; e.calm_floor += a.calm_floor; e.calm_ceiling += a.calm_ceiling; e.ceiling += a.ceiling; e.ceiling_no_threat += a.ceiling_no_threat; } }
                let calm: Vec<&Acc> = pool.values().filter(|a| a.calm > 0).collect();
                let c5_pass = calm.iter().filter(|a| share(a.calm_floor, a.calm) < 20.0 && share(a.calm_ceiling, a.calm) < 20.0).count();
                let med = |m: &BTreeMap<String, Vec<f64>>| -> Vec<f64> { m.values().map(|x| pct(x, 0.5)).collect() };
                let spread = |v: &[f64]| v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min);
                let mut legit: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for rr in runs {
                    for (k, x) in &rr.legit { legit.entry(k.clone()).or_default().extend(x); }
                    for (k, x) in &rr.eo { eo.entry(k.clone()).or_default().extend(x); }
                }
                let lspread = spread(&med(&legit));
                let frac = |x: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * x.iter().filter(|y| pr(**y)).count() as f64 / x.len().max(1) as f64;
                let c1_pass = eo.values().filter(|x| frac(x, &|y| y >= 99.0) < 20.0 && frac(x, &|y| y <= 1.0) < 20.0).count();
                let espread = spread(&med(&eo));
                let tm: Vec<f64> = tiers(sc).iter().map(|t| { let x: Vec<f64> = t.iter().filter_map(|a| eo.get(*a)).flatten().copied().collect(); pct(&x, 0.5) }).collect();
                let ordered = tm.windows(2).all(|w| w[0] > w[1]);
                let (lv, cnt, cnt_nt) = pool.values().fold((0, 0, 0), |s, a| (s.0 + a.living, s.1 + a.ceiling, s.2 + a.ceiling_no_threat));
                let c = runs.iter().fold([0.0; 6], |mut s, rr| { for (x, y) in s.iter_mut().zip(&rr.corr) { *x += y; } s });
                let (n, sx, sy, sxx, syy, sxy) = (c[0], c[1], c[2], c[3], c[4], c[5]);
                let corr = (n * sxy - sx * sy) / ((n * sxx - sx * sx).sqrt() * (n * syy - sy * sy).sqrt());
                let counted: Vec<&(bool, u8)> = runs.iter().flat_map(|rr| rr.declines.iter()).filter(|d| d.1 == 2).collect();
                let ok = counted.iter().filter(|d| d.0).count();
                stop_rows.push(format!("| {sc} | {world} | {label} | {c1_pass} / {} · {} · {espread:.0} | {c5_pass} / {} · {lspread:.0} | {:.0} % ({:.0} % at the ceiling) · {corr:.2} · {} cases {:.0} % |",
                    eo.len(), if ordered { "ordered" } else { "**not ordered**" }, calm.len(), share(cnt_nt, lv), share(cnt, lv), counted.len(), share(ok as u64, counted.len() as u64)));
                stop.entry(label).or_default().push(Stop {
                    c1: [100.0 * c1_pass as f64 / eo.len().max(1) as f64 >= 80.0, ordered, espread >= 20.0],
                    c5: [100.0 * c5_pass as f64 / calm.len().max(1) as f64 >= 80.0, lspread >= 15.0],
                    c6: [share(cnt_nt, lv) < 30.0, corr >= 0.7],
                    c6_decline: (counted.len() as u64, ok as u64),
                });
                // Ottomans
                if sc == "constantinople_1430" {
                    let mid: Vec<f64> = runs.iter().flat_map(|rr| rr.ott_army.iter().filter(|x| (40..=50).contains(&x.0)).map(|x| x.1)).collect();
                    let (o, on) = runs.iter().fold((0, 0), |s, rr| (s.0 + rr.ott_over_220.0, s.1 + rr.ott_over_220.1));
                    ott_rows.push(format!("| {world} | {label} | {} | {:.0} % |", q(&mid), share(o, on)));
                    if m != Model::Base {
                        let (mut b_n, mut b_w, mut a_n, mut a_w, mut games) = (0u64, 0u64, 0u64, 0u64, 0);
                        for rr in runs {
                            let Some(mt) = rr.mehmed_tick else { continue };
                            games += 1;
                            for b in rr.battles.iter().filter(|b| b.attacker == "ottomans" || b.defender == "ottomans") {
                                let won = (b.attacker == "ottomans") == b.attacker_won;
                                if b.tick < mt { b_n += 1; b_w += won as u64; } else { a_n += 1; a_w += won as u64; }
                            }
                        }
                        mehmed_rows.push(format!("| {world} | {label} | {games} | {b_n}: {:.0} % | {a_n}: {:.0} % |", share(b_w, b_n), share(a_w, a_n)));
                    }
                }
                // strong powers
                for p in STRONG {
                    let army: Vec<f64> = runs.iter().filter_map(|rr| rr.strong.get(p)).flat_map(|x| x.0.iter().copied()).collect();
                    if army.is_empty() { continue; }
                    let tp: Vec<f64> = runs.iter().filter_map(|rr| rr.strong.get(p)).flat_map(|x| x.1.iter().copied()).collect();
                    strong_rows.push(format!("| {sc} | {world} | {p} | {label} | {} | {:.0} |", q(&army), tp.iter().sum::<f64>() / tp.len().max(1) as f64));
                }
                if sc == "milan_1477" && m != Model::Base {
                    let mb: Vec<&census::Battle> = runs.iter().flat_map(|rr| rr.battles.iter()).filter(|b| b.attacker == "milan" || b.defender == "milan").collect();
                    let won = mb.iter().filter(|b| (b.attacker == "milan") == b.attacker_won).count();
                    milan_rows.push(format!("| {world} | {label} | {} | {:.0} % | {} / {seeds} |", mb.len(), share(won as u64, mb.len() as u64), runs.iter().filter(|rr| rr.dead.contains_key("milan")).count()));
                }
                // deaths, §9.2
                let keys: Vec<String> = KEY.iter().filter(|k| base[0].acc.contains_key(**k)).map(|k| {
                    format!("{k} {}→{} ({})", base.iter().filter(|rr| rr.dead.contains_key(*k)).count(), runs.iter().filter(|rr| rr.dead.contains_key(*k)).count(),
                        paired(&base, runs, |rr| if rr.dead.contains_key(*k) { 1.0 } else { 0.0 }))
                }).collect();
                death_rows.push(format!("| {sc} | {world} | {label} | {} | {} | {} |", runs.iter().map(|rr| rr.deaths).sum::<u32>(), paired(&base, runs, |rr| rr.deaths as f64), keys.join("; ")));
                let wins: Vec<f64> = runs.iter().filter_map(|rr| rr.win.map(|t| t as f64)).collect();
                let mut perw: BTreeMap<u32, u32> = BTreeMap::new();
                for t in &wins { *perw.entry(*t as u32).or_default() += 1; }
                let top = perw.values().max().copied().unwrap_or(0);
                let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let wins_s = if wins.is_empty() { "0".into() } else { format!("{} ({}; 40–43: {on}; top {:.0} %)", wins.len(), q(&wins), 100.0 * top as f64 / wins.len() as f64) };
                let mut oc: BTreeMap<String, u32> = BTreeMap::new();
                for rr in runs { for o in &rr.outcomes { *oc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                let outc = if oc.is_empty() { "—".into() } else { oc.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ") };
                let fork = if sc == "milan_1477" { format!("{} / {}", runs.iter().filter(|rr| rr.stab).count(), runs.iter().filter(|rr| rr.deep).count()) } else { "—".into() };
                let split = if sc == "rome_375" { runs.iter().filter(|rr| rr.split40).count().to_string() } else { "—".into() };
                let hist = match (sc, *world) {
                    ("rome_375", _) => format!("Rome dies {} / {seeds}", runs.iter().filter(|rr| rr.dead.contains_key("rome")).count()),
                    ("constantinople_1430", "none") => {
                        let ft: Vec<f64> = runs.iter().filter_map(|rr| rr.dead.get("byzantium").map(|t| *t as f64)).collect();
                        format!("Byzantium falls {} / {seeds}, tick {}", ft.len(), q(&ft))
                    }
                    ("milan_1477", _) => format!("Milan dies {} / {seeds}", runs.iter().filter(|rr| rr.dead.contains_key("milan")).count()),
                    _ => "—".into(),
                };
                info_rows.push(format!("| {sc} | {world} | {label} | {hist} | {wins_s} | {split} | {outc} | {fork} | {} |", runs.iter().map(|rr| rr.vassal_ticks).sum::<u64>()));
            }
        }
    }
    println!("## 1. Ц4's measure: battles with army ratio 0.8–1.25 and a quality gap ≥ 10 — won by the side of higher quality\n");
    println!("| scenario | model | battles counted | won by higher quality |");
    println!("|---|---|---|---|");
    for m in [Model::A, Model::B, Model::C] {
        let mut tot = (0, 0);
        for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
            let (n, w) = c4.get(&(sc.to_string(), m.label())).copied().unwrap_or((0, 0));
            tot.0 += n; tot.1 += w;
            println!("| {sc} | {} | {n} | {:.1} % |", m.label(), share(w, n));
        }
        println!("| **all 10 worlds** | {} | {} | **{:.1} %** |", m.label(), tot.0, share(tot.1, tot.0));
    }
    println!("\n### By the quality gap: realised against what the (a) formula S = army × quality / 100 gives the side of higher quality\n");
    println!("| model | quality gap | battles | won by higher quality | the formula's mean p for that side |");
    println!("|---|---|---|---|---|");
    for ((m, bin), (n, w, p)) in &gaps {
        println!("| {m} | {} | {n} | {:.1} % | {:.1} % |", ["10–20", "20–30", "≥ 30"][*bin as usize], share(*w, *n), 100.0 * p / (*n).max(1) as f64);
    }
    println!("\n### Per world\n");
    println!("| scenario | world | model | battles | won by the attacker | counted | won by higher quality |");
    println!("|---|---|---|---|---|---|---|");
    for r in battle_rows { println!("{r}"); }
    println!("\n## 2. Stop rule: Ц1, Ц5, Ц6\n");
    println!("| scenario | world | model | Ц1: actors · tiers · eo spread | Ц5: actors · legitimacy spread | Ц6: ceiling without a threat (at the ceiling) · corr · decline, corrected |");
    println!("|---|---|---|---|---|---|");
    for r in stop_rows { println!("{r}"); }
    println!();
    println!("| model | Ц1 actors / tiers / spread | Ц5 actors / spread | Ц6 ceiling without a threat / corr | Ц6 decline, corrected |");
    println!("|---|---|---|---|---|");
    for m in models {
        let v = &stop[m.label()];
        let n = v.len();
        let c = |f: &dyn Fn(&Stop) -> bool| v.iter().filter(|x| f(x)).count();
        let (dn, dok) = v.iter().fold((0, 0), |s, x| (s.0 + x.c6_decline.0, s.1 + x.c6_decline.1));
        println!("| {} | {} / {} / {} of {n} | {} / {} of {n} | {} / {} of {n} | {:.0} % of {dn} |", m.label(), c(&|x| x.c1[0]), c(&|x| x.c1[1]), c(&|x| x.c1[2]),
            c(&|x| x.c5[0]), c(&|x| x.c5[1]), c(&|x| x.c6[0]), c(&|x| x.c6[1]), share(dok, dn));
    }
    println!("\n## 3. The Ottoman army\n");
    println!("### The fall on ticks 2–5 (constantinople, no player): mean army on ticks 0 → 2 → 3 → 5, and military_size writes on ticks 0–5 by source (mean per game)\n");
    println!("| model | army | writes by source |");
    println!("|---|---|---|");
    for r in ott_src_rows { println!("{r}"); }
    println!("\n### Median army on ticks 40–50 (start 180) and the share of living ticks above 220 (the coalition tax, A10)\n");
    println!("| world | model | army ticks 40–50 p10/50/90 | ticks above 220 |");
    println!("|---|---|---|---|");
    for r in ott_rows { println!("{r}"); }
    println!("\n### `mehmed_accelerates` (−15 quality): Ottoman battles won before and after it, in games where it fired\n");
    println!("| world | model | games | before: battles, won | after: battles, won |");
    println!("|---|---|---|---|---|");
    for r in mehmed_rows { println!("{r}"); }
    println!("\n## 4. Strong powers: army over the game, and the mean threat T_p of their neighbours at distance 1\n");
    println!("| scenario | world | power | model | army p10/50/90 | neighbours' T_p |");
    println!("|---|---|---|---|---|---|");
    for r in strong_rows { println!("{r}"); }
    println!("\n## 5. Milan's battles\n");
    println!("| world | model | battles | won by Milan | Milan dies |");
    println!("|---|---|---|---|---|");
    for r in milan_rows { println!("{r}"); }
    println!("\n## 6. Deaths paired by seed against base\n");
    println!("| scenario | world | model | deaths | per seed | key actors (games, paired) |");
    println!("|---|---|---|---|---|---|");
    for r in death_rows { println!("{r}"); }
    println!("\n## 7. §9.2 (no gate)\n");
    println!("| scenario | world | model | key actor | wins | split on 40 | outcomes | regency stab / deep | vassalage ticks |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in info_rows { println!("{r}"); }
}
