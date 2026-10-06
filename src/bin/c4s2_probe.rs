//! Economy project, Ц4 closed: the corrected Ц4 measure on held-out seeds, and the arithmetic
//! for Ц7 (docs/economy_project_brief.md §9). Built with `--features census`.
//!
//! **measure** (default; seeds 100–129, never used to derive the measure): variants (a) the battle
//! outcome, (b) strength without quality. In battles with the army ratio 0.8–1.25 and a quality
//! gap ≥ 10: (1) the share won by the side of higher quality under (a) is not below what the
//! formula `S = army × quality / 100` promises on those battles minus 2 standard errors; (2)
//! (a) − (b) is at least 2 standard errors of the difference. Stop rule at (a): Ц1, Ц5, Ц6 (the
//! corrected "ceiling without a threat"). Then the content check: v2 as the content stands is the
//! same world as (a) set here, bit for bit, every tick.
//!
//! **c7** (seeds 0–29, the stage 1 runs, model (a)): the arithmetic Ц7 needs before its
//! pre-commitment — how often an actor under full threat (`T_p` ≥ 90) fights and loses, and how
//! many actors reach K lost battles in a row under full threat, and on which tick.
//!
//! **vassal** (seeds 0–29, model (a)): Ц7 with submission — who would become a vassal at the first
//! streak of K₁ losses in a row to one winner with `S_w ≥ R × S_l`, of whom, and when.
//!
//! Usage: cargo run --release --features census --bin c4s2_probe -- [measure|c7|c7b|vassal|content|decline] [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

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

#[derive(Clone, Copy, PartialEq)]
enum Model { A, B, Content, NoOutcome }

const KMAX: usize = 8;

#[derive(Default)]
struct Run {
    battles: Vec<census::Battle>,
    // stop rule
    eo: BTreeMap<String, Vec<f64>>,
    legit: BTreeMap<String, Vec<f64>>,
    calm: BTreeMap<String, (u64, u64, u64)>,
    living: u64,
    ceiling_no_threat: u64,
    corr: [f64; 6],
    declines: Vec<(bool, u8)>,
    fingerprint: u64,
    // Ц7: actor-ticks under full threat, battles fought and lost there, first tick of a K-streak
    full_threat_ticks: u64,
    ft_battles: u64,
    ft_lost: u64,
    streak_first: BTreeMap<String, [Option<u32>; KMAX + 1]>,
    deaths: BTreeMap<String, u32>,
}

fn run(sc: &str, world: &str, m: Model, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        match m {
            Model::A | Model::B => s.economy_v2_combat_outcome = true,
            Model::Content => {}
            Model::NoOutcome => s.economy_v2_combat_outcome = false,
        }
    }
    census::set_combat_quality(m != Model::B);
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    let mut fp = std::collections::hash_map::DefaultHasher::new();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    let mut streak: BTreeMap<String, usize> = BTreeMap::new();
    type Open = (String, u32, f64, f64, bool, f64, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    let _ = census::take_battles();
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
        let battles = census::take_battles();
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) { r.deaths.insert(d.clone(), t); }
        // Ц7: the threat each side stood under at the end of the previous tick (the battle's own tick)
        for b in &battles {
            for (id, lost) in [(&b.attacker, !b.attacker_won), (&b.defender, b.attacker_won)] {
                let full = prev_tp.get(id).is_some_and(|tp| *tp >= 90.0);
                if full {
                    r.ft_battles += 1;
                    if lost { r.ft_lost += 1; }
                }
                let s = streak.entry(id.clone()).or_default();
                *s = if full && lost { *s + 1 } else { 0 };
                let first = r.streak_first.entry(id.clone()).or_insert([None; KMAX + 1]);
                for (k, slot) in first.iter_mut().enumerate().take((*s).min(KMAX) + 1).skip(1) {
                    if slot.is_none() { *slot = Some(t); let _ = k; }
                }
            }
        }
        r.battles.extend(battles);
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        open.retain_mut(|(id, deadline, goal, drop, below, tp_new, tp_max, reached)| {
            let Some(a) = ws.actors.get(id).filter(|_| !ws.dead_actor_ids.contains(id)) else { return false };
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) { *tp_max = tp_max.max(tp); }
            if a.get_metric("external_pressure") <= *goal { *reached = true; }
            if t < *deadline { return true; }
            r.declines.push((*reached, if *below { 0 } else if *tp_max >= *tp_new + *drop / 2.0 { 1 } else { 2 }));
            false
        });
        for id in ids {
            let a = &ws.actors[id];
            let mut ms: Vec<(&String, &f64)> = a.metrics.iter().collect();
            ms.sort_by(|x, y| x.0.cmp(y.0));
            for (k, v) in ms { (id, k, v.to_bits()).hash(&mut fp); }
            if ws.dead_actor_ids.contains(id) { continue; }
            let l = a.get_metric("legitimacy");
            let ep = a.get_metric("external_pressure");
            let tp = engine13::engine::pressure_threat(ws, id);
            r.living += 1;
            if ep >= 99.0 && tp.is_some_and(|x| x < 90.0) { r.ceiling_no_threat += 1; }
            if tp.is_some_and(|x| x >= 90.0) { r.full_threat_ticks += 1; }
            let crisis = a.actor_tags.values().any(|tag| tag.metrics_modifier.iter().any(|(mm, v)| mm.as_str() == "legitimacy" && *v < 0));
            if !crisis {
                let e = r.calm.entry(id.clone()).or_default();
                e.0 += 1;
                if l <= 1.0 { e.1 += 1; }
                if l >= 99.0 { e.2 += 1; }
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
        }
    }
    census::set_combat_quality(true);
    r.fingerprint = fp.finish();
    r
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if s.is_empty() { return f64::NAN; }
    s[((s.len() - 1) as f64 * p).round() as usize]
}

