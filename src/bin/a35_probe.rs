//! A35 stage 1 — rome's family victory: a timer, a spike, or a game? (docs/TRIAGE.md). The
//! A10 method for rome. Measurement only; the sustained-tick variants live in memory.
//!
//! At stage 1 the victory stood on `family:influence ≥ 90`, from tick 30, sustained 1 tick — the
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
//! Stage 2 (`stage2` as the third argument): the minimum tick removed, hold 1 / 5 / 10 —
//! variants (a) / (b) / (c) — with the same table plus the largest share of a world's wins on
//! one tick, Rome's split on tick 40 and Rome's deaths. The owner's acceptance: no tick holds
//! more than 15 % of a world's wins; strategies differ; games without a win; no player 0;
//! balanced and influence keep at least half of 23 and 15 wins.
//!
//! Usage: cargo run --release --features census --bin a35_probe -- [seeds] [ticks] [stage2]

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
    let stage2 = args.get(3).map(|s| s == "stage2").unwrap_or(false);
    // (label, minimum tick, hold); `None` keeps the scenario's value. Since A35 stage 2 the
    // scenario ships (b) — no minimum tick, hold 5 — so stage 1's grid sets min 30 explicitly.
    let variants: Vec<(&str, Option<u32>, Option<u32>)> = if stage2 {
        vec![("as shipped", None, None), ("(a)", Some(0), Some(1)), ("(b)", Some(0), Some(5)), ("(c)", Some(0), Some(10))]
    } else {
        vec![("stage-1 base", Some(30), Some(1)), ("hold 3", Some(30), Some(3)), ("hold 5", Some(30), Some(5)), ("hold 10", Some(30), Some(10))]
    };
    println!("# A35 stage {} — rome's family victory, {seeds} seeds × {ticks} ticks per world\n", if stage2 { 2 } else { 1 });
    census::enable_occupancy();
    census::enable_writes();
    census::watch_all_metrics(true);
    let victory_key = "victory | family:influence".to_string();
    // world -> source -> Σ asked (base), and on the first-true tick
    let mut overall: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let mut spike: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let mut spikes: BTreeMap<String, u32> = BTreeMap::new();
    println!("| variant | world | wins | tick p10/50/90 | wins on 30–33 | checks true in a row up to the win (incl.) p10/50/90 | tick-end influence ≥ 90 in a row: up to the win (incl.) / after it, p10/50/90 | first tick-end influence ≥ 90, runs (tick p10/50/90) | of them before tick 30 | condition true, share of checks | largest share of wins on one tick | Rome split on tick 40 | Rome dies | win ticks |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (i, (label, min_tick, hold)) in variants.iter().copied().enumerate() {
        // what the variant actually ran with, for the label
        let mut shown = (0u32, 0u32);
        for world in WORLDS {
            let (mut wins, mut before) = (Vec::new(), Vec::new());
            let (mut true_checks, mut checks) = (0u64, 0u64);
            let (mut first90, mut end_before, mut end_after) = (Vec::new(), Vec::new(), Vec::new());
            // runs whose tick-end influence reached 90 before the minimum tick 30
            let mut early = 0u32;
            let (mut split40, mut rome_dies) = (0u32, 0u32);
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, "rome_375".into()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                {
                    let vc = st.current_scenario.as_mut().unwrap().victory_condition.as_mut().unwrap();
                    if let Some(h) = hold { vc.sustained_ticks_required = h; }
                    if let Some(m) = min_tick { vc.minimum_tick = m; }
                    shown = (vc.minimum_tick, vc.sustained_ticks_required);
                }
                let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, "rome_375"));
                let _ = census::take_occupancy();
                let _ = census::take_writes();
                // per tick: was the condition checked, and was it true
                let mut trace: Vec<Option<bool>> = Vec::new();
                let mut won: Option<usize> = None;
                let mut seen90 = false;
                let mut split_tick: Option<u32> = None;
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
                    if i == 0 {
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
                    if split_tick.is_none() && ws.milestone_events_fired.iter().any(|m| m == "rome_splits") { split_tick = Some(ws.tick - 1); }
                }
                {
                    let ws = st.world_state.as_ref().unwrap();
                    if ws.dead_actor_ids.contains("rome") { rome_dies += 1; }
                    if split_tick == Some(40) { split40 += 1; }
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
            let mut per_tick: BTreeMap<u32, u32> = BTreeMap::new();
            for t in &wins { *per_tick.entry(*t as u32).or_default() += 1; }
            let (top_tick, top_n) = per_tick.iter().max_by_key(|(t, n)| (**n, std::cmp::Reverse(**t))).map(|(t, n)| (*t, *n)).unwrap_or((0, 0));
            println!("| {label}, min {}, hold {} | {world} | {} | {} | {on} ({:.0} %) | {} | {} / {} | {} ({}) | {early} | {:.2} % of {checks} | {} | {split40} / {seeds} | {rome_dies} / {seeds} | {} |",
                shown.0, shown.1,
                wins.len(), q(&wins), 100.0 * on as f64 / wins.len().max(1) as f64, q(&before), q(&end_before), q(&end_after),
                first90.len(), q(&first90),
                100.0 * true_checks as f64 / checks.max(1) as f64,
                if top_n == 0 { "—".to_string() } else { format!("{:.0} % (tick {top_tick}, {top_n})", 100.0 * top_n as f64 / wins.len() as f64) },
                { let mut w = wins.clone(); w.sort_by(|a, b| a.partial_cmp(b).unwrap()); w.iter().map(|t| format!("{t:.0}")).collect::<Vec<_>>().join(" ") });
        }
    }
    println!("\n## Sources of the family's influence (first variant; Σ asked over all runs)\n");
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
