//! Economy project, Ц5 stage 2: legitimacy as a level (docs/economy_project_brief.md §9). Built
//! with `--features census`. Every world of the three scenarios, 30 seeds × 300 ticks.
//!
//! The model (v2, `economy_v2_legitimacy_pull`): tags' `legitimacy` modifiers are levels; the
//! pull `L += r × (T_L − L)`, `T_L` = the authored start + the tags' levels; dependency rules
//! reading legitimacy measure against `T_L`. Variants: (a) r = 0.03, (b) 0.05, (c) 0.10, all with
//! `siege_rally_cohesion_bonus`; (d) r = 0.05 without it. Base = v2 as in the content (`fa54353`).
//!
//! Before the run (§9.7): the b / r table of every legitimacy writer at each r, from base.
//! Ц5's measure (pre-commitment): per actor, legitimacy ≤ 1 and ≥ 99 on ticks without a crisis tag
//! each under 20 %, for ≥ 80 % of actors in each world; the spread of actor medians ≥ 15 in each
//! world. For information: |L − T_L| ≥ 10; cohesion < 15, eo < T/2, Rome's depopulation; deaths
//! paired against base; the §9.2 rows; A38 (the trace of `milan_legitimacy`); the authored
//! conditions reading legitimacy and how often each held; Ц1's and Ц6's measures (a stop rule).
//!
//! Usage: cargo run --release --features census --bin c5s2_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];
const RALLY: &str = "siege_rally_cohesion_bonus";
const PULLS: [f64; 3] = [0.03, 0.05, 0.10];

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

/// None = base; Some((r, rally kept))
type Model = Option<(f64, bool)>;

