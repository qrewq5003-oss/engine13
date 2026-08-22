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
    ActionCondition, ComparisonOperator, DependencyMode, DependencyRule, EventConditionType,
    MetricRef, RelativeMetricRef, Scenario, WorldState,
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

/// Task 22 needs the same walk for four metrics instead of one, so the filter that
/// used to be hard-coded to `treasury` became a parameter. The walk itself is unchanged
/// and still knows nothing about which metric is interesting — only the printing is
/// filtered, which is what keeps the census (printed in full) usable as the completeness
/// check for a hand-made enumeration.
fn inventory_for(filter: &str) {
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

    println!("\n# Сайты, относящиеся к {}", filter);
    for h in &hits {
        if h.key.contains(filter) {
            println!("HIT\t{}\t{}\t{}\t{}\t{}\t{}", h.scenario, h.container, h.site, h.key, h.role, h.detail);
        }
    }

    println!("\n# Итог по контейнерам (только {})", filter);
    let mut per: BTreeMap<(String, String, &str), u32> = BTreeMap::new();
    for h in &hits {
        if h.key.contains(filter) {
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

// ===========================================================================
// attractor mode — task 22 stage 1, steps 2 and 4
// ===========================================================================
//
// Two things no existing mode can express, which is why this one exists rather than a
// parameter on `popsink` (§4 of the task-22 statement demands the justification):
//
// 1. **Occupancy of a metric PAIR, not of one metric.** `popsink` follows `population`
//    only. Task 22 asks for the share of the run each actor spends in the absorbing
//    state, and for the second candidate cycle `cohesion↔legitimacy` — a pair no probe
//    mode has ever read. Occupancy also needs *returns* (exits from the state), which
//    `popsink` does not count.
// 2. **Scripted play.** Every probe mode drives `tick()` with no player. `sim`'s scripted
//    loop lives in a *binary* crate (`src/bin/sim.rs`), so it cannot be imported; and
//    `sim`'s own scripted output prints only the player actor's core metrics, never
//    `population` and never the other 24 actors. The loop below therefore replicates it
//    from the same library entry points sim uses (`apply_player_action`, `AppState`) with
//    the same priority lists and the same Milan reserve discipline, and reports enough
//    aggregates (`victory`, `raise_troops`) to be cross-checked against `sim` — the
//    discipline task 19 imposed on any replication.
//
// Read-only with respect to the engine: no engine symbol is modified, actions go through
// the same application-layer path the UI uses.
const CONST_BALANCED: &[&str] = &[
    "venice_diplomacy", "genoa_financial_aid", "milan_bankers", "venice_naval_support",
    "genoa_mercenaries", "milan_condottieri", "venice_trade_deal", "genoa_galata_garrison",
];
const CONST_DIPLOMACY: &[&str] = &[
    "venice_diplomacy", "genoa_financial_aid", "milan_bankers", "venice_trade_deal",
    "genoa_galata_garrison", "venice_naval_support", "genoa_mercenaries", "milan_condottieri",
];
const CONST_MILITARY: &[&str] = &[
    "venice_naval_support", "genoa_mercenaries", "milan_condottieri", "genoa_galata_garrison",
    "venice_diplomacy", "genoa_financial_aid", "milan_bankers", "venice_trade_deal",
];
const ROME_BALANCED: &[&str] = &[
    "expand_network", "build_reputation", "support_city", "back_administration",
    "fund_defense", "lay_low", "invest_wealth", "gather_information", "educate_family",
];
const ROME_WEALTH: &[&str] = &[
    "lay_low", "invest_wealth", "gather_information", "expand_network", "educate_family",
    "support_city", "back_administration", "build_reputation", "fund_defense",
];
const MILAN_AGGRESSIVE: &[&str] = &[
    "milan_raise_troops", "milan_pressure_genoa", "incite_baronial_revolt",
    "milan_hire_condottieri", "milan_hire_urbino_condottieri", "milan_lease_genoese_fleet",
    "milan_banking_deal_florence", "milan_bribe_curia", "milan_court_patronage",
    "milan_diplomacy_ferrara", "milan_marriage_venice", "milan_marriage_naples",
    "call_papal_arbitration", "milan_savoy_alliance",
];

/// Price the population-writing dependency rules of a scenario on one actor's state,
/// using whatever mode and coefficients that scenario actually carries.
///
/// Read from the loaded `Scenario` rather than from constants mirrored in this file, so
/// the flow decomposition is valid for the absolute rule and the normalized one alike —
/// which is the only way a before/after comparison of "what else moves `population`"
/// means anything. Mirrors `engine::apply_dependency_rule`, including the sequential
/// semantics: the second rule sees the stock the first one already reduced.
fn price_population_rules(rules: &[DependencyRule], pop: f64, eo: f64) -> f64 {
    let mut stock = pop;
    for rule in rules.iter().filter(|r| r.to.as_str() == "population") {
        let from_val = if rule.from.as_str() == "economic_output" { eo } else { continue };
        let d = match rule.threshold {
            Some(t) if from_val < t => (t - from_val, t),
            _ => continue,
        };
        let delta = match rule.mode {
            DependencyMode::Deficit => -(d.0 * rule.coefficient),
            DependencyMode::DeficitProportional => -(stock * rule.coefficient * d.0 / d.1),
            _ => continue,
        };
        // the engine clamps `population` to `0..MAX` once per tick, so a rule that
        // overshoots the stock cannot charge more than the stock — pricing it without
        // the clamp would credit the absolute rule with losses it never took
        stock = (stock + delta).max(0.0);
    }
    pop - stock
}

fn priority_list(scenario_id: &str, strategy: &str) -> &'static [&'static str] {
    match (scenario_id, strategy) {
        ("rome_375", "wealth") => ROME_WEALTH,
        ("rome_375", _) => ROME_BALANCED,
        ("milan_1477", _) => MILAN_AGGRESSIVE,
        (_, "diplomacy") => CONST_DIPLOMACY,
        (_, "military") => CONST_MILITARY,
        (_, _) => CONST_BALANCED,
    }
}

/// The scripted-player step of `sim::run_scripted`, lifted verbatim out of
/// [`attractor`] so that it and [`popevents`] cannot drift apart in the one place a
/// probe is easiest to get subtly wrong. Returns the actions applied this tick and
/// how many of them were `milan_raise_troops` — the two counters both modes
/// cross-check against `sim`.
fn scripted_step(
    state: &mut engine13::commands::AppState,
    scenario_id: &str,
    strategy: Option<&str>,
) -> (u32, u32) {
    use engine13::application::actions::{apply_player_action, PlayerActionInput};

    let strat = match strategy {
        Some(s) => s,
        None => return (0, 0),
    };
    let mut raise_troops = 0u32;
    let list = priority_list(scenario_id, strat);
    let per_tick = state.current_scenario.as_ref().unwrap().actions_per_tick;
    let mut applied = 0u32;
    if scenario_id == "milan_1477" {
        const RAISE_TROOPS_GATE: f64 = 70.0;
        let treasury_before = state
            .world_state
            .as_ref()
            .unwrap()
            .actors
            .get("milan")
            .map(|a| a.get_metric("treasury"))
            .unwrap_or(0.0);
        if treasury_before > RAISE_TROOPS_GATE {
            let input = PlayerActionInput {
                action_id: "milan_raise_troops".to_string(),
                target_actor_id: None,
            };
            if apply_player_action(state, &input).is_ok() {
                applied += 1;
                raise_troops += 1;
            }
        }
        for action_id in list.iter().filter(|id| **id != "milan_raise_troops") {
            if applied >= per_tick {
                break;
            }
            let treasury_now = state
                .world_state
                .as_ref()
                .unwrap()
                .actors
                .get("milan")
                .map(|a| a.get_metric("treasury"))
                .unwrap_or(0.0);
            let surplus = treasury_now - RAISE_TROOPS_GATE;
            if surplus <= 0.0 {
                break;
            }
            let cost = state
                .current_scenario
                .as_ref()
                .unwrap()
                .patron_actions
                .iter()
                .find(|a| a.id == *action_id)
                .and_then(|a| a.cost.get(&MetricRef::literal("actor:milan.treasury")))
                .map(|c| -c)
                .unwrap_or(f64::MAX);
            if cost > surplus {
                continue;
            }
            let input = PlayerActionInput {
                action_id: action_id.to_string(),
                target_actor_id: None,
            };
            if apply_player_action(state, &input).is_ok() {
                applied += 1;
            }
        }
    } else {
        for action_id in list.iter() {
            if applied >= per_tick {
                break;
            }
            let input = PlayerActionInput {
                action_id: action_id.to_string(),
                target_actor_id: None,
            };
            if apply_player_action(state, &input).is_ok() {
                applied += 1;
            }
        }
    }
    (applied, raise_troops)
}

#[derive(Default)]
struct AttrAcc {
    ticks: u32,
    pop_first: f64,
    eo_first: f64,
    seen: bool,
    pop_zero: u32,
    pop_zero_first: i64,
    pop_returns: u32,
    prev_pop_zero: Option<bool>,
    eo_below_50: u32,
    eo_zero: u32,
    eo_recover_at: i64,
    both_zero: u32,
    income_zero: u32,
    mutual: u32,
    mutual_first: i64,
    mutual_returns: u32,
    prev_mutual: Option<bool>,
    coh_lt25: u32,
    coh_lt15: u32,
    cl_both_zero: u32,
    tag_eo: i32,
    rank: String,
    // --- stage 2 (task 22): scale-aware occupancy ------------------------------
    //
    // `pop_zero` counts `population == 0.0` exactly, which is the right measure for
    // the *absolute* sink: it overshoots the stock and the `0..MAX` clamp lands the
    // actor precisely on zero. Under a sink normalized by the stock the same measure
    // becomes vacuous by construction — geometric decay never reaches zero — so a
    // variant (D) would "pass" the occupancy criterion while leaving actors at
    // `population = 1e-9`, i.e. still economically dead (income is
    // `eo · pop · 0.001`, `mod.rs:647`). These three columns are what makes the
    // criterion honest across both forms of the rule:
    //   `pop_lt1`   — absolute floor; the smallest starting population in the project
    //                 is 15 (`urbino`), so below 1 is dead on any reading;
    //   `pop_lt10`  — relative floor, scale-free: below 10 % of the actor's own start;
    //   `pop_final` — where the trajectory actually ended.
    pop_lt1: u32,
    pop_lt10pct: u32,
    pop_last: f64,
    // --- stage 2 (task 22): the integrated deficit, which is what (D) is priced on ---
    //
    // Under the normalized form the per-tick loss fraction is `k₁·d₁ + k₂·d₂` with
    // `d₁ = max(0, 50−eo)/50` and `d₂ = max(0, 15−eo)/15`, so the retained share over a
    // whole run is `Π(1−k₁d₁ₜ)(1−k₂d₂ₜ)`. Summing `d₁`/`d₂` along the *baseline*
    // trajectory therefore prices any candidate `k` **without running the modified
    // engine** — the "equilibrium calculation before code" the statement demands, and
    // the same discipline §4.1 used to close variant (A).
    //
    // It is a prediction, not an identity: `population` feeds `economic_output` back
    // through `population_to_economic_output` (threshold 3000 / 500), so for an actor
    // whose population crosses that threshold the baseline `eo` path is not the path
    // under (D). For every actor in the attractor the threshold is far out of reach,
    // which is why the prediction is worth making — and it is checked against the run.
    d1_sum: f64,
    d2_sum: f64,
    // --- stage 2 (task 22): who actually moves `population` -----------------------
    //
    // The integrated deficit prices the *dependency rule*. It does not price anything
    // else, and stage 1 §2.2 found four other channels (migration `-1 %` source /
    // `+0.5 %` receiver, the successor split, one rome `auto_delta`, three shared
    // random events). Comparing the rule's own loss against the actual per-tick change
    // separates them without touching the engine: `other_flow` is everything the rule
    // did not do. Approximate by construction — the observation is taken at the top of
    // the tick, so the rule is priced on the population it sees there rather than on
    // the value it sees mid-tick, and every non-rule channel is lumped together.
    rule_loss: f64,
    other_flow: f64,
    prev_pop: Option<f64>,
    pending_rule_loss: f64,
}

fn attractor(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    println!("actor\tseed\tmode\tticks\tpop0\teo0\trank\ttag_eo\tpop0%\tpop_zero_at\tpop_ret\teo<50%\teo=0%\teo_rec_at\tboth0%\tinc0%\tmutual%\tmut_at\tmut_ret\tcoh<25%\tcoh<15%\tcl00%\tpop<1%\tpop<10%p0\tpopF\td1sum\td2sum\trule_loss\tother_flow");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        // Same init as `sim::run_scripted`, for both modes, so that the only difference
        // between them is the presence of player actions.
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        // priced from the loaded scenario, so the decomposition is valid in both worlds
        let pop_rules: Vec<DependencyRule> = state
            .current_scenario
            .as_ref()
            .unwrap()
            .dependencies
            .clone();
        let mut acc: BTreeMap<String, AttrAcc> = BTreeMap::new();
        let mut raise_troops = 0u32;
        let mut applied_total = 0u32;
        let mut victory_tick: i64 = -1;
        // --- stage 2 (task 22): the three ratified guards, measured in the same run ---
        // `sim` stops a run on victory *or* on the protagonist's death; this probe runs
        // to the horizon, so a death recorded after the victory tick is not the death
        // `sim`'s 8/30 counted. Both the tick and the victory tick are therefore printed
        // and the "death" statistic is computed as death-before-victory downstream.
        let protagonist = match scenario_id {
            "constantinople_1430" => "byzantium",
            "rome_375" => "rome",
            _ => "milan",
        };
        let mut prot_dead_tick: i64 = -1;
        let mut italy_unified: i64 = -1;

        for t in 0..ticks {
            // --- observation, before actions and before the tick ---------------
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let pop = a.get_metric("population");
                    let eo = a.get_metric("economic_output");
                    let coh = a.get_metric("cohesion");
                    let leg = a.get_metric("legitimacy");
                    let income = eo * pop * 0.001;
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.pop_first = pop;
                        e.eo_first = eo;
                        e.pop_zero_first = -1;
                        e.mutual_first = -1;
                        e.eo_recover_at = -1;
                        e.rank = format!("{:?}", a.region_rank);
                        e.tag_eo = a
                            .actor_tags
                            .values()
                            .filter_map(|t| {
                                t.metrics_modifier
                                    .iter()
                                    .find(|(k, _)| k.as_str() == "economic_output")
                                    .map(|(_, v)| *v)
                            })
                            .sum();
                    }
                    e.ticks += 1;
                    let pz = pop == 0.0;
                    if pz {
                        e.pop_zero += 1;
                        if e.pop_zero_first < 0 {
                            e.pop_zero_first = t as i64;
                        }
                    }
                    if let Some(p) = e.prev_pop_zero {
                        if p && !pz {
                            e.pop_returns += 1;
                        }
                    }
                    e.prev_pop_zero = Some(pz);
                    if pop < 1.0 {
                        e.pop_lt1 += 1;
                    }
                    if pop < 0.1 * e.pop_first {
                        e.pop_lt10pct += 1;
                    }
                    e.pop_last = pop;
                    let d1 = (50.0 - eo).max(0.0) / 50.0;
                    let d2 = (15.0 - eo).max(0.0) / 15.0;
                    e.d1_sum += d1;
                    e.d2_sum += d2;
                    if let Some(prev) = e.prev_pop {
                        // charged against the *previous* tick, whose eo produced it
                        let charged = e.pending_rule_loss;
                        e.rule_loss += charged;
                        e.other_flow += (pop - prev) + charged;
                    }
                    e.prev_pop = Some(pop);
                    let _ = (d1, d2);
                    e.pending_rule_loss = price_population_rules(&pop_rules, pop, eo);
                    if eo < 50.0 {
                        e.eo_below_50 += 1;
                    } else if e.eo_below_50 > 0 && e.eo_recover_at < 0 {
                        e.eo_recover_at = t as i64;
                    }
                    if eo == 0.0 {
                        e.eo_zero += 1;
                    }
                    if pz && eo == 0.0 {
                        e.both_zero += 1;
                    }
                    if income == 0.0 {
                        e.income_zero += 1;
                    }
                    // second cycle: both edges of cohesion<->legitimacy are `deficit`
                    // below 50, so the mutually-negative region is coh<50 AND leg<50
                    let mu = coh < 50.0 && leg < 50.0;
                    if mu {
                        e.mutual += 1;
                        if e.mutual_first < 0 {
                            e.mutual_first = t as i64;
                        }
                    }
                    if let Some(p) = e.prev_mutual {
                        if p && !mu {
                            e.mutual_returns += 1;
                        }
                    }
                    e.prev_mutual = Some(mu);
                    if coh < 25.0 {
                        e.coh_lt25 += 1;
                    }
                    if coh < 15.0 {
                        e.coh_lt15 += 1;
                    }
                    if coh == 0.0 && leg == 0.0 {
                        e.cl_both_zero += 1;
                    }
                }
            }

            // --- player actions, replicating sim::run_scripted -----------------
            {
                let (applied, rt) = scripted_step(&mut state, scenario_id, strategy);
                applied_total += applied;
                raise_troops += rt;
            }

            let world_state = state.world_state.as_mut().unwrap();
            let scenario_ref = state.current_scenario.as_ref().unwrap();
            let rng = state.rng.as_mut().unwrap();
            tick(world_state, scenario_ref, &mut state.event_log, rng);
            // `sim` stops the run on victory; this probe keeps observing, so the
            // victory tick is recorded instead of used as a stopping rule (the
            // difference is named in the document, not hidden).
            if victory_tick < 0 && state.world_state.as_ref().unwrap().victory_achieved {
                victory_tick = (t + 1) as i64;
            }
            if prot_dead_tick < 0
                && state
                    .world_state
                    .as_ref()
                    .unwrap()
                    .dead_actor_ids
                    .iter()
                    .any(|id| id == protagonist)
            {
                prot_dead_tick = (t + 1) as i64;
            }
            if italy_unified < 0
                && state
                    .event_log
                    .events
                    .iter()
                    .any(|e| e.id == "italy_unified")
            {
                italy_unified = (t + 1) as i64;
            }
        }

        let mode = strategy.unwrap_or("noplayer");
        for (aid, e) in &acc {
            let n = e.ticks as f64;
            println!(
                "{}\t{}\t{}\t{}\t{:.0}\t{:.1}\t{}\t{:+}\t{:.1}\t{}\t{}\t{:.1}\t{:.1}\t{}\t{:.1}\t{:.1}\t{:.1}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.3}\t{:.3}\t{:.3}\t{:.1}\t{:+.1}",
                aid, seed, mode, e.ticks, e.pop_first, e.eo_first, e.rank, e.tag_eo,
                100.0 * e.pop_zero as f64 / n, e.pop_zero_first, e.pop_returns,
                100.0 * e.eo_below_50 as f64 / n, 100.0 * e.eo_zero as f64 / n, e.eo_recover_at,
                100.0 * e.both_zero as f64 / n, 100.0 * e.income_zero as f64 / n,
                100.0 * e.mutual as f64 / n, e.mutual_first, e.mutual_returns,
                100.0 * e.coh_lt25 as f64 / n, 100.0 * e.coh_lt15 as f64 / n,
                100.0 * e.cl_both_zero as f64 / n,
                100.0 * e.pop_lt1 as f64 / n, 100.0 * e.pop_lt10pct as f64 / n, e.pop_last,
                e.d1_sum, e.d2_sum, e.rule_loss, e.other_flow
            );
        }
        let world = state.world_state.as_ref().unwrap();
        let generations = world
            .family_state
            .as_ref()
            .map(|f| f.generation_count)
            .unwrap_or(0);
        println!(
            "#XCHECK\tseed={}\tmode={}\tvictory={}\tvictory_tick={}\ttick={}\tactions={}\traise_troops={}\tprot={}\tprot_dead_tick={}\tgenerations={}\titaly_unified={}",
            seed, mode, world.victory_achieved, victory_tick, world.tick, applied_total,
            raise_troops, protagonist, prot_dead_tick, generations, italy_unified
        );
    }
}

// ============================================================================
// Task 23: `popevents` — the absolute `population` deltas in the shared random
// event pool
// ============================================================================

/// The three shared events that write `population`, with their nominal deltas.
/// Named here rather than read off `common_events()` on purpose: the point of the
/// mode is to compare what the content *declares* against what the run *does*, so
/// the declared side has to be stated independently. `assert_pool_matches_source`
/// below checks the two against each other, so a change to `common.rs` breaks the
/// probe loudly instead of silently re-deriving itself.
const POP_EVENTS: &[(&str, f64)] = &[("plague", -25.0), ("famine", -20.0), ("flood", -15.0)];

/// Fail loudly if `common_events()` stops matching [`POP_EVENTS`] — either because a
/// delta changed, or because a fourth shared event started writing `population`.
fn assert_pool_matches_source() {
    let key = engine13::core::RelativeMetricRef::literal("self.population");
    let mut found: Vec<(String, f64)> = Vec::new();
    for ev in engine13::events::common_events() {
        if let Some(d) = ev.effects.get(&key) {
            found.push((ev.id.clone(), *d));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let mut declared: Vec<(String, f64)> =
        POP_EVENTS.iter().map(|(i, d)| (i.to_string(), *d)).collect();
    declared.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        found, declared,
        "common_events() no longer matches POP_EVENTS — update the probe, not the run"
    );
}

#[derive(Default)]
struct PopEvAcc {
    ticks: u32,
    seen: bool,
    pop_first: f64,
    pop_last: f64,
    rank: String,
    // eligibility: `EventTarget::Any` draws only from `foreground_ids`
    // (`engine/mod.rs:426`), so an actor that is never Foreground can never be hit,
    // however true its gates are.
    fg: u32,
    // gate occupancy — the question the statement's §1 п.5 addition asks: is a gate
    // that *looks* conditional in fact a constant along the whole run?
    coh_lt60: u32,
    eo_lt30: u32,
    pop_gt500: u32,
    plague_gate: u32, // pop > 500 AND coh < 60, i.e. the conjunction actually tested
    // fires, counted off the event log — exact, not inferred: `phase_random_events`
    // records one `Event` per successful application, after the effects are applied.
    n_plague: u32,
    n_famine: u32,
    n_flood: u32,
    ev_nominal: f64, // Σ |delta| over fires; an upper bound on what was applied
    // ticks where the clamp could have bitten: the stock at the top of the tick was
    // smaller than the nominal sink, so `(current + delta).max(0.0)` truncated it.
    // Counted rather than corrected, so the size of the approximation is visible.
    clip_ticks: u32,
    // the rule's own price, on the same footing as task 22 §12.5, so that "which
    // channel dominates" is a comparison and not an assertion
    rule_loss: f64,
    other_flow: f64,
    prev_pop: Option<f64>,
    pending_rule_loss: f64,
    // содержательность: ticks where population fell and the events took more of it
    // than the dependency rule did
    down_ticks: u32,
    ev_dom_ticks: u32,
    rule_dom_ticks: u32,
}

/// Task 23 stage 1. Read-only: drives `tick()`, reads metrics and the event log.
///
/// Three things the existing modes structurally cannot report:
///   1. **fires per event per actor**, taken off the event log rather than inferred
///      from a population drop — `attractor`'s `other_flow` lumps the three events
///      together with migration, the successor split and the rome `auto_delta`;
///   2. **gate occupancy** (`coh < 60`, `eo < 30`, `pop > 500`, and the `plague`
///      conjunction), which is what tells a real condition from a constant;
///   3. **Foreground occupancy**, the eligibility gate that sits in front of all
///      three events and appears in no scenario file.
fn popevents(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    assert_pool_matches_source();

    println!("actor\tseed\tmode\tticks\tpop0\tpopF\trank\tfg%\tcoh<60%\teo<30%\tpop>500%\tpl_gate%\tpl_n\tfa_n\tfl_n\tev_nom\tclip\trule_loss\tother_flow\tdown\tev_dom\trule_dom");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        let pop_rules: Vec<DependencyRule> = state
            .current_scenario
            .as_ref()
            .unwrap()
            .dependencies
            .clone();
        let cap = state
            .current_scenario
            .as_ref()
            .unwrap()
            .max_random_events_per_tick;
        let pool_size = engine13::events::common_events().len()
            + state.current_scenario.as_ref().unwrap().random_events.len();

        let mut acc: BTreeMap<String, PopEvAcc> = BTreeMap::new();
        let mut fires_by_id: BTreeMap<String, u32> = BTreeMap::new();
        let mut raise_troops = 0u32;
        let mut victory_tick: i64 = -1;
        let mut fg_total: u64 = 0;
        let mut at_cap_ticks: u32 = 0;
        let mut random_fires_total: u32 = 0;

        // ids of everything the random-event phase can log, so that the per-tick
        // fire count is taken over that pool only and not over milestones, rank
        // changes, promotions and the rest of the log
        let random_ids: std::collections::HashSet<String> = engine13::events::common_events()
            .iter()
            .map(|e| e.id.clone())
            .chain(
                state
                    .current_scenario
                    .as_ref()
                    .unwrap()
                    .random_events
                    .iter()
                    .map(|e| e.id.clone()),
            )
            .collect();

        for t in 0..ticks {
            // --- observation, top of tick, before actions ----------------------
            let mut pop_before: BTreeMap<String, f64> = BTreeMap::new();
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let pop = a.get_metric("population");
                    let eo = a.get_metric("economic_output");
                    let coh = a.get_metric("cohesion");
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.pop_first = pop;
                        e.rank = format!("{:?}", a.region_rank);
                    }
                    e.ticks += 1;
                    if a.narrative_status == engine13::core::NarrativeStatus::Foreground {
                        e.fg += 1;
                        fg_total += 1;
                    }
                    if coh < 60.0 {
                        e.coh_lt60 += 1;
                    }
                    if eo < 30.0 {
                        e.eo_lt30 += 1;
                    }
                    if pop > 500.0 {
                        e.pop_gt500 += 1;
                    }
                    if pop > 500.0 && coh < 60.0 {
                        e.plague_gate += 1;
                    }
                    e.pop_last = pop;
                    // charge the previous tick's rule price against the change that
                    // tick produced — same convention as `attractor`, so the two
                    // decompositions are comparable
                    if let Some(prev) = e.prev_pop {
                        let charged = e.pending_rule_loss;
                        e.rule_loss += charged;
                        e.other_flow += (pop - prev) + charged;
                    }
                    e.prev_pop = Some(pop);
                    e.pending_rule_loss = price_population_rules(&pop_rules, pop, eo);
                    pop_before.insert(aid.clone(), pop);
                }
            }

            let (_applied, rt) = scripted_step(&mut state, scenario_id, strategy);
            raise_troops += rt;

            let log_len = state.event_log.events.len();
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }
            if victory_tick < 0 && state.world_state.as_ref().unwrap().victory_achieved {
                victory_tick = (t + 1) as i64;
            }

            // --- attribution, off the log --------------------------------------
            let mut fired_here = 0u32;
            let mut ev_this_tick: BTreeMap<String, f64> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                if random_ids.contains(&ev.id) {
                    fired_here += 1;
                    *fires_by_id.entry(ev.id.clone()).or_insert(0) += 1;
                }
                if let Some((_, delta)) = POP_EVENTS.iter().find(|(id, _)| *id == ev.id) {
                    let e = acc.entry(ev.actor_id.clone()).or_default();
                    match ev.id.as_str() {
                        "plague" => e.n_plague += 1,
                        "famine" => e.n_famine += 1,
                        _ => e.n_flood += 1,
                    }
                    e.ev_nominal += -delta;
                    *ev_this_tick.entry(ev.actor_id.clone()).or_insert(0.0) += -delta;
                }
            }
            random_fires_total += fired_here;
            if cap > 0 && fired_here >= cap {
                at_cap_ticks += 1;
            }

            // --- содержательность: which channel took more, this tick -----------
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, before) in &pop_before {
                    let after = match world.actors.get(aid) {
                        Some(a) => a.get_metric("population"),
                        None => continue,
                    };
                    let e = match acc.get_mut(aid) {
                        Some(e) => e,
                        None => continue,
                    };
                    let ev_nom = ev_this_tick.get(aid).copied().unwrap_or(0.0);
                    if ev_nom > *before {
                        e.clip_ticks += 1;
                    }
                    if after < *before {
                        e.down_ticks += 1;
                        let rule = e.pending_rule_loss;
                        if ev_nom > 0.0 && ev_nom > rule {
                            e.ev_dom_ticks += 1;
                        } else if rule > 0.0 && rule >= ev_nom {
                            e.rule_dom_ticks += 1;
                        }
                    }
                }
            }
        }

        let mode = strategy.unwrap_or("noplayer");
        for (aid, e) in &acc {
            let n = e.ticks.max(1) as f64;
            println!(
                "{}\t{}\t{}\t{}\t{:.0}\t{:.1}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{}\t{}\t{}\t{:.1}\t{}\t{:.1}\t{:+.1}\t{}\t{}\t{}",
                aid, seed, mode, e.ticks, e.pop_first, e.pop_last, e.rank,
                100.0 * e.fg as f64 / n,
                100.0 * e.coh_lt60 as f64 / n,
                100.0 * e.eo_lt30 as f64 / n,
                100.0 * e.pop_gt500 as f64 / n,
                100.0 * e.plague_gate as f64 / n,
                e.n_plague, e.n_famine, e.n_flood, e.ev_nominal, e.clip_ticks,
                e.rule_loss, e.other_flow, e.down_ticks, e.ev_dom_ticks, e.rule_dom_ticks
            );
        }
        let live = acc.values().filter(|e| e.ticks > 0).count().max(1);
        let pool: Vec<String> = fires_by_id
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        println!(
            "#POOL\tseed={}\tmode={}\tpool_size={}\tcap={}\tfires={}\tat_cap_ticks={}\tmean_fg={:.2}\tvictory_tick={}\traise_troops={}\t{}",
            seed, mode, pool_size, cap, random_fires_total, at_cap_ticks,
            fg_total as f64 / ticks as f64, victory_tick, raise_troops,
            pool.join(" ")
        );
        let _ = live;
    }
}

