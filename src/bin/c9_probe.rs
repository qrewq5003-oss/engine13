//! Economy project, Ц9 «alliances» together with Ц7 (docs/economy_project_brief.md §9). Built with
//! `--features census`. Every world, 30 seeds × 300 ticks; the seeds are an argument (0 and 100).
//!
//! Models: base (v2 as in the content: Ц1, Ц4, Ц5, Ц6, Ц8; Ц7 and Ц9 off), Ц7 (K₂ = 1, no
//! alliances), Ц9 (K₂ = 1 and `economy_v2_alliances`: milan's Italian League from the start, the
//! league turning on Milan, the Savoy alliance by action).
//!
//! The owner's pre-commitment (2026-10-07), Ц9 with Ц7:
//! 1. Ц7's measure, items 1–4 — item 3 (milan: Milan ≤ 1 of 30, papacy and Venice each ≤ 3 of 30
//!    die or submit, every world) must pass.
//! 2. The historical pairs of PR #241 hold within noise against Ц7 without Ц9: Florence → Siena,
//!    Sassanids → Armenia, Huns → Ostrogoths, Ottomans → Serbia and Byzantium, Rome → the five
//!    peoples. Naples → Sicily goes on purpose (Sicily is in the league) — reported, not gated.
//!    «Within noise», fixed before the run: per world, the mean paired difference of the games
//!    with the submission (for Rome: of the peoples submitting a game) is within 2 standard errors
//!    of the paired differences; with no spread at all, the difference is 0.
//! 3. Stop rule (§9.6): Ц1, Ц4, Ц5, Ц6, Ц8 against base on the same seeds — a stop where base
//!    passes and Ц9 does not.
//!
//! For information: when the league turns on Milan, Milan's deaths and wins, the regency fork.
//!
//! Usage: cargo run --release --features census --bin c9_probe -- [first_seed] [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

const PEOPLES: [&str; 5] = ["alamanni", "vandals", "visigoths", "burgundians", "franks"];
const SCENARIOS: [&str; 3] = ["rome_375", "constantinople_1430", "milan_1477"];
const MODELS: [&str; 3] = ["base", "Ц7", "Ц9"];

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

