//! Economy project, Ц8: cohesion as a level (docs/economy_project_brief.md §9). Built with
//! `--features census`.
//!
//! **pre** (before the run, §9.7; seeds 0–29, v2 as in the content): the b / r table of every
//! cohesion writer at r = 0.12 — b is a dependency rule's largest single write (bins of its source
//! with ≥ 2 % of the writes) or otherwise the largest per-actor rate — with a stop flag above 50;
//! the authored spread of starting cohesion per scenario; and for the Ц1 violators of Ц7
//! (burgundians, alamanni, ostrogoths, sicily) their norm `T_C` and the shift of every writer except
//! `cohesion_natural_decay` (rate / r), in base and — under Ц7, K₂ = 1 — while a vassal, against the
//! threshold of `cohesion_to_economic_output` measured from the norm (50 × T_C / 100).
//!
//! **run** (seeds from an argument; 0 and 100): models base (v2 as in the content), (a) the cohesion
//! pull at r = 0.12, (b) the same with the decay rule kept (census `set_keep_cohesion_decay`), for
//! the record. Ц8's measure: per actor cohesion ≤ 1 and ≥ 99 each under 20 % of living ticks, for
//! ≥ 80 % of actors in every world; the spread of actor medians ≥ 15 in every world. Stop rule
//! (§9.6): Ц1, Ц4, Ц5, Ц6 against base on the same seeds. For information: the treasury of Rome and
//! the Ottomans on ticks 150 and 299 in base (the Ц2 question).
//!
//! **content**: v2 as the content stands against (a) set here, bit for bit, every tick (seeds 100–).
//!
//! Usage: cargo run --release --features census --bin c8_probe -- pre|content [seeds] [ticks]
//!        cargo run --release --features census --bin c8_probe -- run [first_seed] [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const R: f64 = 0.12;
const VIOLATORS: [(&str, &str); 4] = [("rome_375", "burgundians"), ("rome_375", "alamanni"), ("rome_375", "ostrogoths"), ("milan_1477", "sicily")];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

#[derive(Default)]
struct Run {
    /// (actor, source) -> cohesion written (applied), all living ticks
    sources: BTreeMap<(String, String), f64>,
    /// (actor, source) -> cohesion written while the actor is a vassal
    vassal_sources: BTreeMap<(String, String), f64>,
    /// (dependency, bin of its source) -> (writes, applied)
    bins: BTreeMap<(String, i32), (u64, f64)>,
    living: BTreeMap<String, u64>,
    vassal_ticks: BTreeMap<String, u64>,
}

fn run(sc: &str, world: &str, conquest: bool, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        s.economy_v2_conquest_k2 = conquest.then_some(1);
    }
    let from: BTreeMap<String, String> = st.current_scenario.as_ref().unwrap().dependencies.iter().map(|d| (d.id.clone(), d.from.as_str().to_string())).collect();
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut prev: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
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
        let ws = st.world_state.as_ref().unwrap();
        let vassals: std::collections::BTreeSet<String> = ws.vassalages.iter().map(|v| v.vassal_id.clone()).collect();
        for w in census::take_writes() {
            if w.metric != "cohesion" { continue; }
            let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
            *r.sources.entry((w.actor.clone(), src.clone())).or_default() += w.applied;
            if vassals.contains(&w.actor) { *r.vassal_sources.entry((w.actor.clone(), src.clone())).or_default() += w.applied; }
            if let Some(id) = src.strip_prefix("dependency ") {
                if let Some(v) = from.get(id).and_then(|m| prev.get(&w.actor).and_then(|p| p.get(m))) {
                    let e = r.bins.entry((id.to_string(), ((v / 10.0).floor() as i32).clamp(0, 30))).or_default();
                    e.0 += 1; e.1 += w.applied;
                }
            }
        }
        for (id, a) in &ws.actors {
            if ws.dead_actor_ids.contains(id) { continue; }
            *r.living.entry(id.clone()).or_default() += 1;
            if vassals.contains(id) { *r.vassal_ticks.entry(id.clone()).or_default() += 1; }
            prev.insert(id.clone(), a.metrics.iter().map(|(k, v)| (k.clone(), *v)).collect());
        }
    }
    r
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).cloned().unwrap_or_else(|| "pre".into());
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    if mode == "pre" { pre(seeds, ticks); }
    if mode == "content" {
        let (mut same, mut total) = (0, 0);
        for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
            for world in worlds(sc) {
                for seed in 100..100 + seeds {
                    total += 1;
                    if run_full(sc, world, 3, true, seed, ticks).fingerprint == run_full(sc, world, 1, true, seed, ticks).fingerprint { same += 1; }
                }
            }
        }
        println!("Ц8 content check: v2 as in the content against (a) set here, seeds 100–{}: {same} of {total} runs identical, every actor metric every tick.", 99 + seeds);
    }
    if mode == "run" {
        let first: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let seeds: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);
        let ticks: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(300);
        full(first, seeds, ticks);
    }
}

