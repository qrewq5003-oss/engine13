//! Economy project, Ц7: death and submission by war (docs/economy_project_brief.md §9). Built with
//! `--features census`. Every world, 30 seeds × 300 ticks; the seeds are an argument — 0 for
//! choosing K₂ (0–29), 100 for the held-out check (100–129).
//!
//! The model (v2, `economy_v2_conquest_k2`, on the battle outcome): three battles lost in a row to
//! one winner at three times its strength make the loser its vassal; overlord and vassal do not
//! fight; a milestone's `begins_conquest` (constantinople: `final_assault`, tick 46) breaks the
//! pair's bond, forbids it again, and then K₂ such losses kill the target by the conquest path.
//! Models: base (v2 as in the content, `12a757f`), K₂ = 1, K₂ = 2; and each also with strength
//! without quality, for Ц4's condition 2.
//!
//! Ц7's measure (owner's, revised before the build): (1) rome — on average ≥ 3 of the five
//! peoples a game become Rome's vassals, in every world; (2) constantinople none — Byzantium falls
//! in ≥ 20 of 30, median tick 40–59; (3) milan — Milan dies or submits in ≤ 1 of 30 in every world,
//! papacy and Venice each ≤ 3 of 30; (4) every war death and submission recorded with its winner.
//! Stop rule (§9.6): Ц1, Ц4, Ц5, Ц6 against base on the same seeds. For information: B46 «held,
//! then fell», constantinople wins and A10, vassals and their ticks, revolts, §9.2, deaths paired.
//!
//! Usage: cargo run --release --features census --bin c7_probe -- [first_seed] [seeds] [ticks] [c1]
//! (`c1`: only Ц1's measure per world and the failing actors, base against K₂ = 1.)
//! (`deter`: the balance-of-power arithmetic over the protocols of PR #240 — see `deterrence`.)
//! (`papacy`: milan, K₂ = 1, with and without the Ц8 cohesion pull — Naples and the papacy side by side.)
//! (`content`: v2 as the content stands against K₂ = 1 set here, bit for bit.)
//! (`src`: the Ц1 violators in rome none and milan none — economic_output at the floor, its writes by
//! source, mean army and cohesion — base against K₂ = 1, to see what differs.)

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

const PEOPLES: [&str; 5] = ["alamanni", "vandals", "visigoths", "burgundians", "franks"];

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
    calm: BTreeMap<String, (u64, u64, u64)>,
    living: u64,
    ceiling_no_threat: u64,
    corr: [f64; 6],
    declines: Vec<(bool, u8)>,
    /// (vassal, overlord) -> (first tick, ticks bound)
    vassal: BTreeMap<(String, String), (u32, u32)>,
    revolts: u32,
    dead: BTreeMap<String, u32>,
    conquered_by: BTreeMap<String, String>,
    /// (violator, source) -> economic_output written
    eo_sources: BTreeMap<(String, String), f64>,
    /// (violator, source) -> cohesion written
    coh_sources: BTreeMap<(String, String), f64>,
    /// violators: pressure per living tick
    pressure: BTreeMap<String, Vec<f64>>,
    /// violators: army and cohesion per living tick
    army: BTreeMap<String, Vec<f64>>,
    cohesion: BTreeMap<String, Vec<f64>>,
    unpaid_tribute: f64,
    /// treasury of rome and ottomans on ticks 150 and 299
    lord_treasury: BTreeMap<(String, u32), f64>,
    fingerprint: u64,
    deaths: u32,
    win: Option<u32>,
    split40: bool,
    outcomes: Vec<String>,
    held_then_fell: bool,
    stab: bool,
    deep: bool,
}

/// None = base; Some((k2, with quality, tribute stops at the treasury floor))
type Model = Option<(u32, bool, bool)>;

fn label(m: Model) -> String {
    match m {
        None => "base".into(),
        Some((k, q, f)) => format!("K₂ = {k}{}{}", if f { "" } else { ", tribute as before" }, if q { "" } else { ", no quality" }),
    }
}

/// The new violators of Ц1 under PR #236, whose economic_output writes are decomposed.
const VIOLATORS: [&str; 4] = ["burgundians", "alamanni", "ostrogoths", "sicily"];

