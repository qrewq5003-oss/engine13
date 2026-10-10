//! Economy project, the switch to v2 — stage F0, measurement only (docs/economy_project_brief.md §9.2,
//! §9.8; owner's decision after Ц2). Built with `--features census`. Every world, seeds 0–29 and
//! 200–229 × 300 ticks; v1 (the switch off) against v2 as the content stands (nothing overridden).
//!
//! 1. The §9.2 criteria, each on its own, v1 and v2: A10 (spread of the constantinople wins, the share
//!    of the most frequent tick, wins on 40–43, games without a win, wins with no player while the
//!    Ottomans live); A35 (Rome's family wins, the share of the most frequent tick, strategies side by
//!    side); the split on tick 40; B46 (outcomes a game, «held, then fell»); the regency fork with no
//!    player and in aggressive; the death profile (Rome or `rome_west`, Byzantium with no player and
//!    its median tick, Milan with a player).
//! 2. One number for every §9.8 item, and two re-measured: Ц6's decline on the pooled seeds, and the
//!    three submissions Ц2 changed (Ottomans → Byzantium, Florence → Siena, Huns → Ostrogoths) — per
//!    pair, why the vassalage does or does not form (battles, losses, losses at 3× strength, the
//!    longest streak, the army ratio).
//! 3. A summary of the v2 world per scenario: deaths, vassals, wins, endings.
//!
//! Usage: cargo run --release --features census --bin f0_probe -- [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::census;
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

const SCENARIOS: [&str; 3] = ["rome_375", "constantinople_1430", "milan_1477"];
const SEEDS: [std::ops::Range<u64>; 2] = [0..30, 200..230];
const PEOPLES: [&str; 5] = ["alamanni", "vandals", "visigoths", "burgundians", "franks"];
/// (overlord, vassal) pairs Ц2 changed (brief §9.8)
const PAIRS: [(&str, &str, &str); 3] = [("constantinople_1430", "ottomans", "byzantium"), ("milan_1477", "florence", "siena"), ("rome_375", "huns", "ostrogoths")];

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

fn q(v: &[u32]) -> String {
    if v.is_empty() { return "—".into(); }
    let f: Vec<f64> = v.iter().map(|x| *x as f64).collect();
    format!("{:.0}/{:.0}/{:.0}", pct(&f, 0.1), pct(&f, 0.5), pct(&f, 0.9))
}

fn share(x: u64, n: u64) -> f64 { 100.0 * x as f64 / n.max(1) as f64 }

/// The most frequent tick and its share of the list.
fn top(v: &[u32]) -> (u32, f64) {
    let mut c: BTreeMap<u32, u32> = BTreeMap::new();
    for t in v { *c.entry(*t).or_default() += 1; }
    c.into_iter().max_by_key(|(t, n)| (*n, std::cmp::Reverse(*t))).map(|(t, n)| (t, 100.0 * n as f64 / v.len() as f64)).unwrap_or((0, 0.0))
}

/// Per actor, counts over its living ticks.
#[derive(Default, Clone, Copy)]
struct Act {
    living: u64,
    /// pressure ≥ 99 while `T_p` < 90 (Ц6)
    ceil_no_threat: u64,
    /// eo < T / 4, eo < T / 2
    eo_q: u64,
    eo_h: u64,
    /// population below a quarter of its base P₀ (Ц10)
    pop_q: u64,
    /// legitimacy < 40; cohesion < 30 and legitimacy < 40 (`popular_uprising`); legitimacy > 60
    l40: u64,
    uprising: u64,
    l60: u64,
}