// ============================================================================
// Task 23 stage 1b: `decisive23` — is `flood` ever DECISIVE, method of task 19 п.4
// ============================================================================
//
// Task 19 п.4 did not run a second simulation. It recomputed the *predicate* the
// metric feeds, tick by tick, in a second saturated world, and counted the ticks on
// which the **verdict** differed (`cf_divergent`). That is what makes "0" mean "no
// effect" rather than "small effect": a second simulation would diverge through the
// RNG stream (`interactions.rs:438` spends four extra draws only on a successful
// combat roll, and that roll's probability is computed from metrics), so any
// difference it showed would be uninterpretable.
//
// Applied here: the shadow world is the one in which `flood` never removed any
// population. Per actor it is tracked as a *debt* — the population `flood` took,
// carried forward — and the debt is fed into exactly the predicates `population`
// can reach:
//
//   1. COLLAPSE — `check_collapses` (`mod.rs:1488–1495`) reads `legitimacy`,
//      `cohesion` and `external_pressure` and nothing else, and it is the only
//      writer of `dead_actor_ids` (`mod.rs:1588`). `population` cannot reach an
//      actor's death in one step. The only indirect routes run through relevance
//      (a Foreground actor can be drawn as an `EventTarget::Any` victim of events
//      that do write cohesion), so measuring (2) closes (1) as well.
//
//   2. RELEVANCE — `check_actor_upheaval` (`mod.rs:1199`) reads `population`
//      directly and trips on `|back − front| > 30` over the metric-history window.
//      The shadow verdict adds back the `flood` losses that fall inside the same
//      window. This is EXACT, not first-order: the history buffer is written in
//      phase 8 and read in phase 6, so the buffer observed at the top of a tick is
//      precisely the one that tick's phase 6 will read.
//
//   3. VICTORY — no scenario's victory condition reads `population`
//      (constantinople: `global:federation_progress ≥ 80` ∧
//      `actor:ottomans.military_size < 40`; rome: `family:influence ≥ 90`; milan:
//      none). The reachable route is `population → income (eo·pop·0.001,
//      mod.rs:672) → treasury → action availability → federation_progress`. So the
//      shadow carries a treasury debt too, and every `available_if` gate that reads
//      a `treasury` is evaluated in both worlds. Rome has **no** such gate — all
//      nine of its gates read `family:*` — so for rome this channel is closed by
//      construction, not by measurement.
//
// The treasury half is first-order: it credits back the income the missing
// population would have earned, but not the second-order compounding (the restored
// population would itself have been taxed by the proportional dependency rule and
// by migration, and a richer treasury would have bought actions that change `eo`).
// The direction is deliberate — the debt is accumulated from the NOMINAL `-15`,
// i.e. the upper bound of what `flood` actually applied, so the counterfactual
// over-states `flood`'s influence. A zero under an over-stated counterfactual is
// the strong form of the result.

#[derive(Default)]
struct CfAcc {
    ticks: u32,
    seen: bool,
    rank: String,
    // debt: population `flood` removed, carried forward (nominal = upper bound)
    flood_debt: f64,
    all3_debt: f64,
    // per-tick removals, aligned with `metric_history`'s 5-slot window
    flood_hist: std::collections::VecDeque<f64>,
    // foregone income: Σ eo · debt · 0.001
    treas_debt: f64,
    all3_treas_debt: f64,
    // --- outcome class 2: relevance -----------------------------------------
    up_pop_div: u32,   // the `population` sub-predicate alone flips
    up_div: u32,       // the whole 8-metric predicate flips (the real verdict)
    up_div_masked: u32,// ...of which: `coh < 25 || leg < 20` was already true anyway
    up_div_live: u32,  // ...of which: nothing else made the verdict true — an UPPER
                       //    bound on relevance decisions `flood` could have moved
    up_true: u32,      // for context: how often the real predicate is true at all
    // demotion does NOT go through `condition_upheaval`: `update_metric_history`
    // (mod.rs:1252) drives `actor_upheaval_ticks` from `check_actor_upheaval`
    // ALONE, without the `coh < 25 || leg < 20` disjuncts. So masking does not
    // apply on that path and the counter needs its own shadow.
    up_ticks_real: u32,
    up_ticks_shadow: u32,
    recent_div: u32,   // ticks where `counter < 10` — the demotion input — differs
    // --- outcome class 3: victory -------------------------------------------
    gate_div: u32,     // action-availability gate verdicts that flip
    gate_evals: u32,
    // --- outcome class 1: collapse ------------------------------------------
    died_tick: i64,
    fg_flips: u32,
    prev_fg: Option<bool>,
}

/// Verdict of `check_actor_upheaval` for one actor, optionally with `population`'s
/// window delta shifted by `pop_shift` (the shadow world). Mirrors `mod.rs:1199`
/// exactly, including the `history.len() >= 2` guard and the metric list.
fn upheaval_verdict(
    world: &WorldState,
    actor_id: &str,
    pop_shift: f64,
) -> (bool, bool) {
    const METRICS: [&str; 8] = [
        "population", "military_size", "military_quality", "economic_output",
        "cohesion", "legitimacy", "external_pressure", "treasury",
    ];
    let mut any = false;
    let mut pop_only = false;
    for m in METRICS {
        let key = format!("{}:{}", actor_id, m);
        if let Some(h) = world.metric_history.get(&key) {
            if h.len() >= 2 {
                let oldest = h.front().copied().unwrap_or(0.0);
                let newest = h.back().copied().unwrap_or(0.0);
                let mut d = newest - oldest;
                if m == "population" {
                    d += pop_shift;
                    pop_only = d.abs() > 30.0;
                }
                if d.abs() > 30.0 {
                    any = true;
                }
            }
        }
    }
    (any, pop_only)
}

fn decisive23(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    assert_pool_matches_source();

    println!("actor\tseed\tmode\tticks\trank\tflood_debt\ttreas_debt\tup_true\tup_pop_div\tup_div\tup_masked\tup_live\tgate_evals\tgate_div\tfg_flips\tdied\trecent_div");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        // Every action-availability gate that reads a `treasury`, taken off the
        // loaded scenario rather than hardcoded: (owner actor, operator, value, id).
        //
        // Restricted to actions the scripted player actually ATTEMPTS. A gate on an
        // action outside the priority list is never evaluated by the run, so a
        // divergence in it cannot reach any outcome — counting it would inflate the
        // counterfactual with verdicts nobody reads. In `noplayer` mode no action is
        // attempted at all, so this whole channel is closed by construction and the
        // gate list is empty.
        let mut treasury_gates: Vec<(String, ComparisonOperator, f64, String)> = Vec::new();
        if let Some(strat) = strategy {
            let attempted = priority_list(scenario_id, strat);
            let sc = state.current_scenario.as_ref().unwrap();
            for act in sc.patron_actions.iter().chain(sc.universal_actions.iter()) {
                if !attempted.contains(&act.id.as_str()) {
                    continue;
                }
                if let engine13::core::ActionCondition::Metric {
                    metric: MetricRef::Actor { actor_id, metric: m },
                    operator,
                    value,
                } = &act.available_if
                {
                    if m.as_str() == "treasury" {
                        treasury_gates.push((
                            actor_id.as_str().to_string(),
                            operator.clone(),
                            *value,
                            act.id.clone(),
                        ));
                    }
                }
            }
        }
        // Victory reachability: how close the run ever came to the condition, so a
        // non-victory can be told apart from a near-miss without re-running it.
        let (vic_thresh, vic_has) = match state.current_scenario.as_ref().unwrap().victory_condition {
            Some(ref vc) => (vc.threshold, true),
            None => (0.0, false),
        };
        let mut vic_metric_max = f64::NEG_INFINITY;
        let mut vic_add_ok = 0u32;

        let mut acc: BTreeMap<String, CfAcc> = BTreeMap::new();
        let mut victory_tick: i64 = -1;
        let mut collapses = 0u32;

        for t in 0..ticks {
            // ---- verdicts, taken where the engine takes them -------------------
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.rank = format!("{:?}", a.region_rank);
                        e.died_tick = -1;
                    }
                    e.ticks += 1;

                    // relevance: `back − front` misses the oldest slot's own delta,
                    // so the window sum skips the first entry, exactly as the
                    // history buffer does
                    let win: f64 = e.flood_hist.iter().skip(1).sum();
                    let (real, _) = upheaval_verdict(world, aid, 0.0);
                    let (shadow, shadow_pop_only) = upheaval_verdict(world, aid, win);
                    let (_, real_pop_only) = upheaval_verdict(world, aid, 0.0);
                    if real {
                        e.up_true += 1;
                    }
                    if real_pop_only != shadow_pop_only {
                        e.up_pop_div += 1;
                    }
                    if real != shadow {
                        e.up_div += 1;
                        let masked = a.get_metric("cohesion") < 25.0
                            || a.get_metric("legitimacy") < 20.0;
                        if masked {
                            e.up_div_masked += 1;
                        } else {
                            e.up_div_live += 1;
                        }
                    }
                    // demotion path: two counters, driven by the bare predicate
                    if real { e.up_ticks_real = 0; } else { e.up_ticks_real += 1; }
                    if shadow { e.up_ticks_shadow = 0; } else { e.up_ticks_shadow += 1; }
                    if (e.up_ticks_real < 10) != (e.up_ticks_shadow < 10) {
                        e.recent_div += 1;
                    }

                    let fg = a.narrative_status
                        == engine13::core::NarrativeStatus::Foreground;
                    if let Some(p) = e.prev_fg {
                        if p != fg {
                            e.fg_flips += 1;
                        }
                    }
                    e.prev_fg = Some(fg);
                }

                // victory channel: availability gates in both worlds, evaluated at
                // the same point `apply_player_action` evaluates them
                for (owner, op, value, _id) in &treasury_gates {
                    if world.dead_actor_ids.contains(owner) {
                        continue;
                    }
                    let tr = match world.actors.get(owner) {
                        Some(a) => a.get_metric("treasury"),
                        None => continue,
                    };
                    let debt = acc.get(owner).map(|e| e.treas_debt).unwrap_or(0.0);
                    let e = acc.entry(owner.clone()).or_default();
                    e.gate_evals += 1;
                    if op.evaluate(tr, *value) != op.evaluate(tr + debt, *value) {
                        e.gate_div += 1;
                    }
                }

                if vic_has {
                    if let Some(ref vc) = state.current_scenario.as_ref().unwrap().victory_condition {
                        let v = vc.metric.get(world);
                        if v > vic_metric_max {
                            vic_metric_max = v;
                        }
                        if vc.additional_conditions.iter().all(|c| {
                            c.operator.evaluate(c.metric.get(world), c.value)
                        }) {
                            vic_add_ok += 1;
                        }
                    }
                }
            }

            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);

            let log_len = state.event_log.events.len();
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }
            if victory_tick < 0 && state.world_state.as_ref().unwrap().victory_achieved {
                victory_tick = (t + 1) as i64;
            }

            // ---- accrue the debt this tick created ----------------------------
            let mut flood_here: BTreeMap<String, f64> = BTreeMap::new();
            let mut all3_here: BTreeMap<String, f64> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                if let Some((_, delta)) = POP_EVENTS.iter().find(|(id, _)| *id == ev.id) {
                    *all3_here.entry(ev.actor_id.clone()).or_insert(0.0) += -delta;
                    if ev.id == "flood" {
                        *flood_here.entry(ev.actor_id.clone()).or_insert(0.0) += -delta;
                    }
                }
            }
            {
                let world = state.world_state.as_ref().unwrap();
                let live: Vec<String> = world.actors.keys().cloned().collect();
                for aid in live {
                    let eo = world
                        .actors
                        .get(&aid)
                        .map(|a| a.get_metric("economic_output"))
                        .unwrap_or(0.0);
                    let e = acc.entry(aid.clone()).or_default();
                    // income foregone THIS tick is priced on the debt that existed
                    // before this tick's flood, which is what `apply_treasury` saw
                    e.treas_debt += eo * e.flood_debt * 0.001;
                    e.all3_treas_debt += eo * e.all3_debt * 0.001;
                    let f = flood_here.get(&aid).copied().unwrap_or(0.0);
                    e.flood_debt += f;
                    e.all3_debt += all3_here.get(&aid).copied().unwrap_or(0.0);
                    e.flood_hist.push_back(f);
                    while e.flood_hist.len() > 5 {
                        e.flood_hist.pop_front();
                    }
                }
                for (aid, e) in acc.iter_mut() {
                    if e.died_tick < 0 && world.dead_actor_ids.contains(aid) {
                        e.died_tick = (t + 1) as i64;
                        collapses += 1;
                    }
                }
            }
        }

        let mode = strategy.unwrap_or("noplayer");
        let mut tot = (0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
        for (aid, e) in &acc {
            tot.0 += e.up_pop_div;
            tot.1 += e.up_div;
            tot.2 += e.up_div_masked;
            tot.3 += e.up_div_live;
            tot.4 += e.gate_div;
            tot.5 += e.gate_evals;
            tot.6 += e.recent_div;
            println!(
                "{}\t{}\t{}\t{}\t{}\t{:.1}\t{:+.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                aid, seed, mode, e.ticks, e.rank, e.flood_debt, e.treas_debt,
                e.up_true, e.up_pop_div, e.up_div, e.up_div_masked, e.up_div_live,
                e.gate_evals, e.gate_div, e.fg_flips, e.died_tick, e.recent_div
            );
        }
        println!(
            "#CF\tseed={}\tmode={}\tup_pop_div={}\tup_div={}\tup_masked={}\tup_live={}\trecent_div={}\tgate_div={}/{}\tcollapses={}\tvictory_tick={}\tgates={}\tvic_max={:.1}\tvic_thresh={:.1}\tvic_add_ok={}",
            seed, mode, tot.0, tot.1, tot.2, tot.3, tot.6, tot.4, tot.5, collapses,
            victory_tick, treasury_gates.len(),
            if vic_metric_max.is_finite() { vic_metric_max } else { 0.0 },
            vic_thresh, vic_add_ok
        );
    }
}


// ============================================================================
// Task 24: `cohevents` — the shared pool's effects on `cohesion` /
// `economic_output`
// ============================================================================
//
// Task 23 measured the same pool through a different channel (`population`) and
// closed it with `(C)`. That verdict does not carry, and the reason is topological
// rather than cautionary: `population` reached an actor's death only through
// relevance, so a zero on the relevance path closed the collapse class with it.
// `cohesion` is read by the collapse predicate **directly** — `classic_collapse`
// (`legitimacy < 10 ∧ cohesion < 15 ∧ external_pressure > 85`) and
// `internal_collapse` (`legitimacy < 5 ∧ cohesion < 8`), `mod.rs:1487–1495`. There is
// no intermediate layer on which the verdict can zero itself out.
//
// Three structural differences from `popevents`, each of which changes what has to
// be measured:
//
//   1. **The pool is not just the common pool.** Scenario `create_random_events()`
//      write `cohesion` (constantinople 2, rome 4, milan 1) where they wrote
//      `population` never. Every table below therefore carries both, separated.
//   2. **The field of writers is dense** — 17/21/31 write sites for `cohesion` and
//      31/49/29 for `economic_output`, against six for `population`. So the question
//      is not "is this the main sink" but "does its contribution ever decide
//      anything", and the flow decomposition has to name the other writers rather
//      than lump them into a residual.
//   3. **The pool reads its own output.** Three of the common gates stand on
//      `cohesion` itself (`plague < 60`, `popular_uprising < 30`,
//      `charismatic_preacher < 40`) and a fourth on `economic_output`
//      (`trade_boom > 40`, `famine < 30`). The two positive events have opposite gate
//      polarity — `charismatic_preacher` is negative feedback, `trade_boom` positive —
//      so they are counted as two classes, never summed.
//
// Read-only: drives `tick()`, reads metrics and the event log. No engine symbol is
// modified.

/// The shared events that write `cohesion`, with their nominal deltas — declared
/// here independently of `common.rs` for the same reason [`POP_EVENTS`] is, and
/// checked against it by [`assert_pool_matches_source`].
const COH_EVENTS: &[(&str, f64)] = &[
    ("charismatic_preacher", 3.0),
    ("court_conspiracy", -5.0),
    ("desertion", -5.0),
    ("earthquake", -15.0),
    ("famine", -5.0),
    ("flood", -5.0),
    ("plague", -6.0),
    ("popular_uprising", -8.0),
];

/// The shared events that write `economic_output`.
const EO_EVENTS: &[(&str, f64)] = &[
    ("earthquake", -10.0),
    ("flood", -12.0),
    ("piracy", -5.0),
    ("plague", -5.0),
    ("popular_uprising", -8.0),
    ("trade_boom", 5.0),
];

/// Fail loudly if `common_events()` stops matching the declared writer set for
/// `cohesion` / `economic_output` — a changed delta, a new writer, or a writer that
/// stopped writing all break the run instead of silently re-deriving it.
fn assert_pool_matches_metric(metric: &str, declared: &[(&str, f64)]) {
    let key = engine13::core::RelativeMetricRef::literal(&format!("self.{}", metric));
    let mut found: Vec<(String, f64)> = Vec::new();
    for ev in engine13::events::common_events() {
        if let Some(d) = ev.effects.get(&key) {
            found.push((ev.id.clone(), *d));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let mut want: Vec<(String, f64)> = declared.iter().map(|(i, d)| (i.to_string(), *d)).collect();
    want.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        found, want,
        "common_events() no longer matches the declared writers of `{}` — update the probe, not the run",
        metric
    );
}

/// The whole random-event pool one scenario runs: the shared events first, then the
/// scenario's own, with the flag that tells them apart. Built the same way
/// `phase_random_events` builds it (`mod.rs:406–410`), so nothing can be in the run
/// and missing here.
fn pool_of(sc: &Scenario) -> Vec<(engine13::core::RandomEvent, bool)> {
    engine13::events::common_events()
        .into_iter()
        .map(|e| (e, true))
        .chain(sc.random_events.iter().cloned().map(|e| (e, false)))
        .collect()
}

/// Which actor an event's write to `metric` actually lands on, and how much.
///
/// Not the same question as "which actor is the event's target": a scenario event
/// addresses metrics absolutely (`actor:byzantium.cohesion`) and may in principle
/// write to someone other than the actor it fired on. Resolved through the same
/// `RelativeMetricRef::resolve` the engine uses (`mod.rs:492`), so the attribution is
/// the engine's, not a re-reading of the key string.
fn event_writes_to(
    ev: &engine13::core::RandomEvent,
    target_id: &str,
    metric: &str,
) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (m, delta) in &ev.effects {
        if let Ok(MetricRef::Actor { actor_id, metric: name }) = m.resolve(target_id) {
            if name.as_str() == metric {
                out.push((actor_id.as_str().to_string(), *delta));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Price the dependency phase on one actor's metric snapshot and report what it did
/// to `target`.
///
/// Mirrors `engine::apply_dependency_rule` including the sequential semantics (each
/// rule reads the state the previous rules already changed) and the absence of a
/// clamp inside the phase — `clamp_metrics` runs later, in phase 5. Generic in the
/// target metric because task 24 needs it for `cohesion` where task 22/23 needed it
/// for `population`; the population version is kept as is so those numbers stay
/// reproducible.
fn price_dep_metric(
    rules: &[DependencyRule],
    metrics: &std::collections::HashMap<String, f64>,
    target: &str,
) -> f64 {
    let before = metrics.get(target).copied().unwrap_or(0.0);
    let mut m = metrics.clone();
    for rule in rules {
        let from_val = m.get(rule.from.as_str()).copied().unwrap_or(0.0);
        let delta = match rule.mode {
            DependencyMode::Deficit => match rule.threshold {
                Some(t) if from_val < t => -((t - from_val) * rule.coefficient),
                _ => 0.0,
            },
            DependencyMode::Excess => match rule.threshold {
                Some(t) if from_val > t => -((from_val - t) * rule.coefficient),
                _ => 0.0,
            },
            DependencyMode::Bonus => match rule.threshold {
                Some(t) if from_val > t => (from_val - t) * rule.coefficient,
                _ => 0.0,
            },
            DependencyMode::Linear => from_val * rule.coefficient,
            DependencyMode::DeficitProportional => match rule.threshold {
                Some(t) if t > 0.0 && from_val < t => {
                    -(m.get(rule.to.as_str()).copied().unwrap_or(0.0)
                        * rule.coefficient
                        * (t - from_val)
                        / t)
                }
                _ => 0.0,
            },
        };
        if delta != 0.0 {
            let cur = m.get(rule.to.as_str()).copied().unwrap_or(0.0);
            m.insert(rule.to.as_str().to_string(), cur + delta);
        }
    }
    m.get(target).copied().unwrap_or(0.0) - before
}

#[derive(Default, Clone)]
struct GateAcc {
    elig: u32,   // ticks the actor was eligible to be drawn at all
    gate: u32,   // ticks every condition of the event held for this actor
    fires: u32,  // times the event actually applied to this actor
    coh: f64,    // Σ nominal cohesion written to this actor by this event
    eo: f64,     // Σ nominal economic_output written
}

#[derive(Default)]
struct CohEvAcc {
    ticks: u32,
    seen: bool,
    rank: String,
    sea: bool,
    coh_first: f64,
    coh_last: f64,
    coh_min: f64,
    eo_first: f64,
    eo_last: f64,
    fg: u32,
    // danger-zone occupancy: the two thresholds the collapse predicate stands on,
    // plus the three the pool's own gates stand on
    coh_lt8: u32,
    coh_lt15: u32,
    coh_lt30: u32,
    coh_lt40: u32,
    coh_lt60: u32,
    // Saturation. `cohesion` is clamped to `0..100` (`metric_ref.rs:267` at
    // application, `mod.rs:707` again in phase 5), so an actor sitting on a boundary
    // absorbs writes without moving. Without these two columns the flow
    // decomposition below is uninterpretable: at the ceiling the pool's write is the
    // only downward force in the world, and the residual column silently swallows
    // the truncation that puts the actor back.
    coh_eq100: u32,
    coh_eq0: u32,
    eo_eq100: u32,
    eo_eq0: u32,
    // signed sums — the statement forbids summing the two feedback classes together
    coh_neg: f64,
    coh_pos: f64,
    eo_neg: f64,
    eo_pos: f64,
    // ...split by which pool the writer came from
    coh_pool: f64,
    coh_scen: f64,
    // flow decomposition: total change over the run, the dependency phase's own
    // price, and what is left for the remaining writers
    coh_total: f64,
    coh_dep: f64,
    eo_total: f64,
    eo_dep: f64,
    prev_coh: Option<f64>,
    prev_eo: Option<f64>,
    pending_dep_coh: f64,
    pending_dep_eo: f64,
    // clamp: how often the nominal write could not land in full
    clip_lo: u32,
    clip_hi: u32,
    died_tick: i64,
}

fn cohevents(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    assert_pool_matches_source();
    assert_pool_matches_metric("cohesion", COH_EVENTS);
    assert_pool_matches_metric("economic_output", EO_EVENTS);

    println!("actor\tseed\tmode\tticks\trank\tsea\tcoh0\tcohF\tcohMin\teo0\teoF\tfg%\tcoh<8%\tcoh<15%\tcoh<30%\tcoh<40%\tcoh<60%\tcoh=100%\tcoh=0%\teo=100%\teo=0%\tev_coh-\tev_coh+\tev_coh_pool\tev_coh_scen\tev_eo-\tev_eo+\tcoh_total\tcoh_dep\tcoh_rest\teo_total\teo_dep\teo_rest\tclip_lo\tclip_hi\tdied");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        let sc = state.current_scenario.as_ref().unwrap();
        let rules: Vec<DependencyRule> = sc.dependencies.clone();
        let cap = sc.max_random_events_per_tick;
        let pool = pool_of(sc);
        let pool_size = pool.len();
        // `EventTarget::SeaActors` draws only from actors carrying one of these two
        // tags (`mod.rs:415–418`) — the eligibility gate `piracy` stands behind, and
        // it appears in no scenario file as a condition.
        let sea_ids: std::collections::HashSet<String> = sc
            .actors
            .iter()
            .filter(|a| {
                a.tags.contains(&"maritime".to_string())
                    || a.tags.contains(&"trade_empire".to_string())
            })
            .map(|a| a.id.clone())
            .collect();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool
            .iter()
            .map(|(e, c)| (e.id.clone(), (e.clone(), *c)))
            .collect();
        // events that write either metric at all — the only ones the tables below
        // need a row for
        let writers: Vec<String> = pool
            .iter()
            .filter(|(e, _)| {
                e.effects.keys().any(|m| {
                    let s = m.to_string();
                    s.ends_with(".cohesion") || s.ends_with(".economic_output")
                })
            })
            .map(|(e, _)| e.id.clone())
            .collect();

        let mut acc: BTreeMap<String, CohEvAcc> = BTreeMap::new();
        let mut gates: BTreeMap<(String, String), GateAcc> = BTreeMap::new();
        let mut fires_by_id: BTreeMap<String, u32> = BTreeMap::new();
        let mut raise_troops = 0u32;
        let mut victory_tick: i64 = -1;
        let mut at_cap_ticks = 0u32;
        let mut random_fires_total = 0u32;
        let mut collapses = 0u32;

        for t in 0..ticks {
            let mut coh_before: BTreeMap<String, f64> = BTreeMap::new();
            let mut eo_before: BTreeMap<String, f64> = BTreeMap::new();
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let coh = a.get_metric("cohesion");
                    let eo = a.get_metric("economic_output");
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.coh_first = coh;
                        e.eo_first = eo;
                        e.coh_min = coh;
                        e.rank = format!("{:?}", a.region_rank);
                        e.sea = sea_ids.contains(aid);
                        e.died_tick = -1;
                    }
                    e.ticks += 1;
                    let fg = a.narrative_status == engine13::core::NarrativeStatus::Foreground;
                    if fg {
                        e.fg += 1;
                    }
                    if coh < 8.0 { e.coh_lt8 += 1; }
                    if coh < 15.0 { e.coh_lt15 += 1; }
                    if coh < 30.0 { e.coh_lt30 += 1; }
                    if coh < 40.0 { e.coh_lt40 += 1; }
                    if coh < 60.0 { e.coh_lt60 += 1; }
                    if coh >= 100.0 { e.coh_eq100 += 1; }
                    if coh <= 0.0 { e.coh_eq0 += 1; }
                    if eo >= 100.0 { e.eo_eq100 += 1; }
                    if eo <= 0.0 { e.eo_eq0 += 1; }
                    if coh < e.coh_min { e.coh_min = coh; }
                    e.coh_last = coh;
                    e.eo_last = eo;
                    // charge the previous tick's dependency price against the change
                    // that tick produced — same convention as `attractor`/`popevents`
                    if let Some(prev) = e.prev_coh {
                        e.coh_total += coh - prev;
                        e.coh_dep += e.pending_dep_coh;
                    }
                    if let Some(prev) = e.prev_eo {
                        e.eo_total += eo - prev;
                        e.eo_dep += e.pending_dep_eo;
                    }
                    e.prev_coh = Some(coh);
                    e.prev_eo = Some(eo);
                    e.pending_dep_coh = price_dep_metric(&rules, &a.metrics, "cohesion");
                    e.pending_dep_eo = price_dep_metric(&rules, &a.metrics, "economic_output");
                    coh_before.insert(aid.clone(), coh);
                    eo_before.insert(aid.clone(), eo);

                    // --- gate occupancy, evaluated exactly where the engine does ---
                    for id in &writers {
                        let (ev, _) = &by_id[id];
                        let eligible = match ev.target {
                            engine13::core::EventTarget::Any => fg,
                            engine13::core::EventTarget::All => fg,
                            engine13::core::EventTarget::SeaActors => fg && sea_ids.contains(aid),
                            engine13::core::EventTarget::Actor(ref want) => want == aid,
                        };
                        let g = gates.entry((aid.clone(), id.clone())).or_default();
                        if eligible {
                            g.elig += 1;
                        }
                        let met = ev.conditions.iter().all(|c| {
                            let v = match c.metric.resolve(aid) {
                                Ok(r) => r.get(world),
                                Err(_) => return false,
                            };
                            c.operator.evaluate(v, c.value)
                        });
                        if met {
                            g.gate += 1;
                        }
                    }
                }
            }

            let (_applied, rt) = scripted_step(&mut state, scenario_id, strategy);
            raise_troops += rt;

            let log_len = state.event_log.events.len();
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }
            if victory_tick < 0 && state.world_state.as_ref().unwrap().victory_achieved {
                victory_tick = (t + 1) as i64;
            }

            // --- attribution off the log ---------------------------------------
            let mut fired_here = 0u32;
            let mut coh_this: BTreeMap<String, f64> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                let Some((def, is_common)) = by_id.get(&ev.id) else { continue };
                fired_here += 1;
                *fires_by_id.entry(ev.id.clone()).or_insert(0) += 1;
                for (aid, d) in event_writes_to(def, &ev.actor_id, "cohesion") {
                    let e = acc.entry(aid.clone()).or_default();
                    if d < 0.0 { e.coh_neg += d } else { e.coh_pos += d }
                    if *is_common { e.coh_pool += d } else { e.coh_scen += d }
                    *coh_this.entry(aid.clone()).or_insert(0.0) += d;
                    gates.entry((aid, ev.id.clone())).or_default().coh += d;
                }
                for (aid, d) in event_writes_to(def, &ev.actor_id, "economic_output") {
                    let e = acc.entry(aid.clone()).or_default();
                    if d < 0.0 { e.eo_neg += d } else { e.eo_pos += d }
                    gates.entry((aid, ev.id.clone())).or_default().eo += d;
                }
                gates
                    .entry((ev.actor_id.clone(), ev.id.clone()))
                    .or_default()
                    .fires += 1;
            }
            random_fires_total += fired_here;
            if cap > 0 && fired_here >= cap {
                at_cap_ticks += 1;
            }

            // --- clamp accounting ----------------------------------------------
            // `cohesion` is clamped at the moment of application (`metric_ref.rs:267`,
            // the `_ =>` arm) *and* again in phase 5, so a nominal write that would
            // leave `0..100` cannot land in full. Counted, not corrected — the size of
            // the approximation stays visible, as in task 23 §5.3.
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, before) in &coh_before {
                    let Some(d) = coh_this.get(aid) else { continue };
                    let Some(e) = acc.get_mut(aid) else { continue };
                    if before + d < 0.0 {
                        e.clip_lo += 1;
                    }
                    if before + d > 100.0 {
                        e.clip_hi += 1;
                    }
                    let _ = world;
                }
                for (aid, e) in acc.iter_mut() {
                    if e.died_tick < 0 && world.dead_actor_ids.contains(aid) {
                        e.died_tick = (t + 1) as i64;
                        collapses += 1;
                    }
                }
            }
            let _ = &eo_before;
        }

        let mode = strategy.unwrap_or("noplayer");
        for (aid, e) in &acc {
            let n = e.ticks.max(1) as f64;
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:+.1}\t{:.1}\t{:.1}\t{:.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{}\t{}\t{}",
                aid, seed, mode, e.ticks, e.rank, e.sea,
                e.coh_first, e.coh_last, e.coh_min, e.eo_first, e.eo_last,
                100.0 * e.fg as f64 / n,
                100.0 * e.coh_lt8 as f64 / n,
                100.0 * e.coh_lt15 as f64 / n,
                100.0 * e.coh_lt30 as f64 / n,
                100.0 * e.coh_lt40 as f64 / n,
                100.0 * e.coh_lt60 as f64 / n,
                100.0 * e.coh_eq100 as f64 / n,
                100.0 * e.coh_eq0 as f64 / n,
                100.0 * e.eo_eq100 as f64 / n,
                100.0 * e.eo_eq0 as f64 / n,
                e.coh_neg, e.coh_pos, e.coh_pool, e.coh_scen,
                e.eo_neg, e.eo_pos,
                e.coh_total, e.coh_dep, e.coh_total - e.coh_dep - (e.coh_neg + e.coh_pos),
                e.eo_total, e.eo_dep, e.eo_total - e.eo_dep - (e.eo_neg + e.eo_pos),
                e.clip_lo, e.clip_hi, e.died_tick
            );
        }
        for ((aid, evid), g) in &gates {
            // An event can fire ON one actor and write TO another: constantinople's
            // `mehmed_threatens` is gated on `ottomans.military_size > 150`, targets
            // `ottomans`, and writes `byzantium.cohesion −8`. So a row is worth
            // printing when EITHER role is non-empty — filtering on eligibility alone
            // silently drops the write half of exactly the events this task is about.
            if g.elig == 0 && g.fires == 0 && g.coh == 0.0 && g.eo == 0.0 {
                continue;
            }
            let n = acc.get(aid).map(|e| e.ticks.max(1)).unwrap_or(1) as f64;
            println!(
                "#GATE\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{}\t{:+.1}\t{:+.1}",
                aid, seed, mode, evid,
                if by_id.get(evid).map(|(_, c)| *c).unwrap_or(false) { "pool" } else { "scen" },
                100.0 * g.elig as f64 / n,
                100.0 * g.gate as f64 / n,
                g.fires, g.coh, g.eo
            );
        }
        let fires: Vec<String> = fires_by_id
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        println!(
            "#POOL\tseed={}\tmode={}\tpool_size={}\tcap={}\tfires={}\tat_cap_ticks={}\tvictory_tick={}\traise_troops={}\tcollapses={}\t{}",
            seed, mode, pool_size, cap, random_fires_total, at_cap_ticks, victory_tick,
            raise_troops, collapses, fires.join(" ")
        );
    }
}

