//! A8 stage 1 — the constantinople climax against the played world (docs/TRIAGE.md).
//! Measurement only; no threshold is chosen here.
//!
//! Since B46, `mehmed_rises` (`ottomans.military_size > 250` for 5 ticks), `final_assault`
//! (`> 280` for 3) and `constantinople_holds` (`byzantium.cohesion > 70` for 5, after the
//! assault) fire in no world: all their earlier firings were over a fallen city, which
//! `requires_alive = ["byzantium"]` now closes. Per world (no player, balanced, diplomacy,
//! military), counted **only while Byzantium is alive**:
//!
//! 1. the Ottoman army: its peak and the peak's tick, p10/p50/p90; the share of games where it
//!    exceeds 200 / 230 / 250 / 280 at all, for 3 sustained ticks (`final_assault`'s hold) and
//!    for 5 (`mehmed_rises`'s);
//! 2. the same inside ticks 40–50, around 1453, where the B46 «held» check sits (tick 46);
//! 3. Byzantium's cohesion right after the army's peak — its value on the peak tick and whether
//!    it is above 70 for 5 ticks in a row afterwards — which decides whether
//!    `constantinople_holds` is reachable after an assault;
//! 4. the cost: how often and when Byzantium falls.
//!
//! Values are read at the end of each tick. The milestones read them mid-tick, in
//! `phase_events`, before collapses and vassalage; as a consistency check the probe counts,
//! with the current thresholds and holds, the games where its own reading would fire
//! `mehmed_rises` / `final_assault`, next to how often the engine actually fired them.
//!
//! § 5 (stage 2, after the owner dated the climax): `mehmed_rises` on tick 42 and
//! `final_assault` on tick 46 fire exactly where Byzantium is alive at the start of that tick;
//! none over a dead city; `outcome_survived_alone` (tick 47 since the state rule) never before the siege.
//!
//! Usage: cargo run --release --bin a8_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;

const WORLDS: &[&str] = &["none", "balanced", "diplomacy", "military"];
const THRESHOLDS: &[f64] = &[200.0, 230.0, 250.0, 280.0];
const WINDOW: (usize, usize) = (40, 50);

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

/// Longest run of consecutive ticks in `range` where `army > thr`.
fn longest_run(army: &[Option<f64>], range: std::ops::Range<usize>, thr: f64) -> usize {
    let (mut best, mut cur) = (0, 0);
    for t in range {
        if army.get(t).copied().flatten().is_some_and(|v| v > thr) {
            cur += 1;
            best = best.max(cur);
        } else {
            cur = 0;
        }
    }
    best
}

struct Run {
    // per tick, `None` once Byzantium is gone (or the Ottomans are)
    army: Vec<Option<f64>>,
    cohesion: Vec<Option<f64>>,
    fall: Option<usize>,
    fired: [bool; 3],
    // tick each watched milestone first appears (`ws.tick - 1`), and the tick-end order
    fired_at: std::collections::BTreeMap<String, usize>,
    // the Ottoman army every tick regardless of Byzantium (where the old firings lived)
    army_any: Vec<Option<f64>>,
    // the Ottomans' mobilisation capacity (`military_capacity`), Byzantium alive
    capacity: Vec<Option<f64>>,
}