fn run(sc: &str, world: &str, m: Model, seed: u64, ticks: u32, base_no_quality: bool) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        assert!(s.economy_v2_combat_outcome, "the content as at 12a757f has the battle outcome");
        // k2 = u32::MAX: the content as it stands (the content check)
        if m.is_none_or(|x| x.0 != u32::MAX) { s.economy_v2_conquest_k2 = m.map(|x| x.0); }
    }
    census::set_combat_quality(match m { None => !base_no_quality, Some((_, q, _)) => q });
    census::set_tribute_floor(m.is_none_or(|x| x.2));
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_bonds: BTreeSet<(String, String)> = BTreeSet::new();
    type Open = (String, u32, f64, f64, bool, f64, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    let _ = census::take_battles();
    let _ = census::take_writes();
    let _ = census::take_floor_losses();
    let mut fp = std::collections::hash_map::DefaultHasher::new();
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
        for w in census::take_writes() {
            if (w.metric == "economic_output" || w.metric == "cohesion") && VIOLATORS.contains(&w.actor.as_str()) {
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                let map = if w.metric == "cohesion" { &mut r.coh_sources } else { &mut r.eo_sources };
                *map.entry((w.actor.clone(), src)).or_default() += w.applied;
            }
        }
        for (_, src, x) in census::take_floor_losses() {
            if src == "vassal tribute" { r.unpaid_tribute += x; }
        }
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        if t == 150 || t == ticks - 1 {
            for lord in ["rome", "ottomans"] {
                if let Some(a) = ws.actors.get(lord).filter(|_| !ws.dead_actor_ids.contains(lord)) { r.lord_treasury.insert((lord.to_string(), t), a.get_metric("treasury")); }
            }
        }
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) {
            r.dead.insert(d.clone(), t);
            if d == "byzantium" && ws.milestone_events_fired.iter().any(|mm| mm == "outcome_survived_alone") { r.held_then_fell = true; }
        }
        let bonds: BTreeSet<(String, String)> = ws.vassalages.iter().map(|v| (v.vassal_id.clone(), v.overlord_id.clone())).collect();
        for b in &bonds {
            let e = r.vassal.entry(b.clone()).or_insert((t, 0));
            e.1 += 1;
        }
        for b in prev_bonds.difference(&bonds) {
            if ws.actors.contains_key(&b.0) && ws.actors.contains_key(&b.1) && !ws.dead_actor_ids.contains(&b.0) && !ws.dead_actor_ids.contains(&b.1) && !ws.conquests.contains(&(b.1.clone(), b.0.clone())) {
                r.revolts += 1;
            }
        }
        prev_bonds = bonds;
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
            let mut ms: Vec<(&String, &f64)> = ws.actors[id].metrics.iter().collect();
            ms.sort_by(|x, y| x.0.cmp(y.0));
            for (k, v) in ms { std::hash::Hash::hash(&(id, k, v.to_bits()), &mut fp); }
            if ws.dead_actor_ids.contains(id) { continue; }
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
            if VIOLATORS.contains(&id.as_str()) {
                r.army.entry(id.clone()).or_default().push(a.get_metric("military_size"));
                r.cohesion.entry(id.clone()).or_default().push(a.get_metric("cohesion"));
                r.pressure.entry(id.clone()).or_default().push(a.get_metric("external_pressure"));
            }
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
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|mm| mm == "rome_splits") { r.split40 = true; }
    }
    census::set_combat_quality(true);
    census::set_tribute_floor(true);
    let ws = st.world_state.as_ref().unwrap();
    r.deaths = ws.dead_actors.len() as u32;
    r.conquered_by = ws.conquered_by.clone();
    r.fingerprint = std::hash::Hasher::finish(&fp);
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