fn share(x: u64, n: u64) -> f64 { 100.0 * x as f64 / n.max(1) as f64 }

/// (counted, won by higher quality, Σ p promised, Σ p(1 − p))
fn measure<'a>(battles: impl Iterator<Item = &'a census::Battle>) -> (u64, u64, f64, f64) {
    let mut acc = (0, 0, 0.0, 0.0);
    for b in battles {
        if b.army_defender <= 0.0 { continue; }
        let ratio = b.army_attacker / b.army_defender;
        if !(0.8..=1.25).contains(&ratio) || (b.quality_attacker - b.quality_defender).abs() < 10.0 { continue; }
        let hq_att = b.quality_attacker > b.quality_defender;
        let (s_a, s_d) = (b.army_attacker * b.quality_attacker / 100.0, b.army_defender * b.quality_defender / 100.0);
        let p_att = if s_a + s_d > 0.0 { s_a / (s_a + s_d) } else { 0.5 };
        let p = if hq_att { p_att } else { 1.0 - p_att };
        acc.0 += 1;
        acc.1 += (hq_att == b.attacker_won) as u64;
        acc.2 += p;
        acc.3 += p * (1.0 - p);
    }
    acc
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).cloned().unwrap_or_else(|| "measure".into());
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_battles();
    if mode == "c7" { c7(seeds, ticks); return; }
    if mode == "decline" { decline(seeds, ticks); return; }
    if mode == "content" { content(seeds, ticks); return; }
    if mode == "c7b" { c7b(seeds, ticks); return; }
    if mode == "vassal" { vassal(seeds, ticks); return; }
    let first = 100;
    println!("# Ц4 closed — the corrected measure on held-out seeds {first}–{}, {ticks} ticks per world\n", first + seeds - 1);
    let mut tot: BTreeMap<(&str, &str), (u64, u64, f64, f64)> = BTreeMap::new();
    let mut stop_rows = Vec::new();
    let mut stop_ok = [0usize; 7];
    let mut decl = (0u64, 0u64);
    let mut nworld = 0;
    let (mut same, mut total) = (0, 0);
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            nworld += 1;
            for (m, label) in [(Model::A, "(a)"), (Model::B, "(b)")] {
                let runs: Vec<Run> = (first..first + seeds).map(|s| run(sc, world, m, s, ticks)).collect();
                let x = measure(runs.iter().flat_map(|r| r.battles.iter()));
                for key in [(sc, label), ("all", label)] {
                    let e = tot.entry(key).or_default();
                    e.0 += x.0; e.1 += x.1; e.2 += x.2; e.3 += x.3;
                }
                if m != Model::A { continue; }
                // stop rule
                let frac = |v: &[f64], pr: &dyn Fn(f64) -> bool| 100.0 * v.iter().filter(|y| pr(**y)).count() as f64 / v.len().max(1) as f64;
                let mut eo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                let mut legit: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                let mut calm: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
                let (mut lv, mut cnt) = (0, 0);
                let mut c = [0.0; 6];
                for r in &runs {
                    for (k, v) in &r.eo { eo.entry(k.clone()).or_default().extend(v); }
                    for (k, v) in &r.legit { legit.entry(k.clone()).or_default().extend(v); }
                    for (k, v) in &r.calm { let e = calm.entry(k.clone()).or_default(); e.0 += v.0; e.1 += v.1; e.2 += v.2; }
                    lv += r.living; cnt += r.ceiling_no_threat;
                    for (a, b) in c.iter_mut().zip(&r.corr) { *a += b; }
                    for d in r.declines.iter().filter(|d| d.1 == 2) { decl.0 += 1; decl.1 += d.0 as u64; }
                }
                let spread = |m: &BTreeMap<String, Vec<f64>>| { let v: Vec<f64> = m.values().map(|x| pct(x, 0.5)).collect(); v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min) };
                let c1 = eo.values().filter(|x| frac(x, &|y| y >= 99.0) < 20.0 && frac(x, &|y| y <= 1.0) < 20.0).count();
                let tm: Vec<f64> = tiers(sc).iter().map(|t| { let x: Vec<f64> = t.iter().filter_map(|a| eo.get(*a)).flatten().copied().collect(); pct(&x, 0.5) }).collect();
                let ordered = tm.windows(2).all(|w| w[0] > w[1]);
                let c5 = calm.values().filter(|v| v.0 > 0 && share(v.1, v.0) < 20.0 && share(v.2, v.0) < 20.0).count();
                let ncalm = calm.values().filter(|v| v.0 > 0).count();
                let (n, sx, sy, sxx, syy, sxy) = (c[0], c[1], c[2], c[3], c[4], c[5]);
                let corr = (n * sxy - sx * sy) / ((n * sxx - sx * sx).sqrt() * (n * syy - sy * sy).sqrt());
                let flags = [100 * c1 >= 80 * eo.len(), ordered, spread(&eo) >= 20.0, 100 * c5 >= 80 * ncalm, spread(&legit) >= 15.0, share(cnt, lv) < 30.0, corr >= 0.7];
                for (o, f) in stop_ok.iter_mut().zip(flags) { *o += f as usize; }
                stop_rows.push(format!("| {sc} | {world} | {c1} / {} · {} · {:.0} | {c5} / {ncalm} · {:.0} | {:.0} % · {corr:.2} |", eo.len(), if ordered { "ordered" } else { "**not ordered**" }, spread(&eo), spread(&legit), share(cnt, lv)));
                for seed in first..first + seeds {
                    total += 1;
                    let a = runs[(seed - first) as usize].fingerprint;
                    if run(sc, world, Model::Content, seed, ticks).fingerprint == a { same += 1; }
                }
            }
        }
    }
    println!("## 1. The corrected Ц4 measure (army ratio 0.8–1.25, quality gap ≥ 10)\n");
    println!("| set | (a): battles, won by higher quality | the formula promises | 2 SE | (a) ≥ promise − 2 SE | (b): battles, won | (a) − (b) | 2 SE of the difference | (a) − (b) ≥ 2 SE |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for set in ["all", "rome_375", "constantinople_1430", "milan_1477"] {
        let a = tot.get(&(set, "(a)")).copied().unwrap_or_default();
        let b = tot.get(&(set, "(b)")).copied().unwrap_or_default();
        if a.0 == 0 { println!("| {set} | 0 battles | | | | | | | |"); continue; }
        let (pa, pb) = (a.1 as f64 / a.0 as f64, b.1 as f64 / b.0.max(1) as f64);
        let promise = a.2 / a.0 as f64;
        let se = (a.3).sqrt() / a.0 as f64;
        let se_d = (pa * (1.0 - pa) / a.0 as f64 + pb * (1.0 - pb) / b.0.max(1) as f64).sqrt();
        println!("| {set} | {}: {:.1} % | {:.1} % | {:.1} | {} | {}: {:.1} % | {:+.1} | {:.1} | {} |", a.0, 100.0 * pa, 100.0 * promise, 200.0 * se,
            if pa >= promise - 2.0 * se { "yes" } else { "**no**" }, b.0, 100.0 * pb, 100.0 * (pa - pb), 200.0 * se_d, if pa - pb >= 2.0 * se_d { "yes" } else { "**no**" });
    }
    println!("\nThe decision is on the sum over all 10 worlds; the scenarios are for the record.\n");
    println!("## 2. Stop rule at (a), held-out seeds\n");
    println!("| scenario | world | Ц1: actors · tiers · eo spread | Ц5: actors · legitimacy spread | Ц6: ceiling without a threat · corr |");
    println!("|---|---|---|---|---|");
    for r in stop_rows { println!("{r}"); }
    println!("\nWorlds passing of {nworld}: Ц1 actors {} / tiers {} / spread {}; Ц5 actors {} / spread {}; Ц6 ceiling without a threat {} / corr {}; Ц6 decline, corrected, overall {:.0} % of {}.",
        stop_ok[0], stop_ok[1], stop_ok[2], stop_ok[3], stop_ok[4], stop_ok[5], stop_ok[6], share(decl.1, decl.0), decl.0);
    println!("\n## 3. Content check\n\nv2 as in the content against (a) set here: {same} of {total} runs identical, every actor metric every tick.");
}