#[derive(Default)]
struct Run {
    win: Option<u32>,
    /// at the win: Ottomans alive, Byzantium alive
    win_state: (bool, bool),
    /// milestone -> tick it fired (`world.tick − 1`)
    ms: BTreeMap<String, u32>,
    /// random event -> first tick
    ev_first: BTreeMap<String, u32>,
    dead: BTreeMap<String, u32>,
    conquered_by: BTreeMap<String, String>,
    /// (vassal, overlord) -> (first tick, last tick, ticks)
    vassal: BTreeMap<(String, String), (u32, u32, u32)>,
    act: BTreeMap<String, Act>,
    /// Ц6: clean declines (reached the goal in 10 ticks)
    declines: Vec<bool>,
    /// battles involving a pair's vassal
    battles: Vec<census::Battle>,
    /// pair -> army ratio overlord / vassal on ticks both live
    ratio: BTreeMap<usize, Vec<f64>>,
    /// treasury / income on ticks 250–299
    tr_inc: BTreeMap<String, Vec<f64>>,
    /// alliances (sorted members) on tick 34 and at the end
    alliances_34: Vec<Vec<String>>,
    alliances_end: Vec<Vec<String>>,
    /// Milan–Savoy alliance first tick
    savoy: Option<u32>,
    /// `popular_uprising` firings on Rome
    uprisings_rome: u32,
    /// pair -> the engine's streak of the vassal to the overlord (allies counted): max before the
    /// pair's war of conquest is declared, max under it; the tick it was declared
    streak: BTreeMap<usize, (u32, u32, Option<u32>)>,
}

