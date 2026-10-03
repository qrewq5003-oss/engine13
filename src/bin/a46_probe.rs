//! A46 stage 1 — saturation census (docs/TRIAGE.md, «Калибровка насыщений»). Measurement only.
//!
//! For every metric of every actor — and the family and global metrics — in all three
//! scenarios and every world (no player and each scripted strategy), over living ticks only:
//!
//! - the share of ticks at the ceiling (≥ 99) and at the floor (≤ 1);
//! - the drift: mean change per living tick in three phases — the first 50 ticks, the middle
//!   (50–249), the tail (250–299);
//! - the clamp loss: of the inflow asked for, the share cut at the ceiling; of the outflow
//!   asked for, the share cut at the floor. Built with `--features census`: every write is
//!   recorded where it happens with the delta asked and the delta that landed (the A37 sink,
//!   watching every metric), and completeness is checked — the landed deltas of a tick sum
//!   to the tick's change for every container, or the count of mismatches says so.
//!
//! Scales: `legitimacy`, `cohesion`, `military_quality`, `economic_output`,
//! `external_pressure` and every family and global metric live on 0..100 (`clamp_metrics`,
//! `MetricRef::apply`); `military_size` and `population` have a floor at 0 only; `treasury`
//! has no bound. The ceiling share is printed for bounded metrics only.
//!
//! Output: a summary first — every (scenario, carrier, metric) spending more than half of its
//! living ticks at the ceiling or the floor, pooled over the worlds, with the per-world
//! shares — then the full table.
//!
//! Usage: cargo run --release --features census --bin a46_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap};

const BOUNDED: &[&str] = &["legitimacy", "cohesion", "military_quality", "economic_output", "external_pressure"];
const FLOOR_ONLY: &[&str] = &["military_size", "population"];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

#[derive(Default, Clone)]
struct Acc {
    alive: u64,
    ceil: u64,
    floor: u64,
    // per phase: (Σ change, ticks)
    phase: [(f64, u64); 3],
    asked_in: f64,
    cut_ceiling: f64,
    asked_out: f64,
    cut_floor: f64,
}

fn scale(carrier: &str, metric: &str) -> &'static str {
    if carrier == "family" || carrier == "global" || BOUNDED.contains(&metric) {
        "0..100"
    } else if FLOOR_ONLY.contains(&metric) {
        "≥ 0"
    } else {
        "none"
    }
}

