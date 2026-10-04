//! A35 stage 1 — rome's family victory: a timer, a spike, or a game? (docs/TRIAGE.md). The
//! A10 method for rome. Measurement only; the sustained-tick variants live in memory.
//!
//! The victory stands on `family:influence ≥ 90`, from tick 30, sustained 1 tick — and the
//! condition is true in under 1 % of its checks (A46 stage 2). Built with `--features census`:
//!
//! 1. per world (no player, balanced, influence, wealth): wins, tick p10/p50/p90, the share of
//!    wins on ticks 30–33, and how many consecutive checks the condition held around the win —
//!    read **at the victory check itself** (the census annotation of the condition), because the
//!    generation transfer runs later in the tick and the tick-end value is not what was checked;
//!    the check stops once the victory is achieved, so the run after the win is counted on the
//!    tick-end value; and the first tick the tick-end value reaches 90, minimum tick or not;
//! 2. the sources of the family's influence (the A37 write sink): overall, and on the tick the
//!    tick-end value first crosses to ≥ 90 (mean per run) — who lifts it to 90;
//! 3. counterfactuals: sustained 3, 5 and 10 ticks — the same table.
//!
//! Usage: cargo run --release --features census --bin a35_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const WORLDS: &[&str] = &["none", "balanced", "influence", "wealth"];

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

fn source_of(w: &census::Write) -> String {
    w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file().trim_start_matches("src/"), w.location.line()))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("# A35 stage 1 — rome's family victory, {seeds} seeds × {ticks} ticks per world\n");
    census::enable_occupancy();
    census::enable_writes();
    census::watch_all_metrics(true);
    let victory_key = "victory | family:influence".to_string();
    // world -> source -> Σ asked (base), and on the first-true tick
    let mut overall: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let mut spike: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let mut spikes: BTreeMap<String, u32> = BTreeMap::new();
    println!("| sustained | world | wins | tick p10/50/90 | wins on 30–33 | checks true in a row up to the win (incl.) p10/50/90 | tick-end influence ≥ 90 in a row: up to the win (incl.) / after it, p10/50/90 | first tick-end influence ≥ 90, runs (tick p10/50/90) | of them before tick 30 | condition true, share of checks | win ticks |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for sustained in [1u32, 3, 5, 10] {
        for world in WORLDS {
            let (mut wins, mut before) = (Vec::new(), Vec::new());
            let (mut true_checks, mut checks) = (0u64, 0u64);
            let (mut first90, mut end_before, mut end_after) = (Vec::new(), Vec::new(), Vec::new());
            // runs whose tick-end influence reached 90 before the minimum tick 30
            let mut early = 0u32;
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, "rome_375".into()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                st.current_scenario.as_mut().unwrap().victory_condition.as_mut().unwrap().sustained_ticks_required = sustained;
                let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, "rome_375"));
                let _ = census::take_occupancy();
                let _ = census::take_writes();
                // per tick: was the condition checked, and was it true
                let mut trace: Vec<Option<bool>> = Vec::new();
                let mut won: Option<usize> = None;
                let mut seen90 = false;
                // tick-end influence (after the generation transfer — not what the check read)
                let mut end: Vec<f64> = Vec::new();
                for _ in 0..ticks {
                    match &strategy {
                        Some(s) => { play_scripted_tick(&mut st, s); }
                        None => {
                            let ws = st.world_state.as_mut().unwrap();
                            let sc = st.current_scenario.as_ref().unwrap();
                            engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
                        }
                    }
                    let occ = census::take_occupancy();
                    let state = occ.iter().find(|((c, _), _)| c == &victory_key).map(|(_, v)| v.0 > 0);
                    trace.push(state);
                    if let Some(t) = state { checks += 1; if t { true_checks += 1; } }
                    let writes = census::take_writes();
                    let ws = st.world_state.as_ref().unwrap();
                    let v = ws.family_state.as_ref().map(|f| f.metrics.get("influence").copied().unwrap_or(0.0)).unwrap_or(0.0);
                    // the spike: the first tick whose tick-end value crosses to ≥ 90
                    let crossing = v >= 90.0 && !seen90;
                    if sustained == 1 {
                        if crossing { *spikes.entry(world.to_string()).or_default() += 1; }
                        for w in &writes {
                            if w.actor == "family" && w.metric == "influence" {
                                *overall.entry(world.to_string()).or_default().entry(source_of(w)).or_default() += w.requested;
                                if crossing {
                                    *spike.entry(world.to_string()).or_default().entry(source_of(w)).or_default() += w.requested;
                                }
                            }
                        }
                    }
                    if v >= 90.0 { seen90 = true; }
                    end.push(v);
                    if won.is_none() && ws.victory_achieved { won = Some(trace.len() - 1); }
                }
                if let Some(t) = end.iter().position(|v| *v >= 90.0) {
                    first90.push(t as f64);
                    if t < 30 { early += 1; }
                }
                if let Some(i) = won {
                    // the probe's tick index i is the victory tick (`ws.tick - 1` at that moment)
                    wins.push(i as f64);
                    let mut b = 0;
                    let mut j = i as isize;
                    while j >= 0 && end[j as usize] >= 90.0 { b += 1; j -= 1; }
                    let mut a = 0;
                    let mut j = i + 1;
                    while j < end.len() && end[j] >= 90.0 { a += 1; j += 1; }
                    end_before.push(b as f64);
                    end_after.push(a as f64);
                    let mut b = 0;
                    let mut j = i as isize;
                    while j >= 0 && trace[j as usize] == Some(true) { b += 1; j -= 1; }
                    before.push(b as f64);
                }
            }
            let on = wins.iter().filter(|t| (30.0..=33.0).contains(*t)).count();
            println!("| {sustained} | {world} | {} | {} | {on} ({:.0} %) | {} | {} / {} | {} ({}) | {early} | {:.2} % of {checks} | {} |",
                wins.len(), q(&wins), 100.0 * on as f64 / wins.len().max(1) as f64, q(&before), q(&end_before), q(&end_after),
                first90.len(), q(&first90),
                100.0 * true_checks as f64 / checks.max(1) as f64,
                { let mut w = wins.clone(); w.sort_by(|a, b| a.partial_cmp(b).unwrap()); w.iter().map(|t| format!("{t:.0}")).collect::<Vec<_>>().join(" ") });
        }
    }
    println!("\n## Sources of the family's influence (sustained 1; Σ asked over all runs)\n");
    for world in WORLDS {
        let Some(m) = overall.get(*world) else { continue };
        let mut v: Vec<(&String, &f64)> = m.iter().collect();
        v.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());
        let top: Vec<String> = v.iter().take(8).map(|(s, x)| format!("{s} {x:+.0}")).collect();
        println!("- **{world}**, overall: {}", top.join(", "));
        if let Some(sp) = spike.get(*world) {
            let mut v: Vec<(&String, &f64)> = sp.iter().collect();
            v.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());
            let n = spikes.get(*world).copied().unwrap_or(1).max(1) as f64;
            let top: Vec<String> = v.iter().take(8).map(|(s, x)| format!("{s} {:+.2}", **x / n)).collect();
            println!("  - on the tick the tick-end value first crosses to ≥ 90 (mean per run, {n} runs): {}", top.join(", "));
        }
    }
}
