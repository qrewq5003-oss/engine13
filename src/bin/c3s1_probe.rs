//! Economy project, Ц3 stage 1: arithmetic over the protocols, no new model
//! (docs/economy_project_brief.md §9). Built with `--features census`. constantinople, v2 as in
//! the content (Ц1, Ц4–Ц9), four worlds, 30 seeds × 300 ticks; seeds from an argument (0, 100).
//!
//! Ц3's measure (§9.1): the Ottoman army's mean on ticks 40–50 ≥ 1.25 × its mean on ticks 10–20,
//! paired by seed, t ≥ 2, every world, both seed sets. Per seed d = m₄₀ − 1.25 × m₁₀; t of d.
//!
//! 1. Why the Ottoman population does not move: its writes by source over the game.
//! 2. Candidate sources of growth, replayed over the protocol of each game:
//!    (а) the vassals' levy — the overlord's capacity counts σ of its vassals' population,
//!    σ = 0.25 and 0.5;
//!    (б) Ottoman population growth g per tick (0.25 %, 0.5 %, 1 %) — the capacity grows with it;
//!    (в) an authored dated step: how much army a one-off addition before tick 40 must bring.
//!    The replay is first-order: the army is the protocol's plus Δ; Δ shrinks with the protocol's
//!    own share of losses on each tick and grows by the difference the recovery rule
//!    (`5 % × (capacity − army)`, only below capacity) makes against the new capacity. No battle is
//!    replayed: a larger army winning more is not in it.
//! 3. For each: A10 — the coalition tax on `ottomans.military_size > 220` (ticks above, first tick),
//!    and Byzantium — Ottoman vassal before 1453 (games, tick), dies on the assault (games, tick).
//!
//! Usage: cargo run --release --features census --bin c3s1_probe -- [first_seed] [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use engine13::engine::interactions::{MILITARY_CAPACITY_EXPONENT, MILITARY_CAPACITY_K, MILITARY_RECOVERY_RATE};
use rand::SeedableRng;
use std::collections::BTreeMap;

const WORLDS: [&str; 4] = ["none", "balanced", "diplomacy", "military"];

/// One living tick of the Ottomans: (tick, army after it, population, Σ vassals' population,
/// share of the army lost on the tick, army before it)
type Row = (u32, f64, f64, f64, f64, f64);

#[derive(Default)]
struct Run {
    rows: Vec<Row>,
    pop_writes: BTreeMap<String, f64>,
    /// vassal -> first tick
    vassals: BTreeMap<String, u32>,
    byz_vassal: Option<u32>,
    byz_dead: Option<u32>,
}

fn cap(pop: f64) -> f64 { MILITARY_CAPACITY_K * pop.max(0.0).powf(MILITARY_CAPACITY_EXPONENT) }

fn run(world: &str, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, "constantinople_1430".to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        assert!(s.economy_v2_conquest_k2 == Some(1) && s.economy_v2_alliances, "Ц7 and Ц9 are in the content");
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, "constantinople_1430"));
    let mut r = Run::default();
    let _ = census::take_writes();
    for _ in 0..ticks {
        let before = st.world_state.as_ref().unwrap().actors.get("ottomans").map_or(0.0, |a| a.get_metric("military_size"));
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick - 1;
        let mut lost = 0.0;
        for w in census::take_writes().into_iter().filter(|w| w.actor == "ottomans") {
            if w.metric == "population" {
                let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
                *r.pop_writes.entry(src).or_default() += w.applied;
            }
            if w.metric == "military_size" && w.applied < 0.0 { lost -= w.applied; }
        }
        for v in ws.vassalages.iter().filter(|v| v.overlord_id == "ottomans") { r.vassals.entry(v.vassal_id.clone()).or_insert(t); }
        if r.byz_vassal.is_none() && ws.vassalages.iter().any(|v| v.vassal_id == "byzantium" && v.overlord_id == "ottomans") { r.byz_vassal = Some(t); }
        if r.byz_dead.is_none() && ws.dead_actor_ids.contains("byzantium") { r.byz_dead = Some(t); }
        let Some(a) = ws.actors.get("ottomans").filter(|_| !ws.dead_actor_ids.contains("ottomans")) else { continue };
        let vpop: f64 = ws.vassalages.iter().filter(|v| v.overlord_id == "ottomans" && !ws.dead_actor_ids.contains(&v.vassal_id))
            .filter_map(|v| ws.actors.get(&v.vassal_id)).map(|x| x.get_metric("population").max(0.0)).sum();
        let share = if before > 0.0 { (lost / before).min(1.0) } else { 0.0 };
        r.rows.push((t, a.get_metric("military_size"), a.get_metric("population"), vpop, share, before));
    }
    r
}

