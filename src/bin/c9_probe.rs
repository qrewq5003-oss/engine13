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
    /// every actor metric every tick, bit for bit
    fingerprint: u64,
    /// Milan's army on ticks 0 and 1 (the state after them)
    milan_army: Vec<f64>,
    /// the Ottoman army on its living ticks
    ott: Vec<(u32, f64)>,
    /// the tick `mehmed_accelerates` fired
    accel: Option<u32>,
    /// Ц10, per actor: (living ticks, zombie ticks (population ≤ 1), ticks below a quarter of P₀,
    /// ticks with eo < T / 4, ticks in debt among the non-zombie)
    popc: BTreeMap<String, [u64; 5]>,
    /// Ц10: eo / T on each zombie tick, per actor
    zombie_eo: BTreeMap<String, Vec<f64>>,
    /// Ц10: Rome's population and its (scaled) base on tick 100
    rome100: Option<(f64, f64)>,
    /// Ц2: treasury on ticks 0, 10, …, 150, per actor
    tr_steps: BTreeMap<String, Vec<f64>>,
    /// Ц10: population written, (actor, source) -> (taken, added)
    pop_src: BTreeMap<(String, String), (f64, f64)>,
    /// Ц10: Rome's eo / T on tick 100
    rome_eo100: Option<f64>,
    /// Ц10: (event, actor) -> firings of `flood`, `famine`, `plague`
    ev: BTreeMap<(String, String), u32>,
    /// Ц10: eo / T on every living tick, per actor
    eo_rel: BTreeMap<String, Vec<f64>>,
    /// economic_output written, (actor, source) -> sum
    eo_src: BTreeMap<(String, String), f64>,
    /// Ц2: ticks the unpaid share of the army left, per actor
    desert: BTreeMap<String, u32>,
    /// Ц2: treasury / income on ticks 250–299, per actor
    tr_inc: BTreeMap<String, Vec<f64>>,
    /// the five peoples' armies (sum) on ticks 15, 45, 100
    peoples: BTreeMap<u32, f64>,
}

