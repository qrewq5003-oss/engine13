//! Economy project, Ц2: the army a state can pay for — arithmetic over the protocols of the world
//! with Ц10 (docs/economy_project_brief.md §9). Built with `--features census`. v2 as in the
//! content (Ц1, Ц3–Ц10), every world, 30 seeds × 300 ticks; seeds from an argument (0, 100). The
//! engine is not touched: each actor's army and treasury are replayed tick by tick over the
//! protocol of its game. **Battles are not replayed** — losses are the protocol's share of the army
//! lost on the tick, so a larger or smaller army winning or losing differently is not in it.
//!
//! The replay of a tick, in the engine's order: the treasury formula (income from the protocol,
//! upkeep `0.8 × army`), [the debt rule: four ticks below zero → −5 % army a tick], [the sink (Г)];
//! the protocol's other gains of army (auto-deltas, events, milestones); recovery toward the norm M
//! at 5 % of the gap, only below M and only with a non-negative treasury; [«down always»: above M,
//! 5 % of the excess]; the protocol's share of losses; the protocol's other treasury writes
//! (losses stop at zero, as in v2).
//!
//! Norms: C = 0.767 × population^(2/3) (people), F = income / 0.8 (money).
//! (А) M = min(C, F); (Б) M = F; (В) M = C + s × max(0, F − C), s = 0.25 and 0.5 — each with
//! «down always» and «down only in debt» (the debt rule, x = 5 %). (Г) no norm (M = C as now), the
//! treasury above R = 20 × the tick's income loses 5 % of the excess a tick. «replay» — M = C,
//! nothing else: the fidelity of the replay against the protocol.
//!
//! Usage: cargo run --release --features census --bin c2a_probe -- [first_seed] [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const SCENARIOS: [&str; 3] = ["rome_375", "constantinople_1430", "milan_1477"];
const KEY: [&str; 5] = ["rome", "ottomans", "huns", "byzantium", "milan"];

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if s.is_empty() { return f64::NAN; }
    s[((s.len() - 1) as f64 * p).round() as usize]
}

/// One living tick of an actor in the protocol.
#[derive(Clone, Default)]
struct Row {
    tick: u32,
    pop: f64,
    income: f64,
    /// army and treasury at the start of the tick
    army0: f64,
    treasury0: f64,
    /// army: recovery, other gains, losses (positive number) on the tick
    rec: f64,
    gain: f64,
    loss: f64,
    /// other treasury writes (not the formula): gains, losses (negative)
    tr_gain: f64,
    tr_loss: f64,
    /// army and treasury at the end of the tick in the protocol
    army1: f64,
    treasury1: f64,
    /// economic_output and its norm T at the end of the tick; population × the income coefficient
    eo: f64,
    teo: f64,
    pc: f64,
    /// the actor carries `tribal_confederation` or `nomadic` at the end of the tick
    militia: bool,
    /// threat neighbours counted by the engine at the end of the tick: (id, weight)
    nb: Vec<(String, f64)>,
}

type Game = BTreeMap<String, Vec<Row>>;
/// (tick, army at the end, treasury at the end, population), per actor
type Replayed = BTreeMap<String, Vec<(u32, f64, f64, f64)>>;
/// the same with the norm M on each tick
type Replayed2 = BTreeMap<String, Vec<(u32, f64, f64, f64, f64)>>;
/// per actor: Σ shift (absolute), Σ shift (relative), ticks, eo, T
type Br = BTreeMap<String, (f64, f64, u64, Vec<f64>, Vec<f64>)>;
/// (tick, army, treasury, population, deserted), per actor
type Replayed3 = BTreeMap<String, Vec<(u32, f64, f64, f64, bool)>>;
/// … and the income, per actor
type Replayed4 = BTreeMap<String, Vec<(u32, f64, f64, f64, bool, f64)>>;

fn recovery_line() -> u32 {
    let src = include_str!("../engine/interactions.rs");
    src.lines().position(|l| l.contains("current + (capacity - current) * MILITARY_RECOVERY_RATE")).map(|i| i as u32 + 1).expect("the recovery write")
}

