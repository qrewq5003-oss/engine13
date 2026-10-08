//! Economy project, Ц3 picture: the Ottoman army on v2 as in the content (Ц1, Ц4, Ц5, Ц6, Ц8; Ц7
//! off) — no new model (docs/economy_project_brief.md §9). Built with `--features census`.
//! constantinople, four worlds, 30 seeds × 300 ticks; seeds from an argument (0 and 100).
//!
//! 1. The army's median on tick 0, ticks 2–5, 10–20, 40–50 and tick 100; against the start (180)
//!    and against ticks 10–20, paired by seed (per seed: the window's mean minus the reference).
//! 2. The fall on ticks 2–5 by write source (census), mean per game.
//! 3. The mobilisation capacity (`0.767 × population^(2/3)`), the share of living ticks the army is
//!    above it, and which writers put it there: the writes by source on ticks the army ends above
//!    capacity.
//!
//! Usage: cargo run --release --features census --bin c3_probe -- [first_seed] [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const WORLDS: [&str; 4] = ["none", "balanced", "diplomacy", "military"];

#[derive(Default)]
struct Run {
    /// (tick, army, capacity, population)
    army: Vec<(u32, f64, f64, f64)>,
    early: BTreeMap<String, f64>,
    above: BTreeMap<String, f64>,
}

fn run(world: &str, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, "constantinople_1430".to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        // the world of PR #242: Ц7 and Ц9 off (they are in the content since their write)
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        s.economy_v2_conquest_k2 = None;
        s.economy_v2_alliances = false;
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, "constantinople_1430"));
    let mut r = Run::default();
    {
        let ws = st.world_state.as_ref().unwrap();
        let a = &ws.actors["ottomans"];
        r.army.push((0, a.get_metric("military_size"), engine13::engine::interactions::military_capacity(a), a.get_metric("population")));
    }
    let _ = census::take_writes();
    for _ in 0..ticks {
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        let t = ws.tick; // the state after the tick that took the world to `t`
        let writes: Vec<census::Write> = census::take_writes().into_iter().filter(|w| w.actor == "ottomans" && w.metric == "military_size").collect();
        let Some(a) = ws.actors.get("ottomans").filter(|_| !ws.dead_actor_ids.contains("ottomans")) else { continue };
        let (army, cap) = (a.get_metric("military_size"), engine13::engine::interactions::military_capacity(a));
        r.army.push((t, army, cap, a.get_metric("population")));
        for w in &writes {
            let src = w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file(), w.location.line()));
            if t <= 5 { *r.early.entry(src.clone()).or_default() += w.applied; }
            if army > cap { *r.above.entry(src).or_default() += w.applied; }
        }
    }
    r
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if s.is_empty() { return f64::NAN; }
    s[((s.len() - 1) as f64 * p).round() as usize]
}

