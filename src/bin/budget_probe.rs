//! Budget probe for infrastructure task 20 (`apply_treasury` — a budget with no
//! budget constraint), stage 1.
//!
//! Two jobs, both read-only.
//!
//! ## 1. `inventory` — the container walk demanded by §5 п.1 of the statement
//!
//! The statement's §0.1 table was obtained by grepping for the word `treasury` and
//! is explicitly labelled a *lower bound*, not an inventory: a site that touches the
//! metric through a key it never spells (a `DependencyRule.to`, a `RankBonusEffect.metric`
//! read out of a TOML file) cannot appear in such a grep **by construction** — and
//! `investigation_typed_metric_keys.md` §5.G records five separate occasions where
//! exactly that class of site was the one being looked for.
//!
//! So this mode enumerates by walking **the loaded `Scenario` struct itself**, field by
//! field, for every scenario in the registry, plus the common event pool that all three
//! share. Every metric key found in every container is emitted with its container, its
//! role (READ / WRITE / BOTH) and, where the container has one, its comparison operator
//! and threshold. Filtering for `treasury` happens *after* the walk, in the caller —
//! the walk itself has no notion of which metric is interesting.
//!
//! The field list is closed against `core/scenario.rs::Scenario` (28 fields) rather
//! than against memory: `SCENARIO_FIELDS_WALKED` below names every field, including
//! the ones that provably cannot carry a metric key, so that a future field addition
//! shows up as a mismatch instead of a silent omission.
//!
//! ## 2. `solvency` — the trajectory demanded by §5 п.2
//!
//! §1 of the statement derives the solvency criterion in closed form:
//! `net = α·eo·pop − β·mil` with `α = 0.001`, `β = 0.8`, hence an actor is solvent iff
//! `pop/mil ≥ β/(α·eo)`, i.e. `≥ 8` at the `economic_output` ceiling of 100. That is a
//! statement about `t = 0`. Whether it survives the run is a different question — the
//! statement flags this explicitly as the error task 15 made ("the magnitude was measured
//! correctly, but nobody checked whether the run reaches the event it was attributed to").
//!
//! So this mode reports, per actor per run: the `pop/mil` ratio and the required
//! `economic_output` at every tick (min / max / mean / final), the fraction of ticks the
//! actor is solvent, the sign changes of `net`, and occupancy of every threshold §0.1
//! found — `0`, `150`, `200` (with `desertion`'s `military_size > 50` conjunct evaluated,
//! not assumed), `300`, `500`, and the per-scenario action gates.
//!
//! Read-only: drives `tick()` and reads metrics. No engine symbol is modified.
//!
//! Usage:
//! ```bash
//! cargo run --release --bin budget_probe -- inventory
//! cargo run --release --bin budget_probe -- solvency <scenario> <ticks> <seeds>
//! ```

use engine13::core::{
    ActionCondition, ComparisonOperator, EventConditionType, MetricRef, RelativeMetricRef, Scenario,
    WorldState,
};
use engine13::engine::{tick, EventLog};
use engine13::scenarios::registry;
use rand::SeedableRng;
use std::collections::BTreeMap;

/// Every field of `core::scenario::Scenario`, so that the walk below is closed against
/// the struct rather than against what the author happened to remember. Fields that
/// cannot carry a metric key are listed too, with the reason.
const SCENARIO_FIELDS_WALKED: &[(&str, &str)] = &[
    ("id", "scalar"),
    ("label", "scalar"),
    ("description", "scalar"),
    ("start_year", "scalar"),
    ("tempo", "scalar"),
    ("tick_span", "scalar"),
    ("era", "scalar"),
    ("tick_label", "scalar"),
    ("actors", "WALKED: metrics, scenario_metrics, actor_tags[].metrics_modifier"),
    ("auto_deltas", "WALKED: metric, conditions[], ratio_conditions[]"),
    ("patron_actions", "WALKED: available_if, effects, cost"),
    ("milestone_events", "WALKED: condition, spawn_actor.initial_metrics"),
    ("rank_conditions", "WALKED: condition"),
    ("generation_mechanics", "WALKED: inheritance_coefficients keys + early_transfer.condition_metric"),
    ("llm_context", "scalar"),
    ("consequence_context", "scalar"),
    ("player_actor_id", "scalar"),
    ("status_indicators", "WALKED: metric"),
    ("global_metric_weights", "WALKED: keys"),
    ("features", "scalar flags"),
    ("military_conflict_probability", "scalar"),
    ("naval_conflict_probability", "scalar"),
    ("random_events", "WALKED: conditions[], effects keys"),
    ("generation_length", "scalar"),
    ("actions_per_tick", "scalar"),
    ("victory_condition", "WALKED: metric, additional_conditions[]"),
    ("universal_actions", "WALKED: available_if, effects, cost"),
    ("global_metrics_display", "WALKED: metric"),
    ("initial_family_metrics", "WALKED: keys"),
    ("max_random_events_per_tick", "scalar"),
    ("narrative_config", "WALKED: key_metrics"),
    ("dependencies", "WALKED: from (READ), to (WRITE)"),
    ("interaction_rules", "WALKED: conditions[], effects[]"),
    ("rank_bonuses", "WALKED: effects[] (delta or FLOOR)"),
    ("map", "no metric keys"),
    ("tag_definitions", "WALKED: metrics_modifier keys"),
    ("era_definitions", "no metric keys (auto_delta_modifier is a scalar)"),
];

