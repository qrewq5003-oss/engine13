//! Economy project, Ц6 stage 3: are the authored pressure auto-deltas a double count of the
//! threat? (docs/economy_project_brief.md §9). Built with `--features census`. Every world of the
//! three scenarios, 30 seeds × 300 ticks.
//!
//! Models: base (v2 without the threat model); (a) the threat model at r = 0.10 with the authored
//! pressure auto-deltas; (b), (c) the threat model at r = 0.05 / 0.10 without them
//! (`economy_v2_pressure_auto_deltas_off`). The measures of stage 2; the pre-commitment holds only
//! Ц6's own measures (ceiling, correlation, refined decline). Byzantium without a player: does it
//! fall in the 1450s (ticks 40–59) without its authored siege delta.
//!
//! Usage: cargo run --release --features census --bin c6s3_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

/// (r, authored pressure auto-deltas off)
const VARIANTS: [(f64, bool); 3] = [(0.10, false), (0.05, true), (0.10, true)];
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

/// None = base (no threat model); Some((r, authored pressure auto-deltas off))
type Model = Option<(f64, bool)>;

fn label(m: Model) -> String {
    match m {
        None => "base (v2, no threat model)".into(),
        Some((r, false)) => format!("(a) r = {r:.2}, auto-deltas kept"),
        Some((r, true)) => format!("{} r = {r:.2}, auto-deltas off", if r < 0.1 { "(b)" } else { "(c)" }),
    }
}

