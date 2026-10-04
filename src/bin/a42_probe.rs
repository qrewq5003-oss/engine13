//! A42 stage 1 — milan: the player's two pressure valves the bot never reaches
//! (docs/TRIAGE.md). Measurement only; nothing is chosen.
//!
//! `call_papal_arbitration` (−6 on Milan's `external_pressure`, 50 treasury, needs
//! `papacy.legitimacy > 70`) and `milan_savoy_alliance` (−3 Milan, −5 Savoy, 30 treasury) are
//! 13th and 14th of 14 in `MILAN_AGGRESSIVE`, at three actions a tick: the bot never applies
//! either — the A38 pattern. A4 (`milan_regency_stabilizes`, 0 of 30 when played) waits for a
//! second milan strategy. In memory, through the bot's own policy
//! (`play_scripted_priorities_tick`, milan's reserve discipline included):
//!
//! - base: as shipped (valves 13th and 14th);
//! - (а) both valves at positions 2–3 (right after `milan_raise_troops`, which the policy
//!   always tries first anyway);
//! - (б) both valves in the middle (positions 7–8).
//!
//! Per variant: valve uses per game; Milan's legitimacy, cohesion, external pressure and
//! treasury at ticks 50 and 150 (p10/p50/p90, Milan alive); Milan's deaths and all deaths;
//! `milan_regency_stabilizes` as shipped (`legitimacy > 65` for 5 ticks) — games and tick —
//! and, for A4, how many games would hold the decided `>= 48` for 5 ticks. The no-player
//! world has no bot and is printed once, for reference. Also: the share of turns with
//! treasury above the bot's reserve (70) — the only turns it spends beyond raising troops —
//! and the ticks the valves were used.
//!
//! Usage: cargo run --release --bin a42_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_priorities_tick, ScriptedStrategy};
use rand::SeedableRng;

const VALVES: [&str; 2] = ["call_papal_arbitration", "milan_savoy_alliance"];
const MARKS: [usize; 2] = [50, 150];
const METRICS: [&str; 4] = ["legitimacy", "cohesion", "external_pressure", "treasury"];