fn c7(seeds: u64, ticks: u32) {
    println!("# Arithmetic for Ц7 — the stage 1 runs (seeds 0–{}), model (a), {ticks} ticks\n", seeds - 1);
    println!("Under full threat = T_p ≥ 90 at the end of the previous tick. A streak counts battles lost in a row while under full threat; a won battle, or a battle fought below full threat, resets it.\n");
    println!("| scenario | world | actor-ticks under full threat | battles there per actor-tick | lost there | games × actors reaching K = 1 / 2 / 3 / 4 / 5 / 6 lost in a row | median first tick for K = 2 / 3 / 4 / 5 |");
    println!("|---|---|---|---|---|---|---|");
    let mut who_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let runs: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::A, s, ticks)).collect();
            let ft: u64 = runs.iter().map(|r| r.full_threat_ticks).sum();
            let fb: u64 = runs.iter().map(|r| r.ft_battles).sum();
            let fl: u64 = runs.iter().map(|r| r.ft_lost).sum();
            let mut reach = [0u64; KMAX + 1];
            let mut first: Vec<Vec<f64>> = vec![Vec::new(); KMAX + 1];
            let mut per_actor: BTreeMap<String, [u64; KMAX + 1]> = BTreeMap::new();
            for r in &runs {
                for (id, f) in &r.streak_first {
                    for k in 1..=KMAX {
                        if let Some(t) = f[k] {
                            reach[k] += 1;
                            first[k].push(t as f64);
                            per_actor.entry(id.clone()).or_default()[k] += 1;
                        }
                    }
                }
            }
            println!("| {sc} | {world} | {ft} | {:.3} | {:.0} % | {} | {} |", fb as f64 / ft.max(1) as f64, share(fl, fb),
                (1..=6).map(|k| reach[k].to_string()).collect::<Vec<_>>().join(" / "),
                (2..=5).map(|k| if first[k].is_empty() { "—".into() } else { format!("{:.0}", pct(&first[k], 0.5)) }).collect::<Vec<_>>().join(" / "));
            let mut v: Vec<(String, [u64; KMAX + 1])> = per_actor.into_iter().filter(|x| x.1[3] > 0).collect();
            v.sort_by(|a, b| b.1[3].cmp(&a.1[3]));
            let cells: Vec<String> = v.iter().take(10).map(|(id, c)| format!("{id} {} / {} / {}", c[3], c[4], c[5])).collect();
            who_rows.push(format!("| {sc} | {world} | {} |", if cells.is_empty() { "—".into() } else { cells.join(", ") }));
        }
    }
    println!("\n## Who reaches K = 3 / 4 / 5 lost in a row under full threat (games of {seeds})\n");
    println!("| scenario | world | actors |");
    println!("|---|---|---|");
    for r in who_rows { println!("{r}"); }
}