/// Per model: worlds passing each world-level measure, pooled proportions
#[derive(Default, Clone)]
struct Pool {
    worlds: usize,
    c1: [usize; 3],
    c5: [usize; 2],
    c6: [usize; 2],
    decline: (u64, u64),
    c4: (u64, u64, f64, f64),
    c4b: (u64, u64),
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let first: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_battles();
    census::enable_writes();
    census::watch_all_metrics(true);
    census::enable_floor_losses();
    if args.get(4).map(String::as_str) == Some("c1") { c1_detail(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("src") { c1_sources(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("papacy") { papacy_pair(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("deter") { deterrence(first, seeds, ticks); return; }
    if args.get(4).map(String::as_str) == Some("content") {
        let (mut same, mut total) = (0, 0);
        for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
            for world in worlds(sc) {
                for seed in first..first + seeds {
                    total += 1;
                    if run(sc, world, Some((u32::MAX, true, true)), seed, ticks, false).fingerprint == run(sc, world, Some((1, true, true)), seed, ticks, false).fingerprint { same += 1; }
                }
            }
        }
        println!("Ц7 content check: v2 as in the content against K₂ = 1 set here, seeds {first}–{}: {same} of {total} runs identical, every actor metric every tick.", first + seeds - 1);
        return;
    }
    println!("# Ц7 — death and submission by war, seeds {first}–{}, {ticks} ticks per world\n", first + seeds - 1);
    let models: [Model; 3] = [None, Some((1, true, false)), Some((1, true, true))];
    let mut eo_rows = Vec::new();
    let mut tribute_rows = Vec::new();
    let mut pools: BTreeMap<String, Pool> = BTreeMap::new();
    let mut c7_rows = Vec::new();
    let mut c7_verdict: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    let mut vassal_rows = Vec::new();
    let mut death_rows = Vec::new();
    let mut info_rows = Vec::new();
    let mut path_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let base: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, None, s, ticks, false)).collect();
            for m in models {
                let fresh: Vec<Run>;
                let runs: &[Run] = if m.is_none() { &base } else { fresh = (first..first + seeds).map(|s| run(sc, world, m, s, ticks, false)).collect(); &fresh };
                let nq: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, m.map(|x| (x.0, false, x.2)), s, ticks, true)).collect();
                let lab = label(m);
                // ---- stop-rule measures
                let p = pools.entry(lab.clone()).or_default();
                p.worlds += 1;
                let frac = |v: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * v.iter().filter(|y| pr(**y)).count() as f64 / v.len().max(1) as f64;
                let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                let mut legit: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                let mut calm: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
                let (mut lv, mut cnt) = (0, 0);
                let mut c = [0.0; 6];
                for rr in runs {
                    for (k, v) in &rr.eo { eo.entry(k.clone()).or_default().extend(v); }
                    for (k, v) in &rr.legit { legit.entry(k.clone()).or_default().extend(v); }
                    for (k, v) in &rr.calm { let e = calm.entry(k.clone()).or_default(); e.0 += v.0; e.1 += v.1; e.2 += v.2; }
                    lv += rr.living; cnt += rr.ceiling_no_threat;
                    for (a, b) in c.iter_mut().zip(&rr.corr) { *a += b; }
                    for d in rr.declines.iter().filter(|d| d.1 == 2) { p.decline.0 += 1; p.decline.1 += d.0 as u64; }
                    let x = c4(&rr.battles);
                    p.c4.0 += x.0; p.c4.1 += x.1; p.c4.2 += x.2; p.c4.3 += x.3;
                }
                for rr in &nq { let x = c4(&rr.battles); p.c4b.0 += x.0; p.c4b.1 += x.1; }
                let spread = |m: &BTreeMap<String, Vec<f64>>| { let v: Vec<f64> = m.values().map(|x| pct(x, 0.5)).collect(); v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min) };
                let c1n = eo.values().filter(|x| frac(x, &|y| y >= 99.0) < 20.0 && frac(x, &|y| y <= 1.0) < 20.0).count();
                let tm: Vec<f64> = tiers(sc).iter().map(|t| { let x: Vec<f64> = t.iter().filter_map(|a| eo.get(*a)).flatten().copied().collect(); pct(&x, 0.5) }).collect();
                let c5n = calm.values().filter(|v| v.0 > 0 && share(v.1, v.0) < 20.0 && share(v.2, v.0) < 20.0).count();
                let ncalm = calm.values().filter(|v| v.0 > 0).count();
                let (n, sx, sy, sxx, syy, sxy) = (c[0], c[1], c[2], c[3], c[4], c[5]);
                let corr = (n * sxy - sx * sy) / ((n * sxx - sx * sx).sqrt() * (n * syy - sy * sy).sqrt());
                for (slot, ok) in p.c1.iter_mut().zip([100 * c1n >= 80 * eo.len(), tm.windows(2).all(|w| w[0] > w[1]), spread(&eo) >= 20.0]) { *slot += ok as usize; }
                for (slot, ok) in p.c5.iter_mut().zip([100 * c5n >= 80 * ncalm, spread(&legit) >= 15.0]) { *slot += ok as usize; }
                for (slot, ok) in p.c6.iter_mut().zip([share(cnt, lv) < 30.0, corr >= 0.7]) { *slot += ok as usize; }
                // ---- Ц7 measure
                let vassal_of = |rr: &Run, v: &str, lord: Option<&str>| rr.vassal.keys().any(|(a, b)| a == v && lord.is_none_or(|l| l == b));
                match sc {
                    "rome_375" => {
                        let per_game = runs.iter().map(|rr| PEOPLES.iter().filter(|p| vassal_of(rr, p, Some("rome"))).count()).sum::<usize>() as f64 / seeds as f64;
                        let vis: Vec<f64> = runs.iter().filter_map(|rr| rr.vassal.get(&("visigoths".to_string(), "rome".to_string())).map(|x| x.0 as f64)).collect();
                        c7_rows.push(format!("| {sc} | {world} | {lab} | five peoples Rome's vassals: {per_game:.1} a game; visigoths submit in {} @ {} |", vis.len(), q(&vis)));
                        if m.is_some() { c7_verdict.entry(lab.clone()).or_default().push((format!("(1) rome {world}"), per_game >= 3.0)); }
                    }
                    "constantinople_1430" => {
                        let ft: Vec<f64> = runs.iter().filter_map(|rr| rr.dead.get("byzantium").map(|t| *t as f64)).collect();
                        let sub: Vec<f64> = runs.iter().filter_map(|rr| rr.vassal.get(&("byzantium".to_string(), "ottomans".to_string())).map(|x| x.0 as f64)).collect();
                        let htf = runs.iter().filter(|rr| rr.held_then_fell).count();
                        let at46 = ft.iter().filter(|t| **t == 46.0).count();
                        c7_rows.push(format!("| {sc} | {world} | {lab} | Byzantium falls in {} @ {}, on tick 46 in {at46}; Ottoman vassal in {} @ {}; «held, then fell» (B46) {htf} |", ft.len(), q(&ft), sub.len(), q(&sub)));
                        if m.is_some() && *world == "none" {
                            c7_verdict.entry(lab.clone()).or_default().push(("(2) Byzantium none".into(), ft.len() >= 20 && (40.0..=59.0).contains(&pct(&ft, 0.5))));
                        }
                    }
                    _ => {
                        let hit = |id: &str| runs.iter().filter(|rr| rr.dead.contains_key(id) || vassal_of(rr, id, None)).count();
                        let (mi, pa, ve) = (hit("milan"), hit("papacy"), hit("venice"));
                        c7_rows.push(format!("| {sc} | {world} | {lab} | dies or submits: Milan {mi}, papacy {pa}, Venice {ve} |"));
                        if m.is_some() { c7_verdict.entry(lab.clone()).or_default().push((format!("(3) milan {world}"), mi <= 1 && pa <= 3 && ve <= 3)); }
                    }
                }
                // ---- the Ц1 violators' economic_output by source, unpaid tribute, overlords' treasury
                if m.is_some() {
                    for v in VIOLATORS {
                        let mut src: BTreeMap<String, f64> = BTreeMap::new();
                        for rr in runs { for ((a, k), x) in &rr.eo_sources { if a == v { *src.entry(k.clone()).or_default() += x / seeds as f64; } } }
                        if src.is_empty() { continue; }
                        let mut vv: Vec<(String, f64)> = src.into_iter().filter(|x| x.1.abs() >= 1.0).collect();
                        vv.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                        let eo_floor = runs.iter().flat_map(|rr| rr.eo.get(v).into_iter().flatten()).filter(|x| **x <= 1.0).count() as f64
                            / runs.iter().flat_map(|rr| rr.eo.get(v).into_iter().flatten()).count().max(1) as f64 * 100.0;
                        eo_rows.push(format!("| {sc} | {world} | {v} | {lab} | {eo_floor:.0} % | {} |", vv.iter().map(|(k, x)| format!("{k} {x:+.0}")).collect::<Vec<_>>().join("; ")));
                    }
                    let unpaid = runs.iter().map(|rr| rr.unpaid_tribute).sum::<f64>() / seeds as f64;
                    let lt = |lord: &str, tk: u32| -> Vec<f64> { runs.iter().filter_map(|rr| rr.lord_treasury.get(&(lord.to_string(), tk)).copied()).collect() };
                    tribute_rows.push(format!("| {sc} | {world} | {lab} | {unpaid:.0} | rome {} → {} | ottomans {} → {} |", q(&lt("rome", 150)), q(&lt("rome", ticks - 1)), q(&lt("ottomans", 150)), q(&lt("ottomans", ticks - 1))));
                }
                // ---- (4) protocol and paths
                let war_deaths: usize = runs.iter().map(|rr| rr.dead.keys().filter(|d| rr.conquered_by.contains_key(*d)).count()).sum();
                let unrecorded: usize = runs.iter().map(|rr| rr.conquered_by.keys().filter(|d| !rr.dead.contains_key(*d)).count()).sum();
                let all_deaths: u32 = runs.iter().map(|rr| rr.deaths).sum();
                path_rows.push(format!("| {sc} | {world} | {lab} | {all_deaths} | {war_deaths} | {} | {unrecorded} |", all_deaths as usize - war_deaths));
                // ---- vassals
                if m.is_some() {
                    let mut pairs: BTreeMap<(String, String), (u32, u64)> = BTreeMap::new();
                    for rr in runs { for (k, v) in &rr.vassal { let e = pairs.entry(k.clone()).or_default(); e.0 += 1; e.1 += v.1 as u64; } }
                    let mut v: Vec<((String, String), (u32, u64))> = pairs.into_iter().collect();
                    v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
                    let cells: Vec<String> = v.iter().take(14).map(|((a, b), (g, t))| format!("{a}→{b} {g} ({:.0} ticks)", *t as f64 / *g as f64)).collect();
                    vassal_rows.push(format!("| {sc} | {world} | {lab} | {} | {} |", runs.iter().map(|rr| rr.revolts).sum::<u32>(), if cells.is_empty() { "—".into() } else { cells.join(", ") }));
                }
                // ---- deaths paired, §9.2
                let keys: Vec<String> = ["rome", "byzantium", "ottomans", "milan", "papacy", "venice"].iter().filter(|k| base[0].eo.contains_key(**k)).map(|k| {
                    format!("{k} {}→{} ({})", base.iter().filter(|rr| rr.dead.contains_key(*k)).count(), runs.iter().filter(|rr| rr.dead.contains_key(*k)).count(), paired(&base, runs, |rr| if rr.dead.contains_key(*k) { 1.0 } else { 0.0 }))
                }).collect();
                death_rows.push(format!("| {sc} | {world} | {lab} | {all_deaths} | {} | {} |", paired(&base, runs, |rr| rr.deaths as f64), keys.join("; ")));
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
                let rome = if sc == "rome_375" { format!("Rome dies {}", runs.iter().filter(|rr| rr.dead.contains_key("rome")).count()) } else { "—".into() };
                info_rows.push(format!("| {sc} | {world} | {lab} | {rome} | {wins_s} | {split} | {outc} | {fork} |"));
            }
        }
    }
    println!("## 1. Ц7's measure\n");
    println!("| scenario | world | model | |");
    println!("|---|---|---|---|");
    for r in c7_rows { println!("{r}"); }
    println!();
    println!("| model | (1) rome worlds | (2) Byzantium none | (3) milan worlds | all |");
    println!("|---|---|---|---|---|");
    for m in models.iter().skip(1) {
        let v = &c7_verdict[&label(*m)];
        let cnt = |p: &str| (v.iter().filter(|x| x.0.starts_with(p) && x.1).count(), v.iter().filter(|x| x.0.starts_with(p)).count());
        let (a, an) = cnt("(1)");
        let (b, bn) = cnt("(2)");
        let (c, cn) = cnt("(3)");
        println!("| {} | {a} / {an} | {} | {c} / {cn} | {} |", label(*m), if b == bn { "yes" } else { "no" }, if a == an && b == bn && c == cn { "**yes**" } else { "no" });
    }
    println!("\n### (4) Deaths by path: every war death with its conqueror\n");
    println!("| scenario | world | model | deaths | by conquest (conqueror recorded) | by paths 1–2 | conquered but not dead |");
    println!("|---|---|---|---|---|---|---|");
    for r in path_rows { println!("{r}"); }
    println!("\n## 2. Stop rule against base on the same seeds (§9.6)\n");
    println!("| model | Ц1 actors / tiers / spread | Ц5 actors / spread | Ц6 ceiling without a threat / corr | Ц6 decline, corrected | Ц4 (1): won vs promise − 2 SE | Ц4 (2): (a) − (b) vs 2 SE |");
    println!("|---|---|---|---|---|---|---|");
    for m in models {
        let p = &pools[&label(m)];
        let n = p.worlds;
        let pa = p.c4.1 as f64 / p.c4.0.max(1) as f64;
        let promise = p.c4.2 / p.c4.0.max(1) as f64;
        let se = p.c4.3.sqrt() / p.c4.0.max(1) as f64;
        let pb = p.c4b.1 as f64 / p.c4b.0.max(1) as f64;
        let se_d = (pa * (1.0 - pa) / p.c4.0.max(1) as f64 + pb * (1.0 - pb) / p.c4b.0.max(1) as f64).sqrt();
        let dec = share(p.decline.1, p.decline.0);
        println!("| {} | {} / {} / {} of {n} | {} / {} of {n} | {} / {} of {n} | {dec:.1} % of {} | {:.1} % vs {:.1} % {} | {:+.1} vs {:.1} {} |", label(m), p.c1[0], p.c1[1], p.c1[2], p.c5[0], p.c5[1], p.c6[0], p.c6[1], p.decline.0,
            100.0 * pa, 100.0 * (promise - 2.0 * se), if pa >= promise - 2.0 * se { "yes" } else { "**no**" }, 100.0 * (pa - pb), 200.0 * se_d, if pa - pb >= 2.0 * se_d { "yes" } else { "**no**" });
    }
    println!("\n## 2a. The Ц1 violators of PR #236: share of life at the economic_output floor, and its writes by source (mean per game; |x| ≥ 1)\n");
    println!("| scenario | world | actor | model | at the floor | writes by source |");
    println!("|---|---|---|---|---|---|");
    for r in eo_rows { println!("{r}"); }
    println!("\n## 2b. Tribute unpaid at the floor (mean per game), and the overlords' treasury p10/50/90 on tick 150 → 299\n");
    println!("| scenario | world | model | unpaid tribute | rome | ottomans |");
    println!("|---|---|---|---|---|---|");
    for r in tribute_rows { println!("{r}"); }
    println!("\n## 3. Vassals: revolts, and pairs (games, mean ticks bound)\n");
    println!("| scenario | world | model | revolts | vassal → overlord |");
    println!("|---|---|---|---|---|");
    for r in vassal_rows { println!("{r}"); }
    println!("\n## 4. Deaths paired by seed against base\n");
    println!("| scenario | world | model | deaths | per seed | key actors |");
    println!("|---|---|---|---|---|---|");
    for r in death_rows { println!("{r}"); }
    println!("\n## 5. §9.2 (no gate)\n");
    println!("| scenario | world | model | Rome | wins | split on 40 | outcomes | regency stab / deep |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in info_rows { println!("{r}"); }
}