#[derive(Default)]
struct Run {
    acc: BTreeMap<String, [u64; 5]>,
    corr: [f64; 6],
    // refined decline: (drop, success)
    /// (drop, success, why it failed: 0 the goal lies below the new T_p, 1 T_p rebounded
    /// by half the drop or more inside the window, 2 other)
    declines: Vec<(f64, bool, u8)>,
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
        s.economy_v2_pressure_tags_as_level = m.is_some();
        s.economy_v2_pressure_pull = m.map(|x| x.0);
        s.economy_v2_pressure_auto_deltas_off = m.is_some_and(|x| x.1);
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    // open declines: (actor, deadline tick, target pressure, drop, goal < new T_p, new T_p, highest T_p since)
    let mut open: Vec<(String, u32, f64, f64, bool, f64, f64)> = Vec::new();
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
        open.retain_mut(|(id, deadline, goal, drop, below, tp_new, tp_max)| {
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) { *tp_max = tp_max.max(tp); }
            let why = if *below { 0 } else if *tp_max >= *tp_new + *drop / 2.0 { 1 } else { 2 };
            let ep = ws.actors.get(id).filter(|_| !ws.dead_actor_ids.contains(id)).map(|a| a.get_metric("external_pressure"));
            match ep {
                None => false,
                Some(ep) if ep <= *goal => { r.declines.push((*drop, true, why)); false }
                Some(_) if t >= *deadline => { r.declines.push((*drop, false, why)); false }
                _ => true,
            }
        });
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let ep = a.get_metric("external_pressure");
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
                    if drop >= 20.0 { open.push((id.clone(), t + 10, e0 - drop / 2.0, drop, e0 - drop / 2.0 < tp, tp, f64::MIN)); }
                }
                prev_tp.insert(id.clone(), tp);
            }
            prev_ep.insert(id.clone(), ep);
            if id == "rome" && a.get_metric("population") <= 1.0 { r.rome_zombie = true; }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    for (_, _, _, drop, below, tp_new, tp_max) in open { r.declines.push((drop, false, if below { 0 } else if tp_max >= tp_new + drop / 2.0 { 1 } else { 2 })); }
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
    declines: (u64, u64),
    /// failed cases by reason (see `Run::declines`)
    failed: [u64; 3],
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    println!("# Ц6 stage 3 — authored pressure auto-deltas against the threat, {seeds} seeds × {ticks} ticks per world\n");
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
    let main_models: Vec<Model> = std::iter::once(None).chain(VARIANTS.iter().map(|v| Some(*v))).collect();
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
                let with_sources = m == Some((0.10, true));
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
                let decl: Vec<&(f64, bool, u8)> = runs.iter().flat_map(|rr| rr.declines.iter()).collect();
                let ok = decl.iter().filter(|d| d.1).count();
                measure_rows.push(format!("| {sc} | {world} | {} | {ceiling:.0} % | {} / {} | {corr:.2} | {} cases, {:.0} % |", label(m),
                    pool.values().filter(|a| p(a[1], a[0]) < 30.0).count(), pool.len(), decl.len(), 100.0 * ok as f64 / decl.len().max(1) as f64));
                if m.is_some() {
                    verdict.entry(label(m)).or_default().push(Verdict { ceiling_ok: ceiling < 30.0, corr_ok: corr >= 0.7, declines: (ok as u64, decl.len() as u64), failed: [0u8, 1, 2].map(|k| decl.iter().filter(|d| !d.1 && d.2 == k).count() as u64) });
                }
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
    println!("## 1. Pressure writers that remain ((c): r = 0.10, auto-deltas off; the pull itself excluded)\n");
    println!("| scenario | writer | mean over all actor-ticks | largest per-actor mean (actor) | b / r at r = 0.03 / 0.05 / 0.10 |");
    println!("|---|---|---|---|---|");
    for r in br_rows { println!("{r}"); }
    println!("\n## 2. The Ц6 measure\n");
    println!("| scenario | world | model | at the ceiling (≥ 99) | actors under 30 % | corr(pressure, T_p) | refined decline: T_p falls ≥ 20, pressure falls ≥ half within 10 ticks |");
    println!("|---|---|---|---|---|---|---|");
    for r in measure_rows { println!("{r}"); }
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
    println!("\n## 9. The pre-commitment\n");
    println!("Only Ц6's own measures decide (§9.6); the death profile is for information.\n");
    println!("| variant | worlds at the ceiling < 30 % | worlds with corr ≥ 0.7 | refined decline overall | Byzantium (none): falls / median tick | Rome dies, min over rome worlds | Milan (aggressive) dies | passes |");
    println!("|---|---|---|---|---|---|---|---|");
    let mut fail_rows = Vec::new();
    for v in VARIANTS {
        let key = label(Some(v));
        let v = &verdict[&key];
        let a = v.iter().filter(|x| x.ceiling_ok).count();
        let b = v.iter().filter(|x| x.corr_ok).count();
        let (ok, n) = v.iter().fold((0, 0), |s, x| (s.0 + x.declines.0, s.1 + x.declines.1));
        let dec = 100.0 * ok as f64 / n.max(1) as f64;
        let failed = v.iter().fold([0u64; 3], |mut s, x| { for (a, b) in s.iter_mut().zip(&x.failed) { *a += b; } s });
        fail_rows.push(format!("| {key} | {} | {} | {} | {} |", n - ok, failed[0], failed[1], failed[2]));
        let h = &hist[&key];
        let (bf, bt) = h.0.unwrap_or((0, f64::NAN));
        let rmin = h.1.iter().copied().min().unwrap_or(0);
        let md = h.2.unwrap_or(0);
        let pass = a == v.len() && b == v.len() && dec >= 80.0;
        println!("| {key} | {a} / {} | {b} / {} | {dec:.0} % of {n} | {bf} / {bt:.0} | {rmin} | {md} | {} |", v.len(), v.len(), if pass { "yes" } else { "no" });
    }
    println!("\n## 10. Why the refined decline fails (failed cases)\n");
    println!("| variant | failed | goal below the new T_p (full convergence does not reach it) | T_p rebounded by half the drop or more within 10 ticks | other |");
    println!("|---|---|---|---|---|");
    for r in fail_rows { println!("{r}"); }
}