#[derive(Default)]
struct Run {
    battles: Vec<census::Battle>,
    eo: BTreeMap<String, Vec<f64>>,
    legit: BTreeMap<String, Vec<f64>>,
    coh: BTreeMap<String, Vec<f64>>,
    calm: BTreeMap<String, (u64, u64, u64)>,
    living: u64,
    ceiling_no_threat: u64,
    corr: [f64; 6],
    declines: Vec<(bool, u8)>,
    /// (vassal, overlord) -> first tick
    vassal: BTreeMap<(String, String), u32>,
    dead: BTreeMap<String, u32>,
    conquered_by: BTreeMap<String, String>,
    deaths: u32,
    league_turns: Option<u32>,
    savoy_alliance: Option<u32>,
    win: Option<u32>,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

/// model: 0 = base, 1 = Ц7, 2 = Ц9
fn run(sc: &str, world: &str, model: usize, quality: bool, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        assert!(s.economy_v2_conquest_k2.is_none() && !s.economy_v2_alliances, "Ц7 and Ц9 are off in the content");
        if model >= 1 { s.economy_v2_conquest_k2 = Some(1); }
        s.economy_v2_alliances = model == 2;
    }
    census::set_combat_quality(quality);
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    type Open = (String, u32, f64, f64, bool, f64, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    let _ = census::take_battles();
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
        r.battles.extend(census::take_battles());
        let _ = census::take_writes();
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) { r.dead.insert(d.clone(), t); }
        for v in &ws.vassalages { r.vassal.entry((v.vassal_id.clone(), v.overlord_id.clone())).or_insert(t); }
        if r.league_turns.is_none() && ws.fired_events.contains("italian_league_against_milan") { r.league_turns = Some(t); }
        if r.savoy_alliance.is_none() && engine13::engine::interactions::allied(ws, "milan", "savoy") { r.savoy_alliance = Some(t); }
        open.retain_mut(|(id, deadline, goal, drop, below, tp_new, tp_max, reached)| {
            let Some(a) = ws.actors.get(id).filter(|_| !ws.dead_actor_ids.contains(id)) else { return false };
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) { *tp_max = tp_max.max(tp); }
            if a.get_metric("external_pressure") <= *goal { *reached = true; }
            if t < *deadline { return true; }
            r.declines.push((*reached, if *below { 0 } else if *tp_max >= *tp_new + *drop / 2.0 { 1 } else { 2 }));
            false
        });
        let mut ids: Vec<&String> = ws.actors.keys().filter(|id| !ws.dead_actor_ids.contains(*id)).collect();
        ids.sort();
        for id in ids {
            let a = &ws.actors[id];
            let l = a.get_metric("legitimacy");
            let ep = a.get_metric("external_pressure");
            let tp = engine13::engine::pressure_threat(ws, id);
            r.living += 1;
            if ep >= 99.0 && tp.is_some_and(|x| x < 90.0) { r.ceiling_no_threat += 1; }
            let crisis = a.actor_tags.values().any(|tag| tag.metrics_modifier.iter().any(|(mm, v)| mm.as_str() == "legitimacy" && *v < 0));
            if !crisis {
                let e = r.calm.entry(id.clone()).or_default();
                e.0 += 1;
                if l <= 1.0 { e.1 += 1; }
                if l >= 99.0 { e.2 += 1; }
            }
            r.legit.entry(id.clone()).or_default().push(l);
            r.eo.entry(id.clone()).or_default().push(a.get_metric("economic_output"));
            r.coh.entry(id.clone()).or_default().push(a.get_metric("cohesion"));
            if let Some(tp) = tp {
                r.corr[0] += 1.0; r.corr[1] += ep; r.corr[2] += tp; r.corr[3] += ep * ep; r.corr[4] += tp * tp; r.corr[5] += ep * tp;
                if let (Some(p), Some(e0)) = (prev_tp.get(id), prev_ep.get(id)) {
                    let drop = p - tp;
                    if drop >= 20.0 { open.push((id.clone(), t + 10, e0 - drop / 2.0, drop, e0 - drop / 2.0 < tp, tp, f64::MIN, false)); }
                }
                prev_tp.insert(id.clone(), tp);
            }
            prev_ep.insert(id.clone(), ep);
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
    }
    census::set_combat_quality(true);
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    r.conquered_by = ws.conquered_by.clone();
    r.outcomes = ws.milestone_events_fired.iter().filter(|mm| mm.starts_with("outcome_")).cloned().collect();
    r.stab = ws.milestone_events_fired.iter().any(|mm| mm == "milan_regency_stabilizes");
    r.deep = ws.milestone_events_fired.iter().any(|mm| mm == "milan_regency_crisis_deepens");
    r
}

/// Ц4 pooled: (with quality: counted, won by higher quality, Σ p, Σ p(1 − p)), (without: counted, won)
type C4Pool = ((u64, u64, f64, f64), (u64, u64));

/// Ц4 over battles: (counted, won by higher quality, Σ p, Σ p(1 − p))
fn c4(battles: &[census::Battle]) -> (u64, u64, f64, f64) {
    let mut acc = (0, 0, 0.0, 0.0);
    for b in battles {
        if b.army_defender <= 0.0 { continue; }
        let ratio = b.army_attacker / b.army_defender;
        if !(0.8..=1.25).contains(&ratio) || (b.quality_attacker - b.quality_defender).abs() < 10.0 { continue; }
        let hq_att = b.quality_attacker > b.quality_defender;
        let (s_a, s_d) = (b.army_attacker * b.quality_attacker / 100.0, b.army_defender * b.quality_defender / 100.0);
        let p_att = if s_a + s_d > 0.0 { s_a / (s_a + s_d) } else { 0.5 };
        let p = if hq_att { p_att } else { 1.0 - p_att };
        acc.0 += 1; acc.1 += (hq_att == b.attacker_won) as u64; acc.2 += p; acc.3 += p * (1.0 - p);
    }
    acc
}