/// Ц1 per world, base against K₂ = 1: actors passing (eo at the ceiling and at the floor each
/// under 20 % of living ticks), the threshold (80 %), and the failing actors with their shares.
fn c1_detail(first: u64, seeds: u64, ticks: u32) {
    println!("# Ц1 per world, seeds {first}–{}: base against K₂ = 1\n", first + seeds - 1);
    println!("| scenario | world | model | actors passing / all (need ≥ 80 %) | failing: ceiling / floor share |");
    println!("|---|---|---|---|---|");
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            for m in [None, Some((1, true, true))] {
                let runs: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, m, s, ticks, false)).collect();
                let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for rr in &runs { for (k, v) in &rr.eo { eo.entry(k.clone()).or_default().extend(v); } }
                let frac = |v: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * v.iter().filter(|y| pr(**y)).count() as f64 / v.len().max(1) as f64;
                let failing: Vec<String> = eo.iter().filter(|(_, x)| frac(x, &|y| y >= 99.0) >= 20.0 || frac(x, &|y| y <= 1.0) >= 20.0)
                    .map(|(k, x)| format!("{k} ({:.0} / {:.0} %)", frac(x, &|y| y >= 99.0), frac(x, &|y| y <= 1.0))).collect();
                let pass = eo.len() - failing.len();
                println!("| {sc} | {world} | {} | {pass} / {} ({:.0} %){} | {} |", label(m), eo.len(), 100.0 * pass as f64 / eo.len().max(1) as f64,
                    if 100 * pass >= 80 * eo.len() { "" } else { " **fails**" }, if failing.is_empty() { "—".into() } else { failing.join(", ") });
            }
        }
    }
}

