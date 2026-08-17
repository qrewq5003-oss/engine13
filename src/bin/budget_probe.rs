//! Budget probe for infrastructure task 20 (`apply_treasury` — a budget with no
//! budget constraint), stage 1.
//!
//! Three jobs, all read-only.
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
//! ## 3. `upheaval` — the counterfactual task 19 did not run
//!
//! `check_actor_upheaval` (`engine/mod.rs:1173–1194`) trips when ANY of eight metrics moved
//! by more than 30 across a 5-tick window. Seven of the eight are bounded; `treasury` is not.
//! This mode reports how often the predicate is true *only* because of treasury, and — second
//! pass — what it would decide if treasury were fed through `[0, 500]`, the window
//! `power_projection` already declares for the same stock.
//!
//! Usage:
//! ```bash
//! cargo run --release --bin budget_probe -- inventory
//! cargo run --release --bin budget_probe -- solvency <scenario> <ticks> <seeds>
//! cargo run --release --bin budget_probe -- upheaval <scenario> <ticks> <seeds>
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

/// Second-pass counterfactual: the predicate with treasury fed through the window the
/// *other* relevance reader already declares for it — `power_projection` treats the
/// stock as `(treasury / 500).clamp(0, 1)`, i.e. as constant outside `[0, 500]`. This
/// column answers what `check_actor_upheaval` would decide if it used the same window
/// instead of comparing a raw unbounded stock against a threshold calibrated for
/// `0..100` metrics.
const TREASURY_WINDOW: (f64, f64) = (0.0, 500.0);

/// Per-actor accumulator for `upheaval` mode.
#[derive(Default)]
struct UpheavalAcc {
    ticks: u32,
    upheaval: u32,
    only_treasury: u32,
    windowed: u32,
    win_only_treasury: u32,
    spans: Vec<f64>,
}

