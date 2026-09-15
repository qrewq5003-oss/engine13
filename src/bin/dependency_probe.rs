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
    // Army as a share of the actor's own mobilisation capacity. Under a penalty priced
    // on the stock every actor has the SAME equilibrium share — but equilibrium is not
    // identity: combat, events and recovery keep actors scattered around it. The width
    // of that scatter is what decides whether a capacity-relative threshold can
    // discriminate at all, so it is measured rather than assumed.
    let mut fill_by_actor: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut deaths: BTreeMap<String, usize> = BTreeMap::new();
    let mut alive_end: BTreeMap<String, usize> = BTreeMap::new();

    // The rule under the microscope. Hard-coding one rule id in a measuring device is the
    // same smell the engine was just guarded against, so it is a switch with a default.
    let subject: String = std::env::var("E13_SUBJECT")
        .unwrap_or_else(|_| "external_pressure_to_military_size".to_string());
    let subject_metric: String = scenario
        .dependencies
        .iter()
        .find(|r| r.id == subject)
        .map(|r| r.to.as_str().to_string())
        .unwrap_or_else(|| "military_size".to_string());

    // Part 6: can each spawn milestone's gate ever be crossed in this world?
    // Authored content that never enters the world is invisible to every balance
    // measure in the project — it shows up as an actor that simply is not there.
    // Measured over the same runs, per (actor, metric) named by the gate.
    let mut gate_extremes: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new();
    // A range answers "is the gate reachable" only for an instantaneous gate. A gate
    // with `duration` asks whether the value HOLDS past the threshold for that many
    // ticks in a row, and a range can contain the threshold while no run of the needed
    // length exists anywhere. So the longest run is measured too, per milestone, using
    // the engine's own `ComparisonOperator::evaluate` rather than a re-implementation.
    let mut gate_longest_run: BTreeMap<String, u32> = BTreeMap::new();
    // Part 7: does every authored gated object ever fire? Milestones and random events
    // both put their own authored id into the event log, so the log is the census: an id
    // that never appears is content that was written, validated, shipped — and never
    // reached a single player. Counted per seed, so "fires in 1 of 30" is visible as
    // distinct from "never".
    let mut fired_seeds: BTreeMap<String, usize> = BTreeMap::new();
    // Part 9: the authored content the event log cannot see, because it is applied
    // silently — tags, region-rank bonuses, eras, auto-deltas. Measured from world
    // state, with one deliberate difference: auto-delta conditions are sampled BEFORE
    // the tick, because `phase_auto_deltas` is the first phase, so the pre-tick state
    // is exactly what it reads. No direction caveat is needed for that one.
    let mut tag_ticks: BTreeMap<String, usize> = BTreeMap::new();
    let mut tag_spread_seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut ranks_seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut eras_seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut autodelta_fired: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    // Two different questions, and they gave different answers on the first run:
    // "did it ever enter the world" is not "was it alive at the end".
    let mut spawned_seeds: BTreeMap<String, usize> = BTreeMap::new();
    let mut alive_end_seeds: BTreeMap<String, usize> = BTreeMap::new();
    // End-of-run values for spawned actors, so the no-player world reports the SAME
    // statistic as the played world (`sim`). Comparing a median-over-run here against an
    // end-of-run there is the median-of-ratio mistake wearing a different hat.
    let mut spawn_end: BTreeMap<String, Vec<(f64, f64, f64)>> = BTreeMap::new();

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
        let mut ever_seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut fired_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut run_now: BTreeMap<String, u32> = BTreeMap::new();
        let mut event_log = EventLog::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);

        // Tags carried at tick 0: everything beyond this set was acquired by spread.
        let mut initial_tags: std::collections::BTreeSet<(String, String)> =
            std::collections::BTreeSet::new();
        for (id, a) in world.actors.iter() {
            for t in &a.tags {
                initial_tags.insert((id.clone(), t.clone()));
            }
        }

        dep_trace_enable();
        for _ in 0..ticks {
            // BEFORE the tick: exactly the state `phase_auto_deltas` will read.
            // Conditions of an auto-delta are ADDITIVE MODIFIERS, not gates: the engine
            // starts from `base` and adds each satisfied condition's own delta
            // (`phase_auto_deltas`, mod.rs). So the question is not "does the auto-delta
            // fire" — it always does — but "does this particular modifier ever apply".
            // Measured per (auto-delta, condition) pair, with the engine's own reader
            // (`MetricRef::get`, which defaults an absent container to 0.0) and the
            // engine's own comparison, rather than a re-implementation.
            for (i, ad) in scenario.auto_deltas.iter().enumerate() {
                for (j, c) in ad.conditions.iter().enumerate() {
                    if c.operator.evaluate(c.metric.get(&world), c.value) {
                        *autodelta_fired.entry((i, j)).or_default() += 1;
                    }
                }
            }
            tick(&mut world, &scenario, &mut event_log, &mut rng);
            let rows: Vec<DepTraceRow> = dep_trace_take();
            for e in &event_log.events {
                fired_ids.insert(e.id.clone());
            }
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
                if row.rule == subject {
                    drain_by_actor.entry(row.actor.clone()).or_default().push(row.delta.abs());
                    if row.to_before.abs() > 1e-12 {
                        rel_by_actor
                            .entry(row.actor.clone())
                            .or_default()
                            .push(row.delta.abs() / row.to_before.abs());
                    }
                }
            }
            for m in &scenario.milestone_events {
                if let Some(spawn) = &m.spawn_actor {
                    if world.actors.contains_key(&spawn.actor_id) {
                        ever_seen.insert(spawn.actor_id.clone());
                    }
                }
            }

            // Spawn-gate metrics, sampled at the tick boundary. The engine evaluates
            // milestones inside the tick (`phase_events`), so this sample is later than
            // the engine's read. What makes that safe is the DIRECTION of what lies
            // between them, not its size: the only metric writer reachable from
            // `phase_events` is `apply_milestone_effects`, and it only LOWERS
            // `ottomans.cohesion` (by 10 — larger than the 7.97 margin of the mamluks
            // finding, so "the difference is small" was the wrong argument). Because the
            // write goes down, this sample is a LOWER bound on what the engine saw, and
            // "the gate was never crossed" only gets stronger. That set of writers is
            // pinned by `phase_events_world_writers_are_the_expected_set`.
            //
            // Independently: the `entered N/30` column is the engine's own decision, not
            // an inference from these samples, so it does not depend on any of this.
            for m in &scenario.milestone_events {
                let engine13::core::EventConditionType::Metric { metric, .. } = &m.condition.condition_type
                else {
                    continue;
                };
                let (gate_actor, bare) = match metric {
                    engine13::core::MetricRef::Actor { actor_id, metric } => {
                        (actor_id.to_string(), metric.as_str().to_string())
                    }
                    _ => continue,
                };
                if let Some(a) = world.actors.get(&gate_actor) {
                    let v = a.get_metric(&bare);
                    let e = gate_extremes
                        .entry((gate_actor.clone(), bare.clone()))
                        .or_insert((f64::MAX, f64::MIN));
                    e.0 = e.0.min(v);
                    e.1 = e.1.max(v);
                    let engine13::core::EventConditionType::Metric { operator, value, .. } =
                        &m.condition.condition_type
                    else {
                        continue;
                    };
                    let run = run_now.entry(m.id.clone()).or_insert(0);
                    if operator.evaluate(v, *value) {
                        *run += 1;
                        let best = gate_longest_run.entry(m.id.clone()).or_insert(0);
                        if *run > *best {
                            *best = *run;
                        }
                    } else {
                        *run = 0;
                    }
                }
            }

            for (id, a) in world.actors.iter() {
                *ranks_seen.entry(format!("{:?}", a.region_rank)).or_default() += 1;
                *eras_seen.entry(format!("{:?}", a.era)).or_default() += 1;
                for t in &a.tags {
                    *tag_ticks.entry(t.clone()).or_default() += 1;
                    if !initial_tags.contains(&(id.clone(), t.clone())) {
                        *tag_spread_seen.entry(t.clone()).or_default() += 1;
                    }
                }
            }

            // End-of-tick state per actor.
            for (id, actor) in world.actors.iter() {
                let mil = actor.get_metric(&subject_metric);
                mil_by_actor.entry(id.clone()).or_default().push(mil);
                pop_by_actor.entry(id.clone()).or_default().push(actor.get_metric("population"));
                cap_by_actor
                    .entry(id.clone())
                    .or_default()
                    .push(interactions::military_capacity(actor));
                let cap_now = interactions::military_capacity(actor);
                if cap_now > 1e-9 {
                    fill_by_actor.entry(id.clone()).or_default().push(mil / cap_now);
                }
                let z = zero_army.entry(id.clone()).or_insert((0, 0));
                z.1 += 1;
                if mil < interactions::MIN_DEFENSIBLE_MILITARY {
                    z.0 += 1;
                }
            }
        }
        for id in &fired_ids {
            *fired_seeds.entry(id.clone()).or_default() += 1;
        }
        for m in &scenario.milestone_events {
            if let Some(spawn) = &m.spawn_actor {
                if ever_seen.contains(&spawn.actor_id) {
                    *spawned_seeds.entry(spawn.actor_id.clone()).or_default() += 1;
                }
                if let Some(a) = world.actors.get(&spawn.actor_id) {
                    *alive_end_seeds.entry(spawn.actor_id.clone()).or_default() += 1;
                    spawn_end.entry(spawn.actor_id.clone()).or_default().push((
                        a.get_metric("population"),
                        interactions::military_capacity(a),
                        a.get_metric("military_size"),
                    ));
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
    println!("--- Part 3: {subject} (target `{subject_metric}`), per actor ---");
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
    let mut pooled_fill: Vec<f64> = fill_by_actor.values().flatten().copied().collect();
    let (fmin, f10, f50, f90, fmax) = quantiles(&mut pooled_fill);
    println!(
        "  army / capacity over all actor-ticks: min {fmin:.3}  p10 {f10:.3}  median {f50:.3}  p90 {f90:.3}  max {fmax:.3}"
    );
    let mut per_actor_fill: Vec<(String, f64)> = fill_by_actor
        .iter()
        .map(|(id, v)| {
            let mut v = v.clone();
            let (_, _, m, _, _) = quantiles(&mut v);
            (id.clone(), m)
        })
        .collect();
    per_actor_fill.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    // Printed in full, and next to the ratio of the two medians Part 3 reports, because
    // the two are different statistics: `median(m/C)` is not `median(m)/median(C)` when
    // both move. Two adjacent numbers about one actor with no note is how a reader is
    // sent looking for a regime difference that is not there.
    println!("  per-actor army/capacity — median of the ratio vs ratio of the medians:");
    for (id, med_ratio) in &per_actor_fill {
        let mut m = mil_by_actor.get(id).cloned().unwrap_or_default();
        let mut c = cap_by_actor.get(id).cloned().unwrap_or_default();
        let (_, _, m50, _, _) = quantiles(&mut m);
        let (_, _, c50, _, _) = quantiles(&mut c);
        let ratio_of_medians = if c50 > 1e-9 { m50 / c50 } else { f64::NAN };
        println!("    {id:18} median(m/C) {med_ratio:>9.3}   median(m)/median(C) {ratio_of_medians:>9.3}");
    }
    let below = |t: f64| {
        100.0 * pooled_fill.iter().filter(|x| **x < t).count() as f64 / pooled_fill.len() as f64
    };
    println!(
        "  actor-ticks below a capacity-relative floor: 0.10 -> {:.2}%  0.25 -> {:.2}%  0.40 -> {:.2}%  0.60 -> {:.2}%",
        below(0.10), below(0.25), below(0.40), below(0.60)
    );
    println!("\n--- Part 6: spawn milestones — is the gate reachable at all? ---");
    let mut any_spawn = false;
    for m in &scenario.milestone_events {
        let Some(spawn) = &m.spawn_actor else { continue };
        any_spawn = true;
        let seeds = spawned_seeds.get(&spawn.actor_id).copied().unwrap_or(0);
        let alive = alive_end_seeds.get(&spawn.actor_id).copied().unwrap_or(0);
        match &m.condition.condition_type {
            engine13::core::EventConditionType::Metric { metric, operator, value, .. } => {
                let gate = match metric {
                    engine13::core::MetricRef::Actor { actor_id, metric } => {
                        Some((actor_id.to_string(), metric.as_str().to_string()))
                    }
                    _ => None,
                };
                let seen = gate
                    .as_ref()
                    .and_then(|k| gate_extremes.get(&(k.0.clone(), k.1.clone())).copied());
                let dur = m.condition.duration.unwrap_or(1);
                let longest = gate_longest_run.get(&m.id).copied().unwrap_or(0);
                match (gate, seen) {
                    (Some((ga, gm)), Some((lo, hi))) => println!(
                        "  {:18} entered {seeds:>2}/{seed_count}, alive at end {alive:>2}/{seed_count} | gate {ga}.{gm} {operator:?} {value} for {dur} tick(s) | observed {lo:.2} .. {hi:.2} | longest run past it {longest}{}",
                        spawn.actor_id,
                        if seeds == 0 { "   <-- NEVER ENTERS THE WORLD" } else if alive == 0 { "   <-- enters, never survives" } else { "" }
                    ),
                    _ => println!(
                        "  {:18} entered {seeds:>2}/{seed_count}, alive at end {alive:>2}/{seed_count} | gate not an actor metric",
                        spawn.actor_id
                    ),
                }
            }
            engine13::core::EventConditionType::Tick { tick } => println!(
                "  {:18} entered {seeds:>2}/{seed_count}, alive at end {alive:>2}/{seed_count} | gate tick == {tick}",
                spawn.actor_id
            ),
            engine13::core::EventConditionType::ActorState { actor_id, state } => println!(
                "  {:18} entered {seeds:>2}/{seed_count}, alive at end {alive:>2}/{seed_count} | gate {actor_id} state {state:?}",
                spawn.actor_id
            ),
        }
    }
    if !any_spawn {
        println!("  (this scenario spawns no actors)");
    }

    println!("\n--- Part 6b: spawned actors AT END OF RUN (same statistic as `sim` prints) ---");
    for m in &scenario.milestone_events {
        let Some(sp) = &m.spawn_actor else { continue };
        let entered = spawned_seeds.get(&sp.actor_id).copied().unwrap_or(0);
        let alive = alive_end_seeds.get(&sp.actor_id).copied().unwrap_or(0);
        let rows = spawn_end.get(&sp.actor_id).cloned().unwrap_or_default();
        if rows.is_empty() {
            println!("  {:18} entered {entered}/{seed_count}, alive at end 0/{seed_count}", sp.actor_id);
            continue;
        }
        let mut pops: Vec<f64> = rows.iter().map(|r| r.0).collect();
        let mut arms: Vec<f64> = rows.iter().map(|r| r.2).collect();
        let (_, _, p50, _, _) = quantiles(&mut pops);
        let (_, _, a50, _, _) = quantiles(&mut arms);
        println!(
            "  {:18} entered {entered}/{seed_count}, alive at end {alive}/{seed_count} | at end: population median {p50:.1} | army median {a50:.2}",
            sp.actor_id
        );
    }

    println!("\n--- Part 7: authored gated content — does it ever fire? ---");
    let mut never: Vec<String> = Vec::new();
    let mut rare: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut report = |kind: &str, id: &str, never: &mut Vec<String>, rare: &mut Vec<String>| {
        let n = fired_seeds.get(id).copied().unwrap_or(0);
        if n == 0 {
            never.push(format!("  {kind:10} {id:34} NEVER in {seed_count} seeds"));
        } else if n * 10 <= seed_count as usize {
            rare.push(format!("  {kind:10} {id:34} {n}/{seed_count} seeds"));
        }
    };
    for m in &scenario.milestone_events {
        total += 1;
        report("milestone", &m.id, &mut never, &mut rare);
    }
    let pool: Vec<engine13::core::RandomEvent> = engine13::events::common_events()
        .into_iter()
        .chain(scenario.random_events.iter().cloned())
        .collect();
    for e in &pool {
        total += 1;
        report("random", &e.id, &mut never, &mut rare);
    }
    println!("  authored gated objects checked: {total} ({} milestones, {} random events)", scenario.milestone_events.len(), pool.len());
    if never.is_empty() {
        println!("  never fires: none");
    } else {
        println!("  never fires ({}):", never.len());
        for l in &never {
            println!("{l}");
        }
    }
    if !rare.is_empty() {
        println!("  fires in 10 % of seeds or fewer ({}):", rare.len());
        for l in &rare {
            println!("{l}");
        }
    }

    println!("\n--- Part 8: gate of every milestone, fired or not ---");
    for m in &scenario.milestone_events {
        let n = fired_seeds.get(&m.id).copied().unwrap_or(0);
        let dur = m.condition.duration.unwrap_or(1);
        let longest = gate_longest_run.get(&m.id).copied().unwrap_or(0);
        let flag = if n == 0 { "   <-- NEVER" } else { "" };
        match &m.condition.condition_type {
            engine13::core::EventConditionType::Metric { metric, operator, value, .. } => {
                let key = match metric {
                    engine13::core::MetricRef::Actor { actor_id, metric } => {
                        Some((actor_id.to_string(), metric.as_str().to_string()))
                    }
                    _ => None,
                };
                match key.and_then(|k| gate_extremes.get(&k).copied().map(|v| (k, v))) {
                    Some(((ga, gm), (lo, hi))) => println!(
                        "  {:28} fired {n:>2}/{seed_count} | {ga}.{gm} {operator:?} {value} for {dur} | observed {lo:.2} .. {hi:.2} | longest run {longest}{flag}",
                        m.id
                    ),
                    None => println!(
                        "  {:28} fired {n:>2}/{seed_count} | gate not actor-scoped: {metric:?} {operator:?} {value}{flag}",
                        m.id
                    ),
                }
            }
            engine13::core::EventConditionType::Tick { tick } => println!(
                "  {:28} fired {n:>2}/{seed_count} | gate tick == {tick}{flag}",
                m.id
            ),
            engine13::core::EventConditionType::ActorState { actor_id, state } => println!(
                "  {:28} fired {n:>2}/{seed_count} | gate {actor_id} state {state:?}{flag}",
                m.id
            ),
        }
    }

    println!("\n--- Part 9: authored content the event log cannot see ---");
    println!("  tags ({} authored):", scenario.tag_definitions.len());
    let mut dead_tags = 0usize;
    for t in &scenario.tag_definitions {
        let carried = tag_ticks.get(&t.id).copied().unwrap_or(0);
        let spread = tag_spread_seen.get(&t.id).copied().unwrap_or(0);
        if carried == 0 {
            dead_tags += 1;
            println!("    {:26} NEVER carried by anyone", t.id);
        } else if !t.spreads_via.is_empty() && spread == 0 {
            println!("    {:26} carried {carried} actor-ticks, but NEVER spread (spreads_via {:?})", t.id, t.spreads_via);
        }
    }
    if dead_tags == 0 {
        println!("    (every authored tag is carried by someone)");
    }
    println!("  region ranks held by living actors: {ranks_seen:?}");
    println!("  rank-bonus rules ({} authored):", scenario.rank_bonuses.len());
    for r in &scenario.rank_bonuses {
        let key = format!("{:?}", r.rank);
        let held = ranks_seen.get(&key).copied().unwrap_or(0);
        println!(
            "    rank {key:8} {} effect(s) | rank held {held} actor-ticks{}",
            r.effects.len(),
            if held == 0 { "   <-- NOBODY EVER HOLDS THIS RANK" } else { "" }
        );
    }
    println!("  eras reached: {eras_seen:?}");
    println!("  era definitions ({} authored):", scenario.era_definitions.len());
    for e in &scenario.era_definitions {
        let key = format!("{:?}", e.era);
        let seen = eras_seen.get(&key).copied().unwrap_or(0);
        println!(
            "    {key:16} min_tick {} | seen {seen} actor-ticks{}",
            e.min_tick,
            if seen == 0 { "   <-- NEVER REACHED" } else { "" }
        );
    }
    let total_conds: usize = scenario.auto_deltas.iter().map(|a| a.conditions.len()).sum();
    println!(
        "  auto-delta condition modifiers ({} across {} auto-deltas; conditions are ADDITIVE, not gates; sampled pre-tick):",
        total_conds,
        scenario.auto_deltas.len()
    );
    let mut dead_mods = 0usize;
    for (i, ad) in scenario.auto_deltas.iter().enumerate() {
        for (j, c) in ad.conditions.iter().enumerate() {
            let n = autodelta_fired.get(&(i, j)).copied().unwrap_or(0);
            if n == 0 {
                dead_mods += 1;
                println!(
                    "    #{i}.{j} on {} | {:?} {:?} {} adds {} | NEVER APPLIES",
                    ad.metric, c.metric, c.operator, c.value, c.delta
                );
            }
        }
    }
    println!("    dead modifiers: {dead_mods} of {total_conds}");

    let mut world_mil: Vec<f64> = mil_by_actor.values().flatten().copied().collect();
    let (_, w10, w50, w90, _) = quantiles(&mut world_mil);
    println!("  military_size across all actor-ticks: p10 {w10:.2}  median {w50:.2}  p90 {w90:.2}");
    let _: HashMap<(), ()> = HashMap::new();
}