/// The world-level measures of the stop rule: [Ц1 actors, Ц1 tiers, Ц1 spread, Ц5 actors, Ц5 spread,
/// Ц6 ceiling, Ц6 corr, Ц8 extremes, Ц8 spread]
fn world_measures(sc: &str, runs: &[Run]) -> ([bool; 9], String) {
    let frac = |v: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * v.iter().filter(|y| pr(**y)).count() as f64 / v.len().max(1) as f64;
    let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut legit: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut coh: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut calm: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    let (mut lv, mut cnt) = (0, 0);
    let mut c = [0.0; 6];
    for rr in runs {
        for (k, v) in &rr.eo { eo.entry(k.clone()).or_default().extend(v); }
        for (k, v) in &rr.legit { legit.entry(k.clone()).or_default().extend(v); }
        for (k, v) in &rr.coh { coh.entry(k.clone()).or_default().extend(v); }
        for (k, v) in &rr.calm { let e = calm.entry(k.clone()).or_default(); e.0 += v.0; e.1 += v.1; e.2 += v.2; }
        lv += rr.living; cnt += rr.ceiling_no_threat;
        for (a, b) in c.iter_mut().zip(&rr.corr) { *a += b; }
    }
    let spread = |m: &BTreeMap<String, Vec<f64>>| { let v: Vec<f64> = m.values().map(|x| pct(x, 0.5)).collect(); v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min) };
    let c1n = eo.values().filter(|x| frac(x, &|y| y >= 99.0) < 20.0 && frac(x, &|y| y <= 1.0) < 20.0).count();
    let tm: Vec<f64> = tiers(sc).iter().map(|t| { let x: Vec<f64> = t.iter().filter_map(|a| eo.get(*a)).flatten().copied().collect(); pct(&x, 0.5) }).collect();
    let c5n = calm.values().filter(|v| v.0 > 0 && share(v.1, v.0) < 20.0 && share(v.2, v.0) < 20.0).count();
    let ncalm = calm.values().filter(|v| v.0 > 0).count();
    let c8n = coh.values().filter(|x| frac(x, &|y| y <= 1.0) < 20.0 && frac(x, &|y| y >= 99.0) < 20.0).count();
    let (n, sx, sy, sxx, syy, sxy) = (c[0], c[1], c[2], c[3], c[4], c[5]);
    let corr = (n * sxy - sx * sy) / ((n * sxx - sx * sx).sqrt() * (n * syy - sy * sy).sqrt());
    let ok = [
        100 * c1n >= 80 * eo.len(), tm.windows(2).all(|w| w[0] > w[1]), spread(&eo) >= 20.0,
        100 * c5n >= 80 * ncalm, spread(&legit) >= 15.0,
        share(cnt, lv) < 30.0, corr >= 0.7,
        100 * c8n >= 80 * coh.len(), spread(&coh) >= 15.0,
    ];
    let detail = format!("Ц1 {c1n}/{} spread {:.0}; Ц5 {c5n}/{ncalm} spread {:.0}; Ц6 {:.1} % corr {corr:.2}; Ц8 {c8n}/{} spread {:.0}",
        eo.len(), spread(&eo), spread(&legit), share(cnt, lv), coh.len(), spread(&coh));
    (ok, detail)
}

