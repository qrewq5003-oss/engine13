//! NOT FOR MERGE — measuring device for the investigation into the *form* of
//! dependency rules whose target does not live on one scale for every actor.
//!
//! It reads the engine's own trace (`engine::dep_trace_*`), so every number it
//! prints is a delta the engine actually applied, at the mid-tick moment it
//! applied it. Nothing here re-implements `apply_dependency_rule` — that
//! re-implementation already exists twice in `budget_probe` and is exactly the
//! defect class this probe is meant not to repeat.
//!
//! Counterfactual switches (scenario-side only; the probe owns the `Scenario`
//! object, so the engine is untouched and no RNG draw moves):
//!   E13_DEP_OFF=rule_id[,rule_id...]     set those rules' coefficient to 0
//!   E13_DEP_COEF=rule_id:val[,rule:val]  override those rules' coefficient
//! With an empty environment the run is byte-identical to `sim`'s world.
//!
//! Usage: dependency_probe <scenario> <ticks> <seed_from> <seed_count>

use engine13::{
    core::WorldState,
    engine::{dep_trace_enable, dep_trace_take, interactions, tick, DepTraceRow, EventLog},
    scenarios::registry,
};
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap};

/// Metrics the engine clamps to 0..100 — one shared scale for every actor, so an
/// absolute delta means the same thing to Rome and to Urbino.
const BOUNDED_0_100: [&str; 5] = [
    "legitimacy",
    "cohesion",
    "military_quality",
    "economic_output",
    "external_pressure",
];
/// Metrics with no upper bound: an absolute delta is a different-sized event for
/// every actor. `treasury` is clamped at neither end.
const UNBOUNDED: [&str; 3] = ["military_size", "population", "treasury"];