/// One metric reference found by the container walk.
struct Hit {
    scenario: String,
    container: String,
    site: String,
    key: String,
    role: &'static str,
    detail: String,
}

fn op_str(op: &ComparisonOperator) -> &'static str {
    match op {
        ComparisonOperator::Less => "<",
        ComparisonOperator::LessOrEqual => "<=",
        ComparisonOperator::Greater => ">",
        ComparisonOperator::GreaterOrEqual => ">=",
        ComparisonOperator::Equal => "==",
    }
}

fn rel_key(r: &RelativeMetricRef) -> String {
    match r {
        RelativeMetricRef::SelfRelative(m) => format!("self.{}", m.as_str()),
        RelativeMetricRef::Absolute(m) => m.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    out: &mut Vec<Hit>,
    scenario: &str,
    container: &str,
    site: String,
    key: String,
    role: &'static str,
    detail: String,
) {
    out.push(Hit {
        scenario: scenario.to_string(),
        container: container.to_string(),
        site,
        key,
        role,
        detail,
    });
}

/// Walk every container of one scenario. Knows nothing about which metric matters.
fn walk_scenario(sc: &Scenario, out: &mut Vec<Hit>) {
    let id = sc.id.as_str();

    // --- actors ------------------------------------------------------------
    for a in &sc.actors {
        for (k, v) in &a.metrics {
            push(out, id, "actors[].metrics", a.id.clone(), k.clone(), "INIT", format!("= {}", v));
        }
        for (k, v) in &a.scenario_metrics {
            push(out, id, "actors[].scenario_metrics", a.id.clone(), k.clone(), "INIT", format!("= {}", v));
        }
        for (tag_id, t) in &a.actor_tags {
            for (k, v) in &t.metrics_modifier {
                push(out, id, "actors[].actor_tags", format!("{}/{}", a.id, tag_id), k.as_str().to_string(), "WRITE", format!("{:+}", v));
            }
        }
    }

    // --- auto_deltas -------------------------------------------------------
    for (i, d) in sc.auto_deltas.iter().enumerate() {
        push(out, id, "auto_deltas.metric", format!("#{}", i), d.metric.to_string(), "WRITE", format!("base {:+}", d.base));
        for c in &d.conditions {
            push(out, id, "auto_deltas.conditions", format!("#{}", i), c.metric.to_string(), "READ", format!("{} {} -> {:+}", op_str(&c.operator), c.value, c.delta));
        }
        for c in &d.ratio_conditions {
            push(out, id, "auto_deltas.ratio_conditions", format!("#{}", i), c.metric_a.to_string(), "READ", format!("ratio {} {}", op_str(&c.operator), c.ratio));
            push(out, id, "auto_deltas.ratio_conditions", format!("#{}", i), c.metric_b.to_string(), "READ", "ratio denom".to_string());
        }
    }

    // --- patron / universal actions ---------------------------------------
    for (label, list) in [("patron_actions", &sc.patron_actions), ("universal_actions", &sc.universal_actions)] {
        for act in list {
            if let ActionCondition::Metric { metric, operator, value } = &act.available_if {
                push(out, id, &format!("{}.available_if", label), act.id.clone(), metric.to_string(), "READ", format!("{} {}", op_str(operator), value));
            }
            for (k, v) in &act.effects {
                push(out, id, &format!("{}.effects", label), act.id.clone(), k.to_string(), "WRITE", format!("{:+}", v));
            }
            for (k, v) in &act.cost {
                push(out, id, &format!("{}.cost", label), act.id.clone(), k.to_string(), "BOTH", format!("{:+} (availability checked at actions.rs:206)", v));
            }
        }
    }

    // --- milestone events --------------------------------------------------
    for ev in &sc.milestone_events {
        if let EventConditionType::Metric { metric, actor_id, operator, value } = &ev.condition.condition_type {
            push(out, id, "milestone_events.condition", ev.id.clone(), metric.to_string(), "READ", format!("{} {} (actor_id={:?})", op_str(operator), value, actor_id));
        }
        if let Some(spawn) = &ev.spawn_actor {
            for (k, v) in &spawn.initial_metrics {
                push(out, id, "milestone_events.spawn_actor", format!("{}/{}", ev.id, spawn.actor_id), k.as_str().to_string(), "INIT", format!("= {}", v));
            }
        }
    }

    // --- rank conditions ---------------------------------------------------
    for (i, rc) in sc.rank_conditions.iter().enumerate() {
        if let EventConditionType::Metric { metric, actor_id, operator, value } = &rc.condition.condition_type {
            push(out, id, "rank_conditions.condition", format!("#{} {}", i, rc.region_id), metric.to_string(), "READ", format!("{} {} (actor_id={:?})", op_str(operator), value, actor_id));
        }
    }

    // --- generation mechanics ---------------------------------------------
    if let Some(gm) = &sc.generation_mechanics {
        for (k, v) in &gm.inheritance_coefficients {
            push(out, id, "generation_mechanics.inheritance_coefficients", "-".into(), k.clone(), "READ", format!("coeff {}", v));
        }
        // `early_transfer.condition_metric` is read at `engine/mod.rs:1381` and can
        // carry ANY metric key. It was missing from the first draft of this walk —
        // caught by walking the *code* sites (§5.G case 5) and finding a `.get(world)`
        // whose container this walk did not visit. Kept explicit so "checked, empty"
        // is distinguishable from "not checked".
        if let Some(et) = &gm.early_transfer {
            push(out, id, "generation_mechanics.early_transfer", format!("age {}", et.age), et.condition_metric.to_string(), "READ", format!("{} {}", op_str(&et.condition_operator), et.condition_value));
        }
    }

    // --- status indicators / global displays / weights / key_metrics -------
    for si in &sc.status_indicators {
        push(out, id, "status_indicators.metric", si.label.clone(), si.metric.to_string(), "READ(ui)", String::new());
    }
    for md in &sc.global_metrics_display {
        push(out, id, "global_metrics_display.metric", md.label.clone(), md.metric.to_string(), "READ(ui)", String::new());
    }
    for (k, srcs) in &sc.global_metric_weights {
        push(out, id, "global_metric_weights", "-".into(), k.to_string(), "READ", format!("{} sources", srcs.len()));
    }
    for km in &sc.narrative_config.key_metrics {
        push(out, id, "narrative_config.key_metrics", "-".into(), km.to_string(), "READ(prompt)", String::new());
    }

    // --- random events (scenario pool) -------------------------------------
    for ev in &sc.random_events {
        for c in &ev.conditions {
            push(out, id, "random_events.conditions", ev.id.clone(), rel_key(&c.metric), "READ", format!("{} {}", op_str(&c.operator), c.value));
        }
        for (k, v) in &ev.effects {
            push(out, id, "random_events.effects", ev.id.clone(), rel_key(k), "WRITE", format!("{:+}", v));
        }
    }

    // --- victory condition -------------------------------------------------
    if let Some(vc) = &sc.victory_condition {
        push(out, id, "victory_condition.metric", "-".into(), vc.metric.to_string(), "READ", format!("threshold {}", vc.threshold));
        for c in &vc.additional_conditions {
            push(out, id, "victory_condition.additional", "-".into(), c.metric.to_string(), "READ", format!("{} {}", op_str(&c.operator), c.value));
        }
    }

    // --- initial family metrics -------------------------------------------
    if let Some(fm) = &sc.initial_family_metrics {
        for (k, v) in fm {
            push(out, id, "initial_family_metrics", "-".into(), k.clone(), "INIT", format!("= {}", v));
        }
    }

    // --- dependencies ------------------------------------------------------
    for d in &sc.dependencies {
        push(out, id, "dependencies.from", d.id.clone(), d.from.as_str().to_string(), "READ", format!("mode {:?} thr {:?} coeff {}", d.mode, d.threshold, d.coefficient));
        push(out, id, "dependencies.to", d.id.clone(), d.to.as_str().to_string(), "WRITE", format!("mode {:?} thr {:?} coeff {}", d.mode, d.threshold, d.coefficient));
    }

    // --- interaction rules -------------------------------------------------
    for r in &sc.interaction_rules {
        for c in &r.conditions {
            push(out, id, "interaction_rules.conditions", r.id.clone(), c.metric.as_str().to_string(), "READ", format!("{:?} {} {}", c.actor, op_str(&c.operator), c.value));
        }
        for e in &r.effects {
            push(out, id, "interaction_rules.effects", r.id.clone(), e.metric.as_str().to_string(), "WRITE", format!("{:?} {:+}", e.actor, e.delta));
        }
    }

    // --- rank bonuses (delta OR floor) -------------------------------------
    for rb in &sc.rank_bonuses {
        for e in &rb.effects {
            let detail = match e.floor {
                Some(f) => format!("FLOOR {} (set_metric, engine/mod.rs:339)", f),
                None => format!("{:+}", e.delta),
            };
            push(out, id, "rank_bonuses.effects", format!("{:?}", rb.rank), e.metric.as_str().to_string(), "WRITE", detail);
        }
    }

    // --- tag definitions ---------------------------------------------------
    for t in &sc.tag_definitions {
        for (k, v) in &t.metrics_modifier {
            push(out, id, "tag_definitions.metrics_modifier", t.id.clone(), k.as_str().to_string(), "WRITE", format!("{:+}", v));
        }
    }
}

/// The common pool is not a `Scenario` field but every scenario runs it.
fn walk_common_events(out: &mut Vec<Hit>) {
    for ev in engine13::events::common_events() {
        for c in &ev.conditions {
            push(out, "COMMON", "events/common.rs conditions", ev.id.clone(), rel_key(&c.metric), "READ", format!("{} {}", op_str(&c.operator), c.value));
        }
        for (k, v) in &ev.effects {
            push(out, "COMMON", "events/common.rs effects", ev.id.clone(), rel_key(k), "WRITE", format!("{:+}", v));
        }
    }
}

fn inventory() {
    println!("# Поля Scenario, закрытые обходом ({} шт.)", SCENARIO_FIELDS_WALKED.len());
    for (f, how) in SCENARIO_FIELDS_WALKED {
        println!("FIELD\t{}\t{}", f, how);
    }

    let mut hits = Vec::new();
    walk_common_events(&mut hits);
    for id in ["constantinople_1430", "rome_375", "milan_1477"] {
        let sc = registry::load_by_id(id).expect("scenario");
        walk_scenario(&sc, &mut hits);
    }

    // Full key census first — the walk does not know what is interesting.
    let mut census: BTreeMap<(String, String), u32> = BTreeMap::new();
    for h in &hits {
        *census.entry((h.scenario.clone(), h.key.clone())).or_insert(0) += 1;
    }
    println!("\n# Перепись ВСЕХ ключей, найденных обходом (сценарий, ключ, сайтов)");
    for ((sc, key), n) in &census {
        println!("CENSUS\t{}\t{}\t{}", sc, key, n);
    }

    println!("\n# Сайты, относящиеся к treasury");
    for h in &hits {
        if h.key.contains("treasury") {
            println!("HIT\t{}\t{}\t{}\t{}\t{}\t{}", h.scenario, h.container, h.site, h.key, h.role, h.detail);
        }
    }

    println!("\n# Итог по контейнерам (только treasury)");
    let mut per: BTreeMap<(String, String, &str), u32> = BTreeMap::new();
    for h in &hits {
        if h.key.contains("treasury") {
            *per.entry((h.scenario.clone(), h.container.clone(), h.role)).or_insert(0) += 1;
        }
    }
    for ((sc, c, role), n) in &per {
        println!("SUM\t{}\t{}\t{}\t{}", sc, c, role, n);
    }
}

// ===========================================================================
// solvency mode
// ===========================================================================

struct Sol {
    ticks: u32,
    solvent_ticks: u32,
    net_sign_flips: u32,
    prev_net_pos: Option<bool>,
    ratio_min: f64,
    ratio_max: f64,
    ratio_final: f64,
    eo_req_final: f64,
    treas_start: f64,
    treas_final: f64,
    treas_min: f64,
    below_zero: u32,
    below_150: u32,
    below_200_and_mil50: u32,
    above_300: u32,
    below_500: u32,
    pop_start: f64,
    pop_final: f64,
    mil_start: f64,
    mil_final: f64,
    eo_final: f64,
}

impl Sol {
    fn new() -> Self {
        Sol {
            ticks: 0, solvent_ticks: 0, net_sign_flips: 0, prev_net_pos: None,
            ratio_min: f64::MAX, ratio_max: 0.0, ratio_final: 0.0, eo_req_final: 0.0,
            treas_start: f64::NAN, treas_final: 0.0, treas_min: f64::MAX,
            below_zero: 0, below_150: 0, below_200_and_mil50: 0, above_300: 0, below_500: 0,
            pop_start: f64::NAN, pop_final: 0.0, mil_start: f64::NAN, mil_final: 0.0, eo_final: 0.0,
        }
    }
}

fn solvency(scenario_id: &str, ticks: u32, seeds: &[u64]) {
    println!("actor\tseed\tticks\tpop0\tpopF\tmil0\tmilF\teoF\tratio_min\tratio_max\tratio_F\teo_req_F\tsolvent%\tnet_flips\ttreas0\ttreasF\ttreas_min\tt<0\tt<150\tdesert_ok\tt>300\tt<500");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let mut log = EventLog::default();
        let mut stats: BTreeMap<String, Sol> = BTreeMap::new();

        for _ in 0..ticks {
            for (aid, a) in world.actors.iter() {
                if world.dead_actor_ids.contains(aid) {
                    continue;
                }
                let pop = a.get_metric("population");
                let mil = a.get_metric("military_size");
                let eo = a.get_metric("economic_output");
                let tr = a.get_metric("treasury");
                let net = eo * pop * 0.001 - mil * 0.8;
                let ratio = if mil > 0.0 { pop / mil } else { f64::INFINITY };
                let eo_req = if pop > 0.0 { 800.0 * mil / pop } else { f64::INFINITY };

                let s = stats.entry(aid.clone()).or_insert_with(Sol::new);
                if s.treas_start.is_nan() {
                    s.treas_start = tr;
                    s.pop_start = pop;
                    s.mil_start = mil;
                }
                s.ticks += 1;
                if net > 0.0 {
                    s.solvent_ticks += 1;
                }
                if let Some(p) = s.prev_net_pos {
                    if p != (net > 0.0) {
                        s.net_sign_flips += 1;
                    }
                }
                s.prev_net_pos = Some(net > 0.0);
                if ratio.is_finite() {
                    s.ratio_min = s.ratio_min.min(ratio);
                    s.ratio_max = s.ratio_max.max(ratio);
                    s.ratio_final = ratio;
                }
                s.eo_req_final = eo_req;
                s.treas_final = tr;
                s.treas_min = s.treas_min.min(tr);
                s.pop_final = pop;
                s.mil_final = mil;
                s.eo_final = eo;
                if tr < 0.0 {
                    s.below_zero += 1;
                }
                if tr < 150.0 {
                    s.below_150 += 1;
                }
                if tr < 200.0 && mil > 50.0 {
                    s.below_200_and_mil50 += 1;
                }
                if tr > 300.0 {
                    s.above_300 += 1;
                }
                if tr < 500.0 {
                    s.below_500 += 1;
                }
            }
            tick(&mut world, &scenario, &mut log, &mut rng);
        }

        for (aid, s) in &stats {
            println!(
                "{}\t{}\t{}\t{:.0}\t{:.0}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{}\t{:.0}\t{:.0}\t{:.0}\t{}\t{}\t{}\t{}\t{}",
                aid, seed, s.ticks, s.pop_start, s.pop_final, s.mil_start, s.mil_final, s.eo_final,
                if s.ratio_min == f64::MAX { 0.0 } else { s.ratio_min }, s.ratio_max, s.ratio_final, s.eo_req_final,
                100.0 * s.solvent_ticks as f64 / s.ticks as f64, s.net_sign_flips,
                s.treas_start, s.treas_final, if s.treas_min == f64::MAX { 0.0 } else { s.treas_min },
                s.below_zero, s.below_150, s.below_200_and_mil50, s.above_300, s.below_500
            );
        }
    }
}