/// The corrected Ц6 decline on the held-out seeds for v2 without the outcome (the content as at
/// `e84341c`) and with it: does the outcome move it, or is the shortfall already there?
fn decline(seeds: u64, ticks: u32) {
    println!("# Ц6's corrected decline on held-out seeds 100–{}, {ticks} ticks\n", 99 + seeds);
    println!("| scenario | world | without the outcome: counted, reached | with the outcome (a): counted, reached |");
    println!("|---|---|---|---|");
    let mut tot = [(0u64, 0u64); 2];
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let mut cells = Vec::new();
            for (i, m) in [Model::NoOutcome, Model::A].into_iter().enumerate() {
                let mut c = (0u64, 0u64);
                for seed in 100..100 + seeds {
                    for d in run(sc, world, m, seed, ticks).declines.iter().filter(|d| d.1 == 2) { c.0 += 1; c.1 += d.0 as u64; }
                }
                tot[i].0 += c.0; tot[i].1 += c.1;
                cells.push(format!("{}: {:.0} %", c.0, share(c.1, c.0)));
            }
            println!("| {sc} | {world} | {} | {} |", cells[0], cells[1]);
        }
    }
    println!("| **all** | | **{}: {:.1} %** | **{}: {:.1} %** |", tot[0].0, share(tot[0].1, tot[0].0), tot[1].0, share(tot[1].1, tot[1].0));
}