// ============================================================================
// Task 24 stage 1: `decisive24` — is the pool's `cohesion` write ever DECISIVE?
// ============================================================================
//
// Method of task 19 п.4 / task 23 §14.1, carried over unchanged: **no second
// simulation**. The RNG stream diverges from any change of state
// (`interactions.rs:438` spends four extra draws only on a successful combat roll,
// and that roll's probability is computed from metrics), so a second run's
// differences would be uninterpretable. What is recomputed instead is the **verdict**
// the metric feeds, tick by tick, in a shadow world.
//
// The shadow world here is the one in which random events never touched `cohesion`
// (and, for the second half, never touched `economic_output`). Two things make this
// harder than task 23's population debt, and both are handled explicitly:
//
//   1. **`cohesion` is clamped to `0..100`** — at application (`metric_ref.rs:267`)
//      and again in phase 5. A carried-forward debt would therefore be wrong in both
//      directions: it would credit the shadow with losses the real world's floor
//      already absorbed, and it would let the shadow drift above 100 and never come
//      back. So the shadow is carried as a **value with its own clamp**, advanced by
//      the real world's non-event increment:
//          S(t) = clamp( S(t−1) + [C(t) − C(t−1)] − δ_events(t), 0, 100 )
//      The debt `S − C` is then an output of the model, not an input, and is bounded
//      by construction.
//
//   2. **Collapse needs three consecutive dangerous ticks**, and the counter resets on
//      the first non-dangerous one (`mod.rs:1538–1552`). A shadow verdict without its
//      own counter would be simply wrong — the same trap task 23 hit on the demotion
//      path, where `actor_upheaval_ticks` needed its own shadow. So the shadow keeps
//      `collapse_warning_ticks` of its own, with the same reset rule and the same
//      `minimum_survival_ticks` skip.
//
// Where the verdict is taken. `check_collapses` runs in phase 7. Nothing after it
// writes `cohesion`, `legitimacy`, `external_pressure` or `military_size`
// (`phase_vassalage` writes only `expansion_count`; `phase_record` writes only
// `family_state`; `phase_advance` writes only the clock), so the metrics observed
// **after** `tick()` returns are exactly the ones phase 7 read. The replication is
// therefore exact, not first-order — and it is checked, not asserted: the replicated
// *real* counter is compared against `world.collapse_warning_ticks` every tick and
// `cw_mismatch` is printed.
//
// Two of three death channels read `cohesion`; the third does not:
//   * `classic_collapse`  = `leg < 10 ∧ coh < 15 ∧ ep > 85`      — shadowed
//   * `internal_collapse` = `leg < 5  ∧ coh < 8`                  — shadowed
//   * `conquest_collapse` = `mil < MIN ∧ leg < 10 ∧ ep > 85 ∧ besieged` — identical in
//     both worlds, and it is part of the same `in_danger` disjunction, so it can mask
//     a shadowed difference. Counted separately (`cq_mask`) rather than ignored.
//
// The `economic_output` half has no edge into the collapse predicate at all (no
// dependency rule writes `cohesion` from `economic_output`; the walk found four
// writers of `cohesion` and none of them reads it). Its reachable channels are the
// two task 23 already built machinery for — the relevance predicate, where both
// metrics are among the eight, and `treasury` through `income = eo·pop·0.001` — so
// both are carried here too rather than left unstated.

#[derive(Default)]
struct Cf24 {
    ticks: u32,
    seen: bool,
    rank: String,
    min_surv: Option<u32>,
    neighbors: Vec<(String, u32)>,
    // shadow values, each with its own clamp
    s_coh: f64,
    s_eo: f64,
    p_coh: f64, // second shadow: common pool only, scenario events left in place
    prev_coh: Option<f64>,
    prev_eo: Option<f64>,
    // debt series aligned with `metric_history`'s 5-slot window
    coh_debt_hist: std::collections::VecDeque<f64>,
    eo_debt_hist: std::collections::VecDeque<f64>,
    treas_debt: f64,
    // --- collapse ---------------------------------------------------------
    real_ct: u32,
    shad_ct: u32,
    pool_ct: u32,
    dng_true: u32,   // ticks the real `in_danger` held, for scale
    s_dng: u32,      // ticks the shadow `in_danger` held — the "would it have died
                     // anyway, later" question, answered as far as a shadow can
    r_cls: u32,      // ticks each real path held, so a death can be attributed
    r_int: u32,
    r_cq: u32,
    s_cls: u32,
    s_int: u32,
    died_path: &'static str, // which real path was true on the death tick
    dng_div: u32,    // ticks the shadow `in_danger` differs
    cq_mask: u32,    // ...of which the difference was masked by `conquest_collapse`
    col_div: u32,    // ticks the collapse DECISION (counter ≥ 3) differs
    died_tick: i64,
    saved: u32,      // real collapse fired while the shadow counter was below 3
    saved_pool: u32, // same, common pool only
    // --- relevance --------------------------------------------------------
    up_true: u32,
    up_div: u32,
    up_masked: u32,
    up_live: u32,
    cu_div: u32,     // full `condition_upheaval`, incl. the `coh < 25` disjunct
    // --- vassalage band ---------------------------------------------------
    band_div: u32,
    // --- victory ----------------------------------------------------------
    gate_evals: u32,
    gate_div: u32,
    cw_mismatch: u32,
    // narrative-status flips actually observed, so a divergence in the INPUT of a
    // relevance decision can be told apart from a decision that moved — the same
    // discipline task 23 §14.2 used to close its demotion path
    fg_flips: u32,
    prev_fg: Option<bool>,
}

/// `check_actor_upheaval` (`mod.rs:1199`) with the window delta of any subset of the
/// eight metrics shifted — the shadow world's history. Mirrors the engine exactly,
/// including the `len() >= 2` guard and the metric list.
///
/// Separate from [`upheaval_verdict`] on purpose: that one is task 23's and its
/// numbers are quoted in a published document, so it is left byte-identical.
fn upheaval_verdict_multi(
    world: &WorldState,
    actor_id: &str,
    shifts: &[(&str, f64)],
) -> bool {
    const METRICS: [&str; 8] = [
        "population", "military_size", "military_quality", "economic_output",
        "cohesion", "legitimacy", "external_pressure", "treasury",
    ];
    for m in METRICS {
        let key = format!("{}:{}", actor_id, m);
        if let Some(h) = world.metric_history.get(&key) {
            if h.len() >= 2 {
                let oldest = h.front().copied().unwrap_or(0.0);
                let newest = h.back().copied().unwrap_or(0.0);
                let mut d = newest - oldest;
                if let Some((_, s)) = shifts.iter().find(|(k, _)| *k == m) {
                    d += *s;
                }
                if d.abs() > 30.0 {
                    return true;
                }
            }
        }
    }
    false
}

/// The engine's `in_danger` disjunction (`mod.rs:1487–1536`), evaluated on supplied
/// values so the same code serves the real world and the shadow. Returns the three
/// paths separately because the third one does not read `cohesion` and can therefore
/// mask a shadowed difference.
fn danger_paths(coh: f64, leg: f64, ep: f64, mil: f64, besieged: bool) -> (bool, bool, bool) {
    let classic = leg < 10.0 && coh < 15.0 && ep > 85.0;
    let internal = leg < 5.0 && coh < 8.0;
    let conquest = mil < engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY
        && leg < 10.0
        && ep > 85.0
        && besieged;
    (classic, internal, conquest)
}

fn mode_label(strategy: Option<&str>) -> &str {
    strategy.unwrap_or("noplayer")
}

fn decisive24(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    assert_pool_matches_source();
    assert_pool_matches_metric("cohesion", COH_EVENTS);
    assert_pool_matches_metric("economic_output", EO_EVENTS);

    println!("actor\tseed\tmode\tticks\trank\tcoh_debt\teo_debt\ttreas_debt\tdng_true\ts_dng\tr_cls\tr_int\tr_cq\ts_cls\ts_int\tdng_div\tcq_mask\tcol_div\tsaved\tsaved_pool\tdied\tup_true\tup_div\tup_masked\tup_live\tcu_div\tband_div\tgate_evals\tgate_div\tcw_mism\tfg_flips");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        let sc = state.current_scenario.as_ref().unwrap();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool_of(sc)
            .into_iter()
            .map(|(e, c)| (e.id.clone(), (e, c)))
            .collect();

        // Same restriction as `decisive23`: only gates on actions the scripted player
        // actually attempts. A gate nobody evaluates cannot reach an outcome.
        let mut treasury_gates: Vec<(String, ComparisonOperator, f64, String)> = Vec::new();
        if let Some(strat) = strategy {
            let attempted = priority_list(scenario_id, strat);
            for act in sc.patron_actions.iter().chain(sc.universal_actions.iter()) {
                if !attempted.contains(&act.id.as_str()) {
                    continue;
                }
                if let engine13::core::ActionCondition::Metric {
                    metric: MetricRef::Actor { actor_id, metric: m },
                    operator,
                    value,
                } = &act.available_if
                {
                    if m.as_str() == "treasury" {
                        treasury_gates.push((
                            actor_id.as_str().to_string(),
                            operator.clone(),
                            *value,
                            act.id.clone(),
                        ));
                    }
                }
            }
        }
        // cloned out of the scenario so the borrow ends here: the loop below needs
        // `state` mutably for `tick()`
        let victory = sc.victory_condition.clone();
        let (vic_thresh, vic_has) = match victory {
            Some(ref vc) => (vc.threshold, true),
            None => (0.0, false),
        };
        let mut vic_metric_max = f64::NEG_INFINITY;
        let mut vic_add_ok = 0u32;

        let mut acc: BTreeMap<String, Cf24> = BTreeMap::new();
        let mut victory_tick: i64 = -1;
        let mut collapses = 0u32;

        for t in 0..ticks {
            // ---- seed the shadow from the world's own starting values ----------
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.rank = format!("{:?}", a.region_rank);
                        e.min_surv = a.minimum_survival_ticks;
                        e.neighbors = a.neighbors.iter().map(|n| (n.id.clone(), n.distance)).collect();
                        e.s_coh = a.get_metric("cohesion");
                        e.p_coh = a.get_metric("cohesion");
                        e.s_eo = a.get_metric("economic_output");
                        e.prev_coh = Some(a.get_metric("cohesion"));
                        e.prev_eo = Some(a.get_metric("economic_output"));
                        e.died_tick = -1;
                    }
                }
                // victory reachability, same two columns as `decisive23`
                if vic_has {
                    if let Some(ref vc) = victory {
                        let v = vc.metric.get(world);
                        if v > vic_metric_max {
                            vic_metric_max = v;
                        }
                        if vc
                            .additional_conditions
                            .iter()
                            .all(|c| c.operator.evaluate(c.metric.get(world), c.value))
                        {
                            vic_add_ok += 1;
                        }
                    }
                }
                // victory channel: availability gates in both worlds
                for (owner, op, value, _id) in &treasury_gates {
                    if world.dead_actor_ids.contains(owner) {
                        continue;
                    }
                    let Some(a) = world.actors.get(owner) else { continue };
                    let tr = a.get_metric("treasury");
                    let debt = acc.get(owner).map(|e| e.treas_debt).unwrap_or(0.0);
                    let e = acc.entry(owner.clone()).or_default();
                    e.gate_evals += 1;
                    if op.evaluate(tr, *value) != op.evaluate(tr + debt, *value) {
                        e.gate_div += 1;
                    }
                }
            }

            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);

            let log_len = state.event_log.events.len();
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }
            if victory_tick < 0 && state.world_state.as_ref().unwrap().victory_achieved {
                victory_tick = (t + 1) as i64;
            }

            // ---- what the events wrote this tick -------------------------------
            let mut coh_ev: BTreeMap<String, f64> = BTreeMap::new();
            let mut coh_ev_pool: BTreeMap<String, f64> = BTreeMap::new();
            let mut eo_ev: BTreeMap<String, f64> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                let Some((def, is_common)) = by_id.get(&ev.id) else { continue };
                for (aid, d) in event_writes_to(def, &ev.actor_id, "cohesion") {
                    *coh_ev.entry(aid.clone()).or_insert(0.0) += d;
                    if *is_common {
                        *coh_ev_pool.entry(aid).or_insert(0.0) += d;
                    }
                }
                for (aid, d) in event_writes_to(def, &ev.actor_id, "economic_output") {
                    *eo_ev.entry(aid).or_insert(0.0) += d;
                }
            }

            // ---- the verdict, on the values phase 7 actually read ---------------
            let world = state.world_state.as_ref().unwrap();
            let cur_tick = t; // `check_collapses` ran with `world.tick == t`
            // an actor that died THIS tick is gone from `world.actors`; its metrics at
            // the moment of the verdict survive in `dead_actors[].final_metrics`
            let just_dead: BTreeMap<String, &std::collections::HashMap<String, f64>> = world
                .dead_actors
                .iter()
                .filter(|d| d.tick_death == cur_tick)
                .map(|d| (d.id.clone(), &d.final_metrics))
                .collect();
            let live_mil: BTreeMap<String, f64> = world
                .actors
                .iter()
                .map(|(k, a)| (k.clone(), a.get_metric("military_size")))
                .collect();

            let ids: Vec<String> = acc.keys().cloned().collect();
            for aid in ids {
                let alive = world.actors.contains_key(&aid) && !world.dead_actor_ids.contains(&aid);
                let metrics: Option<std::collections::HashMap<String, f64>> = if alive {
                    world.actors.get(&aid).map(|a| a.metrics.clone())
                } else {
                    just_dead.get(&aid).map(|m| (*m).clone())
                };
                let Some(m) = metrics else { continue };
                let e = acc.get_mut(&aid).expect("seeded");
                if !e.seen {
                    continue; // spawned mid-tick: phase 7 of this tick did not see it
                }

                let coh = m.get("cohesion").copied().unwrap_or(0.0);
                let eo = m.get("economic_output").copied().unwrap_or(0.0);
                let leg = m.get("legitimacy").copied().unwrap_or(0.0);
                let ep = m.get("external_pressure").copied().unwrap_or(0.0);
                let mil = m.get("military_size").copied().unwrap_or(0.0);
                let pop = m.get("population").copied().unwrap_or(0.0);

                // --- advance the shadow values ---------------------------------
                let d_coh = coh - e.prev_coh.unwrap_or(coh);
                let d_eo = eo - e.prev_eo.unwrap_or(eo);
                let ev_c = coh_ev.get(&aid).copied().unwrap_or(0.0);
                let ev_cp = coh_ev_pool.get(&aid).copied().unwrap_or(0.0);
                let ev_e = eo_ev.get(&aid).copied().unwrap_or(0.0);
                e.s_coh = (e.s_coh + d_coh - ev_c).clamp(0.0, 100.0);
                e.p_coh = (e.p_coh + d_coh - ev_cp).clamp(0.0, 100.0);
                e.s_eo = (e.s_eo + d_eo - ev_e).clamp(0.0, 100.0);
                e.prev_coh = Some(coh);
                e.prev_eo = Some(eo);
                e.ticks += 1;

                // income the missing `economic_output` would have earned; priced on the
                // debt that existed before this tick, which is what `apply_treasury` saw
                e.treas_debt += (e.s_eo - eo) * pop * 0.001;

                // debt series, aligned with the history slot phase 8 pushed this tick
                e.coh_debt_hist.push_back(e.s_coh - coh);
                while e.coh_debt_hist.len() > 5 { e.coh_debt_hist.pop_front(); }
                e.eo_debt_hist.push_back(e.s_eo - eo);
                while e.eo_debt_hist.len() > 5 { e.eo_debt_hist.pop_front(); }

                // --- collapse verdict, both worlds ------------------------------
                let besieged = e.neighbors.iter().any(|(nid, dist)| {
                    *dist == 1
                        && live_mil
                            .get(nid)
                            .map(|v| *v >= engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY)
                            .unwrap_or(false)
                });
                let skip = matches!(e.min_surv, Some(ms) if cur_tick < ms);
                if !skip {
                    let (rc, ri, rq) = danger_paths(coh, leg, ep, mil, besieged);
                    let (sc_, si, _) = danger_paths(e.s_coh, leg, ep, mil, besieged);
                    let (pc, pi, _) = danger_paths(e.p_coh, leg, ep, mil, besieged);
                    let real_d = rc || ri || rq;
                    let shad_d = sc_ || si || rq;
                    let pool_d = pc || pi || rq;
                    if real_d { e.dng_true += 1; }
                    if shad_d { e.s_dng += 1; }
                    if rc { e.r_cls += 1; }
                    if ri { e.r_int += 1; }
                    if rq { e.r_cq += 1; }
                    if sc_ { e.s_cls += 1; }
                    if si { e.s_int += 1; }
                    if real_d != shad_d { e.dng_div += 1; }
                    // the cohesion-reading half differed but `conquest_collapse` held
                    // the disjunction up anyway
                    if (rc || ri) != (sc_ || si) && real_d == shad_d {
                        e.cq_mask += 1;
                    }
                    if real_d { e.real_ct += 1 } else { e.real_ct = 0 }
                    if shad_d { e.shad_ct += 1 } else { e.shad_ct = 0 }
                    if pool_d { e.pool_ct += 1 } else { e.pool_ct = 0 }
                    if (e.real_ct >= 3) != (e.shad_ct >= 3) { e.col_div += 1; }
                    // cross-check the replication against the engine's own counter
                    // The engine leaves the entry in place at 3 for an actor it has
                    // just removed, so a plain comparison holds on the death tick too.
                    let engine_ct = world.collapse_warning_ticks.get(&aid).copied().unwrap_or(0);
                    if engine_ct != e.real_ct {
                        e.cw_mismatch += 1;
                    }
                }
                if !alive && e.died_tick < 0 {
                    e.died_tick = (t + 1) as i64;
                    collapses += 1;
                    let besieged_d = e.neighbors.iter().any(|(nid, dist)| {
                        *dist == 1
                            && live_mil
                                .get(nid)
                                .map(|v| *v >= engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY)
                                .unwrap_or(false)
                    });
                    let (dc, di, dq) = danger_paths(coh, leg, ep, mil, besieged_d);
                    e.died_path = match (dc, di, dq) {
                        (true, _, _) => "classic",
                        (_, true, _) => "internal",
                        (_, _, true) => "conquest",
                        _ => "none",
                    };
                    if e.shad_ct < 3 { e.saved += 1; }
                    if e.pool_ct < 3 { e.saved_pool += 1; }
                    println!(
                        "#DEATH\t{}\t{}\t{}\t{}\ttick={}\tpath={}\treal_ct={}\tshad_ct={}\tpool_ct={}\tcoh={:.2}\ts_coh={:.2}\tleg={:.2}\tep={:.2}\tmil={:.4}\tr_dng={}\ts_dng={}",
                        aid, seed, mode_label(strategy), e.rank, t + 1, e.died_path,
                        e.real_ct, e.shad_ct, e.pool_ct, coh, e.s_coh, leg, ep, mil,
                        e.dng_true, e.s_dng
                    );
                }

                // --- relevance --------------------------------------------------
                // the shadow's window delta is `(C+D)_newest − (C+D)_oldest`, i.e. the
                // real delta plus `D_newest − D_oldest`. `D` is carried cumulatively
                // here (task 23 carried per-tick removals and summed them, which is
                // the same quantity written differently).
                let cw: f64 = e.coh_debt_hist.back().copied().unwrap_or(0.0)
                    - e.coh_debt_hist.front().copied().unwrap_or(0.0);
                let ew: f64 = e.eo_debt_hist.back().copied().unwrap_or(0.0)
                    - e.eo_debt_hist.front().copied().unwrap_or(0.0);
                let real_up = upheaval_verdict_multi(world, &aid, &[]);
                let shad_up =
                    upheaval_verdict_multi(world, &aid, &[("cohesion", cw), ("economic_output", ew)]);
                if real_up { e.up_true += 1; }
                if real_up != shad_up {
                    e.up_div += 1;
                    if coh < 25.0 || leg < 20.0 { e.up_masked += 1 } else { e.up_live += 1 }
                }
                let real_cu = real_up || coh < 25.0 || leg < 20.0;
                let shad_cu = shad_up || e.s_coh < 25.0 || leg < 20.0;
                if real_cu != shad_cu { e.cu_div += 1; }

                let fg = world
                    .actors
                    .get(&aid)
                    .map(|a| a.narrative_status == engine13::core::NarrativeStatus::Foreground)
                    .unwrap_or(false);
                if let Some(p) = e.prev_fg {
                    if p != fg && alive {
                        e.fg_flips += 1;
                    }
                }
                e.prev_fg = Some(fg);

                // --- vassalage band (the third consumer of `cohesion`) ----------
                let band_r = (70.0..=85.0).contains(&ep)
                    && (10.0..=25.0).contains(&leg)
                    && (15.0..=30.0).contains(&coh);
                let band_s = (70.0..=85.0).contains(&ep)
                    && (10.0..=25.0).contains(&leg)
                    && (15.0..=30.0).contains(&e.s_coh);
                if band_r != band_s { e.band_div += 1; }
            }
        }

        let mode = strategy.unwrap_or("noplayer");
        let mut tot = [0u32; 12];
        for (aid, e) in &acc {
            tot[0] += e.dng_div;
            tot[1] += e.cq_mask;
            tot[2] += e.col_div;
            tot[3] += e.saved;
            tot[4] += e.up_div;
            tot[5] += e.up_live;
            tot[6] += e.cu_div;
            tot[7] += e.band_div;
            tot[8] += e.gate_div;
            tot[9] += e.cw_mismatch;
            tot[10] += e.saved_pool;
            tot[11] += e.s_dng;
            println!(
                "{}\t{}\t{}\t{}\t{}\t{:+.2}\t{:+.2}\t{:+.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                aid, seed, mode, e.ticks, e.rank,
                e.s_coh - e.prev_coh.unwrap_or(0.0),
                e.s_eo - e.prev_eo.unwrap_or(0.0),
                e.treas_debt,
                e.dng_true, e.s_dng, e.r_cls, e.r_int, e.r_cq, e.s_cls, e.s_int,
                e.dng_div, e.cq_mask, e.col_div, e.saved, e.saved_pool,
                e.died_tick,
                e.up_true, e.up_div, e.up_masked, e.up_live, e.cu_div, e.band_div,
                e.gate_evals, e.gate_div, e.cw_mismatch, e.fg_flips
            );
        }
        println!(
            "#CF24\tseed={}\tmode={}\tdng_div={}\tcq_mask={}\tcol_div={}\tsaved={}\tsaved_pool={}\ts_dng={}\tup_div={}\tup_live={}\tcu_div={}\tband_div={}\tgate_div={}\tcollapses={}\tvictory_tick={}\tvic_max={:.1}\tvic_thresh={:.1}\tvic_add_ok={}\tcw_mismatch={}",
            seed, mode, tot[0], tot[1], tot[2], tot[3], tot[10], tot[11], tot[4], tot[5], tot[6], tot[7],
            tot[8], collapses, victory_tick,
            if vic_metric_max.is_finite() { vic_metric_max } else { 0.0 },
            vic_thresh, vic_add_ok, tot[9]
        );
    }
}


// ===========================================================================
// task 25 — spawn completeness: `spawnwalk` and `decisive25`
// ===========================================================================
//
// Two modes, and the reason neither is a parameter on an existing one is the same
// reason task 24 gave for `cohevents`: the question is about a container no mode
// reads.
//
// * `spawnwalk` answers §1 п.1–п.3 of the statement — the FACT (which of the eight
//   declared keys exist in the world, when they start existing and who created
//   them), the EDGES (how many neighbour pairs each spawned actor takes part in,
//   replicating `get_neighbor_pairs`, which is private), and the REACHABILITY of
//   every writer (how many times each container writer actually landed on the
//   actor). All three are measured in the world, not read off the TOML.
//
// * `decisive25` answers §1 п.5 — the counterfactual. It does NOT run a second
//   simulation (§2 forbids it): it advances, alongside the real run, one shadow
//   copy of each spawned actor per candidate variant, and recomputes the collapse
//   verdict and the relevance verdict on the shadow values with their own
//   `collapse_warning_ticks`.
//
// The shadow here is stronger than task 24's, and the reason is structural: the
// three spawned actors of `constantinople_1430` have NO neighbours, hence take
// part in no interaction, and `constantinople_1430` has no rank-C bonus and no
// `auto_delta` addressed to them. Their entire trajectory is therefore produced by
// four things — `apply_treasury`, the dependency phase, the random events that fire
// on them, and the clamps — all four of which the probe can reproduce exactly. So
// instead of a differential shadow ("real delta minus what the events wrote") the
// probe runs a FULL replica and validates it against the engine every tick
// (`rep_mismatch`). A variant shadow is the same replica with the missing keys
// filled in. The one assumption that remains is named in the document: the firing
// schedule of random events is taken from the real run, because a shadow cannot
// re-roll RNG.