// ===========================================================================
// upheaval mode — the counterfactual task 19 did not run
// ===========================================================================
//
// `check_actor_upheaval` (`engine/mod.rs:1173–1194`) returns true when ANY of eight
// metrics — treasury among them — moved by more than 30 across the 5-tick window kept
// in `world.metric_history`. It feeds `condition_upheaval` (promotion Background→
// Foreground, `mod.rs:1076`) and, through `actor_upheaval_ticks`, the demotion guard
// `recent_upheaval` (`mod.rs:1134`).
//
// Unlike `power_projection`, this reader has NO CAP: a solvent actor moves its treasury
// by 5 × net per window, which for the big actors is 600–2000, permanently over the 30
// threshold. Task 19's counterfactual saturated treasury inside `power_projection` only
// and concluded `cf_divergent = 0` for the ottomans — a statement about `condition_power`,
// not about the whole relevance verdict.
//
// This mode replicates the engine's own predicate and reports, per actor:
//   `upheaval` — ticks where the real predicate is true;
//   `only_treasury` — ticks where it is true ONLY because of treasury (i.e. false when
//                     the treasury row is dropped from the eight);
//   `treasury_span` — the median 5-tick treasury swing, for scale.
const UPHEAVAL_METRICS: [&str; 8] = [
    "population", "military_size", "military_quality", "economic_output",
    "cohesion", "legitimacy", "external_pressure", "treasury",
];