fn pre(seeds: u64, ticks: u32) {
    println!("# Ц8 before the run — v2 as in the content, seeds 0–{}, {ticks} ticks, r = {R}\n", seeds - 1);
    // authored spread
    println!("## 1. Authored starting cohesion per scenario (starting actors)\n");
    println!("| scenario | min | max | spread | median |");
    println!("|---|---|---|---|---|");
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let db = engine13::db::Db::open_in_memory().unwrap();
        let mut st = engine13::AppState::default();
        engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
        let mut v: Vec<f64> = st.current_scenario.as_ref().unwrap().actors.iter().filter(|a| !a.is_successor_template).filter_map(|a| a.metrics.get("cohesion").copied()).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("| {sc} | {:.0} | {:.0} | {:.0} | {:.0} |", v[0], v[v.len() - 1], v[v.len() - 1] - v[0], v[v.len() / 2]);
    }
    println!("\n## 2. b / r of every cohesion writer at r = {R} (base, pooled over each scenario's worlds)\n");
    println!("| scenario | writer | mean per actor-tick | state at the largest b | b / r | > 50 |");
    println!("|---|---|---|---|---|---|");
    let mut stop = Vec::new();
    let mut base_rates: BTreeMap<(String, String), BTreeMap<String, f64>> = BTreeMap::new();
    let mut vassal_rates: BTreeMap<(String, String), BTreeMap<String, f64>> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        let mut src: BTreeMap<(String, String), f64> = BTreeMap::new();
        let mut bins: BTreeMap<(String, i32), (u64, f64)> = BTreeMap::new();
        let mut living: BTreeMap<String, u64> = BTreeMap::new();
        for world in worlds(sc) {
            let runs: Vec<Run> = (0..seeds).map(|s| run(sc, world, false, s, ticks)).collect();
            for rr in &runs {
                for (k, x) in &rr.sources { *src.entry(k.clone()).or_default() += x; }
                for (k, x) in &rr.bins { let e = bins.entry(k.clone()).or_default(); e.0 += x.0; e.1 += x.1; }
                for (k, x) in &rr.living { *living.entry(k.clone()).or_default() += x; }
            }
            if *world == "none" {
                for (vsc, actor) in VIOLATORS.iter().filter(|v| v.0 == sc) {
                    let lv: u64 = runs.iter().map(|rr| rr.living.get(*actor).copied().unwrap_or(0)).sum();
                    let e = base_rates.entry((vsc.to_string(), actor.to_string())).or_default();
                    for rr in &runs { for ((a, k), x) in &rr.sources { if a == actor { *e.entry(k.clone()).or_default() += x / lv.max(1) as f64; } } }
                }
                let conq: Vec<Run> = (0..seeds).map(|s| run(sc, world, true, s, ticks)).collect();
                for (vsc, actor) in VIOLATORS.iter().filter(|v| v.0 == sc) {
                    let vt: u64 = conq.iter().map(|rr| rr.vassal_ticks.get(*actor).copied().unwrap_or(0)).sum();
                    let e = vassal_rates.entry((vsc.to_string(), actor.to_string())).or_default();
                    for rr in &conq { for ((a, k), x) in &rr.vassal_sources { if a == actor { *e.entry(k.clone()).or_default() += x / vt.max(1) as f64; } } }
                }
            }
        }
        let total: u64 = living.values().sum();
        let mut by: BTreeMap<String, (f64, f64, String)> = BTreeMap::new();
        for ((actor, s), x) in &src {
            let e = by.entry(s.clone()).or_insert((0.0, 0.0, String::new()));
            e.0 += x;
            let per = x / living.get(actor).copied().unwrap_or(1).max(1) as f64;
            if per.abs() > e.1.abs() { e.1 = per; e.2 = actor.clone(); }
        }
        let mut v: Vec<(String, (f64, f64, String))> = by.into_iter().collect();
        v.sort_by(|a, b| b.1 .1.abs().partial_cmp(&a.1 .1.abs()).unwrap());
        for (s, (sum, max, who)) in v.iter().filter(|x| x.1 .1.abs() >= 0.02) {
            let (b, state) = match s.strip_prefix("dependency ") {
                Some(id) => {
                    let bs: Vec<(i32, (u64, f64))> = bins.iter().filter(|((d, _), _)| d == id).map(|((_, bb), x)| (*bb, *x)).collect();
                    let writes: u64 = bs.iter().map(|x| x.1 .0).sum();
                    match bs.iter().filter(|x| x.1 .0 * 50 >= writes).max_by(|a, b| (a.1 .1 / a.1 .0 as f64).abs().partial_cmp(&(b.1 .1 / b.1 .0 as f64).abs()).unwrap()) {
                        Some((bb, (n, x))) => (x / *n as f64, format!("source {}–{}: {:+.2} a write ({:.0} % of writes)", bb * 10, bb * 10 + 10, x / *n as f64, 100.0 * *n as f64 / writes.max(1) as f64)),
                        None => (*max, "—".into()),
                    }
                }
                None => (*max, format!("largest per-actor rate ({who})")),
            };
            let over = (b / R).abs() > 50.0 && !s.contains("cohesion_natural_decay");
            if over { stop.push(format!("{sc}: {s} {:+.0}", b / R)); }
            println!("| {sc} | {s} | {:+.3} | {state} | {:+.0} | {} |", sum / total.max(1) as f64, b / R, if over { "**yes**" } else { "" });
        }
    }
    println!("\n`cohesion_natural_decay` is the one-sided forerunner of the pull and is not applied under it; its row is for the record.\n");
    println!("**Writers with a shift above 50:** {}\n", if stop.is_empty() { "none".into() } else { stop.join("; ") });
    println!("## 3. The Ц1 violators of Ц7: the norm, the shifts, and the threshold of cohesion_to_economic_output (50 × T_C / 100)\n");
    println!("Shift = Σ (rate per living tick) / r over every writer except `cohesion_natural_decay`; rates in world `none`. Equilibrium ≈ T_C + shift (the dependency rules depend on the state, so this is a first estimate).\n");
    println!("| actor | T_C | threshold | base: shift → equilibrium | vassal (Ц7, K₂ = 1, ticks while a vassal): shift → equilibrium | largest writers as a vassal (shift) |");
    println!("|---|---|---|---|---|---|");
    for (sc, actor) in VIOLATORS {
        let db = engine13::db::Db::open_in_memory().unwrap();
        let mut st = engine13::AppState::default();
        engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
        let tc = st.current_scenario.as_ref().unwrap().actors.iter().find(|a| a.id == actor).and_then(|a| a.metrics.get("cohesion").copied()).unwrap_or(f64::NAN);
        let shift = |m: Option<&BTreeMap<String, f64>>| m.map(|m| m.iter().filter(|(k, _)| !k.contains("cohesion_natural_decay")).map(|(_, x)| x / R).sum::<f64>()).unwrap_or(f64::NAN);
        let key = (sc.to_string(), actor.to_string());
        let (sb, sv) = (shift(base_rates.get(&key)), shift(vassal_rates.get(&key)));
        let mut top: Vec<(String, f64)> = vassal_rates.get(&key).map(|m| m.iter().filter(|(k, _)| !k.contains("cohesion_natural_decay")).map(|(k, x)| (k.clone(), x / R)).collect()).unwrap_or_default();
        top.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());
        println!("| {actor} | {tc:.0} | {:.0} | {sb:+.0} → {:.0} | {sv:+.0} → {:.0} | {} |", 0.5 * tc, tc + sb, tc + sv, top.iter().take(5).map(|(k, x)| format!("{k} {x:+.0}")).collect::<Vec<_>>().join("; "));
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

