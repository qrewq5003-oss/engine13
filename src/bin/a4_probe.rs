//! A4 stage 1 — milan's regency fork and the permanent `regency_crisis` (docs/TRIAGE.md).
//! Measurement only; nothing is chosen.
//!
//! Milan starts with `regency_crisis` (legitimacy −2, cohesion −1 every tick; never spreads,
//! never ends). The fork — `milan_regency_stabilizes` (legitimacy > 65 for 5 ticks) against
//! `milan_regency_crisis_deepens` (< 25 for 3) — is decided by that tag. Variants in memory
//! (built with `--features census`):
//!
//! - (а) the tag's modifiers halved: −1, −0.5 (`census::set_tag_scale_of`, the modifiers are
//!   integers);
//! - (б) the tag removed from Milan on tick 6 (1480, Ludovico Sforza takes the regency);
//! - (в) the tag removed when `milan_regency_crisis_deepens` fires;
//! - (г) the tag removed when either branch of the fork fires — only with `with-g`.
//!
//! The probe removes the tag (from `tags` and `actor_tags`) at the end of the tick the trigger
//! happens on; its last effect is that tick's, as a removal by a milestone or a dated
//! milestone would be. Per variant and milan world: Milan's legitimacy and cohesion at ticks
//! 25 / 50 / 150 and their share of living ticks at the floor (≤ 1) and the ceiling (≥ 99);
//! each branch of the fork (games, tick); Milan's deaths and all deaths; and Milan's
//! legitimacy sources per living tick after the tag is gone (the A37 write sink, asked
//! deltas, before the clamp), next to the base's over the same ticks.
//!
//! Usage: cargo run --release --features census --bin a4_probe -- [seeds] [ticks] [with-g]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const TAG: &str = "regency_crisis";
const STAB: &str = "milan_regency_stabilizes";
const DEEP: &str = "milan_regency_crisis_deepens";
const MARKS: [usize; 3] = [25, 50, 150];

#[derive(Clone, Copy, PartialEq)]
enum V { Base, Half, Date, Deepens, Either }

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

#[derive(Default)]
struct Run {
    series: Vec<Option<(f64, f64)>>,
    stab: Option<usize>,
    deep: Option<usize>,
    removed: Option<usize>,
    milan_dead: bool,
    deaths: u32,
    // legitimacy sources (asked) per tick, from tick `from` on
    sources: BTreeMap<String, f64>,
    source_ticks: u32,
    // legitimacy sources on ticks 0–6 (before (б)'s removal), whatever the variant
    early: BTreeMap<String, f64>,
}