/// The Ц1 violators, base against K₂ = 1, in the two worlds where Ц1 failed: share of life with
/// economic_output at the floor, its writes by source, and the mean army and cohesion.
fn c1_sources(first: u64, seeds: u64, ticks: u32) {
    println!("# The Ц1 violators: base against K₂ = 1, seeds {first}–{}\n", first + seeds - 1);
    println!("| world | actor | model | eo at the floor | mean army | mean cohesion | mean pressure | vassal ticks (mean) | economic_output writes by source (mean per game, |x| ≥ 2) | cohesion writes by source (|x| ≥ 20) |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (sc, world, actors) in [("rome_375", "none", &["burgundians", "alamanni", "ostrogoths"][..]), ("milan_1477", "none", &["sicily"][..])] {
        let base: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, None, s, ticks, false)).collect();
        let k1: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, Some((1, true, true)), s, ticks, false)).collect();
        for a in actors {
            for (lab, runs) in [("base", &base), ("K₂ = 1", &k1)] {
                let eo: Vec<f64> = runs.iter().flat_map(|rr| rr.eo.get(*a).into_iter().flatten().copied()).collect();
                let floor = 100.0 * eo.iter().filter(|x| **x <= 1.0).count() as f64 / eo.len().max(1) as f64;
                let army: Vec<f64> = runs.iter().flat_map(|rr| rr.army.get(*a).into_iter().flatten().copied()).collect();
                let coh: Vec<f64> = runs.iter().flat_map(|rr| rr.cohesion.get(*a).into_iter().flatten().copied()).collect();
                let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
                let vt = runs.iter().map(|rr| rr.vassal.iter().filter(|((v, _), _)| v == *a).map(|(_, x)| x.1 as f64).sum::<f64>()).sum::<f64>() / seeds as f64;
                let mut src: BTreeMap<String, f64> = BTreeMap::new();
                for rr in runs.iter() { for ((x, k), y) in &rr.eo_sources { if x == *a { *src.entry(k.clone()).or_default() += y / seeds as f64; } } }
                let mut v: Vec<(String, f64)> = src.into_iter().filter(|x| x.1.abs() >= 2.0).collect();
                v.sort_by(|p, q| p.1.partial_cmp(&q.1).unwrap());
                let pr: Vec<f64> = runs.iter().flat_map(|rr| rr.pressure.get(*a).into_iter().flatten().copied()).collect();
                let mut cs: BTreeMap<String, f64> = BTreeMap::new();
                for rr in runs.iter() { for ((x, k), y) in &rr.coh_sources { if x == *a { *cs.entry(k.clone()).or_default() += y / seeds as f64; } } }
                let mut cv: Vec<(String, f64)> = cs.into_iter().filter(|x| x.1.abs() >= 20.0).collect();
                cv.sort_by(|p, q| p.1.partial_cmp(&q.1).unwrap());
                println!("| {sc} {world} | {a} | {lab} | {floor:.0} % | {:.1} | {:.1} | {:.0} | {vt:.0} | {} | {} |", mean(&army), mean(&coh), mean(&pr), v.iter().map(|(k, x)| format!("{k} {x:+.0}")).collect::<Vec<_>>().join("; "),
                    cv.iter().map(|(k, x)| format!("{k} {x:+.0}")).collect::<Vec<_>>().join("; "));
            }
        }
    }
}