fn share(x: u64, n: u64) -> f64 { 100.0 * x as f64 / n.max(1) as f64 }

#[derive(Default)]
struct Full {
    eo: BTreeMap<String, Vec<f64>>,
    legit: BTreeMap<String, Vec<f64>>,
    coh: BTreeMap<String, Vec<f64>>,
    calm: BTreeMap<String, (u64, u64, u64)>,
    living: u64,
    ceiling_no_threat: u64,
    corr: [f64; 6],
    declines: Vec<(bool, u8)>,
    battles: Vec<census::Battle>,
    treasury: BTreeMap<(String, u32), f64>,
    deaths: u32,
    fingerprint: u64,
}

/// 0 = base, 1 = (a) pull, 2 = (b) pull with the decay rule kept, 3 = v2 as the content stands
fn run_full(sc: &str, world: &str, m: u8, quality: bool, seed: u64, ticks: u32) -> Full {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        if m != 3 { s.economy_v2_cohesion_pull = (m > 0).then_some(R); }
    }
    census::set_keep_cohesion_decay(m == 2);
    census::set_combat_quality(quality);
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Full::default();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    type Open = (String, u32, f64, f64, bool, f64, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    let _ = census::take_battles();
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
        let _ = census::take_writes();
        r.battles.extend(census::take_battles());
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        if t == 150 || t == ticks - 1 {
            for lord in ["rome", "ottomans"] {
                if let Some(a) = ws.actors.get(lord).filter(|_| !ws.dead_actor_ids.contains(lord)) { r.treasury.insert((lord.to_string(), t), a.get_metric("treasury")); }
            }
        }
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
    }
    census::set_keep_cohesion_decay(false);
    census::set_combat_quality(true);
    r.deaths = st.world_state.as_ref().unwrap().dead_actors.len() as u32;
    r.fingerprint = std::hash::Hasher::finish(&fp);
    r
}

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