/// The Ц4 content check: v2 as the content stands against (a) set here, held-out seeds.
fn content(seeds: u64, ticks: u32) {
    let (mut same, mut total) = (0, 0);
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            for seed in 100..100 + seeds {
                total += 1;
                if run(sc, world, Model::Content, seed, ticks).fingerprint == run(sc, world, Model::A, seed, ticks).fingerprint { same += 1; }
            }
        }
    }
    println!("# Ц4 content check\n\nv2 as in the content against (a) set here, seeds 100–{}: {same} of {total} runs identical, every actor metric every tick.", 99 + seeds);
}

/// Ц7, the conqueror arithmetic (owner's grid): a streak is K battles lost in a row to the same
/// winner whose strength is at least R times the loser's (`S_w ≥ R × S_l`). Any other battle of
/// the actor — a win, a loss to someone else, a loss to a winner below R times — ends the streak;
/// a qualifying loss to a new winner starts a new one at 1. The actor is counted as falling on its
/// first streak of length K. From the protocols of (a), stage 1 seeds 0–29; nothing new is run
/// beyond those games, and deaths' second-order effects are not modelled.
fn c7b(seeds: u64, ticks: u32) {
    const RS: [f64; 3] = [3.0, 5.0, 10.0];
    const KS: [usize; 2] = [3, 5];
    const PEOPLES: [&str; 5] = ["alamanni", "vandals", "visigoths", "burgundians", "franks"];
    const WATCH: [&str; 7] = ["rome", "byzantium", "papacy", "venice", "savoy", "mantua", "ferrara"];
    println!("# Ц7 — one conqueror of overwhelming strength: K losses in a row to the same winner with S_w ≥ R × S_l\n");
    println!("Protocols of (a), seeds 0–{}, {ticks} ticks. An actor counts as falling on its first such streak (tick = the K-th loss).\n", seeds - 1);
    // (cell) -> rows
    let mut rows: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
    let mut verdict: BTreeMap<(usize, usize), Vec<(String, bool)>> = BTreeMap::new();
    let mut who: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let runs: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::A, s, ticks)).collect();
            for (ri, r) in RS.iter().enumerate() {
                for (ki, k) in KS.iter().enumerate() {
                    // actor -> first ticks over games
                    let mut falls: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                    let mut conquerors: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
                    for run in &runs {
                        let mut streak: BTreeMap<String, (String, usize)> = BTreeMap::new();
                        let mut fallen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                        for b in &run.battles {
                            let (w, l, s_w, s_l) = if b.attacker_won { (&b.attacker, &b.defender, b.strength_attacker, b.strength_defender) } else { (&b.defender, &b.attacker, b.strength_defender, b.strength_attacker) };
                            // the winner's own streak ends
                            streak.remove(w);
                            if fallen.contains(l) { continue; }
                            let qualifies = s_w >= r * s_l;
                            let e = streak.entry(l.clone()).or_insert((w.clone(), 0));
                            if qualifies {
                                if &e.0 == w { e.1 += 1; } else { *e = (w.clone(), 1); }
                            } else {
                                *e = (w.clone(), 0);
                            }
                            if e.1 >= *k {
                                fallen.insert(l.clone());
                                falls.entry(l.clone()).or_default().push(b.tick as f64);
                                *conquerors.entry(l.clone()).or_default().entry(w.clone()).or_default() += 1;
                            }
                        }
                    }
                    let games = |id: &str| falls.get(id).map_or(0, |v| v.len());
                    let med = |id: &str| falls.get(id).map_or("—".to_string(), |v| format!("{:.0}", pct(v, 0.5)));
                    let mut cells = Vec::new();
                    match sc {
                        "rome_375" => {
                            let per_game = PEOPLES.iter().map(|p| games(p)).sum::<usize>() as f64 / seeds as f64;
                            cells.push(format!("five peoples: {per_game:.1} a game ({})", PEOPLES.iter().map(|p| format!("{p} {} @{}", games(p), med(p))).collect::<Vec<_>>().join(", ")));
                            cells.push(format!("rome {} @{}", games("rome"), med("rome")));
                            verdict.entry((ri, ki)).or_default().push((format!("rome {world}: ≥ 3 of 5 a game"), per_game >= 3.0));
                        }
                        "constantinople_1430" => {
                            cells.push(format!("byzantium {} @{}", games("byzantium"), med("byzantium")));
                            if *world == "none" {
                                let m = falls.get("byzantium").map(|v| pct(v, 0.5)).unwrap_or(f64::NAN);
                                verdict.entry((ri, ki)).or_default().push(("constantinople none: Byzantium ≥ 20 / 30, median 40–59".into(), games("byzantium") >= 20 && (40.0..=59.0).contains(&m)));
                            }
                        }
                        _ => {
                            cells.push(format!("milan {} @{}", games("milan"), med("milan")));
                            verdict.entry((ri, ki)).or_default().push((format!("milan {world}: Milan ≤ 1, papacy ≤ 3, Venice ≤ 3"), games("milan") <= 1 && games("papacy") <= 3 && games("venice") <= 3));
                        }
                    }
                    for wch in WATCH.iter().filter(|w| !matches!(**w, "rome" | "byzantium") && sc == "milan_1477") {
                        cells.push(format!("{wch} {} @{}", games(wch), med(wch)));
                    }
                    let mut all: Vec<(String, usize)> = falls.iter().map(|(id, v)| (id.clone(), v.len())).collect();
                    all.sort_by(|a, b| b.1.cmp(&a.1));
                    let by: Vec<String> = all.iter().take(12).map(|(id, n)| {
                        let top = conquerors.get(id).and_then(|c| c.iter().max_by_key(|x| x.1)).map(|(w, _)| w.clone()).unwrap_or_default();
                        format!("{id} {n} (by {top})")
                    }).collect();
                    rows.entry((ri, ki)).or_default().push(format!("| {sc} | {world} | {} |", cells.join("; ")));
                    who.entry((ri, ki)).or_default().push(format!("| {sc} | {world} | {} |", if by.is_empty() { "—".into() } else { by.join(", ") }));
                }
            }
        }
    }
    println!("## Summary: does a cell pass items 1–3 of the Ц7 measure?\n");
    println!("| R | K | rome: ≥ 3 of 5 peoples a game, worlds | Byzantium none ≥ 20 / 30 at median 40–59 | milan: Milan ≤ 1, papacy ≤ 3, Venice ≤ 3, worlds | all |");
    println!("|---|---|---|---|---|---|");
    for (ri, r) in RS.iter().enumerate() {
        for (ki, k) in KS.iter().enumerate() {
            let v = &verdict[&(ri, ki)];
            let cnt = |p: &str| (v.iter().filter(|x| x.0.starts_with(p) && x.1).count(), v.iter().filter(|x| x.0.starts_with(p)).count());
            let (a, an) = cnt("rome");
            let (b, bn) = cnt("constantinople");
            let (c, cn) = cnt("milan");
            println!("| {r} | {k} | {a} / {an} | {} | {c} / {cn} | {} |", if b == bn { "yes" } else { "no" }, if a == an && b == bn && c == cn { "**yes**" } else { "no" });
        }
    }
    for (ri, r) in RS.iter().enumerate() {
        for (ki, k) in KS.iter().enumerate() {
            println!("\n## R = {r}, K = {k}\n");
            println!("| scenario | world | games of {seeds} @ median first tick |");
            println!("|---|---|---|");
            for row in &rows[&(ri, ki)] { println!("{row}"); }
            println!("\nWho falls (games, the most frequent conqueror):\n");
            println!("| scenario | world | actors |");
            println!("|---|---|---|");
            for row in &who[&(ri, ki)] { println!("{row}"); }
        }
    }
}