/// The eight keys `default_metrics()` declares, with the values variant (C) would
/// install. Asserted against the engine so a change there breaks the run.
const DEFAULT_KEYS: &[(&str, f64)] = &[
    ("cohesion", 50.0),
    ("economic_output", 50.0),
    ("external_pressure", 30.0),
    ("legitimacy", 50.0),
    ("military_quality", 50.0),
    ("military_size", 50.0),
    ("population", 1000.0),
    ("treasury", 100.0),
];

fn assert_defaults_match_engine() {
    let mut have: Vec<(String, f64)> = engine13::core::actor::default_metrics()
        .into_iter()
        .collect();
    have.sort_by(|a, b| a.0.cmp(&b.0));
    let want: Vec<(String, f64)> = DEFAULT_KEYS
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
    assert_eq!(
        have, want,
        "core::actor::default_metrics() changed — update the probe, not the run"
    );
}

/// `get_neighbor_pairs` (`interactions.rs:296`) is private; this is the same
/// construction — an edge counts only if the other end is a LIVING actor, and the
/// pair is deduplicated on the sorted key, so a one-sided edge still makes a pair.
fn neighbor_pairs_of(world: &WorldState, aid: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(a) = world.actors.get(aid) {
        for n in &a.neighbors {
            if world.actors.contains_key(&n.id) {
                out.push(n.id.clone());
            }
        }
    }
    for (other_id, other) in &world.actors {
        if other_id == aid {
            continue;
        }
        if other.neighbors.iter().any(|n| n.id == aid) && !out.contains(other_id) {
            out.push(other_id.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Every `spawn_actor` config of a scenario, with the keys it does NOT name.
fn spawn_configs(sc: &Scenario) -> Vec<(String, Vec<String>, Vec<String>, usize)> {
    let mut out = Vec::new();
    for me in &sc.milestone_events {
        if let Some(cfg) = &me.spawn_actor {
            let named: Vec<String> = {
                let mut v: Vec<String> = cfg
                    .initial_metrics
                    .keys()
                    .map(|k| k.as_str().to_string())
                    .collect();
                v.sort();
                v
            };
            let missing: Vec<String> = DEFAULT_KEYS
                .iter()
                .map(|(k, _)| k.to_string())
                .filter(|k| !named.contains(k))
                .collect();
            out.push((cfg.actor_id.clone(), named, missing, cfg.neighbors.len()));
        }
    }
    out
}

#[derive(Default, Clone)]
struct KeyAcc {
    present_first: bool, // the key existed on the first tick the actor was observed
    first_tick: i64,     // tick the key first existed at (−1: never)
    val_first: f64,
    val_last: f64,
    ticks_present: u32,
    ticks_zero: u32,
    total: f64,   // Σ observed change
    dep: f64,     // Σ priced dependency-phase contribution
    ev: f64,      // Σ nominal random-event contribution
    treas: f64,   // Σ `apply_treasury` contribution (treasury only)
}

#[derive(Default)]
struct SpawnAcc {
    seen: bool,
    first_tick: i64,
    died_tick: i64,
    ticks: u32,
    rank: String,
    religion: String,
    culture: String,
    n_tags: usize,
    n_actor_tags: usize,
    n_on_collapse: usize,
    min_surv: Option<u32>,
    nbr_out: usize,
    pair_ticks: u32,
    partners: std::collections::BTreeSet<String>,
    keys: BTreeMap<String, KeyAcc>,
    prev: std::collections::HashMap<String, f64>,
    // per-writer reachability: how many times each named container writer landed
    writers: BTreeMap<String, u32>,
}

fn spawnwalk(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;
    assert_defaults_match_engine();
    assert_pool_matches_source();

    // the container half, printed before the world half so the two can be crossed
    {
        let sc = registry::load_by_id(scenario_id).expect("scenario");
        for (aid, named, missing, nbrs) in spawn_configs(&sc) {
            println!(
                "#SPAWNCFG\t{}\t{}\tnamed={}\tmissing={}\tneighbors={}",
                scenario_id,
                aid,
                named.join("+"),
                if missing.is_empty() { "-".to_string() } else { missing.join("+") },
                nbrs
            );
        }
    }

    println!("actor\tseed\tmode\tfirst\tdied\tticks\trank\trelig\tcult\ttags\tatags\toncol\tminsurv\tnbr_out\tpair_ticks\tpartners\tmetric\tkey0\tkey_at\tv0\tvlast\tt_present\tt_zero\ttotal\tdep\tev\ttreas");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        let sc = state.current_scenario.as_ref().unwrap();
        let rules = sc.dependencies.clone();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool_of(sc)
            .into_iter()
            .map(|(e, c)| (e.id.clone(), (e, c)))
            .collect();
        let mut acc: BTreeMap<String, SpawnAcc> = BTreeMap::new();

        for t in 0..ticks {
            // snapshot before the tick: this is what the dependency phase reads for an
            // actor untouched by phases 1–2 (proved separately for the three spawns)
            let before: BTreeMap<String, std::collections::HashMap<String, f64>> = {
                let w = state.world_state.as_ref().unwrap();
                w.actors.iter().map(|(k, a)| (k.clone(), a.metrics.clone())).collect()
            };
            {
                let w = state.world_state.as_ref().unwrap();
                for (aid, a) in w.actors.iter() {
                    if w.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.first_tick = t as i64;
                        e.died_tick = -1;
                        e.rank = format!("{:?}", a.region_rank);
                        e.religion = format!("{:?}", a.religion);
                        e.culture = format!("{:?}", a.culture);
                        e.n_tags = a.tags.len();
                        e.n_actor_tags = a.actor_tags.len();
                        e.n_on_collapse = a.on_collapse.len();
                        e.min_surv = a.minimum_survival_ticks;
                        e.nbr_out = a.neighbors.len();
                        for (k, _) in DEFAULT_KEYS {
                            let ka = e.keys.entry(k.to_string()).or_default();
                            ka.present_first = a.metrics.contains_key(*k);
                            ka.first_tick = if ka.present_first { t as i64 } else { -1 };
                            ka.val_first = a.get_metric(k);
                        }
                        e.prev = a.metrics.clone();
                    }
                    let ps = neighbor_pairs_of(w, aid);
                    if !ps.is_empty() {
                        e.pair_ticks += 1;
                        for p in ps {
                            e.partners.insert(p);
                        }
                    }
                }
            }

            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);
            let log_len = state.event_log.events.len();
            {
                let ws = state.world_state.as_mut().unwrap();
                let sr = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(ws, sr, &mut state.event_log, rng);
            }

            // what the events wrote this tick, per actor per metric, and to whom
            let mut ev_writes: BTreeMap<(String, String), f64> = BTreeMap::new();
            let mut ev_hits: BTreeMap<(String, String), u32> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                let Some((def, _is_common)) = by_id.get(&ev.id) else { continue };
                for (k, _) in DEFAULT_KEYS {
                    for (aid, d) in event_writes_to(def, &ev.actor_id, k) {
                        *ev_writes.entry((aid.clone(), k.to_string())).or_insert(0.0) += d;
                        *ev_hits.entry((aid, format!("event:{}", def.id))).or_insert(0) += 1;
                    }
                }
            }

            let w = state.world_state.as_ref().unwrap();
            // an actor that died THIS tick is gone from `world.actors`; its metrics at
            // the moment of the verdict survive in `dead_actors[].final_metrics`. Only
            // this tick: after it the actor is out of the world and must stop
            // contributing to every column, or a short-lived actor accumulates
            // hundreds of posthumous ticks of nominal dependency price.
            let just_dead: BTreeMap<String, std::collections::HashMap<String, f64>> = w
                .dead_actors
                .iter()
                .filter(|d| d.tick_death == t)
                .map(|d| (d.id.clone(), d.final_metrics.clone()))
                .collect();
            let ids: Vec<String> = acc.keys().cloned().collect();
            for aid in ids {
                let alive = w.actors.contains_key(&aid) && !w.dead_actor_ids.contains(&aid);
                let m: Option<std::collections::HashMap<String, f64>> = if alive {
                    w.actors.get(&aid).map(|a| a.metrics.clone())
                } else {
                    just_dead.get(&aid).cloned()
                };
                let Some(m) = m else { continue };
                let bef = before.get(&aid).cloned().unwrap_or_default();
                let e = acc.get_mut(&aid).expect("seeded");
                if !e.seen {
                    continue;
                }
                if !alive && e.died_tick < 0 {
                    e.died_tick = (t + 1) as i64;
                }
                e.ticks += 1;
                for ((waid, wid), n) in ev_hits.iter() {
                    if *waid == aid {
                        *e.writers.entry(wid.clone()).or_insert(0) += n;
                    }
                }
                for (k, _) in DEFAULT_KEYS {
                    let present = m.contains_key(*k);
                    let v = m.get(*k).copied().unwrap_or(0.0);
                    let pv = e.prev.get(*k).copied().unwrap_or(0.0);
                    let ka = e.keys.entry(k.to_string()).or_default();
                    if present {
                        if ka.first_tick < 0 {
                            ka.first_tick = (t + 1) as i64;
                        }
                        ka.ticks_present += 1;
                        if v == 0.0 {
                            ka.ticks_zero += 1;
                        }
                    }
                    ka.val_last = v;
                    ka.total += v - pv;
                    ka.dep += price_dep_metric(&rules, &bef, k);
                    ka.ev += ev_writes.get(&(aid.clone(), k.to_string())).copied().unwrap_or(0.0);
                    if *k == "treasury" {
                        let eo = bef.get("economic_output").copied().unwrap_or(0.0);
                        let pop = bef.get("population").copied().unwrap_or(0.0);
                        let ms = bef.get("military_size").copied().unwrap_or(0.0);
                        ka.treas += eo * pop * 0.001 - ms * 0.8;
                    }
                }
                e.prev = m;
            }
        }

        for (aid, e) in &acc {
            for (k, _) in DEFAULT_KEYS {
                let ka = e.keys.get(*k).cloned().unwrap_or_default();
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{:.3}\t{}\t{}\t{:+.3}\t{:+.3}\t{:+.3}\t{:+.3}",
                    aid, seed, mode_label(strategy), e.first_tick, e.died_tick, e.ticks,
                    e.rank, e.religion, e.culture, e.n_tags, e.n_actor_tags, e.n_on_collapse,
                    e.min_surv.map(|v| v as i64).unwrap_or(-1),
                    e.nbr_out, e.pair_ticks,
                    if e.partners.is_empty() { "-".to_string() } else { e.partners.iter().cloned().collect::<Vec<_>>().join("+") },
                    k, ka.present_first, ka.first_tick, ka.val_first, ka.val_last,
                    ka.ticks_present, ka.ticks_zero, ka.total, ka.dep, ka.ev, ka.treas
                );
            }
            for (wid, n) in &e.writers {
                println!("#WRITER\t{}\t{}\t{}\t{}\t{}", aid, seed, mode_label(strategy), wid, n);
            }
        }
    }
}

/// One candidate filling of the missing keys. `None` means "leave the key exactly as
/// the world has it" — which for the three spawns of `constantinople_1430` means
/// "absent", i.e. read as `0.0`.
#[derive(Clone, Debug)]
struct Variant {
    label: String,
    fill: BTreeMap<String, f64>,
}

/// `C` — `ensure_default_metrics` (the engine's own declared answer, variant (C) of
/// the fork). `A` — the `france` template of `milan_1477`, the project's only complete
/// `spawn_actor` (variant (A)). Anything else is an explicit `key=value` list joined
/// by `+`, with the short names `ep` / `pop` / `tr` / `mq`.
fn parse_variant(spec: &str) -> Variant {
    let mut fill = BTreeMap::new();
    match spec {
        "base" => {}
        "C" => {
            for (k, v) in DEFAULT_KEYS {
                fill.insert(k.to_string(), *v);
            }
        }
        "A" => {
            fill.insert("population".into(), 800.0);
            fill.insert("external_pressure".into(), 15.0);
            fill.insert("treasury".into(), 500.0);
            fill.insert("military_quality".into(), 65.0);
        }
        _ => {
            for part in spec.split('+') {
                let (k, v) = part.split_once('=').unwrap_or_else(|| panic!("bad variant `{}`", part));
                let key = match k {
                    "ep" => "external_pressure",
                    "pop" => "population",
                    "tr" => "treasury",
                    "mq" => "military_quality",
                    other => other,
                };
                fill.insert(key.to_string(), v.parse().unwrap_or_else(|_| panic!("bad value in `{}`", part)));
            }
        }
    }
    Variant { label: spec.to_string(), fill }
}

/// The dependency phase, applied in place to a metric map, with the engine's
/// sequential semantics and the engine's "delta 0.0 does not create the key" rule
/// (`apply_dependency_rule`, `mod.rs:141`). Unlike [`price_dep_metric`] this mutates,
/// because the replica needs the state, not the price.
fn apply_deps_in_place(rules: &[DependencyRule], m: &mut std::collections::HashMap<String, f64>) {
    for rule in rules {
        let from_val = m.get(rule.from.as_str()).copied().unwrap_or(0.0);
        let delta = match rule.mode {
            DependencyMode::Deficit => match rule.threshold {
                Some(t) if from_val < t => -((t - from_val) * rule.coefficient),
                _ => 0.0,
            },
            DependencyMode::Excess => match rule.threshold {
                Some(t) if from_val > t => -((from_val - t) * rule.coefficient),
                _ => 0.0,
            },
            DependencyMode::Bonus => match rule.threshold {
                Some(t) if from_val > t => (from_val - t) * rule.coefficient,
                _ => 0.0,
            },
            DependencyMode::Linear => from_val * rule.coefficient,
            DependencyMode::DeficitProportional => match rule.threshold {
                Some(t) if t > 0.0 && from_val < t => {
                    -(m.get(rule.to.as_str()).copied().unwrap_or(0.0)
                        * rule.coefficient
                        * (t - from_val)
                        / t)
                }
                _ => 0.0,
            },
        };
        if delta != 0.0 {
            // `add_metric` creates the key at 0.0 first — the reason `military_quality`
            // and `treasury` exist at these actors at all
            *m.entry(rule.to.as_str().to_string()).or_insert(0.0) += delta;
        }
    }
}

/// `MetricRef::apply`'s per-metric floor/clamp (`metric_ref.rs:261–271`), which is NOT
/// the same as `clamp_metrics` — it runs at application time and it does create the key.
fn apply_event_delta(m: &mut std::collections::HashMap<String, f64>, key: &str, delta: f64) {
    let cur = m.get(key).copied().unwrap_or(0.0);
    let new = match key {
        "treasury" => cur + delta,
        "economic_output" | "military_size" | "population" => (cur + delta).max(0.0),
        _ => (cur + delta).clamp(0.0, 100.0),
    };
    m.insert(key.to_string(), new);
}

/// `clamp_metrics` (`mod.rs:705`): only clamps keys that already exist.
fn clamp_in_place(m: &mut std::collections::HashMap<String, f64>) {
    for k in ["legitimacy", "cohesion", "military_quality", "economic_output", "external_pressure"] {
        if let Some(v) = m.get_mut(k) {
            *v = v.clamp(0.0, 100.0);
        }
    }
    for k in ["military_size", "population"] {
        if let Some(v) = m.get_mut(k) {
            *v = v.max(0.0);
        }
    }
}

/// `phase_region_ranks` (`mod.rs:353`), for the actor's own rank.
fn apply_rank_bonuses(sc: &Scenario, rank: &engine13::core::RegionRank, m: &mut std::collections::HashMap<String, f64>) {
    for rule in &sc.rank_bonuses {
        if &rule.rank != rank {
            continue;
        }
        for effect in &rule.effects {
            let key = effect.metric.as_str();
            if let Some(floor) = effect.floor {
                let cur = m.get(key).copied().unwrap_or(0.0);
                if cur < floor {
                    m.insert(key.to_string(), floor);
                }
            } else {
                *m.entry(key.to_string()).or_insert(0.0) += effect.delta;
            }
        }
    }
}

#[derive(Default, Clone)]
struct Shadow {
    m: std::collections::HashMap<String, f64>,
    ct: u32,   // its own `collapse_warning_ticks`
    dng: u32,  // ticks it was in danger
    cls: u32,
    int_: u32,
    cq: u32,
    dead: bool,
    dead_tick: i64,
    // relevance: its own history window for the eight metrics
    hist: BTreeMap<String, std::collections::VecDeque<f64>>,
    up_true: u32,
    dec: u32,      // `upheaval ∧ ¬power ∧ ¬contact` — task 21's decisive tick
    // How long a filled key survives its own sinks, and which gates it opens while
    // it lasts — §1 п.4 of the statement asks for the interval in which the fix is
    // neither inert nor fatal, and for `population` / `treasury` / `military_quality`
    // that interval is bounded from ABOVE by the sink, not by a threshold.
    z_pop: i64,    // first tick `population` is back at 0 (−1: never)
    z_mq: i64,     // ...same for `military_quality`
    z_tr300: i64,  // first tick `treasury` falls below the `mercenary_influx` gate
    g_plague: u32, // ticks the `plague` gate `pop > 500` held
    g_merc: u32,   // ticks the `mercenary_influx` gate `treasury > 300` held
    g_desert: u32, // ticks the `desertion` gate `treasury < 200 ∧ military_size > 50` held
    power_t: u32,  // ticks the relevance `power` conjunct held on shadow values
}

#[derive(Default)]
struct Cf25 {
    seen: bool,
    rank: String,
    min_surv: Option<u32>,
    ticks: u32,
    real_ct: u32,
    real_dng: u32,
    r_cls: u32,
    r_int: u32,
    r_cq: u32,
    died_tick: i64,
    died_path: &'static str,
    up_true_r: u32,
    dec_r: u32,
    cw_mismatch: u32,
    rep_mismatch: u32,
    rep_maxdiff: f64,
    ad_hits: u32, // auto_deltas addressed to this actor — must be 0 for the replica
    shadows: Vec<Shadow>,
}

fn decisive25(
    scenario_id: &str,
    ticks: u32,
    seeds: &[u64],
    strategy: Option<&str>,
    variants: &[Variant],
) {
    use engine13::commands::AppState;
    assert_defaults_match_engine();
    assert_pool_matches_source();
    assert_pool_matches_metric("cohesion", COH_EVENTS);

    let labels: Vec<String> = variants.iter().map(|v| v.label.clone()).collect();
    println!("#VARIANTS\t{}", labels.join("\t"));
    println!("actor\tseed\tmode\tticks\trank\tad_hits\trep_mism\trep_maxdiff\tcw_mism\tr_dng\tr_cls\tr_int\tr_cq\tdied\tdied_path\tup_true\tdec\tvariant\ts_dng\ts_cls\ts_int\ts_dead\ts_dead_tick\ts_up\ts_dec\ts_coh\ts_leg\ts_ep\ts_pop\ts_tr\ts_mq\tz_pop\tz_mq\tz_tr300\tg_plague\tg_merc\tg_desert\tpower_t");

    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let spawn_ids: Vec<String> = spawn_configs(&scenario)
            .into_iter()
            .filter(|(_, _, missing, _)| !missing.is_empty())
            .map(|(id, _, _, _)| id)
            .collect();

        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        let sc = state.current_scenario.as_ref().unwrap();
        let rules = sc.dependencies.clone();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool_of(sc)
            .into_iter()
            .map(|(e, c)| (e.id.clone(), (e, c)))
            .collect();
        // which actors an `auto_delta` writes to — the replica claims none of the
        // spawned actors is among them, and this is the check, not the claim
        let ad_targets: Vec<String> = sc
            .auto_deltas
            .iter()
            .filter_map(|ad| match &ad.metric {
                MetricRef::Actor { actor_id, .. } => Some(actor_id.as_str().to_string()),
                _ => None,
            })
            .collect();
        let sc_clone = sc.clone();

        let mut acc: BTreeMap<String, Cf25> = BTreeMap::new();
        let mut victory_tick: i64 = -1;

        for t in 0..ticks {
            {
                let w = state.world_state.as_ref().unwrap();
                for aid in &spawn_ids {
                    if !w.actors.contains_key(aid) || w.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let a = w.actors.get(aid).unwrap();
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.rank = format!("{:?}", a.region_rank);
                        e.min_surv = a.minimum_survival_ticks;
                        e.died_tick = -1;
                        e.ad_hits = ad_targets.iter().filter(|x| *x == aid).count() as u32;
                        for v in variants {
                            let mut m = a.metrics.clone();
                            for (k, val) in &v.fill {
                                m.entry(k.clone()).or_insert(*val);
                            }
                            e.shadows.push(Shadow {
                                m,
                                dead_tick: -1,
                                z_pop: -1,
                                z_mq: -1,
                                z_tr300: -1,
                                ..Default::default()
                            });
                        }
                    }
                }
            }

            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);
            let log_len = state.event_log.events.len();
            {
                let ws = state.world_state.as_mut().unwrap();
                let sr = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(ws, sr, &mut state.event_log, rng);
            }
            if victory_tick < 0 && state.world_state.as_ref().unwrap().victory_achieved {
                victory_tick = (t + 1) as i64;
            }

            // every event write of this tick, resolved to (actor, metric, delta), in
            // log order so the replica applies them in the engine's order
            let mut ev_seq: Vec<(String, String, f64)> = Vec::new();
            for ev in &state.event_log.events[log_len..] {
                let Some((def, _)) = by_id.get(&ev.id) else { continue };
                for (k, _) in DEFAULT_KEYS {
                    for (aid, d) in event_writes_to(def, &ev.actor_id, k) {
                        ev_seq.push((aid, k.to_string(), d));
                    }
                }
            }

            let w = state.world_state.as_ref().unwrap();
            let cur_tick = t;
            let just_dead: BTreeMap<String, std::collections::HashMap<String, f64>> = w
                .dead_actors
                .iter()
                .filter(|d| d.tick_death == cur_tick)
                .map(|d| (d.id.clone(), d.final_metrics.clone()))
                .collect();
            let live_mil: BTreeMap<String, f64> = w
                .actors
                .iter()
                .map(|(k, a)| (k.clone(), a.get_metric("military_size")))
                .collect();
            // `power` and `contact` of `check_relevance_thresholds` — identical in both
            // worlds for these actors (no edges ⇒ `contact` false; `power_projection`
            // reads `military_quality`, so the shadow gets its own value below)
            let max_mil = w
                .actors
                .values()
                .map(|a| a.get_metric("military_size"))
                .fold(1.0_f64, f64::max);
            let avg_pp: f64 = w
                .actors
                .values()
                .map(|a| a.power_projection(1.0, max_mil))
                .sum::<f64>()
                / w.actors.len().max(1) as f64;
            let fg: Vec<String> = w
                .actors
                .iter()
                .filter(|(_, a)| a.narrative_status == engine13::core::NarrativeStatus::Foreground)
                .map(|(id, _)| id.clone())
                .collect();

            let ids: Vec<String> = acc.keys().cloned().collect();
            for aid in ids {
                let alive = w.actors.contains_key(&aid) && !w.dead_actor_ids.contains(&aid);
                let real_m: Option<std::collections::HashMap<String, f64>> = if alive {
                    w.actors.get(&aid).map(|a| a.metrics.clone())
                } else {
                    just_dead.get(&aid).cloned()
                };
                let Some(real_m) = real_m else { continue };
                let rank = w.actors.get(&aid).map(|a| a.region_rank.clone());
                let e = acc.get_mut(&aid).expect("seeded");
                if !e.seen {
                    continue;
                }
                e.ticks += 1;

                let coh = real_m.get("cohesion").copied().unwrap_or(0.0);
                let leg = real_m.get("legitimacy").copied().unwrap_or(0.0);
                let ep = real_m.get("external_pressure").copied().unwrap_or(0.0);
                let mil = real_m.get("military_size").copied().unwrap_or(0.0);

                // no edges ⇒ never besieged; computed anyway so the column is measured
                let besieged = w
                    .actors
                    .get(&aid)
                    .map(|a| {
                        a.neighbors.iter().any(|n| {
                            n.distance == 1
                                && live_mil
                                    .get(&n.id)
                                    .map(|v| *v >= engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY)
                                    .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);
                let skip = matches!(e.min_surv, Some(ms) if cur_tick < ms);

                // ---- real verdict --------------------------------------------
                if !skip {
                    let (rc, ri, rq) = danger_paths(coh, leg, ep, mil, besieged);
                    if rc { e.r_cls += 1 }
                    if ri { e.r_int += 1 }
                    if rq { e.r_cq += 1 }
                    let d = rc || ri || rq;
                    if d { e.real_dng += 1; e.real_ct += 1 } else { e.real_ct = 0 }
                    let engine_ct = w.collapse_warning_ticks.get(&aid).copied().unwrap_or(0);
                    if engine_ct != e.real_ct { e.cw_mismatch += 1; }
                }
                let real_up = upheaval_verdict_multi(w, &aid, &[]);
                let real_soft = coh < 25.0 || leg < 20.0;
                let real_power = w
                    .actors
                    .get(&aid)
                    .map(|a| a.power_projection(1.0, max_mil) > avg_pp * 0.7)
                    .unwrap_or(false);
                let real_contact = fg.iter().filter(|f| **f != aid).any(|f| {
                    w.actors
                        .get(f)
                        .map(|n| n.neighbors.iter().any(|x| x.id == aid && x.distance <= 2))
                        .unwrap_or(false)
                });
                if real_up || real_soft { e.up_true_r += 1; }
                if (real_up || real_soft) && !real_power && !real_contact { e.dec_r += 1; }
                if !alive && e.died_tick < 0 {
                    e.died_tick = (t + 1) as i64;
                    let (dc, di, dq) = danger_paths(coh, leg, ep, mil, besieged);
                    e.died_path = match (dc, di, dq) {
                        (true, _, _) => "classic",
                        (_, true, _) => "internal",
                        (_, _, true) => "conquest",
                        _ => "none",
                    };
                }

                // ---- advance every shadow ------------------------------------
                for (vi, v) in variants.iter().enumerate() {
                    let sh = &mut e.shadows[vi];
                    if sh.dead {
                        continue;
                    }
                    // 1. apply_treasury (phase 1)
                    let inc = sh.m.get("economic_output").copied().unwrap_or(0.0)
                        * sh.m.get("population").copied().unwrap_or(0.0)
                        * 0.001;
                    let exp = sh.m.get("military_size").copied().unwrap_or(0.0) * 0.8;
                    *sh.m.entry("treasury".to_string()).or_insert(0.0) += inc - exp;
                    // 2. rank bonuses (phase 2)
                    if let Some(ref r) = rank {
                        apply_rank_bonuses(&sc_clone, r, &mut sh.m);
                    }
                    // 3. dependency phase (phase 3); no interactions — no edges
                    apply_deps_in_place(&rules, &mut sh.m);
                    // 4. random events (phase 3b), same firing schedule as the real run
                    for (t_aid, key, d) in &ev_seq {
                        if t_aid == &aid {
                            apply_event_delta(&mut sh.m, key, *d);
                        }
                    }
                    // 5. clamps (phase 5)
                    clamp_in_place(&mut sh.m);

                    // replica check: variant `base` must reproduce the engine exactly
                    if v.fill.is_empty() {
                        let mut maxd = 0.0_f64;
                        for (k, _) in DEFAULT_KEYS {
                            let a = sh.m.get(*k).copied().unwrap_or(0.0);
                            let b = real_m.get(*k).copied().unwrap_or(0.0);
                            maxd = maxd.max((a - b).abs());
                        }
                        if maxd > 1e-9 { e.rep_mismatch += 1; }
                        if maxd > e.rep_maxdiff { e.rep_maxdiff = maxd; }
                    }

                    let s_coh = sh.m.get("cohesion").copied().unwrap_or(0.0);
                    let s_leg = sh.m.get("legitimacy").copied().unwrap_or(0.0);
                    let s_ep = sh.m.get("external_pressure").copied().unwrap_or(0.0);
                    let s_mil = sh.m.get("military_size").copied().unwrap_or(0.0);
                    if !skip {
                        let (sc_, si, sq) = danger_paths(s_coh, s_leg, s_ep, s_mil, besieged);
                        if sc_ { sh.cls += 1 }
                        if si { sh.int_ += 1 }
                        if sq { sh.cq += 1 }
                        let d = sc_ || si || sq;
                        if d { sh.dng += 1; sh.ct += 1 } else { sh.ct = 0 }
                        if sh.ct >= 3 && !sh.dead {
                            sh.dead = true;
                            sh.dead_tick = (t + 1) as i64;
                        }
                    }
                    // relevance on the shadow's own five-slot history
                    for (k, _) in DEFAULT_KEYS {
                        let h = sh.hist.entry(k.to_string()).or_default();
                        h.push_back(sh.m.get(*k).copied().unwrap_or(0.0));
                        while h.len() > 5 { h.pop_front(); }
                    }
                    let mut moved = false;
                    for (k, _) in DEFAULT_KEYS {
                        if let Some(h) = sh.hist.get(*k) {
                            if h.len() >= 2 {
                                let d = h.back().copied().unwrap_or(0.0) - h.front().copied().unwrap_or(0.0);
                                if d.abs() > 30.0 { moved = true; }
                            }
                        }
                    }
                    let s_up = moved || s_coh < 25.0 || s_leg < 20.0;
                    if s_up { sh.up_true += 1; }
                    // `power` recomputed with the shadow's `military_quality`, which is
                    // 0.35 of `power_projection` (`core/actor.rs:179`) and the reason
                    // task 21 §4 could not price variant (A) without measuring
                    // `power_projection` (`core/actor.rs:171`), reproduced on the
                    // shadow's own values: `military_quality` carries 0.35 of it and
                    // `treasury` another 0.20 through a 500-cap, so TWO of the four
                    // missing keys move this verdict — the point task 21 §4 left as an
                    // estimate.
                    let s_pp = {
                        let ms = if max_mil > 0.0 { (s_mil / max_mil).clamp(0.0, 1.0) } else { 0.0 };
                        let mq = (sh.m.get("military_quality").copied().unwrap_or(0.0) / 100.0).clamp(0.0, 1.0);
                        let tr = (sh.m.get("treasury").copied().unwrap_or(0.0) / 500.0).clamp(0.0, 1.0);
                        (ms * 0.45 + mq * 0.35 + tr * 0.20) * 100.0
                    };
                    let s_power = s_pp > avg_pp * 0.7;
                    if s_power { sh.power_t += 1; }
                    if s_up && !s_power && !real_contact { sh.dec += 1; }

                    // how long each filled key lasts, and what it opens meanwhile
                    let s_pop = sh.m.get("population").copied().unwrap_or(0.0);
                    let s_tr = sh.m.get("treasury").copied().unwrap_or(0.0);
                    let s_mq = sh.m.get("military_quality").copied().unwrap_or(0.0);
                    if sh.z_pop < 0 && v.fill.contains_key("population") && s_pop <= 1e-9 {
                        sh.z_pop = (t + 1) as i64;
                    }
                    if sh.z_mq < 0 && v.fill.contains_key("military_quality") && s_mq <= 1e-9 {
                        sh.z_mq = (t + 1) as i64;
                    }
                    if sh.z_tr300 < 0 && v.fill.contains_key("treasury") && s_tr <= 300.0 {
                        sh.z_tr300 = (t + 1) as i64;
                    }
                    if s_pop > 500.0 { sh.g_plague += 1; }
                    if s_tr > 300.0 { sh.g_merc += 1; }
                    if s_tr < 200.0 && s_mil > 50.0 { sh.g_desert += 1; }
                }

                if !alive {
                    // the real actor is gone; its shadows keep running only until the
                    // horizon, but nothing else in the world observes them
                    for sh in e.shadows.iter_mut() {
                        if !sh.dead {
                            // leave alive: that IS the counterfactual answer
                        }
                    }
                }
            }
        }

        for (aid, e) in &acc {
            for (vi, v) in variants.iter().enumerate() {
                let sh = &e.shadows[vi];
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2e}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.1}\t{:.1}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    aid, seed, mode_label(strategy), e.ticks, e.rank, e.ad_hits,
                    e.rep_mismatch, e.rep_maxdiff, e.cw_mismatch,
                    e.real_dng, e.r_cls, e.r_int, e.r_cq, e.died_tick, e.died_path,
                    e.up_true_r, e.dec_r,
                    v.label, sh.dng, sh.cls, sh.int_, sh.dead, sh.dead_tick, sh.up_true, sh.dec,
                    sh.m.get("cohesion").copied().unwrap_or(0.0),
                    sh.m.get("legitimacy").copied().unwrap_or(0.0),
                    sh.m.get("external_pressure").copied().unwrap_or(0.0),
                    sh.m.get("population").copied().unwrap_or(0.0),
                    sh.m.get("treasury").copied().unwrap_or(0.0),
                    sh.m.get("military_quality").copied().unwrap_or(0.0),
                    sh.z_pop, sh.z_mq, sh.z_tr300,
                    sh.g_plague, sh.g_merc, sh.g_desert, sh.power_t,
                );
            }
        }
        println!("#CF25\tseed={}\tmode={}\tvictory_tick={}", seed, mode_label(strategy), victory_tick);
    }
}