/// Why the papacy submits to Naples more often under Ц8: milan, K₂ = 1, the cohesion pull on and
/// off; mean army, quality, cohesion, economic output and population of both, their strength
/// ratio, and the games in which the papacy submits.
fn papacy_pair(first: u64, seeds: u64, ticks: u32) {
    println!("# Naples and the papacy, milan, K₂ = 1, seeds {first}–{}: Ц8's cohesion pull on / off\n", first + seeds - 1);
    println!("| world | cohesion pull | actor | army | quality | cohesion | economic output | population | S Naples / S papacy (mean of ticks) | papacy submits (games) |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for world in ["none", "aggressive"] {
        for pull in [None, Some(0.12)] {
            let mut acc: BTreeMap<&str, [f64; 5]> = BTreeMap::new();
            let mut n = 0.0;
            let (mut ratio, mut rn): (f64, f64) = (0.0, 0.0);
            let mut submits = 0;
            for seed in first..first + seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, "milan_1477".to_string()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                {
                    let s = st.current_scenario.as_mut().unwrap();
                    s.features.economy_v2 = true;
                    s.economy_v2_conquest_k2 = Some(1);
                    s.economy_v2_cohesion_pull = pull;
                }
                let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, "milan_1477"));
                let mut sub = false;
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
                    if ws.vassalages.iter().any(|v| v.vassal_id == "papacy" && v.overlord_id == "naples") { sub = true; }
                    let (Some(na), Some(pa)) = (ws.actors.get("naples"), ws.actors.get("papacy")) else { continue };
                    n += 1.0;
                    for (id, a) in [("naples", na), ("papacy", pa)] {
                        let e = acc.entry(id).or_default();
                        for (i, m) in ["military_size", "military_quality", "cohesion", "economic_output", "population"].iter().enumerate() { e[i] += a.get_metric(m); }
                    }
                    let s = |a: &engine13::core::Actor| a.get_metric("military_size") * a.get_metric("military_quality") / 100.0;
                    if s(pa) > 0.0 { ratio += s(na) / s(pa); rn += 1.0; }
                }
                if sub { submits += 1; }
            }
            for id in ["naples", "papacy"] {
                let e = acc[id];
                println!("| {world} | {} | {id} | {:.1} | {:.1} | {:.1} | {:.1} | {:.0} | {} | {} |", pull.map_or("off".into(), |r| format!("{r}")), e[0] / n, e[1] / n, e[2] / n, e[3] / n, e[4] / n,
                    if id == "naples" { format!("{:.2}", ratio / rn.max(1.0)) } else { String::new() }, if id == "naples" { submits.to_string() } else { String::new() });
            }
        }
    }
}

/// One battle with what the balance-of-power rule needs: the loser's neighbours other than the
/// winner and free of a vassal bond with it, their strength summed at distance 1 and at ≤ 2 (state
/// at the start of the battle's tick).
struct DeterBattle {
    tick: u32,
    winner: String,
    loser: String,
    s_w: f64,
    s_l: f64,
    sum_d1: f64,
    sum_d2: f64,
}

/// The world's battles under one model, with the neighbour sums; and papacy's population writes.
/// (battles, papacy's population writes by source, papacy's (tick, population, army))
type DeterOut = (Vec<DeterBattle>, BTreeMap<String, f64>, Vec<(u32, f64, f64)>);