fn run(sc: &str, world: &str, v2: bool, seed: u64, ticks: u32) -> Run {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    st.current_scenario.as_mut().unwrap().features.economy_v2 = v2;
    census::enable_battles();
    let _ = census::take_battles();
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let targets: BTreeSet<&str> = PAIRS.iter().filter(|p| p.0 == sc).map(|p| p.2).collect();
    let mut r = Run::default();
    let mut prev_tp: BTreeMap<String, f64> = BTreeMap::new();
    let mut prev_ep: BTreeMap<String, f64> = BTreeMap::new();
    // open declines: (actor, deadline, goal, clean, reached) — the Ц6 rule of c6s3/c9
    type Open = (String, u32, f64, f64, f64, bool, f64, bool);
    let mut open: Vec<Open> = Vec::new();
    for _ in 0..ticks {
        let before: BTreeSet<String> = st.world_state.as_ref().unwrap().dead_actor_ids.iter().cloned().collect();
        let fired_before: BTreeSet<String> = st.world_state.as_ref().unwrap().fired_events.iter().cloned().collect();
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        r.battles.extend(census::take_battles().into_iter().filter(|b| targets.contains(b.attacker.as_str()) || targets.contains(b.defender.as_str())));
        let ws = st.world_state.as_ref().unwrap();
        let scn = st.current_scenario.as_ref().unwrap();
        let t = ws.tick - 1;
        for d in ws.dead_actor_ids.iter().filter(|d| !before.contains(*d)) { r.dead.insert(d.clone(), t); }
        for m in &ws.milestone_events_fired { r.ms.entry(m.clone()).or_insert(t); }
        for e in ws.fired_events.iter().filter(|e| !fired_before.contains(*e)) { r.ev_first.entry(e.clone()).or_insert(t); }
        for v in &ws.vassalages {
            let e = r.vassal.entry((v.vassal_id.clone(), v.overlord_id.clone())).or_insert((t, t, 0));
            e.1 = t;
            e.2 += 1;
        }
        if r.savoy.is_none() && engine13::engine::interactions::allied(ws, "milan", "savoy") { r.savoy = Some(t); }
        let al = |ws: &engine13::core::WorldState| ws.alliances.iter().map(|a| { let mut m = a.actor_ids.clone(); m.sort(); m }).collect::<Vec<_>>();
        if t == 34 { r.alliances_34 = al(ws); }
        r.alliances_end = al(ws);
        let alive = |id: &str| ws.actors.contains_key(id) && !ws.dead_actor_ids.contains(id);
        for (i, (psc, lord, vas)) in PAIRS.iter().enumerate() {
            if *psc == sc && alive(lord) && alive(vas) {
                let mv = ws.actors[*vas].get_metric("military_size");
                if mv > 0.0 { r.ratio.entry(i).or_default().push(ws.actors[*lord].get_metric("military_size") / mv); }
            }
        }
        for (i, (psc, lord, vas)) in PAIRS.iter().enumerate() {
            if *psc != sc { continue; }
            let e = r.streak.entry(i).or_default();
            let war = ws.conquests.contains(&(lord.to_string(), vas.to_string()));
            if war && e.2.is_none() { e.2 = Some(t); }
            if let Some((w, c)) = ws.war_streaks.get(*vas) {
                if w == lord { if war { e.1 = e.1.max(*c); } else { e.0 = e.0.max(*c); } }
            }
        }
        open.retain_mut(|(id, deadline, goal, drop, tp_new, below, tp_max, reached)| {
            let Some(a) = ws.actors.get(id).filter(|_| !ws.dead_actor_ids.contains(id)) else { return false };
            if let Some(tp) = engine13::engine::pressure_threat(ws, id) { *tp_max = tp_max.max(tp); }
            if a.get_metric("external_pressure") <= *goal { *reached = true; }
            if t < *deadline { return true; }
            if !*below && *tp_max < *tp_new + *drop / 2.0 { r.declines.push(*reached); }
            false
        });
        let mut ids: Vec<&String> = ws.actors.keys().filter(|id| !ws.dead_actor_ids.contains(*id)).collect();
        ids.sort();
        for id in ids {
            let a = &ws.actors[id];
            let c = r.act.entry(id.clone()).or_default();
            c.living += 1;
            let ep = a.get_metric("external_pressure");
            let l = a.get_metric("legitimacy");
            let tp = engine13::engine::pressure_threat(ws, id);
            if ep >= 99.0 && tp.is_some_and(|x| x < 90.0) { c.ceil_no_threat += 1; }
            let eo = a.get_metric("economic_output");
            if let Some(tt) = engine13::engine::eo_target(ws, scn, id).filter(|x| *x > 0.0) {
                if eo < tt / 4.0 { c.eo_q += 1; }
                if eo < tt / 2.0 { c.eo_h += 1; }
            }
            if engine13::engine::population_base(ws, scn, id).is_some_and(|b| a.get_metric("population") < 0.25 * b) { c.pop_q += 1; }
            if l < 40.0 { c.l40 += 1; if a.get_metric("cohesion") < 30.0 { c.uprising += 1; } }
            if l > 60.0 { c.l60 += 1; }
            if t >= 250 {
                let inc = engine13::engine::interactions::tick_income(a, scn);
                if inc > 0.0 { r.tr_inc.entry(id.clone()).or_default().push(a.get_metric("treasury") / inc); }
            }
            if let Some(tp) = tp {
                if let (Some(p), Some(e0)) = (prev_tp.get(id), prev_ep.get(id)) {
                    let drop = p - tp;
                    if drop >= 20.0 { open.push((id.clone(), t + 10, e0 - drop / 2.0, drop, tp, e0 - drop / 2.0 < tp, f64::MIN, false)); }
                }
                prev_tp.insert(id.clone(), tp);
            }
            prev_ep.insert(id.clone(), ep);
        }
        if r.win.is_none() && ws.victory_achieved {
            r.win = Some(t);
            r.win_state = (alive("ottomans"), alive("byzantium"));
        }
    }
    let ws = st.world_state.as_ref().unwrap();
    r.conquered_by = ws.conquered_by.clone();
    r.uprisings_rome = st.event_log.events.iter().filter(|e| e.id == "popular_uprising" && e.actor_id == "rome").count() as u32;
    r
}