fn q(v: &[f64]) -> String {
    if v.is_empty() {
        return "—".into();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    format!("{:.0}/{:.0}/{:.0}", at(0.1), at(0.5), at(0.9))
}

fn reorder(base: &[&'static str], at: usize) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = base.iter().copied().filter(|a| !VALVES.contains(a)).collect();
    for (i, a) in VALVES.iter().enumerate() {
        v.insert(at - 1 + i, a);
    }
    v
}

#[derive(Default)]
struct Run {
    uses: [u32; 2],
    // per tick, Milan's metrics (None once Milan is gone)
    series: Vec<Option<[f64; 4]>>,
    papacy_open: u32,
    // turns with treasury above the reserve (70): the only turns with discretionary spend
    surplus_turns: u32,
    use_ticks: Vec<usize>,
    milan_dead: bool,
    deaths: u32,
    regency: Option<usize>,
}

fn run(priorities: Option<&[&'static str]>, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, "milan_1477".into()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    let mut r = Run::default();
    for _ in 0..ticks {
        if st.world_state.as_ref().unwrap().actors.get("papacy").is_some_and(|p| p.get_metric("legitimacy") > 70.0) {
            r.papacy_open += 1;
        }
        let t = st.world_state.as_ref().unwrap().tick as usize;
        if st.world_state.as_ref().unwrap().actors.get("milan").is_some_and(|m| m.get_metric("treasury") > 70.0) {
            r.surplus_turns += 1;
        }
        match priorities {
            Some(p) => {
                let turn = play_scripted_priorities_tick(&mut st, p);
                for (i, v) in VALVES.iter().enumerate() {
                    let n = turn.applied.iter().filter(|a| *a == v).count();
                    r.uses[i] += n as u32;
                    if n > 0 { r.use_ticks.push(t); }
                }
            }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let sc = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        let m = ws.actors.get("milan").filter(|_| !ws.dead_actor_ids.contains("milan"));
        r.series.push(m.map(|m| METRICS.map(|k| m.get_metric(k))));
        if r.regency.is_none() && ws.milestone_events_fired.iter().any(|x| x == "milan_regency_stabilizes") {
            r.regency = Some(ws.tick as usize - 1);
        }
    }
    let ws = st.world_state.as_ref().unwrap();
    r.milan_dead = ws.dead_actor_ids.contains("milan");
    r.deaths = ws.dead_actors.len() as u32;
    r
}

fn held(series: &[Option<[f64; 4]>], pred: impl Fn(f64) -> bool, n: usize) -> bool {
    let mut cur = 0;
    for s in series {
        if s.is_some_and(|m| pred(m[0])) { cur += 1 } else { cur = 0 }
        if cur >= n {
            return true;
        }
    }
    false
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let base = ScriptedStrategy::from_str("aggressive", "milan_1477").priority_actions();
    let variants: Vec<(&str, Option<Vec<&'static str>>)> = vec![
        ("no player (reference)", None),
        ("base (13–14)", Some(base.clone())),
        ("(а) 2–3", Some(reorder(&base, 2))),
        ("(б) 7–8", Some(reorder(&base, 7))),
    ];
    println!("# A42 stage 1 — milan, {seeds} seeds × {ticks} ticks per variant\n");
    println!("## Priority lists (position: action)\n");
    for (label, p) in &variants {
        if let Some(p) = p {
            let pos: Vec<String> = VALVES.iter().map(|v| format!("`{v}` {}", p.iter().position(|a| a == v).unwrap() + 1)).collect();
            println!("- **{label}**: {} — {}", pos.join(", "), p.join(", "));
        }
    }
    println!("\n## Results\n");
    println!("| variant | `call_papal_arbitration` per game p10/50/90 (games ≥ 1) | `milan_savoy_alliance` per game (games ≥ 1) | papacy legitimacy > 70, share of ticks | Milan dies | all deaths | regency as shipped (> 65 × 5): games (tick p10/50/90) | would hold `>= 48` × 5: games | turns with treasury > 70 (reserve), share | valve use ticks p10/50/90 |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let mut at_marks = Vec::new();
    for (label, p) in &variants {
        let runs: Vec<Run> = (0..seeds).map(|s| run(p.as_deref(), s, ticks)).collect();
        let uses = |i: usize| {
            let v: Vec<f64> = runs.iter().map(|r| r.uses[i] as f64).collect();
            format!("{} ({})", q(&v), runs.iter().filter(|r| r.uses[i] > 0).count())
        };
        let open: u32 = runs.iter().map(|r| r.papacy_open).sum();
        let reg: Vec<f64> = runs.iter().filter_map(|r| r.regency.map(|t| t as f64)).collect();
        let decided = runs.iter().filter(|r| held(&r.series, |l| l >= 48.0, 5)).count();
        let surplus: u32 = runs.iter().map(|r| r.surplus_turns).sum();
        let ut: Vec<f64> = runs.iter().flat_map(|r| r.use_ticks.iter().map(|t| *t as f64)).collect();
        println!("| {label} | {} | {} | {:.0} % | {} | {} | {} ({}) | {decided} | {:.1} % | {} |",
            if p.is_some() { uses(0) } else { "—".into() }, if p.is_some() { uses(1) } else { "—".into() },
            100.0 * open as f64 / (seeds as f64 * ticks as f64),
            runs.iter().filter(|r| r.milan_dead).count(), runs.iter().map(|r| r.deaths).sum::<u32>(), reg.len(), q(&reg),
            100.0 * surplus as f64 / (seeds as f64 * ticks as f64), q(&ut));
        for &t in &MARKS {
            let cells: Vec<String> = (0..METRICS.len()).map(|k| q(&runs.iter().filter_map(|r| r.series.get(t).copied().flatten().map(|m| m[k])).collect::<Vec<_>>())).collect();
            let alive = runs.iter().filter(|r| r.series.get(t).copied().flatten().is_some()).count();
            at_marks.push(format!("| {label} | {t} | {alive} | {} |", cells.join(" | ")));
        }
    }
    println!("\n## Milan's metrics (end of tick, Milan alive), p10/p50/p90\n");
    println!("| variant | tick | Milan alive | {} |", METRICS.join(" | "));
    println!("|---|---|---|{}", "---|".repeat(METRICS.len()));
    for row in at_marks {
        println!("{row}");
    }
}