/// The army of a candidate, replayed over the protocol: `new_cap(row)` is the candidate's capacity
/// on that tick; Δ carries over with the tick's losses and the recovery difference.
fn replay(r: &Run, new_cap: &dyn Fn(&Row) -> f64) -> Vec<(u32, f64)> {
    let mut delta = 0.0;
    r.rows.iter().map(|row| {
        let (t, army, pop, _, share, before) = *row;
        let c0 = cap(pop);
        let c1 = new_cap(row);
        let rec0 = if before < c0 { (c0 - before) * MILITARY_RECOVERY_RATE } else { 0.0 };
        let b1 = before + delta;
        let rec1 = if b1 < c1 { (c1 - b1) * MILITARY_RECOVERY_RATE } else { 0.0 };
        delta = (delta + rec1 - rec0) * (1.0 - share);
        (t, army + delta)
    }).collect()
}

/// The army with a one-off step `x` on tick `t0` (after the tick's battles), replayed over the
/// protocol: the step shrinks with the tick's share of losses and, below capacity, with the
/// recovery it displaces (the capacity is unchanged).
fn replay_step(r: &Run, t0: u32, x: f64) -> Vec<(u32, f64)> {
    let mut delta = 0.0;
    r.rows.iter().map(|row| {
        let (t, army, pop, _, share, before) = *row;
        let c = cap(pop);
        let rec0 = if before < c { (c - before) * MILITARY_RECOVERY_RATE } else { 0.0 };
        let b1 = before + delta;
        let rec1 = if b1 < c { (c - b1) * MILITARY_RECOVERY_RATE } else { 0.0 };
        delta = (delta + rec1 - rec0) * (1.0 - share);
        if t == t0 { delta += x; }
        (t, army + delta)
    }).collect()
}

fn window(v: &[(u32, f64)], a: u32, b: u32) -> Option<f64> {
    let x: Vec<f64> = v.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.1).collect();
    (!x.is_empty()).then(|| x.iter().sum::<f64>() / x.len() as f64)
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if s.is_empty() { return f64::NAN; }
    s[((s.len() - 1) as f64 * p).round() as usize]
}

fn q(v: &[f64]) -> String {
    if v.is_empty() { return "—".into(); }
    format!("{:.0}/{:.0}/{:.0}", pct(v, 0.1), pct(v, 0.5), pct(v, 0.9))
}

