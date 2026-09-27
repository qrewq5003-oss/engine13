//! Column-A probe — the measured basis for A1, A3 and A4 (docs/TRIAGE.md).
//!
//! The owner's decisions of 2026-09-21 set thresholds from "the upper quarter of the
//! observed maximum". That rule does not tell a peak the game reached from a value the
//! world *starts* at: A3's 120 is the Huns' starting `military_size`, so `>= 90` holds
//! on tick 0. This probe reports, per world, the start value, the maximum and the first
//! tick a candidate gate holds, and how often the authored content actually fires.
//!
//! Played worlds go through `application::scripted::play_scripted_tick` (A29).
//!
//! Also A2's `family_falls` and A31's `wallachia_emerges`: two milestones open on tick 0.
//!
//! Usage: cargo run --release --bin cola_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::MetricRef;
use rand::SeedableRng;

struct Row {
    start: f64,
    max: f64,
    first: Vec<Option<u32>>, // first tick each gate holds
    fired: Option<u32>,      // first tick the authored event appears in the log
}

fn run(scenario: &str, strategy: Option<&str>, seed: u64, ticks: u32, metric: &str, gates: &[f64], event: &str) -> Row {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, scenario.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    let strategy = strategy.map(|s| ScriptedStrategy::from_str(s, scenario));
    let m = MetricRef::literal(metric);
    let read = |st: &engine13::AppState| m.get(st.world_state.as_ref().unwrap());
    let start = read(&st);
    let mut row = Row { start, max: start, first: vec![None; gates.len()], fired: None };
    for t in 0..ticks {
        let v = read(&st);
        row.max = row.max.max(v);
        for (i, g) in gates.iter().enumerate() {
            if row.first[i].is_none() && v >= *g {
                row.first[i] = Some(t);
            }
        }
        match &strategy {
            Some(s) => {
                play_scripted_tick(&mut st, s);
            }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let sc = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        if row.fired.is_none() && st.event_log.events.iter().any(|e| e.id == event) {
            row.fired = Some(t);
        }
    }
    row
}

fn median(mut v: Vec<u32>) -> String {
    if v.is_empty() {
        return "—".into();
    }
    v.sort();
    v[v.len() / 2].to_string()
}

#[allow(clippy::too_many_arguments)]
fn report(item: &str, scenario: &str, strategies: &[Option<&str>], metric: &str, gates: &[f64], event: &str, seeds: u64, ticks: u32) {
    println!("== {item}: {metric} in {scenario}, event `{event}` ({seeds} seeds × {ticks} ticks)");
    for strat in strategies {
        let rows: Vec<Row> = (0..seeds).map(|s| run(scenario, *strat, s, ticks, metric, gates, event)).collect();
        let start = rows.iter().map(|r| r.start).fold(f64::NAN, f64::min);
        let max = rows.iter().map(|r| r.max).fold(f64::MIN, f64::max);
        print!("  {:<10} start {:>7.2}  max {:>7.2} |", strat.unwrap_or("no player"), start, max);
        for (i, g) in gates.iter().enumerate() {
            let hits: Vec<u32> = rows.iter().filter_map(|r| r.first[i]).collect();
            print!("  >= {g}: {}/{} (tick {})", hits.len(), seeds, median(hits));
        }
        let fired: Vec<u32> = rows.iter().filter_map(|r| r.fired).collect();
        println!("  | fired {}/{} (tick {})", fired.len(), seeds, median(fired));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let rome = [None, Some("balanced"), Some("influence"), Some("wealth")];
    report("A1 senator_bribe", "rome_375", &rome, "family:wealth", &[60.0, 200.0], "senator_bribe", seeds, ticks);
    report("A3 huns_visible", "rome_375", &rome, "actor:huns.military_size", &[90.0, 200.0], "huns_visible", seeds, ticks);
    // Same class as A3, found by the guard `metric_milestones_are_closed_in_the_starting_world`.
    report("A2 family_falls", "rome_375", &rome, "family:influence", &[5.0], "family_falls", seeds, ticks);
    report("A31 wallachia_emerges", "constantinople_1430", &[None, Some("balanced"), Some("diplomacy"), Some("military")], "actor:ottomans.military_size", &[70.0], "wallachia_emerges", seeds, ticks);
    report("A4 milan_regency_stabilizes", "milan_1477", &[None, Some("aggressive")], "actor:milan.legitimacy", &[48.0, 65.0], "milan_regency_stabilizes", seeds, ticks);
}