fn run(world: &str, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, "constantinople_1430".into()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, "constantinople_1430"));
    let mut r = Run { army: Vec::new(), cohesion: Vec::new(), fall: None, fired: [false; 3], fired_at: Default::default(), army_any: Vec::new(), capacity: Vec::new() };
    for _ in 0..ticks {
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let sc = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        let byz = ws.actors.get("byzantium").filter(|_| !ws.dead_actor_ids.contains("byzantium"));
        if byz.is_none() && r.fall.is_none() {
            r.fall = Some(ws.tick as usize - 1);
        }
        let ott = ws.actors.get("ottomans");
        r.army.push(byz.and(ott).map(|o| o.get_metric("military_size")));
        r.capacity.push(byz.and(ott).map(engine13::engine::interactions::military_capacity));
        r.army_any.push(ott.filter(|_| !ws.dead_actor_ids.contains("ottomans")).map(|o| o.get_metric("military_size")));
        r.cohesion.push(byz.map(|b| b.get_metric("cohesion")));
        for id in ["mehmed_rises", "final_assault", "outcome_survived_alone"] {
            if ws.milestone_events_fired.iter().any(|m| m == id) && !r.fired_at.contains_key(id) {
                // same tick: order by position in the fired list
                r.fired_at.insert(id.to_string(), ws.tick as usize - 1);
                if id == "outcome_survived_alone" {
                    let pos = |x: &str| ws.milestone_events_fired.iter().position(|m| m == x);
                    if pos("final_assault").is_none_or(|f| Some(f) > pos(id)) {
                        r.fired_at.insert("outcome_before_assault".into(), 1);
                    }
                }
            }
        }
        for (i, id) in ["mehmed_rises", "final_assault", "constantinople_holds"].iter().enumerate() {
            r.fired[i] = ws.milestone_events_fired.iter().any(|m| m == id);
        }
    }
    r
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("# A8 stage 1 — constantinople, {seeds} seeds × {ticks} ticks per world; only ticks with Byzantium alive\n");
    let all: Vec<(&str, Vec<Run>)> = WORLDS.iter().map(|w| (*w, (0..seeds).map(|s| run(w, s, ticks)).collect())).collect();
    let n = seeds as f64;
    let share = |k: usize| format!("{:.0} %", 100.0 * k as f64 / n);

    println!("## 1. The Ottoman army over the whole game\n");
    println!("| world | peak p10/50/90 | peak tick p10/50/90 | > 200: any / 3 ticks / 5 ticks | > 230 | > 250 | > 280 |");
    println!("|---|---|---|---|---|---|---|");
    for (w, runs) in &all {
        let mut peaks = Vec::new();
        let mut peak_ticks = Vec::new();
        for r in runs {
            if let Some((t, v)) = r.army.iter().enumerate().filter_map(|(t, v)| v.map(|v| (t, v))).max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) {
                peaks.push(v);
                peak_ticks.push(t as f64);
            }
        }
        let cells: Vec<String> = THRESHOLDS.iter().map(|thr| {
            let c = |h: usize| runs.iter().filter(|r| longest_run(&r.army, 0..r.army.len(), *thr) >= h).count();
            format!("{} / {} / {}", share(c(1)), share(c(3)), share(c(5)))
        }).collect();
        println!("| {w} | {} | {} | {} |", q(&peaks), q(&peak_ticks), cells.join(" | "));
    }

    println!("\n## 1b. The Ottoman army over time (end of tick), p10/50/90 — Byzantium alive / after its fall\n");
    let marks = [0usize, 2, 5, 10, 20, 30, 40, 46, 50, 75, 100, 150, 200, 299];
    println!("| world | {} |", marks.iter().map(|t| format!("tick {t}")).collect::<Vec<_>>().join(" | "));
    println!("|---|{}", "---|".repeat(marks.len()));
    for (w, runs) in &all {
        let cells: Vec<String> = marks.iter().map(|&t| {
            let alive: Vec<f64> = runs.iter().filter_map(|r| r.army.get(t).copied().flatten()).collect();
            let fallen: Vec<f64> = runs.iter().filter(|r| r.fall.is_some_and(|f| f <= t)).filter_map(|r| r.army_any.get(t).copied().flatten()).collect();
            format!("{} ({}) / {} ({})", q(&alive), alive.len(), q(&fallen), fallen.len())
        }).collect();
        println!("| {w} | {} |", cells.join(" | "));
    }
    println!("\n## 1d. The Ottomans' mobilisation capacity (`military_capacity`, the ceiling of recovery), Byzantium alive, p10/50/90\n");
    println!("| world | {} |", marks.iter().map(|t| format!("tick {t}")).collect::<Vec<_>>().join(" | "));
    println!("|---|{}", "---|".repeat(marks.len()));
    for (w, runs) in &all {
        let cells: Vec<String> = marks.iter().map(|&t| q(&runs.iter().filter_map(|r| r.capacity.get(t).copied().flatten()).collect::<Vec<_>>())).collect();
        println!("| {w} | {} |", cells.join(" | "));
    }

    println!("\n## 1c. After Byzantium's fall: the Ottoman army's peak and the old thresholds\n");
    println!("| world | games with a fall | peak after the fall p10/50/90 | its tick p10/50/90 | > 250 for 5 ticks | > 280 for 3 ticks |");
    println!("|---|---|---|---|---|---|");
    for (w, runs) in &all {
        let fallen: Vec<&Run> = runs.iter().filter(|r| r.fall.is_some()).collect();
        let mut peaks = Vec::new();
        let mut ticks_ = Vec::new();
        for r in &fallen {
            let f = r.fall.unwrap();
            if let Some((t, v)) = r.army_any.iter().enumerate().skip(f).filter_map(|(t, v)| v.map(|v| (t, v))).max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) {
                peaks.push(v);
                ticks_.push(t as f64);
            }
        }
        let m = fallen.iter().filter(|r| longest_run(&r.army_any, r.fall.unwrap()..r.army_any.len(), 250.0) >= 5).count();
        let f3 = fallen.iter().filter(|r| longest_run(&r.army_any, r.fall.unwrap()..r.army_any.len(), 280.0) >= 3).count();
        println!("| {w} | {} | {} | {} | {m} | {f3} |", fallen.len(), q(&peaks), q(&ticks_));
    }

    println!("\n## 2. The same inside ticks {}–{} (around 1453)\n", WINDOW.0, WINDOW.1);
    println!("| world | Byzantium alive on tick {} | window peak p10/50/90 | > 200: any / 3 ticks / 5 ticks | > 230 | > 250 | > 280 |", WINDOW.0);
    println!("|---|---|---|---|---|---|---|");
    for (w, runs) in &all {
        let alive = runs.iter().filter(|r| r.army.get(WINDOW.0).copied().flatten().is_some()).count();
        let peaks: Vec<f64> = runs.iter().filter_map(|r| {
            r.army[WINDOW.0..=WINDOW.1].iter().filter_map(|v| *v).max_by(|a, b| a.partial_cmp(b).unwrap())
        }).collect();
        let cells: Vec<String> = THRESHOLDS.iter().map(|thr| {
            let c = |h: usize| runs.iter().filter(|r| longest_run(&r.army, WINDOW.0..WINDOW.1 + 1, *thr) >= h).count();
            format!("{} / {} / {}", share(c(1)), share(c(3)), share(c(5)))
        }).collect();
        println!("| {w} | {alive} / {seeds} | {} | {} |", q(&peaks), cells.join(" | "));
    }

    println!("\n## 3. Byzantium's cohesion after the army's peak\n");
    println!("| world | cohesion on the peak tick p10/50/90 | > 70 for 5 ticks in a row after the peak | games with any 5-tick run of cohesion > 70 (alive) | first such run, tick p10/50/90 |");
    println!("|---|---|---|---|---|");
    for (w, runs) in &all {
        let mut at_peak = Vec::new();
        let mut after = 0;
        let mut any = 0;
        let mut first = Vec::new();
        for r in runs {
            let Some((t, _)) = r.army.iter().enumerate().filter_map(|(t, v)| v.map(|v| (t, v))).max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) else { continue };
            if let Some(c) = r.cohesion[t] { at_peak.push(c); }
            if longest_run(&r.cohesion, t + 1..r.cohesion.len(), 70.0) >= 5 { after += 1; }
            let mut cur = 0;
            for (i, c) in r.cohesion.iter().enumerate() {
                if c.is_some_and(|c| c > 70.0) { cur += 1 } else { cur = 0 }
                if cur == 5 { first.push(i as f64); break; }
            }
            if longest_run(&r.cohesion, 0..r.cohesion.len(), 70.0) >= 5 { any += 1; }
        }
        println!("| {w} | {} | {} | {} | {} |", q(&at_peak), share(after), share(any), q(&first));
    }

    println!("\n## 3b. Byzantium's cohesion over time (end of tick, alive), p10/50/90 (games)\n");
    let marks = [0usize, 5, 10, 20, 30, 40, 46, 50, 60, 75, 100, 150, 200, 299];
    println!("| world | {} |", marks.iter().map(|t| format!("tick {t}")).collect::<Vec<_>>().join(" | "));
    println!("|---|{}", "---|".repeat(marks.len()));
    for (w, runs) in &all {
        let cells: Vec<String> = marks.iter().map(|&t| {
            let v: Vec<f64> = runs.iter().filter_map(|r| r.cohesion.get(t).copied().flatten()).collect();
            format!("{} ({})", q(&v), v.len())
        }).collect();
        println!("| {w} | {} |", cells.join(" | "));
    }
    println!("\n| world | a 5-tick run of cohesion > 70 starting on tick ≥ T (alive), games: T = 40 / 46 / 60 / 100 |");
    println!("|---|---|");
    for (w, runs) in &all {
        let c = |from: usize| runs.iter().filter(|r| longest_run(&r.cohesion, from..r.cohesion.len(), 70.0) >= 5).count();
        println!("| {w} | {} / {} / {} / {} |", c(40), c(46), c(60), c(100));
    }

    println!("\n## 4. Cost, and the consistency check\n");
    println!("| world | Byzantium falls (tick p10/50/90) | probe's reading would fire `mehmed_rises` / `final_assault` | engine fired `mehmed_rises` / `final_assault` / `constantinople_holds` |");
    println!("|---|---|---|---|");
    for (w, runs) in &all {
        let falls: Vec<f64> = runs.iter().filter_map(|r| r.fall.map(|t| t as f64)).collect();
        let would_m = runs.iter().filter(|r| longest_run(&r.army, 0..r.army.len(), 250.0) >= 5).count();
        let would_f = runs.iter().filter(|r| longest_run(&r.army, 0..r.army.len(), 280.0) >= 3).count();
        let fired = |i: usize| runs.iter().filter(|r| r.fired[i]).count();
        println!("| {w} | {} ({}) | {would_m} / {would_f} | {} / {} / {} |", falls.len(), q(&falls), fired(0), fired(1), fired(2));
    }

    println!("\n## 5. Dated milestones (A8 stage 2): `mehmed_rises` tick 42, `final_assault` tick 46\n");
    println!("| world | Byzantium alive at the start of tick 42 / `mehmed_rises` fired / on tick 42 / mismatched games | alive at the start of 46 / `final_assault` fired / on tick 46 / mismatched | fired over a dead city | `outcome_survived_alone` fired (ticks) / before `final_assault` |");
    println!("|---|---|---|---|---|");
    for (w, runs) in &all {
        // `cohesion[t]` is Some iff Byzantium is alive at the end of tick t
        let alive_at = |r: &Run, t: usize| r.cohesion.get(t - 1).copied().flatten().is_some();
        let cell = |id: &str, t: usize| {
            let alive = runs.iter().filter(|r| alive_at(r, t)).count();
            let fired = runs.iter().filter(|r| r.fired_at.contains_key(id)).count();
            let on = runs.iter().filter(|r| r.fired_at.get(id) == Some(&t)).count();
            let bad = runs.iter().filter(|r| alive_at(r, t) != r.fired_at.contains_key(id)).count();
            format!("{alive} / {fired} / {on} / {bad}")
        };
        let dead = runs.iter().filter(|r| {
            ["mehmed_rises", "final_assault", "outcome_survived_alone"].iter().any(|id| r.fired_at.get(*id).is_some_and(|&t| r.fall.is_some_and(|f| f < t)))
        }).count();
        let ticks: Vec<f64> = runs.iter().filter_map(|r| r.fired_at.get("outcome_survived_alone").map(|t| *t as f64)).collect();
        let before = runs.iter().filter(|r| r.fired_at.contains_key("outcome_before_assault")).count();
        println!("| {w} | {} | {} | {dead} | {} ({}) / {before} |", cell("mehmed_rises", 42), cell("final_assault", 46), ticks.len(), q(&ticks));
    }
}