fn paired(d: &[f64]) -> String {
    let n = d.len() as f64;
    if n < 2.0 { return "—".into(); }
    let mean = d.iter().sum::<f64>() / n;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let t = if sd > 0.0 { mean / (sd / n.sqrt()) } else { 0.0 };
    format!("{mean:+.0} (t {t:+.1})")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let first: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    println!("# Ц3 picture — the Ottoman army on v2 as in the content, seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    let windows: [(&str, u32, u32); 5] = [("0", 0, 0), ("2–5", 2, 5), ("10–20", 10, 20), ("40–50", 40, 50), ("100", 100, 100)];
    let mut rows = Vec::new();
    let mut pair_rows = Vec::new();
    let mut early_rows = Vec::new();
    let mut cap_rows = Vec::new();
    for world in WORLDS {
        let runs: Vec<Run> = (first..first + seeds).map(|s| run(world, s, ticks)).collect();
        let window = |r: &Run, a: u32, b: u32| -> Option<f64> {
            let v: Vec<f64> = r.army.iter().filter(|x| (a..=b).contains(&x.0)).map(|x| x.1).collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
        };
        let mut cells = Vec::new();
        for (name, a, b) in windows {
            let all: Vec<f64> = runs.iter().flat_map(|r| r.army.iter().filter(|x| (a..=b).contains(&x.0)).map(|x| x.1)).collect();
            cells.push(format!("{:.0}", pct(&all, 0.5)));
            let vs_start: Vec<f64> = runs.iter().filter_map(|r| window(r, a, b).map(|x| x - 180.0)).collect();
            let vs_10: Vec<f64> = runs.iter().filter_map(|r| Some(window(r, a, b)? - window(r, 10, 20)?)).collect();
            pair_rows.push(format!("| {world} | {name} | {} | {} | {} |", vs_start.len(), paired(&vs_start), paired(&vs_10)));
        }
        rows.push(format!("| {world} | {} |", cells.join(" | ")));
        let mut early: BTreeMap<String, f64> = BTreeMap::new();
        for r in &runs { for (k, x) in &r.early { *early.entry(k.clone()).or_default() += x / seeds as f64; } }
        let mut ev: Vec<(String, f64)> = early.into_iter().filter(|x| x.1.abs() >= 0.5).collect();
        ev.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mean_at = |t: u32| { let v: Vec<f64> = runs.iter().filter_map(|r| r.army.iter().find(|x| x.0 == t).map(|x| x.1)).collect(); v.iter().sum::<f64>() / v.len().max(1) as f64 };
        early_rows.push(format!("| {world} | {:.0} → {:.0} → {:.0} → {:.0} → {:.0} | {} |", mean_at(0), mean_at(2), mean_at(3), mean_at(4), mean_at(5), ev.iter().map(|(k, x)| format!("{k} {x:+.1}")).collect::<Vec<_>>().join("; ")));
        let living: usize = runs.iter().map(|r| r.army.len()).sum();
        let above = runs.iter().map(|r| r.army.iter().filter(|x| x.1 > x.2).count()).sum::<usize>();
        let caps: Vec<f64> = runs.iter().flat_map(|r| r.army.iter().map(|x| x.2)).collect();
        let cap_at = |a: u32, b: u32| { let v: Vec<f64> = runs.iter().flat_map(|r| r.army.iter().filter(|x| (a..=b).contains(&x.0)).map(|x| x.2)).collect(); pct(&v, 0.5) };
        let pop_at = |a: u32, b: u32| { let v: Vec<f64> = runs.iter().flat_map(|r| r.army.iter().filter(|x| (a..=b).contains(&x.0)).map(|x| x.3)).collect(); pct(&v, 0.5) };
        let mut ab: BTreeMap<String, f64> = BTreeMap::new();
        for r in &runs { for (k, x) in &r.above { *ab.entry(k.clone()).or_default() += x / seeds as f64; } }
        let mut av: Vec<(String, f64)> = ab.into_iter().filter(|x| x.1.abs() >= 1.0).collect();
        av.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        cap_rows.push(format!("| {world} | {:.0} / {:.0} / {:.0} (all ticks p50 {:.0}) | {:.0} / {:.0} / {:.0} | {:.0} % | {} |", cap_at(0, 0), cap_at(40, 50), cap_at(100, 100), pct(&caps, 0.5),
            pop_at(0, 0), pop_at(40, 50), pop_at(100, 100), 100.0 * above as f64 / living.max(1) as f64, av.iter().map(|(k, x)| format!("{k} {x:+.0}")).collect::<Vec<_>>().join("; ")));
    }
    println!("## 1. The Ottoman army, median (the start is 180)\n");
    println!("| world | tick 0 | ticks 2–5 | ticks 10–20 | ticks 40–50 | tick 100 |");
    println!("|---|---|---|---|---|---|");
    for r in rows { println!("{r}"); }
    println!("\n### Paired by seed: the window's mean against the start (180) and against ticks 10–20\n");
    println!("| world | window | games | − 180 | − ticks 10–20 |");
    println!("|---|---|---|---|---|");
    for r in pair_rows { println!("{r}"); }
    println!("\n## 2. Ticks 0–5: the mean army, and military_size writes by source (mean per game)\n");
    println!("| world | army on ticks 0 → 2 → 3 → 4 → 5 | writes by source |");
    println!("|---|---|---|");
    for r in early_rows { println!("{r}"); }
    println!("\n## 3. Capacity (0.767 × population^(2/3)) and the army above it\n");
    println!("| world | capacity, tick 0 / 40–50 / 100 (median) | population, tick 0 / 40–50 / 100 | ticks the army is above capacity | writes on those ticks by source (mean per game, |x| ≥ 1) |");
    println!("|---|---|---|---|---|");
    for r in cap_rows { println!("{r}"); }
}