/// Every (carrier, metric) → value: actors by id, the family as `family`, globals as `global`.
fn snapshot(ws: &engine13::core::WorldState) -> HashMap<(String, String), f64> {
    let mut m = HashMap::new();
    for (id, a) in &ws.actors {
        for (k, v) in &a.metrics {
            m.insert((id.clone(), k.clone()), *v);
        }
    }
    if let Some(fs) = &ws.family_state {
        for (k, v) in &fs.metrics {
            m.insert(("family".to_string(), k.clone()), *v);
        }
    }
    for (k, v) in &ws.global_metrics {
        m.insert(("global".to_string(), k.clone()), *v);
    }
    m
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    // (scenario, carrier, metric) -> world -> Acc
    let mut table: BTreeMap<(String, String, String), BTreeMap<String, Acc>> = BTreeMap::new();
    let (mut checked, mut mismatches, mut worst) = (0u64, 0u64, 0.0f64);
    census::enable_writes();
    census::watch_all_metrics(true);
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, sc));
                let _ = census::take_writes();
                for t in 0..ticks {
                    let before = snapshot(st.world_state.as_ref().unwrap());
                    match &strategy {
                        Some(s) => { play_scripted_tick(&mut st, s); }
                        None => {
                            let ws = st.world_state.as_mut().unwrap();
                            let scn = st.current_scenario.as_ref().unwrap();
                            engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
                        }
                    }
                    let writes = census::take_writes();
                    let after = snapshot(st.world_state.as_ref().unwrap());
                    let phase = if t < 50 { 0 } else if t < 250 { 1 } else { 2 };
                    let mut landed: HashMap<(&str, &str), f64> = HashMap::new();
                    for w in &writes {
                        *landed.entry((w.actor.as_str(), w.metric.as_str())).or_default() += w.applied;
                    }
                    for ((carrier, metric), v1) in &after {
                        let Some(v0) = before.get(&(carrier.clone(), metric.clone())) else { continue };
                        // completeness
                        let s = landed.get(&(carrier.as_str(), metric.as_str())).copied().unwrap_or(0.0);
                        let err = ((v1 - v0) - s).abs();
                        checked += 1;
                        worst = worst.max(err);
                        if err > 1e-6 * (1.0 + v1.abs()) {
                            mismatches += 1;
                            if std::env::var("A46_DEBUG").is_ok() { eprintln!("MISMATCH {sc} {world} {seed} t{t} {carrier}.{metric} change {:.4} landed {:.4}", v1 - v0, s); }
                        }
                        let a = table.entry((sc.to_string(), carrier.clone(), metric.clone())).or_default().entry(world.to_string()).or_default();
                        a.alive += 1;
                        if *v1 >= 99.0 { a.ceil += 1; }
                        if *v1 <= 1.0 { a.floor += 1; }
                        a.phase[phase].0 += v1 - v0;
                        a.phase[phase].1 += 1;
                    }
                    for w in &writes {
                        // the split rewrites the seat wholesale: structure, not inflow
                        if w.source.as_deref() == Some("seat_split") { continue; }
                        let key = (sc.to_string(), w.actor.clone(), w.metric.clone());
                        let Some(a) = table.get_mut(&key).and_then(|m| m.get_mut(*world)) else { continue };
                        if w.requested > 0.0 { a.asked_in += w.requested; }
                        if w.requested < 0.0 { a.asked_out += -w.requested; }
                        let cut = w.requested - w.applied;
                        if cut > 1e-12 { a.cut_ceiling += cut; }
                        if cut < -1e-12 { a.cut_floor += -cut; }
                    }
                }
            }
        }
    }
    let pct = |x: u64, n: u64| if n == 0 { 0.0 } else { 100.0 * x as f64 / n as f64 };
    let pooled = |m: &BTreeMap<String, Acc>| {
        let mut p = Acc::default();
        for a in m.values() {
            p.alive += a.alive; p.ceil += a.ceil; p.floor += a.floor;
            for i in 0..3 { p.phase[i].0 += a.phase[i].0; p.phase[i].1 += a.phase[i].1; }
            p.asked_in += a.asked_in; p.cut_ceiling += a.cut_ceiling; p.asked_out += a.asked_out; p.cut_floor += a.cut_floor;
        }
        p
    };
    println!("# A46 stage 1 — saturation census, {seeds} seeds × {ticks} ticks per world\n");
    println!("completeness: {mismatches} of {checked} container-metric-ticks where landed writes ≠ change (worst {worst:.2e})\n");
    println!("## Summary: more than half of the living ticks at the ceiling (≥ 99) or the floor (≤ 1), pooled over the worlds\n");
    println!("| scenario | carrier | metric | scale | at ceiling | at floor | per world (ceiling / floor) |");
    println!("|---|---|---|---|---|---|---|");
    for ((sc, carrier, metric), m) in &table {
        let p = pooled(m);
        let sc_ = scale(carrier, metric);
        let c = if sc_ == "0..100" { pct(p.ceil, p.alive) } else { 0.0 };
        let f = if sc_ != "none" { pct(p.floor, p.alive) } else { 0.0 };
        if c > 50.0 || f > 50.0 {
            let per: Vec<String> = m.iter().map(|(w, a)| format!("{w} {:.0}/{:.0}", if sc_ == "0..100" { pct(a.ceil, a.alive) } else { 0.0 }, pct(a.floor, a.alive))).collect();
            println!("| {sc} | {carrier} | {metric} | {sc_} | {c:.1} % | {f:.1} % | {} |", per.join(" · "));
        }
    }
    println!("\n## Full table, pooled over the worlds\n");
    println!("| scenario | carrier | metric | scale | living ticks | at ceiling | at floor | drift per tick: first 50 / middle / tail | inflow cut at ceiling | outflow cut at floor |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for ((sc, carrier, metric), m) in &table {
        let p = pooled(m);
        let sc_ = scale(carrier, metric);
        let d: Vec<String> = p.phase.iter().map(|(s, n)| if *n == 0 { "—".into() } else { format!("{:+.3}", s / *n as f64) }).collect();
        let cin = if p.asked_in > 0.0 { format!("{:.1} %", 100.0 * p.cut_ceiling / p.asked_in) } else { "—".into() };
        let cout = if p.asked_out > 0.0 { format!("{:.1} %", 100.0 * p.cut_floor / p.asked_out) } else { "—".into() };
        let c = if sc_ == "0..100" { format!("{:.1} %", pct(p.ceil, p.alive)) } else { "—".into() };
        let f = if sc_ != "none" { format!("{:.1} %", pct(p.floor, p.alive)) } else { "—".into() };
        println!("| {sc} | {carrier} | {metric} | {sc_} | {} | {c} | {f} | {} | {cin} | {cout} |", p.alive, d.join(" / "));
    }
}