fn play(sc: &str, world: &str, seed: u64, ticks: u32, rec_line: u32) -> Game {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        assert!(s.economy_v2_population_pull.is_some() && s.economy_v2_debt_cut.is_none(), "Ц10 on, the debt rule off, as in the content");
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut g: Game = BTreeMap::new();
    let _ = census::take_writes();
    let _ = census::take_treasury_parts();
    for _ in 0..ticks {
        let start: BTreeMap<String, (f64, f64)> = st.world_state.as_ref().unwrap().actors.iter().map(|(k, a)| (k.clone(), (a.get_metric("military_size"), a.get_metric("treasury")))).collect();
        let t = st.world_state.as_ref().unwrap().tick;
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let mut rows: BTreeMap<String, Row> = BTreeMap::new();
        for (a, inc, _) in census::take_treasury_parts() { rows.entry(a).or_default().income += inc; }
        for w in census::take_writes() {
            let r = rows.entry(w.actor.clone()).or_default();
            match w.metric.as_str() {
                "military_size" => {
                    if w.source.is_none() && w.location.file().ends_with("interactions.rs") && w.location.line() == rec_line { r.rec += w.applied; }
                    else if w.applied >= 0.0 { r.gain += w.applied; } else { r.loss -= w.applied; }
                }
                "treasury" if w.source.as_deref() != Some("treasury formula") => {
                    if w.applied >= 0.0 { r.tr_gain += w.applied; } else { r.tr_loss += w.applied; }
                }
                _ => {}
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        for (id, a) in &ws.actors {
            if ws.dead_actor_ids.contains(id) { continue; }
            let Some(&(a0, t0)) = start.get(id) else { continue };
            let mut r = rows.remove(id).unwrap_or_default();
            r.tick = t;
            r.pop = a.get_metric("population");
            r.army0 = a0;
            r.treasury0 = t0;
            r.army1 = a.get_metric("military_size");
            r.treasury1 = a.get_metric("treasury");
            {
                let scn = st.current_scenario.as_ref().unwrap();
                r.eo = a.get_metric("economic_output");
                r.teo = engine13::engine::eo_target(ws, scn, id).unwrap_or(f64::NAN);
                r.pc = r.pop.max(0.0) * scn.economy_v2_income_coefficient.unwrap_or(0.001);
                r.militia = a.actor_tags.contains_key("tribal_confederation") || a.actor_tags.contains_key("nomadic");
            }
            let bound = |o: &str| ws.vassalages.iter().any(|v| (v.vassal_id == *id && v.overlord_id == o) || (v.overlord_id == *id && v.vassal_id == o))
                || ws.alliances.iter().any(|al| al.actor_ids.iter().any(|x| x == id) && al.actor_ids.iter().any(|x| x == o));
            r.nb = a.neighbors.iter().filter(|n| n.distance == 1 && !ws.dead_actor_ids.contains(&n.id) && ws.actors.contains_key(&n.id) && !bound(&n.id))
                .map(|n| (n.id.clone(), if n.border_type == engine13::core::BorderType::Sea { 0.5 } else { 1.0 })).collect();
            g.entry(id.clone()).or_default().push(r);
        }
    }
    g
}

#[derive(Clone, Copy, PartialEq)]
enum Norm { Replay, A, B, V(f64), G, Ap(f64) }
#[derive(Clone, Copy, PartialEq)]
enum Down { None, Always, Debt }

fn cell_label(n: Norm, d: Down) -> String {
    let n = match n { Norm::Replay => "replay (M = C)".to_string(), Norm::A => "(А) min(C, F)".into(), Norm::B => "(Б) F".into(), Norm::V(s) => format!("(В) C + {s} × (F − C)⁺"), Norm::G => "(Г) sink above 20 × income".into(), Norm::Ap(al) => format!("(А′) min(C, {al} F)") };
    match d { Down::None => n, Down::Always => format!("{n}, down always"), Down::Debt => format!("{n}, down in debt") }
}

/// The replayed (tick, army at the end, treasury at the end, population) of one actor.
fn replay(rows: &[Row], n: Norm, d: Down) -> Vec<(u32, f64, f64, f64)> {
    replay2(rows, n, d, false, 0.0).into_iter().map(|x| (x.0, x.1, x.2, x.3)).collect()
}

/// The replay with the sink switchable apart from the norm, and the economy shifted by `eo_shift`
/// (income = max(0, eo + shift) × population × c when the shift is not zero); returns also M.
fn replay2(rows: &[Row], n: Norm, d: Down, sink: bool, eo_shift: f64) -> Vec<(u32, f64, f64, f64, f64)> {
    let Some(first) = rows.first() else { return vec![] };
    let (mut a, mut tr) = (first.army0, first.treasury0);
    let mut debt = 0u32;
    let mut out = Vec::with_capacity(rows.len());
    let mut prev_tick = first.tick;
    for r in rows {
        if r.tick != prev_tick + 1 && r.tick != first.tick { a = r.army0; tr = r.treasury0; }
        prev_tick = r.tick;
        // treasury formula, then the debt rule or the sink
        let income = if eo_shift == 0.0 { r.income } else { (r.eo + eo_shift).clamp(0.0, 100.0) * r.pc };
        tr += income - 0.8 * a;
        if d == Down::Debt {
            if tr < 0.0 { debt += 1; if debt >= 4 { a -= 0.05 * a; } } else { debt = 0; }
        }
        if n == Norm::G || sink {
            let reserve = 20.0 * income.max(0.0);
            if tr > reserve { tr -= 0.05 * (tr - reserve); }
        }
        // other gains, recovery toward M, down
        a += r.gain;
        let c = 0.767 * r.pop.max(0.0).powf(2.0 / 3.0);
        let f = income.max(0.0) / 0.8;
        let m = match n { Norm::Replay | Norm::G => c, Norm::A => c.min(f), Norm::B => f, Norm::V(s) => c + s * (f - c).max(0.0), Norm::Ap(al) => c.min(al * f) };
        if a < m && tr >= 0.0 { a += 0.05 * (m - a); }
        if d == Down::Always && a > m { a -= 0.05 * (a - m); }
        // the protocol's share of losses
        let base = r.army0 + r.gain + r.rec;
        let share = if base > 0.0 { (r.loss / base).min(1.0) } else { 0.0 };
        a *= 1.0 - share;
        a = a.max(0.0);
        // other treasury writes; losses stop at zero
        tr += r.tr_gain;
        if r.tr_loss < 0.0 { tr += r.tr_loss.max(-tr.max(0.0)); }
        out.push((r.tick, a, tr, r.pop, m));
    }
    out
}

fn mean_window(v: &[(u32, f64, f64, f64)], a: u32, b: u32) -> Option<f64> {
    let x: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect();
    (!x.is_empty()).then(|| x.iter().sum::<f64>() / x.len() as f64)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let first: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    census::enable_treasury_parts();
    let rec_line = recovery_line();
    if args.get(4).map(String::as_str) == Some("round2") { round2(first, seeds, ticks, rec_line); return; }
    if args.get(4).map(String::as_str) == Some("round3") { round3(first, seeds, ticks, rec_line); return; }
    if args.get(4).map(String::as_str) == Some("round4") { round4(first, seeds, ticks, rec_line); return; }
    if args.get(4).map(String::as_str) == Some("trebizond") { trebizond(first, seeds, ticks, rec_line); return; }
    let mut cells: Vec<(Norm, Down)> = vec![(Norm::Replay, Down::None)];
    for n in [Norm::A, Norm::B, Norm::V(0.25), Norm::V(0.5)] { for d in [Down::Always, Down::Debt] { cells.push((n, d)); } }
    cells.push((Norm::G, Down::None));
    println!("# Ц2 — the army a state can pay for: arithmetic over the protocols (Ц10 world), seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    println!("Battles are not replayed: losses are the protocol's share of the army lost on the tick. Ц2 item 4 (deaths of key actors) cannot be computed this way.\n");
    let mut debt_rows = Vec::new();
    let mut army_rows = Vec::new();
    let mut c3_rows = Vec::new();
    let mut tp_rows = Vec::new();
    let mut fidelity = Vec::new();
    for sc in SCENARIOS {
        for world in worlds(sc) {
            let games: Vec<Game> = (first..first + seeds).map(|s| play(sc, world, s, ticks, rec_line)).collect();
            // fidelity: the replay against the protocol, army on tick 100 / 299
            let mut errs = Vec::new();
            let (mut agree, mut all) = (0u64, 0u64);
            let mut terr = Vec::new();
            for g in &games {
                for rows in g.values() {
                    let rp = replay(rows, Norm::Replay, Down::None);
                    for (row, x) in rows.iter().zip(&rp) {
                        if (row.tick == 100 || row.tick == 299) && row.army1 > 1.0 { errs.push(100.0 * (x.1 - row.army1).abs() / row.army1); }
                        if (row.tick == 100 || row.tick == 299) && row.treasury1.abs() > 10.0 { terr.push(100.0 * (x.2 - row.treasury1).abs() / row.treasury1.abs()); }
                        if row.pop > 1.0 { all += 1; if (x.2 < 0.0) == (row.treasury1 < 0.0) { agree += 1; } }
                    }
                }
            }
            fidelity.push(format!("| {sc} | {world} | {:.1} % / {:.1} % | {:.1} % / {:.1} % | {:.1} % |", pct(&errs, 0.5), pct(&errs, 0.9), pct(&terr, 0.5), pct(&terr, 0.9), 100.0 * agree as f64 / all.max(1) as f64));
            // the largest armies at the start, and their neighbours
            let mut start: Vec<(String, f64)> = games[0].iter().filter_map(|(k, v)| v.first().map(|r| (k.clone(), r.army0))).collect();
            start.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let big: Vec<String> = start.iter().take(3).map(|x| x.0.clone()).collect();
            for &(n, d) in &cells {
                let label = cell_label(n, d);
                // replay every actor of every game
                let reps: Vec<Replayed> = games.iter().map(|g| g.iter().map(|(k, v)| (k.clone(), replay(v, n, d))).collect()).collect();
                // ---- debt (items 1, 2; 3 printed): treasury < 0 on non-zombie ticks
                let mut acc: BTreeMap<String, (u64, u64, Vec<f64>)> = BTreeMap::new();
                for rep in &reps {
                    for (id, v) in rep {
                        let e = acc.entry(id.clone()).or_default();
                        let mut spell = 0u32;
                        for &(_, _, tr, pop) in v {
                            if pop <= 1.0 { continue; }
                            e.0 += 1;
                            if tr < 0.0 { e.1 += 1; spell += 1; } else if spell > 0 { e.2.push(spell as f64); spell = 0; }
                        }
                        if spell > 0 { e.2.push(spell as f64); }
                    }
                }
                let p90 = acc.values().map(|x| if x.2.is_empty() { 0.0 } else { pct(&x.2, 0.9) }).fold(0.0, f64::max);
                let over: Vec<String> = acc.iter().filter(|(_, x)| 100.0 * x.1 as f64 / x.0.max(1) as f64 > 20.0).map(|(k, x)| format!("{k} {:.0}", 100.0 * x.1 as f64 / x.0.max(1) as f64)).collect();
                let played = *world != "none";
                let i1 = p90 <= 12.0;
                let i2 = played || over.is_empty();
                // ---- accumulation: monotone to 150
                let mut mono: BTreeMap<String, u32> = BTreeMap::new();
                for rep in &reps {
                    for (id, v) in rep {
                        let steps: Option<Vec<f64>> = (0..=15).map(|i| v.iter().find(|p| p.0 == i * 10).map(|p| p.2)).collect();
                        if let Some(s) = steps { if s.windows(2).all(|w| w[1] >= w[0]) && s[15] > s[0] { *mono.entry(id.clone()).or_default() += 1; } }
                    }
                }
                let monos: Vec<String> = mono.iter().filter(|(_, c)| **c as u64 * 2 >= seeds).map(|(k, c)| format!("{k} {c}")).collect();
                debt_rows.push(format!("| {sc} | {world} | {label} | {p90:.0} {} | {} | {} |", if i1 { "yes" } else { "**no**" },
                    if played { format!("(played) {}", if over.is_empty() { "—".into() } else { over.join(", ") }) } else if i2 { "yes".into() } else { format!("**no**: {}", over.join(", ")) },
                    if monos.is_empty() { "—".to_string() } else { monos.join(", ") }));
                // ---- armies of key actors
                let mut cells_a = Vec::new();
                for k in KEY {
                    let w = |a: u32, b: u32| { let x: Vec<f64> = reps.iter().filter_map(|rep| rep.get(k).and_then(|v| mean_window(v, a, b))).collect(); if x.is_empty() { "—".to_string() } else { format!("{:.0}", pct(&x, 0.5)) } };
                    if reps.iter().any(|rep| rep.contains_key(k)) { cells_a.push(format!("{k} {} / {} / {}", w(10, 20), w(40, 50), w(100, 100))); }
                }
                army_rows.push(format!("| {sc} | {world} | {label} | {} |", cells_a.join("; ")));
                // ---- Ц3 and the +60 of tick 42
                if sc == "constantinople_1430" {
                    let pairs: Vec<(f64, f64)> = reps.iter().filter_map(|rep| { let v = rep.get("ottomans")?; Some((mean_window(v, 10, 20)?, mean_window(v, 40, 50)?)) }).collect();
                    let dd: Vec<f64> = pairs.iter().map(|(a, b)| b - 1.25 * a).collect();
                    let nn = dd.len() as f64;
                    let mean = dd.iter().sum::<f64>() / nn.max(1.0);
                    let sd = (dd.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nn - 1.0).max(1.0)).sqrt();
                    let tt = if sd > 0.0 { mean / (sd / nn.sqrt()) } else { 0.0 };
                    let at = |t: u32| { let x: Vec<f64> = reps.iter().filter_map(|rep| rep.get("ottomans").and_then(|v| v.iter().find(|p| p.0 == t).map(|p| p.1))).collect(); pct(&x, 0.5) };
                    c3_rows.push(format!("| {world} | {label} | {mean:+.1} (t {tt:+.1}) {} | {:.0} → {:.0} → {:.0} → {:.0} |", if mean > 0.0 && tt >= 2.0 { "yes" } else { "**no**" }, at(41), at(42), at(46), at(50)));
                }
                // ---- T_p of the big armies' neighbours
                let mut tps_p = Vec::new();
                let mut tps_c = Vec::new();
                for (g, rep) in games.iter().zip(&reps) {
                    let army_at = |id: &str, t: u32, replayed: bool| -> f64 {
                        if replayed { rep.get(id).and_then(|v| v.iter().find(|p| p.0 == t)).map_or(0.0, |p| p.1) } else { g.get(id).and_then(|v| v.iter().find(|r| r.tick == t)).map_or(0.0, |r| r.army1) }
                    };
                    for b in &big {
                        let Some(brows) = g.get(b) else { continue };
                        for nbid in brows.first().map(|r| r.nb.iter().map(|x| x.0.clone()).collect::<Vec<_>>()).unwrap_or_default() {
                            let Some(rows) = g.get(&nbid) else { continue };
                            for r in rows.iter().filter(|r| r.tick % 5 == 0) {
                                for (replayed, out) in [(false, &mut tps_p), (true, &mut tps_c)] {
                                    let nsum: f64 = r.nb.iter().map(|(o, w)| w * army_at(o, r.tick, replayed)).sum();
                                    let own = army_at(&nbid, r.tick, replayed);
                                    out.push(if nsum <= 0.0 { 0.0 } else { 100.0 * nsum / (nsum + own) });
                                }
                            }
                        }
                    }
                }
                tp_rows.push(format!("| {sc} | {world} | {label} | {} | {:.0} → {:.0} |", big.join(", "), pct(&tps_p, 0.5), pct(&tps_c, 0.5)));
            }
        }
    }
    println!("## 0. Fidelity of the replay (M = C, nothing changed) against the protocol: |error| on ticks 100 and 299, p50 / p90; debt status (treasury < 0) agreeing on non-zombie ticks\n");
    println!("| scenario | world | army p50 / p90 | treasury p50 / p90 | debt status agrees |");
    println!("|---|---|---|---|---|");
    for r in fidelity { println!("{r}"); }
    println!("\n## 1. Ц2: debt — (1) worst p90 spell ≤ 12 (zombie ticks not counted), (2) no player: nobody in debt > 20 % / played: who; accumulation — treasury monotone to 150 in half the games or more\n");
    println!("| scenario | world | cell | (1) | (2) / (3) | monotone (games) |");
    println!("|---|---|---|---|---|---|");
    for r in debt_rows { println!("{r}"); }
    println!("\n## 2. Armies of key actors: mean on ticks 10–20 / 40–50 / tick 100, median over games\n");
    println!("| scenario | world | cell | armies |");
    println!("|---|---|---|---|");
    for r in army_rows { println!("{r}"); }
    println!("\n## 3. Ц3 (Ottoman army m₄₀ ≥ 1.25 m₁₀), and the Ottoman army on ticks 41 → 42 (`mehmed_rises` +60) → 46 → 50, median\n");
    println!("| world | cell | d (t) | army 41 → 42 → 46 → 50 |");
    println!("|---|---|---|---|");
    for r in c3_rows { println!("{r}"); }
    println!("\n## 4. T_p of the neighbours (distance 1, counted by the engine) of the three largest starting armies: median every 5th tick, protocol → cell\n");
    println!("| scenario | world | cell | largest armies | T_p |");
    println!("|---|---|---|---|---|");
    for r in tp_rows { println!("{r}"); }
}

/// The p90 debt spell of one replayed actor (non-zombie ticks).
fn p90_spell(v: &[(u32, f64, f64, f64, f64)]) -> (f64, u64, u64) {
    let (mut n, mut d, mut spell) = (0u64, 0u64, 0u32);
    let mut spells = Vec::new();
    for x in v {
        if x.3 <= 1.0 { continue; }
        n += 1;
        if x.2 < 0.0 { d += 1; spell += 1; } else if spell > 0 { spells.push(spell as f64); spell = 0; }
    }
    if spell > 0 { spells.push(spell as f64); }
    (if spells.is_empty() { 0.0 } else { pct(&spells, 0.9) }, n, d)
}

/// Second round (owner's decision after PR #254): (А′) M = min(C, 0.75 F) with and without the
/// sink (Г), both branches; the b/r of `military_size_to_economic_output` (k 0.01, deficit below 50,
/// r = 0.03) with the absolute threshold and with the threshold 50 × M / 100; and the classes of the
/// debtors that fail item 1 — held only by the economy at the floor from that rule (the replay with
/// the economy shifted by the rule's change brings the p90 spell to ≤ 12) or anything else.
fn round2(first: u64, seeds: u64, ticks: u32, rec_line: u32) {
    const K: f64 = 0.01;
    const R: f64 = 0.03;
    let shift = |army: f64, thr: f64| -K * (thr - army).max(0.0) / R;
    let cells = [(Norm::Ap(0.75), Down::Always, true), (Norm::Ap(0.75), Down::Debt, true), (Norm::Ap(0.75), Down::Always, false), (Norm::Ap(0.75), Down::Debt, false)];
    let lab = |n: Norm, d: Down, g: bool| format!("{}{}", cell_label(n, d), if g { " + (Г)" } else { "" });
    println!("# Ц2, second round — (А′) M = min(C, 0.75 F), the sink, the small-army threshold from the norm; seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    let mut debt_rows = Vec::new();
    let mut army_rows = Vec::new();
    let mut c3_rows = Vec::new();
    let mut br_rows = Vec::new();
    let mut class_rows = Vec::new();
    for sc in SCENARIOS {
        for world in worlds(sc) {
            let games: Vec<Game> = (first..first + seeds).map(|s| play(sc, world, s, ticks, rec_line)).collect();
            for &(n, d, g) in &cells {
                let label = lab(n, d, g);
                let reps: Vec<Replayed2> = games.iter().map(|gm| gm.iter().map(|(k, v)| (k.clone(), replay2(v, n, d, g, 0.0))).collect()).collect();
                // per actor: debt, b/r
                let mut acc: BTreeMap<String, (u64, u64, Vec<f64>)> = BTreeMap::new();
                let mut br: Br = BTreeMap::new();
                for (gm, rep) in games.iter().zip(&reps) {
                    for (id, v) in rep {
                        let e = acc.entry(id.clone()).or_default();
                        let mut spell = 0u32;
                        for x in v {
                            if x.3 <= 1.0 { continue; }
                            e.0 += 1;
                            if x.2 < 0.0 { e.1 += 1; spell += 1; } else if spell > 0 { e.2.push(spell as f64); spell = 0; }
                        }
                        if spell > 0 { e.2.push(spell as f64); }
                        let b = br.entry(id.clone()).or_default();
                        for (row, x) in gm[id].iter().zip(v) {
                            b.0 += shift(row.army1, 50.0);
                            b.1 += shift(x.1, 0.5 * x.4);
                            b.2 += 1;
                            b.3.push(row.eo);
                            b.4.push(row.teo);
                        }
                    }
                }
                let p90 = acc.values().map(|x| if x.2.is_empty() { 0.0 } else { pct(&x.2, 0.9) }).fold(0.0, f64::max);
                let over: Vec<String> = acc.iter().filter(|(_, x)| 100.0 * x.1 as f64 / x.0.max(1) as f64 > 20.0).map(|(k, x)| format!("{k} {:.0}", 100.0 * x.1 as f64 / x.0.max(1) as f64)).collect();
                let played = *world != "none";
                let mut mono: BTreeMap<String, u32> = BTreeMap::new();
                for rep in &reps {
                    for (id, v) in rep {
                        let steps: Option<Vec<f64>> = (0..=15).map(|i| v.iter().find(|p| p.0 == i * 10).map(|p| p.2)).collect();
                        if let Some(st) = steps { if st.windows(2).all(|w| w[1] >= w[0]) && st[15] > st[0] { *mono.entry(id.clone()).or_default() += 1; } }
                    }
                }
                let monos: Vec<String> = mono.iter().filter(|(_, c)| **c as u64 * 2 >= seeds).map(|(k, c)| format!("{k} {c}")).collect();
                // classes of the item-1 debtors: replay with the economy shifted by the rule's change
                let mut class1 = Vec::new();
                let mut class2: Vec<(String, f64)> = Vec::new();
                for (id, x) in acc.iter().filter(|(_, x)| !x.2.is_empty() && pct(&x.2, 0.9) > 12.0) {
                    let b = &br[id];
                    let delta = (b.1 - b.0) / b.2.max(1) as f64;
                    let mut spells_after = Vec::new();
                    for gm in &games {
                        if let Some(rows) = gm.get(id) {
                            let v = replay2(rows, n, d, g, if delta == 0.0 { 1e-12 } else { delta });
                            let (q90, _, _) = p90_spell(&v);
                            spells_after.push(q90);
                        }
                    }
                    let after = spells_after.iter().cloned().fold(0.0, f64::max);
                    let before = pct(&x.2, 0.9);
                    if after <= 12.0 { class1.push(format!("{id} {before:.0}→{after:.0}")); } else {
                        // the deepest debt and the 25 % margin of income on the norm: ticks to repay
                        let (mut deep, mut inc, mut ni, mut first_debt) = (Vec::new(), 0.0, 0u64, Vec::new());
                        for (gm, rep) in games.iter().zip(&reps) {
                            if let (Some(v), Some(rows)) = (rep.get(id), gm.get(id)) {
                                deep.push(v.iter().map(|x| x.2).fold(0.0, f64::min));
                                if let Some(t) = v.iter().find(|x| x.2 < 0.0 && x.3 > 1.0) { first_debt.push(t.0 as f64); }
                                for r in rows { inc += r.income; ni += 1; }
                            }
                        }
                        let mi = inc / ni.max(1) as f64;
                        let dp = pct(&deep, 0.5);
                        class2.push((format!("{id} {before:.0} [debt p50 {dp:.0}, from tick {:.0}, 25 % of income {:.2}/tick → {:.0} ticks]", pct(&first_debt, 0.5), 0.25 * mi, if mi > 0.0 { -dp / (0.25 * mi) } else { f64::INFINITY }), before));
                    }
                }
                let worst2 = class2.iter().map(|x| x.1).fold(0.0, f64::max);
                class_rows.push(format!("| {sc} | {world} | {label} | {p90:.0} | {} | {} | {} |", if class1.is_empty() { "—".into() } else { class1.join(", ") },
                    if class2.is_empty() { "—".into() } else { class2.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>().join(", ") }, if worst2 <= 12.0 { "**yes**" } else { "no" }));
                debt_rows.push(format!("| {sc} | {world} | {label} | {p90:.0} {} | {} | {} |", if p90 <= 12.0 { "yes" } else { "**no**" },
                    if played { format!("(played) {}", if over.is_empty() { "—".into() } else { over.join(", ") }) } else if over.is_empty() { "yes".into() } else { format!("**no**: {}", over.join(", ")) },
                    if monos.is_empty() { "—".to_string() } else { monos.join(", ") }));
                // b/r rows (once per world: the «down in debt + (Г)» cell)
                if d == Down::Debt && g {
                    for (id, b) in &br {
                        let nn = b.2.max(1) as f64;
                        let (sa, sr) = (b.0 / nn, b.1 / nn);
                        let (eo_m, t_m) = (pct(&b.3, 0.5), pct(&b.4, 0.5));
                        let named = ["saxons", "urbino", "wallachia"].contains(&id.as_str());
                        if sa < -10.0 || sr < -10.0 || named {
                            let exp = ((eo_m - sa + sr).clamp(0.0, 100.0)) / t_m;
                            br_rows.push(format!("| {sc} | {world} | {id} | {sa:+.1} | {sr:+.1} | {:.0} % | {:.0} % |", 100.0 * eo_m / t_m, 100.0 * exp));
                        }
                    }
                }
                // armies and Ц3
                let mut cells_a = Vec::new();
                for k in KEY {
                    let w = |a: u32, b: u32| { let x: Vec<f64> = reps.iter().filter_map(|rep| rep.get(k).and_then(|v| { let y: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect(); (!y.is_empty()).then(|| y.iter().sum::<f64>() / y.len() as f64) })).collect(); if x.is_empty() { "—".to_string() } else { format!("{:.0}", pct(&x, 0.5)) } };
                    if reps.iter().any(|rep| rep.contains_key(k)) { cells_a.push(format!("{k} {} / {} / {}", w(10, 20), w(40, 50), w(100, 100))); }
                }
                army_rows.push(format!("| {sc} | {world} | {label} | {} |", cells_a.join("; ")));
                if sc == "constantinople_1430" {
                    let win = |v: &Vec<(u32, f64, f64, f64, f64)>, a: u32, b: u32| { let y: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect(); (!y.is_empty()).then(|| y.iter().sum::<f64>() / y.len() as f64) };
                    let dd: Vec<f64> = reps.iter().filter_map(|rep| { let v = rep.get("ottomans")?; Some(win(v, 40, 50)? - 1.25 * win(v, 10, 20)?) }).collect();
                    let nn = dd.len() as f64;
                    let mean = dd.iter().sum::<f64>() / nn.max(1.0);
                    let sd = (dd.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nn - 1.0).max(1.0)).sqrt();
                    let tt = if sd > 0.0 { mean / (sd / nn.sqrt()) } else { 0.0 };
                    let at = |t: u32| { let x: Vec<f64> = reps.iter().filter_map(|rep| rep.get("ottomans").and_then(|v| v.iter().find(|p| p.0 == t).map(|p| p.1))).collect(); pct(&x, 0.5) };
                    c3_rows.push(format!("| {world} | {label} | {mean:+.1} (t {tt:+.1}) {} | {:.0} → {:.0} → {:.0} → {:.0} |", if mean > 0.0 && tt >= 2.0 { "yes" } else { "**no**" }, at(41), at(42), at(46), at(50)));
                }
            }
        }
    }
    println!("## 1. Debt (items 1, 2; 3 printed) and accumulation\n");
    println!("| scenario | world | cell | (1) worst p90 ≤ 12 | (2) / (3) | monotone (games) |");
    println!("|---|---|---|---|---|---|");
    for r in debt_rows { println!("{r}"); }
    println!("\n## 2. Classes of the item-1 debtors (p90 spell > 12): class 1 — the replay with the economy shifted by the small-army rule's change (absolute → relative) brings it to ≤ 12 (before → after); class 2 — the rest. Item 1 without class 1\n");
    println!("| scenario | world | cell | worst p90 | class 1 | class 2 (p90) | item 1 without class 1 |");
    println!("|---|---|---|---|---|---|---|");
    for r in class_rows { println!("{r}"); }
    println!("\n## 3. b/r of `military_size_to_economic_output` (k 0.01, r 0.03), «down in debt + (Г)» cell: mean shift of the eo target, absolute threshold 50 against 50 × M / 100; eo / T now and expected\n");
    println!("| scenario | world | actor | shift, absolute | shift, relative | eo / T now (median) | expected eo / T |");
    println!("|---|---|---|---|---|---|---|");
    for r in br_rows { println!("{r}"); }
    println!("\n## 4. Armies of key actors (mean on ticks 10–20 / 40–50 / tick 100, median)\n");
    println!("| scenario | world | cell | armies |");
    println!("|---|---|---|---|");
    for r in army_rows { println!("{r}"); }
    println!("\n## 5. Ц3 and the Ottoman army 41 → 42 → 46 → 50\n");
    println!("| world | cell | d (t) | army |");
    println!("|---|---|---|---|");
    for r in c3_rows { println!("{r}"); }
}

/// Third round (owner's decision after PR #255): the unpaid army leaves on the same tick. Each tick
/// the army gets what the state can pay — the tick's income plus a positive treasury; the unpaid
/// share leaves (`army × (1 − u)`), the upkeep never takes the treasury below zero; recruiting toward
/// M = min(C, 0.75 F) only with a non-negative treasury; the small-army threshold from the norm (the
/// economy shifted by removing the absolute rule's mean shift). With and without the sink (Г).
/// Returns (tick, army, treasury, population, deserted this tick).
fn replay3(rows: &[Row], sink: bool, eo_shift: f64) -> Vec<(u32, f64, f64, f64, bool)> {
    let Some(first) = rows.first() else { return vec![] };
    let (mut a, mut tr) = (first.army0, first.treasury0.max(0.0));
    let mut out = Vec::with_capacity(rows.len());
    let mut prev = first.tick;
    for r in rows {
        if r.tick != prev + 1 && r.tick != first.tick { a = r.army0; tr = r.treasury0.max(0.0); }
        prev = r.tick;
        let income = (r.eo + eo_shift).clamp(0.0, 100.0) * r.pc;
        // pay the army out of the tick's income and a positive treasury; the unpaid share leaves
        let payable = income.max(0.0) + tr.max(0.0);
        let upkeep = 0.8 * a;
        let deserted = upkeep > payable + 1e-12;
        if deserted { a = payable / 0.8; tr = 0.0; } else { tr = tr.max(0.0) + income - upkeep; }
        if sink {
            let reserve = 20.0 * income.max(0.0);
            if tr > reserve { tr -= 0.05 * (tr - reserve); }
        }
        a += r.gain;
        let c = 0.767 * r.pop.max(0.0).powf(2.0 / 3.0);
        let m = c.min(0.75 * income.max(0.0) / 0.8);
        if a < m && tr >= 0.0 { a += 0.05 * (m - a); }
        let base = r.army0 + r.gain + r.rec;
        let share = if base > 0.0 { (r.loss / base).min(1.0) } else { 0.0 };
        a = (a * (1.0 - share)).max(0.0);
        tr += r.tr_gain;
        if r.tr_loss < 0.0 { tr += r.tr_loss.max(-tr.max(0.0)); }
        out.push((r.tick, a, tr, r.pop, deserted));
    }
    out
}

fn round3(first: u64, seeds: u64, ticks: u32, rec_line: u32) {
    const PEOPLES: [&str; 5] = ["alamanni", "vandals", "visigoths", "burgundians", "franks"];
    const MIDDLE: [&str; 4] = ["florence", "genoa", "papacy", "venice"];
    println!("# Ц2, third round — the unpaid army leaves on the same tick; M = min(C, 0.75 F); small-army threshold from the norm; seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    println!("Battles are not replayed (losses: the protocol's share). The economy is shifted per actor by removing the absolute small-army rule's mean shift (b / r, r = 0.03).\n");
    let mut acc_rows = Vec::new();
    let mut desert_rows = Vec::new();
    let mut army_rows = Vec::new();
    let mut middle_rows = Vec::new();
    let mut c3_rows = Vec::new();
    let mut tp_rows = Vec::new();
    let mut huns_rows = Vec::new();
    for sc in SCENARIOS {
        for world in worlds(sc) {
            let games: Vec<Game> = (first..first + seeds).map(|s| play(sc, world, s, ticks, rec_line)).collect();
            // per actor per game: the eo shift of rule 4
            let shift_of = |rows: &[Row]| -> f64 { let n = rows.len().max(1) as f64; -rows.iter().map(|r| -0.01 * (50.0 - r.army1).max(0.0) / 0.03).sum::<f64>() / n };
            for sink in [false, true] {
                let label = if sink { "rule + (Г)" } else { "rule" };
                let reps: Vec<Replayed3> = games.iter().map(|g| g.iter().map(|(k, v)| (k.clone(), replay3(v, sink, shift_of(v)))).collect()).collect();
                // accumulation
                let mut mono: BTreeMap<String, u32> = BTreeMap::new();
                for rep in &reps { for (id, v) in rep {
                    let st: Option<Vec<f64>> = (0..=15).map(|i| v.iter().find(|p| p.0 == i * 10).map(|p| p.2)).collect();
                    if let Some(st) = st { if st.windows(2).all(|w| w[1] >= w[0]) && st[15] > st[0] { *mono.entry(id.clone()).or_default() += 1; } }
                } }
                let monos: Vec<String> = mono.iter().filter(|(_, c)| **c as u64 * 2 >= seeds).map(|(k, c)| format!("{k} {c}")).collect();
                acc_rows.push(format!("| {sc} | {world} | {label} | {} |", if monos.is_empty() { "—".into() } else { monos.join(", ") }));
                // desertion
                let mut des: BTreeMap<String, (u64, u64)> = BTreeMap::new();
                for rep in &reps { for (id, v) in rep { let e = des.entry(id.clone()).or_default(); for x in v.iter().filter(|x| x.3 > 1.0) { e.0 += 1; e.1 += x.4 as u64; } } }
                let (tot, dt): (u64, u64) = des.values().fold((0, 0), |a, v| (a.0 + v.0, a.1 + v.1));
                let persistent: Vec<String> = des.iter().filter(|(_, v)| v.1 * 5 > v.0).map(|(k, v)| format!("{k} {:.0}", 100.0 * v.1 as f64 / v.0.max(1) as f64)).collect();
                desert_rows.push(format!("| {sc} | {world} | {label} | {:.1} % | {} |", 100.0 * dt as f64 / tot.max(1) as f64, if persistent.is_empty() { "—".into() } else { persistent.join(", ") }));
                if sc == "milan_1477" {
                    let cells: Vec<String> = MIDDLE.iter().filter_map(|m| des.get(*m).map(|v| format!("{m} {:.1} %", 100.0 * v.1 as f64 / v.0.max(1) as f64))).collect();
                    middle_rows.push(format!("| {world} | {label} | {} |", cells.join(", ")));
                }
                // armies
                let at = |k: &str, t: u32| -> String { let x: Vec<f64> = reps.iter().filter_map(|rep| rep.get(k).and_then(|v| v.iter().find(|p| p.0 == t).map(|p| p.1))).collect(); if x.is_empty() { "—".into() } else { format!("{:.0}", pct(&x, 0.5)) } };
                let win = |k: &str, a: u32, b: u32| -> String { let x: Vec<f64> = reps.iter().filter_map(|rep| rep.get(k).and_then(|v| { let y: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect(); (!y.is_empty()).then(|| y.iter().sum::<f64>() / y.len() as f64) })).collect(); if x.is_empty() { "—".into() } else { format!("{:.0}", pct(&x, 0.5)) } };
                let mut cells_a = Vec::new();
                for k in ["rome", "huns", "ottomans", "byzantium", "milan"] {
                    if reps.iter().any(|rep| rep.contains_key(k)) { cells_a.push(format!("{k} {} / {} / {} / {} / {} / {}", at(k, 0), at(k, 1), at(k, 2), win(k, 10, 20), win(k, 40, 50), at(k, 100))); }
                }
                if sc == "rome_375" {
                    let sum_at = |t: u32| { let x: Vec<f64> = reps.iter().map(|rep| PEOPLES.iter().filter_map(|p| rep.get(*p).and_then(|v| v.iter().find(|q| q.0 == t).map(|q| q.1))).sum()).collect(); format!("{:.0}", pct(&x, 0.5)) };
                    cells_a.push(format!("five peoples (sum) {} / {} / {} / {} / {} / {}", sum_at(0), sum_at(1), sum_at(2), sum_at(15), sum_at(45), sum_at(100)));
                    // huns: the stop condition — ticks 10–20 against the protocol
                    let prot: Vec<f64> = games.iter().filter_map(|g| g.get("huns").map(|v| { let y: Vec<f64> = v.iter().filter(|r| (10..=20).contains(&r.tick)).map(|r| r.army1).collect(); y.iter().sum::<f64>() / y.len().max(1) as f64 })).collect();
                    let now: Vec<f64> = reps.iter().filter_map(|rep| rep.get("huns").map(|v| { let y: Vec<f64> = v.iter().filter(|p| (10..=20).contains(&p.0)).map(|p| p.1).collect(); y.iter().sum::<f64>() / y.len().max(1) as f64 })).collect();
                    let (p, n) = (pct(&prot, 0.5), pct(&now, 0.5));
                    huns_rows.push(format!("| {world} | {label} | {p:.0} | {n:.0} | {:.0} % | {} |", 100.0 * n / p, if n < p / 3.0 { "**below a third — stop**" } else { "ok" }));
                }
                army_rows.push(format!("| {sc} | {world} | {label} | {} |", cells_a.join("; ")));
                // Ц3
                if sc == "constantinople_1430" {
                    let w = |v: &Vec<(u32, f64, f64, f64, bool)>, a: u32, b: u32| { let y: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect(); (!y.is_empty()).then(|| y.iter().sum::<f64>() / y.len() as f64) };
                    let dd: Vec<f64> = reps.iter().filter_map(|rep| { let v = rep.get("ottomans")?; Some(w(v, 40, 50)? - 1.25 * w(v, 10, 20)?) }).collect();
                    let nn = dd.len() as f64;
                    let mean = dd.iter().sum::<f64>() / nn.max(1.0);
                    let sd = (dd.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nn - 1.0).max(1.0)).sqrt();
                    let tt = if sd > 0.0 { mean / (sd / nn.sqrt()) } else { 0.0 };
                    c3_rows.push(format!("| {world} | {label} | {mean:+.1} (t {tt:+.1}) | {} |", if mean > 0.0 && tt >= 2.0 { "yes" } else { "**no — stop**" }));
                }
                // T_p of the neighbours of huns and ottomans
                for big in ["huns", "ottomans"] {
                    let mut tps_p = Vec::new();
                    let mut tps_c = Vec::new();
                    for (g, rep) in games.iter().zip(&reps) {
                        let Some(brows) = g.get(big) else { continue };
                        let army_at = |id: &str, t: u32, replayed: bool| -> f64 { if replayed { rep.get(id).and_then(|v| v.iter().find(|p| p.0 == t)).map_or(0.0, |p| p.1) } else { g.get(id).and_then(|v| v.iter().find(|r| r.tick == t)).map_or(0.0, |r| r.army1) } };
                        for nbid in brows.first().map(|r| r.nb.iter().map(|x| x.0.clone()).collect::<Vec<_>>()).unwrap_or_default() {
                            let Some(rows) = g.get(&nbid) else { continue };
                            for r in rows.iter().filter(|r| r.tick % 5 == 0) {
                                for (replayed, out) in [(false, &mut tps_p), (true, &mut tps_c)] {
                                    let nsum: f64 = r.nb.iter().map(|(o, w)| w * army_at(o, r.tick, replayed)).sum();
                                    let own = army_at(&nbid, r.tick, replayed);
                                    out.push(if nsum <= 0.0 { 0.0 } else { 100.0 * nsum / (nsum + own) });
                                }
                            }
                        }
                    }
                    if !tps_p.is_empty() { tp_rows.push(format!("| {sc} | {world} | {label} | {big} | {:.0} → {:.0} |", pct(&tps_p, 0.5), pct(&tps_c, 0.5))); }
                }
            }
        }
    }
    println!("## Stop conditions\n\n### The huns' army on ticks 10–20 (median): protocol → rule\n");
    println!("| world | cell | protocol | rule | share | |");
    println!("|---|---|---|---|---|---|");
    for r in huns_rows { println!("{r}"); }
    println!("\n### Ц3 (Ottoman army m₄₀ ≥ 1.25 m₁₀)\n");
    println!("| world | cell | d (t) | |");
    println!("|---|---|---|---|");
    for r in c3_rows { println!("{r}"); }
    println!("\n## 1. Accumulation: treasury monotone to 150 in half the games or more\n");
    println!("| scenario | world | cell | who (games) |");
    println!("|---|---|---|---|");
    for r in acc_rows { println!("{r}"); }
    println!("\n## 2. Desertion: share of living actor-ticks the army leaves; who on more than 20 % of its ticks\n");
    println!("| scenario | world | cell | share | persistent |");
    println!("|---|---|---|---|---|");
    for r in desert_rows { println!("{r}"); }
    println!("\n## 3. Armies (median): tick 0 / 1 / 2 / mean 10–20 / mean 40–50 / tick 100 (five peoples: sum on 0 / 1 / 2 / 15 / 45 / 100)\n");
    println!("| scenario | world | cell | armies |");
    println!("|---|---|---|---|");
    for r in army_rows { println!("{r}"); }
    println!("\n## 4. milan's middle states: desertion share with and without the sink\n");
    println!("| world | cell | desertion |");
    println!("|---|---|---|");
    for r in middle_rows { println!("{r}"); }
    println!("\n## 5. T_p of the neighbours of the huns and the Ottomans (median every 5th tick), protocol → rule\n");
    println!("| scenario | world | cell | of | T_p |");
    println!("|---|---|---|---|---|");
    for r in tp_rows { println!("{r}"); }
}

/// Fourth round (owner's decision after PR #256): the militia — carriers of `tribal_confederation`
/// or `nomadic` on the tick — draw no pay from the treasury and do not desert; their norm is C. The
/// rest as in the third round (paid army, the unpaid share leaves, recruiting to min(C, 0.75 F)). The
/// small-army threshold from the norm; the sink (Г) for everyone. Returns (tick, army, treasury,
/// population, deserted, income).
fn replay4(rows: &[Row], eo_shift: f64) -> Vec<(u32, f64, f64, f64, bool, f64)> {
    let Some(first) = rows.first() else { return vec![] };
    let (mut a, mut tr) = (first.army0, first.treasury0.max(0.0));
    let mut out = Vec::with_capacity(rows.len());
    let mut prev = first.tick;
    for r in rows {
        if r.tick != prev + 1 && r.tick != first.tick { a = r.army0; tr = r.treasury0.max(0.0); }
        prev = r.tick;
        let income = (r.eo + eo_shift).clamp(0.0, 100.0) * r.pc;
        let mut deserted = false;
        if r.militia {
            tr = tr.max(0.0) + income;
        } else {
            let payable = income.max(0.0) + tr.max(0.0);
            let upkeep = 0.8 * a;
            if upkeep > payable + 1e-12 { a = payable / 0.8; tr = 0.0; deserted = true; } else { tr = tr.max(0.0) + income - upkeep; }
        }
        let reserve = 20.0 * income.max(0.0);
        if tr > reserve { tr -= 0.05 * (tr - reserve); }
        a += r.gain;
        let c = 0.767 * r.pop.max(0.0).powf(2.0 / 3.0);
        let m = if r.militia { c } else { c.min(0.75 * income.max(0.0) / 0.8) };
        if a < m && tr >= 0.0 { a += 0.05 * (m - a); }
        let base = r.army0 + r.gain + r.rec;
        let share = if base > 0.0 { (r.loss / base).min(1.0) } else { 0.0 };
        a = (a * (1.0 - share)).max(0.0);
        tr += r.tr_gain;
        if r.tr_loss < 0.0 { tr += r.tr_loss.max(-tr.max(0.0)); }
        out.push((r.tick, a, tr, r.pop, deserted, income));
    }
    out
}

/// The accumulation measure (§9.1, refined before the build): per actor the mean of treasury /
/// income on ticks 125–150 against ticks 75–100 (pooled over games, ticks with income > 0); fails
/// where the later is more than 1.1 × the earlier (and the earlier is positive or the later is).
fn accumulation(series: &[Vec<(u32, f64, f64)>]) -> Vec<(f64, f64)> {
    // series: per game, (tick, treasury, income); refined a second time (§9.1): ticks 150–199 against 250–299
    let ratio = |a: u32, b: u32| { let x: Vec<f64> = series.iter().flat_map(|v| v.iter().filter(|p| (a..=b).contains(&p.0) && p.2 > 0.0).map(|p| p.1 / p.2)).collect(); (!x.is_empty()).then(|| x.iter().sum::<f64>() / x.len() as f64) };
    match (ratio(150, 199), ratio(250, 299)) { (Some(e), Some(l)) => vec![(e, l)], _ => vec![] }
}

/// The accumulation measure refined a second time: fails where the late mean exceeds 1.25 × the
/// earlier mean + 5. Returns the margin (late − bound): negative passes.
fn accumulation_margin(e: f64, l: f64) -> f64 { l - (1.25 * e + 5.0) }

fn round4(first: u64, seeds: u64, ticks: u32, rec_line: u32) {
    const PEOPLES: [&str; 5] = ["alamanni", "vandals", "visigoths", "burgundians", "franks"];
    println!("# Ц2, fourth round — the militia (tribal_confederation / nomadic) draw no pay; the paid army as in round 3; the sink for all; seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    let mut tag_rows = Vec::new();
    let mut acc_rows = Vec::new();
    let mut desert_rows = Vec::new();
    let mut army_rows = Vec::new();
    let mut c3_rows = Vec::new();
    let mut huns_rows = Vec::new();
    let mut tp_rows = Vec::new();
    let (mut acc_now_fail, mut acc_new_fail) = (0usize, 0usize);
    for sc in SCENARIOS {
        for world in worlds(sc) {
            let games: Vec<Game> = (first..first + seeds).map(|s| play(sc, world, s, ticks, rec_line)).collect();
            // tags: holders on ticks 0, 50, 100, 299; acquired later by a state without them at the start
            let mut holders: BTreeMap<u32, BTreeMap<String, u32>> = BTreeMap::new();
            let mut acquired: BTreeMap<String, Vec<f64>> = BTreeMap::new();
            for g in &games {
                for (id, rows) in g {
                    let start = rows.first().is_some_and(|r| r.tick == 0 && r.militia);
                    for r in rows.iter().filter(|r| [0, 50, 100, 299].contains(&r.tick) && r.militia) { *holders.entry(r.tick).or_default().entry(id.clone()).or_default() += 1; }
                    if !start { if let Some(r) = rows.iter().find(|r| r.militia) { acquired.entry(id.clone()).or_default().push(r.tick as f64); } }
                }
            }
            let h = |t: u32| holders.get(&t).map_or("—".to_string(), |m| m.iter().map(|(k, c)| format!("{k} {c}")).collect::<Vec<_>>().join(", "));
            tag_rows.push(format!("| {sc} | {world} | {} | {} | {} | {} | {} |", h(0), h(50), h(100), h(299),
                if acquired.is_empty() { "—".into() } else { acquired.iter().map(|(k, v)| format!("{k} {} (from tick {:.0})", v.len(), pct(v, 0.5))).collect::<Vec<_>>().join(", ") }));
            let shift_of = |rows: &[Row]| -> f64 { let n = rows.len().max(1) as f64; -rows.iter().map(|r| -0.01 * (50.0 - r.army1).max(0.0) / 0.03).sum::<f64>() / n };
            let reps: Vec<Replayed4> = games.iter().map(|g| g.iter().map(|(k, v)| (k.clone(), replay4(v, shift_of(v)))).collect()).collect();
            // accumulation, both ways: the protocol (no sink) and the rule
            let mut fails_now = Vec::new();
            let mut fails_new = Vec::new();
            let mut near_now: Vec<(f64, String)> = Vec::new();
            let mut near_new: Vec<(f64, String)> = Vec::new();
            let ids: Vec<String> = games.iter().flat_map(|g| g.keys().cloned()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            for id in &ids {
                let now: Vec<Vec<(u32, f64, f64)>> = games.iter().filter_map(|g| g.get(id).map(|v| v.iter().map(|r| (r.tick, r.treasury1, r.income)).collect())).collect();
                let new: Vec<Vec<(u32, f64, f64)>> = reps.iter().filter_map(|rep| rep.get(id).map(|v| v.iter().map(|x| (x.0, x.2, x.5)).collect())).collect();
                for (series, out, near) in [(&now, &mut fails_now, &mut near_now), (&new, &mut fails_new, &mut near_new)] {
                    if let Some(&(e, l)) = accumulation(series).first() {
                        let m = accumulation_margin(e, l);
                        if m > 0.0 { out.push(format!("{id} {e:.0}→{l:.0}")); }
                        near.push((m, format!("{id} {e:.1}→{l:.1} (bound {:.1})", 1.25 * e + 5.0)));
                    }
                }
            }
            acc_now_fail += (!fails_now.is_empty()) as usize;
            acc_new_fail += (!fails_new.is_empty()) as usize;
            near_now.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            near_new.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            acc_rows.push(format!("| {sc} | {world} | {} | {} | {} |", if fails_now.is_empty() { format!("passes (closest: {})", near_now.first().map_or("—".to_string(), |x| x.1.clone())) } else { format!("**fails**: {}", fails_now.join(", ")) },
                if fails_new.is_empty() { "**passes**".into() } else { format!("fails: {}", fails_new.join(", ")) },
                near_new.iter().take(3).map(|x| x.1.clone()).collect::<Vec<_>>().join("; ")));
            // desertion
            let mut des: BTreeMap<String, (u64, u64)> = BTreeMap::new();
            for rep in &reps { for (id, v) in rep { let e = des.entry(id.clone()).or_default(); for x in v.iter().filter(|x| x.3 > 1.0) { e.0 += 1; e.1 += x.4 as u64; } } }
            let (tot, dt): (u64, u64) = des.values().fold((0, 0), |a, v| (a.0 + v.0, a.1 + v.1));
            let persistent: Vec<String> = des.iter().filter(|(_, v)| v.1 * 5 > v.0).map(|(k, v)| format!("{k} {:.0}", 100.0 * v.1 as f64 / v.0.max(1) as f64)).collect();
            desert_rows.push(format!("| {sc} | {world} | {:.1} % | {} |", 100.0 * dt as f64 / tot.max(1) as f64, if persistent.is_empty() { "—".into() } else { persistent.join(", ") }));
            // armies
            let at = |k: &str, t: u32| -> String { let x: Vec<f64> = reps.iter().filter_map(|rep| rep.get(k).and_then(|v| v.iter().find(|p| p.0 == t).map(|p| p.1))).collect(); if x.is_empty() { "—".into() } else { format!("{:.0}", pct(&x, 0.5)) } };
            let win = |k: &str, a: u32, b: u32| -> String { let x: Vec<f64> = reps.iter().filter_map(|rep| rep.get(k).and_then(|v| { let y: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect(); (!y.is_empty()).then(|| y.iter().sum::<f64>() / y.len() as f64) })).collect(); if x.is_empty() { "—".into() } else { format!("{:.0}", pct(&x, 0.5)) } };
            let mut cells_a = Vec::new();
            for k in ["rome", "huns", "ottomans", "byzantium", "milan"] {
                if reps.iter().any(|rep| rep.contains_key(k)) { cells_a.push(format!("{k} {} / {} / {} / {} / {} / {}", at(k, 0), at(k, 1), at(k, 2), win(k, 10, 20), win(k, 40, 50), at(k, 100))); }
            }
            if sc == "rome_375" {
                let sum_at = |t: u32| { let x: Vec<f64> = reps.iter().map(|rep| PEOPLES.iter().filter_map(|p| rep.get(*p).and_then(|v| v.iter().find(|q| q.0 == t).map(|q| q.1))).sum()).collect(); format!("{:.0}", pct(&x, 0.5)) };
                let psum = |t: u32| { let x: Vec<f64> = games.iter().map(|g| PEOPLES.iter().filter_map(|p| g.get(*p).and_then(|v| v.iter().find(|r| r.tick == t).map(|r| r.army1))).sum()).collect(); format!("{:.0}", pct(&x, 0.5)) };
                cells_a.push(format!("five peoples (sum) {} / {} / {} / {} / {} / {} (protocol {} / {} / {})", sum_at(0), sum_at(1), sum_at(2), sum_at(15), sum_at(45), sum_at(100), psum(15), psum(45), psum(100)));
                let prot: Vec<f64> = games.iter().filter_map(|g| g.get("huns").map(|v| { let y: Vec<f64> = v.iter().filter(|r| (10..=20).contains(&r.tick)).map(|r| r.army1).collect(); y.iter().sum::<f64>() / y.len().max(1) as f64 })).collect();
                let now: Vec<f64> = reps.iter().filter_map(|rep| rep.get("huns").map(|v| { let y: Vec<f64> = v.iter().filter(|p| (10..=20).contains(&p.0)).map(|p| p.1).collect(); y.iter().sum::<f64>() / y.len().max(1) as f64 })).collect();
                let (p, n) = (pct(&prot, 0.5), pct(&now, 0.5));
                huns_rows.push(format!("| {world} | {p:.0} | {n:.0} | {:.0} % | {} |", 100.0 * n / p, if n < p / 3.0 { "**below a third — stop**" } else { "ok" }));
            }
            army_rows.push(format!("| {sc} | {world} | {} |", cells_a.join("; ")));
            if sc == "constantinople_1430" {
                let w = |v: &Vec<(u32, f64, f64, f64, bool, f64)>, a: u32, b: u32| { let y: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect(); (!y.is_empty()).then(|| y.iter().sum::<f64>() / y.len() as f64) };
                let dd: Vec<f64> = reps.iter().filter_map(|rep| { let v = rep.get("ottomans")?; Some(w(v, 40, 50)? - 1.25 * w(v, 10, 20)?) }).collect();
                let nn = dd.len() as f64;
                let mean = dd.iter().sum::<f64>() / nn.max(1.0);
                let sd = (dd.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nn - 1.0).max(1.0)).sqrt();
                let tt = if sd > 0.0 { mean / (sd / nn.sqrt()) } else { 0.0 };
                c3_rows.push(format!("| {world} | {mean:+.1} (t {tt:+.1}) | {} |", if mean > 0.0 && tt >= 2.0 { "yes" } else { "**no — stop**" }));
            }
            for big in ["huns", "ottomans"] {
                let mut tps_p = Vec::new();
                let mut tps_c = Vec::new();
                for (g, rep) in games.iter().zip(&reps) {
                    let Some(brows) = g.get(big) else { continue };
                    let army_at = |id: &str, t: u32, replayed: bool| -> f64 { if replayed { rep.get(id).and_then(|v| v.iter().find(|p| p.0 == t)).map_or(0.0, |p| p.1) } else { g.get(id).and_then(|v| v.iter().find(|r| r.tick == t)).map_or(0.0, |r| r.army1) } };
                    for nbid in brows.first().map(|r| r.nb.iter().map(|x| x.0.clone()).collect::<Vec<_>>()).unwrap_or_default() {
                        let Some(rows) = g.get(&nbid) else { continue };
                        for r in rows.iter().filter(|r| r.tick % 5 == 0) {
                            for (replayed, out) in [(false, &mut tps_p), (true, &mut tps_c)] {
                                let nsum: f64 = r.nb.iter().map(|(o, w)| w * army_at(o, r.tick, replayed)).sum();
                                let own = army_at(&nbid, r.tick, replayed);
                                out.push(if nsum <= 0.0 { 0.0 } else { 100.0 * nsum / (nsum + own) });
                            }
                        }
                    }
                }
                if !tps_p.is_empty() { tp_rows.push(format!("| {sc} | {world} | {big} | {:.0} → {:.0} |", pct(&tps_p, 0.5), pct(&tps_c, 0.5))); }
            }
        }
    }
    println!("## 1. Who carries `tribal_confederation` / `nomadic` (games of {seeds}) on ticks 0 / 50 / 100 / 299; acquired later by a state without them at the start\n");
    println!("| scenario | world | 0 | 50 | 100 | 299 | acquired later |");
    println!("|---|---|---|---|---|---|---|");
    for r in tag_rows { println!("{r}"); }
    println!("\n## 2. Stop conditions\n\n### The huns' army on ticks 10–20 (median): protocol → rule\n");
    println!("| world | protocol | rule | share | |");
    println!("|---|---|---|---|---|");
    for r in huns_rows { println!("{r}"); }
    println!("\n### Ц3\n");
    println!("| world | d (t) | |");
    println!("|---|---|---|");
    for r in c3_rows { println!("{r}"); }
    println!("\n## 3. The accumulation measure, both ways: mean treasury / income on ticks 250–299 ≤ 1.25 × on ticks 150–199 + 5, every actor (failing actors: earlier → later)\n");
    println!("| scenario | world | the world now (protocol, no sink) — must fail | the rule with the sink — must pass | the rule: closest to the bound |");
    println!("|---|---|---|---|---|");
    for r in acc_rows { println!("{r}"); }
    println!("\nThe world now fails in {acc_now_fail} of 10 worlds; the rule fails in {acc_new_fail} of 10.");
    println!("\n## 4. Desertion: share of living actor-ticks; who on more than 20 % of its ticks\n");
    println!("| scenario | world | share | persistent |");
    println!("|---|---|---|---|");
    for r in desert_rows { println!("{r}"); }
    println!("\n## 5. Armies (median): tick 0 / 1 / 2 / mean 10–20 / mean 40–50 / tick 100\n");
    println!("| scenario | world | armies |");
    println!("|---|---|---|");
    for r in army_rows { println!("{r}"); }
    println!("\n## 6. T_p of the neighbours of the huns and the Ottomans, protocol → rule\n");
    println!("| scenario | world | of | T_p |");
    println!("|---|---|---|---|");
    for r in tp_rows { println!("{r}"); }
}

/// Why Trebizond (and the mamluks) cross the accumulation bound under the rule: per window of 50
/// ticks, the medians of income, upkeep, other treasury writes (gains / losses), treasury, the
/// reserve, army and population, constantinople, every world.
fn trebizond(first: u64, seeds: u64, ticks: u32, rec_line: u32) {
    println!("# Trebizond and the mamluks under the rule (round 4 with the sink), seeds {first}–{}\n", first + seeds - 1);
    println!("| world | actor | ticks | income | upkeep (paid) | other treasury +/− | treasury | T / income | army | population |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for world in worlds("constantinople_1430") {
        let games: Vec<Game> = (first..first + seeds).map(|s| play("constantinople_1430", world, s, ticks, rec_line)).collect();
        for id in ["trebizond", "mamluks"] {
            for (a, b) in [(0u32, 49u32), (50, 99), (100, 149), (150, 199), (200, 249), (250, 299)] {
                let mut cols: [Vec<f64>; 8] = Default::default();
                for g in &games {
                    let Some(rows) = g.get(id) else { continue };
                    let shift = { let n = rows.len().max(1) as f64; -rows.iter().map(|r| -0.01 * (50.0 - r.army1).max(0.0) / 0.03).sum::<f64>() / n };
                    let rep = replay4(rows, shift);
                    for (r, x) in rows.iter().zip(&rep).filter(|(r, _)| (a..=b).contains(&r.tick)) {
                        cols[0].push(x.5);
                        cols[1].push(if r.militia { 0.0 } else { 0.8 * r.army0 });
                        cols[2].push(r.tr_gain);
                        cols[3].push(r.tr_loss);
                        cols[4].push(x.2);
                        if x.5 > 0.0 { cols[5].push(x.2 / x.5); }
                        cols[6].push(x.1);
                        cols[7].push(x.3);
                    }
                }
                if cols[0].is_empty() { continue; }
                let m = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len().max(1) as f64;
                println!("| {world} | {id} | {a}–{b} | {:.2} | {:.2} | +{:.2} / {:.2} | {:.1} | {:.1} | {:.1} | {:.0} |", m(&cols[0]), m(&cols[1]), m(&cols[2]), m(&cols[3]), m(&cols[4]), m(&cols[5]), m(&cols[6]), m(&cols[7]));
            }
        }
    }
}