/// `poolcut` — how much of the pool has to be taken away before a death stops
/// happening.
///
/// Task 24's counterfactual is the single point `α = 1` (remove the pool entirely);
/// it saves 99 of 243. The number §7 of task 25 needs is different: the fix under
/// discussion does not remove the pool, it *dilutes* it — three actors that die at
/// ticks 40 / 275 / … would live to 300, `EventTarget::Any` divides the same number of
/// draws over more actors, and everyone else's share falls by a measurable percentage.
/// So the question is the MARGIN: at which α does each death start being prevented?
///
/// Same shadow as `decisive24` (real delta minus what the pool wrote, own clamp, own
/// `collapse_warning_ticks`, cross-checked against the engine), evaluated at several α
/// at once. `α = 1` must reproduce `decisive24` exactly — that is the mode's own test.
#[derive(Default, Clone)]
struct PcShadow {
    coh: f64,
    ct: u32,
}

#[derive(Default)]
struct PcAcc {
    seen: bool,
    min_surv: Option<u32>,
    neighbors: Vec<(String, u32)>,
    prev_coh: Option<f64>,
    real_ct: u32,
    cw_mismatch: u32,
    died: bool,
    sh: Vec<PcShadow>,
}

fn poolcut(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>, alphas: &[f64]) {
    use engine13::commands::AppState;
    assert_pool_matches_source();
    assert_pool_matches_metric("cohesion", COH_EVENTS);

    println!("#ALPHAS\t{}", alphas.iter().map(|a| format!("{}", a)).collect::<Vec<_>>().join("\t"));
    println!("actor\tseed\tmode\tdied_tick\tdied_path\talpha\tsaved\tcw_mism");

    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };
        let sc = state.current_scenario.as_ref().unwrap();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool_of(sc)
            .into_iter()
            .map(|(e, c)| (e.id.clone(), (e, c)))
            .collect();
        let mut acc: BTreeMap<String, PcAcc> = BTreeMap::new();

        for t in 0..ticks {
            {
                let w = state.world_state.as_ref().unwrap();
                for (aid, a) in w.actors.iter() {
                    if w.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.min_surv = a.minimum_survival_ticks;
                        e.neighbors = a.neighbors.iter().map(|n| (n.id.clone(), n.distance)).collect();
                        e.prev_coh = Some(a.get_metric("cohesion"));
                        e.sh = alphas
                            .iter()
                            .map(|_| PcShadow { coh: a.get_metric("cohesion"), ..Default::default() })
                            .collect();
                    }
                }
            }
            let (_a, _r) = scripted_step(&mut state, scenario_id, strategy);
            let log_len = state.event_log.events.len();
            {
                let ws = state.world_state.as_mut().unwrap();
                let sr = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(ws, sr, &mut state.event_log, rng);
            }
            let mut coh_pool: BTreeMap<String, f64> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                let Some((def, is_common)) = by_id.get(&ev.id) else { continue };
                if !*is_common {
                    continue;
                }
                for (aid, d) in event_writes_to(def, &ev.actor_id, "cohesion") {
                    *coh_pool.entry(aid).or_insert(0.0) += d;
                }
            }
            let w = state.world_state.as_ref().unwrap();
            let just_dead: BTreeMap<String, std::collections::HashMap<String, f64>> = w
                .dead_actors
                .iter()
                .filter(|d| d.tick_death == t)
                .map(|d| (d.id.clone(), d.final_metrics.clone()))
                .collect();
            let live_mil: BTreeMap<String, f64> = w
                .actors
                .iter()
                .map(|(k, a)| (k.clone(), a.get_metric("military_size")))
                .collect();
            let ids: Vec<String> = acc.keys().cloned().collect();
            for aid in ids {
                let alive = w.actors.contains_key(&aid) && !w.dead_actor_ids.contains(&aid);
                let m = if alive {
                    w.actors.get(&aid).map(|a| a.metrics.clone())
                } else {
                    just_dead.get(&aid).cloned()
                };
                let Some(m) = m else { continue };
                let e = acc.get_mut(&aid).expect("seeded");
                if !e.seen || e.died {
                    continue;
                }
                let coh = m.get("cohesion").copied().unwrap_or(0.0);
                let leg = m.get("legitimacy").copied().unwrap_or(0.0);
                let ep = m.get("external_pressure").copied().unwrap_or(0.0);
                let mil = m.get("military_size").copied().unwrap_or(0.0);
                let d_coh = coh - e.prev_coh.unwrap_or(coh);
                e.prev_coh = Some(coh);
                let ev_c = coh_pool.get(&aid).copied().unwrap_or(0.0);
                let besieged = e.neighbors.iter().any(|(nid, dist)| {
                    *dist == 1
                        && live_mil
                            .get(nid)
                            .map(|v| *v >= engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY)
                            .unwrap_or(false)
                });
                let skip = matches!(e.min_surv, Some(ms) if t < ms);
                let (rc, ri, rq) = danger_paths(coh, leg, ep, mil, besieged);
                if !skip {
                    if rc || ri || rq { e.real_ct += 1 } else { e.real_ct = 0 }
                    let engine_ct = w.collapse_warning_ticks.get(&aid).copied().unwrap_or(0);
                    if engine_ct != e.real_ct { e.cw_mismatch += 1; }
                }
                for (ai, alpha) in alphas.iter().enumerate() {
                    let s = &mut e.sh[ai];
                    s.coh = (s.coh + d_coh - alpha * ev_c).clamp(0.0, 100.0);
                    if !skip {
                        let (sc_, si, _) = danger_paths(s.coh, leg, ep, mil, besieged);
                        if sc_ || si || rq { s.ct += 1 } else { s.ct = 0 }
                    }
                }
                if !alive {
                    e.died = true;
                    let path = match (rc, ri, rq) {
                        (true, _, _) => "classic",
                        (_, true, _) => "internal",
                        (_, _, true) => "conquest",
                        _ => "none",
                    };
                    for (ai, alpha) in alphas.iter().enumerate() {
                        let s = &e.sh[ai];
                        println!(
                            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                            aid, seed, mode_label(strategy), t + 1, path, alpha,
                            (s.ct < 3) as u8, e.cw_mismatch
                        );
                    }
                }
            }
        }
    }
}

// ===========================================================================
// task 24 stage 2 — (D₂), event addressing: `evaddr` and `evtarget`
// ===========================================================================
//
// `(D₂)` is a claim about ADDRESSING, and the statement (§7.1) is explicit that
// the claim is not "an effect is addressed to someone other than the target" —
// that is the idiom of every scenario event in the project. The claim is that
// `mehmed_threatens` is the only event whose target receives NOTHING from its own
// effect vector, while its firing frequency is controlled entirely by that same
// non-receiver.
//
// Three slots carry addressing, and they are not the same slot:
//   * `target`     — eligibility and attribution (`mod.rs:452–470`);
//   * `conditions` — frequency, resolved through `RelativeMetricRef::resolve`
//                    against the target (`mod.rs:485–492`);
//   * `effects`    — the write, resolved through the same call (`mod.rs:492`).
// A key is bound to the target only when it is written `self.<metric>`; anything
// else is `Absolute` and ignores the target by construction
// (`core/metric_ref.rs:404–413`).
//
// `evaddr` walks all four event containers of the project and classifies every
// key of every event by that resolution, so "unique" is a counted number and not
// an impression from reading the file.

/// How one metric key addresses the world, relative to the actor an event fires on.
/// Classified through `RelativeMetricRef::resolve` — the engine's own call — so a
/// key that *looks* absolute but names the target still lands in `SameActor`.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Addr {
    /// `self.<metric>` — binds to whoever the event fires on, by construction.
    SelfRel,
    /// Absolute, and names the target actor.
    SameActor,
    /// Absolute, and names a different actor.
    OtherActor,
    /// `global:<key>` — addresses no actor at all.
    Global,
    /// `family:<key>`.
    Family,
}

fn classify_addr(r: &RelativeMetricRef, target: Option<&str>) -> Addr {
    match r {
        RelativeMetricRef::SelfRelative(_) => Addr::SelfRel,
        RelativeMetricRef::Absolute(m) => match m {
            MetricRef::Actor { actor_id, .. } => match target {
                Some(t) if actor_id.as_str() == t => Addr::SameActor,
                // A drawn target (`Any`/`SeaActors`/`All`) can coincide with the named
                // actor on some draws; counted as `OtherActor` and reported separately
                // so the coincidence is never silently folded into "addresses target".
                _ => Addr::OtherActor,
            },
            MetricRef::Global { .. } => Addr::Global,
            MetricRef::Family { .. } => Addr::Family,
        },
    }
}

struct AddrRow {
    scenario: String,
    id: String,
    common: bool,
    target_kind: &'static str,
    target_id: String,
    eff: [u32; 5],  // SelfRel, SameActor, OtherActor, Global, Family
    cond: [u32; 5],
    /// no effect reaches the target: neither `self.` nor an absolute key naming it
    empty_intersect: bool,
    /// a condition reads a DIFFERENT actor — the structural half of `(D₂)`
    gate_other_actor: bool,
    /// the event is gated, but only on non-actor scope (`family:` / `global:`),
    /// which is a different class and must not be counted with the above
    gate_nonactor_only: bool,
    /// the effect vector names actors other than the target
    other_actors: Vec<String>,
    /// the condition vector names actors other than the target
    cond_actors: Vec<String>,
}

fn addr_rows_of(scenario_label: &str, events: &[(engine13::core::RandomEvent, bool)]) -> Vec<AddrRow> {
    let mut out = Vec::new();
    for (ev, common) in events {
        let (kind, target): (&'static str, Option<String>) = match &ev.target {
            engine13::core::EventTarget::Actor(id) => ("Actor", Some(id.clone())),
            engine13::core::EventTarget::Any => ("Any", None),
            engine13::core::EventTarget::SeaActors => ("SeaActors", None),
            engine13::core::EventTarget::All => ("All", None),
        };
        let t = target.as_deref();
        let mut eff = [0u32; 5];
        let mut cond = [0u32; 5];
        let mut others: Vec<String> = Vec::new();
        for k in ev.effects.keys() {
            let a = classify_addr(k, t);
            eff[a as usize] += 1;
            if a == Addr::OtherActor {
                if let RelativeMetricRef::Absolute(MetricRef::Actor { actor_id, .. }) = k {
                    others.push(actor_id.as_str().to_string());
                }
            }
        }
        let mut cond_others: Vec<String> = Vec::new();
        for c in &ev.conditions {
            let a = classify_addr(&c.metric, t);
            cond[a as usize] += 1;
            if a == Addr::OtherActor {
                if let RelativeMetricRef::Absolute(MetricRef::Actor { actor_id, metric }) = &c.metric {
                    cond_others.push(format!(
                        "{}.{}{}{}",
                        actor_id.as_str(), metric.as_str(), op_str(&c.operator), c.value
                    ));
                }
            }
        }
        others.sort();
        others.dedup();
        // "reaches the target" = at least one effect is self-relative or names it
        let empty_intersect = eff[Addr::SelfRel as usize] == 0 && eff[Addr::SameActor as usize] == 0;
        let gate_other_actor = cond[Addr::OtherActor as usize] > 0;
        let gate_nonactor_only = !ev.conditions.is_empty()
            && cond[Addr::SelfRel as usize] == 0
            && cond[Addr::SameActor as usize] == 0
            && cond[Addr::OtherActor as usize] == 0;
        out.push(AddrRow {
            scenario: scenario_label.to_string(),
            id: ev.id.clone(),
            common: *common,
            target_kind: kind,
            target_id: target.unwrap_or_else(|| "<drawn>".to_string()),
            eff,
            cond,
            empty_intersect,
            gate_other_actor,
            gate_nonactor_only,
            other_actors: others,
            cond_actors: cond_others,
        });
    }
    out
}

/// §7.4 п.2 — the addressing walk over all four event containers of the project.
fn evaddr() {
    println!("scenario\tevent\tpool\ttarget_kind\ttarget\tp\teff_self\teff_same\teff_other\teff_glob\teff_fam\tcond_self\tcond_same\tcond_other\tcond_glob\tcond_fam\tempty_isect\tgate_other_actor\tgate_nonactor\tother_actors");
    let mut rows: Vec<AddrRow> = Vec::new();
    let common: Vec<(engine13::core::RandomEvent, bool)> = engine13::events::common_events()
        .into_iter()
        .map(|e| (e, true))
        .collect();
    rows.extend(addr_rows_of("common", &common));
    let mut probs: BTreeMap<String, f64> = BTreeMap::new();
    for (e, _) in &common {
        probs.insert(e.id.clone(), e.probability);
    }
    for sid in ["constantinople_1430", "rome_375", "milan_1477"] {
        let sc = registry::load_by_id(sid).expect("scenario");
        let scen: Vec<(engine13::core::RandomEvent, bool)> =
            sc.random_events.iter().cloned().map(|e| (e, false)).collect();
        for (e, _) in &scen {
            probs.insert(format!("{}::{}", sid, e.id), e.probability);
        }
        rows.extend(addr_rows_of(sid, &scen));
    }
    let mut n_empty = 0u32;
    let mut n_gate_off = 0u32;
    let mut n_both = 0u32;
    let mut n_self_keys = 0u32;
    let mut n_abs_actor_keys = 0u32;
    for r in &rows {
        let p = probs
            .get(&format!("{}::{}", r.scenario, r.id))
            .or_else(|| probs.get(&r.id))
            .copied()
            .unwrap_or(f64::NAN);
        println!(
            "{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.scenario, r.id, if r.common { "pool" } else { "scen" },
            r.target_kind, r.target_id, p,
            r.eff[0], r.eff[1], r.eff[2], r.eff[3], r.eff[4],
            r.cond[0], r.cond[1], r.cond[2], r.cond[3], r.cond[4],
            r.empty_intersect as u8, r.gate_other_actor as u8, r.gate_nonactor_only as u8,
            if r.other_actors.is_empty() { "-".to_string() } else { r.other_actors.join(",") }
        );
        if r.empty_intersect { n_empty += 1; }
        if r.gate_other_actor { n_gate_off += 1; }
        if r.empty_intersect && r.gate_other_actor { n_both += 1; }
        n_self_keys += r.eff[0] + r.cond[0];
        n_abs_actor_keys += r.eff[1] + r.eff[2] + r.cond[1] + r.cond[2];
    }
    println!(
        "#ADDRSUM\tevents={}\tempty_isect={}\tgate_other_actor={}\tboth={}\tself_keys={}\tabs_actor_keys={}",
        rows.len(), n_empty, n_gate_off, n_both, n_self_keys, n_abs_actor_keys
    );
    for r in &rows {
        if r.empty_intersect || r.gate_other_actor {
            println!(
                "#ADDRFLAG\t{}\t{}\tempty_isect={}\tgate_other_actor={}\ttarget={}\tother_actors={}",
                r.scenario, r.id, r.empty_intersect as u8, r.gate_other_actor as u8, r.target_id,
                if r.other_actors.is_empty() { "-".to_string() } else { r.other_actors.join(",") }
            );
            println!(
                "#ADDRGATE\t{}\t{}\tconds_on_other_actors={}",
                r.scenario, r.id,
                if r.cond_actors.is_empty() { "-".to_string() } else { r.cond_actors.join(",") }
            );
        }
    }
}

/// Which actor an event's write to a GLOBAL key lands on — there is no actor, so
/// this returns just the delta. Separate from [`event_writes_to`] because the
/// resolution branch is different (`MetricRef::Global`) and `federation_progress`
/// is the first conjunct of `constantinople_1430`'s victory condition.
fn event_writes_global(ev: &engine13::core::RandomEvent, target_id: &str, key: &str) -> f64 {
    let mut sum = 0.0;
    for (m, delta) in &ev.effects {
        if let Ok(MetricRef::Global { key: k }) = m.resolve(target_id) {
            if k.as_str() == key {
                sum += *delta;
            }
        }
    }
    sum
}

/// The four addressing variants of §7.6, plus the self-test.
///
/// `Base` and `Cut` exist to validate the shadow, not to answer the question:
/// `Base` must produce a shadow identical to the real world (zero divergence), and
/// `Cut` must reproduce `decisive24` exactly — same 90 runs, same `saved`.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Redirect {
    /// shadow == real; floor test
    Base,
    /// remove every event write to `cohesion` — task 24's shadow, re-derived here
    Cut,
    /// (i) the cohesion write of `mehmed_threatens` follows its gate onto `ottomans`
    I,
    /// (i′) the whole actor vector follows the gate: cohesion AND external_pressure
    IPrime,
}

fn redirect_label(r: Redirect) -> &'static str {
    match r {
        Redirect::Base => "base",
        Redirect::Cut => "cut",
        Redirect::I => "i",
        Redirect::IPrime => "i_prime",
    }
}

const REDIRECTS: [Redirect; 4] = [Redirect::Base, Redirect::Cut, Redirect::I, Redirect::IPrime];

/// Candidate gates for variant (ii) — "the gate follows the effect onto the victim".
/// The statement forbids picking one by tuning, so the probe measures occupancy of a
/// declared family and reports it; the choice is stage 2's, on these numbers.
const II_CANDIDATES: &[(&str, &str, f64, bool)] = &[
    ("byz.military_size<30", "military_size", 30.0, true),
    ("byz.military_size<40", "military_size", 40.0, true),
    ("byz.military_size<50", "military_size", 50.0, true),
    ("byz.military_size<60", "military_size", 60.0, true),
    ("byz.cohesion<40", "cohesion", 40.0, true),
    ("byz.cohesion<50", "cohesion", 50.0, true),
    ("byz.legitimacy<40", "legitimacy", 40.0, true),
    ("byz.external_pressure>70", "external_pressure", 70.0, false),
    ("byz.external_pressure>80", "external_pressure", 80.0, false),
];

