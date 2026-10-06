//! Economy project, Ц6 closed: the threat model as written into v2 (r = 0.10, pressure tags as a
//! level, stage 2 items 1–3; the authored pressure auto-deltas kept) against v2 without it
//! (docs/economy_project_brief.md §9). Built with `--features census`. Every world of the three
//! scenarios, 30 seeds × 300 ticks.
//!
//! The decline measure is the corrected one (§9.7): a tracking measure counts only lasting and
//! reachable changes of its target. A case opens when T_p falls by ≥ 20 in one tick; it counts if
//! the goal (pressure before the fall minus half the fall) is not below the new T_p and T_p does not
//! come back by half the fall or more within the 10-tick window; it succeeds if pressure reaches the
//! goal within the window. The old (uncorrected) share is printed beside it.
//!
//! Also Ц1's measure with Ц6 on (each actor under 20 % of living ticks at the ceiling and at the
//! floor of `economic_output`, the spread of actor medians, the tiers of §9.5), the stage 2 measures
//! and the §9.2 rows for information.
//!
//! Usage: cargo run --release --features census --bin c6s3_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];

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

/// None = v2 without the threat model; Some(r) = v2 as written in the content (r read from it)
type Model = Option<f64>;

fn label(m: Model) -> String {
    match m {
        None => "v2 without the threat model".into(),
        Some(r) => format!("v2 with Ц6 (content, r = {r:.2})"),
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

#[derive(Default)]
struct Run {
    acc: BTreeMap<String, [u64; 5]>,
    corr: [f64; 6],
    /// decline cases: (success, class: 0 the goal lies below the new T_p, 1 T_p came back by half
    /// the fall or more inside the window, 2 lasting and reachable — the only class counted)
    declines: Vec<(bool, u8)>,
    /// economic_output per living tick, per actor (Ц1's measure)
    eo: BTreeMap<String, Vec<f64>>,
    sources: BTreeMap<(String, String), f64>,
    dead: BTreeMap<String, (u32, f64)>,
    deaths: u32,
    vassal_excl: u64,
    rome_zombie: bool,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

fn run(sc: &str, world: &str, m: Model, seed: u64, ticks: u32, with_sources: bool) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        if m.is_none() {
            s.economy_v2_pressure_tags_as_level = false;
            s.economy_v2_pressure_pull = None;
        }
        assert_eq!(s.economy_v2_pressure_pull, m, "the content carries the threat model");
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    // open declines: (actor, deadline tick, goal, drop, goal < new T_p, new T_p, highest T_p since, reached)
    type Open = (String, u32, f64, f64, bool, f64, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    let _ = census::take_writes();
    for _ in 0..ticks {
        let before: std::collections::BTreeSet<String> = st.world_state.as_ref().unwrap().dead_actor_ids.iter().cloned().collect();
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
                if w.metric == "external_pressure" && ws.actors.contains_key(&w.actor) {
                    let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                    *r.sources.entry((w.actor.clone(), src)).or_default() += w.requested;
                }
            }
        }
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) {
            r.dead.insert(d.clone(), (t, prev_tp.get(d).copied().unwrap_or(f64::NAN)));
        }
        r.vassal_excl += ws.vassalages.len() as u64;
        // resolve open declines
        // a case stays open for its whole window, so a rebound after success is seen too
        open.retain_mut(|(id, deadline, goal, drop, below, tp_new, tp_max, reached)| {
            let Some(a) = ws.actors.get(id).filter(|_| !ws.dead_actor_ids.contains(id)) else { return false };
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) { *tp_max = tp_max.max(tp); }
            if a.get_metric("external_pressure") <= *goal { *reached = true; }
            if t < *deadline { return true; }
            let class = if *below { 0 } else if *tp_max >= *tp_new + *drop / 2.0 { 1 } else { 2 };
            r.declines.push((*reached, class));
            false
        });
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let ep = a.get_metric("external_pressure");
            r.eo.entry(id.clone()).or_default().push(a.get_metric("economic_output"));
            let e = r.acc.entry(id.clone()).or_default();
            e[0] += 1;
            if ep >= 99.0 { e[1] += 1; }
            if a.get_metric("legitimacy") <= 1.0 { e[2] += 1; }
            if a.get_metric("cohesion") < 15.0 { e[3] += 1; }
            if engine13::engine::eo_target(ws, scn, id).is_some_and(|tg| a.get_metric("economic_output") < tg / 2.0) { e[4] += 1; }
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) {
                r.corr[0] += 1.0; r.corr[1] += ep; r.corr[2] += tp; r.corr[3] += ep * ep; r.corr[4] += tp * tp; r.corr[5] += ep * tp;
                if let (Some(p), Some(e0)) = (prev_tp.get(id), prev_ep.get(id)) {
                    let drop = p - tp;
                    if drop >= 20.0 { open.push((id.clone(), t + 10, e0 - drop / 2.0, drop, e0 - drop / 2.0 < tp, tp, f64::MIN, false)); }
                }
                prev_tp.insert(id.clone(), tp);
            }
            prev_ep.insert(id.clone(), ep);
            if id == "rome" && a.get_metric("population") <= 1.0 { r.rome_zombie = true; }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    // windows cut by the end of the game are not counted
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    r.outcomes = ws.milestone_events_fired.iter().filter(|m| m.starts_with("outcome_")).cloned().collect();
    r.stab = ws.milestone_events_fired.iter().any(|m| m == "milan_regency_stabilizes");
    r.deep = ws.milestone_events_fired.iter().any(|m| m == "milan_regency_crisis_deepens");
    r
}

