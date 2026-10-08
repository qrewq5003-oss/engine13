//! Economy project, Ц2 and population — diagnosis on the world as it is now (docs/economy_project_brief.md
//! §9). Built with `--features census`. v2 as in the content (Ц1, Ц3–Ц9), every world, 30 seeds × 300
//! ticks; seeds from an argument (0, 100). No engine change.
//!
//! 1. **Debt, the corrected pre-commitment (§9.7):** the debt rule off and at x = 5 / 10 / 20 % (4 ticks,
//!    as in the content). (1) every world: p90 debt spell ≤ 12 ticks, zombie ticks (population ≤ 1)
//!    not counted; (2) worlds without a player: nobody in debt on more than 20 % of living ticks;
//!    (3) played worlds: the share printed; (4) key actors' deaths not rising (paired t < 3, against
//!    the rule off). Who is in debt and its class; the frozen debt of huns, saxons, Urbino, Sicily.
//! 2. **Treasury accumulation** (rule off): whose treasury rises monotonically to tick 150 (every
//!    10-tick step from 0 to 150 non-decreasing), and its parts over ticks 0–150: income, upkeep,
//!    other writes by source; what recruiting adds to the army (recruiting is not paid from the
//!    treasury — `apply_military_recovery` only checks solvency).
//! 3. **Population, stage 0** (rule off): writers by source per scenario; zombies (living, population
//!    ≤ 1); Rome's depopulation; and the b/r table for «population pulled to its authored norm»,
//!    r = 0.01: per rate writer, the share of living actor-ticks it acts, its relative rate ρ = loss /
//!    population, and the equilibrium P*/N = r / (r + ρ) at its mean and p90 ρ.
//!
//! Usage: cargo run --release --features census --bin c2d_probe -- [first_seed] [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::BTreeMap;

const SCENARIOS: [&str; 3] = ["rome_375", "constantinople_1430", "milan_1477"];
const CUTS: [Option<f64>; 4] = [None, Some(0.05), Some(0.10), Some(0.20)];
const KEY: [&str; 4] = ["rome", "byzantium", "ottomans", "milan"];
const FROZEN: [&str; 4] = ["huns", "saxons", "urbino", "sicily"];
const R: f64 = 0.01;

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

fn q(v: &[f64]) -> String {
    if v.is_empty() { return "—".into(); }
    format!("{:.0}/{:.0}/{:.0}", pct(v, 0.1), pct(v, 0.5), pct(v, 0.9))
}

/// One living tick of an actor: (tick, treasury, population, income, upkeep)
type Tick = (u32, f64, f64, f64, f64);

#[derive(Default)]
struct Run {
    ticks: BTreeMap<String, Vec<Tick>>,
    dead: BTreeMap<String, u32>,
    /// (actor, source) -> treasury written on ticks 0–150
    treasury_src: BTreeMap<(String, String), f64>,
    /// (actor, source) -> army added (positive writes) on ticks 0–150
    army_src: BTreeMap<(String, String), f64>,
    /// source -> (written −, written +) population, all actors
    pop_src: BTreeMap<String, (f64, f64)>,
    /// rate source -> relative rates ρ of the actor-ticks it acted on
    rho: BTreeMap<String, Vec<f64>>,
    /// living actor-ticks
    living: u64,
    /// actor -> (ticks as a vassal, ticks in debt as a vassal)
    vassal: BTreeMap<String, (u32, u32)>,
    start_pop: f64,
}

fn src_of(w: &census::Write) -> String {
    w.source.clone().unwrap_or_else(|| format!("{}:{}", w.location.file().rsplit('/').next().unwrap_or(""), w.location.line()))
}

fn run(sc: &str, world: &str, cut: Option<f64>, seed: u64, ticks: u32) -> Run { run_upto(sc, world, cut, seed, ticks, 150) }