#[derive(Default, Clone)]
struct EtActor {
    seen: bool,
    ticks: u32,
    min_surv: Option<u32>,
    neighbors: Vec<(String, u32)>,
    prev_coh: Option<f64>,
    prev_ep: Option<f64>,
    s_coh: [f64; 4],
    /// (i′) alone moves `external_pressure` too, and `external_pressure` is a
    /// conjunct of `classic_collapse`; so that variant needs its own pressure
    /// shadow or it silently degenerates into (i).
    s_ep: f64,
    real_ct: u32,
    shad_ct: [u32; 4],
    saved: [u32; 4],
    added: [u32; 4],
    shadow_dead: [bool; 4],
    shadow_dead_tick: [i64; 4],
    /// which shadow path was true when the variant's death was recorded — an added
    /// death can only come through a path that reads the shadowed metric, and
    /// criterion §7.7 п.3 is a share of exactly those paths
    added_path: [&'static str; 4],
    died_tick: i64,
    died_path: &'static str,
    cw_mismatch: u32,
    /// times this actor was the drawn victim of an `EventTarget::Any` event
    any_draws: u32,
    any_draws_before_ott_death: u32,
}

/// §7.4 п.3–п.5 — the four channels, the redirect shadow, and the envelopes the
/// shadow provably cannot compute.
fn evtarget(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    assert_pool_matches_source();
    assert_pool_matches_metric("cohesion", COH_EVENTS);

    println!("actor\tseed\tmode\tticks\tdied\tpath\tsaved_base\tsaved_cut\tsaved_i\tsaved_ip\tadded_base\tadded_cut\tadded_i\tadded_ip\ts_coh_i\ts_coh_ip\tcw_mism\tany_draws");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        let sc = state.current_scenario.as_ref().unwrap();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool_of(sc)
            .into_iter()
            .map(|(e, c)| (e.id.clone(), (e, c)))
            .collect();
        let any_ids: std::collections::HashSet<String> = pool_of(sc)
            .into_iter()
            .filter(|(e, _)| matches!(e.target, engine13::core::EventTarget::Any))
            .map(|(e, _)| e.id)
            .collect();
        let victory = sc.victory_condition.clone();
        let vic_key: Option<MetricRef> = victory.as_ref().map(|v| v.metric.clone());
        let vic_thresh = victory.as_ref().map(|v| v.threshold).unwrap_or(0.0);

        let mut acc: BTreeMap<String, EtActor> = BTreeMap::new();
        let mut victory_tick: i64 = -1;
        let mut ott_death: i64 = -1;
        let mut byz_death: i64 = -1;
        let mut collapses = 0u32;

        // --- federation_progress accounting -------------------------------------
        let mut fed_nominal: BTreeMap<String, f64> = BTreeMap::new(); // by event id
        let mut fed_nominal_total = 0.0f64;
        let mut fed_realized_total = 0.0f64;
        let mut fed_prev = 0.0f64;
        let mut fed_first_80: i64 = -1;
        let mut fed_ticks_80 = 0u32;
        let mut fed_max = 0.0f64;
        let mut fed_at_100 = 0u32;

        // --- envelopes ----------------------------------------------------------
        let mut gate_true = 0u32;            // ottomans.military_size > 150
        let mut gate_true_alive = 0u32;      // ...and the target was eligible
        let mut elig_now = 0u32;             // ottomans in the world (target of record)
        let mut elig_iii = 0u32;             // byzantium in the world (target under (iii))
        let mut env_iii_gain = 0u32;         // gate held, byz alive, ott NOT alive
        let mut env_iii_loss = 0u32;         // gate held, ott alive, byz NOT alive
        let mut ii_occ = [0u32; II_CANDIDATES.len()];
        let mut fires_mehmed = 0u32;
        let mut fires_greek = 0u32;
        let mut greek_gate_true = 0u32;
        let mut greek_gate_shadow_true = 0u32; // without mehmed's +10 on byz.ep
        let mut greek_gate_flip = 0u32;
        let mut byz_ep_shadow = f64::NAN;     // (i′) shadow of byzantium.external_pressure
        let mut byz_ep_prev = f64::NAN;
        let mut fg_size_sum = 0u64;
        let mut fg_ticks = 0u32;
        let mut live_ticks = 0u32;

        for t in 0..ticks {
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.min_surv = a.minimum_survival_ticks;
                        e.neighbors = a.neighbors.iter().map(|n| (n.id.clone(), n.distance)).collect();
                        let c = a.get_metric("cohesion");
                        e.s_coh = [c, c, c, c];
                        e.prev_coh = Some(c);
                        e.s_ep = a.get_metric("external_pressure");
                        e.prev_ep = Some(a.get_metric("external_pressure"));
                        e.died_tick = -1;
                        e.shadow_dead_tick = [-1, -1, -1, -1];
                    }
                }
                // eligibility and gate occupancy, measured before the tick runs
                let ott_alive = world.actors.contains_key("ottomans")
                    && !world.dead_actor_ids.contains("ottomans");
                let byz_alive = world.actors.contains_key("byzantium")
                    && !world.dead_actor_ids.contains("byzantium");
                // the gate is `MetricRef::get`, i.e. `unwrap_or(0.0)` for a dead actor —
                // read exactly as the engine reads it, not from the live actor map
                let ott_mil = MetricRef::literal("actor:ottomans.military_size").get(world);
                let g = ott_mil > 150.0;
                if g { gate_true += 1; }
                if g && ott_alive { gate_true_alive += 1; }
                if ott_alive { elig_now += 1; }
                if byz_alive { elig_iii += 1; }
                if g && byz_alive && !ott_alive { env_iii_gain += 1; }
                if g && ott_alive && !byz_alive { env_iii_loss += 1; }
                if byz_alive {
                    live_ticks += 1;
                    for (i, (_, m, v, less)) in II_CANDIDATES.iter().enumerate() {
                        let x = MetricRef::literal(&format!("actor:byzantium.{}", m)).get(world);
                        if (*less && x < *v) || (!*less && x > *v) {
                            ii_occ[i] += 1;
                        }
                    }
                    let ep = MetricRef::literal("actor:byzantium.external_pressure").get(world);
                    if byz_ep_shadow.is_nan() {
                        byz_ep_shadow = ep;
                        byz_ep_prev = ep;
                    }
                    let real_g = ep > 70.0;
                    let shad_g = byz_ep_shadow > 70.0;
                    if real_g { greek_gate_true += 1; }
                    if shad_g { greek_gate_shadow_true += 1; }
                    if real_g != shad_g { greek_gate_flip += 1; }
                }
                let fg = world
                    .actors
                    .values()
                    .filter(|a| {
                        a.narrative_status == engine13::core::NarrativeStatus::Foreground
                            && !world.dead_actor_ids.contains(&a.id)
                    })
                    .count();
                fg_size_sum += fg as u64;
                fg_ticks += 1;
            }

            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);

            let log_len = state.event_log.events.len();
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }
            if victory_tick < 0 && state.world_state.as_ref().unwrap().victory_achieved {
                victory_tick = (t + 1) as i64;
            }

            // ---- what fired, and what each variant would have written -----------
            // `delta[v][actor]` = (what variant v writes) − (what the run wrote).
            let mut delta: Vec<BTreeMap<String, f64>> = vec![BTreeMap::new(); REDIRECTS.len()];
            let mut ep_delta: BTreeMap<String, f64> = BTreeMap::new(); // (i′) only
            let mut byz_ep_delta = 0.0f64;
            for ev in &state.event_log.events[log_len..] {
                let Some((def, _is_common)) = by_id.get(&ev.id) else { continue };
                if any_ids.contains(&ev.id) {
                    let e = acc.entry(ev.actor_id.clone()).or_default();
                    e.any_draws += 1;
                    if ott_death < 0 { e.any_draws_before_ott_death += 1; }
                }
                let coh_writes = event_writes_to(def, &ev.actor_id, "cohesion");
                for (v, r) in REDIRECTS.iter().enumerate() {
                    match r {
                        Redirect::Base => {}
                        Redirect::Cut => {
                            for (aid, d) in &coh_writes {
                                *delta[v].entry(aid.clone()).or_insert(0.0) -= *d;
                            }
                        }
                        Redirect::I | Redirect::IPrime => {
                            if ev.id == "mehmed_threatens" {
                                for (aid, d) in &coh_writes {
                                    *delta[v].entry(aid.clone()).or_insert(0.0) -= *d;
                                    *delta[v].entry("ottomans".to_string()).or_insert(0.0) += *d;
                                }
                            }
                        }
                    }
                }
                if ev.id == "mehmed_threatens" {
                    fires_mehmed += 1;
                    // (i′) also moves the pressure write off byzantium and onto the
                    // actor the gate reads
                    for (aid, d) in event_writes_to(def, &ev.actor_id, "external_pressure") {
                        if aid == "byzantium" {
                            byz_ep_delta -= d;
                        }
                        *ep_delta.entry(aid.clone()).or_insert(0.0) -= d;
                        *ep_delta.entry("ottomans".to_string()).or_insert(0.0) += d;
                    }
                }
                if ev.id == "greek_scholars_flee" {
                    fires_greek += 1;
                }
                let g = event_writes_global(def, &ev.actor_id, "federation_progress");
                if g != 0.0 {
                    *fed_nominal.entry(ev.id.clone()).or_insert(0.0) += g;
                    fed_nominal_total += g;
                }
            }

            let world = state.world_state.as_ref().unwrap();
            let cur_tick = t;

            // ---- federation_progress, realized against nominal -------------------
            if let Some(ref k) = vic_key {
                let v = k.get(world);
                fed_realized_total += v - fed_prev;
                fed_prev = v;
                if v > fed_max { fed_max = v; }
                if v >= 100.0 { fed_at_100 += 1; }
                if v >= vic_thresh {
                    fed_ticks_80 += 1;
                    if fed_first_80 < 0 { fed_first_80 = (t + 1) as i64; }
                }
            }

            // ---- byzantium's shadow pressure (variant i′) ------------------------
            if let Some(a) = world.actors.get("byzantium") {
                let ep = a.get_metric("external_pressure");
                if !byz_ep_shadow.is_nan() {
                    let d = ep - byz_ep_prev;
                    byz_ep_shadow = (byz_ep_shadow + d + byz_ep_delta).clamp(0.0, 100.0);
                }
                byz_ep_prev = ep;
            }

            let just_dead: BTreeMap<String, &std::collections::HashMap<String, f64>> = world
                .dead_actors
                .iter()
                .filter(|d| d.tick_death == cur_tick)
                .map(|d| (d.id.clone(), &d.final_metrics))
                .collect();
            let live_mil: BTreeMap<String, f64> = world
                .actors
                .iter()
                .map(|(k, a)| (k.clone(), a.get_metric("military_size")))
                .collect();

            let ids: Vec<String> = acc.keys().cloned().collect();
            for aid in ids {
                let alive = world.actors.contains_key(&aid) && !world.dead_actor_ids.contains(&aid);
                let metrics: Option<std::collections::HashMap<String, f64>> = if alive {
                    world.actors.get(&aid).map(|a| a.metrics.clone())
                } else {
                    just_dead.get(&aid).map(|m| (*m).clone())
                };
                let Some(m) = metrics else { continue };
                let e = acc.get_mut(&aid).expect("seeded");
                if !e.seen { continue; }

                let coh = m.get("cohesion").copied().unwrap_or(0.0);
                let leg = m.get("legitimacy").copied().unwrap_or(0.0);
                let ep = m.get("external_pressure").copied().unwrap_or(0.0);
                let mil = m.get("military_size").copied().unwrap_or(0.0);

                let d_coh = coh - e.prev_coh.unwrap_or(coh);
                for (v, dmap) in delta.iter().enumerate() {
                    let dv = dmap.get(&aid).copied().unwrap_or(0.0);
                    e.s_coh[v] = (e.s_coh[v] + d_coh + dv).clamp(0.0, 100.0);
                }
                let d_ep = ep - e.prev_ep.unwrap_or(ep);
                e.s_ep = (e.s_ep + d_ep + ep_delta.get(&aid).copied().unwrap_or(0.0)).clamp(0.0, 100.0);
                e.prev_coh = Some(coh);
                e.prev_ep = Some(ep);
                e.ticks += 1;

                let besieged = e.neighbors.iter().any(|(nid, dist)| {
                    *dist == 1
                        && live_mil
                            .get(nid)
                            .map(|v| *v >= engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY)
                            .unwrap_or(false)
                });
                let skip = matches!(e.min_surv, Some(ms) if cur_tick < ms);
                if !skip {
                    let (rc, ri, rq) = danger_paths(coh, leg, ep, mil, besieged);
                    let real_d = rc || ri || rq;
                    if real_d { e.real_ct += 1 } else { e.real_ct = 0 }
                    let engine_ct = world.collapse_warning_ticks.get(&aid).copied().unwrap_or(0);
                    if engine_ct != e.real_ct { e.cw_mismatch += 1; }
                    for (v, rv) in REDIRECTS.iter().enumerate() {
                        // only (i′) shadows pressure; the others read the real value, so
                        // their `classic` conjunct is the engine's own
                        let ep_v = if *rv == Redirect::IPrime { e.s_ep } else { ep };
                        let (sc_, si, sq) = danger_paths(e.s_coh[v], leg, ep_v, mil, besieged);
                        let shad_d = sc_ || si || if *rv == Redirect::IPrime { sq } else { rq };
                        if shad_d { e.shad_ct[v] += 1 } else { e.shad_ct[v] = 0 }
                        // a shadow that reaches three consecutive dangerous ticks while
                        // the real actor is still alive is a death the variant ADDS
                        if e.shad_ct[v] >= 3 && alive && !e.shadow_dead[v] {
                            e.shadow_dead[v] = true;
                            e.shadow_dead_tick[v] = (t + 1) as i64;
                            e.added[v] += 1;
                            e.added_path[v] = match (sc_, si, rq) {
                                (true, _, _) => "classic",
                                (_, true, _) => "internal",
                                (_, _, true) => "conquest",
                                _ => "none",
                            };
                        }
                    }
                }
                if !alive && e.died_tick < 0 {
                    e.died_tick = (t + 1) as i64;
                    collapses += 1;
                    if aid == "ottomans" { ott_death = e.died_tick; }
                    if aid == "byzantium" { byz_death = e.died_tick; }
                    let (dc, di, dq) = danger_paths(coh, leg, ep, mil, besieged);
                    e.died_path = match (dc, di, dq) {
                        (true, _, _) => "classic",
                        (_, true, _) => "internal",
                        (_, _, true) => "conquest",
                        _ => "none",
                    };
                    for v in 0..REDIRECTS.len() {
                        if e.shad_ct[v] < 3 { e.saved[v] += 1; }
                    }
                }
            }
        }

        let mode = mode_label(strategy);
        let mut tot_saved = [0u32; 4];
        let mut tot_added = [0u32; 4];
        let mut tot_added_new = [0u32; 4];
        let mut tot_added_earlier = [0u32; 4];
        let mut cwm = 0u32;
        for (aid, e) in &acc {
            for v in 0..REDIRECTS.len() {
                tot_saved[v] += e.saved[v];
                tot_added[v] += e.added[v];
                if e.shadow_dead[v] {
                    if e.died_tick < 0 {
                        tot_added_new[v] += 1;
                    } else {
                        tot_added_earlier[v] += 1;
                    }
                    println!(
                        "#ADD\t{}\t{}\t{}\t{}\tshadow_tick={}\treal_tick={}\tgap={}\tclass={}\tpath={}",
                        aid, seed, mode_label(strategy), redirect_label(REDIRECTS[v]),
                        e.shadow_dead_tick[v], e.died_tick,
                        if e.died_tick > 0 { e.died_tick - e.shadow_dead_tick[v] } else { -1 },
                        if e.died_tick < 0 { "new" } else { "earlier" },
                        e.added_path[v]
                    );
                }
            }
            cwm += e.cw_mismatch;
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{}\t{}",
                aid, seed, mode, e.ticks, e.died_tick, e.died_path,
                e.saved[0], e.saved[1], e.saved[2], e.saved[3],
                e.added[0], e.added[1], e.added[2], e.added[3],
                e.s_coh[2], e.s_coh[3], e.cw_mismatch, e.any_draws
            );
        }
        let mut fed_rows: Vec<(String, f64)> = fed_nominal.into_iter().collect();
        fed_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        // Shares are taken against the POSITIVE mass, not the net: the net is a small
        // difference of two large opposite flows, so a share of it is meaningless —
        // the same trap task 24 §4.1 named for cohesion (two feedback classes must not
        // be summed).
        let fed_pos: f64 = fed_rows.iter().filter(|(_, v)| *v > 0.0).map(|(_, v)| *v).sum();
        let fed_neg: f64 = fed_rows.iter().filter(|(_, v)| *v < 0.0).map(|(_, v)| *v).sum();
        for (id, v) in &fed_rows {
            println!(
                "#FEDW\t{}\t{}\t{}\t{:+.1}\tshare_of_pos={:.1}%",
                seed, mode, id, v,
                if *v > 0.0 && fed_pos != 0.0 { 100.0 * v / fed_pos } else { 0.0 }
            );
        }
        println!(
            "#FED\tseed={}\tmode={}\tnom_pos={:+.1}\tnom_neg={:+.1}\tnominal={:+.1}\trealized={:+.1}\tclamp_loss={:+.1}\tmax={:.1}\tfirst80={}\tticks80={}\tticks100={}\tvictory={}\tott_death={}\tbyz_death={}",
            seed, mode, fed_pos, fed_neg, fed_nominal_total, fed_realized_total,
            fed_realized_total - fed_nominal_total, fed_max, fed_first_80, fed_ticks_80,
            fed_at_100, victory_tick, ott_death, byz_death
        );
        println!(
            "#ENV\tseed={}\tmode={}\tgate_true={}\tgate_true_alive={}\telig_now={}\telig_iii={}\tiii_gain={}\tiii_loss={}\tfires_mehmed={}\tfires_greek={}\tgreek_gate={}\tgreek_gate_shadow={}\tgreek_flip={}\tfg_mean={:.2}",
            seed, mode, gate_true, gate_true_alive, elig_now, elig_iii,
            env_iii_gain, env_iii_loss, fires_mehmed, fires_greek,
            greek_gate_true, greek_gate_shadow_true, greek_gate_flip,
            fg_size_sum as f64 / fg_ticks.max(1) as f64
        );
        for (i, (label, _, _, _)) in II_CANDIDATES.iter().enumerate() {
            println!(
                "#II\tseed={}\tmode={}\t{}\tocc={:.1}%\tticks={}",
                seed, mode, label,
                100.0 * ii_occ[i] as f64 / live_ticks.max(1) as f64,
                ii_occ[i]
            );
        }
        println!(
            "#ET\tseed={}\tmode={}\tcollapses={}\tsaved_base={}\tsaved_cut={}\tsaved_i={}\tsaved_ip={}\tadded_base={}\tadded_cut={}\tadded_i={}\tadded_ip={}\tnew_i={}\tearlier_i={}\tnew_ip={}\tearlier_ip={}\tcw_mismatch={}",
            seed, mode, collapses,
            tot_saved[0], tot_saved[1], tot_saved[2], tot_saved[3],
            tot_added[0], tot_added[1], tot_added[2], tot_added[3],
            tot_added_new[2], tot_added_earlier[2], tot_added_new[3], tot_added_earlier[3], cwm
        );
    }
}

// ===========================================================================
// task 26 — the migration channel: `migwalk` and `decisive26`
// ===========================================================================
//
// `calculate_migration_interaction` (`interactions.rs:611–700`) is called once per
// neighbour pair per tick, unconditionally and **without consuming RNG**. That is
// what makes this channel different from every previous one: given the state the
// pair loop starts from, migration is deterministic, so the probe can *replicate*
// it rather than estimate it — the standard задача 23 §11 says the residual
// attribution failed to meet.
//
// Four facts of the carrier, all from the code and all load-bearing here:
//   1. the gate tests `actor_a` first, and `get_neighbor_pairs` sorts pairs
//      alphabetically ⇒ when both neighbours qualify, the alphabetically smaller
//      one is always the source;
//   2. `pressuring_pop` is read at call time (`:660`), not at the start of the
//      tick ⇒ a second firing in the same tick cuts an already-reduced stock, and
//      the sink of that firing receives 0.5 % of the reduced value;
//   3. the loss is multiplicative (`set_metric(pop * 0.99)`) and the gain additive
//      (`add_metric(pressuring_pop * 0.005)`) ⇒ half of what moves disappears;
//   4. `(ep − 65) · 0.2 / distance` is added to the SINK's `external_pressure`
//      (`:681`), unclamped until phase 5 ⇒ migration can push a neighbour over the
//      gate's own threshold, inside the same tick, and manufacture a new source.
//
// Population writers, established by walk and not by grep: `auto_deltas` (phase 1),
// rank bonuses (phase 2), dependency rules (phase 3a), **migration — the only
// population writer inside the pair loop** (`interaction_rules` is empty in all
// three scenarios), random events (phase 3b), tags (phase 4), the `max(0.0)` clamp
// (phase 5), and the successor split (`mod.rs:1640`).

/// Distance and border of one live land pair, in the order the engine builds it.
#[derive(Clone)]
struct LandPair {
    a: String,
    b: String,
    distance: u32,
}

/// Replicates `get_neighbor_pairs` (`interactions.rs:296–330`) and keeps the land
/// pairs: dedup by sorted key, both actors alive, sorted by `(a, b)`.
///
/// Private in the engine, so it is re-derived here — the same way задача 25
/// re-derived it for the spawn walk. The sort is not cosmetic: it decides which
/// side of a two-qualifier pair becomes the source.
fn land_pairs(world: &WorldState) -> Vec<LandPair> {
    let mut seen = std::collections::HashSet::new();
    let mut pairs = Vec::new();
    for (actor_id, actor) in &world.actors {
        for n in &actor.neighbors {
            if n.border_type != engine13::core::BorderType::Land {
                continue;
            }
            if !world.actors.contains_key(&n.id) || world.dead_actor_ids.contains(&n.id) {
                continue;
            }
            if world.dead_actor_ids.contains(actor_id) {
                continue;
            }
            let (a, b) = if actor_id < &n.id {
                (actor_id.clone(), n.id.clone())
            } else {
                (n.id.clone(), actor_id.clone())
            };
            let key = format!("{}-{}", a, b);
            if seen.insert(key) {
                pairs.push(LandPair { a, b, distance: n.distance });
            }
        }
    }
    pairs.sort_by(|x, y| (&x.a, &x.b).cmp(&(&y.a, &y.b)));
    pairs
}

fn passes_gate(m: &std::collections::HashMap<String, f64>) -> bool {
    m.get("external_pressure").copied().unwrap_or(0.0) > 65.0
        && m.get("cohesion").copied().unwrap_or(0.0) < 40.0
}

#[derive(Default, Clone)]
struct MigActor {
    ticks: u32,
    k_min: u32,
    k_max: u32,
    k_sum: u64,
    gate_ticks: u32,
    /// the two conjuncts separately — an actor with `cohesion < 40` and no
    /// pressure is one `(D₁)`-style key away from being a source (§4.3)
    coh_ticks: u32,
    ep_ticks: u32,
    /// ticks this actor would be the source on ≥1 pair
    src_ticks: u32,
    /// histogram of "how many pairs at once", index = count, capped at 8
    src_hist: [u32; 9],
    sink_ticks: u32,
    /// ticks the actor passed the gate but was NOT the source on some pair
    /// because the other side qualified too and sorted first
    yielded: u32,
    first_gate_tick: i64,
    /// had already received a migration pressure transfer before it first
    /// entered the gate — the manufactured-source contour
    pressured_before_gate: bool,
    ep_first_gate: f64,
    pop0: f64,
    pop_final: f64,
}

/// §1 п.1 and п.4 — the graph and the gate, measured in the world tick by tick.
fn migwalk(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    println!("actor\tseed\tmode\tticks\tk_min\tk_max\tk_mean\tgate%\tcoh40%\tep65%\tsrc%\tsink%\tyield\tsrc1\tsrc2\tsrc3\tsrc4+\tfirst_gate\tpre_press\tpop0\tpopF");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };

        let mut acc: BTreeMap<String, MigActor> = BTreeMap::new();
        let mut pair_hist: BTreeMap<u32, u32> = BTreeMap::new(); // live land pairs per tick
        let mut fire_pairs = 0u64;   // predicted firings, gate at loop start
        let mut both_qualify = 0u64; // ticks a pair had two qualifying sides

        for t in 0..ticks {
            // ---- observation BEFORE the tick: this is what the pair loop of the
            // previous tick left behind, and the closest observable point to the
            // loop start of this one. `migwalk` is a census, not the counter —
            // the counter is `decisive26`, which advances the state itself.
            {
                let world = state.world_state.as_ref().unwrap();
                let pairs = land_pairs(world);
                *pair_hist.entry(pairs.len() as u32).or_insert(0) += 1;
                let mut k: BTreeMap<String, u32> = BTreeMap::new();
                for p in &pairs {
                    *k.entry(p.a.clone()).or_insert(0) += 1;
                    *k.entry(p.b.clone()).or_insert(0) += 1;
                }
                let mut src_count: BTreeMap<String, u32> = BTreeMap::new();
                let mut sink: std::collections::HashSet<String> = std::collections::HashSet::new();
                let mut yielded: std::collections::HashSet<String> = std::collections::HashSet::new();
                for p in &pairs {
                    let ga = world.actors.get(&p.a).map(|x| passes_gate(&x.metrics)).unwrap_or(false);
                    let gb = world.actors.get(&p.b).map(|x| passes_gate(&x.metrics)).unwrap_or(false);
                    if ga && gb {
                        both_qualify += 1;
                        yielded.insert(p.b.clone()); // b qualifies but a sorts first
                    }
                    let src = if ga { Some(&p.a) } else if gb { Some(&p.b) } else { None };
                    if let Some(s) = src {
                        *src_count.entry(s.clone()).or_insert(0) += 1;
                        fire_pairs += 1;
                        sink.insert(if s == &p.a { p.b.clone() } else { p.a.clone() });
                    }
                }
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let e = acc.entry(aid.clone()).or_insert_with(|| MigActor {
                        k_min: u32::MAX,
                        first_gate_tick: -1,
                        pop0: a.get_metric("population"),
                        ..Default::default()
                    });
                    e.ticks += 1;
                    let kk = k.get(aid).copied().unwrap_or(0);
                    e.k_min = e.k_min.min(kk);
                    e.k_max = e.k_max.max(kk);
                    e.k_sum += kk as u64;
                    if a.get_metric("cohesion") < 40.0 { e.coh_ticks += 1; }
                    if a.get_metric("external_pressure") > 65.0 { e.ep_ticks += 1; }
                    if passes_gate(&a.metrics) {
                        e.gate_ticks += 1;
                        if e.first_gate_tick < 0 {
                            e.first_gate_tick = t as i64;
                            e.ep_first_gate = a.get_metric("external_pressure");
                        }
                    }
                    if let Some(c) = src_count.get(aid) {
                        e.src_ticks += 1;
                        e.src_hist[(*c as usize).min(8)] += 1;
                    }
                    if sink.contains(aid) {
                        e.sink_ticks += 1;
                        if e.first_gate_tick < 0 {
                            // received a pressure transfer while still outside the gate
                            e.pressured_before_gate = true;
                        }
                    }
                    if yielded.contains(aid) {
                        e.yielded += 1;
                    }
                    e.pop_final = a.get_metric("population");
                }
            }

            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }
        }

        let mode = mode_label(strategy);
        for (aid, e) in &acc {
            let n = e.ticks.max(1) as f64;
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}",
                aid, seed, mode, e.ticks,
                if e.k_min == u32::MAX { 0 } else { e.k_min }, e.k_max, e.k_sum as f64 / n,
                100.0 * e.gate_ticks as f64 / n,
                100.0 * e.coh_ticks as f64 / n,
                100.0 * e.ep_ticks as f64 / n,
                100.0 * e.src_ticks as f64 / n,
                100.0 * e.sink_ticks as f64 / n,
                e.yielded,
                e.src_hist[1], e.src_hist[2], e.src_hist[3],
                e.src_hist[4] + e.src_hist[5] + e.src_hist[6] + e.src_hist[7] + e.src_hist[8],
                e.first_gate_tick,
                e.pressured_before_gate as u8,
                e.pop0, e.pop_final
            );
        }
        let pairs_desc: Vec<String> = pair_hist.iter().map(|(k, v)| format!("{}x{}", k, v)).collect();
        println!(
            "#MIGWALK\tseed={}\tmode={}\tfire_pairs={}\tboth_qualify={}\tpairs_per_tick={}",
            seed, mode, fire_pairs, both_qualify, pairs_desc.join(",")
        );
    }
}

/// One tick of the migration sub-loop, replicated exactly.
///
/// Returns per-actor `(population loss, population gain, external_pressure gained)`
/// and the ordered list of firings. The loop order is the engine's own — pairs
/// sorted by `(a, b)` — because that order decides both which side of a
/// two-qualifier pair is the source and how much each later sink receives.
///
/// `pop` and `ep` are advanced in place, so a pressure transfer inside the tick can
/// open the gate for a pair processed later, exactly as it does in the engine.
fn replay_migration(
    pairs: &[LandPair],
    pop: &mut BTreeMap<String, f64>,
    ep: &mut BTreeMap<String, f64>,
    coh: &BTreeMap<String, f64>,
    loss: &mut BTreeMap<String, f64>,
    gain: &mut BTreeMap<String, f64>,
    ep_gain: &mut BTreeMap<String, f64>,
) -> Vec<(String, String)> {
    let mut fired = Vec::new();
    for p in pairs {
        let qual = |id: &str| -> bool {
            ep.get(id).copied().unwrap_or(0.0) > 65.0 && coh.get(id).copied().unwrap_or(0.0) < 40.0
        };
        // the engine tests `actor_a` first (`interactions.rs:640–646`)
        let src = if qual(&p.a) {
            p.a.clone()
        } else if qual(&p.b) {
            p.b.clone()
        } else {
            continue;
        };
        let dst = if src == p.a { p.b.clone() } else { p.a.clone() };
        let src_pop = pop.get(&src).copied().unwrap_or(0.0);
        let src_ep = ep.get(&src).copied().unwrap_or(0.0);
        let transfer = (src_ep - 65.0) * 0.2 / p.distance as f64;
        // source: multiplicative, on the stock as it stands at THIS call
        let lost = src_pop * 0.01;
        pop.insert(src.clone(), src_pop - lost);
        *loss.entry(src.clone()).or_insert(0.0) += lost;
        // sink: additive, half of what left, plus the pressure
        let got = src_pop * 0.005;
        *pop.entry(dst.clone()).or_insert(0.0) += got;
        *gain.entry(dst.clone()).or_insert(0.0) += got;
        *ep.entry(dst.clone()).or_insert(0.0) += transfer;
        *ep_gain.entry(dst.clone()).or_insert(0.0) += transfer;
        fired.push((src, dst));
    }
    fired
}

#[derive(Default, Clone)]
struct Mig26 {
    seen: bool,
    ticks: u32,
    min_surv: Option<u32>,
    neighbors: Vec<(String, u32)>,
    // --- counters (§1 п.2): every population writer gets its own -----------
    c_dep: f64,      // dependency rules
    c_events: f64,   // random events, nominal
    c_mig_loss: f64, // migration, as source
    c_mig_gain: f64, // migration, as sink
    c_split: f64,    // successor split (born with parent's pop × weight)
    c_floor: f64,    // truncation at the 0 floor
    c_resid: f64,    // what none of the above explains
    // --- replica validation ------------------------------------------------
    rep_bad: u32,
    rep_worst: f64,
    // --- shadow: the world where migration never ran -----------------------
    s_pop: f64,
    s_ep: f64,
    prev_pop: Option<f64>,
    prev_ep: Option<f64>,
    real_ct: u32,
    shad_ct: u32,
    saved: u32,
    added: u32,
    shadow_dead: bool,
    died_tick: i64,
    died_path: &'static str,
    cw_mismatch: u32,
    // --- other consumers of the two metrics --------------------------------
    treas_debt: f64,
    up_div: u32,
    pop_hist: std::collections::VecDeque<f64>,
    ep_hist: std::collections::VecDeque<f64>,
}

/// The deterministic half of every `auto_delta` that writes an actor's
/// `population`, plus the bound on the half that is not deterministic.
///
/// `phase_auto_deltas` runs FIRST in the tick (`mod.rs:296–332`), so evaluating its
/// conditions on the state observed before the tick is exact — unlike the gate of
/// the pair loop, which sits three phases later. What is *not* exact is the noise
/// term, `(rng.gen() − 0.5)·2·noise`: it consumes RNG and cannot be replicated from
/// outside. It is bounded by `±noise`, and that bound is returned so the identity
/// can be checked to the tolerance the engine actually allows instead of pretending
/// to a precision that does not exist. `rome_375` is the only scenario with such a
/// block (`rome.population`, base `0.3`, three conditions, `noise: 0.1`).
fn price_pop_auto_deltas(
    sc: &Scenario,
    snap: &BTreeMap<String, std::collections::HashMap<String, f64>>,
    globals: &std::collections::HashMap<String, f64>,
) -> (BTreeMap<String, f64>, BTreeMap<String, f64>) {
    let read = |r: &MetricRef| -> f64 {
        match r {
            MetricRef::Actor { actor_id, metric } => snap
                .get(actor_id.as_str())
                .and_then(|m| m.get(metric.as_str()))
                .copied()
                .unwrap_or(0.0),
            MetricRef::Global { key } => globals.get(key.as_str()).copied().unwrap_or(0.0),
            MetricRef::Family { .. } => 0.0,
        }
    };
    let mut delta: BTreeMap<String, f64> = BTreeMap::new();
    let mut noise: BTreeMap<String, f64> = BTreeMap::new();
    for ad in &sc.auto_deltas {
        let MetricRef::Actor { actor_id, metric } = &ad.metric else { continue };
        if metric.as_str() != "population" {
            continue;
        }
        let mut d = ad.base;
        for c in &ad.conditions {
            let v = read(&c.metric);
            let hit = match c.operator {
                ComparisonOperator::Less => v < c.value,
                ComparisonOperator::LessOrEqual => v <= c.value,
                ComparisonOperator::Greater => v > c.value,
                ComparisonOperator::GreaterOrEqual => v >= c.value,
                ComparisonOperator::Equal => (v - c.value).abs() < 0.001,
            };
            if hit {
                d += c.delta;
            }
        }
        for rc in &ad.ratio_conditions {
            let b = read(&rc.metric_b);
            if b == 0.0 {
                continue;
            }
            if rc.operator.evaluate(read(&rc.metric_a) / b, rc.ratio) {
                d += rc.delta;
            }
        }
        *delta.entry(actor_id.as_str().to_string()).or_insert(0.0) += d;
        *noise.entry(actor_id.as_str().to_string()).or_insert(0.0) += ad.noise;
    }
    (delta, noise)
}

/// What one solved tick of migration produced: per-actor loss, per-actor gain,
/// per-actor pressure received, and the ordered list of firings.
type MigrationTick = (
    BTreeMap<String, f64>,
    BTreeMap<String, f64>,
    BTreeMap<String, f64>,
    Vec<(String, String)>,
);

/// The counter proper (§1 п.2): instead of *predicting* which pairs fired from the
/// state observed before the tick, solve for the set that reproduces the observed
/// populations exactly.
///
/// Why a search and not a prediction: the gate reads `external_pressure` and
/// `cohesion` **at the moment the pair loop runs**, and between the observation
/// point and that moment sit `auto_deltas`, rank bonuses, the dependency phase and
/// the military/diplomatic interactions of earlier pairs — the last of which are
/// RNG-driven. A prediction is therefore right most of the time and wrong near the
/// threshold, which is exactly where this channel lives. The search has no such
/// blind spot: it is validated by the population identity of every actor at once,
/// so a wrong set cannot pass.
///
/// Candidates are pruned to actors near the gate (`ep > 45 ∧ coh < 75` — wide,
/// because a military defeat inside the same tick can drop `cohesion` by 10–20
/// points before the pair loop reaches the pair), which in
/// all three scenarios keeps the subset space at a handful; a tick whose candidate
/// set is larger than `MAX_SOLVE_CANDIDATES` is reported unresolved rather than
/// guessed.
const MAX_SOLVE_CANDIDATES: usize = 12;