/// Why a pair's vassalage forms or not: per game, battles between the two, the vassal's losses to
/// the overlord, of them at 3× strength, the vassal's longest streak of such losses to the overlord,
/// the vassal's other battles, and the mean army ratio.
fn pair_row(i: usize, runs: &[Run]) -> String {
    let (_, lord, vas) = PAIRS[i];
    let (mut games, mut fights, mut lost, mut q3, mut other, mut streak3) = (0, 0u64, 0u64, 0u64, 0u64, 0);
    let mut max_streak: Vec<u32> = Vec::new();
    let mut ratio: Vec<f64> = Vec::new();
    let mut ratio_q: Vec<f64> = Vec::new();
    let mut blocked = 0;
    let (mut eng_pre, mut eng_war, mut ticks_b): (Vec<u32>, Vec<u32>, Vec<u32>) = (Vec::new(), Vec::new(), Vec::new());
    for rr in runs {
        // the engine drops the streak when the vassalage forms, so a formed pair counts as 3
        let formed = rr.vassal.keys().any(|(v, o)| v == vas && o == lord);
        if let Some((a, b, _)) = rr.streak.get(&i) { eng_pre.push(if formed { (*a).max(3) } else { *a }); eng_war.push(*b); }
        for b in rr.battles.iter().filter(|b| (b.attacker == lord && b.defender == vas) || (b.attacker == vas && b.defender == lord)) { ticks_b.push(b.tick); }
        if rr.vassal.keys().any(|(v, o)| v == vas && o == lord) { games += 1; }
        let mut streak = 0u32;
        let mut best = 0u32;
        for b in &rr.battles {
            let involves = |x: &str| b.attacker == x || b.defender == x;
            if !involves(vas) { continue; }
            let vs_lord = involves(lord);
            let vas_att = b.attacker == vas;
            let vas_lost = vas_att != b.attacker_won;
            let (s_v, s_o) = if vas_att { (b.strength_attacker, b.strength_defender) } else { (b.strength_defender, b.strength_attacker) };
            if vs_lord {
                fights += 1;
                if s_v > 0.0 { ratio_q.push(s_o / s_v); }
                if vas_lost {
                    lost += 1;
                    if s_o >= 3.0 * s_v { q3 += 1; streak += 1; } else { streak = 0; }
                } else { streak = 0; }
            } else {
                other += 1;
                // a loss to someone else at 3× starts that winner's streak; any other battle resets
                streak = 0;
            }
            best = best.max(streak);
        }
        if best >= 3 { streak3 += 1; if !rr.vassal.keys().any(|(v, o)| v == vas && o == lord) { blocked += 1; } }
        max_streak.push(best);
        ratio.extend(rr.ratio.get(&i).into_iter().flatten());
    }
    let n = runs.len() as f64;
    format!("{lord} → {vas}: vassal in {games}; a game: battles between them {:.1}, {vas} lost {:.1}, of them at ≥ 3× strength {:.1}; {vas}'s other battles {:.1}; longest streak p10/50/90 {}; streak ≥ 3 in {streak3} (no vassalage after it in {blocked}); army ratio {lord}/{vas} p10/50/90 {:.1}/{:.1}/{:.1}; strength ratio in their battles p50 {:.1}; the engine's streak (allies counted) max before the war of conquest p10/50/90 {}, ≥ 3 in {}, under it p10/50/90 {}; battle ticks p10/50/90 {}",
        fights as f64 / n, lost as f64 / n, q3 as f64 / n, other as f64 / n, q(&max_streak), pct(&ratio, 0.1), pct(&ratio, 0.5), pct(&ratio, 0.9), pct(&ratio_q, 0.5), q(&eng_pre), eng_pre.iter().filter(|x| **x >= 3).count(), q(&eng_war), q(&ticks_b))
}

/// one world's games per seed set, keyed by (scenario, world, v2)
type Results<'a> = Vec<((&'a str, &'a str, bool), Vec<Vec<Run>>)>;