/// `upto`: the last tick whose treasury and army writes are summed.
fn run_upto(sc: &str, world: &str, cut: Option<f64>, seed: u64, ticks: u32, upto: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = true;
        assert!(s.economy_v2_debt_cut.is_none() && s.economy_v2_depopulation_ticks.is_none(), "debt and depopulation are off in the content");
        assert_eq!(s.economy_v2_debt_ticks, Some(4));
        s.economy_v2_debt_cut = cut;
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut r = Run { start_pop: st.world_state.as_ref().unwrap().actors.values().map(|a| a.get_metric("population")).sum(), ..Default::default() };
    let _ = census::take_writes();
    let _ = census::take_treasury_parts();
    for _ in 0..ticks {
        let pop_before: BTreeMap<String, f64> = st.world_state.as_ref().unwrap().actors.iter().map(|(k, a)| (k.clone(), a.get_metric("population"))).collect();
        let t = st.world_state.as_ref().unwrap().tick;
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let mut parts: BTreeMap<String, (f64, f64)> = BTreeMap::new();
        for (a, inc, upk) in census::take_treasury_parts() { let e = parts.entry(a).or_default(); e.0 += inc; e.1 += upk; }
        for w in census::take_writes() {
            let src = src_of(&w);
            match w.metric.as_str() {
                "treasury" if t <= upto => { *r.treasury_src.entry((w.actor.clone(), src)).or_default() += w.applied; }
                "military_size" if t <= upto && w.applied > 0.0 => { *r.army_src.entry((w.actor.clone(), src)).or_default() += w.applied; }
                "population" => {
                    let e = r.pop_src.entry(src.clone()).or_default();
                    if w.applied < 0.0 { e.0 += w.applied; } else { e.1 += w.applied; }
                    let p = pop_before.get(&w.actor).copied().unwrap_or(0.0);
                    if w.applied < 0.0 && p > 1.0 && !src.starts_with("event") {
                        r.rho.entry(src).or_default().push(-w.applied / p);
                    }
                }
                _ => {}
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        for d in &ws.dead_actor_ids { r.dead.entry(d.clone()).or_insert(t); }
        for (id, a) in &ws.actors {
            if ws.dead_actor_ids.contains(id) { continue; }
            r.living += 1;
            if ws.vassalages.iter().any(|v| &v.vassal_id == id) {
                let e = r.vassal.entry(id.clone()).or_default();
                e.0 += 1;
                if a.get_metric("treasury") < 0.0 { e.1 += 1; }
            }
            let (inc, upk) = parts.get(id).copied().unwrap_or((0.0, 0.0));
            r.ticks.entry(id.clone()).or_default().push((t, a.get_metric("treasury"), a.get_metric("population"), inc, upk));
        }
    }
    r
}

/// Per actor over a world's runs: (debt share of counted ticks, p90 spell, counted ticks, zombie ticks,
/// debt ticks with upkeep > income among counted)
fn debt_stats(runs: &[Run]) -> BTreeMap<String, (f64, f64, u64, u64, u64)> {
    let mut acc: BTreeMap<String, (u64, u64, u64, u64, Vec<f64>)> = BTreeMap::new();
    for r in runs {
        for (id, v) in &r.ticks {
            let e = acc.entry(id.clone()).or_default();
            let mut spell = 0u32;
            for &(_, tr, pop, inc, upk) in v {
                if pop <= 1.0 { e.3 += 1; continue; }
                e.0 += 1;
                if tr < 0.0 {
                    e.1 += 1;
                    if upk > inc { e.2 += 1; }
                    spell += 1;
                } else if spell > 0 {
                    e.4.push(spell as f64);
                    spell = 0;
                }
            }
            if spell > 0 { e.4.push(spell as f64); }
        }
    }
    acc.into_iter().map(|(k, (n, d, structural, z, spells))| (k, (100.0 * d as f64 / n.max(1) as f64, if spells.is_empty() { 0.0 } else { pct(&spells, 0.9) }, d, z, structural))).collect()
}

fn paired_t(a: &[f64], b: &[f64]) -> f64 {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| y - x).collect();
    let n = d.len() as f64;
    let mean = d.iter().sum::<f64>() / n;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    if sd > 0.0 { mean / (sd / n.sqrt()) } else if mean > 0.0 { f64::INFINITY } else { 0.0 }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let first: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    census::enable_writes();
    census::watch_all_metrics(true);
    census::enable_treasury_parts();
    if args.get(4).map(String::as_str) == Some("debtors") { debtors(first, seeds, ticks); return; }
    println!("# Ц2 and population — diagnosis, v2 as in the content, seeds {first}–{}, {ticks} ticks\n", first + seeds - 1);
    let mut verdict_rows = Vec::new();
    let mut debtor_rows = Vec::new();
    let mut frozen_rows = Vec::new();
    let mut mono_rows = Vec::new();
    let mut parts_rows = Vec::new();
    let mut pop_rows = Vec::new();
    let mut zombie_rows = Vec::new();
    let mut rome_rows = Vec::new();
    let mut rho_rows = Vec::new();
    for sc in SCENARIOS {
        let mut pop_src: BTreeMap<String, (f64, f64)> = BTreeMap::new();
        let mut rho: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut living_sc = 0u64;
        let mut start_pop = 0.0;
        for world in worlds(sc) {
            let played = *world != "none";
            let models: Vec<Vec<Run>> = CUTS.iter().map(|c| (first..first + seeds).map(|s| run(sc, world, *c, s, ticks)).collect()).collect();
            // ---- 1. debt
            for (ci, cut) in CUTS.iter().enumerate() {
                let ds = debt_stats(&models[ci]);
                let worst_share = ds.values().map(|x| x.0).fold(0.0, f64::max);
                let worst_p90 = ds.values().map(|x| x.1).fold(0.0, f64::max);
                let over: Vec<String> = ds.iter().filter(|(_, x)| x.0 > 20.0).map(|(k, x)| format!("{k} {:.0} %", x.0)).collect();
                let i1 = worst_p90 <= 12.0;
                let i2 = played || over.is_empty();
                let mut i4 = true;
                let mut keys = Vec::new();
                if ci > 0 {
                    for k in KEY {
                        if !models[0][0].ticks.contains_key(k) { continue; }
                        let a: Vec<f64> = models[0].iter().map(|r| r.dead.contains_key(k) as u8 as f64).collect();
                        let b: Vec<f64> = models[ci].iter().map(|r| r.dead.contains_key(k) as u8 as f64).collect();
                        let t = paired_t(&a, &b);
                        if t >= 3.0 { i4 = false; }
                        keys.push(format!("{k} {}→{}", a.iter().sum::<f64>(), b.iter().sum::<f64>()));
                    }
                }
                let label = cut.map_or("off".to_string(), |x| format!("{:.0} %", 100.0 * x));
                verdict_rows.push(format!("| {sc} | {world} | {label} | {worst_p90:.0} {} | {} | {} | {} |",
                    if i1 { "yes" } else { "**no**" },
                    if played { format!("(played) {}", if over.is_empty() { "—".into() } else { over.join(", ") }) } else if i2 { "yes".into() } else { format!("**no**: {}", over.join(", ")) },
                    if ci == 0 { "—".into() } else if i4 { format!("yes ({})", keys.join("; ")) } else { format!("**no** ({})", keys.join("; ")) },
                    if i1 && i2 && i4 && ci > 0 { "**passes**" } else { "" }));
                let _ = worst_share;
                if ci == 0 || ci == 3 {
                    for (k, x) in ds.iter().filter(|(_, x)| x.0 > 20.0) {
                        let class = if x.4 * 2 >= x.2 { "army dearer than income" } else { "other" };
                        debtor_rows.push(format!("| {sc} | {world} | {label} | {k} | {:.0} % | {:.0} | {} | {class}{} |", x.0, x.1, x.3, if played { ", played world" } else { "" }));
                    }
                }
                if ci == 3 {
                    for k in FROZEN {
                        if let Some(x) = ds.get(k) {
                            let end: Vec<f64> = models[ci].iter().filter_map(|r| r.ticks.get(k).and_then(|v| v.last()).map(|x| x.1)).collect();
                            let popend: Vec<f64> = models[ci].iter().filter_map(|r| r.ticks.get(k).and_then(|v| v.last()).map(|x| x.2)).collect();
                            let dead = models[ci].iter().filter(|r| r.dead.contains_key(k)).count();
                            frozen_rows.push(format!("| {sc} | {world} | {k} | {:.0} % | {:.0} | {} | {} | {} | {dead} |", x.0, x.1, x.3, q(&end), q(&popend)));
                        }
                    }
                }
            }
            // ---- 2. treasury accumulation (rule off)
            let base = &models[0];
            let mut mono: BTreeMap<String, (u32, u32, Vec<f64>)> = BTreeMap::new();
            for r in base {
                for (id, v) in &r.ticks {
                    let at = |t: u32| v.iter().find(|x| x.0 == t).map(|x| x.1);
                    let steps: Option<Vec<f64>> = (0..=15).map(|i| at(i * 10)).collect();
                    let Some(steps) = steps else { continue };
                    let e = mono.entry(id.clone()).or_default();
                    e.1 += 1;
                    if steps.windows(2).all(|w| w[1] >= w[0]) && steps[15] > steps[0] { e.0 += 1; }
                    e.2.push(steps[15]);
                }
            }
            let monos: Vec<String> = mono.iter().filter(|(_, x)| x.0 * 2 >= x.1 && x.1 > 0).map(|(k, x)| format!("{k} {}/{} (t150 {})", x.0, x.1, q(&x.2))).collect();
            mono_rows.push(format!("| {sc} | {world} | {} |", if monos.is_empty() { "—".into() } else { monos.join("; ") }));
            for (id, x) in mono.iter().filter(|(_, x)| x.0 * 2 >= x.1 && x.1 > 0) {
                let _ = x;
                let mut inc = 0.0;
                let mut upk = 0.0;
                for r in base { for t in r.ticks.get(id).into_iter().flatten().filter(|t| t.0 <= 150) { inc += t.3; upk += t.4; } }
                let mut src: BTreeMap<String, f64> = BTreeMap::new();
                for r in base { for ((a, s), v) in &r.treasury_src { if a == id && s != "treasury formula" { *src.entry(s.clone()).or_default() += v / seeds as f64; } } }
                let mut sv: Vec<(String, f64)> = src.into_iter().filter(|x| x.1.abs() >= 20.0).collect();
                sv.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                let mut arm: BTreeMap<String, f64> = BTreeMap::new();
                for r in base { for ((a, s), v) in &r.army_src { if a == id { *arm.entry(s.clone()).or_default() += v / seeds as f64; } } }
                let mut av: Vec<(String, f64)> = arm.into_iter().filter(|x| x.1 >= 5.0).collect();
                av.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                parts_rows.push(format!("| {sc} | {world} | {id} | {:+.0} | {:-.0} | {} | {} |", inc / seeds as f64, -upk / seeds as f64,
                    sv.iter().map(|(k, x)| format!("{k} {x:+.0}")).collect::<Vec<_>>().join("; "), av.iter().map(|(k, x)| format!("{k} +{x:.0}")).collect::<Vec<_>>().join("; ")));
            }
            // ---- 3. population (rule off)
            for r in base {
                for (k, (m, p)) in &r.pop_src { let e = pop_src.entry(k.clone()).or_default(); e.0 += m; e.1 += p; }
                for (k, v) in &r.rho { rho.entry(k.clone()).or_default().extend(v); }
                living_sc += r.living;
                start_pop += r.start_pop;
            }
            let mut z: BTreeMap<String, (u32, u64)> = BTreeMap::new();
            for r in base {
                for (id, v) in &r.ticks {
                    let n = v.iter().filter(|x| x.2 <= 1.0).count() as u64;
                    if n > 0 { let e = z.entry(id.clone()).or_default(); e.0 += 1; e.1 += n; }
                }
            }
            let zt: u64 = z.values().map(|x| x.1).sum();
            let lv: u64 = base.iter().map(|r| r.living).sum();
            zombie_rows.push(format!("| {sc} | {world} | {:.1} % | {} |", 100.0 * zt as f64 / lv.max(1) as f64,
                if z.is_empty() { "—".into() } else { z.iter().map(|(k, x)| format!("{k} {} ({:.0})", x.0, x.1 as f64 / x.0 as f64)).collect::<Vec<_>>().join(", ") }));
            if sc == "rome_375" {
                let at = |t: u32| q(&base.iter().filter_map(|r| r.ticks.get("rome").and_then(|v| v.iter().find(|x| x.0 == t)).map(|x| x.2)).collect::<Vec<f64>>());
                let zg = base.iter().filter(|r| r.ticks.get("rome").is_some_and(|v| v.iter().any(|x| x.2 <= 1.0))).count();
                let dead = base.iter().filter(|r| r.dead.contains_key("rome")).count();
                rome_rows.push(format!("| {world} | {} | {} | {} | {} | {zg} | {dead} |", at(0), at(50), at(150), at(299)));
            }
        }
        let n = (seeds as f64) * worlds(sc).len() as f64;
        let mut v: Vec<(String, (f64, f64))> = pop_src.into_iter().filter(|x| (x.1 .0 - x.1 .1).abs() / n >= 1.0 || x.1 .1 / n >= 1.0).collect();
        v.sort_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap());
        pop_rows.push(format!("| {sc} | {:.0} | {} |", start_pop / n, v.iter().map(|(k, (m, p))| format!("{k} {:.0}/+{:.0}", m / n, p / n)).collect::<Vec<_>>().join("; ")));
        for (k, x) in &rho {
            let mean = x.iter().sum::<f64>() / x.len().max(1) as f64;
            let p90 = pct(x, 0.9);
            rho_rows.push(format!("| {sc} | {k} | {:.1} % | {:.4} | {:.4} | {:.0} % | {:.0} % |", 100.0 * x.len() as f64 / living_sc.max(1) as f64, mean, p90, 100.0 * R / (R + mean), 100.0 * R / (R + p90)));
        }
    }
    println!("## 1. Debt — the corrected pre-commitment\n");
    println!("| scenario | world | x | (1) worst p90 spell ≤ 12 | (2) no player: nobody > 20 % / played: who | (4) key deaths (games, off → x), t < 3 | |");
    println!("|---|---|---|---|---|---|---|");
    for r in verdict_rows { println!("{r}"); }
    println!("\n### Debtors (> 20 % of counted ticks), rule off and x = 20 %\n");
    println!("| scenario | world | x | actor | share | p90 spell | zombie ticks (not counted) | class (upkeep > income on half the debt ticks or more) |");
    println!("|---|---|---|---|---|---|---|---|");
    for r in debtor_rows { println!("{r}"); }
    println!("\n### The frozen debt of states without people (x = 20 %)\n");
    println!("| scenario | world | actor | debt share (counted) | p90 spell | zombie ticks | treasury at its last tick p10/50/90 | population at its last tick | dies (games) |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in frozen_rows { println!("{r}"); }
    println!("\n## 2. Treasury rising monotonically to tick 150 (rule off): actors monotone in half the games or more\n");
    println!("| scenario | world | actor games monotone / alive at 150 (treasury on tick 150 p10/50/90) |");
    println!("|---|---|---|");
    for r in mono_rows { println!("{r}"); }
    println!("\n### Their treasury over ticks 0–150, mean per game: income, upkeep, other writes by source (|x| ≥ 20); army added by source\n");
    println!("| scenario | world | actor | income | upkeep | other treasury writes | army added (recruiting is free) |");
    println!("|---|---|---|---|---|---|---|");
    for r in parts_rows { println!("{r}"); }
    println!("\n## 3. Population\n");
    println!("### Writers by source, mean per game (all actors): taken / added\n");
    println!("| scenario | starting population (all actors) | by source |");
    println!("|---|---|---|");
    for r in pop_rows { println!("{r}"); }
    println!("\n### Zombies: living with population ≤ 1 — share of living actor-ticks; games (mean zombie ticks a game)\n");
    println!("| scenario | world | share | actors |");
    println!("|---|---|---|---|");
    for r in zombie_rows { println!("{r}"); }
    println!("\n### Rome's population p10/50/90 on ticks 0 / 50 / 150 / 299; games with Rome at ≤ 1; Rome dies\n");
    println!("| world | 0 | 50 | 150 | 299 | games at ≤ 1 | dies |");
    println!("|---|---|---|---|---|---|---|");
    for r in rome_rows { println!("{r}"); }
    println!("\n### b/r for «population pulled to its authored norm», r = {R}: rate writers (events excluded)\n");
    println!("| scenario | writer | acts on living actor-ticks | mean ρ | p90 ρ | P*/N at mean ρ | at p90 ρ |");
    println!("|---|---|---|---|---|---|---|");
    for r in rho_rows { println!("{r}"); }
}

/// The debtors of the worlds without a player (rule off, > 20 % of counted ticks): over the whole
/// game, mean per game — income, upkeep, tribute paid, other treasury writes; how much of their
/// debt is spent as a vassal. Tribute is paid in the interaction phase, before the treasury formula
/// takes the upkeep, so a vassal pays out of the money that would have paid its army.
fn debtors(first: u64, seeds: u64, ticks: u32) {
    println!("# Debtors without a player (rule off), seeds {first}–{}: the whole game, mean per game\n", first + seeds - 1);
    println!("| scenario | actor | debt share | vassal: share of its living ticks / of its debt ticks | income | upkeep | tribute paid | other treasury writes (|x| ≥ 10) |");
    println!("|---|---|---|---|---|---|---|---|");
    for sc in SCENARIOS {
        let runs: Vec<Run> = (first..first + seeds).map(|s| run_upto(sc, "none", None, s, ticks, u32::MAX)).collect();
        let ds = debt_stats(&runs);
        for (id, x) in ds.iter().filter(|(_, x)| x.0 > 20.0) {
            let (mut inc, mut upk) = (0.0, 0.0);
            let (mut lt, mut dt) = (0u64, 0u64);
            for r in &runs {
                for t in r.ticks.get(id).into_iter().flatten() { inc += t.3; upk += t.4; lt += 1; if t.1 < 0.0 { dt += 1; } }
            }
            let (vt, vd): (u64, u64) = runs.iter().filter_map(|r| r.vassal.get(id)).fold((0, 0), |a, v| (a.0 + v.0 as u64, a.1 + v.1 as u64));
            let mut src: BTreeMap<String, f64> = BTreeMap::new();
            for r in &runs { for ((a, k), v) in &r.treasury_src { if a == id && k != "treasury formula" { *src.entry(k.clone()).or_default() += v / seeds as f64; } } }
            let tribute = src.remove("vassal tribute").unwrap_or(0.0);
            let mut sv: Vec<(String, f64)> = src.into_iter().filter(|x| x.1.abs() >= 10.0).collect();
            sv.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            println!("| {sc} | {id} | {:.0} % | {:.0} % / {:.0} % | {:+.0} | {:.0} | {:.0} | {} |", x.0, 100.0 * vt as f64 / lt.max(1) as f64, 100.0 * vd as f64 / dt.max(1) as f64,
                inc / seeds as f64, -upk / seeds as f64, tribute, sv.iter().map(|(k, v)| format!("{k} {v:+.0}")).collect::<Vec<_>>().join("; "));
        }
    }
}