/// Mean paired difference (b − a) and its standard error.
fn paired(a: &[f64], b: &[f64]) -> (f64, f64) {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| y - x).collect();
    let n = d.len() as f64;
    let mean = d.iter().sum::<f64>() / n;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    (mean, sd / n.sqrt())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let first: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_battles();
    census::enable_writes();
    if args.get(4).map(String::as_str) == Some("c4") { c4_split(first, seeds, ticks); return; }
    println!("# Ц9 with Ц7 — seeds {first}–{}, {ticks} ticks per world\n", first + seeds - 1);
    const MEASURES: [&str; 9] = ["Ц1 actors", "Ц1 tiers", "Ц1 spread", "Ц5 actors", "Ц5 spread", "Ц6 ceiling", "Ц6 corr", "Ц8 extremes", "Ц8 spread"];
    let mut c7_rows = Vec::new();
    let mut c7_ok: BTreeMap<&str, Vec<bool>> = BTreeMap::new();
    let mut pair_rows = Vec::new();
    let mut pairs_ok = true;
    let mut stop_rows = Vec::new();
    let mut stops: Vec<String> = Vec::new();
    let mut c4p: BTreeMap<&str, C4Pool> = BTreeMap::new();
    let mut decline: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    let mut path_rows = Vec::new();
    let mut info_rows = Vec::new();
    for sc in SCENARIOS {
        for world in worlds(sc) {
            let all: Vec<Vec<Run>> = (0..3).map(|m| (first..first + seeds).map(|s| run(sc, world, m, true, s, ticks)).collect()).collect();
            // ---- Ц4 condition 2 needs strength without quality: base and Ц9
            for m in [0, 2] {
                let nq: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, m, false, s, ticks)).collect();
                let e = c4p.entry(MODELS[m]).or_default();
                for rr in &all[m] { let x = c4(&rr.battles); e.0 .0 += x.0; e.0 .1 += x.1; e.0 .2 += x.2; e.0 .3 += x.3; }
                for rr in &nq { let x = c4(&rr.battles); e.1 .0 += x.0; e.1 .1 += x.1; }
                let d = decline.entry(MODELS[m]).or_default();
                for rr in &all[m] { for x in rr.declines.iter().filter(|x| x.1 == 2) { d.0 += 1; d.1 += x.0 as u64; } }
            }
            // ---- stop rule, world-level
            let (ok_b, det_b) = world_measures(sc, &all[0]);
            let (ok_9, det_9) = world_measures(sc, &all[2]);
            for (i, name) in MEASURES.iter().enumerate() {
                if ok_b[i] && !ok_9[i] { stops.push(format!("{sc} {world}: {name}")); }
            }
            stop_rows.push(format!("| {sc} | {world} | {det_b} | {det_9} |"));
            // ---- Ц7 measure, items 1–3, for Ц7 and Ц9
            for m in [1, 2] {
                let runs = &all[m];
                let vassal_of = |rr: &Run, v: &str, lord: Option<&str>| rr.vassal.keys().any(|(a, b)| a == v && lord.is_none_or(|l| l == b));
                let (cell, ok) = match sc {
                    "rome_375" => {
                        let per_game = runs.iter().map(|rr| PEOPLES.iter().filter(|p| vassal_of(rr, p, Some("rome"))).count()).sum::<usize>() as f64 / seeds as f64;
                        (format!("five peoples Rome's vassals {per_game:.1} a game"), per_game >= 3.0)
                    }
                    "constantinople_1430" => {
                        let ft: Vec<f64> = runs.iter().filter_map(|rr| rr.dead.get("byzantium").map(|t| *t as f64)).collect();
                        let ok = *world != "none" || (ft.len() >= 20 && (40.0..=59.0).contains(&pct(&ft, 0.5)));
                        (format!("Byzantium falls in {} @ {}", ft.len(), q(&ft)), ok)
                    }
                    _ => {
                        let hit = |id: &str| runs.iter().filter(|rr| rr.dead.contains_key(id) || vassal_of(rr, id, None)).count();
                        let (mi, pa, ve) = (hit("milan"), hit("papacy"), hit("venice"));
                        (format!("dies or submits: Milan {mi}, papacy {pa}, Venice {ve}"), mi <= 1 && pa <= 3 && ve <= 3)
                    }
                };
                c7_rows.push(format!("| {sc} | {world} | {} | {cell} | {} |", MODELS[m], if ok { "yes" } else { "**no**" }));
                c7_ok.entry(MODELS[m]).or_default().push(ok);
                // (4): every war death recorded with its conqueror
                let war: usize = runs.iter().map(|rr| rr.dead.keys().filter(|d| rr.conquered_by.contains_key(*d)).count()).sum();
                let unrec: usize = runs.iter().map(|rr| rr.conquered_by.keys().filter(|d| !rr.dead.contains_key(*d)).count()).sum();
                let deaths: u32 = runs.iter().map(|rr| rr.deaths).sum();
                path_rows.push(format!("| {sc} | {world} | {} | {deaths} | {war} | {} | {unrec} |", MODELS[m], deaths as usize - war));
            }
            // ---- historical pairs, Ц9 against Ц7
            let pairs: Vec<(&str, &str, bool)> = match sc {
                "rome_375" => vec![("sassanids", "armenia", true), ("huns", "ostrogoths", true), ("rome", "*peoples", true)],
                "constantinople_1430" => vec![("ottomans", "serbia", true), ("ottomans", "byzantium", true)],
                _ => vec![("florence", "siena", true), ("naples", "sicily", false), ("naples", "papacy", false)],
            };
            for (lord, v, gated) in pairs {
                let count = |rr: &Run| if v == "*peoples" { PEOPLES.iter().filter(|p| rr.vassal.contains_key(&(p.to_string(), lord.to_string()))).count() as f64 } else { rr.vassal.contains_key(&(v.to_string(), lord.to_string())) as u8 as f64 };
                let a: Vec<f64> = all[1].iter().map(count).collect();
                let b: Vec<f64> = all[2].iter().map(count).collect();
                let (mean, se) = paired(&a, &b);
                let holds = if se == 0.0 { mean == 0.0 } else { mean.abs() <= 2.0 * se };
                if gated && !holds { pairs_ok = false; }
                let ticks_of = |runs: &[Run]| -> Vec<f64> { if v == "*peoples" { vec![] } else { runs.iter().filter_map(|rr| rr.vassal.get(&(v.to_string(), lord.to_string())).map(|t| *t as f64)).collect() } };
                let (ta, tb) = (ticks_of(&all[1]), ticks_of(&all[2]));
                pair_rows.push(format!("| {lord} → {} | {sc} {world} | {:.1} (@{}) | {:.1} (@{}) | {mean:+.2} ± 2×{se:.2} | {} |",
                    if v == "*peoples" { "five peoples (a game)" } else { v }, a.iter().sum::<f64>() / if v == "*peoples" { seeds as f64 } else { 1.0 }, q(&ta), b.iter().sum::<f64>() / if v == "*peoples" { seeds as f64 } else { 1.0 }, q(&tb),
                    if !gated { "not gated".to_string() } else if holds { "holds".into() } else { "**no**".into() }));
            }
            // ---- for information
            if sc == "milan_1477" {
                for m in [1, 2] {
                    let runs = &all[m];
                    let lt: Vec<f64> = runs.iter().filter_map(|rr| rr.league_turns.map(|t| t as f64)).collect();
                    let sv: Vec<f64> = runs.iter().filter_map(|rr| rr.savoy_alliance.map(|t| t as f64)).collect();
                    let md = runs.iter().filter(|rr| rr.dead.contains_key("milan")).count();
                    let wins = runs.iter().filter(|rr| rr.win.is_some()).count();
                    let mut oc: BTreeMap<String, u32> = BTreeMap::new();
                    for rr in runs { for o in &rr.outcomes { *oc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                    info_rows.push(format!("| {world} | {} | {} @ {} | {} @ {} | {md} | {wins} | {} | {} / {} |", MODELS[m], lt.len(), q(&lt), sv.len(), q(&sv),
                        if oc.is_empty() { "—".into() } else { oc.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ") },
                        runs.iter().filter(|rr| rr.stab).count(), runs.iter().filter(|rr| rr.deep).count()));
                }
            }
        }
    }
    println!("## 1. Ц7's measure, items 1–3\n");
    println!("| scenario | world | model | | passes |");
    println!("|---|---|---|---|---|");
    for r in &c7_rows { println!("{r}"); }
    for m in ["Ц7", "Ц9"] { let v = &c7_ok[m]; println!("\n{m}: {} of {} cells pass", v.iter().filter(|x| **x).count(), v.len()); }
    println!("\n### (4) Deaths by path\n");
    println!("| scenario | world | model | deaths | by conquest (conqueror recorded) | by paths 1–2 | conquered but not dead |");
    println!("|---|---|---|---|---|---|---|");
    for r in &path_rows { println!("{r}"); }
    println!("\n## 2. Historical pairs: Ц9 against Ц7 (games with the submission, median tick; paired difference ± 2 SE)\n");
    println!("| pair | world | Ц7 | Ц9 | Ц9 − Ц7 | |");
    println!("|---|---|---|---|---|---|");
    for r in &pair_rows { println!("{r}"); }
    println!("\nAll gated pairs hold: {}", if pairs_ok { "**yes**" } else { "**no**" });
    println!("\n## 3. Stop rule: base against Ц9 on the same seeds\n");
    println!("| scenario | world | base | Ц9 |");
    println!("|---|---|---|---|");
    for r in &stop_rows { println!("{r}"); }
    println!();
    for m in ["base", "Ц9"] {
        let ((n, w, sp, spq), (nb, wb)) = c4p[m];
        let pa = w as f64 / n.max(1) as f64;
        let promise = sp / n.max(1) as f64;
        let se = spq.sqrt() / n.max(1) as f64;
        let pb = wb as f64 / nb.max(1) as f64;
        let se_d = (pa * (1.0 - pa) / n.max(1) as f64 + pb * (1.0 - pb) / nb.max(1) as f64).sqrt();
        let (ok1, ok2) = (pa >= promise - 2.0 * se, pa - pb >= 2.0 * se_d);
        if m == "Ц9" {
            let ((bn, bw, bsp, bspq), (bnb, bwb)) = c4p["base"];
            let bpa = bw as f64 / bn.max(1) as f64;
            let b1 = bpa >= bsp / bn.max(1) as f64 - 2.0 * bspq.sqrt() / bn.max(1) as f64;
            let bpb = bwb as f64 / bnb.max(1) as f64;
            let b2 = bpa - bpb >= 2.0 * (bpa * (1.0 - bpa) / bn.max(1) as f64 + bpb * (1.0 - bpb) / bnb.max(1) as f64).sqrt();
            if b1 && !ok1 { stops.push("Ц4 (1)".into()); }
            if b2 && !ok2 { stops.push("Ц4 (2)".into()); }
        }
        let d = decline[m];
        println!("- {m}: Ц4 (1) {:.1} % vs promise − 2 SE {:.1} % — {}; Ц4 (2) {:+.1} vs 2 SE {:.1} — {} ({n} battles); Ц6 decline {:.1} % of {}", 100.0 * pa, 100.0 * (promise - 2.0 * se), if ok1 { "yes" } else { "no" },
            100.0 * (pa - pb), 200.0 * se_d, if ok2 { "yes" } else { "no" }, share(d.1, d.0), d.0);
    }
    println!("\nStops (base passes, Ц9 does not): {}", if stops.is_empty() { "none".into() } else { stops.join("; ") });
    println!("\n## 4. For information: milan\n");
    println!("| world | model | league turns on Milan: games @ tick | Savoy alliance: games @ tick | Milan dies | wins | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in &info_rows { println!("{r}"); }
}

/// Ц4 per model (base, Ц7, Ц9) and per scenario — where the stop on Ц4 (2) comes from. Only milan
/// has alliances, so in rome and constantinople Ц9 is Ц7.
fn c4_split(first: u64, seeds: u64, ticks: u32) {
    println!("# Ц4 by model and scenario, seeds {first}–{}\n", first + seeds - 1);
    println!("| model | scope | battles | (1) won by higher quality vs promise − 2 SE | (2) with − without quality vs 2 SE |");
    println!("|---|---|---|---|---|");
    for (m, name) in MODELS.iter().enumerate() {
        let mut all: C4Pool = Default::default();
        for sc in SCENARIOS {
            let mut p: C4Pool = Default::default();
            for world in worlds(sc) {
                for s in first..first + seeds {
                    let x = c4(&run(sc, world, m, true, s, ticks).battles); p.0 .0 += x.0; p.0 .1 += x.1; p.0 .2 += x.2; p.0 .3 += x.3;
                    let y = c4(&run(sc, world, m, false, s, ticks).battles); p.1 .0 += y.0; p.1 .1 += y.1;
                }
            }
            println!("{}", c4_row(name, sc, p));
            all.0 .0 += p.0 .0; all.0 .1 += p.0 .1; all.0 .2 += p.0 .2; all.0 .3 += p.0 .3; all.1 .0 += p.1 .0; all.1 .1 += p.1 .1;
        }
        println!("{}", c4_row(name, "**all**", all));
    }
}

fn c4_row(name: &str, scope: &str, ((n, w, sp, spq), (nb, wb)): C4Pool) -> String {
    let pa = w as f64 / n.max(1) as f64;
    let promise = sp / n.max(1) as f64;
    let se = spq.sqrt() / n.max(1) as f64;
    let pb = wb as f64 / nb.max(1) as f64;
    let se_d = (pa * (1.0 - pa) / n.max(1) as f64 + pb * (1.0 - pb) / nb.max(1) as f64).sqrt();
    format!("| {name} | {scope} | {n} / {nb} | {:.1} % vs {:.1} % | {:+.1} vs {:.1} {} |", 100.0 * pa, 100.0 * (promise - 2.0 * se), 100.0 * (pa - pb), 200.0 * se_d, if pa - pb >= 2.0 * se_d { "yes" } else { "**no**" })
}