/// model: 0 = base, 1 = Ц7, 2 = Ц9, 3 = the content before Ц10, 4 = that and Ц3 set here,
/// 5 = the content and Ц10 (а), 6 = Ц10 (б), 7 = the content and Ц2, 8 = the content as it stands
fn run(sc: &str, world: &str, model: usize, quality: bool, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        // model 3: the content as it stands (Ц7 and Ц9 written) — the content check;
        // model 4: the content and Ц3 set here — `mehmed_rises` gives the Ottoman army +60 (v2)
        if model == 4 {
            let m = s.milestone_events.iter_mut().find(|m| m.id == "mehmed_rises");
            if let Some(m) = m { m.economy_v2_effects.insert(engine13::core::MetricRef::literal("actor:ottomans.military_size"), 60.0); }
        }
        if model < 3 {
            s.economy_v2_conquest_k2 = (model >= 1).then_some(1);
            s.economy_v2_alliances = model == 2;
        }
        // model 5: the content and Ц10 (а) set here — population pulled to P₀ × eo / T, r = 0.01;
        // model 6: Ц10 (б) — pulled to the constant P₀, the deficit rules kept (census)
        if model == 5 || model == 6 { s.economy_v2_population_pull = Some(0.01); }
        // model 7: the content and Ц2 (the army paid out of the treasury) set here
        if model == 7 { s.economy_v2_army_pay = true; }
        // models 0–4 are the worlds before Ц10 (it is in the content since its write); model 8 — the
        // content exactly as it stands, nothing overridden (the base of Ц2)
        if model <= 4 { s.economy_v2_population_pull = None; }
    }
    census::set_population_constant_norm(model == 6);
    census::set_combat_quality(quality);
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    type Open = (String, u32, f64, f64, bool, f64, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    let _ = census::take_battles();
    let _ = census::take_writes();
    let mut fp = std::collections::hash_map::DefaultHasher::new();
    if let Some(m) = st.world_state.as_ref().unwrap().actors.get("milan") { r.milan_army.push(m.get_metric("military_size")); }
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
        for w in census::take_writes().into_iter().filter(|w| w.metric == "population" || w.metric == "economic_output" || (w.metric == "military_size" && w.source.as_deref() == Some("unpaid army"))) {
            if w.metric == "military_size" { *r.desert.entry(w.actor.clone()).or_default() += 1; continue; }
            let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file().rsplit('/').next().unwrap_or(""), w.location.line()));
            if w.metric == "economic_output" { *r.eo_src.entry((w.actor.clone(), src)).or_default() += w.applied; continue; }
            let e = r.pop_src.entry((w.actor.clone(), src)).or_default();
            if w.applied < 0.0 { e.0 += w.applied; } else { e.1 += w.applied; }
        }
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
        if t <= 1 { if let Some(m) = ws.actors.get("milan") { r.milan_army.push(m.get_metric("military_size")); } }
        let mut all_ids: Vec<&String> = ws.actors.keys().collect();
        all_ids.sort();
        for id in all_ids {
            let mut ms: Vec<(&String, &f64)> = ws.actors[id].metrics.iter().collect();
            ms.sort_by(|x, y| x.0.cmp(y.0));
            for (k, v) in ms { std::hash::Hash::hash(&(id, k, v.to_bits()), &mut fp); }
        }
        let mut ids: Vec<&String> = ws.actors.keys().filter(|id| !ws.dead_actor_ids.contains(*id)).collect();
        ids.sort();
        let scn = st.current_scenario.as_ref().unwrap();
        if t == 100 {
            if let (Some(a), Some(b)) = (ws.actors.get("rome").filter(|_| !ws.dead_actor_ids.contains("rome")), engine13::engine::population_base(ws, scn, "rome")) { r.rome100 = Some((a.get_metric("population"), b)); }
            if let (Some(a), Some(tt)) = (ws.actors.get("rome"), engine13::engine::eo_target(ws, scn, "rome")) { r.rome_eo100 = Some(a.get_metric("economic_output") / tt); }
        }
        for id in ids {
            let a = &ws.actors[id];
            {
                let pop = a.get_metric("population");
                let eo = a.get_metric("economic_output");
                let c = r.popc.entry(id.clone()).or_default();
                c[0] += 1;
                let tt = engine13::engine::eo_target(ws, scn, id).filter(|x| *x > 0.0);
                if pop <= 1.0 {
                    c[1] += 1;
                    if let Some(tt) = tt { r.zombie_eo.entry(id.clone()).or_default().push(eo / tt); }
                } else if a.get_metric("treasury") < 0.0 { c[4] += 1; }
                if engine13::engine::population_base(ws, scn, id).is_some_and(|b| pop < 0.25 * b) { c[2] += 1; }
                if tt.is_some_and(|tt| eo < tt / 4.0) { c[3] += 1; }
                if let Some(tt) = tt { r.eo_rel.entry(id.clone()).or_default().push(eo / tt); }
                if t >= 250 { let inc = engine13::engine::interactions::tick_income(a, scn); if inc > 0.0 { r.tr_inc.entry(id.clone()).or_default().push(a.get_metric("treasury") / inc); } }
                if t % 10 == 0 && t <= 150 { r.tr_steps.entry(id.clone()).or_default().push(a.get_metric("treasury")); }
            }
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
        if [15, 45, 100].contains(&t) { r.peoples.insert(t, PEOPLES.iter().filter_map(|p| ws.actors.get(*p).filter(|_| !ws.dead_actor_ids.contains(*p))).map(|a| a.get_metric("military_size")).sum()); }
        if let Some(o) = ws.actors.get("ottomans").filter(|_| !ws.dead_actor_ids.contains("ottomans")) { r.ott.push((t, o.get_metric("military_size"))); }
        if r.accel.is_none() && ws.milestone_events_fired.iter().any(|m| m == "mehmed_accelerates") { r.accel = Some(t); }
    }
    census::set_combat_quality(true);
    census::set_population_constant_norm(false);
    r.fingerprint = std::hash::Hasher::finish(&fp);
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    for e in st.event_log.events.iter().filter(|e| ["flood", "famine", "plague"].contains(&e.id.as_str())) {
        *r.ev.entry((e.id.clone(), e.actor_id.clone())).or_default() += 1;
    }
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
    if args.get(4).map(String::as_str) == Some("content") { content_check(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("c3") { c3_run(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("c10pre") { c10_pre(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("c10") { c10_run(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("c2") { census::watch_all_metrics(true); c2_run(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("c2c4") {
        println!("# Ц4 by scenario, base (content) against Ц2, seeds {first}–{}\n", first + seeds - 1);
        println!("| model | scope | battles | (1) won by higher quality vs promise − 2 SE | (2) with − without quality vs 2 SE |");
        println!("|---|---|---|---|---|");
        for (m, name) in [(8usize, "base"), (7, "Ц2")] {
            let mut all: C4Pool = Default::default();
            for sc in SCENARIOS {
                let mut p: C4Pool = Default::default();
                for world in worlds(sc) {
                    for sd in first..first + seeds {
                        let x = c4(&run(sc, world, m, true, sd, ticks).battles); p.0 .0 += x.0; p.0 .1 += x.1; p.0 .2 += x.2; p.0 .3 += x.3;
                        let y = c4(&run(sc, world, m, false, sd, ticks).battles); p.1 .0 += y.0; p.1 .1 += y.1;
                    }
                }
                println!("{}", c4_row(name, sc, p));
                all.0 .0 += p.0 .0; all.0 .1 += p.0 .1; all.0 .2 += p.0 .2; all.0 .3 += p.0 .3; all.1 .0 += p.1 .0; all.1 .1 += p.1 .1;
            }
            println!("{}", c4_row(name, "**all**", all));
        }
        return;
    }
    if args.get(4).map(String::as_str) == Some("c10ev") { c10_events(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("eolow") { census::watch_all_metrics(true); eo_low(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("c10why") { census::watch_all_metrics(true); c10_why(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("c2content") {
        let (mut same, mut total) = (0, 0);
        for sc in SCENARIOS { for world in worlds(sc) { for sd in first..first + seeds {
            total += 1;
            if run_content(sc, world, sd, ticks) == run(sc, world, 7, true, sd, ticks).fingerprint { same += 1; }
        } } }
        println!("Ц2 content check, seeds {first}–{}: {same} of {total} runs identical (content against Ц2 set here).", first + seeds - 1);
        return;
    }
    if args.get(4).map(String::as_str) == Some("c10content") {
        let (mut same, mut total) = (0, 0);
        for sc in SCENARIOS {
            for world in worlds(sc) {
                for s in first..first + seeds {
                    total += 1;
                    if run_content(sc, world, s, ticks) == run(sc, world, 5, true, s, ticks).fingerprint { same += 1; }
                }
            }
        }
        println!("Ц10 content check, seeds {first}–{}: {same} of {total} runs identical (content against Ц10 (а) set here).", first + seeds - 1);
        return;
    }
    if args.get(4).map(String::as_str) == Some("c3content") {
        let (mut same, mut total) = (0, 0);
        for world in worlds("constantinople_1430") {
            for s in first..first + seeds {
                total += 1;
                if run("constantinople_1430", world, 3, true, s, ticks).fingerprint == run("constantinople_1430", world, 4, true, s, ticks).fingerprint { same += 1; }
            }
        }
        println!("Ц3 content check, constantinople, seeds {first}–{}: {same} of {total} runs identical (content against Ц3 set here).", first + seeds - 1);
        return;
    }
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

/// The content check after the write: v2 as the content stands against Ц9 set by the probe, every
/// actor metric every tick; and in milan, Milan's army on ticks 0–1 and the tick the league turns.
fn content_check(first: u64, seeds: u64, ticks: u32) {
    let (mut same, mut total) = (0, 0);
    let mut rows = Vec::new();
    for sc in SCENARIOS {
        for world in worlds(sc) {
            let mut lt = Vec::new();
            let mut army: Vec<Vec<f64>> = Vec::new();
            for s in first..first + seeds {
                let a = run(sc, world, 3, true, s, ticks);
                let b = run(sc, world, 2, true, s, ticks);
                total += 1;
                if a.fingerprint == b.fingerprint { same += 1; }
                if let Some(t) = a.league_turns { lt.push(t as f64); }
                army.push(a.milan_army.clone());
            }
            if sc == "milan_1477" {
                let at = |i: usize| q(&army.iter().filter_map(|v| v.get(i).copied()).collect::<Vec<f64>>());
                rows.push(format!("| {world} | {} / {} / {} | {} @ {} |", at(0), at(1), at(2), lt.len(), q(&lt)));
            }
        }
    }
    println!("Content check, seeds {first}–{}: {same} of {total} runs identical (content against Ц9 set here), every actor metric every tick.\n", first + seeds - 1);
    println!("| milan world | Milan's army p10/50/90: start / after tick 0 / after tick 1 | league turns on Milan: games @ tick p10/50/90 |");
    println!("|---|---|---|");
    for r in rows { println!("{r}"); }
}

fn window(v: &[(u32, f64)], a: u32, b: u32) -> Option<f64> {
    let x: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect();
    (!x.is_empty()).then(|| x.iter().sum::<f64>() / x.len() as f64)
}

/// Ц3 (owner's model): `mehmed_rises` (tick 42) gives the Ottoman army +60 under v2. constantinople
/// only — the milestone is there; rome and milan have nothing it touches. Base = v2 as in the
/// content (Ц1, Ц4–Ц9), the variant = the same with the step. Ц3's measure, the stop rule (Ц1, Ц4,
/// Ц5, Ц6, Ц7 item 2, Ц8; Ц9's measure is milan's), and for information A10, wins, Ottoman battles
/// around `mehmed_accelerates`, the assault, new submissions to the Ottomans after tick 42.
fn c3_run(first: u64, seeds: u64, ticks: u32) {
    let sc = "constantinople_1430";
    println!("# Ц3 — +60 Ottoman army on `mehmed_rises` (tick 42), constantinople, seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    let mut rows = Vec::new();
    let mut stop_rows = Vec::new();
    let mut stops: Vec<String> = Vec::new();
    let mut info = Vec::new();
    let mut c4p: BTreeMap<usize, C4Pool> = BTreeMap::new();
    let mut c3_all = true;
    for world in worlds(sc) {
        let base: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, 3, true, s, ticks)).collect();
        let var: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, 4, true, s, ticks)).collect();
        for (m, runs) in [(3usize, &base), (4, &var)] {
            let nq: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, m, false, s, ticks)).collect();
            let e = c4p.entry(m).or_default();
            for rr in runs.iter() { let x = c4(&rr.battles); e.0 .0 += x.0; e.0 .1 += x.1; e.0 .2 += x.2; e.0 .3 += x.3; }
            for rr in &nq { let x = c4(&rr.battles); e.1 .0 += x.0; e.1 .1 += x.1; }
            // Ц3's measure
            let pairs: Vec<(f64, f64)> = runs.iter().filter_map(|r| Some((window(&r.ott, 10, 20)?, window(&r.ott, 40, 50)?))).collect();
            let d: Vec<f64> = pairs.iter().map(|(a, b)| b - 1.25 * a).collect();
            let n = d.len() as f64;
            let mean = d.iter().sum::<f64>() / n.max(1.0);
            let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
            let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
            let ok = mean > 0.0 && t >= 2.0;
            if m == 4 && !ok { c3_all = false; }
            let ratio = pairs.iter().map(|(a, b)| b / a).sum::<f64>() / n.max(1.0);
            let label = if m == 3 { "base" } else { "Ц3" };
            rows.push(format!("| {world} | {label} | {} | {ratio:.3} | {mean:+.1} (t {t:+.1}) | {} |", pairs.len(), if ok { "**yes**" } else { "no" }));
            // information
            let living: usize = runs.iter().map(|r| r.ott.len()).sum();
            let above: usize = runs.iter().map(|r| r.ott.iter().filter(|p| p.1 > 220.0).count()).sum();
            let wins: Vec<f64> = runs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
            let byz: Vec<f64> = runs.iter().filter_map(|r| r.dead.get("byzantium").map(|t| *t as f64)).collect();
            let at46 = byz.iter().filter(|t| **t == 46.0).count();
            let (mut bw, mut bn, mut aw, mut an) = (0, 0, 0, 0);
            for r in runs.iter() {
                let Some(ac) = r.accel else { continue };
                for b in r.battles.iter().filter(|b| b.attacker == "ottomans" || b.defender == "ottomans") {
                    let won = (b.attacker == "ottomans") == b.attacker_won;
                    if b.tick < ac { bn += 1; bw += won as u32; } else { an += 1; aw += won as u32; }
                }
            }
            let accel = runs.iter().filter(|r| r.accel.is_some()).count();
            let mut subs: BTreeMap<String, u32> = BTreeMap::new();
            for r in runs.iter() { for ((v, l), t0) in &r.vassal { if l == "ottomans" && *t0 >= 42 { *subs.entry(v.clone()).or_default() += 1; } } }
            info.push(format!("| {world} | {label} | {:.0} % | {} @ {} | {} @ {} (tick 46: {at46}) | {accel}: {bw}/{bn} → {aw}/{an} | {} |",
                100.0 * above as f64 / living.max(1) as f64, wins.len(), q(&wins), byz.len(), q(&byz),
                if subs.is_empty() { "—".into() } else { subs.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ") }));
        }
        // stop rule, world-level, and Ц7 item 2 (none)
        let (ok_b, det_b) = world_measures(sc, &base);
        let (ok_v, det_v) = world_measures(sc, &var);
        const MEASURES: [&str; 9] = ["Ц1 actors", "Ц1 tiers", "Ц1 spread", "Ц5 actors", "Ц5 spread", "Ц6 ceiling", "Ц6 corr", "Ц8 extremes", "Ц8 spread"];
        for (i, name) in MEASURES.iter().enumerate() { if ok_b[i] && !ok_v[i] { stops.push(format!("{world}: {name}")); } }
        if *world == "none" {
            let ok7 = |runs: &[Run]| { let ft: Vec<f64> = runs.iter().filter_map(|r| r.dead.get("byzantium").map(|t| *t as f64)).collect(); ft.len() >= 20 && (40.0..=59.0).contains(&pct(&ft, 0.5)) };
            if ok7(&base) && !ok7(&var) { stops.push("Ц7 item 2".into()); }
        }
        stop_rows.push(format!("| {world} | {det_b} | {det_v} |"));
    }
    println!("## 1. Ц3's measure: m₄₀ ≥ 1.25 × m₁₀, d = m₄₀ − 1.25 m₁₀ paired, t ≥ 2\n");
    println!("| world | model | games | mean m₄₀/m₁₀ | d (t) | passes |");
    println!("|---|---|---|---|---|---|");
    for r in rows { println!("{r}"); }
    println!("\nЦ3 passes in every world: {}", if c3_all { "**yes**" } else { "**no**" });
    println!("\n## 2. Stop rule against base on the same seeds\n");
    println!("| world | base | Ц3 |");
    println!("|---|---|---|");
    for r in stop_rows { println!("{r}"); }
    for m in [3usize, 4] {
        let ((n, w, sp, spq), (nb, wb)) = c4p[&m];
        let pa = w as f64 / n.max(1) as f64;
        let promise = sp / n.max(1) as f64;
        let se = spq.sqrt() / n.max(1) as f64;
        let pb = wb as f64 / nb.max(1) as f64;
        let se_d = (pa * (1.0 - pa) / n.max(1) as f64 + pb * (1.0 - pb) / nb.max(1) as f64).sqrt();
        println!("- {}: Ц4 (1) {:.1} % vs {:.1} % — {}; Ц4 (2) {:+.1} vs 2 SE {:.1} — {} ({n} battles)", if m == 3 { "base" } else { "Ц3" }, 100.0 * pa, 100.0 * (promise - 2.0 * se),
            if pa >= promise - 2.0 * se { "yes" } else { "no" }, 100.0 * (pa - pb), 200.0 * se_d, if pa - pb >= 2.0 * se_d { "yes" } else { "no" });
    }
    println!("\nStops (base passes, Ц3 does not; Ц4 read above): {}", if stops.is_empty() { "none".into() } else { stops.join("; ") });
    println!("\n## 3. For information\n");
    println!("| world | model | Ottoman army > 220, living ticks | wins @ tick p10/50/90 | Byzantium dies @ tick | games with `mehmed_accelerates`: Ottoman battles won/fought before → after it | submit to the Ottomans on tick ≥ 42 (games) |");
    println!("|---|---|---|---|---|---|---|");
    for r in info { println!("{r}"); }
}

/// Ц10's measure over a world's runs: (zombie share, share below a quarter of P₀, eo < T/4 share)
fn c10_shares(runs: &[Run]) -> (f64, f64, f64) {
    let mut c = [0u64; 5];
    for r in runs { for v in r.popc.values() { for i in 0..5 { c[i] += v[i]; } } }
    (share(c[1], c[0]), share(c[2], c[0]), share(c[3], c[0]))
}

/// Ц10 before the run (§9.7), on the protocols of v2 as in the content: per world the share of
/// living actor-ticks with eo < T / 4 (about where the norm would stand below a quarter of P₀), the
/// zombie and below-a-quarter shares now; the zombies' starting population and eo / T on their
/// zombie ticks (do they leave zero if eo is off the floor).
fn c10_pre(first: u64, seeds: u64, ticks: u32) {
    println!("# Ц10 before the run — v2 as in the content, seeds {first}–{}\n", first + seeds - 1);
    println!("| scenario | world | eo < T/4, living actor-ticks | zombies now | below a quarter of P₀ now |");
    println!("|---|---|---|---|---|");
    let mut zrows = Vec::new();
    for sc in SCENARIOS {
        let scn = engine13::scenarios::registry::load_by_id(sc).unwrap();
        for world in worlds(sc) {
            let runs: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, 3, true, s, ticks)).collect();
            let (z, low, eq) = c10_shares(&runs);
            println!("| {sc} | {world} | {eq:.1} % | {z:.1} % | {low:.1} % |");
            let mut ze: BTreeMap<String, Vec<f64>> = BTreeMap::new();
            for r in &runs { for (k, v) in &r.zombie_eo { ze.entry(k.clone()).or_default().extend(v); } }
            for (k, v) in ze.iter().filter(|(_, v)| v.len() as u64 >= seeds * 10) {
                let p0 = engine13::engine::metric_base(&scn, k, "population").unwrap_or(f64::NAN);
                let frac = |v: &[f64]| { let x: Vec<f64> = v.iter().map(|y| y * 100.0).collect(); q(&x) };
                zrows.push(format!("| {sc} | {world} | {k} | {p0:.0} | {} | {} % | {:.0} |", v.len(), frac(v), p0 * pct(v, 0.5)));
            }
        }
    }
    println!("\n### Zombies now (≥ 10 zombie ticks a game on average): P₀, zombie ticks, eo / T on them (p10/50/90, %), the norm P₀ × eo / T at the median\n");
    println!("| scenario | world | actor | P₀ | zombie ticks | eo / T | norm at the median |");
    println!("|---|---|---|---|---|---|---|");
    for r in zrows { println!("{r}"); }
}

/// Ц10: base (v2 as in the content) against (а) the model and (б) the constant norm with the
/// deficit rules kept. Ц10's measure; the stop rule (Ц1, Ц3–Ц9 against base); for information
/// Ц2's measures (debt, accumulation), deaths paired, the pairs of Ц7.
fn c10_run(first: u64, seeds: u64, ticks: u32) {
    println!("# Ц10 — population pulled to its norm, seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    const MEASURES: [&str; 9] = ["Ц1 actors", "Ц1 tiers", "Ц1 spread", "Ц5 actors", "Ц5 spread", "Ц6 ceiling", "Ц6 corr", "Ц8 extremes", "Ц8 spread"];
    let labels = |m: usize| match m { 3 => "base", 5 => "(а)", _ => "(б)" };
    let mut c10_rows = Vec::new();
    let mut c10_ok: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut stops: Vec<String> = Vec::new();
    let mut stop_rows = Vec::new();
    let mut c7_rows = Vec::new();
    let mut c3_rows = Vec::new();
    let mut c4p: BTreeMap<usize, C4Pool> = BTreeMap::new();
    let mut info = Vec::new();
    let mut pair_rows = Vec::new();
    for sc in SCENARIOS {
        for world in worlds(sc) {
            let all: BTreeMap<usize, Vec<Run>> = [3usize, 5, 6].iter().map(|m| (*m, (first..first + seeds).map(|s| run(sc, world, *m, true, s, ticks)).collect())).collect();
            for m in [3usize, 5] {
                let nq: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, m, false, s, ticks)).collect();
                let e = c4p.entry(m).or_default();
                for rr in &all[&m] { let x = c4(&rr.battles); e.0 .0 += x.0; e.0 .1 += x.1; e.0 .2 += x.2; e.0 .3 += x.3; }
                for rr in &nq { let x = c4(&rr.battles); e.1 .0 += x.0; e.1 .1 += x.1; }
            }
            // ---- Ц10's measure
            for m in [3usize, 5, 6] {
                let runs = &all[&m];
                let (z, low, _) = c10_shares(runs);
                let rome: Vec<(f64, f64)> = runs.iter().filter_map(|r| r.rome100).collect();
                // condition 3 (corrected after PR #252): Rome's base after the split is the seat's share × 8000
                // in every game; population above it is allowed (eo / T > 1). Base: no pull, no scaled base.
                let seat = {
                    let scn = engine13::scenarios::registry::load_by_id("rome_375").unwrap();
                    let r0 = scn.actors.iter().find(|a| a.id == "rome").unwrap();
                    let total: f64 = r0.on_collapse.iter().map(|h| h.weight).sum();
                    r0.on_collapse.iter().find(|h| h.keeps_seat).map_or(1.0, |h| h.weight / total) * 8000.0
                };
                let rome_ok = sc != "rome_375" || (rome.len() == runs.len() && rome.iter().all(|(_, b)| (*b - seat).abs() < 1e-6));
                let ok = z < 1.0 && low < 10.0 && rome_ok;
                let e = c10_ok.entry(m).or_default();
                e.1 += 1; e.0 += ok as usize;
                let rome_cell = if sc == "rome_375" { format!("base = {seat:.0} in {} of {} games; pop p50 {:.0} (p90 {:.0})", rome.iter().filter(|(_, b)| (*b - seat).abs() < 1e-6).count(), runs.len(), pct(&rome.iter().map(|x| x.0).collect::<Vec<_>>(), 0.5), pct(&rome.iter().map(|x| x.0).collect::<Vec<_>>(), 0.9)) } else { "—".into() };
                c10_rows.push(format!("| {sc} | {world} | {} | {z:.2} % | {low:.1} % | {rome_cell} | {} |", labels(m), if ok { "**yes**" } else { "no" }));
            }
            let base = &all[&3];
            let var = &all[&5];
            // ---- stop rule
            let (ok_b, det_b) = world_measures(sc, base);
            let (ok_v, det_v) = world_measures(sc, var);
            for (i, name) in MEASURES.iter().enumerate() { if ok_b[i] && !ok_v[i] { stops.push(format!("{sc} {world}: {name}")); } }
            stop_rows.push(format!("| {sc} | {world} | {det_b} | {det_v} |"));
            let vassal_of = |rr: &Run, v: &str, lord: Option<&str>| rr.vassal.keys().any(|(a, b)| a == v && lord.is_none_or(|l| l == b));
            for (m, runs) in [(3usize, base), (5, var)] {
                let (cell, ok) = match sc {
                    "rome_375" => { let pg = runs.iter().map(|rr| PEOPLES.iter().filter(|p| vassal_of(rr, p, Some("rome"))).count()).sum::<usize>() as f64 / seeds as f64; (format!("five peoples {pg:.1}"), pg >= 3.0) }
                    "constantinople_1430" => { let ft: Vec<f64> = runs.iter().filter_map(|rr| rr.dead.get("byzantium").map(|t| *t as f64)).collect(); (format!("Byzantium falls {} @ {}", ft.len(), q(&ft)), *world != "none" || (ft.len() >= 20 && (40.0..=59.0).contains(&pct(&ft, 0.5)))) }
                    _ => { let hit = |id: &str| runs.iter().filter(|rr| rr.dead.contains_key(id) || vassal_of(rr, id, None)).count(); (format!("Milan {}, papacy {}, Venice {}", hit("milan"), hit("papacy"), hit("venice")), hit("milan") <= 1 && hit("papacy") <= 3 && hit("venice") <= 3) }
                };
                if m == 5 && !ok && c7_rows.last().is_some_and(|l: &String| l.contains("| base |") && l.ends_with("yes |")) { stops.push(format!("{sc} {world}: Ц7")); }
                c7_rows.push(format!("| {sc} | {world} | {} | {cell} | {} |", labels(m), if ok { "yes" } else { "no" }));
                if sc == "constantinople_1430" {
                    let pairs: Vec<(f64, f64)> = runs.iter().filter_map(|r| Some((window(&r.ott, 10, 20)?, window(&r.ott, 40, 50)?))).collect();
                    let d: Vec<f64> = pairs.iter().map(|(a, b)| b - 1.25 * a).collect();
                    let n = d.len() as f64;
                    let mean = d.iter().sum::<f64>() / n.max(1.0);
                    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
                    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
                    let ok3 = mean > 0.0 && t >= 2.0;
                    if m == 5 && !ok3 { stops.push(format!("{world}: Ц3")); }
                    c3_rows.push(format!("| {world} | {} | {mean:+.1} (t {t:+.1}) | {} |", labels(m), if ok3 { "yes" } else { "no" }));
                }
            }
            // ---- information: Ц2 measures, deaths
            for m in [3usize, 5] {
                let runs = &all[&m];
                let mut debt: BTreeMap<String, (u64, u64)> = BTreeMap::new();
                for r in runs { for (k, v) in &r.popc { let e = debt.entry(k.clone()).or_default(); e.0 += v[0] - v[1]; e.1 += v[4]; } }
                let over: Vec<String> = debt.iter().filter(|(_, v)| share(v.1, v.0) > 20.0).map(|(k, v)| format!("{k} {:.0}", share(v.1, v.0))).collect();
                let mut mono: BTreeMap<String, u32> = BTreeMap::new();
                for r in runs { for (k, v) in &r.tr_steps { if v.len() == 16 && v.windows(2).all(|w| w[1] >= w[0]) && v[15] > v[0] { *mono.entry(k.clone()).or_default() += 1; } } }
                let monos: Vec<String> = mono.iter().filter(|(_, n)| **n as u64 * 2 >= seeds).map(|(k, n)| format!("{k} {n}")).collect();
                let deaths: u32 = runs.iter().map(|r| r.deaths).sum();
                let keys: Vec<String> = ["rome", "byzantium", "ottomans", "milan", "papacy", "venice"].iter().filter(|k| runs[0].eo.contains_key(**k)).map(|k| format!("{k} {}", runs.iter().filter(|r| r.dead.contains_key(*k)).count())).collect();
                info.push(format!("| {sc} | {world} | {} | {} | {} | {deaths} | {} |", labels(m), if over.is_empty() { "—".into() } else { over.join(", ") }, if monos.is_empty() { "—".into() } else { monos.join(", ") }, keys.join(", ")));
            }
            let pairs: Vec<(&str, &str)> = match sc {
                "rome_375" => vec![("sassanids", "armenia"), ("huns", "ostrogoths")],
                "constantinople_1430" => vec![("ottomans", "serbia"), ("ottomans", "byzantium")],
                _ => vec![("florence", "siena"), ("naples", "papacy")],
            };
            for (lord, v) in pairs {
                let c = |runs: &[Run]| runs.iter().filter(|r| r.vassal.contains_key(&(v.to_string(), lord.to_string()))).count();
                pair_rows.push(format!("| {lord} → {v} | {sc} {world} | {} | {} |", c(base), c(var)));
            }
        }
    }
    println!("## 1. Ц10's measure: (1) zombies < 1 %, (2) below a quarter of P₀ < 10 % of living actor-ticks, (3) Rome on tick 100 ≤ its share of P₀\n");
    println!("| scenario | world | model | zombies | below P₀ / 4 | Rome on tick 100 | passes |");
    println!("|---|---|---|---|---|---|---|");
    for r in c10_rows { println!("{r}"); }
    for m in [3usize, 5, 6] { let x = c10_ok[&m]; println!("\n{}: {} of {} worlds pass", labels(m), x.0, x.1); }
    println!("\n## 2. Stop rule: base against (а) on the same seeds\n");
    println!("| scenario | world | base | (а) |");
    println!("|---|---|---|---|");
    for r in stop_rows { println!("{r}"); }
    for m in [3usize, 5] {
        let ((n, w, sp, spq), (nb, wb)) = c4p[&m];
        let pa = w as f64 / n.max(1) as f64;
        let promise = sp / n.max(1) as f64;
        let se = spq.sqrt() / n.max(1) as f64;
        let pb = wb as f64 / nb.max(1) as f64;
        let se_d = (pa * (1.0 - pa) / n.max(1) as f64 + pb * (1.0 - pb) / nb.max(1) as f64).sqrt();
        let (o1, o2) = (pa >= promise - 2.0 * se, pa - pb >= 2.0 * se_d);
        if m == 5 {
            let ((bn, bw, bsp, bspq), (bnb, bwb)) = c4p[&3];
            let bpa = bw as f64 / bn.max(1) as f64;
            let bpb = bwb as f64 / bnb.max(1) as f64;
            if bpa >= bsp / bn.max(1) as f64 - 2.0 * bspq.sqrt() / bn.max(1) as f64 && !o1 { stops.push("Ц4 (1)".into()); }
            if bpa - bpb >= 2.0 * (bpa * (1.0 - bpa) / bn.max(1) as f64 + bpb * (1.0 - bpb) / bnb.max(1) as f64).sqrt() && !o2 { stops.push("Ц4 (2)".into()); }
        }
        println!("- {}: Ц4 (1) {:.1} % vs {:.1} % — {}; Ц4 (2) {:+.1} vs 2 SE {:.1} — {} ({n} battles)", labels(m), 100.0 * pa, 100.0 * (promise - 2.0 * se), if o1 { "yes" } else { "no" }, 100.0 * (pa - pb), 200.0 * se_d, if o2 { "yes" } else { "no" });
    }
    println!("\n### Ц7 items 1–3 (Ц9 is item 3)\n");
    println!("| scenario | world | model | | passes |");
    println!("|---|---|---|---|---|");
    for r in c7_rows { println!("{r}"); }
    println!("\n### Ц3 (constantinople): d = m₄₀ − 1.25 m₁₀ (t)\n");
    println!("| world | model | d | passes |");
    println!("|---|---|---|---|");
    for r in c3_rows { println!("{r}"); }
    println!("\nStops (base passes, (а) does not): {}", if stops.is_empty() { "none".into() } else { stops.join("; ") });
    println!("\n## 3. For information: Ц2 (debt > 20 % of non-zombie ticks; treasury monotone to 150 in half the games), deaths\n");
    println!("| scenario | world | model | in debt > 20 % | monotone treasury (games) | deaths | key actors dying (games) |");
    println!("|---|---|---|---|---|---|---|");
    for r in info { println!("{r}"); }
    println!("\n### Pairs of Ц7: games with the submission, base → (а)\n");
    println!("| pair | world | base | (а) |");
    println!("|---|---|---|---|");
    for r in pair_rows { println!("{r}"); }
}

/// Why (а) fails Ц10: Rome on tick 100 (population against its scaled base, eo / T), and per actor
/// in milan (and the rest) the zombie and below-a-quarter ticks with population writes by source.
fn c10_why(first: u64, seeds: u64, ticks: u32) {
    println!("# Ц10 (а): where the measure fails, seeds {first}–{}\n", first + seeds - 1);
    println!("## Rome on tick 100\n");
    println!("| world | population / base p10/50/90 (%) | eo / T p10/50/90 (%) |");
    println!("|---|---|---|");
    for world in worlds("rome_375") {
        let runs: Vec<Run> = (first..first + seeds).map(|s| run("rome_375", world, 5, true, s, ticks)).collect();
        let pb: Vec<f64> = runs.iter().filter_map(|r| r.rome100.map(|(p, b)| 100.0 * p / b)).collect();
        let et: Vec<f64> = runs.iter().filter_map(|r| r.rome_eo100.map(|x| 100.0 * x)).collect();
        println!("| {world} | {} | {} |", q(&pb), q(&et));
    }
    println!("\n## Actors with zombie or below-a-quarter ticks (≥ 5 a game on average): ticks a game, P₀, population writes by source (mean a game, taken / added; |x| ≥ 0.5)\n");
    println!("| scenario | world | actor | P₀ | zombie / below P₀/4 ticks a game | population writes |");
    println!("|---|---|---|---|---|---|");
    for sc in SCENARIOS {
        let scn = engine13::scenarios::registry::load_by_id(sc).unwrap();
        for world in worlds(sc).iter().take(1) {
            let runs: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, 5, true, s, ticks)).collect();
            let mut c: BTreeMap<String, [u64; 5]> = BTreeMap::new();
            for r in &runs { for (k, v) in &r.popc { let e = c.entry(k.clone()).or_default(); for i in 0..5 { e[i] += v[i]; } } }
            for (k, v) in c.iter().filter(|(_, v)| v[2] >= 5 * seeds) {
                let mut src: BTreeMap<String, (f64, f64)> = BTreeMap::new();
                for r in &runs { for ((a, s2), x) in &r.pop_src { if a == k { let e = src.entry(s2.clone()).or_default(); e.0 += x.0 / seeds as f64; e.1 += x.1 / seeds as f64; } } }
                let mut sv: Vec<(String, (f64, f64))> = src.into_iter().filter(|x| x.1 .0.abs() >= 0.5 || x.1 .1 >= 0.5).collect();
                sv.sort_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap());
                let p0 = engine13::engine::metric_base(&scn, k, "population").unwrap_or(f64::NAN);
                println!("| {sc} | {world} | {k} | {p0:.0} | {:.0} / {:.0} | {} |", v[1] as f64 / seeds as f64, v[2] as f64 / seeds as f64,
                    sv.iter().map(|(s2, (m, p))| format!("{s2} {m:.1}/+{p:.1}")).collect::<Vec<_>>().join("; "));
            }
        }
    }
}

/// The owner's shares of the three population events under the pull (Ц10): `flood` 1 %, `famine` 5 %,
/// `plague` 10 %.
const EVENT_SHARES: [(&str, f64); 3] = [("flood", 0.01), ("famine", 0.05), ("plague", 0.10)];

/// Ц10, before building relative events (§9.7): on the protocols of (а) (r = 0.01, the events still
/// absolute — their firing does not depend on the size of the blow), each event's rate p per living
/// actor-tick and P*/N = r / (r + p × f); per actor P*/P₀ = median(eo / T) × r / (r + Σ p_actor × f),
/// and the arithmetic of Ц10's conditions 1 and 2 from it.
fn c10_events(first: u64, seeds: u64, ticks: u32) {
    const R: f64 = 0.01;
    println!("# Ц10 — relative population events, arithmetic before the build, seeds {first}–{}\n", first + seeds - 1);
    let mut ev_rows = Vec::new();
    let mut actor_rows = Vec::new();
    let mut verdict_rows = Vec::new();
    for sc in SCENARIOS {
        let scn = engine13::scenarios::registry::load_by_id(sc).unwrap();
        for world in worlds(sc) {
            let runs: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, 5, true, s, ticks)).collect();
            let mut living: BTreeMap<String, u64> = BTreeMap::new();
            let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
            let mut fires: BTreeMap<(String, String), u32> = BTreeMap::new();
            for r in &runs {
                for (k, v) in &r.popc { *living.entry(k.clone()).or_default() += v[0]; }
                for (k, v) in &r.eo_rel { eo.entry(k.clone()).or_default().extend(v); }
                for (k, v) in &r.ev { *fires.entry(k.clone()).or_default() += v; }
            }
            let total: u64 = living.values().sum();
            for (e, f) in EVENT_SHARES {
                let n: u32 = fires.iter().filter(|((x, _), _)| x == e).map(|(_, v)| v).sum();
                let p = n as f64 / total.max(1) as f64;
                ev_rows.push(format!("| {sc} | {world} | {e} | {:.0} % | {p:.4} | {:.0} % |", 100.0 * f, 100.0 * R / (R + p * f)));
            }
            // per actor
            let (mut low_ticks, mut zombie_ticks) = (0u64, 0u64);
            let mut lows = Vec::new();
            for (id, lt) in &living {
                let Some(p0) = engine13::engine::metric_base(&scn, id, "population") else { continue };
                let load: f64 = EVENT_SHARES.iter().map(|(e, f)| fires.get(&(e.to_string(), id.clone())).copied().unwrap_or(0) as f64 / (*lt).max(1) as f64 * f).sum();
                let rel = eo.get(id).map_or(1.0, |v| pct(v, 0.5));
                let ps = rel * R / (R + load);
                if ps < 0.25 { low_ticks += lt; lows.push(id.clone()); }
                if ps * p0 <= 1.0 { zombie_ticks += lt; }
                if sc == "milan_1477" || ["saxons", "mantua", "urbino"].contains(&id.as_str()) {
                    actor_rows.push(format!("| {sc} | {world} | {id} | {p0:.0} | {:.0} % | {load:.4} | {:.0} % | {:.1} | {} |", 100.0 * rel, 100.0 * ps, ps * p0,
                        if ps * p0 <= 1.0 { "**≤ 1**" } else if ps < 0.25 { "**< P₀/4**" } else { "ok" }));
                }
            }
            let (z, l) = (share(zombie_ticks, total), share(low_ticks, total));
            verdict_rows.push(format!("| {sc} | {world} | {z:.1} % | {l:.1} % ({}) | {} |", if lows.is_empty() { "—".into() } else { lows.join(", ") }, if z < 1.0 && l < 10.0 { "yes" } else { "**no**" }));
        }
    }
    println!("## Events: rate per living actor-tick and the equilibrium each alone allows\n");
    println!("| scenario | world | event | share f | p | P*/N = r / (r + p f) |");
    println!("|---|---|---|---|---|---|");
    for r in ev_rows { println!("{r}"); }
    println!("\n## Actors (milan, and saxons): median eo / T, the events' load Σ p f, P*/P₀, P*\n");
    println!("| scenario | world | actor | P₀ | eo / T | Σ p f | P*/P₀ | P* | |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in actor_rows { println!("{r}"); }
    println!("\n## Arithmetic of Ц10's conditions 1–2 (an actor whose P* is ≤ 1 or below P₀/4 counts all its living ticks)\n");
    println!("| scenario | world | (1) zombies | (2) below P₀/4 (who) | both pass |");
    println!("|---|---|---|---|---|");
    for r in verdict_rows { println!("{r}"); }
}

/// Which actors hold eo < T / 4 for 20 % of their life or more under Ц10 (а), and why: their
/// economic_output writes by source (mean a game, |x| ≥ 5), the first world of each scenario.
fn eo_low(first: u64, seeds: u64, ticks: u32) {
    println!("# eo < T / 4 under Ц10 (а), seeds {first}–{}\n", first + seeds - 1);
    println!("| scenario | world | actor | share of life with eo < T/4 | eo / T p10/50/90 (%) | economic_output writes by source (mean a game) |");
    println!("|---|---|---|---|---|---|");
    for sc in SCENARIOS {
        let world = worlds(sc)[0];
        let runs: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, 5, true, s, ticks)).collect();
        let mut c: BTreeMap<String, [u64; 5]> = BTreeMap::new();
        let mut rel: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for r in &runs {
            for (k, v) in &r.popc { let e = c.entry(k.clone()).or_default(); for i in 0..5 { e[i] += v[i]; } }
            for (k, v) in &r.eo_rel { rel.entry(k.clone()).or_default().extend(v.iter().map(|x| 100.0 * x)); }
        }
        for (k, v) in c.iter().filter(|(_, v)| share(v[3], v[0]) >= 20.0) {
            let mut src: BTreeMap<String, f64> = BTreeMap::new();
            for r in &runs { for ((a, s2), x) in &r.eo_src { if a == k { *src.entry(s2.clone()).or_default() += x / seeds as f64; } } }
            let mut sv: Vec<(String, f64)> = src.into_iter().filter(|x| x.1.abs() >= 5.0).collect();
            sv.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            println!("| {sc} | {world} | {k} | {:.0} % | {} | {} |", share(v[3], v[0]), q(rel.get(k).map_or(&[][..], |x| &x[..])), sv.iter().map(|(s2, x)| format!("{s2} {x:+.0}")).collect::<Vec<_>>().join("; "));
        }
    }
}

/// The fingerprint of v2 exactly as in the content (nothing overridden), for the Ц10 content check.
fn run_content(sc: &str, world: &str, seed: u64, ticks: u32) -> u64 {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    st.current_scenario.as_mut().unwrap().features.economy_v2 = true;
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut fp = std::collections::hash_map::DefaultHasher::new();
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
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            let mut ms: Vec<(&String, &f64)> = ws.actors[id].metrics.iter().collect();
            ms.sort_by(|x, y| x.0.cmp(y.0));
            for (k, v) in ms { std::hash::Hash::hash(&(id, k, v.to_bits()), &mut fp); }
        }
    }
    std::hash::Hasher::finish(&fp)
}

/// Ц2 (owner's rule, one build, one run): base (v2 as in the content) against the army paid out of
/// the treasury. The only gate — the stop rule: Ц1, Ц3–Ц10 against base on the same seeds (§9.6).
/// For information: treasury / income, desertion, the peoples' armies and Ц7's submissions, §9.2,
/// deaths paired.
fn c2_run(first: u64, seeds: u64, ticks: u32) {
    const MEASURES: [&str; 9] = ["Ц1 actors", "Ц1 tiers", "Ц1 spread", "Ц5 actors", "Ц5 spread", "Ц6 ceiling", "Ц6 corr", "Ц8 extremes", "Ц8 spread"];
    println!("# Ц2 — the army paid out of the treasury, seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    let lab = |m: usize| if m == 8 { "base" } else { "Ц2" };
    let mut stops: Vec<String> = Vec::new();
    let mut stop_rows = Vec::new();
    let mut gate_rows = Vec::new();
    let mut c4p: BTreeMap<usize, C4Pool> = BTreeMap::new();
    let mut info = Vec::new();
    let mut pair_rows = Vec::new();
    let mut army_rows = Vec::new();
    for sc in SCENARIOS {
        for world in worlds(sc) {
            let all: BTreeMap<usize, Vec<Run>> = [8usize, 7].iter().map(|m| (*m, (first..first + seeds).map(|s| run(sc, world, *m, true, s, ticks)).collect())).collect();
            for m in [8usize, 7] {
                let nq: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, m, false, s, ticks)).collect();
                let e = c4p.entry(m).or_default();
                for rr in &all[&m] { let x = c4(&rr.battles); e.0 .0 += x.0; e.0 .1 += x.1; e.0 .2 += x.2; e.0 .3 += x.3; }
                for rr in &nq { let x = c4(&rr.battles); e.1 .0 += x.0; e.1 .1 += x.1; }
            }
            let (base, var) = (&all[&8], &all[&7]);
            let (ok_b, det_b) = world_measures(sc, base);
            let (ok_v, det_v) = world_measures(sc, var);
            for (i, name) in MEASURES.iter().enumerate() { if ok_b[i] && !ok_v[i] { stops.push(format!("{sc} {world}: {name}")); } }
            stop_rows.push(format!("| {sc} | {world} | {det_b} | {det_v} |"));
            // Ц7 items 1–3 (Ц9 = item 3), Ц3, Ц10
            let vassal_of = |rr: &Run, v: &str, lord: Option<&str>| rr.vassal.keys().any(|(a, b)| a == v && lord.is_none_or(|l| l == b));
            let mut cells = Vec::new();
            for (m, runs) in [(8usize, base), (7, var)] {
                let c7 = match sc {
                    "rome_375" => { let pg = runs.iter().map(|rr| PEOPLES.iter().filter(|p| vassal_of(rr, p, Some("rome"))).count()).sum::<usize>() as f64 / seeds as f64; (format!("five peoples {pg:.1}"), pg >= 3.0) }
                    "constantinople_1430" => { let ft: Vec<f64> = runs.iter().filter_map(|rr| rr.dead.get("byzantium").map(|t| *t as f64)).collect(); (format!("Byzantium falls {} @ {}", ft.len(), q(&ft)), *world != "none" || (ft.len() >= 20 && (40.0..=59.0).contains(&pct(&ft, 0.5)))) }
                    _ => { let hit = |id: &str| runs.iter().filter(|rr| rr.dead.contains_key(id) || vassal_of(rr, id, None)).count(); (format!("Milan {}, papacy {}, Venice {}", hit("milan"), hit("papacy"), hit("venice")), hit("milan") <= 1 && hit("papacy") <= 3 && hit("venice") <= 3) }
                };
                let c3 = if sc == "constantinople_1430" {
                    let pairs: Vec<(f64, f64)> = runs.iter().filter_map(|r| Some((window(&r.ott, 10, 20)?, window(&r.ott, 40, 50)?))).collect();
                    let d: Vec<f64> = pairs.iter().map(|(a, b)| b - 1.25 * a).collect();
                    let n = d.len() as f64;
                    let mean = d.iter().sum::<f64>() / n.max(1.0);
                    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
                    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
                    Some((format!("{mean:+.1} (t {t:+.1})"), mean > 0.0 && t >= 2.0))
                } else { None };
                let (z, low, _) = c10_shares(runs);
                let rome_ok = sc != "rome_375" || runs.iter().filter_map(|r| r.rome100).all(|(_, b)| (b - 3600.0).abs() < 1e-6);
                let c10 = (format!("zombies {z:.2} %, below P₀/4 {low:.1} %"), z < 1.0 && low < 10.0 && rome_ok);
                cells.push((m, c7.clone(), c3.clone(), c10.clone()));
            }
            let (b, v) = (&cells[0], &cells[1]);
            if b.1 .1 && !v.1 .1 { stops.push(format!("{sc} {world}: Ц7/Ц9")); }
            if let (Some(bc), Some(vc)) = (&b.2, &v.2) { if bc.1 && !vc.1 { stops.push(format!("{world}: Ц3")); } }
            if b.3 .1 && !v.3 .1 { stops.push(format!("{sc} {world}: Ц10")); }
            gate_rows.push(format!("| {sc} | {world} | {} → {} | {} | {} → {} |", b.1 .0, v.1 .0, match (&b.2, &v.2) { (Some(x), Some(y)) => format!("{} → {}", x.0, y.0), _ => "—".into() }, b.3 .0, v.3 .0));
            // information
            for (m, runs) in [(8usize, base), (7, var)] {
                let mut ratios: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for r in runs.iter() { for (k, v) in &r.tr_inc { ratios.entry(k.clone()).or_default().extend(v); } }
                let mut top: Vec<(String, f64)> = ratios.iter().map(|(k, v)| (k.clone(), pct(v, 0.5))).collect();
                top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                let living: u64 = runs.iter().map(|r| r.popc.values().map(|v| v[0]).sum::<u64>()).sum();
                let des: u64 = runs.iter().map(|r| r.desert.values().map(|x| *x as u64).sum::<u64>()).sum();
                let mut desa: BTreeMap<String, u64> = BTreeMap::new();
                for r in runs.iter() { for (k, v) in &r.desert { *desa.entry(k.clone()).or_default() += *v as u64; } }
                let mut lv: BTreeMap<String, u64> = BTreeMap::new();
                for r in runs.iter() { for (k, v) in &r.popc { *lv.entry(k.clone()).or_default() += v[0]; } }
                let persistent: Vec<String> = desa.iter().filter(|(k, d)| **d * 5 > lv.get(*k).copied().unwrap_or(1)).map(|(k, d)| format!("{k} {:.0} %", 100.0 * *d as f64 / lv.get(k).copied().unwrap_or(1) as f64)).collect();
                let deaths: u32 = runs.iter().map(|r| r.deaths).sum();
                let keys: Vec<String> = ["rome", "byzantium", "ottomans", "milan", "papacy", "venice"].iter().filter(|k| runs[0].eo.contains_key(**k)).map(|k| format!("{k} {}", runs.iter().filter(|r| r.dead.contains_key(*k)).count())).collect();
                info.push(format!("| {sc} | {world} | {} | {} | {:.1} % | {} | {deaths} | {} |", lab(m), top.iter().take(3).map(|(k, x)| format!("{k} {x:.0}")).collect::<Vec<_>>().join(", "),
                    100.0 * des as f64 / living.max(1) as f64, if persistent.is_empty() { "—".into() } else { persistent.join(", ") }, keys.join(", ")));
            }
            if sc == "rome_375" {
                let at = |runs: &[Run], t: u32| { let x: Vec<f64> = runs.iter().filter_map(|r| r.peoples.get(&t).copied()).collect(); format!("{:.0}", pct(&x, 0.5)) };
                info.push(format!("| {sc} | {world} | five peoples' armies (sum) on ticks 15 / 45 / 100 | base {} / {} / {} | Ц2 {} / {} / {} | | | |", at(base, 15), at(base, 45), at(base, 100), at(var, 15), at(var, 45), at(var, 100)));
            }
            let pairs: Vec<(&str, &str)> = match sc {
                "rome_375" => vec![("rome", "alamanni"), ("rome", "vandals"), ("rome", "visigoths"), ("rome", "burgundians"), ("rome", "franks"), ("sassanids", "armenia"), ("huns", "ostrogoths")],
                "constantinople_1430" => vec![("ottomans", "serbia"), ("ottomans", "byzantium")],
                _ => vec![("florence", "siena"), ("naples", "papacy")],
            };
            for (lord, v) in pairs {
                let c = |runs: &[Run]| { let t: Vec<f64> = runs.iter().filter_map(|r| r.vassal.get(&(v.to_string(), lord.to_string())).map(|x| *x as f64)).collect(); format!("{} @ {}", t.len(), q(&t)) };
                pair_rows.push(format!("| {lord} → {v} | {sc} {world} | {} | {} |", c(base), c(var)));
            }
            let dpair = |k: &str| { let a: Vec<f64> = base.iter().map(|r| r.dead.contains_key(k) as u8 as f64).collect(); let b: Vec<f64> = var.iter().map(|r| r.dead.contains_key(k) as u8 as f64).collect(); let d: Vec<f64> = a.iter().zip(&b).map(|(x, y)| y - x).collect(); let n = d.len() as f64; let mean = d.iter().sum::<f64>() / n; let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt(); if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 } };
            let alld: Vec<f64> = base.iter().zip(var.iter()).map(|(a, b)| b.deaths as f64 - a.deaths as f64).collect();
            let mean = alld.iter().sum::<f64>() / alld.len() as f64;
            army_rows.push(format!("| {sc} | {world} | {:+.2} | {} |", mean, ["rome", "byzantium", "ottomans", "milan"].iter().filter(|k| base[0].eo.contains_key(**k)).map(|k| format!("{k} t {:+.1}", dpair(k))).collect::<Vec<_>>().join(", ")));
        }
    }
    println!("## 1. The gate — the stop rule, base against Ц2 on the same seeds\n");
    println!("| scenario | world | base | Ц2 |");
    println!("|---|---|---|---|");
    for r in stop_rows { println!("{r}"); }
    for m in [8usize, 7] {
        let ((n, w, sp, spq), (nb, wb)) = c4p[&m];
        let pa = w as f64 / n.max(1) as f64;
        let promise = sp / n.max(1) as f64;
        let se = spq.sqrt() / n.max(1) as f64;
        let pb = wb as f64 / nb.max(1) as f64;
        let se_d = (pa * (1.0 - pa) / n.max(1) as f64 + pb * (1.0 - pb) / nb.max(1) as f64).sqrt();
        let (o1, o2) = (pa >= promise - 2.0 * se, pa - pb >= 2.0 * se_d);
        if m == 7 {
            let ((bn, bw, bsp, bspq), (bnb, bwb)) = c4p[&8];
            let bpa = bw as f64 / bn.max(1) as f64;
            let bpb = bwb as f64 / bnb.max(1) as f64;
            if bpa >= bsp / bn.max(1) as f64 - 2.0 * bspq.sqrt() / bn.max(1) as f64 && !o1 { stops.push("Ц4 (1)".into()); }
            if bpa - bpb >= 2.0 * (bpa * (1.0 - bpa) / bn.max(1) as f64 + bpb * (1.0 - bpb) / bnb.max(1) as f64).sqrt() && !o2 { stops.push("Ц4 (2)".into()); }
        }
        println!("- {}: Ц4 (1) {:.1} % vs {:.1} % — {}; Ц4 (2) {:+.1} vs 2 SE {:.1} — {} ({n} battles)", lab(m), 100.0 * pa, 100.0 * (promise - 2.0 * se), if o1 { "yes" } else { "no" }, 100.0 * (pa - pb), 200.0 * se_d, if o2 { "yes" } else { "no" });
    }
    println!("\n### Ц7 items 1–3 (Ц9 = item 3), Ц3, Ц10: base → Ц2\n");
    println!("| scenario | world | Ц7 | Ц3 d (t) | Ц10 |");
    println!("|---|---|---|---|---|");
    for r in gate_rows { println!("{r}"); }
    println!("\nStops (base passes, Ц2 does not): {}", if stops.is_empty() { "none".into() } else { stops.join("; ") });
    println!("\n## 2. For information: treasury / income on ticks 250–299 (median, top three), desertion, deaths\n");
    println!("| scenario | world | model | treasury / income, top | desertion (living actor-ticks) | desertion > 20 % | deaths | key actors dying (games) |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in info { println!("{r}"); }
    println!("\n### Deaths paired (Ц2 − base, mean a game) and key actors (paired t)\n");
    println!("| scenario | world | deaths | key |");
    println!("|---|---|---|---|");
    for r in army_rows { println!("{r}"); }
    println!("\n### Submissions (Ц7): games @ tick p10/50/90, base → Ц2\n");
    println!("| pair | world | base | Ц2 |");
    println!("|---|---|---|---|");
    for r in pair_rows { println!("{r}"); }
}