fn label(m: Model) -> String {
    match m {
        None => "base (v2, fa54353)".into(),
        Some((r, true)) => format!("{} r = {r:.2}", match r { x if x < 0.04 => "(a)", x if x < 0.07 => "(b)", _ => "(c)" }),
        Some((r, false)) => format!("(d) r = {r:.2}, no siege rally"),
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
    floor: u64,
    ceiling: u64,
    normed: u64,
    dev10: u64,
    coh_gate: u64,
    eo_low: u64,
    ep_ceiling: u64,
}

impl Acc {
    fn add(&mut self, o: &Acc) {
        self.living += o.living; self.calm += o.calm; self.calm_floor += o.calm_floor; self.calm_ceiling += o.calm_ceiling;
        self.floor += o.floor; self.ceiling += o.ceiling; self.normed += o.normed; self.dev10 += o.dev10;
        self.coh_gate += o.coh_gate; self.eo_low += o.eo_low; self.ep_ceiling += o.ep_ceiling;
    }
}

#[derive(Default)]
struct Run {
    acc: BTreeMap<String, Acc>,
    legit: BTreeMap<String, Vec<f64>>,
    eo: BTreeMap<String, Vec<f64>>,
    corr: [f64; 6],
    /// Ц6 decline cases: (success, class: 0 unreachable, 1 rebounded, 2 counted)
    declines: Vec<(bool, u8)>,
    /// (actor, source) -> asked, legitimacy only
    sources: BTreeMap<(String, String), f64>,
    /// (dependency id, bin of the source) -> (writes, asked), legitimacy only
    bins: BTreeMap<(String, i32), (u64, f64)>,
    /// (context, test) -> (held, evaluated), conditions reading legitimacy
    occupancy: BTreeMap<(String, String), (u64, u64)>,
    /// A38: byzantium's legitimacy gain after `milan_legitimacy`, at +0 / +5 / +10 ticks
    a38: Vec<[f64; 3]>,
    deaths: u32,
    dead: BTreeMap<String, u32>,
    rome_zombie: bool,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    stab: bool,
    deep: bool,
}

fn run(sc: &str, world: &str, m: Model, seed: u64, ticks: u32, detail: bool) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        // base = the content as at fa54353 (no legitimacy pull), whatever the content holds now
        s.economy_v2_legitimacy_pull = m.map(|x| x.0);
        s.economy_v2_combat_outcome = false; // the world of stage 2, before Ц4
        if m.is_some_and(|x| !x.1) {
            let before = s.dependencies.len();
            s.dependencies.retain(|d| d.id != RALLY);
            assert_eq!(before - s.dependencies.len(), 1);
        }
    }
    let from: BTreeMap<String, String> = st.current_scenario.as_ref().unwrap().dependencies.iter()
        .map(|d| (d.id.clone(), d.from.as_str().to_string())).collect();
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut prev: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    type Open = (String, u32, f64, f64, bool, f64, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    // A38: (tick of the action, byzantium's legitimacy the tick before)
    let mut a38_open: Vec<(u32, f64)> = Vec::new();
    let mut byz_hist: Vec<f64> = Vec::new();
    let _ = census::take_writes();
    let _ = census::take_occupancy();
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
        let occ = census::take_occupancy();
        let scn = st.current_scenario.as_ref().unwrap();
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        if detail {
            for ((ctx, test), (held, n)) in occ {
                if ctx.starts_with("dependency") || !(ctx.contains("legitimacy") || test.contains("legitimacy")) { continue; }
                // one row per authored condition, not per actor it was tested on
                let ctx = match ctx.split_once(" @") {
                    Some((head, tail)) => format!("{head}{}", tail.find(' ').map_or(String::new(), |i| tail[i..].to_string())),
                    None => ctx,
                };
                let e = r.occupancy.entry((ctx, test)).or_default();
                e.0 += held; e.1 += n;
            }
        }
        let mut a38_now = false;
        for w in &writes {
            if w.metric != "legitimacy" { continue; }
            let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
            if src == "action milan_legitimacy" && w.actor == "byzantium" { a38_now = true; }
            if !detail { continue; }
            *r.sources.entry((w.actor.clone(), src.clone())).or_default() += w.requested;
            if let Some(id) = src.strip_prefix("dependency ") {
                if let Some(v) = from.get(id).and_then(|mm| prev.get(&w.actor).and_then(|p| p.get(mm))) {
                    let e = r.bins.entry((id.to_string(), ((v / 10.0).floor() as i32).clamp(0, 30))).or_default();
                    e.0 += 1;
                    e.1 += w.requested;
                }
            }
        }
        // A38 trace
        let byz = ws.actors.get("byzantium").filter(|_| !ws.dead_actor_ids.contains("byzantium")).map(|a| a.get_metric("legitimacy"));
        if a38_now {
            if let (Some(_), Some(b0)) = (byz, byz_hist.last()) { a38_open.push((t, *b0)); }
        }
        if let Some(b) = byz { byz_hist.push(b); } else { byz_hist.clear(); a38_open.clear(); }
        a38_open.retain(|(t0, b0)| {
            if t == t0 + 10 {
                let n = byz_hist.len();
                if n >= 11 { r.a38.push([byz_hist[n - 11] - b0, byz_hist[n - 6] - b0, byz_hist[n - 1] - b0]); }
                false
            } else { true }
        });
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) { r.dead.insert(d.clone(), t); }
        // Ц6's corrected decline
        open.retain_mut(|(id, deadline, goal, drop, below, tp_new, tp_max, reached)| {
            let Some(a) = ws.actors.get(id).filter(|_| !ws.dead_actor_ids.contains(id)) else { return false };
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) { *tp_max = tp_max.max(tp); }
            if a.get_metric("external_pressure") <= *goal { *reached = true; }
            if t < *deadline { return true; }
            r.declines.push((*reached, if *below { 0 } else if *tp_max >= *tp_new + *drop / 2.0 { 1 } else { 2 }));
            false
        });
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) { continue; }
            let a = &ws.actors[id];
            let l = a.get_metric("legitimacy");
            let ep = a.get_metric("external_pressure");
            let crisis = a.actor_tags.values().any(|tag| tag.metrics_modifier.iter().any(|(mm, v)| mm.as_str() == "legitimacy" && *v < 0));
            let e = r.acc.entry(id.clone()).or_default();
            e.living += 1;
            if l <= 1.0 { e.floor += 1; }
            if l >= 99.0 { e.ceiling += 1; }
            if !crisis {
                e.calm += 1;
                if l <= 1.0 { e.calm_floor += 1; }
                if l >= 99.0 { e.calm_ceiling += 1; }
            }
            if let Some(tl) = engine13::engine::metric_target(ws, scn, id, "legitimacy") {
                e.normed += 1;
                if (l - tl).abs() >= 10.0 { e.dev10 += 1; }
            }
            if a.get_metric("cohesion") < 15.0 { e.coh_gate += 1; }
            if engine13::engine::eo_target(ws, scn, id).is_some_and(|tg| a.get_metric("economic_output") < tg / 2.0) { e.eo_low += 1; }
            if ep >= 99.0 { e.ep_ceiling += 1; }
            r.legit.entry(id.clone()).or_default().push(l);
            r.eo.entry(id.clone()).or_default().push(a.get_metric("economic_output"));
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) {
                r.corr[0] += 1.0; r.corr[1] += ep; r.corr[2] += tp; r.corr[3] += ep * ep; r.corr[4] += tp * tp; r.corr[5] += ep * tp;
                if let (Some(p), Some(e0)) = (prev_tp.get(id), prev.get(id).and_then(|p| p.get("external_pressure"))) {
                    let drop = p - tp;
                    if drop >= 20.0 { open.push((id.clone(), t + 10, e0 - drop / 2.0, drop, e0 - drop / 2.0 < tp, tp, f64::MIN, false)); }
                }
                prev_tp.insert(id.clone(), tp);
            }
            if id == "rome" && a.get_metric("population") <= 1.0 { r.rome_zombie = true; }
            prev.insert(id.clone(), a.metrics.iter().map(|(k, v)| (k.clone(), *v)).collect());
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|mm| mm == "rome_splits") { r.split40 = true; }
    }
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