fn deter_run(sc: &str, world: &str, k2: Option<u32>, seed: u64, ticks: u32) -> DeterOut {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        s.economy_v2_conquest_k2 = k2;
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let strength = |a: &engine13::core::Actor| a.get_metric("military_size").max(0.0) * a.get_metric("military_quality").clamp(0.0, 100.0) / 100.0;
    let mut out = Vec::new();
    let mut pop_src: BTreeMap<String, f64> = BTreeMap::new();
    let mut papacy: Vec<(u32, f64, f64)> = Vec::new();
    let _ = census::take_battles();
    let _ = census::take_writes();
    for _ in 0..ticks {
        // the state at the start of the tick: every actor's neighbours (id, distance, S), and bonds
        let ws = st.world_state.as_ref().unwrap();
        let mut nbs: BTreeMap<String, Vec<(String, u32, f64)>> = BTreeMap::new();
        for (id, a) in &ws.actors {
            if ws.dead_actor_ids.contains(id) { continue; }
            let v: Vec<(String, u32, f64)> = a.neighbors.iter().filter(|n| n.distance <= 2 && !ws.dead_actor_ids.contains(&n.id))
                .filter_map(|n| ws.actors.get(&n.id).map(|b| (n.id.clone(), n.distance, strength(b)))).collect();
            nbs.insert(id.clone(), v);
        }
        let bonds: Vec<(String, String)> = ws.vassalages.iter().map(|v| (v.vassal_id.clone(), v.overlord_id.clone())).collect();
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        for w in census::take_writes() {
            if w.actor == "papacy" && w.metric == "population" {
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                *pop_src.entry(src).or_default() += w.applied;
            }
        }
        for b in census::take_battles() {
            let (w, l, s_w, s_l) = if b.attacker_won { (b.attacker.clone(), b.defender.clone(), b.strength_attacker, b.strength_defender) } else { (b.defender.clone(), b.attacker.clone(), b.strength_defender, b.strength_attacker) };
            let bound = |n: &str| bonds.iter().any(|(v, o)| (v == n && o == &w) || (o == n && v == &w));
            let (mut d1, mut d2) = (0.0, 0.0);
            for (n, d, s) in nbs.get(&l).into_iter().flatten() {
                if n == &w || bound(n) { continue; }
                d2 += s;
                if *d == 1 { d1 += s; }
            }
            out.push(DeterBattle { tick: b.tick, winner: w, loser: l, s_w, s_l, sum_d1: d1, sum_d2: d2 });
        }
        let ws = st.world_state.as_ref().unwrap();
        if let Some(a) = ws.actors.get("papacy").filter(|_| !ws.dead_actor_ids.contains("papacy")) {
            papacy.push((ws.tick - 1, a.get_metric("population"), a.get_metric("military_size")));
        }
    }
    (out, pop_src, papacy)
}

