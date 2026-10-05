//! Stage 1 for three open items in one probe (docs/TRIAGE.md, owner's decisions 2026-10-05).
//! Measurement only; nothing is chosen. Built with `--features census`.
//!
//! - **A19** — the second cycle `cohesion ↔ legitimacy`. For every actor of the three
//!   scenarios: the share of living ticks in the engine's collapse zone (any of the three
//!   paths of `check_collapses`: classic, internal, conquest), in `cohesion < 15` (the
//!   classic gate the old 76.8 % was about) and in the mutual-sink area
//!   (`cohesion < 50 ∧ legitimacy < 50`); what `legitimacy_to_cohesion` and
//!   `cohesion_to_legitimacy` write per living tick (the A37 write sink, landed deltas);
//!   and the zone again with both rules removed from the scenario in memory. The owner's
//!   rule: if the actor stays in the zone without the two rules, the zone is held by the
//!   floor of legitimacy (A46), not by the cycle → the brief.
//! - **A20b + A41** — `flood` (common event, 0.08: `economic_output −12`, `population −15`,
//!   `cohesion −5`) muted: it still rolls, picks its target and is logged, but its effects
//!   are not applied (`census::set_muted_event`), so the random stream is the base's and the
//!   difference is the event's effect alone. Per world: deaths (all, key), the share of
//!   actor-ticks in the collapse zone, `economic_output` at the ceiling, the A10 row, the A35
//!   row and the split on tick 40. The owner's rule: deaths move by ≤ 2 in every world and
//!   the A10, A35 and split rows do not change → the brief.
//! - **A43, `late_sassanids`** — how often `sassanids` die, and when (rome, four worlds).
//!
//! Zone membership is read at the end of each tick; the engine checks it mid-tick (in
//! `phase_collapses`, after the clamp), so the share is that of tick-end states.
//!
//! Usage: cargo run --release --features census --bin stage1_open_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const RULES: [&str; 2] = ["legitimacy_to_cohesion", "cohesion_to_legitimacy"];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

fn key_actors(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["rome", "sassanids"],
        "milan_1477" => &["milan"],
        _ => &["byzantium", "ottomans"],
    }
}

#[derive(Clone, Copy, PartialEq)]
enum V { Base, NoRules, NoFlood }

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

#[derive(Default, Clone)]
struct ActorAcc {
    living: u64,
    zone: u64,
    coh15: u64,
    mutual: u64,
    // landed writes of the two rules: (to cohesion, to legitimacy)
    rule_writes: (f64, f64),
}

#[derive(Default)]
struct Run {
    actors: BTreeMap<String, ActorAcc>,
    deaths: u32,
    key_dead: BTreeMap<String, Option<u32>>,
    win: Option<u32>,
    split40: bool,
    eo_ceiling: (u64, u64),
    floods: u32,
}

fn in_zone(ws: &engine13::core::WorldState, a: &engine13::core::Actor) -> bool {
    let (l, c, ep) = (a.get_metric("legitimacy"), a.get_metric("cohesion"), a.get_metric("external_pressure"));
    let min = engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY;
    let besieged = a.neighbors.iter().any(|n| {
        n.distance == 1 && ws.actors.get(&n.id).is_some_and(|nb| nb.get_metric("military_size") >= min)
    });
    (l < 10.0 && c < 15.0 && ep > 85.0) || (l < 5.0 && c < 8.0) || (a.get_metric("military_size") < min && l < 10.0 && ep > 85.0 && besieged)
}