fn quantiles(v: &mut [f64]) -> (f64, f64, f64, f64, f64) {
    if v.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| {
        let i = ((v.len() - 1) as f64 * p).round() as usize;
        v[i]
    };
    (q(0.0), q(0.1), q(0.5), q(0.9), q(1.0))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("rome_375");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed_from: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let seed_count: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(30);

    let mut scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");

    // ---- counterfactual switches ------------------------------------------
    let mut overrides: BTreeMap<String, f64> = BTreeMap::new();
    if let Ok(v) = std::env::var("E13_DEP_OFF") {
        for id in v.split(',').filter(|s| !s.is_empty()) {
            overrides.insert(id.to_string(), 0.0);
        }
    }
    if let Ok(v) = std::env::var("E13_DEP_COEF") {
        for pair in v.split(',').filter(|s| !s.is_empty()) {
            let (id, val) = pair.split_once(':').expect("E13_DEP_COEF wants rule_id:value");
            overrides.insert(id.to_string(), val.parse().expect("bad coefficient"));
        }
    }
    let mut mode_overrides: BTreeMap<String, String> = BTreeMap::new();
    if let Ok(v) = std::env::var("E13_DEP_MODE") {
        for pair in v.split(',').filter(|s| !s.is_empty()) {
            let (id, m) = pair.split_once(':').expect("E13_DEP_MODE wants rule_id:mode");
            mode_overrides.insert(id.to_string(), m.to_string());
        }
    }
    for rule in scenario.dependencies.iter_mut() {
        if let Some(c) = overrides.get(&rule.id) {
            rule.coefficient = *c;
        }
        if let Some(m) = mode_overrides.get(&rule.id) {
            rule.mode = match m.as_str() {
                "excess" => engine13::core::DependencyMode::Excess,
                "excess_proportional" => engine13::core::DependencyMode::ExcessProportional,
                "deficit" => engine13::core::DependencyMode::Deficit,
                "deficit_proportional" => engine13::core::DependencyMode::DeficitProportional,
                other => panic!("unknown mode {other}"),
            };
        }
    }

    println!(
        "=== DEPENDENCY PROBE: {} ({} ticks, seeds {}..{}) ===",
        scenario_id,
        ticks,
        seed_from,
        seed_from + seed_count - 1
    );
    if overrides.is_empty() && mode_overrides.is_empty() {
        println!("counterfactual: none (engine content as authored)\n");
    } else {
        println!("counterfactual: coefficients {overrides:?}, modes {mode_overrides:?}\n");
    }

    // ---- Part 1: static census of the rules by scale class ------------------
    println!("--- Part 1: rules by target scale ---");
    println!(
        "{:46} {:18} -> {:17} {:20} {:>9} {:>7}  target scale",
        "id", "from", "to", "mode", "coef", "thr"
    );
    let mut class_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for r in &scenario.dependencies {
        let to = r.to.as_str();
        let proportional = format!("{:?}", r.mode).contains("Proportional");
        let scale = if BOUNDED_0_100.contains(&to) {
            "bounded 0..100"
        } else if UNBOUNDED.contains(&to) {
            if proportional {
                "UNBOUNDED, priced on target"
            } else {
                "UNBOUNDED, ABSOLUTE DELTA"
            }
        } else {
            "unknown"
        };
        *class_counts.entry(scale).or_default() += 1;
        println!(
            "{:46} {:18} -> {:17} {:20} {:>9} {:>7}  {}",
            r.id,
            r.from.as_str(),
            to,
            format!("{:?}", r.mode),
            r.coefficient,
            r.threshold.map(|t| t.to_string()).unwrap_or_else(|| "-".into()),
            scale
        );
    }
    println!("  by class: {class_counts:?}\n");

    // ---- run the seeds ------------------------------------------------------
    // Pooled per rule: absolute |delta|, and the relative price |delta| / target stock.
    let mut abs_by_rule: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut rel_by_rule: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut fired_by_rule: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    // Per actor, for the one rule under investigation.
    let mut rel_by_actor: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut drain_by_actor: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut mil_by_actor: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut pop_by_actor: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut cap_by_actor: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut zero_army: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut deaths: BTreeMap<String, usize> = BTreeMap::new();
    let mut alive_end: BTreeMap<String, usize> = BTreeMap::new();

    const SUBJECT: &str = "external_pressure_to_military_size";

    for seed in seed_from..seed_from + seed_count {
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for actor in &scenario.actors {
            if !actor.is_successor_template {
                world.actors.insert(actor.id.clone(), actor.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40) as u32;
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let start_ids: Vec<String> = world.actors.keys().cloned().collect();
        let mut event_log = EventLog::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);

        dep_trace_enable();
        for _ in 0..ticks {
            tick(&mut world, &scenario, &mut event_log, &mut rng);
            let rows: Vec<DepTraceRow> = dep_trace_take();
            for row in rows {
                let e = fired_by_rule.entry(row.rule.clone()).or_insert((0, 0));
                e.1 += 1;
                if row.delta != 0.0 {
                    e.0 += 1;
                }
                if row.delta == 0.0 {
                    continue;
                }
                abs_by_rule.entry(row.rule.clone()).or_default().push(row.delta.abs());
                if row.to_before.abs() > 1e-12 {
                    rel_by_rule
                        .entry(row.rule.clone())
                        .or_default()
                        .push(row.delta.abs() / row.to_before.abs());
                }
                if row.rule == SUBJECT {
                    drain_by_actor.entry(row.actor.clone()).or_default().push(row.delta.abs());
                    if row.to_before.abs() > 1e-12 {
                        rel_by_actor
                            .entry(row.actor.clone())
                            .or_default()
                            .push(row.delta.abs() / row.to_before.abs());
                    }
                }
            }
            // End-of-tick state per actor.
            for (id, actor) in world.actors.iter() {
                let mil = actor.get_metric("military_size");
                mil_by_actor.entry(id.clone()).or_default().push(mil);
                pop_by_actor.entry(id.clone()).or_default().push(actor.get_metric("population"));
                cap_by_actor
                    .entry(id.clone())
                    .or_default()
                    .push(interactions::military_capacity(actor));
                let z = zero_army.entry(id.clone()).or_insert((0, 0));
                z.1 += 1;
                if mil < interactions::MIN_DEFENSIBLE_MILITARY {
                    z.0 += 1;
                }
            }
        }
        for id in &start_ids {
            if world.actors.contains_key(id) {
                *alive_end.entry(id.clone()).or_default() += 1;
            } else {
                *deaths.entry(id.clone()).or_default() += 1;
            }
        }
    }

    // ---- Part 2: what each rule actually charged ----------------------------
    println!("--- Part 2: realized per-tick delta by rule (pooled over seeds x ticks x actors) ---");
    println!(
        "{:46} {:>7} {:>10} {:>10} {:>10} | relative price |delta|/target  {:>8} {:>8} {:>8}",
        "rule", "fired%", "|d| p10", "|d| med", "|d| p90", "p10", "med", "p90"
    );
    for r in &scenario.dependencies {
        let (f, n) = fired_by_rule.get(&r.id).copied().unwrap_or((0, 0));
        let mut abs = abs_by_rule.get(&r.id).cloned().unwrap_or_default();
        let mut rel = rel_by_rule.get(&r.id).cloned().unwrap_or_default();
        let (_, a10, a50, a90, _) = quantiles(&mut abs);
        let (_, r10, r50, r90, _) = quantiles(&mut rel);
        println!(
            "{:46} {:>6.1}% {:>10.3} {:>10.3} {:>10.3} | {:>39.4} {:>8.4} {:>8.4}",
            r.id,
            if n > 0 { 100.0 * f as f64 / n as f64 } else { 0.0 },
            a10,
            a50,
            a90,
            r10,
            r50,
            r90
        );
    }
    println!();

    // ---- Part 3: the subject rule, per actor --------------------------------
    println!("--- Part 3: {SUBJECT}, per actor ---");
    println!(
        "{:16} {:>9} {:>9} {:>9} {:>9} {:>10} {:>9} {:>8} {:>7}",
        "actor", "pop med", "cap med", "mil med", "drain med", "drain/mil", "m* pred", "army=0%", "deaths"
    );
    let mut names: Vec<String> = mil_by_actor.keys().cloned().collect();
    names.sort();
    let rate = interactions::MILITARY_RECOVERY_RATE;
    let mut rel_medians: Vec<(String, f64)> = Vec::new();
    for id in &names {
        let mut pop = pop_by_actor.get(id).cloned().unwrap_or_default();
        let mut cap = cap_by_actor.get(id).cloned().unwrap_or_default();
        let mut mil = mil_by_actor.get(id).cloned().unwrap_or_default();
        let mut dr = drain_by_actor.get(id).cloned().unwrap_or_default();
        let mut rel = rel_by_actor.get(id).cloned().unwrap_or_default();
        let (_, _, pop50, _, _) = quantiles(&mut pop);
        let (_, _, cap50, _, _) = quantiles(&mut cap);
        let (_, _, mil50, _, _) = quantiles(&mut mil);
        let (_, _, dr50, _, _) = quantiles(&mut dr);
        let (_, _, rel50, _, _) = quantiles(&mut rel);
        let (z, zt) = zero_army.get(id).copied().unwrap_or((0, 0));
        // Equilibrium of "recover 5% of the shortfall, then pay the drain":
        // (C - m) * rate = drain  =>  m* = C - drain/rate. Negative means the drain
        // alone is bigger than the largest inflow the actor's population can fund,
        // i.e. the army is pinned at zero no matter what.
        let m_star = cap50 - dr50 / rate;
        if rel50.is_finite() {
            rel_medians.push((id.clone(), rel50));
        }
        if dr50.is_nan() {
            println!(
                "{:16} {:>9.1} {:>9.2} {:>9.2} {:>9} {:>10} {:>9} {:>7.1}% {:>7}",
                id,
                pop50,
                cap50,
                mil50,
                "never",
                "never",
                "never",
                if zt > 0 { 100.0 * z as f64 / zt as f64 } else { 0.0 },
                deaths.get(id).copied().unwrap_or(0)
            );
            continue;
        }
        println!(
            "{:16} {:>9.1} {:>9.2} {:>9.2} {:>9.3} {:>9.2}% {:>9.2} {:>7.1}% {:>7}",
            id,
            pop50,
            cap50,
            mil50,
            dr50,
            rel50 * 100.0,
            m_star,
            if zt > 0 { 100.0 * z as f64 / zt as f64 } else { 0.0 },
            deaths.get(id).copied().unwrap_or(0)
        );
    }
    rel_medians.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    if let (Some(lo), Some(hi)) = (rel_medians.first(), rel_medians.last()) {
        println!(
            "\n  relative price spread across actors: {:.4}% ({}) .. {:.4}% ({})  =  {:.1}x",
            lo.1 * 100.0,
            lo.0,
            hi.1 * 100.0,
            hi.0,
            if lo.1 > 0.0 { hi.1 / lo.1 } else { f64::INFINITY }
        );
    }
    let pinned: Vec<&(String, f64)> = rel_medians.iter().collect();
    let _ = pinned;

    // ---- Part 4: outcome summary for counterfactual comparison --------------
    println!("\n--- Part 4: outcomes (for before/after comparison) ---");
    let mut total_deaths = 0usize;
    for (id, d) in &deaths {
        total_deaths += d;
        let _ = id;
    }
    println!("  deaths (actor-runs): {total_deaths} over {seed_count} seeds");
    let mut all_zero = 0usize;
    let mut all_ticks = 0usize;
    for (_, (z, t)) in zero_army.iter() {
        all_zero += z;
        all_ticks += t;
    }
    println!(
        "  actor-ticks with army < {:.2}: {} of {} = {:.1}%",
        interactions::MIN_DEFENSIBLE_MILITARY,
        all_zero,
        all_ticks,
        100.0 * all_zero as f64 / all_ticks as f64
    );
    let mut world_mil: Vec<f64> = mil_by_actor.values().flatten().copied().collect();
    let (_, w10, w50, w90, _) = quantiles(&mut world_mil);
    println!("  military_size across all actor-ticks: p10 {w10:.2}  median {w50:.2}  p90 {w90:.2}");
    let _: HashMap<(), ()> = HashMap::new();
}