/// The balance-of-power arithmetic (owner's grid): a loss counts toward the streak only if
/// `S_w ≥ 3 × (S_l + Σ S_n)` — the loser's neighbours other than the winner and free of a vassal
/// bond with it, at distance 1 or ≤ 2. Submission (K₁ = 3) is replayed over the protocols of the
/// base world of PR #240 (v2 with Ц8, no Ц7 — every battle still fought); the 1453 assault over the
/// protocols of the K₂ = 1 world. The rule applies to submission only, or to submission and the
/// declared conquest. Arithmetic, not a model: the world after a submission is not replayed.
fn deterrence(first: u64, seeds: u64, ticks: u32) {
    const PEOPLES: [&str; 5] = ["alamanni", "vandals", "visigoths", "burgundians", "franks"];
    const PAIRS: [(&str, &str, &str); 8] = [
        ("milan_1477", "naples", "papacy"), ("milan_1477", "florence", "siena"), ("milan_1477", "naples", "sicily"),
        ("constantinople_1430", "ottomans", "serbia"), ("constantinople_1430", "ottomans", "byzantium"),
        ("rome_375", "sassanids", "armenia"), ("rome_375", "huns", "ostrogoths"), ("milan_1477", "naples", "venice"),
    ];
    println!("# Ц7 deterrence — the balance-of-power rule over the protocols of PR #240, seeds {first}–{}\n", first + seeds - 1);
    println!("A loss counts only if S_w ≥ 3 × (S_l + Σ S_n), Σ over the loser's neighbours other than the winner and free of a vassal bond with it. Submission: K₁ = 3 over the base world's protocols (every battle fought); the assault: the K₂ = 1 world's battle on tick 46.\n");
    // cell: 0 = no rule, 1 = d1, 2 = d≤2
    let scopes = ["no rule (PR #240)", "distance 1", "distance ≤ 2"];
    let mut sub: BTreeMap<(usize, String, String, String), u32> = BTreeMap::new(); // (scope, world, vassal, lord) -> games
    let mut sub_tick: BTreeMap<(usize, String, String, String), Vec<f64>> = BTreeMap::new();
    let mut assault: BTreeMap<(usize, String), (u32, u32)> = BTreeMap::new(); // (scope, world) -> (falls on the assault, games with an assault)
    let mut pop_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            for seed in first..first + seeds {
                let (battles, pop_src, papacy) = deter_run(sc, world, None, seed, ticks);
                for (si, _) in scopes.iter().enumerate() {
                    let mut streak: BTreeMap<String, (String, u32)> = BTreeMap::new();
                    let mut done: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                    for b in &battles {
                        streak.remove(&b.winner);
                        if done.contains(&b.loser) { continue; }
                        let extra = match si { 0 => 0.0, 1 => b.sum_d1, _ => b.sum_d2 };
                        let qualifies = b.s_w >= 3.0 * (b.s_l + extra);
                        let e = streak.entry(b.loser.clone()).or_insert((b.winner.clone(), 0));
                        if qualifies { if e.0 == b.winner { e.1 += 1; } else { *e = (b.winner.clone(), 1); } } else { *e = (b.winner.clone(), 0); }
                        if e.1 >= 3 {
                            done.insert(b.loser.clone());
                            let key = (si, format!("{sc}/{world}"), b.loser.clone(), b.winner.clone());
                            *sub.entry(key.clone()).or_default() += 1;
                            sub_tick.entry(key).or_default().push(b.tick as f64);
                        }
                    }
                }
                if sc == "milan_1477" && seed == first {
                    let mut v: Vec<(String, f64)> = pop_src.into_iter().filter(|x| x.1.abs() >= 1.0).collect();
                    v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                    let at = |t: u32| papacy.iter().find(|x| x.0 == t).map(|x| format!("{:.0} / {:.1}", x.1, x.2)).unwrap_or("—".into());
                    pop_rows.push(format!("| {world} (seed {first}) | {} | {} | {} | {} | {} |", at(0), at(10), at(50), at(299), v.iter().map(|(k, x)| format!("{k} {x:+.0}")).collect::<Vec<_>>().join("; ")));
                }
                if sc == "constantinople_1430" {
                    let (cb, _, _) = deter_run(sc, world, Some(1), seed, ticks);
                    if let Some(b) = cb.iter().find(|b| b.tick == 46 && ((b.winner == "ottomans" && b.loser == "byzantium") || (b.winner == "byzantium" && b.loser == "ottomans"))) {
                        for (si, _) in scopes.iter().enumerate() {
                            let extra = match si { 0 => 0.0, 1 => b.sum_d1, _ => b.sum_d2 };
                            let falls = b.winner == "ottomans" && b.s_w >= 3.0 * (b.s_l + extra);
                            let e = assault.entry((si, world.to_string())).or_default();
                            e.0 += falls as u32;
                            e.1 += 1;
                        }
                    }
                }
            }
        }
    }
    let games = |si: usize, w: &str, v: &str, l: &str| sub.get(&(si, w.to_string(), v.to_string(), l.to_string())).copied().unwrap_or(0);
    let med = |si: usize, w: &str, v: &str, l: &str| sub_tick.get(&(si, w.to_string(), v.to_string(), l.to_string())).map(|x| format!("{:.0}", pct(x, 0.5))).unwrap_or("—".into());
    println!("## 1. Pairs: games of {seeds} in which the vassal submits (median tick)\n");
    print!("| pair | world |");
    for s in &scopes { print!(" {s} |"); }
    println!();
    println!("|---|---|---|---|---|");
    for (sc, lord, v) in PAIRS {
        for world in worlds(sc) {
            let w = format!("{sc}/{world}");
            print!("| {lord} → {v} | {world} |");
            for si in 0..3 { print!(" {} @{} |", games(si, &w, v, lord), med(si, &w, v, lord)); }
            println!();
        }
    }
    for world in worlds("rome_375") {
        let w = format!("rome_375/{world}");
        print!("| rome → five peoples (a game) | {world} |");
        for si in 0..3 { print!(" {:.1} |", PEOPLES.iter().map(|p| games(si, &w, p, "rome")).sum::<u32>() as f64 / seeds as f64); }
        println!();
    }
    println!("\n## 2. The 1453 assault (K₂ = 1 world): Byzantium falls on the assault, of the games with an assault\n");
    println!("| world | rule only on submission | rule also on conquest: distance 1 | distance ≤ 2 |");
    println!("|---|---|---|---|");
    for world in worlds("constantinople_1430") {
        let a = |si: usize| assault.get(&(si, world.to_string())).map(|x| format!("{} of {}", x.0, x.1)).unwrap_or("—".into());
        println!("| {world} | {} | {} | {} |", a(0), a(1), a(2));
    }
    println!("\n## 3. Ц7's items 1–3 per cell\n");
    println!("| neighbours | rule on | (1) rome: ≥ 3 of 5 peoples, worlds | (2) Byzantium none: falls ≥ 20 / 30 on the assault | (3) milan: papacy ≤ 3, Venice ≤ 3, Milan ≤ 1, worlds |");
    println!("|---|---|---|---|---|");
    for (si, scope) in scopes.iter().enumerate().skip(1) {
        for on_conquest in [false, true] {
            let r1 = worlds("rome_375").iter().filter(|w| PEOPLES.iter().map(|p| games(si, &format!("rome_375/{w}"), p, "rome")).sum::<u32>() as f64 / seeds as f64 >= 3.0).count();
            let ai = if on_conquest { si } else { 0 };
            let (f, n) = assault.get(&(ai, "none".to_string())).copied().unwrap_or((0, 0));
            let r3 = worlds("milan_1477").iter().filter(|w| {
                let wk = format!("milan_1477/{w}");
                let any = |v: &str| sub.iter().filter(|((s, ww, vv, _), _)| *s == si && ww == &wk && vv == v).map(|(_, g)| *g).sum::<u32>();
                any("papacy") <= 3 && any("venice") <= 3 && any("milan") <= 1
            }).count();
            println!("| {scope} | {} | {r1} / 4 | {f} of {n} — {} | {r3} / 2 |", if on_conquest { "submission and conquest" } else { "submission only" }, if f >= 20 { "yes" } else { "no" });
        }
    }
    println!("\n## 4. Why the papacy loses its people (milan, base world, one game): population / army on ticks 0, 10, 50, 299, and population writes by source\n");
    println!("| world | tick 0 | tick 10 | tick 50 | tick 299 | population writes by source (|x| ≥ 1) |");
    println!("|---|---|---|---|---|---|");
    for r in pop_rows { println!("{r}"); }
}