/// Ц7 with submission (owner's grid R ∈ {3, 5, 10} × K₁ ∈ {2, 3}): the same streak as `c7b`, but
/// its first completion makes the loser a vassal of that winner. Only the first submission per
/// actor and game is counted; the world after it (overlord and vassal stop fighting) is not
/// modelled, so neither death at K₂ nor the breaking of the bond can be read from these protocols.
fn vassal(seeds: u64, ticks: u32) {
    const RS: [f64; 3] = [3.0, 5.0, 10.0];
    const KS: [usize; 2] = [2, 3];
    const WATCH: [&str; 13] = ["byzantium", "serbia", "wallachia", "armenia", "sicily", "ostrogoths",
        "alamanni", "vandals", "visigoths", "burgundians", "franks", "papacy", "venice"];
    println!("# Ц7 with submission — the first streak of K₁ losses in a row to one winner with S_w ≥ R × S_l makes the loser its vassal\n");
    println!("Protocols of (a), seeds 0–{}, {ticks} ticks. Cells: games of {seeds} @ median tick of submission (the overlord most often).\n", seeds - 1);
    let mut rows: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let runs: Vec<Run> = (0..seeds).map(|s| run(sc, world, Model::A, s, ticks)).collect();
            for (ri, r) in RS.iter().enumerate() {
                for (ki, k) in KS.iter().enumerate() {
                    let mut subs: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                    let mut lords: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
                    for run in &runs {
                        let mut streak: BTreeMap<String, (String, usize)> = BTreeMap::new();
                        let mut bound: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                        for b in &run.battles {
                            let (w, l, s_w, s_l) = if b.attacker_won { (&b.attacker, &b.defender, b.strength_attacker, b.strength_defender) } else { (&b.defender, &b.attacker, b.strength_defender, b.strength_attacker) };
                            streak.remove(w);
                            if bound.contains(l) { continue; }
                            let e = streak.entry(l.clone()).or_insert((w.clone(), 0));
                            if s_w >= r * s_l {
                                if &e.0 == w { e.1 += 1; } else { *e = (w.clone(), 1); }
                            } else {
                                *e = (w.clone(), 0);
                            }
                            if e.1 >= *k {
                                bound.insert(l.clone());
                                subs.entry(l.clone()).or_default().push(b.tick as f64);
                                *lords.entry(l.clone()).or_default().entry(w.clone()).or_default() += 1;
                            }
                        }
                    }
                    let cell = |id: &str| -> Option<String> {
                        let v = subs.get(id)?;
                        let lord = lords.get(id).and_then(|c| c.iter().max_by_key(|x| x.1)).map(|(w, _)| w.clone()).unwrap_or_default();
                        Some(format!("{id} {} @{:.0} ({lord})", v.len(), pct(v, 0.5)))
                    };
                    let watched: Vec<String> = WATCH.iter().filter_map(|id| cell(id)).collect();
                    let milan = if sc == "milan_1477" { cell("milan").unwrap_or_else(|| "0".into()) } else { "—".into() };
                    let mut all: Vec<(&String, usize)> = subs.iter().map(|(id, v)| (id, v.len())).collect();
                    all.sort_by(|a, b| b.1.cmp(&a.1));
                    let others: Vec<String> = all.iter().filter(|(id, _)| !WATCH.contains(&id.as_str()) && id.as_str() != "milan").filter_map(|(id, _)| cell(id)).collect();
                    rows.entry((ri, ki)).or_default().push(format!("| {sc} | {world} | {} | {milan} | {} |",
                        if watched.is_empty() { "—".into() } else { watched.join(", ") }, if others.is_empty() { "—".into() } else { others.join(", ") }));
                }
            }
        }
    }
    for (ri, r) in RS.iter().enumerate() {
        for (ki, k) in KS.iter().enumerate() {
            println!("## R = {r}, K₁ = {k}\n");
            println!("| scenario | world | watched actors | Milan | others who submit |");
            println!("|---|---|---|---|---|");
            for row in &rows[&(ri, ki)] { println!("{row}"); }
            println!();
        }
    }
}