#[allow(clippy::too_many_arguments)]
fn solve_migration(
    pairs: &[LandPair],
    base_pop: &BTreeMap<String, f64>,
    ep0: &BTreeMap<String, f64>,
    coh0: &BTreeMap<String, f64>,
    ev_pop: &BTreeMap<String, f64>,
    observed: &BTreeMap<String, f64>,
    tol: &BTreeMap<String, f64>,
) -> Option<MigrationTick> {
    let mut cands: Vec<String> = ep0
        .iter()
        .filter(|(id, e)| **e > 45.0 && coh0.get(*id).copied().unwrap_or(100.0) < 75.0)
        .map(|(id, _)| id.clone())
        .collect();
    cands.sort();
    if cands.len() > MAX_SOLVE_CANDIDATES {
        return None;
    }
    for mask in 0u32..(1u32 << cands.len()) {
        let srcs: std::collections::HashSet<&String> = cands
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, c)| c)
            .collect();
        let mut pop = base_pop.clone();
        let mut loss = BTreeMap::new();
        let mut gain = BTreeMap::new();
        let mut epg = BTreeMap::new();
        let mut fired = Vec::new();
        // the engine's order and tie-break, with the source set fixed by the mask
        for p in pairs {
            let src = if srcs.contains(&p.a) {
                p.a.clone()
            } else if srcs.contains(&p.b) {
                p.b.clone()
            } else {
                continue;
            };
            let dst = if src == p.a { p.b.clone() } else { p.a.clone() };
            let sp = pop.get(&src).copied().unwrap_or(0.0);
            let se = ep0.get(&src).copied().unwrap_or(0.0);
            pop.insert(src.clone(), sp - sp * 0.01);
            *loss.entry(src.clone()).or_insert(0.0) += sp * 0.01;
            *pop.entry(dst.clone()).or_insert(0.0) += sp * 0.005;
            *gain.entry(dst.clone()).or_insert(0.0) += sp * 0.005;
            *epg.entry(dst.clone()).or_insert(0.0) += (se - 65.0) * 0.2 / p.distance as f64;
            fired.push((src, dst));
        }
        let ok = observed.iter().all(|(aid, obs)| {
            let pred = (pop.get(aid).copied().unwrap_or(0.0)
                + ev_pop.get(aid).copied().unwrap_or(0.0))
            .max(0.0);
            (pred - obs).abs()
                <= 1e-6 * obs.abs().max(1.0) + tol.get(aid).copied().unwrap_or(0.0)
        });
        if ok {
            return Some((loss, gain, epg, fired));
        }
    }
    None
}

/// §1 п.2, п.3, п.5 — the counter, the `K` decomposition and the counterfactual.
fn decisive26(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    println!("actor\tseed\tmode\tticks\tdep\tevents\tmig_loss\tmig_gain\tsplit\tfloor\tresid\trep_bad\trep_worst\ts_pop\ts_ep\tsaved\tadded\tdied\tpath\tcw_mism\tup_div\ttreas_debt");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };
        let sc = state.current_scenario.as_ref().unwrap();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool_of(sc)
            .into_iter()
            .map(|(e, c)| (e.id.clone(), (e, c)))
            .collect();
        let deps = sc.dependencies.clone();
        let sc_owned = sc.clone();
        let sc_ref = &sc_owned;

        let mut acc: BTreeMap<String, Mig26> = BTreeMap::new();
        let mut collapses = 0u32;
        let mut victory_tick: i64 = -1;
        // per-tick histogram of "how many pairs one source fired on at once"
        let mut khist: BTreeMap<u32, u64> = BTreeMap::new();
        let mut fires = 0u64;
        let mut fires_first = 0u64; // the first firing of a source in a tick
        let mut loss_first = 0.0f64;
        let mut loss_rest = 0.0f64;
        let mut ep_made_source = 0u64;      // gate opened by a transfer received earlier
        let mut ep_made_source_pred = 0u64; // same, as the naive prediction saw it
        let mut solve_unresolved = 0u32; // ticks the search could not reproduce
        let mut pred_wrong = 0u32;       // ticks the naive prediction differed from the solved set

        for t in 0..ticks {
            // ---------- predict the migration of this tick ----------------------
            let (fired_pred, pred_loss, pred_gain, pred_ep, pred_pop, prev_snapshot, pairs_t, base_pop, ep0, coh0, pop_noise) = {
                let world = state.world_state.as_ref().unwrap();
                let pairs = land_pairs(world);
                let mut pop: BTreeMap<String, f64> = BTreeMap::new();
                let mut ep: BTreeMap<String, f64> = BTreeMap::new();
                let mut coh: BTreeMap<String, f64> = BTreeMap::new();
                let mut snapshot: BTreeMap<String, (f64, f64, f64, f64)> = BTreeMap::new();
                let mut full: BTreeMap<String, std::collections::HashMap<String, f64>> = BTreeMap::new();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let (p, e, c, eo) = (
                        a.get_metric("population"),
                        a.get_metric("external_pressure"),
                        a.get_metric("cohesion"),
                        a.get_metric("economic_output"),
                    );
                    snapshot.insert(aid.clone(), (p, e, c, eo));
                    full.insert(aid.clone(), a.metrics.clone());
                    // the dependency phase runs BEFORE the pair loop, so the stock the
                    // loop cuts is already net of the rule
                    pop.insert(aid.clone(), p); // auto_delta and the rule are folded in below
                    ep.insert(aid.clone(), e);
                    coh.insert(aid.clone(), c);
                }
                // phase 1 (auto_deltas) runs before the dependency phase, which runs
                // before the pair loop — so the stock the loop cuts is the start value
                // plus the auto_delta and minus the rule, in that order
                let (ad_delta, ad_noise) = price_pop_auto_deltas(sc_ref, &full, &world.global_metrics);
                for (aid, d) in &ad_delta {
                    if let Some(v) = pop.get_mut(aid) {
                        *v = (*v + d).max(0.0);
                    }
                }
                for (aid, v) in pop.iter_mut() {
                    let eo = snapshot.get(aid).map(|x| x.3).unwrap_or(0.0);
                    *v -= price_population_rules(&deps, *v, eo);
                }
                let ep_before = ep.clone();
                let base_for_solver = pop.clone();
                let mut loss = BTreeMap::new();
                let mut gain = BTreeMap::new();
                let mut ep_gain = BTreeMap::new();
                let fired = replay_migration(
                    &pairs, &mut pop, &mut ep, &coh, &mut loss, &mut gain, &mut ep_gain,
                );
                // a source whose gate was open only because of a transfer received
                // earlier in this same tick — the contour of §0.1 п.4, counted inside
                // the tick where it is unambiguous
                for (src, _) in &fired {
                    if ep_before.get(src).copied().unwrap_or(0.0) <= 65.0 {
                        ep_made_source_pred += 1;
                    }
                }
                (fired, loss, gain, ep_gain, pop.clone(), snapshot, pairs, base_for_solver, ep_before, coh, ad_noise)
            };

            let live_before: std::collections::HashSet<String> = prev_snapshot.keys().cloned().collect();

            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);
            let log_len = state.event_log.events.len();
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }
            if victory_tick < 0 && state.world_state.as_ref().unwrap().victory_achieved {
                victory_tick = (t + 1) as i64;
            }

            // what the events wrote to population this tick, nominal
            let mut ev_pop: BTreeMap<String, f64> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                let Some((def, _)) = by_id.get(&ev.id) else { continue };
                for (aid, d) in event_writes_to(def, &ev.actor_id, "population") {
                    *ev_pop.entry(aid).or_insert(0.0) += d;
                }
            }

            // ---------- solve for the firing set that reproduces the tick --------
            let (mig_loss, mig_gain, mig_ep, fired, solved_ok) = {
                let world = state.world_state.as_ref().unwrap();
                let mut observed: BTreeMap<String, f64> = BTreeMap::new();
                for aid in base_pop.keys() {
                    if let Some(a) = world.actors.get(aid) {
                        if !world.dead_actor_ids.contains(aid) {
                            observed.insert(aid.clone(), a.get_metric("population"));
                            continue;
                        }
                    }
                    if let Some(d) = world.dead_actors.iter().find(|d| &d.id == aid && d.tick_death == t) {
                        observed.insert(aid.clone(), d.final_metrics.get("population").copied().unwrap_or(0.0));
                    }
                }
                match solve_migration(&pairs_t, &base_pop, &ep0, &coh0, &ev_pop, &observed, &pop_noise) {
                    Some((l, g, e, f)) => (l, g, e, f, true),
                    None => (pred_loss.clone(), pred_gain.clone(), pred_ep.clone(), fired_pred.clone(), false),
                }
            };
            if !solved_ok {
                solve_unresolved += 1;
                // Characterise the miss rather than absorb it: the suspect is
                // `economic_output` moving across the dependency rule's threshold
                // between the observation point and the dependency phase, which
                // changes the stock the pair loop cuts.
                let world = state.world_state.as_ref().unwrap();
                let mut near = Vec::new();
                for (aid, (p0, e0, c0, eo0)) in prev_snapshot.iter() {
                    let eo_now = world.actors.get(aid).map(|a| a.get_metric("economic_output")).unwrap_or(-1.0);
                    if (*eo0 - 50.0).abs() < 10.0 || (eo_now - 50.0).abs() < 10.0 || *e0 > 65.0 {
                        near.push(format!(
                            "{}:p0={:.1},ep={:.1},coh={:.1},eo0={:.1},eo1={:.1}",
                            aid, p0, e0, c0, eo0, eo_now
                        ));
                    }
                }
                println!("#UNRES\t{}\t{}\ttick={}\t{}", seed, mode_label(strategy), t + 1, near.join(" "));
            }
            if fired != fired_pred {
                pred_wrong += 1;
            }
            // §4.2 — a source whose pressure at the START of the pair loop was below
            // the gate: it can only have crossed it on a transfer received earlier in
            // this same tick. Counted on the SOLVED set; the prediction-based twin
            // (`ep_made_source_pred`) is kept alongside so the two can disagree
            // visibly instead of silently.
            for (src, _) in &fired {
                if ep0.get(src).copied().unwrap_or(0.0) <= 65.0 {
                    ep_made_source += 1;
                }
            }

            // per-source firing count of this tick
            {
                let mut per_src: BTreeMap<String, u32> = BTreeMap::new();
                for (src, _) in &fired {
                    *per_src.entry(src.clone()).or_insert(0) += 1;
                }
                for (src, n) in &per_src {
                    *khist.entry(*n).or_insert(0) += 1;
                    fires += *n as u64;
                    fires_first += 1;
                    // the first cut of the tick against the ones that compound on it
                    let base = prev_snapshot.get(src).map(|x| x.0).unwrap_or(0.0)
                        - price_population_rules(
                            &deps,
                            prev_snapshot.get(src).map(|x| x.0).unwrap_or(0.0),
                            prev_snapshot.get(src).map(|x| x.3).unwrap_or(0.0),
                        );
                    let first = base * 0.01;
                    loss_first += first;
                    loss_rest += mig_loss.get(src).copied().unwrap_or(0.0) - first;
                }
            }


            let world = state.world_state.as_ref().unwrap();
            let cur_tick = t;
            let just_dead: BTreeMap<String, &std::collections::HashMap<String, f64>> = world
                .dead_actors
                .iter()
                .filter(|d| d.tick_death == cur_tick)
                .map(|d| (d.id.clone(), &d.final_metrics))
                .collect();
            let live_mil: BTreeMap<String, f64> = world
                .actors
                .iter()
                .map(|(k, a)| (k.clone(), a.get_metric("military_size")))
                .collect();

            // successors born this tick carry `parent_pop × weight` (`mod.rs:1640`)
            for (aid, a) in world.actors.iter() {
                if !live_before.contains(aid) && !world.dead_actor_ids.contains(aid) {
                    let e = acc.entry(aid.clone()).or_default();
                    e.c_split += a.get_metric("population");
                }
            }

            let ids: Vec<String> = world
                .actors
                .keys()
                .cloned()
                .chain(just_dead.keys().cloned())
                .collect();
            for aid in ids {
                let alive = world.actors.contains_key(&aid) && !world.dead_actor_ids.contains(&aid);
                let metrics: Option<std::collections::HashMap<String, f64>> = if alive {
                    world.actors.get(&aid).map(|a| a.metrics.clone())
                } else {
                    just_dead.get(&aid).map(|m| (*m).clone())
                };
                let Some(m) = metrics else { continue };
                let pop = m.get("population").copied().unwrap_or(0.0);
                let ep = m.get("external_pressure").copied().unwrap_or(0.0);
                let coh = m.get("cohesion").copied().unwrap_or(0.0);
                let leg = m.get("legitimacy").copied().unwrap_or(0.0);
                let mil = m.get("military_size").copied().unwrap_or(0.0);
                let eo = m.get("economic_output").copied().unwrap_or(0.0);

                let e = acc.entry(aid.clone()).or_default();
                if !e.seen {
                    e.seen = true;
                    e.min_surv = world.actors.get(&aid).and_then(|a| a.minimum_survival_ticks);
                    e.neighbors = world
                        .actors
                        .get(&aid)
                        .map(|a| a.neighbors.iter().map(|n| (n.id.clone(), n.distance)).collect())
                        .unwrap_or_default();
                    e.s_pop = pop;
                    e.s_ep = ep;
                    e.prev_pop = Some(pop);
                    e.prev_ep = Some(ep);
                    e.died_tick = -1;
                    continue; // no previous tick to reconcile against
                }
                e.ticks += 1;

                // ---- the counter: does the replica reproduce the tick? ----------
                if let Some((p0, _e0, _c0, eo0)) = prev_snapshot.get(&aid) {
                    let base = base_pop.get(&aid).copied().unwrap_or(*p0);
                    let dep = price_population_rules(&deps, *p0, *eo0);
                    let l = mig_loss.get(&aid).copied().unwrap_or(0.0);
                    let g = mig_gain.get(&aid).copied().unwrap_or(0.0);
                    let evd = ev_pop.get(&aid).copied().unwrap_or(0.0);
                    let _ = &pred_pop;
                    // the identity is checked against the SOLVED set, so `rep_bad`
                    // measures the counter, not the naive prediction
                    let nominal = base - l + g + evd;
                    let floor = if nominal < 0.0 { -nominal } else { 0.0 };
                    let predicted = nominal.max(0.0);
                    let err = predicted - pop;
                    e.c_dep -= dep;
                    e.c_mig_loss -= l;
                    e.c_mig_gain += g;
                    e.c_events += evd;
                    e.c_floor += floor;
                    if err.abs() > 1e-6 * pop.abs().max(1.0) + pop_noise.get(&aid).copied().unwrap_or(0.0) {
                        e.rep_bad += 1;
                        if err.abs() > e.rep_worst.abs() {
                            e.rep_worst = err;
                        }
                        e.c_resid += pop - predicted;
                    }
                }

                // ---- the shadow: no migration at all ----------------------------
                let d_pop = pop - e.prev_pop.unwrap_or(pop);
                let d_ep = ep - e.prev_ep.unwrap_or(ep);
                let ml = mig_loss.get(&aid).copied().unwrap_or(0.0);
                let mg = mig_gain.get(&aid).copied().unwrap_or(0.0);
                let me = mig_ep.get(&aid).copied().unwrap_or(0.0);
                e.s_pop = (e.s_pop + d_pop + ml - mg).max(0.0);
                e.s_ep = (e.s_ep + d_ep - me).clamp(0.0, 100.0);
                e.prev_pop = Some(pop);
                e.prev_ep = Some(ep);

                e.treas_debt += (e.s_pop - pop) * eo * 0.001;
                e.pop_hist.push_back(e.s_pop - pop);
                while e.pop_hist.len() > 5 { e.pop_hist.pop_front(); }
                e.ep_hist.push_back(e.s_ep - ep);
                while e.ep_hist.len() > 5 { e.ep_hist.pop_front(); }

                // ---- verdicts ----------------------------------------------------
                let besieged = e.neighbors.iter().any(|(nid, dist)| {
                    *dist == 1
                        && live_mil
                            .get(nid)
                            .map(|v| *v >= engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY)
                            .unwrap_or(false)
                });
                let skip = matches!(e.min_surv, Some(ms) if cur_tick < ms);
                if !skip {
                    let (rc, ri, rq) = danger_paths(coh, leg, ep, mil, besieged);
                    let real_d = rc || ri || rq;
                    if real_d { e.real_ct += 1 } else { e.real_ct = 0 }
                    let engine_ct = world.collapse_warning_ticks.get(&aid).copied().unwrap_or(0);
                    if engine_ct != e.real_ct { e.cw_mismatch += 1; }
                    // only `external_pressure` reaches the collapse predicate; the
                    // shadow of `population` reaches relevance and the treasury
                    let (sc_, si, sq) = danger_paths(coh, leg, e.s_ep, mil, besieged);
                    let shad_d = sc_ || si || sq;
                    if shad_d { e.shad_ct += 1 } else { e.shad_ct = 0 }
                    if e.shad_ct >= 3 && alive && !e.shadow_dead {
                        e.shadow_dead = true;
                        e.added += 1;
                    }
                }
                let pw: f64 = e.pop_hist.back().copied().unwrap_or(0.0)
                    - e.pop_hist.front().copied().unwrap_or(0.0);
                let ew: f64 = e.ep_hist.back().copied().unwrap_or(0.0)
                    - e.ep_hist.front().copied().unwrap_or(0.0);
                let real_up = upheaval_verdict_multi(world, &aid, &[]);
                let shad_up = upheaval_verdict_multi(
                    world, &aid, &[("population", pw), ("external_pressure", ew)],
                );
                if real_up != shad_up { e.up_div += 1; }

                if !alive && e.died_tick < 0 {
                    e.died_tick = (t + 1) as i64;
                    collapses += 1;
                    let (dc, di, dq) = danger_paths(coh, leg, ep, mil, besieged);
                    e.died_path = match (dc, di, dq) {
                        (true, _, _) => "classic",
                        (_, true, _) => "internal",
                        (_, _, true) => "conquest",
                        _ => "none",
                    };
                    if e.shad_ct < 3 { e.saved += 1; }
                    println!(
                        "#DEATH26\t{}\t{}\t{}\ttick={}\tpath={}\treal_ct={}\tshad_ct={}\tep={:.2}\ts_ep={:.2}\tpop={:.1}\ts_pop={:.1}",
                        aid, seed, mode_label(strategy), t + 1, e.died_path,
                        e.real_ct, e.shad_ct, ep, e.s_ep, pop, e.s_pop
                    );
                }
            }
        }

        let mode = mode_label(strategy);
        let mut tot = [0.0f64; 7];
        let (mut saved, mut added, mut cwm, mut repbad, mut updiv) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for (aid, e) in &acc {
            tot[0] += e.c_dep; tot[1] += e.c_events; tot[2] += e.c_mig_loss;
            tot[3] += e.c_mig_gain; tot[4] += e.c_split; tot[5] += e.c_floor; tot[6] += e.c_resid;
            saved += e.saved; added += e.added; cwm += e.cw_mismatch; repbad += e.rep_bad; updiv += e.up_div;
            println!(
                "{}\t{}\t{}\t{}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{}\t{:+.2e}\t{:.1}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}\t{:+.1}",
                aid, seed, mode, e.ticks,
                e.c_dep, e.c_events, e.c_mig_loss, e.c_mig_gain, e.c_split, e.c_floor, e.c_resid,
                e.rep_bad, e.rep_worst, e.s_pop, e.s_ep,
                e.saved, e.added, e.died_tick, e.died_path, e.cw_mismatch, e.up_div, e.treas_debt
            );
        }
        let kdesc: Vec<String> = khist.iter().map(|(k, v)| format!("{}x{}", k, v)).collect();
        println!(
            "#CF26\tseed={}\tmode={}\tdep={:+.1}\tevents={:+.1}\tmig_loss={:+.1}\tmig_gain={:+.1}\tmig_net={:+.1}\tsplit={:+.1}\tfloor={:+.1}\tresid={:+.1}\trep_bad={}\tsaved={}\tadded={}\tcw_mismatch={}\tup_div={}\tcollapses={}\tvictory={}\tfires={}\tsources={}\tloss_first={:.1}\tloss_compound={:.1}\tep_made_source={}\tep_made_source_pred={}\tsolve_unresolved={}\tpred_wrong={}\tkhist={}",
            seed, mode, tot[0], tot[1], tot[2], tot[3], tot[2] + tot[3], tot[4], tot[5], tot[6],
            repbad, saved, added, cwm, updiv, collapses, victory_tick,
            fires, fires_first, loss_first, loss_rest, ep_made_source, ep_made_source_pred,
            solve_unresolved, pred_wrong, kdesc.join(",")
        );
    }
}

/// The variants §6 put on the table, priced before any of them is written into the
/// engine (§1 п.4 of the statement, стадия 2).
///
/// The shadow here is **exact in its own model**, and for a reason specific to this
/// channel: the gate reads `external_pressure` and `cohesion`, never `population`.
/// So changing what migration does to population cannot change **which pairs fire**
/// — the firing set solved from the real run stays valid for every variant, and only
/// the amounts differ. The feedback that remains is `population → treasury income`
/// (`mod.rs:672`), `population → plague` (`> 500`), and `population → relevance`;
/// the first two are measured below, the third is bounded by задача 26 §5.1
/// (`up_div` 0…130 actor-ticks).
#[derive(Clone, Copy, PartialEq, Debug)]
enum MigVariant {
    /// the world as it is — the floor test
    Base,
    /// (A₁) one aggregated transfer per source per tick, split between the pairs
    /// that fired, instead of `K` sequential cuts
    A1,
    /// (A₂) the sink receives what the source lost — the transfer stops evaporating
    A2,
    /// (B) both ratios scaled
    B50,
    B25,
    /// (A₁)+(A₂) — a separate variant with its own arithmetic, per §1 п.4 of the
    /// stage-2 brief: not an automatic union
    A1A2,
}

const MIG_VARIANTS: [MigVariant; 6] = [
    MigVariant::Base, MigVariant::A1, MigVariant::A2,
    MigVariant::B50, MigVariant::B25, MigVariant::A1A2,
];

fn mig_variant_label(v: MigVariant) -> &'static str {
    match v {
        MigVariant::Base => "base",
        MigVariant::A1 => "A1_aggregate",
        MigVariant::A2 => "A2_full_transfer",
        MigVariant::B50 => "B_half",
        MigVariant::B25 => "B_quarter",
        MigVariant::A1A2 => "A1A2",
    }
}

/// `(loss ratio, gain ratio, aggregate?)` for a variant.
fn mig_variant_params(v: MigVariant) -> (f64, f64, bool) {
    match v {
        MigVariant::Base => (0.01, 0.005, false),
        MigVariant::A1 => (0.01, 0.005, true),
        MigVariant::A2 => (0.01, 0.01, false),
        MigVariant::B50 => (0.005, 0.0025, false),
        MigVariant::B25 => (0.0025, 0.00125, false),
        MigVariant::A1A2 => (0.01, 0.01, true),
    }
}

#[derive(Default, Clone)]
struct VarActor {
    seen: bool,
    pop: [f64; 6],
    /// Σ (shadow − real) · eo · 0.001 — what the variant does to treasury income,
    /// the only quantitative consumer of `population` in the engine
    income: [f64; 6],
    /// ticks where the `plague` gate (`population > 500`) differs from the real run
    plague_flip: [u32; 6],
    real_pop_final: f64,
    pop0: f64,
}

/// Стадия 2, шаг 1 — the equilibrium calculation across the fork, on the trajectory.
fn migvariants(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    println!(
        "actor\tseed\tmode\tpop0\treal\t{}\t{}\t{}\t{}\t{}\tinc_A1\tinc_A2\tinc_A1A2\tplague_A1\tplague_A2",
        mig_variant_label(MigVariant::A1),
        mig_variant_label(MigVariant::A2),
        mig_variant_label(MigVariant::B50),
        mig_variant_label(MigVariant::B25),
        mig_variant_label(MigVariant::A1A2),
    );
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };
        let sc = state.current_scenario.as_ref().unwrap();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool_of(sc)
            .into_iter()
            .map(|(e, c)| (e.id.clone(), (e, c)))
            .collect();
        let deps = sc.dependencies.clone();
        let sc_owned = sc.clone();
        let sc_ref = &sc_owned;

        let mut acc: BTreeMap<String, VarActor> = BTreeMap::new();
        let mut unresolved = 0u32;

        for t in 0..ticks {
            let (pairs_t, base_pop, ep0, coh0, snapshot, pop_noise) = {
                let world = state.world_state.as_ref().unwrap();
                let pairs = land_pairs(world);
                let mut pop: BTreeMap<String, f64> = BTreeMap::new();
                let mut ep: BTreeMap<String, f64> = BTreeMap::new();
                let mut coh: BTreeMap<String, f64> = BTreeMap::new();
                let mut snap: BTreeMap<String, (f64, f64)> = BTreeMap::new();
                let mut full: BTreeMap<String, std::collections::HashMap<String, f64>> = BTreeMap::new();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let (p, eo) = (a.get_metric("population"), a.get_metric("economic_output"));
                    snap.insert(aid.clone(), (p, eo));
                    full.insert(aid.clone(), a.metrics.clone());
                    pop.insert(aid.clone(), p);
                    ep.insert(aid.clone(), a.get_metric("external_pressure"));
                    coh.insert(aid.clone(), a.get_metric("cohesion"));
                }
                let (ad_delta, ad_noise) = price_pop_auto_deltas(sc_ref, &full, &world.global_metrics);
                for (aid, d) in &ad_delta {
                    if let Some(v) = pop.get_mut(aid) {
                        *v = (*v + d).max(0.0);
                    }
                }
                for (aid, v) in pop.iter_mut() {
                    let eo = snap.get(aid).map(|x| x.1).unwrap_or(0.0);
                    *v -= price_population_rules(&deps, *v, eo);
                }
                (pairs, pop, ep, coh, snap, ad_noise)
            };

            // seed the shadows from the real world on first sight
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, (p, _eo)) in snapshot.iter() {
                    let e = acc.entry(aid.clone()).or_default();
                    if !e.seen {
                        e.seen = true;
                        e.pop = [*p; 6];
                        e.pop0 = *p;
                    }
                    let _ = world;
                }
            }

            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);
            let log_len = state.event_log.events.len();
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }
            let mut ev_pop: BTreeMap<String, f64> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                let Some((def, _)) = by_id.get(&ev.id) else { continue };
                for (aid, d) in event_writes_to(def, &ev.actor_id, "population") {
                    *ev_pop.entry(aid).or_insert(0.0) += d;
                }
            }

            // the firing set, solved against the real world (§1 п.2 of stage 1)
            let fired = {
                let world = state.world_state.as_ref().unwrap();
                let mut observed: BTreeMap<String, f64> = BTreeMap::new();
                for aid in base_pop.keys() {
                    if let Some(a) = world.actors.get(aid) {
                        if !world.dead_actor_ids.contains(aid) {
                            observed.insert(aid.clone(), a.get_metric("population"));
                            continue;
                        }
                    }
                    if let Some(d) = world.dead_actors.iter().find(|d| &d.id == aid && d.tick_death == t) {
                        observed.insert(aid.clone(), d.final_metrics.get("population").copied().unwrap_or(0.0));
                    }
                }
                match solve_migration(&pairs_t, &base_pop, &ep0, &coh0, &ev_pop, &observed, &pop_noise) {
                    Some((_, _, _, f)) => f,
                    None => {
                        unresolved += 1;
                        Vec::new()
                    }
                }
            };

            // how many pairs each source fired on — (A₁) needs it to split the transfer
            let mut per_src: BTreeMap<String, u32> = BTreeMap::new();
            for (src, _) in &fired {
                *per_src.entry(src.clone()).or_insert(0) += 1;
            }

            // advance every shadow with its own arithmetic
            let world = state.world_state.as_ref().unwrap();
            for (vi, v) in MIG_VARIANTS.iter().enumerate() {
                let (loss_r, gain_r, aggregate) = mig_variant_params(*v);
                // the stock the pair loop cuts, in THIS shadow: auto_delta and the
                // dependency rule are proportional, so they must be re-priced on the
                // shadow's own population, not copied from the real run
                let mut spop: BTreeMap<String, f64> = BTreeMap::new();
                for (aid, (p_real, eo)) in snapshot.iter() {
                    let e = acc.get(aid).map(|e| e.pop[vi]).unwrap_or(*p_real);
                    let ad = base_pop.get(aid).copied().unwrap_or(*p_real) - *p_real
                        + price_population_rules(&deps, *p_real, *eo);
                    let after_ad = (e + ad).max(0.0);
                    spop.insert(aid.clone(), after_ad - price_population_rules(&deps, after_ad, *eo));
                }
                if aggregate {
                    // one cut per source per tick, the transfer divided between the
                    // pairs that fired — the loss stops compounding and the sinks
                    // share what actually left
                    for (src, n) in &per_src {
                        let sp = spop.get(src).copied().unwrap_or(0.0);
                        let lost = sp * loss_r;
                        spop.insert(src.clone(), sp - lost);
                        let each = sp * gain_r / *n as f64;
                        for (s2, dst) in &fired {
                            if s2 == src {
                                *spop.entry(dst.clone()).or_insert(0.0) += each;
                            }
                        }
                    }
                } else {
                    for (src, dst) in &fired {
                        let sp = spop.get(src).copied().unwrap_or(0.0);
                        spop.insert(src.clone(), sp - sp * loss_r);
                        *spop.entry(dst.clone()).or_insert(0.0) += sp * gain_r;
                    }
                }
                for (aid, e) in acc.iter_mut() {
                    let Some(sp) = spop.get(aid) else { continue };
                    let evd = ev_pop.get(aid).copied().unwrap_or(0.0);
                    e.pop[vi] = (sp + evd).max(0.0);
                    let real = world
                        .actors
                        .get(aid)
                        .map(|a| a.get_metric("population"))
                        .unwrap_or(e.real_pop_final);
                    let eo = world.actors.get(aid).map(|a| a.get_metric("economic_output")).unwrap_or(0.0);
                    e.income[vi] += (e.pop[vi] - real) * eo * 0.001;
                    if (e.pop[vi] > 500.0) != (real > 500.0) {
                        e.plague_flip[vi] += 1;
                    }
                }
            }
            for (aid, e) in acc.iter_mut() {
                if let Some(a) = world.actors.get(aid) {
                    e.real_pop_final = a.get_metric("population");
                }
            }
        }

        let mode = mode_label(strategy);
        for (aid, e) in &acc {
            println!(
                "{}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{}\t{}",
                aid, seed, mode, e.pop0, e.real_pop_final,
                e.pop[1], e.pop[2], e.pop[3], e.pop[4], e.pop[5],
                e.income[1], e.income[2], e.income[5],
                e.plague_flip[1], e.plague_flip[2]
            );
        }
        let base_err: f64 = acc.values().map(|e| (e.pop[0] - e.real_pop_final).abs()).sum();
        println!(
            "#VAR\tseed={}\tmode={}\tbase_err={:.4}\tunresolved={}",
            seed, mode, base_err, unresolved
        );
    }
}