/// Ц3's measure over the armies of a world's games: (mean ratio m₄₀/m₁₀, mean d, t, passes);
/// and A10: (share of living ticks above 220, first tick above 220 p10/50/90 of the games with one).
fn measure(armies: &[Vec<(u32, f64)>]) -> String {
    let pairs: Vec<(f64, f64)> = armies.iter().filter_map(|v| Some((window(v, 10, 20)?, window(v, 40, 50)?))).collect();
    let d: Vec<f64> = pairs.iter().map(|(m10, m40)| m40 - 1.25 * m10).collect();
    let n = d.len() as f64;
    let mean = d.iter().sum::<f64>() / n.max(1.0);
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
    let ratio = pairs.iter().map(|(a, b)| b / a).sum::<f64>() / n.max(1.0);
    let living: usize = armies.iter().map(|v| v.len()).sum();
    let above: usize = armies.iter().map(|v| v.iter().filter(|p| p.1 > 220.0).count()).sum();
    let first: Vec<f64> = armies.iter().filter_map(|v| v.iter().find(|p| p.1 > 220.0).map(|p| p.0 as f64)).collect();
    format!("{ratio:.3} | {mean:+.1} (t {t:+.1}) {} | {:.0} % | {} @ {} |", if mean > 0.0 && t >= 2.0 { "**yes**" } else { "no" },
        100.0 * above as f64 / living.max(1) as f64, first.len(), q(&first))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let first: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    if args.get(4).map(String::as_str) == Some("step") {
        println!("# Ц3: the owner's step — +60 Ottoman army on tick 42 (`mehmed_rises`), replayed over the protocols, seeds {first}–{}\n", first + seeds - 1);
        println!("| world | candidate | mean m₄₀ / m₁₀ | d = m₄₀ − 1.25 m₁₀ (t), passes | living ticks > 220 | games with a tick > 220 @ first tick |");
        println!("|---|---|---|---|---|---|");
        for world in WORLDS {
            let runs: Vec<Run> = (first..first + seeds).map(|s| run(world, s, ticks)).collect();
            let protocol: Vec<Vec<(u32, f64)>> = runs.iter().map(|r| r.rows.iter().map(|x| (x.0, x.1)).collect()).collect();
            println!("| {world} | protocol | {}", measure(&protocol));
            let kept: Vec<Vec<(u32, f64)>> = protocol.iter().map(|v| v.iter().map(|p| (p.0, p.1 + if p.0 >= 42 { 60.0 } else { 0.0 })).collect()).collect();
            println!("| {world} | +60 from tick 42, kept whole (upper bound) | {}", measure(&kept));
            let step: Vec<Vec<(u32, f64)>> = runs.iter().map(|r| replay_step(r, 42, 60.0)).collect();
            println!("| {world} | +60 on tick 42, replayed | {}", measure(&step));
        }
        return;
    }
    println!("# Ц3 stage 1 — arithmetic over the protocols, constantinople v2 as in the content (Ц7, Ц9 on), seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    let mut pop_rows = Vec::new();
    let mut vassal_rows = Vec::new();
    let mut cand_rows = Vec::new();
    let mut step_rows = Vec::new();
    let mut byz_rows = Vec::new();
    for world in WORLDS {
        let runs: Vec<Run> = (first..first + seeds).map(|s| run(world, s, ticks)).collect();
        // ---- 1. population writes
        let mut pw: BTreeMap<String, f64> = BTreeMap::new();
        for r in &runs { for (k, x) in &r.pop_writes { *pw.entry(k.clone()).or_default() += x / seeds as f64; } }
        let pops: Vec<f64> = runs.iter().flat_map(|r| r.rows.iter().map(|x| x.2)).collect();
        pop_rows.push(format!("| {world} | {} | {:.0} … {:.0} | {} |", runs.iter().map(|r| r.rows.len()).sum::<usize>(),
            pops.iter().cloned().fold(f64::MAX, f64::min), pops.iter().cloned().fold(f64::MIN, f64::max),
            if pw.is_empty() { "none".into() } else { pw.iter().map(|(k, x)| format!("{k} {x:+.2}")).collect::<Vec<_>>().join("; ") }));
        // ---- vassals and their population
        let mut vs: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for r in &runs { for (k, t) in &r.vassals { vs.entry(k.clone()).or_default().push(*t as f64); } }
        let vp = |a: u32, b: u32| { let x: Vec<f64> = runs.iter().flat_map(|r| r.rows.iter().filter(|p| (a..=b).contains(&p.0)).map(|p| p.3)).collect(); pct(&x, 0.5) };
        vassal_rows.push(format!("| {world} | {} | {:.0} / {:.0} / {:.0} |", vs.iter().map(|(k, t)| format!("{k} {} @ {}", t.len(), q(t))).collect::<Vec<_>>().join("; "), vp(10, 20), vp(40, 50), vp(100, 100)));
        // ---- 2. candidates
        let protocol: Vec<Vec<(u32, f64)>> = runs.iter().map(|r| r.rows.iter().map(|x| (x.0, x.1)).collect()).collect();
        cand_rows.push(format!("| {world} | protocol (no candidate) | {}", measure(&protocol)));
        for sigma in [0.25, 0.5] {
            let a: Vec<Vec<(u32, f64)>> = runs.iter().map(|r| replay(r, &|row: &Row| cap(row.2 + sigma * row.3))).collect();
            cand_rows.push(format!("| {world} | (а) vassals' levy σ = {sigma} | {}", measure(&a)));
        }
        for g in [0.0025, 0.005, 0.01] {
            let a: Vec<Vec<(u32, f64)>> = runs.iter().map(|r| {
                let p0 = r.rows.first().map_or(0.0, |x| x.2);
                replay(r, &|row: &Row| cap(p0 * (1.0f64 + g).powi(row.0 as i32 + 1) + (row.2 - p0)))
            }).collect();
            cand_rows.push(format!("| {world} | (б) population + {:.2} % a tick | {}", 100.0 * g, measure(&a)));
        }
        // ---- (в) the step a game needs: 1.25 × m₁₀ − m₄₀, if positive
        let need: Vec<f64> = protocol.iter().filter_map(|v| Some((1.25 * window(v, 10, 20)? - window(v, 40, 50)?).max(0.0))).collect();
        let m10: Vec<f64> = protocol.iter().filter_map(|v| window(v, 10, 20)).collect();
        step_rows.push(format!("| {world} | {} | {} | {} |", q(&m10), q(&need), need.iter().filter(|x| **x > 0.0).count()));
        // ---- Byzantium
        let bv: Vec<f64> = runs.iter().filter_map(|r| r.byz_vassal.map(|t| t as f64)).collect();
        let bd: Vec<f64> = runs.iter().filter_map(|r| r.byz_dead.map(|t| t as f64)).collect();
        let before46 = bd.iter().filter(|t| **t < 46.0).count();
        byz_rows.push(format!("| {world} | {} @ {} | {} @ {} | {before46} |", bv.len(), q(&bv), bd.len(), q(&bd)));
    }
    println!("## 1. The Ottoman population: range over living ticks, and its writes by source (mean per game)\n");
    println!("| world | living ticks | population min … max | population writes by source |");
    println!("|---|---|---|---|");
    for r in pop_rows { println!("{r}"); }
    println!("\n### The Ottomans' vassals (games @ first tick p10/50/90) and their living population, median on ticks 10–20 / 40–50 / 100\n");
    println!("| world | vassals | Σ vassal population |");
    println!("|---|---|---|");
    for r in vassal_rows { println!("{r}"); }
    println!("\n## 2. Candidates: Ц3's measure and A10 (army > 220)\n");
    println!("| world | candidate | mean m₄₀ / m₁₀ | d = m₄₀ − 1.25 m₁₀ (t), passes | living ticks > 220 | games with a tick > 220 @ first tick |");
    println!("|---|---|---|---|---|---|");
    for r in cand_rows { println!("{r}"); }
    println!("\n### (в) The step a game needs: m₁₀ (army on ticks 10–20) and 1.25 × m₁₀ − m₄₀, p10/50/90\n");
    println!("| world | m₁₀ | needed on ticks 40–50 | games needing one |");
    println!("|---|---|---|---|");
    for r in step_rows { println!("{r}"); }
    println!("\n## 3. Byzantium (the same games; no candidate changes a battle in the replay)\n");
    println!("| world | Ottoman vassal: games @ tick | dies: games @ tick | dies before the assault (tick 46) |");
    println!("|---|---|---|---|");
    for r in byz_rows { println!("{r}"); }
}