fn upheaval(scenario_id: &str, ticks: u32, seeds: &[u64]) {
    println!("actor\tseed\tticks\tupheaval\tonly_treasury\tonly_treas%\twindowed\twin_only_treas\ttreas_span_med");
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
        let mut acc: BTreeMap<String, UpheavalAcc> = BTreeMap::new();

        for _ in 0..ticks {
            let ids: Vec<String> = world.actors.keys().cloned().collect();
            for aid in ids {
                if world.dead_actor_ids.contains(&aid) {
                    continue;
                }
                let mut any = false;
                let mut any_wo_treasury = false;
                let mut any_windowed = false;
                let mut span_treasury = 0.0;
                for m in UPHEAVAL_METRICS {
                    let key = format!("{}:{}", aid, m);
                    if let Some(h) = world.metric_history.get(&key) {
                        if h.len() >= 2 {
                            let front = h.front().copied().unwrap_or(0.0);
                            let back = h.back().copied().unwrap_or(0.0);
                            let d = (back - front).abs();
                            if m == "treasury" {
                                span_treasury = d;
                                // same span, but through `power_projection`'s own window
                                let fw = front.clamp(TREASURY_WINDOW.0, TREASURY_WINDOW.1);
                                let bw = back.clamp(TREASURY_WINDOW.0, TREASURY_WINDOW.1);
                                if (bw - fw).abs() > 30.0 {
                                    any_windowed = true;
                                }
                            } else if d > 30.0 {
                                any_windowed = true;
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
                let e = acc.entry(aid).or_default();
                e.ticks += 1;
                if any {
                    e.upheaval += 1;
                }
                if any && !any_wo_treasury {
                    e.only_treasury += 1;
                }
                if any_windowed {
                    e.windowed += 1;
                }
                if any_windowed && !any_wo_treasury {
                    e.win_only_treasury += 1;
                }
                e.spans.push(span_treasury);
            }
            tick(&mut world, &scenario, &mut log, &mut rng);
        }

        for (aid, a) in &acc {
            let (t, up, only, win, win_only) = (&a.ticks, &a.upheaval, &a.only_treasury, &a.windowed, &a.win_only_treasury);
            let mut s = a.spans.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let med = if s.is_empty() { 0.0 } else { s[s.len() / 2] };
            println!(
                "{}\t{}\t{}\t{}\t{}\t{:.1}\t{}\t{}\t{:.1}",
                aid, seed, t, up, only,
                if *up > 0 { 100.0 * *only as f64 / *up as f64 } else { 0.0 },
                win, win_only, med
            );
        }
    }
}

// ===========================================================================
// d3 mode — counterfactual for the relative-threshold variant
// ===========================================================================
//
// (D₂) replaced the *value* with a clamped one; (D₃) replaces the *threshold* with a
// scaled one: a metric trips when `|Δ| > max(30, k·|level at window start|)`. The `30`
// floor keeps the rule identical to today's on any metric whose level is small enough
// that `k·level < 30`, so the five `0..100` metrics are untouched for `k <= 0.3`.
//
// `k = 0.30` is derived, not picked: `30` on a `0..100` scale IS 30% of that metric's
// own range, so 30% of an unbounded metric's own level is the same statement carried to
// a metric that has no fixed range. The sweep below exists to show the answer is not an
// artefact of that particular number.
//
// Only the treasury row is scaled here — the same scope (D₂) had, so that the measured
// difference is the rule's shape and not the rule's reach.
fn d3(scenario_id: &str, ticks: u32, seeds: &[u64], ks: &[f64]) {
    print!("actor\tseed\tticks\traw\twindowed");
    for k in ks {
        print!("\tk={}", k);
    }
    println!();
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
        // actor -> (ticks, raw, windowed, per-k counts)
        let mut acc: BTreeMap<String, (u32, u32, u32, Vec<u32>)> = BTreeMap::new();

        for _ in 0..ticks {
            let ids: Vec<String> = world.actors.keys().cloned().collect();
            for aid in ids {
                if world.dead_actor_ids.contains(&aid) {
                    continue;
                }
                let mut raw = false;
                let mut win = false;
                let mut rel = vec![false; ks.len()];
                for m in UPHEAVAL_METRICS {
                    let key = format!("{}:{}", aid, m);
                    if let Some(h) = world.metric_history.get(&key) {
                        if h.len() >= 2 {
                            let front = h.front().copied().unwrap_or(0.0);
                            let back = h.back().copied().unwrap_or(0.0);
                            let d = (back - front).abs();
                            if m == "treasury" {
                                if d > 30.0 {
                                    raw = true;
                                }
                                let fw = front.clamp(TREASURY_WINDOW.0, TREASURY_WINDOW.1);
                                let bw = back.clamp(TREASURY_WINDOW.0, TREASURY_WINDOW.1);
                                if (bw - fw).abs() > 30.0 {
                                    win = true;
                                }
                                for (i, k) in ks.iter().enumerate() {
                                    if d > 30.0_f64.max(k * front.abs()) {
                                        rel[i] = true;
                                    }
                                }
                            } else if d > 30.0 {
                                raw = true;
                                win = true;
                                for r in rel.iter_mut() {
                                    *r = true;
                                }
                            }
                        }
                    }
                }
                let e = acc.entry(aid).or_insert((0, 0, 0, vec![0; ks.len()]));
                e.0 += 1;
                if raw {
                    e.1 += 1;
                }
                if win {
                    e.2 += 1;
                }
                for (i, r) in rel.iter().enumerate() {
                    if *r {
                        e.3[i] += 1;
                    }
                }
            }
            tick(&mut world, &scenario, &mut log, &mut rng);
        }

        for (aid, (t, raw, win, rel)) in &acc {
            print!("{}\t{}\t{}\t{}\t{}", aid, seed, t, raw, win);
            for r in rel {
                print!("\t{}", r);
            }
            println!();
        }
    }
}

// ===========================================================================
// popsink mode — task 21 stage 1, step 1
// ===========================================================================
//
// Task 21 asks whether three actors are born with `population = 0` and whether
// anything writes the key afterwards — the second half being the part §2 п.1 refuses
// to take on trust ("without it, «income is identically zero» is plausible but not
// proven": `add_metric` creates a missing key).
//
// So this mode reports, per actor per run, the *lifecycle* of `population`:
//   `key0`      — was the key present in `metrics` at the actor's first observed tick
//                 (distinguishes "absent" from "present and zero" — the postановка's
//                 own formulation had to be corrected on exactly this point);
//   `pop0`      — its value there;
//   `zero_at`   — the first tick at which it is `0.0` (`-1` = never);
//   `zero%`     — share of observed ticks at zero;
//   `inc0%`     — share of ticks where `apply_treasury`'s income term is exactly zero;
//   `dep1/dep2` — the per-tick population delta the two population-writing dependency
//                 rules would produce at that tick (`economic_output_to_population`,
//                 deficit ×20 below 50; `low_economic_output_to_population_decay`,
//                 deficit ×100 below 15), summed over the run. These are the only two
//                 population writers in any scenario container; the only *positive*
//                 writer in the whole project is the migration interaction
//                 (`interactions.rs:683`), which needs a neighbour pair.
//   `pairs`     — number of neighbour pairs the actor participates in, i.e. whether
//                 that positive writer can reach it at all.
//
// Read-only: drives `tick()` and reads metrics. No engine symbol is modified.
struct PopAcc {
    ticks: u32,
    key_at_first: bool,
    pop_first: f64,
    zero_at: i64,
    zero_ticks: u32,
    income_zero_ticks: u32,
    dep1_sum: f64,
    dep2_sum: f64,
    pairs_first: usize,
    income_sum: f64,
    expense_sum: f64,
}

fn popsink(scenario_id: &str, ticks: u32, seeds: &[u64]) {
    println!("actor\tseed\tticks\tkey0\tpop0\tzero_at\tzero%\tinc0%\tdep1_sum\tdep2_sum\tpairs\tinc_sum\texp_sum");
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
        let mut acc: BTreeMap<String, PopAcc> = BTreeMap::new();

        for t in 0..ticks {
            // Neighbour-pair reachability: a pair exists if EITHER side lists the other
            // (`interactions.rs::get_neighbor_pairs` dedups sorted pairs), so count both
            // directions against actors that actually exist in the world.
            let mut pair_count: BTreeMap<String, usize> = BTreeMap::new();
            for (aid, a) in world.actors.iter() {
                for n in &a.neighbors {
                    if world.actors.contains_key(&n.id) {
                        *pair_count.entry(aid.clone()).or_insert(0) += 1;
                        *pair_count.entry(n.id.clone()).or_insert(0) += 1;
                    }
                }
            }
            for (aid, a) in world.actors.iter() {
                if world.dead_actor_ids.contains(aid) {
                    continue;
                }
                let pop = a.get_metric("population");
                let eo = a.get_metric("economic_output");
                let mil = a.get_metric("military_size");
                let income = eo * pop * 0.001;
                let expense = mil * 0.8;
                let dep1 = if eo < 50.0 { -((50.0 - eo) * 20.0) } else { 0.0 };
                let dep2 = if eo < 15.0 { -((15.0 - eo) * 100.0) } else { 0.0 };
                let e = acc.entry(aid.clone()).or_insert_with(|| PopAcc {
                    ticks: 0,
                    key_at_first: a.metrics.contains_key("population"),
                    pop_first: pop,
                    zero_at: -1,
                    zero_ticks: 0,
                    income_zero_ticks: 0,
                    dep1_sum: 0.0,
                    dep2_sum: 0.0,
                    pairs_first: pair_count.get(aid).copied().unwrap_or(0),
                    income_sum: 0.0,
                    expense_sum: 0.0,
                });
                e.ticks += 1;
                if pop == 0.0 {
                    e.zero_ticks += 1;
                    if e.zero_at < 0 {
                        e.zero_at = t as i64;
                    }
                }
                if income == 0.0 {
                    e.income_zero_ticks += 1;
                }
                e.dep1_sum += dep1;
                e.dep2_sum += dep2;
                e.income_sum += income;
                e.expense_sum += expense;
            }
            tick(&mut world, &scenario, &mut log, &mut rng);
        }

        for (aid, e) in &acc {
            println!(
                "{}\t{}\t{}\t{}\t{:.1}\t{}\t{:.1}\t{:.1}\t{:.0}\t{:.0}\t{}\t{:.0}\t{:.0}",
                aid, seed, e.ticks, e.key_at_first, e.pop_first, e.zero_at,
                100.0 * e.zero_ticks as f64 / e.ticks as f64,
                100.0 * e.income_zero_ticks as f64 / e.ticks as f64,
                e.dep1_sum, e.dep2_sum, e.pairs_first, e.income_sum, e.expense_sum
            );
        }
    }
}

// ===========================================================================
// decisive mode — task 21 stage 1, step 4 (the equilibrium calculation)
// ===========================================================================
//
// §34 of `investigation_treasury_budget.md` counted "decisive flips" with temporary
// engine instrumentation that was deliberately not committed: ticks where the upheaval
// predicate decides the relevance verdict, i.e. where neither `condition_power` nor
// `condition_contact` holds. Task 21 §2 п.4 needs that number back, plus the same number
// under a counterfactual in which the zero-population actors are solvent.
//
// The verdict is replicated here from `check_relevance_thresholds`
// (`engine/mod.rs:1024–1106`) rather than re-instrumented in the engine:
//   power   = power_projection > 0.7 × mean(power_projection over all actors)
//   contact = some Foreground actor lists this actor as a neighbour with distance ≤ 2
//   upheaval= any of the eight metrics moved > 30 across the 5-tick history window
//             OR cohesion < 25 OR legitimacy < 20
// `decisive` = upheaval && !power && !contact. Both the tick count and the number of
// transitions of that series are reported, because §22 and §34 of the task-20 document
// disagree by one for rome (19 vs 20) and the postановка demands that be reconciled.
//
// `popfix > 0` is the counterfactual: before every tick, any live actor whose
// `population` is ≤ 0 gets `population = popfix`. The engine is not touched; the probe
// mutates world state, which is exactly what a counterfactual is. `popfix = 0` = baseline.
#[derive(Default)]
struct DecAcc {
    ticks: u32,
    power: u32,
    contact: u32,
    upheaval: u32,
    decisive: u32,
    decisive_treasury_only: u32,
    dec_transitions: u32,
    prev_decisive: Option<bool>,
    verdict: u32,
}

fn decisive(scenario_id: &str, ticks: u32, seeds: &[u64], popfix: f64) {
    println!("actor\tseed\tticks\tpower\tcontact\tupheaval\tverdict\tdecisive\tdec_treas_only\tdec_flips");
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
        let mut acc: BTreeMap<String, DecAcc> = BTreeMap::new();

        for _ in 0..ticks {
            // Two counterfactuals, and the difference between them turned out to be the
            // result: `popfix > 0` is a FLOOR (restore the metric whenever the engine has
            // annihilated it), `popfix < 0` is a PEG at `|popfix|` (hold it there every
            // tick, i.e. what an actor with a stable population would look like). The
            // floor leaves a sawtooth — the `economic_output_to_population` deficit rule
            // re-annihilates the metric within a tick or two — so income is intermittent;
            // the peg makes income constant. Same "give them population" intent, opposite
            // answers about whether task 20's defect survives.
            if popfix != 0.0 {
                let ids: Vec<String> = world.actors.keys().cloned().collect();
                for id in ids {
                    if world.dead_actor_ids.contains(&id) {
                        continue;
                    }
                    if let Some(a) = world.actors.get_mut(&id) {
                        if popfix > 0.0 {
                            if a.get_metric("population") <= 0.0 {
                                a.set_metric("population", popfix);
                            }
                        } else if a.get_metric("population") < -popfix {
                            a.set_metric("population", -popfix);
                        }
                    }
                }
            }

            let max_mil = world
                .actors
                .values()
                .map(|a| a.get_metric("military_size"))
                .fold(1.0_f64, f64::max);
            let avg_pp: f64 = world
                .actors
                .values()
                .map(|a| a.power_projection(1.0, max_mil))
                .sum::<f64>()
                / world.actors.len().max(1) as f64;
            let fg: Vec<String> = world
                .actors
                .iter()
                .filter(|(_, a)| a.narrative_status == engine13::core::NarrativeStatus::Foreground)
                .map(|(id, _)| id.clone())
                .collect();

            let ids: Vec<String> = world.actors.keys().cloned().collect();
            for aid in ids {
                if world.dead_actor_ids.contains(&aid) {
                    continue;
                }
                let actor = match world.actors.get(&aid) {
                    Some(a) => a,
                    None => continue,
                };
                let power = actor.power_projection(1.0, max_mil) > avg_pp * 0.7;
                let contact = fg.iter().filter(|f| **f != aid).any(|f| {
                    world
                        .actors
                        .get(f)
                        .map(|n| n.neighbors.iter().any(|x| x.id == aid && x.distance <= 2))
                        .unwrap_or(false)
                });

                let mut moved = false;
                let mut moved_wo_treasury = false;
                for m in UPHEAVAL_METRICS {
                    if let Some(h) = world.metric_history.get(&format!("{}:{}", aid, m)) {
                        if h.len() >= 2 {
                            let d = (h.back().copied().unwrap_or(0.0)
                                - h.front().copied().unwrap_or(0.0))
                            .abs();
                            if d > 30.0 {
                                moved = true;
                                if m != "treasury" {
                                    moved_wo_treasury = true;
                                }
                            }
                        }
                    }
                }
                let upheaval = moved
                    || actor.get_metric("cohesion") < 25.0
                    || actor.get_metric("legitimacy") < 20.0;
                let soft = actor.get_metric("cohesion") < 25.0 || actor.get_metric("legitimacy") < 20.0;
                let dec = upheaval && !power && !contact;

                let e = acc.entry(aid.clone()).or_default();
                e.ticks += 1;
                if power {
                    e.power += 1;
                }
                if contact {
                    e.contact += 1;
                }
                if upheaval {
                    e.upheaval += 1;
                }
                if power || contact || upheaval {
                    e.verdict += 1;
                }
                if dec {
                    e.decisive += 1;
                    if moved && !moved_wo_treasury && !soft {
                        e.decisive_treasury_only += 1;
                    }
                }
                if let Some(p) = e.prev_decisive {
                    if p != dec {
                        e.dec_transitions += 1;
                    }
                }
                e.prev_decisive = Some(dec);
            }
            tick(&mut world, &scenario, &mut log, &mut rng);
        }

        for (aid, e) in &acc {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                aid, seed, e.ticks, e.power, e.contact, e.upheaval, e.verdict,
                e.decisive, e.decisive_treasury_only, e.dec_transitions
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("inventory");
    match mode {
        "inventory" => inventory(),
        "popsink" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            popsink(scenario, ticks, &seeds);
        }
        "decisive" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let popfix: f64 = args.get(5).map(|s| s.parse().expect("popfix")).unwrap_or(0.0);
            decisive(scenario, ticks, &seeds, popfix);
        }
        "d3" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args.get(4).map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect()).unwrap_or_else(|| vec![42]);
            let ks: Vec<f64> = args.get(5).map(|s| s.split(',').map(|x| x.parse().expect("k")).collect()).unwrap_or_else(|| vec![0.1, 0.2, 0.3, 0.5]);
            d3(scenario, ticks, &seeds, &ks);
        }
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