#[derive(Default)]
struct Verdict {
    c5_actors: bool,
    c5_spread: bool,
    c1_actors: bool,
    c1_tiers: bool,
    c1_spread: bool,
    c6_ceiling: bool,
    c6_corr: bool,
    /// corrected decline: (counted, successes)
    c6_decline: (u64, u64),
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    census::enable_occupancy();
    println!("# Ц5 stage 2 — legitimacy as a level, {seeds} seeds × {ticks} ticks per world\n");
    let mut models: Vec<Model> = vec![None];
    models.extend(PULLS.iter().map(|r| Some((*r, true))));
    models.push(Some((0.05, false)));
    let mut br_rows = Vec::new();
    let mut c5_rows = Vec::new();
    let mut c5_fail_rows = Vec::new();
    let mut c1_rows = Vec::new();
    let mut c6_rows = Vec::new();
    let mut casc_rows = Vec::new();
    let mut death_rows = Vec::new();
    let mut info_rows = Vec::new();
    let mut occ_rows = Vec::new();
    let mut a38_rows = Vec::new();
    let mut verdict: BTreeMap<String, Vec<Verdict>> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let mut src_pool: BTreeMap<(String, String), f64> = BTreeMap::new();
        let mut bin_pool: BTreeMap<(String, i32), (u64, f64)> = BTreeMap::new();
        let mut living_pool: BTreeMap<String, u64> = BTreeMap::new();
        let mut occ_pool: BTreeMap<String, census::Occupancy> = BTreeMap::new();
        for world in worlds(sc) {
            let base: Vec<Run> = (0..seeds).map(|s| run(sc, world, None, s, ticks, true)).collect();
            for rr in &base {
                for (k, x) in &rr.sources { *src_pool.entry(k.clone()).or_default() += x; }
                for (k, x) in &rr.bins { let e = bin_pool.entry(k.clone()).or_default(); e.0 += x.0; e.1 += x.1; }
                for (k, a) in &rr.acc { *living_pool.entry(k.clone()).or_default() += a.living; }
            }
            for m in models.clone() {
                let fresh: Vec<Run>;
                let runs: &[Run] = if m.is_none() { &base } else { fresh = (0..seeds).map(|s| run(sc, world, m, s, ticks, true)).collect(); &fresh };
                let o = occ_pool.entry(label(m)).or_default();
                for rr in runs { for (k, x) in &rr.occupancy { let e = o.entry(k.clone()).or_default(); e.0 += x.0; e.1 += x.1; } }
                let mut pool: BTreeMap<String, Acc> = BTreeMap::new();
                for rr in runs { for (k, a) in &rr.acc { pool.entry(k.clone()).or_default().add(a); } }
                let mut tot = Acc::default();
                for a in pool.values() { tot.add(a); }
                // Ц5
                let calm: Vec<(&String, &Acc)> = pool.iter().filter(|(_, a)| a.calm > 0).collect();
                let failing: Vec<String> = calm.iter().filter(|(_, a)| share(a.calm_floor, a.calm) >= 20.0 || share(a.calm_ceiling, a.calm) >= 20.0)
                    .map(|(k, a)| format!("{k} ({:.0} / {:.0} %)", share(a.calm_floor, a.calm), share(a.calm_ceiling, a.calm))).collect();
                let pass = calm.len() - failing.len();
                let mut legit: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for rr in runs { for (k, x) in &rr.legit { legit.entry(k.clone()).or_default().extend(x); } }
                let lmed: Vec<f64> = legit.values().map(|x| pct(x, 0.5)).collect();
                let lspread = lmed.iter().cloned().fold(f64::MIN, f64::max) - lmed.iter().cloned().fold(f64::MAX, f64::min);
                let c5_ok = 100.0 * pass as f64 / calm.len().max(1) as f64 >= 80.0;
                c5_rows.push(format!("| {sc} | {world} | {} | {:.0} % | {:.0} % | {pass} / {} | {lspread:.0} | {} | {:.0} % |", label(m),
                    share(tot.floor, tot.living), share(tot.ceiling, tot.living), calm.len(), q(&lmed), share(tot.dev10, tot.normed)));
                c5_fail_rows.push(format!("| {sc} | {world} | {} | {} |", label(m), if failing.is_empty() { "—".into() } else { failing.join(", ") }));
                // Ц1
                let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for rr in runs { for (k, x) in &rr.eo { eo.entry(k.clone()).or_default().extend(x); } }
                let frac = |x: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * x.iter().filter(|y| pr(**y)).count() as f64 / x.len().max(1) as f64;
                let c1_pass = eo.values().filter(|x| frac(x, &|y| y >= 99.0) < 20.0 && frac(x, &|y| y <= 1.0) < 20.0).count();
                let emed: Vec<f64> = eo.values().map(|x| pct(x, 0.5)).collect();
                let espread = emed.iter().cloned().fold(f64::MIN, f64::max) - emed.iter().cloned().fold(f64::MAX, f64::min);
                let tm: Vec<f64> = tiers(sc).iter().map(|t| { let x: Vec<f64> = t.iter().filter_map(|a| eo.get(*a)).flatten().copied().collect(); pct(&x, 0.5) }).collect();
                let ordered = tm.windows(2).all(|w| w[0] > w[1]);
                c1_rows.push(format!("| {sc} | {world} | {} | {c1_pass} / {} | {espread:.0} | {} | {} |", label(m), eo.len(),
                    tm.iter().map(|x| format!("{x:.0}")).collect::<Vec<_>>().join(" › "), if ordered { "yes" } else { "no" }));
                // Ц6
                let c = runs.iter().fold([0.0; 6], |mut s, rr| { for (x, y) in s.iter_mut().zip(&rr.corr) { *x += y; } s });
                let (n, sx, sy, sxx, syy, sxy) = (c[0], c[1], c[2], c[3], c[4], c[5]);
                let corr = (n * sxy - sx * sy) / ((n * sxx - sx * sx).sqrt() * (n * syy - sy * sy).sqrt());
                let counted: Vec<&(bool, u8)> = runs.iter().flat_map(|rr| rr.declines.iter()).filter(|d| d.1 == 2).collect();
                let ok = counted.iter().filter(|d| d.0).count();
                let ceiling = share(tot.ep_ceiling, tot.living);
                let mut at_ceiling: Vec<(String, f64)> = pool.iter().map(|(k, a)| (k.clone(), share(a.ep_ceiling, a.living))).filter(|x| x.1 >= 50.0).collect();
                at_ceiling.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let dead_share = |k: &str| runs.iter().filter(|rr| rr.dead.contains_key(k)).count();
                let who: Vec<String> = at_ceiling.iter().map(|(k, x)| format!("{k} {x:.0} % (dies {})", dead_share(k))).collect();
                c6_rows.push(format!("| {sc} | {world} | {} | {ceiling:.0} % | {corr:.2} | {} cases, {:.0} % | {} |", label(m), counted.len(), 100.0 * ok as f64 / counted.len().max(1) as f64,
                    if who.is_empty() { "—".into() } else { who.join(", ") }));
                verdict.entry(label(m)).or_default().push(Verdict {
                    c5_actors: c5_ok, c5_spread: lspread >= 15.0,
                    c1_actors: 100.0 * c1_pass as f64 / eo.len().max(1) as f64 >= 80.0, c1_tiers: ordered, c1_spread: espread >= 20.0,
                    c6_ceiling: ceiling < 30.0, c6_corr: corr >= 0.7, c6_decline: (counted.len() as u64, ok as u64),
                });
                // cascade, deaths
                let rome = if sc == "rome_375" { format!("{} / {seeds}", runs.iter().filter(|rr| rr.rome_zombie).count()) } else { "—".into() };
                casc_rows.push(format!("| {sc} | {world} | {} | {:.0} % | {:.0} % | {rome} |", label(m), share(tot.coh_gate, tot.living), share(tot.eo_low, tot.living)));
                let keys: Vec<String> = KEY.iter().filter(|k| base[0].acc.contains_key(**k)).map(|k| {
                    format!("{k} {}→{} ({})", base.iter().filter(|rr| rr.dead.contains_key(*k)).count(), runs.iter().filter(|rr| rr.dead.contains_key(*k)).count(),
                        paired(&base, runs, |rr| if rr.dead.contains_key(*k) { 1.0 } else { 0.0 }))
                }).collect();
                death_rows.push(format!("| {sc} | {world} | {} | {} | {} | {} |", label(m), runs.iter().map(|rr| rr.deaths).sum::<u32>(), paired(&base, runs, |rr| rr.deaths as f64), keys.join("; ")));
                // §9.2
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
                info_rows.push(format!("| {sc} | {world} | {} | {hist} | {wins_s} | {split} | {outc} | {fork} |", label(m)));
                let a38: Vec<&[f64; 3]> = runs.iter().flat_map(|rr| rr.a38.iter()).collect();
                if !a38.is_empty() {
                    let mean = |i: usize| a38.iter().map(|x| x[i]).sum::<f64>() / a38.len() as f64;
                    a38_rows.push(format!("| {sc} | {world} | {} | {} | {:+.1} | {:+.1} | {:+.1} |", label(m), a38.len(), mean(0), mean(1), mean(2)));
                }
            }
        }
        // b / r table from base
        let total_living: u64 = living_pool.values().sum();
        let mut by_src: BTreeMap<String, (f64, f64, String)> = BTreeMap::new();
        for ((actor, src), x) in &src_pool {
            let e = by_src.entry(src.clone()).or_insert((0.0, 0.0, String::new()));
            e.0 += x;
            let per_actor = x / living_pool.get(actor).copied().unwrap_or(1).max(1) as f64;
            if per_actor.abs() > e.1.abs() { e.1 = per_actor; e.2 = actor.clone(); }
        }
        let mut v: Vec<(String, (f64, f64, String))> = by_src.into_iter().collect();
        v.sort_by(|a, b| b.1 .1.abs().partial_cmp(&a.1 .1.abs()).unwrap());
        for (src, (sum, max, who)) in v.iter().filter(|x| x.1 .1.abs() >= 0.005) {
            let (b, state) = match src.strip_prefix("dependency ") {
                Some(id) => {
                    let bins: Vec<(i32, (u64, f64))> = bin_pool.iter().filter(|((d, _), _)| d == id).map(|((_, bb), x)| (*bb, *x)).collect();
                    let writes: u64 = bins.iter().map(|bb| bb.1 .0).sum();
                    match bins.iter().filter(|bb| bb.1 .0 * 50 >= writes).max_by(|a, b| (a.1 .1 / a.1 .0 as f64).abs().partial_cmp(&(b.1 .1 / b.1 .0 as f64).abs()).unwrap()) {
                        Some((bb, (n, x))) => (x / *n as f64, format!("source {}–{}: {:+.2} a write ({:.0} % of writes)", bb * 10, bb * 10 + 10, x / *n as f64, 100.0 * *n as f64 / writes.max(1) as f64)),
                        None => (*max, "—".into()),
                    }
                }
                None => (*max, format!("largest per-actor rate ({who})")),
            };
            br_rows.push(format!("| {sc} | {src} | {:+.3} | {state} | {:+.0} / {:+.0} / {:+.0} |", sum / total_living.max(1) as f64, b / 0.03, b / 0.05, b / 0.10));
        }
        for (model, o) in occ_pool {
            for ((ctx, test), (held, n)) in o {
                occ_rows.push(format!("| {sc} | {ctx} | {test} | {model} | {n} | {:.1} % |", share(held, n)));
            }
        }
    }
    println!("## 0. Before the run: b / r of every legitimacy writer (base, pooled over the scenario's worlds)\n");
    println!("b = for a dependency rule its largest single write (bins of its source with ≥ 2 % of the writes), otherwise the largest per-actor rate; under the pull it shifts the norm by b / r.\n");
    println!("| scenario | writer | mean per actor-tick | state at the largest b | b / r at r = 0.03 / 0.05 / 0.10 |");
    println!("|---|---|---|---|---|");
    for r in br_rows { println!("{r}"); }
    println!("\n## 1. Ц5's measure\n");
    println!("| scenario | world | model | legitimacy ≤ 1 | ≥ 99 | actors passing (calm floor and ceiling < 20 %) | spread of medians | actor medians p10/50/90 | \\|L − T_L\\| ≥ 10 |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in c5_rows { println!("{r}"); }
    println!("\n### Failing actors (calm floor / ceiling)\n");
    println!("| scenario | world | model | failing |");
    println!("|---|---|---|---|");
    for r in c5_fail_rows { println!("{r}"); }
    println!("\n## 2. Ц1's and Ц6's measures (stop rule)\n");
    println!("| scenario | world | model | Ц1 actors passing | eo spread | tier medians | ordered |");
    println!("|---|---|---|---|---|---|---|");
    for r in c1_rows { println!("{r}"); }
    println!();
    println!("| scenario | world | model | pressure at the ceiling | corr(pressure, T_p) | decline, corrected | actors at the ceiling ≥ 50 % of their life (games they die in) |");
    println!("|---|---|---|---|---|---|---|");
    for r in c6_rows { println!("{r}"); }
    println!("\n## 3. Cascade\n");
    println!("| scenario | world | model | cohesion < 15 | eo < T / 2 | Rome depopulated |");
    println!("|---|---|---|---|---|---|");
    for r in casc_rows { println!("{r}"); }
    println!("\n## 4. Deaths paired by seed against base\n");
    println!("| scenario | world | model | deaths | per seed | key actors (games, paired) |");
    println!("|---|---|---|---|---|---|");
    for r in death_rows { println!("{r}"); }
    println!("\n## 5. §9.2 (no gate)\n");
    println!("| scenario | world | model | key actor | wins | split on 40 | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in info_rows { println!("{r}"); }
    println!("\n### A38: byzantium's legitimacy after `milan_legitimacy` (+8), against the tick before\n");
    println!("| scenario | world | model | uses | +0 | +5 | +10 |");
    println!("|---|---|---|---|---|---|---|");
    for r in a38_rows { println!("{r}"); }
    println!("\n## 6. Authored conditions reading legitimacy: how often each held (all worlds of the scenario)\n");
    println!("| scenario | where | test | model | evaluated | held |");
    println!("|---|---|---|---|---|---|");
    for r in occ_rows { println!("{r}"); }
    println!("\n## 7. Pre-commitment (Ц5 only) and the stop rule (Ц1, Ц6)\n");
    println!("| model | Ц5: ≥ 80 % actors | Ц5: spread ≥ 15 | passes Ц5 | Ц1: actors / tiers / spread | Ц6: ceiling / corr | Ц6: decline, corrected |");
    println!("|---|---|---|---|---|---|---|");
    for m in models.iter().skip(1) {
        let v = &verdict[&label(*m)];
        let n = v.len();
        let cnt = |f: &dyn Fn(&Verdict) -> bool| v.iter().filter(|x| f(x)).count();
        let (dn, dok) = v.iter().fold((0, 0), |s, x| (s.0 + x.c6_decline.0, s.1 + x.c6_decline.1));
        let pass = cnt(&|x| x.c5_actors) == n && cnt(&|x| x.c5_spread) == n;
        println!("| {} | {} / {n} | {} / {n} | {} | {} / {} / {} | {} / {} | {:.0} % of {dn} |", label(*m), cnt(&|x| x.c5_actors), cnt(&|x| x.c5_spread), if pass { "yes" } else { "no" },
            cnt(&|x| x.c1_actors), cnt(&|x| x.c1_tiers), cnt(&|x| x.c1_spread), cnt(&|x| x.c6_ceiling), cnt(&|x| x.c6_corr), 100.0 * dok as f64 / dn.max(1) as f64);
    }
}