fn run(world: &str, v: V, seed: u64, ticks: u32, from: Option<usize>) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, "milan_1477".into()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    census::set_tag_scale_of((v == V::Half).then(|| (TAG.to_string(), 0.5)));
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, "milan_1477"));
    let mut r = Run::default();
    let _ = census::take_writes();
    for _ in 0..ticks {
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let sc = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let writes = census::take_writes();
        let ws = st.world_state.as_mut().unwrap();
        let t = ws.tick as usize - 1;
        let fired = |id: &str| ws.milestone_events_fired.iter().any(|m| m == id);
        if r.stab.is_none() && fired(STAB) { r.stab = Some(t); }
        if r.deep.is_none() && fired(DEEP) { r.deep = Some(t); }
        let alive = ws.actors.contains_key("milan") && !ws.dead_actor_ids.contains("milan");
        // sources: from the tick after the removal (variants) or from the given tick (base)
        if alive && t < 7 {
            for w in writes.iter().filter(|w| w.actor == "milan" && w.metric == "legitimacy") {
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                *r.early.entry(src).or_default() += w.requested;
            }
        }
        let start = from.or(r.removed.map(|x| x + 1));
        if alive && start.is_some_and(|s| t >= s) {
            r.source_ticks += 1;
            for w in writes.iter().filter(|w| w.actor == "milan" && w.metric == "legitimacy") {
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                *r.sources.entry(src).or_default() += w.requested;
            }
        }
        let trigger = match v {
            V::Date => t == 6,
            V::Deepens => r.deep.is_some(),
            V::Either => r.deep.is_some() || r.stab.is_some(),
            _ => false,
        };
        if trigger && r.removed.is_none() {
            if let Some(m) = ws.actors.get_mut("milan") {
                m.tags.retain(|x| x != TAG);
                m.actor_tags.remove(TAG);
            }
            r.removed = Some(t);
        }
        r.series.push(ws.actors.get("milan").filter(|_| alive).map(|m| (m.get_metric("legitimacy"), m.get_metric("cohesion"))));
    }
    let ws = st.world_state.as_ref().unwrap();
    r.milan_dead = ws.dead_actor_ids.contains("milan");
    r.deaths = ws.dead_actors.len() as u32;
    r
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let with_g = args.get(3).is_some_and(|a| a == "with-g");
    census::enable_writes();
    census::watch_all_metrics(true);
    let mut variants = vec![(V::Base, "base"), (V::Half, "(а) halved"), (V::Date, "(б) removed on tick 6"), (V::Deepens, "(в) removed when deepens fires")];
    if with_g {
        variants.push((V::Either, "(г) removed when either branch fires"));
    }
    println!("# A4 stage 1 — milan regency, {seeds} seeds × {ticks} ticks per world\n");
    let mut metric_rows = Vec::new();
    let mut source_rows = Vec::new();
    let mut early_rows = Vec::new();
    println!("| world | variant | stabilizes: games (tick p10/50/90) | deepens: games (tick p10/50/90) | tag removed: games (tick p10/50/90) | legitimacy at floor / ceiling | cohesion at floor / ceiling | Milan dies | all deaths |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for world in ["none", "aggressive"] {
        // the base's sources over the ticks the variants measure: from tick 7 (after (б)'s removal)
        for (v, label) in &variants {
            let from = (*v == V::Base || *v == V::Half).then_some(7);
            let runs: Vec<Run> = (0..seeds).map(|s| run(world, *v, s, ticks, from)).collect();
            let ticks_of = |f: &dyn Fn(&Run) -> Option<usize>| -> (usize, Vec<f64>) {
                let v: Vec<f64> = runs.iter().filter_map(|r| f(r).map(|t| t as f64)).collect();
                (v.len(), v)
            };
            let (ns, ts) = ticks_of(&|r| r.stab);
            let (nd, td) = ticks_of(&|r| r.deep);
            let (nr, tr) = ticks_of(&|r| r.removed);
            let share = |k: usize, pred: &dyn Fn(f64) -> bool| {
                let (mut hit, mut n) = (0u64, 0u64);
                for r in &runs {
                    for s in r.series.iter().flatten() {
                        n += 1;
                        let x = if k == 0 { s.0 } else { s.1 };
                        if pred(x) { hit += 1; }
                    }
                }
                100.0 * hit as f64 / n.max(1) as f64
            };
            println!("| {world} | {label} | {ns} ({}) | {nd} ({}) | {} | {:.0} % / {:.0} % | {:.0} % / {:.0} % | {} | {} |",
                q(&ts), q(&td), if *v == V::Base || *v == V::Half { "—".into() } else { format!("{nr} ({})", q(&tr)) },
                share(0, &|x| x <= 1.0), share(0, &|x| x >= 99.0), share(1, &|x| x <= 1.0), share(1, &|x| x >= 99.0),
                runs.iter().filter(|r| r.milan_dead).count(), runs.iter().map(|r| r.deaths).sum::<u32>());
            for &t in &MARKS {
                let l: Vec<f64> = runs.iter().filter_map(|r| r.series.get(t).copied().flatten().map(|s| s.0)).collect();
                let c: Vec<f64> = runs.iter().filter_map(|r| r.series.get(t).copied().flatten().map(|s| s.1)).collect();
                metric_rows.push(format!("| {world} | {label} | {t} | {} | {} | {} |", l.len(), q(&l), q(&c)));
            }
            let mut pooled: BTreeMap<String, f64> = BTreeMap::new();
            let n: u32 = runs.iter().map(|r| r.source_ticks).sum();
            for r in &runs {
                for (k, x) in &r.sources { *pooled.entry(k.clone()).or_default() += x; }
            }
            let mut top: Vec<(String, f64)> = pooled.into_iter().map(|(k, x)| (k, x / n.max(1) as f64)).collect();
            top.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());
            let total: f64 = top.iter().map(|x| x.1).sum();
            let cells: Vec<String> = top.iter().filter(|x| x.1.abs() >= 0.01).take(10).map(|(k, x)| format!("{k} {x:+.2}")).collect();
            source_rows.push(format!("| {world} | {label} | {n} | {total:+.2} | {} |", cells.join(", ")));
            if *v == V::Base {
                let mut e: BTreeMap<String, f64> = BTreeMap::new();
                for r in &runs {
                    for (k, x) in &r.early { *e.entry(k.clone()).or_default() += x; }
                }
                let mut top: Vec<(String, f64)> = e.into_iter().map(|(k, x)| (k, x / seeds as f64)).collect();
                top.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());
                let total: f64 = top.iter().map(|x| x.1).sum();
                let cells: Vec<String> = top.iter().filter(|x| x.1.abs() >= 0.05).take(10).map(|(k, x)| format!("{k} {x:+.1}")).collect();
                early_rows.push(format!("| {world} | {total:+.1} | {} |", cells.join(", ")));
            }
        }
    }
    println!("\n## Milan's legitimacy and cohesion (end of tick, alive), p10/p50/p90\n");
    println!("| world | variant | tick | Milan alive | legitimacy | cohesion |");
    println!("|---|---|---|---|---|---|");
    for row in metric_rows { println!("{row}"); }
    println!("\n## Milan's legitimacy sources per living tick (asked, before the clamp): base and (а) from tick 7, the removal variants from the tick after the removal\n");
    println!("| world | variant | living ticks counted | sum per tick | sources per tick |");
    println!("|---|---|---|---|---|");
    for row in source_rows { println!("{row}"); }
    println!("\n## Milan's legitimacy sources on ticks 0–6, base, sum per game (asked, before the clamp)\n");
    println!("| world | sum per game | sources per game |");
    println!("|---|---|---|");
    for row in early_rows { println!("{row}"); }
    census::set_tag_scale_of(None);
}