#[derive(Default)]
struct Pool {
    worlds: usize,
    c8: [usize; 2],
    c1: [usize; 3],
    c5: [usize; 2],
    c6: [usize; 2],
    decline: (u64, u64),
    c4: (u64, u64, f64, f64),
    c4b: (u64, u64),
}

fn full(first: u64, seeds: u64, ticks: u32) {
    census::enable_battles();
    println!("# Ц8 — cohesion as a level, seeds {first}–{}, {ticks} ticks per world, r = {R}\n", first + seeds - 1);
    let labels = ["base", "(a) pull", "(b) pull + decay kept"];
    let mut pools: Vec<Pool> = (0..3).map(|_| Pool::default()).collect();
    let mut rows = Vec::new();
    let mut fail_rows = Vec::new();
    let mut tre_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            for m in 0..3u8 {
                let runs: Vec<Full> = (first..first + seeds).map(|s| run_full(sc, world, m, true, s, ticks)).collect();
                let p = &mut pools[m as usize];
                p.worlds += 1;
                let frac = |v: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * v.iter().filter(|y| pr(**y)).count() as f64 / v.len().max(1) as f64;
                let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                let mut legit: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                let mut coh: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                let mut calm: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
                let (mut lv, mut cnt) = (0, 0);
                let mut c = [0.0; 6];
                for rr in &runs {
                    for (k, v) in &rr.eo { eo.entry(k.clone()).or_default().extend(v); }
                    for (k, v) in &rr.legit { legit.entry(k.clone()).or_default().extend(v); }
                    for (k, v) in &rr.coh { coh.entry(k.clone()).or_default().extend(v); }
                    for (k, v) in &rr.calm { let e = calm.entry(k.clone()).or_default(); e.0 += v.0; e.1 += v.1; e.2 += v.2; }
                    lv += rr.living; cnt += rr.ceiling_no_threat;
                    for (a, b) in c.iter_mut().zip(&rr.corr) { *a += b; }
                    for d in rr.declines.iter().filter(|d| d.1 == 2) { p.decline.0 += 1; p.decline.1 += d.0 as u64; }
                    let x = c4(&rr.battles);
                    p.c4.0 += x.0; p.c4.1 += x.1; p.c4.2 += x.2; p.c4.3 += x.3;
                }
                if m < 2 {
                    for s in first..first + seeds { let x = c4(&run_full(sc, world, m, false, s, ticks).battles); p.c4b.0 += x.0; p.c4b.1 += x.1; }
                }
                let spread = |m: &BTreeMap<String, Vec<f64>>| { let v: Vec<f64> = m.values().map(|x| pct(x, 0.5)).collect(); v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min) };
                let c8_fail: Vec<String> = coh.iter().filter(|(_, x)| frac(x, &|y| y <= 1.0) >= 20.0 || frac(x, &|y| y >= 99.0) >= 20.0)
                    .map(|(k, x)| format!("{k} ({:.0} / {:.0} %)", frac(x, &|y| y <= 1.0), frac(x, &|y| y >= 99.0))).collect();
                let c8_pass = coh.len() - c8_fail.len();
                let cspread = spread(&coh);
                let c1n = eo.values().filter(|x| frac(x, &|y| y >= 99.0) < 20.0 && frac(x, &|y| y <= 1.0) < 20.0).count();
                let tm: Vec<f64> = tiers(sc).iter().map(|t| { let x: Vec<f64> = t.iter().filter_map(|a| eo.get(*a)).flatten().copied().collect(); pct(&x, 0.5) }).collect();
                let c5n = calm.values().filter(|v| v.0 > 0 && share(v.1, v.0) < 20.0 && share(v.2, v.0) < 20.0).count();
                let ncalm = calm.values().filter(|v| v.0 > 0).count();
                let (n, sx, sy, sxx, syy, sxy) = (c[0], c[1], c[2], c[3], c[4], c[5]);
                let corr = (n * sxy - sx * sy) / ((n * sxx - sx * sx).sqrt() * (n * syy - sy * sy).sqrt());
                p.c8[0] += (100 * c8_pass >= 80 * coh.len()) as usize;
                p.c8[1] += (cspread >= 15.0) as usize;
                for (slot, ok) in p.c1.iter_mut().zip([100 * c1n >= 80 * eo.len(), tm.windows(2).all(|w| w[0] > w[1]), spread(&eo) >= 20.0]) { *slot += ok as usize; }
                for (slot, ok) in p.c5.iter_mut().zip([100 * c5n >= 80 * ncalm, spread(&legit) >= 15.0]) { *slot += ok as usize; }
                for (slot, ok) in p.c6.iter_mut().zip([share(cnt, lv) < 30.0, corr >= 0.7]) { *slot += ok as usize; }
                let all_c: Vec<f64> = coh.values().flatten().copied().collect();
                let meds: Vec<f64> = coh.values().map(|x| pct(x, 0.5)).collect();
                rows.push(format!("| {sc} | {world} | {} | {:.0} % / {:.0} % | {c8_pass} / {} | {cspread:.0} | {:.0}/{:.0}/{:.0} | {c1n} / {} | {} |", labels[m as usize],
                    frac(&all_c, &|y| y <= 1.0), frac(&all_c, &|y| y >= 99.0), coh.len(), pct(&meds, 0.1), pct(&meds, 0.5), pct(&meds, 0.9), eo.len(), runs.iter().map(|rr| rr.deaths).sum::<u32>()));
                if !c8_fail.is_empty() { fail_rows.push(format!("| {sc} | {world} | {} | {} |", labels[m as usize], c8_fail.join(", "))); }
                if m == 0 {
                    let tr = |lord: &str, tk: u32| -> Vec<f64> { runs.iter().filter_map(|rr| rr.treasury.get(&(lord.to_string(), tk)).copied()).collect() };
                    let q3 = |v: Vec<f64>| if v.is_empty() { "—".to_string() } else { format!("{:.0}/{:.0}/{:.0}", pct(&v, 0.1), pct(&v, 0.5), pct(&v, 0.9)) };
                    tre_rows.push(format!("| {sc} | {world} | rome {} → {} | ottomans {} → {} |", q3(tr("rome", 150)), q3(tr("rome", ticks - 1)), q3(tr("ottomans", 150)), q3(tr("ottomans", ticks - 1))));
                }
            }
        }
    }
    println!("## 1. Ц8's measure and Ц1 per world\n");
    println!("| scenario | world | model | cohesion ≤ 1 / ≥ 99, all actor-ticks | actors passing | spread of medians | actor medians p10/50/90 | Ц1 actors | deaths |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in rows { println!("{r}"); }
    println!("\n### Actors failing Ц8 (floor / ceiling share)\n");
    println!("| scenario | world | model | actors |");
    println!("|---|---|---|---|");
    for r in fail_rows { println!("{r}"); }
    println!("\n## 2. Summary: Ц8's pre-commitment and the stop rule against base on the same seeds\n");
    println!("| model | Ц8 actors / spread | Ц1 actors / tiers / spread | Ц5 actors / spread | Ц6 ceiling without a threat / corr | Ц6 decline | Ц4 (1) | Ц4 (2) |");
    println!("|---|---|---|---|---|---|---|---|");
    for (i, p) in pools.iter().enumerate() {
        let n = p.worlds;
        let pa = p.c4.1 as f64 / p.c4.0.max(1) as f64;
        let promise = p.c4.2 / p.c4.0.max(1) as f64;
        let se = p.c4.3.sqrt() / p.c4.0.max(1) as f64;
        let c4b = if p.c4b.0 > 0 {
            let pb = p.c4b.1 as f64 / p.c4b.0 as f64;
            let se_d = (pa * (1.0 - pa) / p.c4.0.max(1) as f64 + pb * (1.0 - pb) / p.c4b.0 as f64).sqrt();
            format!("{:+.1} vs {:.1} {}", 100.0 * (pa - pb), 200.0 * se_d, if pa - pb >= 2.0 * se_d { "yes" } else { "**no**" })
        } else { "—".into() };
        println!("| {} | {} / {} of {n} | {} / {} / {} | {} / {} | {} / {} | {:.1} % of {} | {:.1} % vs {:.1} % {} | {c4b} |", labels[i], p.c8[0], p.c8[1], p.c1[0], p.c1[1], p.c1[2], p.c5[0], p.c5[1], p.c6[0], p.c6[1],
            share(p.decline.1, p.decline.0), p.decline.0, 100.0 * pa, 100.0 * (promise - 2.0 * se), if pa >= promise - 2.0 * se { "yes" } else { "**no**" });
    }
    println!("\n## 3. For information: treasury of Rome and the Ottomans in base (no Ц7), p10/50/90, tick 150 → {}\n", ticks - 1);
    println!("| scenario | world | rome | ottomans |");
    println!("|---|---|---|---|");
    for r in tre_rows { println!("{r}"); }
}