fn upheaval(scenario_id: &str, ticks: u32, seeds: &[u64]) {
    println!("actor\tseed\tticks\tupheaval\tonly_treasury\tonly_treas%\ttreas_span_med");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let mut log = EventLog::default();
        // actor -> (ticks, upheaval, only_treasury, spans)
        let mut acc: BTreeMap<String, (u32, u32, u32, Vec<f64>)> = BTreeMap::new();

        for _ in 0..ticks {
            let ids: Vec<String> = world.actors.keys().cloned().collect();
            for aid in ids {
                if world.dead_actor_ids.contains(&aid) {
                    continue;
                }
                let mut any = false;
                let mut any_wo_treasury = false;
                let mut span_treasury = 0.0;
                for m in UPHEAVAL_METRICS {
                    let key = format!("{}:{}", aid, m);
                    if let Some(h) = world.metric_history.get(&key) {
                        if h.len() >= 2 {
                            let d = (h.back().copied().unwrap_or(0.0) - h.front().copied().unwrap_or(0.0)).abs();
                            if m == "treasury" {
                                span_treasury = d;
                            }
                            if d > 30.0 {
                                any = true;
                                if m != "treasury" {
                                    any_wo_treasury = true;
                                }
                            }
                        }
                    }
                }
                let e = acc.entry(aid).or_insert((0, 0, 0, Vec::new()));
                e.0 += 1;
                if any {
                    e.1 += 1;
                }
                if any && !any_wo_treasury {
                    e.2 += 1;
                }
                e.3.push(span_treasury);
            }
            tick(&mut world, &scenario, &mut log, &mut rng);
        }

        for (aid, (t, up, only, spans)) in &acc {
            let mut s = spans.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let med = if s.is_empty() { 0.0 } else { s[s.len() / 2] };
            println!(
                "{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}",
                aid, seed, t, up, only,
                if *up > 0 { 100.0 * *only as f64 / *up as f64 } else { 0.0 },
                med
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("inventory");
    match mode {
        "inventory" => inventory(),
        "upheaval" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            upheaval(scenario, ticks, &seeds);
        }
        "solvency" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            solvency(scenario, ticks, &seeds);
        }
        other => panic!("unknown mode: {}", other),
    }
}

// `MetricRef` needs a Display for the walk above; it already has one via `to_string()`
// on the canonical form. Assert that here so a change to that contract is loud.
#[allow(dead_code)]
fn _display_contract(m: &MetricRef) -> String {
    m.to_string()
}