fn run(sc: &str, world: &str, v: V, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    if v == V::NoRules {
        st.current_scenario.as_mut().unwrap().dependencies.retain(|d| !RULES.contains(&d.id.as_str()));
    }
    census::set_muted_event((v == V::NoFlood).then(|| "flood".to_string()));
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run::default();
    for k in key_actors(sc) {
        r.key_dead.insert(k.to_string(), None);
    }
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
        let writes = census::take_writes();
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        for w in &writes {
            let Some(src) = w.source.as_deref() else { continue };
            for (i, rule) in RULES.iter().enumerate() {
                if src == format!("dependency {rule}") {
                    let acc = r.actors.entry(w.actor.clone()).or_default();
                    if i == 0 { acc.rule_writes.0 += w.applied } else { acc.rule_writes.1 += w.applied }
                }
            }
        }
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            if ws.dead_actor_ids.contains(id) {
                continue;
            }
            let a = &ws.actors[id];
            let acc = r.actors.entry(id.clone()).or_default();
            acc.living += 1;
            if in_zone(ws, a) { acc.zone += 1; }
            if a.get_metric("cohesion") < 15.0 { acc.coh15 += 1; }
            if a.get_metric("cohesion") < 50.0 && a.get_metric("legitimacy") < 50.0 { acc.mutual += 1; }
            r.eo_ceiling.1 += 1;
            if a.get_metric("economic_output") >= 99.0 { r.eo_ceiling.0 += 1; }
        }
        for (k, d) in r.key_dead.iter_mut() {
            if d.is_none() && ws.dead_actor_ids.contains(k) { *d = Some(t); }
        }
        if r.win.is_none() && ws.victory_achieved { r.win = Some(t); }
        if t == 40 && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { r.split40 = true; }
    }
    r.deaths = st.world_state.as_ref().unwrap().dead_actors.len() as u32;
    r.floods = st.event_log.events.iter().filter(|e| e.id == "flood").count() as u32;
    census::set_muted_event(None);
    r
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    println!("# Stage 1: A19, A20b + A41, late_sassanids — {seeds} seeds × {ticks} ticks per world\n");

    let mut a19_rows = Vec::new();
    let mut flood_rows = Vec::new();
    let mut sass_rows = Vec::new();
    let mut zone_rows = Vec::new();
    let mut paired_rows = Vec::new();
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            let base: Vec<Run> = (0..seeds).map(|s| run(sc, world, V::Base, s, ticks)).collect();
            let norules: Vec<Run> = (0..seeds).map(|s| run(sc, world, V::NoRules, s, ticks)).collect();
            let noflood: Vec<Run> = (0..seeds).map(|s| run(sc, world, V::NoFlood, s, ticks)).collect();
            let pool = |runs: &[Run]| {
                let mut m: BTreeMap<String, ActorAcc> = BTreeMap::new();
                for r in runs {
                    for (k, a) in &r.actors {
                        let e = m.entry(k.clone()).or_default();
                        e.living += a.living; e.zone += a.zone; e.coh15 += a.coh15; e.mutual += a.mutual;
                        e.rule_writes.0 += a.rule_writes.0; e.rule_writes.1 += a.rule_writes.1;
                    }
                }
                m
            };
            let (pb, pn, pf) = (pool(&base), pool(&norules), pool(&noflood));
            let pct = |x: u64, n: u64| 100.0 * x as f64 / n.max(1) as f64;
            for (id, a) in &pb {
                let share = pct(a.zone, a.living);
                let coh = pct(a.coh15, a.living);
                if share >= 5.0 || coh >= 20.0 {
                    let n = pn.get(id).cloned().unwrap_or_default();
                    a19_rows.push(format!("| {sc} | {world} | {id} | {} | {share:.1} % | {:.1} % | {coh:.1} % / {:.1} % | {:.1} % | {:+.3} / {:+.3} |",
                        a.living, pct(n.zone, n.living), pct(n.coh15, n.living), pct(a.mutual, a.living),
                        a.rule_writes.0 / a.living.max(1) as f64, a.rule_writes.1 / a.living.max(1) as f64));
                }
            }
            let total = |m: &BTreeMap<String, ActorAcc>| {
                let (z, n) = m.values().fold((0, 0), |s, a| (s.0 + a.zone, s.1 + a.living));
                pct(z, n)
            };
            zone_rows.push(format!("| {sc} | {world} | {:.1} % | {:.1} % | {:.1} % |", total(&pb), total(&pn), total(&pf)));

            let row = |runs: &[Run]| {
                let deaths: u32 = runs.iter().map(|r| r.deaths).sum();
                let keys: Vec<String> = key_actors(sc).iter().map(|k| format!("{k} {}", runs.iter().filter(|r| r.key_dead[*k].is_some()).count())).collect();
                let wins: Vec<f64> = runs.iter().filter_map(|r| r.win.map(|t| t as f64)).collect();
                let mut per: BTreeMap<u32, u32> = BTreeMap::new();
                for t in &wins { *per.entry(*t as u32).or_default() += 1; }
                let top = per.values().max().copied().unwrap_or(0);
                let on = wins.iter().filter(|t| (40.0..=43.0).contains(*t)).count();
                let split = runs.iter().filter(|r| r.split40).count();
                let (ce, ne) = runs.iter().fold((0, 0), |s, r| (s.0 + r.eo_ceiling.0, s.1 + r.eo_ceiling.1));
                let floods: u32 = runs.iter().map(|r| r.floods).sum();
                let wins_s = if wins.is_empty() { "0".to_string() } else {
                    format!("{} ({}; 40–43: {on}; top tick {:.0} %)", wins.len(), q(&wins), 100.0 * top as f64 / wins.len() as f64)
                };
                (deaths, format!("{} | {} | {split} | {:.1} % | {floods}", keys.join(", "), wins_s, pct(ce, ne)))
            };
            // paired per seed: the same seed with and without the change
            let paired = |other: &[Run]| {
                let d: Vec<f64> = base.iter().zip(other).map(|(a, b)| b.deaths as f64 - a.deaths as f64).collect();
                let n = d.len() as f64;
                let mean = d.iter().sum::<f64>() / n;
                let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
                let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
                let changed = d.iter().filter(|x| **x != 0.0).count();
                format!("{mean:+.2} ± {sd:.2} per seed (t = {t:+.1}; seeds changed {changed}/{})", d.len())
            };
            paired_rows.push(format!("| {sc} | {world} | {} | {} |", paired(&noflood), paired(&norules)));
            let (db, rb) = row(&base);
            let (df, rf) = row(&noflood);
            flood_rows.push(format!("| {sc} | {world} | base | {db} | — | {rb} | {:.1} % |", total(&pb)));
            flood_rows.push(format!("| {sc} | {world} | flood muted | {df} | {:+} | {rf} | {:.1} % |", df as i64 - db as i64, total(&pf)));
            if sc == "rome_375" {
                let t: Vec<f64> = base.iter().filter_map(|r| r.key_dead["sassanids"].map(|t| t as f64)).collect();
                sass_rows.push(format!("| {world} | {} / {seeds} | {} |", t.len(), q(&t)));
            }
        }
    }

    println!("## A19: actors with ≥ 5 % of living ticks in the collapse zone or ≥ 20 % at `cohesion < 15` (base)\n");
    println!("| scenario | world | actor | living ticks | in zone | in zone without the two rules | cohesion < 15: base / without the rules | mutual area (c < 50 ∧ l < 50) | landed per living tick: `legitimacy_to_cohesion` → cohesion / `cohesion_to_legitimacy` → legitimacy |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in a19_rows { println!("{r}"); }
    println!("\n## Share of all living actor-ticks in the collapse zone\n");
    println!("| scenario | world | base | without the two rules | flood muted |");
    println!("|---|---|---|---|---|");
    for r in zone_rows { println!("{r}"); }
    println!("\n## A20b + A41: `flood` muted\n");
    println!("| scenario | world | variant | deaths | Δ | key deaths | wins (tick p10/50/90; on 40–43; largest share on one tick) | split on 40 | `economic_output` at ceiling | floods fired | in zone |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for r in flood_rows { println!("{r}"); }
    println!("\n## Deaths, paired by seed: the change minus the base\n");
    println!("| scenario | world | flood muted | the two rules removed |");
    println!("|---|---|---|---|");
    for r in paired_rows { println!("{r}"); }
    println!("\n## A43: `sassanids` deaths (rome, base)\n");
    println!("| world | games | tick p10/50/90 |");
    println!("|---|---|---|");
    for r in sass_rows { println!("{r}"); }
}