fn paired(a: &[Run], b: &[Run], f: impl Fn(&Run) -> f64) -> (f64, f64) {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| f(y) - f(x)).collect();
    let n = d.len() as f64;
    let mean = d.iter().sum::<f64>() / n;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
    (mean, t)
}

/// (Byzantium without a player: falls, median tick), Rome deaths per rome world, Milan deaths (aggressive)
type Hist = (Option<(usize, f64)>, Vec<usize>, Option<usize>);

#[derive(Default)]
struct Verdict {
    ceiling_ok: bool,
    corr_ok: bool,
    /// per class (see `Run::declines`): (cases, successes)
    classes: [(u64, u64); 3],
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    println!("# Ц6 closed — the threat model as written into v2, {seeds} seeds × {ticks} ticks per world\n");
    let content_r = {
        let db = engine13::db::Db::open_in_memory().unwrap();
        let rs: Vec<Option<f64>> = ["rome_375", "constantinople_1430", "milan_1477"].iter().map(|sc| {
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
            st.current_scenario.as_ref().unwrap().economy_v2_pressure_pull
        }).collect();
        assert!(rs.windows(2).all(|w| w[0] == w[1]) && rs[0].is_some(), "one r in all three scenarios: {rs:?}");
        rs[0]
    };
    println!("## 0. Neighbours at distance 1 at tick 0 (the set N is built from; Sea weighs 0.5)\n");
    println!("No alliance is ever formed by the engine (`world.alliances` has no writer); vassalage is the only explicit relation. Which of these neighbours are friendly by sense is for the owner — the list is printed, nothing is invented.\n");
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let db = engine13::db::Db::open_in_memory().unwrap();
        let mut st = engine13::AppState::default();
        engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
        let ws = st.world_state.as_ref().unwrap();
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        let cells: Vec<String> = ids.iter().map(|id| {
            let n: Vec<String> = ws.actors[*id].neighbors.iter().filter(|n| n.distance == 1)
                .map(|n| if matches!(n.border_type, engine13::core::BorderType::Sea) { format!("{} (sea)", n.id) } else { n.id.clone() }).collect();
            format!("{id}: {}", n.join(", "))
        }).collect();
        println!("- {sc} — {}", cells.join("; "));
    }
    println!();
    let main_models: Vec<Model> = vec![None, content_r];
    let mut c1_rows = Vec::new();
    let mut measure_rows = Vec::new();
    let mut cascade_rows = Vec::new();
    let mut death_rows = Vec::new();
    let mut dying_rows = Vec::new();
    let mut hist_rows = Vec::new();
    let mut info_rows = Vec::new();
    let mut br_rows = Vec::new();
    let mut verdict: BTreeMap<String, Vec<Verdict>> = BTreeMap::new();
    // historical checks per r: (byzantium none falls, median tick), rome min deaths, milan aggressive deaths
    let mut hist: BTreeMap<String, Hist> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let mut src_pool: BTreeMap<(String, String), f64> = BTreeMap::new();
        let mut living_pool: BTreeMap<String, u64> = BTreeMap::new();
        for world in worlds(sc) {
            let base: Vec<Run> = (0..seeds).map(|s| run(sc, world, None, s, ticks, false)).collect();
            for m in main_models.clone() {
                let with_sources = m.is_some();
                let fresh: Vec<Run>;
                let runs: &[Run] = if m.is_none() { &base } else { fresh = (0..seeds).map(|s| run(sc, world, m, s, ticks, with_sources)).collect(); &fresh };
                if with_sources {
                    for rr in runs {
                        for (k, x) in &rr.sources { *src_pool.entry(k.clone()).or_default() += x; }
                        for (k, a) in &rr.acc { *living_pool.entry(k.clone()).or_default() += a[0]; }
                    }
                }
                let mut pool: BTreeMap<String, [u64; 5]> = BTreeMap::new();
                for rr in runs { for (k, a) in &rr.acc { let e = pool.entry(k.clone()).or_default(); for (x, y) in e.iter_mut().zip(a) { *x += y; } } }
                let tot: [u64; 5] = pool.values().fold([0; 5], |mut s, a| { for (x, y) in s.iter_mut().zip(a) { *x += y; } s });
                let p = |x: u64, n: u64| 100.0 * x as f64 / n.max(1) as f64;
                let ceiling = p(tot[1], tot[0]);
                let c = runs.iter().fold([0.0; 6], |mut s, rr| { for (x, y) in s.iter_mut().zip(&rr.corr) { *x += y; } s });
                let (n, sx, sy, sxx, syy, sxy) = (c[0], c[1], c[2], c[3], c[4], c[5]);
                let corr = (n * sxy - sx * sy) / ((n * sxx - sx * sx).sqrt() * (n * syy - sy * sy).sqrt());
                let decl: Vec<&(bool, u8)> = runs.iter().flat_map(|rr| rr.declines.iter()).collect();
                let classes = [0u8, 1, 2].map(|k| (decl.iter().filter(|d| d.1 == k).count() as u64, decl.iter().filter(|d| d.1 == k && d.0).count() as u64));
                let ok_all = decl.iter().filter(|d| d.0).count();
                measure_rows.push(format!("| {sc} | {world} | {} | {ceiling:.0} % | {} / {} | {corr:.2} | {} cases, {:.0} % | {} unreachable, {} rebounded | {} cases, {:.0} % |", label(m),
                    pool.values().filter(|a| p(a[1], a[0]) < 30.0).count(), pool.len(), decl.len(), 100.0 * ok_all as f64 / decl.len().max(1) as f64,
                    classes[0].0, classes[1].0, classes[2].0, 100.0 * classes[2].1 as f64 / classes[2].0.max(1) as f64));
                if m.is_some() {
                    verdict.entry(label(m)).or_default().push(Verdict { ceiling_ok: ceiling < 30.0, corr_ok: corr >= 0.7, classes });
                }
                // Ц1's measure
                let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for rr in runs { for (k, x) in &rr.eo { eo.entry(k.clone()).or_default().extend(x); } }
                let share = |x: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * x.iter().filter(|y| pr(**y)).count() as f64 / x.len().max(1) as f64;
                let failing: Vec<String> = eo.iter().filter(|(_, x)| share(x, &|y| y >= 99.0) >= 20.0 || share(x, &|y| y <= 1.0) >= 20.0)
                    .map(|(k, x)| format!("{k} ({:.0} / {:.0} %)", share(x, &|y| y >= 99.0), share(x, &|y| y <= 1.0))).collect();
                let medians: Vec<f64> = eo.values().map(|x| pct(x, 0.5)).collect();
                let spread = medians.iter().cloned().fold(f64::MIN, f64::max) - medians.iter().cloned().fold(f64::MAX, f64::min);
                let tm: Vec<f64> = tiers(sc).iter().map(|t| { let x: Vec<f64> = t.iter().filter_map(|a| eo.get(*a)).flatten().copied().collect(); pct(&x, 0.5) }).collect();
                c1_rows.push(format!("| {sc} | {world} | {} | {} / {} | {} | {spread:.0} | {} | {} |", label(m), eo.len() - failing.len(), eo.len(),
                    if failing.is_empty() { "—".into() } else { failing.join(", ") },
                    tm.iter().map(|x| format!("{x:.0}")).collect::<Vec<_>>().join(" › "), if tm.windows(2).all(|w| w[0] > w[1]) { "yes" } else { "no" }));
                let rome = if sc == "rome_375" { format!("{} / {seeds}", runs.iter().filter(|rr| rr.rome_zombie).count()) } else { "—".into() };
                cascade_rows.push(format!("| {sc} | {world} | {} | {:.0} % | {:.0} % | {:.0} % | {rome} |", label(m), p(tot[2], tot[0]), p(tot[3], tot[0]), p(tot[4], tot[0])));
                let (dm, dt) = paired(&base, runs, |rr| rr.deaths as f64);
                let keys: Vec<String> = KEY.iter().filter(|k| base[0].acc.contains_key(**k)).map(|k| {
                    let (km, kt) = paired(&base, runs, |rr| if rr.dead.contains_key(*k) { 1.0 } else { 0.0 });
                    format!("{k} {}→{} ({km:+.2}, t {kt:+.1})", base.iter().filter(|rr| rr.dead.contains_key(*k)).count(), runs.iter().filter(|rr| rr.dead.contains_key(*k)).count())
                }).collect();
                death_rows.push(format!("| {sc} | {world} | {} | {} | {dm:+.2} (t {dt:+.1}) | {} |", label(m), runs.iter().map(|rr| rr.deaths).sum::<u32>(), keys.join("; ")));
                if runs.iter().map(|rr| rr.deaths as f64).sum::<f64>() / runs.len() as f64 > 5.0 && m.is_some() {
                    let mut per: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                    for rr in runs { for (k, (_, tp)) in &rr.dead { per.entry(k.clone()).or_default().push(*tp); } }
                    let mut v: Vec<(String, Vec<f64>)> = per.into_iter().collect();
                    v.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
                    let cells: Vec<String> = v.iter().take(10).map(|(k, tps)| {
                        let finite: Vec<f64> = tps.iter().copied().filter(|x| x.is_finite()).collect();
                        format!("{k} {} (T_p {:.0})", tps.len(), pct(&finite, 0.5))
                    }).collect();
                    dying_rows.push(format!("| {sc} | {world} | {} | {} |", label(m), cells.join(", ")));
                }
                let key = label(m);
                let h = hist.entry(key).or_insert((None, Vec::new(), None));
                match (sc, *world) {
                    ("rome_375", _) => {
                        let n = runs.iter().filter(|rr| rr.dead.contains_key("rome")).count();
                        h.1.push(n);
                        hist_rows.push(format!("| {sc} | {world} | {} | Rome dies in {n} / {seeds} |", label(m)));
                    }
                    ("constantinople_1430", "none") => {
                        let ft: Vec<f64> = runs.iter().filter_map(|rr| rr.dead.get("byzantium").map(|x| x.0 as f64)).collect();
                        h.0 = Some((ft.len(), pct(&ft, 0.5)));
                        let fifties = ft.iter().filter(|t| (40.0..60.0).contains(*t)).count();
                        hist_rows.push(format!("| {sc} | {world} | {} | Byzantium falls in {} / {seeds}, tick p10/50/90 {}; in the 1450s (ticks 40–59): {fifties} |", label(m), ft.len(), q(&ft)));
                    }
                    ("milan_1477", "aggressive") => {
                        let n = runs.iter().filter(|rr| rr.dead.contains_key("milan")).count();
                        h.2 = Some(n);
                        hist_rows.push(format!("| {sc} | {world} | {} | Milan dies in {n} / {seeds} |", label(m)));
                    }
                    _ => {}
                }
                let wins: Vec<f64> = runs.iter().filter_map(|rr| rr.win.map(|t| t as f64)).collect();
                let mut perw: BTreeMap<u32, u32> = BTreeMap::new();
                for t in &wins { *perw.entry(*t as u32).or_default() += 1; }
                let top = perw.values().max().copied().unwrap_or(0);
                let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let wins_s = if wins.is_empty() { "0".into() } else { format!("{} ({}; 40–43: {on}; top {:.0} %)", wins.len(), q(&wins), 100.0 * top as f64 / wins.len() as f64) };
                let mut oc: BTreeMap<String, u32> = BTreeMap::new();
                for rr in runs { for o in &rr.outcomes { *oc.entry(o.trim_start_matches("outcome_").to_string()).or_default() += 1; } }
                let multi = runs.iter().filter(|rr| rr.outcomes.len() > 1).count();
                let outc = if oc.is_empty() { "—".into() } else { format!("{} (>1: {multi})", oc.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")) };
                let fork = if sc == "milan_1477" { format!("{} / {}", runs.iter().filter(|rr| rr.stab).count(), runs.iter().filter(|rr| rr.deep).count()) } else { "—".into() };
                let split = if sc == "rome_375" { runs.iter().filter(|rr| rr.split40).count().to_string() } else { "—".into() };
                let vass = runs.iter().map(|rr| rr.vassal_excl).sum::<u64>();
                info_rows.push(format!("| {sc} | {world} | {} | {wins_s} | {split} | {outc} | {fork} | {vass} |", label(m)));
            }
        }
        let total_living: u64 = living_pool.values().sum();
        let mut by_src: BTreeMap<String, (f64, f64, String)> = BTreeMap::new();
        for ((actor, src), x) in &src_pool {
            let e = by_src.entry(src.clone()).or_insert((0.0, 0.0, String::new()));
            e.0 += x;
            let per_actor = x / living_pool.get(actor).copied().unwrap_or(1).max(1) as f64;
            if per_actor.abs() > e.1.abs() { e.1 = per_actor; e.2 = actor.clone(); }
        }
        let mut v: Vec<(String, (f64, f64, String))> = by_src.into_iter().filter(|(k, _)| k != "pressure pull").collect();
        v.sort_by(|a, b| b.1 .1.abs().partial_cmp(&a.1 .1.abs()).unwrap());
        for (src, (sum, max, who)) in v.iter().filter(|x| x.1 .1.abs() >= 0.02).take(12) {
            br_rows.push(format!("| {sc} | {src} | {:+.3} | {max:+.3} ({who}) | {:+.1} / {:+.1} / {:+.1} |", sum / total_living.max(1) as f64, max / 0.03, max / 0.05, max / 0.10));
        }
    }
    println!("## 1. Pressure writers that remain (v2 with Ц6; the pull itself excluded)\n");
    println!("| scenario | writer | mean over all actor-ticks | largest per-actor mean (actor) | b / r at r = 0.03 / 0.05 / 0.10 |");
    println!("|---|---|---|---|---|");
    for r in br_rows { println!("{r}"); }
    println!("\n## 2. The Ц6 measure\n");
    println!("Decline: a case opens when T_p falls by ≥ 20 in one tick; success = pressure reaches its value before the fall minus half the fall within 10 ticks. Old = every case. Corrected = only lasting and reachable cases (the goal not below the new T_p; T_p not back by half the fall within the window).\n");
    println!("| scenario | world | model | at the ceiling (≥ 99) | actors under 30 % | corr(pressure, T_p) | decline, old | not counted | decline, corrected |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in measure_rows { println!("{r}"); }
    println!("\n## 3. Ц1's measure with Ц6\n");
    println!("Each actor under 20 % of living ticks at the ceiling (≥ 99) and at the floor (≤ 1) of economic_output; the spread of actor medians ≥ 20; the tier medians of §9.5 in order.\n");
    println!("| scenario | world | model | actors passing | failing (ceiling / floor share) | spread of medians | tier medians | ordered |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in c1_rows { println!("{r}"); }
    println!("\n## 4. The cascade\n");
    println!("| scenario | world | model | legitimacy ≤ 1 | cohesion < 15 | eo < T / 2 | Rome depopulated (games) |");
    println!("|---|---|---|---|---|---|---|");
    for r in cascade_rows { println!("{r}"); }
    println!("\n## 5. Deaths paired by seed against base\n");
    println!("| scenario | world | model | deaths | per seed | key actors |");
    println!("|---|---|---|---|---|---|");
    for r in death_rows { println!("{r}"); }
    println!("\n## 6. Worlds with more than 5 deaths a game: who dies (games) and their T_p on the last living tick (median)\n");
    println!("| scenario | world | model | dying |");
    println!("|---|---|---|---|");
    for r in dying_rows { println!("{r}"); }
    println!("\n## 7. Historical profile\n");
    println!("| scenario | world | model | |");
    println!("|---|---|---|---|");
    for r in hist_rows { println!("{r}"); }
    println!("\n## 8. For information (and vassalage-tick counts excluded from N)\n");
    println!("| scenario | world | model | wins | split on 40 | outcomes | regency stab / deep | vassalage ticks |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in info_rows { println!("{r}"); }
    println!("\n## 9. Ц6's measures, summed (the decision was taken on the sum)\n");
    println!("| model | worlds at the ceiling < 30 % | worlds with corr ≥ 0.7 | decline, old | decline, corrected | Byzantium (none): falls / median tick | Rome dies, min over rome worlds | Milan (aggressive) dies |");
    println!("|---|---|---|---|---|---|---|---|");
    let key = label(content_r);
    let v = &verdict[&key];
    let a = v.iter().filter(|x| x.ceiling_ok).count();
    let b = v.iter().filter(|x| x.corr_ok).count();
    let cl = v.iter().fold([(0u64, 0u64); 3], |mut s, x| { for (a, b) in s.iter_mut().zip(&x.classes) { a.0 += b.0; a.1 += b.1; } s });
    let n: u64 = cl.iter().map(|c| c.0).sum();
    let ok: u64 = cl.iter().map(|c| c.1).sum();
    let h = &hist[&key];
    let (bf, bt) = h.0.unwrap_or((0, f64::NAN));
    println!("| {key} | {a} / {} | {b} / {} | {:.0} % of {n} | {:.0} % of {} (unreachable {}, rebounded {}) | {bf} / {bt:.0} | {} | {} |", v.len(), v.len(),
        100.0 * ok as f64 / n.max(1) as f64, 100.0 * cl[2].1 as f64 / cl[2].0.max(1) as f64, cl[2].0, cl[0].0, cl[1].0,
        h.1.iter().copied().min().unwrap_or(0), h.2.unwrap_or(0));
}
