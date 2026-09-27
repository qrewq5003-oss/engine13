//! Drift probe — stage 1 of B21 (docs/TRIAGE.md).
//!
//! The same seed gives the same game in discrete terms, but in rome the world's `f64`
//! values differ between processes in the last bit (measured by the owner: 22 of 30
//! seeds over six processes, 0 in constantinople and milan). The per-process part is
//! the `HashMap` hasher seed, so some arithmetic runs in iteration order.
//!
//! This probe prints, per tick, every number the world holds, bit-exact, plus the
//! engine's own trace of dependency and auto-delta applications (`engine::trace`, which
//! emits the value it just used — nothing re-derived). Run it in several processes and
//! `diff` the outputs: the first differing line names the tick, and whether a traced
//! rule already saw a different input or only the tick-end state differs.
//!
//! Trace rows inside one tick are sorted, because their emission order itself follows
//! `HashMap` iteration and would differ without any numeric drift.
//!
//! Read-only: no player, RNG drawn only by the engine, sinks off in shipped code.
//!
//! Usage: cargo run --release --bin drift_probe -- <scenario> <seed> <ticks>

use engine13::db::Db;
use engine13::engine::trace;
use rand::SeedableRng;

fn bits(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario = args.get(1).cloned().unwrap_or_else(|| "rome_375".to_string());
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100);

    let db = Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, scenario.clone()).expect("scenario");
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    trace::enable();

    for t in 0..ticks {
        {
            let ws = st.world_state.as_mut().unwrap();
            let sc = st.current_scenario.as_ref().unwrap();
            engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
        }

        let mut lines: Vec<String> = trace::take_auto_deltas()
            .into_iter()
            .map(|r| format!("{t} A {:>3} {} applied={}", r.index, r.metric, bits(r.applied)))
            .collect();
        lines.sort();
        let mut deps: Vec<String> = trace::take_dependencies()
            .into_iter()
            .map(|r| format!("{t} D {} {} from={} to_before={} delta={}", r.actor, r.rule, bits(r.from_val), bits(r.to_before), bits(r.delta)))
            .collect();
        deps.sort();
        lines.extend(deps);

        let ws = st.world_state.as_ref().unwrap();
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            let a = &ws.actors[id];
            let mut ms: Vec<(&String, &f64)> = a.metrics.iter().collect();
            ms.sort_by(|a, b| a.0.cmp(b.0));
            for (k, v) in ms {
                lines.push(format!("{t} M {id} {k} {}", bits(*v)));
            }
            let mut sm: Vec<(&String, &f64)> = a.scenario_metrics.iter().collect();
            sm.sort_by(|a, b| a.0.cmp(b.0));
            for (k, v) in sm {
                lines.push(format!("{t} S {id} {k} {}", bits(*v)));
            }
        }
        let mut gm: Vec<(&String, &f64)> = ws.global_metrics.iter().collect();
        gm.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in gm {
            lines.push(format!("{t} G {k} {}", bits(*v)));
        }
        if let Some(f) = &ws.family_state {
            let mut fm: Vec<_> = f.metrics.iter().collect();
            fm.sort_by(|a, b| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)));
            for (k, v) in fm {
                lines.push(format!("{t} F {:?} {}", k, bits(*v)));
            }
        }
        for l in lines {
            println!("{l}");
        }
    }
}