// ===========================================================================
// task 27 — the `external_pressure` ratchet: `epratchet`
// ===========================================================================
//
// Задача 2 asked whether decay opens the **vassalage band** and answered no. This
// mode asks about the other consumer of the same threshold — the **mortality
// predicate** (`classic_collapse` and `conquest_collapse`, both with `ep > 85`) —
// and it does three things задача 2 did not:
//
//   1. decomposes the INFLOW by producer, so "it grows from combat and auto_deltas"
//      becomes a number per producer per actor;
//   2. re-measures `drops` (задача 12's `0 in 72/72`) on the current build instead
//      of quoting it;
//   3. sweeps decay **in the shadow**, not in the engine: `λ` is subtracted from a
//      shadow `external_pressure` carried alongside the real one, with its own
//      clamp and its own `collapse_warning_ticks`, and the collapse verdict is
//      recomputed. No second simulation (the rule of задачи 23–26).
//
// The trap named in `investigation_migration_channel.md` §14.7 is avoided here by
// construction: `decisive26`'s solver models the PRE-fix migration arithmetic and
// is left untouched, and this mode carries its own [`solve_migration_v2`], which
// mirrors the aggregated conservative transfer the engine runs today.

/// Decay values swept in the shadow. The range is derived from the magnitudes of
/// the producers, not inherited from задача 2's `1…8`: combat delivers `15…25` per
/// event to the defender (`interactions.rs:455`), migration up to `+7` per firing
/// (`(100−65)·0.2/1`), the successor multiplier is `×1.3`, and the content
/// `auto_deltas` run `+0.3…+2.125` per tick. So the interesting region spans from
/// "smaller than one content tick" to "one combat event per two ticks".
const LAMBDAS: [f64; 8] = [0.0, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 12.5];

#[derive(Default, Clone)]
struct EpActor {
    seen: bool,
    ticks: u32,
    min_surv: Option<u32>,
    neighbors: Vec<(String, u32)>,
    ep0: f64,
    ep_prev: Option<f64>,
    // --- occupancy ---------------------------------------------------------
    at_100: u32,
    in_band: u32,       // (85, 100)
    above_85: u32,
    first_85: i64,
    first_100: i64,
    // --- the ratchet -------------------------------------------------------
    drops: u32,         // ticks the real `ep` ended lower than it started
    drop_mass: f64,
    /// задача 12 counted something else and its number must be re-measured in ITS
    /// own terms, not in ours: it asked how often an actor *left sustained
    /// pressure*, `ep >= 70`. An actor at 100 can lose 3 points and never leave.
    release_70: u32,
    below_70: u32,
    rises: u32,
    rise_mass: f64,
    // --- inflow by producer ------------------------------------------------
    p_auto: f64,
    p_events: f64,
    p_migration: f64,
    p_inherit: f64,
    p_residual: f64,    // combat: not replicable from outside, so it is what is left
    resid_negative: f64, // guard: a negative residual on an UNCLAMPED tick means a producer is missing
    /// nominal inflow the ceiling threw away — the central quantity of this task
    clamp_absorbed: f64,
    /// ticks on which the residual is meaningful at all (no truncation either end)
    free_ticks: u32,
    // --- shadows, one per lambda -------------------------------------------
    s_ep: [f64; 8],
    /// the shadow задача 26 §5.2 built: pressure without the migration transfer,
    /// re-derived here under TODAY's migration arithmetic instead of quoted
    s_nomig: f64,
    nomig_ct: u32,
    saved_nomig: u32,
    real_ct: u32,
    shad_ct: [u32; 8],
    saved: [u32; 8],
    cw_mismatch: u32,
    died_tick: i64,
    died_path: &'static str,
    /// the death was already attributed to migration by задача 26 §5.2
    mig_attributed: bool,
}

/// What one solved tick of TODAY's migration produced: pressure received per actor,
/// and the ordered firings with their distances.
type MigrationTickV2 = (BTreeMap<String, f64>, Vec<(String, String, u32)>);

/// The migration arithmetic the engine runs TODAY (aggregated per source, transfer
/// conserved), solved the same way задача 26 §1 solved the old one: by the set of
/// sources that reproduces every actor's observed population at once.
fn solve_migration_v2(
    pairs: &[LandPair],
    base_pop: &BTreeMap<String, f64>,
    ep0: &BTreeMap<String, f64>,
    coh0: &BTreeMap<String, f64>,
    ev_pop: &BTreeMap<String, f64>,
    observed: &BTreeMap<String, f64>,
    tol: &BTreeMap<String, f64>,
) -> Option<MigrationTickV2> {
    let mut cands: Vec<String> = ep0
        .iter()
        .filter(|(id, e)| **e > 45.0 && coh0.get(*id).copied().unwrap_or(100.0) < 75.0)
        .map(|(id, _)| id.clone())
        .collect();
    cands.sort();
    if cands.len() > MAX_SOLVE_CANDIDATES {
        return None;
    }
    for mask in 0u32..(1u32 << cands.len()) {
        let srcs: std::collections::HashSet<&String> = cands
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, c)| c)
            .collect();
        let mut pop = base_pop.clone();
        let mut ep_gain: BTreeMap<String, f64> = BTreeMap::new();
        let mut fired: Vec<(String, String, u32)> = Vec::new();
        // one aggregated move per source, sinks share what left — `interactions.rs`
        let mut by_src: BTreeMap<String, Vec<(String, u32)>> = BTreeMap::new();
        for p in pairs {
            let src = if srcs.contains(&p.a) {
                p.a.clone()
            } else if srcs.contains(&p.b) {
                p.b.clone()
            } else {
                continue;
            };
            let dst = if src == p.a { p.b.clone() } else { p.a.clone() };
            by_src.entry(src).or_default().push((dst, p.distance));
        }
        for (src, sinks) in &by_src {
            let sp = pop.get(src).copied().unwrap_or(0.0);
            let moved = sp * 0.01;
            pop.insert(src.clone(), sp - moved);
            let each = moved / sinks.len() as f64;
            let se = ep0.get(src).copied().unwrap_or(0.0);
            for (dst, dist) in sinks {
                *pop.entry(dst.clone()).or_insert(0.0) += each;
                *ep_gain.entry(dst.clone()).or_insert(0.0) += (se - 65.0) * 0.2 / *dist as f64;
                fired.push((src.clone(), dst.clone(), *dist));
            }
        }
        let ok = observed.iter().all(|(aid, obs)| {
            let pred = (pop.get(aid).copied().unwrap_or(0.0)
                + ev_pop.get(aid).copied().unwrap_or(0.0))
            .max(0.0);
            (pred - obs).abs()
                <= 1e-6 * obs.abs().max(1.0) + tol.get(aid).copied().unwrap_or(0.0)
        });
        if ok {
            return Some((ep_gain, fired));
        }
    }
    None
}

/// The deterministic half of every `auto_delta` writing an actor's
/// `external_pressure`, plus the noise bound. Same contract as
/// [`price_pop_auto_deltas`]: phase 1 runs first, so the pre-tick state is exact
/// for the conditions, and only the noise term is not reproducible.
fn price_ep_auto_deltas(
    sc: &Scenario,
    snap: &BTreeMap<String, std::collections::HashMap<String, f64>>,
    globals: &std::collections::HashMap<String, f64>,
) -> (BTreeMap<String, f64>, BTreeMap<String, f64>) {
    let read = |r: &MetricRef| -> f64 {
        match r {
            MetricRef::Actor { actor_id, metric } => snap
                .get(actor_id.as_str())
                .and_then(|m| m.get(metric.as_str()))
                .copied()
                .unwrap_or(0.0),
            MetricRef::Global { key } => globals.get(key.as_str()).copied().unwrap_or(0.0),
            MetricRef::Family { .. } => 0.0,
        }
    };
    let mut delta: BTreeMap<String, f64> = BTreeMap::new();
    let mut noise: BTreeMap<String, f64> = BTreeMap::new();
    for ad in &sc.auto_deltas {
        let MetricRef::Actor { actor_id, metric } = &ad.metric else { continue };
        if metric.as_str() != "external_pressure" {
            continue;
        }
        let mut d = ad.base;
        for c in &ad.conditions {
            let v = read(&c.metric);
            let hit = match c.operator {
                ComparisonOperator::Less => v < c.value,
                ComparisonOperator::LessOrEqual => v <= c.value,
                ComparisonOperator::Greater => v > c.value,
                ComparisonOperator::GreaterOrEqual => v >= c.value,
                ComparisonOperator::Equal => (v - c.value).abs() < 0.001,
            };
            if hit {
                d += c.delta;
            }
        }
        for rc in &ad.ratio_conditions {
            let b = read(&rc.metric_b);
            if b == 0.0 {
                continue;
            }
            if rc.operator.evaluate(read(&rc.metric_a) / b, rc.ratio) {
                d += rc.delta;
            }
        }
        *delta.entry(actor_id.as_str().to_string()).or_insert(0.0) += d;
        *noise.entry(actor_id.as_str().to_string()).or_insert(0.0) += ad.noise;
    }
    (delta, noise)
}

/// §1 п.2–п.4 — occupancy, the ratchet, the inflow decomposition and the λ sweep.
fn epratchet(scenario_id: &str, ticks: u32, seeds: &[u64], strategy: Option<&str>) {
    use engine13::commands::AppState;

    println!("actor\tseed\tmode\tticks\tep0\tepF\tat100%\tband%\tabove85%\tfirst85\tfirst100\tdrops\tdrop_mass\trelease70\tbelow70\trises\trise_mass\tauto\tevents\tmigration\tinherit\tcombat_resid\tclamp_lost\tfree%\tresid_neg\tdied\tpath\tsaved_l1\tsaved_l4\tsaved_l12\tcw_mism");
    for &seed in seeds {
        let scenario = registry::load_by_id(scenario_id).expect("scenario");
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for a in &scenario.actors {
            if !a.is_successor_template {
                world.actors.insert(a.id.clone(), a.clone());
            }
        }
        if let Some(ref initial_metrics) = scenario.initial_family_metrics {
            let patriarch_age = scenario
                .generation_mechanics
                .as_ref()
                .map(|g| g.patriarch_start_age)
                .unwrap_or(40);
            world.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(initial_metrics),
                patriarch_age,
                generation_count: 0,
            });
        }
        world.generation_mechanics = scenario.generation_mechanics.clone();
        world.generation_length = scenario.generation_length;

        let mut state = AppState {
            world_state: Some(world),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };
        let sc = state.current_scenario.as_ref().unwrap();
        let by_id: BTreeMap<String, (engine13::core::RandomEvent, bool)> = pool_of(sc)
            .into_iter()
            .map(|(e, c)| (e.id.clone(), (e, c)))
            .collect();
        let deps = sc.dependencies.clone();
        let sc_owned = sc.clone();
        let sc_ref = &sc_owned;

        let mut acc: BTreeMap<String, EpActor> = BTreeMap::new();
        let mut collapses = 0u32;
        let mut mig_unresolved = 0u32;

        for t in 0..ticks {
            let (pairs_t, base_pop, ep0, coh0, snapshot, pop_noise, ep_auto, ep_noise) = {
                let world = state.world_state.as_ref().unwrap();
                let pairs = land_pairs(world);
                let mut pop: BTreeMap<String, f64> = BTreeMap::new();
                let mut ep: BTreeMap<String, f64> = BTreeMap::new();
                let mut coh: BTreeMap<String, f64> = BTreeMap::new();
                let mut snap: BTreeMap<String, (f64, f64, f64)> = BTreeMap::new();
                let mut full: BTreeMap<String, std::collections::HashMap<String, f64>> = BTreeMap::new();
                for (aid, a) in world.actors.iter() {
                    if world.dead_actor_ids.contains(aid) {
                        continue;
                    }
                    let (p, eo, e) = (
                        a.get_metric("population"),
                        a.get_metric("economic_output"),
                        a.get_metric("external_pressure"),
                    );
                    snap.insert(aid.clone(), (p, eo, e));
                    full.insert(aid.clone(), a.metrics.clone());
                    pop.insert(aid.clone(), p);
                    ep.insert(aid.clone(), e);
                    coh.insert(aid.clone(), a.get_metric("cohesion"));
                }
                let (ad_pop, ad_pop_noise) = price_pop_auto_deltas(sc_ref, &full, &world.global_metrics);
                for (aid, d) in &ad_pop {
                    if let Some(v) = pop.get_mut(aid) {
                        *v = (*v + d).max(0.0);
                    }
                }
                for (aid, v) in pop.iter_mut() {
                    let eo = snap.get(aid).map(|x| x.1).unwrap_or(0.0);
                    *v -= price_population_rules(&deps, *v, eo);
                }
                let (ad_ep, ad_ep_noise) = price_ep_auto_deltas(sc_ref, &full, &world.global_metrics);
                (pairs, pop, ep, coh, snap, ad_pop_noise, ad_ep, ad_ep_noise)
            };

            // seed
            {
                let world = state.world_state.as_ref().unwrap();
                for (aid, (_p, _eo, e)) in snapshot.iter() {
                    let entry = acc.entry(aid.clone()).or_default();
                    if !entry.seen {
                        entry.seen = true;
                        entry.ep0 = *e;
                        entry.ep_prev = Some(*e);
                        entry.s_ep = [*e; 8];
                        entry.s_nomig = *e;
                        entry.first_85 = -1;
                        entry.first_100 = -1;
                        entry.died_tick = -1;
                        entry.min_surv = world.actors.get(aid).and_then(|a| a.minimum_survival_ticks);
                        entry.neighbors = world
                            .actors
                            .get(aid)
                            .map(|a| a.neighbors.iter().map(|n| (n.id.clone(), n.distance)).collect())
                            .unwrap_or_default();
                    }
                }
            }

            let live_before: std::collections::HashSet<String> = snapshot.keys().cloned().collect();
            let (_applied, _rt) = scripted_step(&mut state, scenario_id, strategy);
            let log_len = state.event_log.events.len();
            {
                let world_state = state.world_state.as_mut().unwrap();
                let scenario_ref = state.current_scenario.as_ref().unwrap();
                let rng = state.rng.as_mut().unwrap();
                tick(world_state, scenario_ref, &mut state.event_log, rng);
            }

            let mut ev_pop: BTreeMap<String, f64> = BTreeMap::new();
            let mut ev_ep: BTreeMap<String, f64> = BTreeMap::new();
            for ev in &state.event_log.events[log_len..] {
                let Some((def, _)) = by_id.get(&ev.id) else { continue };
                for (aid, d) in event_writes_to(def, &ev.actor_id, "population") {
                    *ev_pop.entry(aid).or_insert(0.0) += d;
                }
                for (aid, d) in event_writes_to(def, &ev.actor_id, "external_pressure") {
                    *ev_ep.entry(aid).or_insert(0.0) += d;
                }
            }

            // migration's share of the pressure, under TODAY's arithmetic
            let mig_ep = {
                let world = state.world_state.as_ref().unwrap();
                let mut observed: BTreeMap<String, f64> = BTreeMap::new();
                for aid in base_pop.keys() {
                    if let Some(a) = world.actors.get(aid) {
                        if !world.dead_actor_ids.contains(aid) {
                            observed.insert(aid.clone(), a.get_metric("population"));
                            continue;
                        }
                    }
                    if let Some(d) = world.dead_actors.iter().find(|d| &d.id == aid && d.tick_death == t) {
                        observed.insert(aid.clone(), d.final_metrics.get("population").copied().unwrap_or(0.0));
                    }
                }
                match solve_migration_v2(&pairs_t, &base_pop, &ep0, &coh0, &ev_pop, &observed, &pop_noise) {
                    Some((g, _)) => g,
                    None => {
                        mig_unresolved += 1;
                        BTreeMap::new()
                    }
                }
            };

            let world = state.world_state.as_ref().unwrap();
            let cur_tick = t;
            let just_dead: BTreeMap<String, &std::collections::HashMap<String, f64>> = world
                .dead_actors
                .iter()
                .filter(|d| d.tick_death == cur_tick)
                .map(|d| (d.id.clone(), &d.final_metrics))
                .collect();
            let live_mil: BTreeMap<String, f64> = world
                .actors
                .iter()
                .map(|(k, a)| (k.clone(), a.get_metric("military_size")))
                .collect();

            // a successor born this tick carries `parent_ep × 1.3` (`mod.rs:1644`)
            for (aid, a) in world.actors.iter() {
                if !live_before.contains(aid) && !world.dead_actor_ids.contains(aid) {
                    let e = acc.entry(aid.clone()).or_default();
                    e.p_inherit += a.get_metric("external_pressure");
                }
            }

            let ids: Vec<String> = acc.keys().cloned().collect();
            for aid in ids {
                let alive = world.actors.contains_key(&aid) && !world.dead_actor_ids.contains(&aid);
                let metrics: Option<std::collections::HashMap<String, f64>> = if alive {
                    world.actors.get(&aid).map(|a| a.metrics.clone())
                } else {
                    just_dead.get(&aid).map(|m| (*m).clone())
                };
                let Some(m) = metrics else { continue };
                let ep = m.get("external_pressure").copied().unwrap_or(0.0);
                let coh = m.get("cohesion").copied().unwrap_or(0.0);
                let leg = m.get("legitimacy").copied().unwrap_or(0.0);
                let mil = m.get("military_size").copied().unwrap_or(0.0);
                let e = acc.get_mut(&aid).expect("seeded");
                if !e.seen {
                    continue;
                }
                e.ticks += 1;

                // ---- occupancy ------------------------------------------------
                if ep < 70.0 { e.below_70 += 1; }
                if ep >= 100.0 { e.at_100 += 1; }
                if ep > 85.0 { e.above_85 += 1; }
                if ep > 85.0 && ep < 100.0 { e.in_band += 1; }
                if ep > 85.0 && e.first_85 < 0 { e.first_85 = (t + 1) as i64; }
                if ep >= 100.0 && e.first_100 < 0 { e.first_100 = (t + 1) as i64; }

                // ---- the ratchet, and the inflow decomposition -----------------
                if let Some(prev) = e.ep_prev {
                    let d = ep - prev;
                    if d < -1e-9 { e.drops += 1; e.drop_mass += -d; }
                    if prev >= 70.0 && ep < 70.0 { e.release_70 += 1; }
                    if d > 1e-9 { e.rises += 1; e.rise_mass += d; }
                    let auto = ep_auto.get(&aid).copied().unwrap_or(0.0);
                    let evd = ev_ep.get(&aid).copied().unwrap_or(0.0);
                    let mg = mig_ep.get(&aid).copied().unwrap_or(0.0);
                    e.p_auto += auto;
                    e.p_events += evd;
                    e.p_migration += mg;
                    // At the ceiling the observed delta is not the delivered inflow:
                    // `external_pressure` clamps at 100, so a tick that starts there
                    // absorbs everything its producers wrote. Nominal and delivered
                    // are therefore accounted separately, and the residual — combat,
                    // the one producer that cannot be replicated from outside — is
                    // only taken on ticks where neither end touched the ceiling.
                    let nominal = auto + evd + mg;
                    let free = prev < 100.0 - 1e-9 && ep < 100.0 - 1e-9;
                    if free {
                        e.free_ticks += 1;
                        let resid = d - nominal;
                        if resid < -(ep_noise.get(&aid).copied().unwrap_or(0.0) + 1e-6) {
                            e.resid_negative += -resid;
                        } else {
                            e.p_residual += resid.max(0.0);
                        }
                    } else if nominal > d {
                        e.clamp_absorbed += nominal - d;
                    }
                }
                e.ep_prev = Some(ep);

                // ---- λ sweep in the shadow ------------------------------------
                let besieged = e.neighbors.iter().any(|(nid, dist)| {
                    *dist == 1
                        && live_mil
                            .get(nid)
                            .map(|v| *v >= engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY)
                            .unwrap_or(false)
                });
                let skip = matches!(e.min_surv, Some(ms) if cur_tick < ms);
                let d_ep = ep - e.s_ep[0]; // λ=0 shadow tracks the real value exactly
                for (li, lam) in LAMBDAS.iter().enumerate() {
                    e.s_ep[li] = (e.s_ep[li] + d_ep - lam).clamp(0.0, 100.0);
                }
                e.s_nomig =
                    (e.s_nomig + d_ep - mig_ep.get(&aid).copied().unwrap_or(0.0)).clamp(0.0, 100.0);
                if !skip {
                    let (rc, ri, rq) = danger_paths(coh, leg, ep, mil, besieged);
                    let real_d = rc || ri || rq;
                    if real_d { e.real_ct += 1 } else { e.real_ct = 0 }
                    let engine_ct = world.collapse_warning_ticks.get(&aid).copied().unwrap_or(0);
                    if engine_ct != e.real_ct { e.cw_mismatch += 1; }
                    for li in 0..LAMBDAS.len() {
                        let (sc_, si, sq) = danger_paths(coh, leg, e.s_ep[li], mil, besieged);
                        let shad_d = sc_ || si || sq;
                        if shad_d { e.shad_ct[li] += 1 } else { e.shad_ct[li] = 0 }
                    }
                    let (nc, ni, nq) = danger_paths(coh, leg, e.s_nomig, mil, besieged);
                    if nc || ni || nq { e.nomig_ct += 1 } else { e.nomig_ct = 0 }
                }
                if !alive && e.died_tick < 0 {
                    e.died_tick = (t + 1) as i64;
                    collapses += 1;
                    let (dc, di, dq) = danger_paths(coh, leg, ep, mil, besieged);
                    e.died_path = match (dc, di, dq) {
                        (true, _, _) => "classic",
                        (_, true, _) => "internal",
                        (_, _, true) => "conquest",
                        _ => "none",
                    };
                    // was this death already attributed to migration? — the shadow of
                    // задача 26 §5.2: no migration pressure at all
                    let no_mig = e.s_nomig;
                    e.mig_attributed = e.nomig_ct < 3;
                    if e.mig_attributed { e.saved_nomig += 1; }
                    for li in 0..LAMBDAS.len() {
                        if e.shad_ct[li] < 3 { e.saved[li] += 1; }
                    }
                    println!(
                        "#DEATH27\t{}\t{}\t{}\ttick={}\tpath={}\tep={:.2}\tep_no_mig={:.2}\tmig_attr={}\tsaved_at={}",
                        aid, seed, mode_label(strategy), t + 1, e.died_path, ep, no_mig.max(0.0),
                        e.mig_attributed as u8,
                        LAMBDAS
                            .iter()
                            .enumerate()
                            .find(|(li, _)| e.shad_ct[*li] < 3)
                            .map(|(_, l)| format!("{}", l))
                            .unwrap_or_else(|| "never".to_string())
                    );
                }
            }
        }

        let mode = mode_label(strategy);
        let mut tot_saved = [0u32; 8];
        let mut cwm = 0u32;
        let mut prod = [0.0f64; 5];
        let mut drops = 0u32;
        let mut rel70 = 0u32;
        let mut negres = 0.0f64;
        let mut clamp_lost = 0.0f64;
        let mut saved_nomig = 0u32;
        let mut free_ticks = 0u32;
        let mut all_ticks = 0u32;
        for (aid, e) in &acc {
            for (li, t) in tot_saved.iter_mut().enumerate() { *t += e.saved[li]; }
            cwm += e.cw_mismatch;
            drops += e.drops;
            rel70 += e.release_70;
            negres += e.resid_negative;
            clamp_lost += e.clamp_absorbed;
            saved_nomig += e.saved_nomig;
            free_ticks += e.free_ticks;
            all_ticks += e.ticks;
            prod[0] += e.p_auto; prod[1] += e.p_events; prod[2] += e.p_migration;
            prod[3] += e.p_inherit; prod[4] += e.p_residual;
            let n = e.ticks.max(1) as f64;
            println!(
                "{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{}\t{}\t{}\t{:+.1}\t{}\t{}\t{}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:+.1}\t{:.1}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}",
                aid, seed, mode, e.ticks, e.ep0, e.ep_prev.unwrap_or(0.0),
                100.0 * e.at_100 as f64 / n, 100.0 * e.in_band as f64 / n,
                100.0 * e.above_85 as f64 / n, e.first_85, e.first_100,
                e.drops, e.drop_mass, e.release_70, e.below_70, e.rises, e.rise_mass,
                e.p_auto, e.p_events, e.p_migration, e.p_inherit, e.p_residual,
                e.clamp_absorbed, 100.0 * e.free_ticks as f64 / n, e.resid_negative,
                e.died_tick, e.died_path,
                e.saved[3], e.saved[5], e.saved[7], e.cw_mismatch
            );
        }
        let sweep: Vec<String> = LAMBDAS
            .iter()
            .enumerate()
            .map(|(li, l)| format!("λ={}:{}", l, tot_saved[li]))
            .collect();
        println!(
            "#EP27\tseed={}\tmode={}\tcollapses={}\tdrops={}\trelease70={}\tauto={:+.1}\tevents={:+.1}\tmigration={:+.1}\tinherit={:+.1}\tcombat_resid={:+.1}\tclamp_lost={:+.1}\tfree_ticks={}/{}\tresid_neg={:.2}\tsaved_no_migration={}\tmig_unresolved={}\tcw_mismatch={}\tsweep={}",
            seed, mode, collapses, drops, rel70, prod[0], prod[1], prod[2], prod[3], prod[4],
            clamp_lost, free_ticks, all_ticks, negres,
            saved_nomig, mig_unresolved, cwm, sweep.join(",")
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("inventory");
    match mode {
        "inventory" => inventory_for(args.get(2).map(|s| s.as_str()).unwrap_or("treasury")),
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
        "attractor" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            attractor(scenario, ticks, &seeds, strategy);
        }
        "popevents" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            popevents(scenario, ticks, &seeds, strategy);
        }
        "cohevents" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            cohevents(scenario, ticks, &seeds, strategy);
        }
        "decisive24" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            decisive24(scenario, ticks, &seeds, strategy);
        }
        "decisive23" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            decisive23(scenario, ticks, &seeds, strategy);
        }
        "spawnwalk" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            spawnwalk(scenario, ticks, &seeds, strategy);
        }
        "decisive25" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            let variants: Vec<Variant> = args
                .get(6)
                .map(|s| s.split(',').map(parse_variant).collect())
                .unwrap_or_else(|| vec![parse_variant("base")]);
            decisive25(scenario, ticks, &seeds, strategy, &variants);
        }
        "poolcut" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            let alphas: Vec<f64> = args
                .get(6)
                .map(|s| s.split(',').map(|x| x.parse().expect("alpha")).collect())
                .unwrap_or_else(|| vec![1.0]);
            poolcut(scenario, ticks, &seeds, strategy, &alphas);
        }
        "evaddr" => evaddr(),
        "decisive26" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            decisive26(scenario, ticks, &seeds, strategy);
        }
        "epratchet" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            epratchet(scenario, ticks, &seeds, strategy);
        }
        "migvariants" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            migvariants(scenario, ticks, &seeds, strategy);
        }
        "migwalk" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            migwalk(scenario, ticks, &seeds, strategy);
        }
        "evtarget" => {
            let scenario = args.get(2).expect("scenario id");
            let ticks: u32 = args.get(3).expect("ticks").parse().expect("ticks");
            let seeds: Vec<u64> = args
                .get(4)
                .map(|s| s.split(',').map(|x| x.parse().expect("seed")).collect())
                .unwrap_or_else(|| vec![42]);
            let strategy = args.get(5).map(|s| s.as_str()).filter(|s| *s != "noplayer");
            evtarget(scenario, ticks, &seeds, strategy);
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