fn main() {
    let ticks: u32 = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(300);
    let jobs: Vec<(&str, &str, bool)> = SCENARIOS.iter().flat_map(|sc| worlds(sc).iter().flat_map(move |w| [false, true].map(|v| (*sc, *w, v)))).collect();
    let results: Results = std::thread::scope(|s| {
        let hs: Vec<_> = jobs.iter().map(|&(sc, w, v)| s.spawn(move || ((sc, w, v), SEEDS.iter().map(|rg| rg.clone().map(|seed| run(sc, w, v, seed, ticks)).collect()).collect()))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let get = |sc: &str, w: &str, v: bool| -> &Vec<Vec<Run>> { &results.iter().find(|((a, b, c), _)| *a == sc && *b == w && *c == v).unwrap().1 };
    let lab = |v: bool| if v { "v2" } else { "v1" };
    let sets = ["0–29", "200–229"];

    println!("# F0 — v1 against v2 as the content stands, {ticks} ticks\n");
    println!("## 1. §9.2\n");
    println!("### A10 — constantinople wins\n");
    println!("| world | seeds | model | wins | tick p10/50/90 | most frequent tick (share) | on 40–43 | games without a win | wins with no player while the Ottomans live |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for w in worlds("constantinople_1430") {
        for (si, set) in sets.iter().enumerate() {
            for v in [false, true] {
                let runs = &get("constantinople_1430", w, v)[si];
                let wins: Vec<u32> = runs.iter().filter_map(|r| r.win).collect();
                let (tt, sh) = top(&wins);
                let on = wins.iter().filter(|t| (40..=43).contains(*t)).count();
                let ott = runs.iter().filter(|r| r.win.is_some() && r.win_state.0).count();
                println!("| {w} | {set} | {} | {} | {} | {tt} ({sh:.0} %) | {on} ({:.0} %) | {} | {} |", lab(v), wins.len(), q(&wins), share(on as u64, wins.len() as u64), runs.len() - wins.len(), if *w == "none" { ott.to_string() } else { "—".into() });
            }
        }
    }
    println!("\n### A35 — Rome's family wins; the split on tick 40\n");
    println!("| world | seeds | model | wins | tick p10/50/90 | most frequent tick (share) | games without a win | split on tick 40 | split tick (other) |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for w in worlds("rome_375") {
        for (si, set) in sets.iter().enumerate() {
            for v in [false, true] {
                let runs = &get("rome_375", w, v)[si];
                let wins: Vec<u32> = runs.iter().filter_map(|r| r.win).collect();
                let (tt, sh) = top(&wins);
                let splits: Vec<u32> = runs.iter().filter_map(|r| r.ms.get("rome_splits").copied()).collect();
                let s40 = splits.iter().filter(|t| **t == 40).count();
                let other: Vec<u32> = splits.iter().copied().filter(|t| *t != 40).collect();
                println!("| {w} | {set} | {} | {} | {} | {tt} ({sh:.0} %) | {} | {s40} / {} | {:?} |", lab(v), wins.len(), q(&wins), runs.len() - wins.len(), runs.len(), other);
            }
        }
    }
    println!("\n### B46 — outcomes a game; «held, then fell»\n");
    println!("| world | seeds | model | games with 0 / 1 / ≥ 2 outcomes | outcomes fired | «survived_alone», then Byzantium fell |");
    println!("|---|---|---|---|---|---|");
    for w in worlds("constantinople_1430") {
        for (si, set) in sets.iter().enumerate() {
            for v in [false, true] {
                let runs = &get("constantinople_1430", w, v)[si];
                let mut n = [0; 3];
                let mut kinds: BTreeMap<String, u32> = BTreeMap::new();
                let mut htf = 0;
                for r in runs {
                    let o: Vec<&String> = r.ms.keys().filter(|m| m.starts_with("outcome_")).collect();
                    n[o.len().min(2)] += 1;
                    for k in &o { *kinds.entry(k.trim_start_matches("outcome_").to_string()).or_default() += 1; }
                    if let (Some(a), Some(d)) = (r.ms.get("outcome_survived_alone"), r.dead.get("byzantium")) { if d > a { htf += 1; } }
                }
                println!("| {w} | {set} | {} | {} / {} / {} | {:?} | {htf} |", lab(v), n[0], n[1], n[2], kinds);
            }
        }
    }
    println!("\n### Regency fork (milan)\n");
    println!("| world | seeds | model | stabilizes (tick p10/50/90) | deepens (tick p10/50/90) | both |");
    println!("|---|---|---|---|---|---|");
    for w in worlds("milan_1477") {
        for (si, set) in sets.iter().enumerate() {
            for v in [false, true] {
                let runs = &get("milan_1477", w, v)[si];
                let st: Vec<u32> = runs.iter().filter_map(|r| r.ms.get("milan_regency_stabilizes").copied()).collect();
                let dp: Vec<u32> = runs.iter().filter_map(|r| r.ms.get("milan_regency_crisis_deepens").copied()).collect();
                let both = runs.iter().filter(|r| r.ms.contains_key("milan_regency_stabilizes") && r.ms.contains_key("milan_regency_crisis_deepens")).count();
                println!("| {w} | {set} | {} | {} ({}) | {} ({}) | {both} |", lab(v), st.len(), q(&st), dp.len(), q(&dp));
            }
        }
    }
    println!("\n### Death profile\n");
    println!("| scenario | world | seeds | model | key deaths |");
    println!("|---|---|---|---|---|");
    for sc in SCENARIOS {
        for w in worlds(sc) {
            for (si, set) in sets.iter().enumerate() {
                for v in [false, true] {
                    let runs = &get(sc, w, v)[si];
                    let cell = match sc {
                        "rome_375" => {
                            let rome = runs.iter().filter(|r| r.dead.contains_key("rome")).count();
                            let west = runs.iter().filter(|r| r.dead.contains_key("rome_west")).count();
                            let either: Vec<u32> = runs.iter().filter_map(|r| r.dead.get("rome").or(r.dead.get("rome_west")).copied()).collect();
                            let peoples = runs.iter().map(|r| PEOPLES.iter().filter(|p| r.dead.contains_key(**p)).count()).sum::<usize>() as f64 / runs.len() as f64;
                            format!("Rome {rome}, rome_west {west}; the West falls (either) in {} @ {}; of the five peoples die {peoples:.1} a game", either.len(), q(&either))
                        }
                        "constantinople_1430" => {
                            let b: Vec<u32> = runs.iter().filter_map(|r| r.dead.get("byzantium").copied()).collect();
                            let o = runs.iter().filter(|r| r.dead.contains_key("ottomans")).count();
                            let by_war = runs.iter().filter(|r| r.conquered_by.get("byzantium").is_some_and(|x| x == "ottomans")).count();
                            format!("Byzantium {} @ {} (by Ottoman conquest {by_war}); Ottomans {o}", b.len(), q(&b))
                        }
                        _ => {
                            let m: Vec<u32> = runs.iter().filter_map(|r| r.dead.get("milan").copied()).collect();
                            let mv = runs.iter().filter(|r| r.vassal.keys().any(|(v, _)| v == "milan")).count();
                            let pap = runs.iter().filter(|r| r.dead.contains_key("papacy") || r.vassal.keys().any(|(v, _)| v == "papacy")).count();
                            let ven = runs.iter().filter(|r| r.dead.contains_key("venice") || r.vassal.keys().any(|(v, _)| v == "venice")).count();
                            format!("Milan dies {} @ {}, submits {mv}; papacy dies or submits {pap}, Venice {ven}", m.len(), q(&m))
                        }
                    };
                    println!("| {sc} | {w} | {set} | {} | {cell} |", lab(v));
                }
            }
        }
    }

    println!("\n## 2. §9.8 numbers\n");
    // Ц6 decline, pooled
    for v in [false, true] {
        let d: Vec<bool> = results.iter().filter(|((_, _, vv), _)| *vv == v).flat_map(|(_, rs)| rs.iter().flatten().flat_map(|r| r.declines.iter().copied())).collect();
        let n = d.len() as f64;
        let p = d.iter().filter(|x| **x).count() as f64 / n.max(1.0);
        let se = (p * (1.0 - p) / n.max(1.0)).sqrt();
        println!("- Ц6 decline {} (all worlds, both seed sets): {:.1} % of {} clean declines, SE {:.1} — 80 % minus the share = {:.1} SE", lab(v), 100.0 * p, d.len(), 100.0 * se, (0.8 - p) / se.max(1e-9));
        for (si, set) in sets.iter().enumerate() {
            let d: Vec<bool> = results.iter().filter(|((_, _, vv), _)| *vv == v).flat_map(|(_, rs)| rs[si].iter().flat_map(|r| r.declines.iter().copied())).collect();
            let p = d.iter().filter(|x| **x).count() as f64 / d.len().max(1) as f64;
            println!("  - seeds {set}: {:.1} % of {}", 100.0 * p, d.len());
        }
    }
    println!("\n### Submissions Ц2 changed — why\n");
    for (i, (sc, _, _)) in PAIRS.iter().enumerate() {
        for w in worlds(sc) {
            for v in [false, true] {
                let runs: Vec<&Run> = get(sc, w, v).iter().flatten().collect();
                let row = pair_row_ref(i, &runs);
                println!("- {sc} {w} {}: {row}", lab(v));
            }
        }
    }
    println!("\n### Per-item numbers (60 games a world)\n");
    for sc in SCENARIOS {
        for w in worlds(sc) {
            for v in [false, true] {
                let runs: Vec<&Run> = get(sc, w, v).iter().flatten().collect();
                let n = runs.len();
                let mut act: BTreeMap<String, Act> = BTreeMap::new();
                for r in &runs { for (k, a) in &r.act { let e = act.entry(k.clone()).or_default(); e.living += a.living; e.ceil_no_threat += a.ceil_no_threat; e.eo_q += a.eo_q; e.eo_h += a.eo_h; e.pop_q += a.pop_q; e.l40 += a.l40; e.uprising += a.uprising; e.l60 += a.l60; } }
                let tot = act.values().fold(Act::default(), |mut s, a| { s.living += a.living; s.ceil_no_threat += a.ceil_no_threat; s.eo_q += a.eo_q; s.eo_h += a.eo_h; s.pop_q += a.pop_q; s });
                let vt: u32 = runs.iter().map(|r| r.vassal.values().map(|x| x.2).sum::<u32>()).sum();
                let mut pairs: BTreeMap<String, u32> = BTreeMap::new();
                for r in &runs { for (vv, o) in r.vassal.keys() { *pairs.entry(format!("{o}→{vv}")).or_default() += 1; } }
                let mut pv: Vec<(String, u32)> = pairs.into_iter().collect();
                pv.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let deaths: usize = runs.iter().map(|r| r.dead.len()).sum();
                let mut dead_by: BTreeMap<String, u32> = BTreeMap::new();
                for r in &runs { for d in r.dead.keys() { *dead_by.entry(d.clone()).or_default() += 1; } }
                let mut dv: Vec<(String, u32)> = dead_by.into_iter().collect();
                dv.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let war: usize = runs.iter().map(|r| r.conquered_by.len()).sum();
                let wins = runs.iter().filter(|r| r.win.is_some()).count();
                let mut outs: BTreeMap<String, u32> = BTreeMap::new();
                for r in &runs { for m in r.ms.keys().filter(|m| m.starts_with("outcome_")) { *outs.entry(m.clone()).or_default() += 1; } }
                let a_ = |id: &str| act.get(id).copied().unwrap_or_default();
                let tri = |id: &str| { let x: Vec<f64> = runs.iter().flat_map(|r| r.tr_inc.get(id).into_iter().flatten().copied()).collect(); if x.is_empty() { "—".to_string() } else { format!("{:.0}", pct(&x, 0.5)) } };
                // freed vs ended by the vassal's death, pairs ending before the last tick
                let (mut ended_dead, mut ended_alive, mut ann): (u32, u32, Vec<u32>) = (0, 0, Vec::new());
                for r in &runs { for ((vv, _), (f, l, _)) in &r.vassal { if *l + 1 < ticks { if r.dead.contains_key(vv) { ended_dead += 1; ann.push(l - f); } else { ended_alive += 1; } } } }
                println!("#### {sc} {w} {}\n", lab(v));
                println!("- deaths {deaths} ({:.1} a game; by war {war}); by actor {:?}", deaths as f64 / n as f64, dv.iter().take(12).collect::<Vec<_>>());
                println!("- vassal-ticks {vt}; pairs (games) {:?}; pairs ended by the vassal's death {ended_dead} (lasted p10/50/90 {}), freed {ended_alive}", pv.iter().take(12).collect::<Vec<_>>(), q(&ann));
                println!("- wins {wins}; outcomes {:?}", outs);
                println!("- pressure ceiling without threat {:.1} % of living actor-ticks; eo < T/4 {:.1} %, eo < T/2 {:.1} %; population < P₀/4 {:.1} %", share(tot.ceil_no_threat, tot.living), share(tot.eo_q, tot.living), share(tot.eo_h, tot.living), share(tot.pop_q, tot.living));
                match sc {
                    "rome_375" => {
                        let r = a_("rome");
                        let up = runs.iter().filter(|x| x.uprisings_rome > 0).count();
                        let hv = runs.iter().filter(|x| x.vassal.keys().any(|(v, o)| v == "ostrogoths" && o == "huns")).count();
                        println!("- Rome: legitimacy < 40 {:.1} % of its living ticks, cohesion < 30 ∧ legitimacy < 40 {:.1} %; `popular_uprising` on Rome in {up} games; saxons eo < T/4 {:.1} %; treasury / income ticks 250–299 p50: rome {}, huns {}, sassanids {}, guptas {}; Huns → Ostrogoths {hv}", share(r.l40, r.living), share(r.uprising, r.living), share(a_("saxons").eo_q, a_("saxons").living), tri("rome"), tri("huns"), tri("sassanids"), tri("guptas"));
                    }
                    "constantinople_1430" => {
                        let wal = runs.iter().filter(|x| x.vassal.keys().any(|(v, o)| v == "wallachia" && o == "ottomans")).count();
                        let ob = runs.iter().filter(|x| x.vassal.keys().any(|(v, o)| v == "byzantium" && o == "ottomans")).count();
                        let ser = runs.iter().filter(|x| x.vassal.keys().any(|(v, o)| v == "serbia" && o == "ottomans")).count();
                        let mehmed: Vec<u32> = runs.iter().filter_map(|x| x.ms.get("mehmed_accelerates").copied()).collect();
                        println!("- Milan legitimacy > 60 {:.1} % of its living ticks; Wallachia eo < T/4 {:.1} %; Ottoman vassals: Wallachia {wal}, Serbia {ser}, Byzantium {ob}; treasury / income ticks 250–299 p50: ottomans {}, venice {}; `mehmed_accelerates` in {} @ {}", share(a_("milan").l60, a_("milan").living), share(a_("wallachia").eo_q, a_("wallachia").living), tri("ottomans"), tri("venice"), mehmed.len(), q(&mehmed));
                    }
                    _ => {
                        let lg: Vec<u32> = runs.iter().filter_map(|x| x.ev_first.get("italian_league_against_milan").copied()).collect();
                        let sv: Vec<u32> = runs.iter().filter_map(|x| x.savoy).collect();
                        let l34 = runs.iter().filter(|x| x.alliances_34.iter().any(|a| a.len() >= 3)).count();
                        let lend = runs.iter().filter(|x| x.alliances_end.iter().any(|a| a.len() >= 3)).count();
                        let fs = runs.iter().filter(|x| x.vassal.keys().any(|(v, o)| v == "siena" && o == "florence")).count();
                        println!("- league turns on Milan in {} @ {}; Milan–Savoy alliance in {} @ {}; an alliance of ≥ 3 on tick 34 in {l34}, at the end in {lend}; Milan pressure ceiling without threat {:.1} %; Florence → Siena {fs}", lg.len(), q(&lg), sv.len(), q(&sv), share(a_("milan").ceil_no_threat, a_("milan").living));
                    }
                }
                println!();
            }
        }
    }
}

fn pair_row_ref(i: usize, runs: &[&Run]) -> String {
    // the same as `pair_row` over references
    let owned: Vec<Run> = runs.iter().map(|r| Run { battles: r.battles.clone(), vassal: r.vassal.clone(), ratio: r.ratio.clone(), streak: r.streak.clone(), ..Default::default() }).collect();
    pair_row(i, &owned)
}
