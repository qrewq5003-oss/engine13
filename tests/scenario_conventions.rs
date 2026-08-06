//! Automated scenario-convention checks.
//!
//! Each test here targets one of the four bug classes found during the
//! Milan 1477 (scenario #3) playtest cycle — all four were content that
//! silently diverged from the convention already followed by rome_375 /
//! constantinople_1430, and none were caught by existing static checks or
//! `cargo test`. See ENGINE13_INFRASTRUCTURE_TASKS.md, Задача 1.
//!
//! These tests are content checks only — they load scenarios through the
//! normal registry and inspect config, they do not modify `engine/`.

use engine13::core::{ComparisonOperator, EventConditionType, MetricName, MetricRef, Scenario};
use engine13::scenarios::registry;
use std::process::Command;

const SCENARIO_IDS: &[&str] = &["rome_375", "constantinople_1430", "milan_1477"];

const GUARDED_METRICS: &[&str] = &["legitimacy", "cohesion"];

/// Bug class 1: a tag that modifies a guarded metric (legitimacy/cohesion)
/// must not spread (spread_chance must be 0.0). Cultural/trade/war contagion
/// across a dense neighbor graph stacks these modifiers on every actor
/// within a few dozen ticks, saturating the metric at its clamp - this is
/// exactly what made the vassalage band unreachable in Milan 1477 before
/// the tags were fixed (see tags.toml comment on the `oligarchy` tag there).
#[test]
fn tags_touching_guarded_metrics_do_not_spread() {
    let mut failures = Vec::new();
    for &id in SCENARIO_IDS {
        let scenario = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));
        for tag in &scenario.tag_definitions {
            let touches_guarded = GUARDED_METRICS
                .iter()
                .any(|m| tag.metrics_modifier.contains_key(&MetricName::new(m).unwrap()));
            if touches_guarded && tag.spread_chance != 0.0 {
                failures.push(format!(
                    "{id}: tag '{}' modifies a guarded metric {:?} but spread_chance = {} (must be 0.0)",
                    tag.id, tag.metrics_modifier, tag.spread_chance
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Contagious guarded-metric tag(s) found:\n{}",
        failures.join("\n")
    );
}

/// Bug class 2: a `type = "metric"` milestone/rank condition must be resolvable
/// by the engine to a real metric value. `check_event_condition` and
/// `check_rank_conditions` (engine/mod.rs) route every metric condition through
/// the shared `eval_metric_condition`, which mirrors `MetricRef`:
///   - `actor_id = Some(id)`: an actor-scoped lookup - `metric` must be a BARE
///     metric name ("legitimacy"), never a prefixed string, or the lookup key
///     never matches a real metric.
///   - `actor_id = None`: the `metric` string carries its own scope, parsed by
///     `MetricRef::parse` - it must start with an explicit `global:`/`family:`
///     prefix, or be an `actor:id.metric` string. A bare metric with no
///     `actor_id` resolves to `global:<name>`, silently reading 0.0.
///
/// Getting this wrong makes the milestone/rank condition dead: it silently
/// never fires, exactly like the bug found in Milan 1477's original
/// milestone_events.toml before it was split into separate fields.
///
/// This check covers milestone conditions AND rank conditions (both share
/// `eval_metric_condition`). `global:`/`family:`-scoped conditions with no
/// `actor_id` are now VALID and expected to pass - the engine resolves them
/// the same way `victory_condition` does (see ENGINE13_INFRASTRUCTURE_TASKS.md
/// Задача 4). The previously-allowlisted dead conditions
/// (`mehmed_accelerates`, `outcome_best`, `outcome_fell_federation`,
/// `family_rises`, `family_falls`, and the anatolia/veneto/lombardy rank
/// conditions) are covered here and must resolve cleanly.

/// Return a violation reason if a split `metric` + `actor_id` condition did not
/// fold into the address the content meant.
///
/// Most of what this used to check is now impossible to express: a bare metric
/// with no `actor_id` and no prefix, a scope prefix *next to* an `actor_id`, a
/// dotted phantom key — none of them can become a `MetricRef` at all, so they
/// fail at load rather than reaching this test. What is still worth pinning is the
/// fold itself: an `actor_id`-scoped condition must have resolved onto *that*
/// actor, not somewhere else.
fn metric_condition_violation(metric: &MetricRef, actor_id: &Option<String>) -> Option<String> {
    let Some(aid) = actor_id else { return None };
    match metric {
        MetricRef::Actor { actor_id: resolved, .. } if resolved.as_str() == aid => None,
        other => Some(format!(
            "actor_id is '{aid}' but the key resolved to '{other}' - the load-time \
             scope fold did not bind this condition to its actor"
        )),
    }
}

#[test]
fn milestone_and_rank_metric_conditions_are_resolvable() {
    let mut failures = Vec::new();
    for &id in SCENARIO_IDS {
        let scenario = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));

        for milestone in &scenario.milestone_events {
            if let EventConditionType::Metric { metric, actor_id, .. } = &milestone.condition.condition_type {
                if let Some(reason) = metric_condition_violation(metric, actor_id) {
                    failures.push(format!("{id}: milestone '{}': {reason}", milestone.id));
                }
            }
        }

        for rank in &scenario.rank_conditions {
            if let EventConditionType::Metric { metric, actor_id, .. } = &rank.condition.condition_type {
                if let Some(reason) = metric_condition_violation(metric, actor_id) {
                    failures.push(format!("{id}: rank condition '{}': {reason}", rank.region_id));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Metric-condition resolution violation(s) - these milestones/rank conditions are dead content:\n{}",
        failures.join("\n")
    );
}

/// Determine the scenario's protagonist actor: the one whose survival /
/// growth the scenario is actually about. Prefer the explicit
/// `player_actor_id`; scenarios that leave it `None` (e.g. a federation
/// scenario played through patrons) are inferred from the victory_condition
/// and, failing that, from the survival status indicator.
fn protagonist_actor_id(scenario: &Scenario) -> Option<String> {
    if let Some(ref id) = scenario.player_actor_id {
        return Some(id.clone());
    }
    let vc = scenario.victory_condition.as_ref()?;
    if let MetricRef::Actor { actor_id, .. } = &vc.metric {
        return Some(actor_id.to_string());
    }
    // Additional conditions may name either the protagonist (a survival gate,
    // e.g. `external_pressure < N`) or an *antagonist* (a suppression gate, e.g.
    // `ottomans.military_size < 40`). A scenario never gates its own victory on
    // the protagonist's military *shrinking*, so a `Less`/`LessOrEqual` bound on
    // `military_size` names the enemy — skip it, don't mistake it for the hero.
    for cond in &vc.additional_conditions {
        if let MetricRef::Actor { actor_id, metric } = &cond.metric {
            let is_antagonist_suppression = metric.as_str() == "military_size"
                && matches!(cond.operator, ComparisonOperator::Less | ComparisonOperator::LessOrEqual);
            if !is_antagonist_suppression {
                return Some(actor_id.to_string());
            }
        }
    }
    // Federation/patron scenarios whose victory is a global metric gated only by
    // antagonist suppression don't name the protagonist anywhere in the victory
    // condition. Fall back to the survival status indicator: an `invert: true`
    // gauge (lower-is-better, e.g. external_pressure) marks the at-risk actor.
    for ind in &scenario.status_indicators {
        if ind.invert {
            if let MetricRef::Actor { actor_id, .. } = &ind.metric {
                return Some(actor_id.to_string());
            }
        }
    }
    None
}

/// Bug class 3: the scenario must have at least one action that grows the
/// protagonist's `military_size`. `military_quality` alone is not enough -
/// there is no `quality -> size` feedback loop in the engine, so a scenario
/// with only quality levers can never grow military_size at all.
#[test]
fn scenario_has_military_size_growth_lever_for_protagonist() {
    let mut failures = Vec::new();
    for &id in SCENARIO_IDS {
        let scenario = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));
        let Some(protagonist) = protagonist_actor_id(&scenario) else {
            failures.push(format!(
                "{id}: could not determine a protagonist actor (no player_actor_id and no \
                 actor-scoped victory_condition to infer one from)"
            ));
            continue;
        };
        // Built through the constructor, not `format!` — the very hazard this task exists to remove.
        let key = MetricRef::actor(&protagonist, "military_size").expect("protagonist key");
        let has_lever = scenario
            .universal_actions
            .iter()
            .chain(scenario.patron_actions.iter())
            .any(|a| a.effects.get(&key).copied().unwrap_or(0.0) > 0.0);
        if !has_lever {
            failures.push(format!(
                "{id}: no action has a positive effect on '{key}' - protagonist military_size \
                 can never grow"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "Missing military_size growth lever(s):\n{}",
        failures.join("\n")
    );
}

/// Bug class 4: `ScriptedStrategy::from_str` (src/bin/sim.rs) must have a
/// real branch for every scenario_id, using that scenario's own action IDs.
/// A scenario that falls through to another scenario's default silently
/// applies zero actions the whole run (this is exactly how the missing
/// Milan branch was found: 0/320 actions applied). This is verified
/// black-box, by actually running the `sim` binary in scripted mode and
/// checking it applies at least one action - a source-level check would
/// have to duplicate the from_str mapping and could drift from it
/// independently.
#[test]
fn scripted_strategy_applies_actions_for_every_scenario() {
    let mut failures = Vec::new();
    for &id in SCENARIO_IDS {
        let output = Command::new(env!("CARGO_BIN_EXE_sim"))
            .args([id, "60", "scripted", "balanced", "42"])
            .output()
            .unwrap_or_else(|e| panic!("{id}: failed to run sim binary: {e}"));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let applied = stdout.lines().find_map(|l| {
            l.strip_prefix("Total actions applied: ")
                .and_then(|n| n.trim().parse::<u32>().ok())
        });
        match applied {
            Some(0) => failures.push(format!(
                "{id}: scripted strategy applied 0 actions over 60 ticks - ScriptedStrategy::from_str \
                 likely has no branch for this scenario_id and fell through to another scenario's \
                 action IDs (sim.rs)"
            )),
            Some(_) => {}
            None => failures.push(format!(
                "{id}: could not find a 'Total actions applied: N' line in sim output"
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "Scripted-strategy dead-action finding(s):\n{}",
        failures.join("\n")
    );
}

/// Bug class 5 (Задача 12 §22.5, closed by Задача 13): **metric names had no
/// allowlist anywhere except `dependencies.toml`.**
///
/// `MetricName` now guarantees the *shape* of a bare key (no prefix, no dot), but
/// a type cannot know which names the engine actually recognises. Nothing stopped
/// content from writing `auto_deltas`, an action effect, a tag modifier or a rank
/// bonus against an invented name — a typo (`legitimicy`), or an engine-internal
/// metric a future mechanic owns. Either lands in `actor.metrics` as a brand-new
/// key that no formula reads and no clamp bounds, and stays silent forever: the
/// same failure mode as the phantom global, one level down.
///
/// This walks every metric key the three scenarios write or read and asserts its
/// bare name is one the engine knows.
const ENGINE_ACTOR_METRICS: &[&str] = &[
    // Clamped and driven by the engine (`clamp_metrics`, `interactions.rs`).
    "population",
    "military_size",
    "military_quality",
    "economic_output",
    "cohesion",
    "legitimacy",
    "external_pressure",
    "treasury",
    // Written by `check_vassalage` (Задача A), read by milan's victory milestone.
    "expansion_count",
];

/// Family metrics are stored unprefixed; content may spell either form.
const ENGINE_FAMILY_METRICS: &[&str] = &[
    "influence",
    "knowledge",
    "wealth",
    "connections",
    "family_influence",
    "family_knowledge",
    "family_wealth",
    "family_connections",
    // milan writes these two through patron actions while having no family_state at
    // all, so they land nowhere. Dead content, not a name error — see the Задача 13
    // writeup; kept here so this test fails on *unknown* names, not on that.
    "family_cohesion",
    "family_legitimacy",
];

const ENGINE_GLOBAL_METRICS: &[&str] = &["federation_progress"];

/// **Known-inert, deliberately not fixed here. Do not extend this list.**
///
/// `constantinople_1430/auto_deltas.toml` opens with five blocks that omit
/// `actor_id`, under a comment that reads *"actor_id omitted = None = applies to
/// all"*. The engine has no such mechanism: with no actor context a bare key is a
/// **global**, so those blocks write `population` / `military_size` / `cohesion` /
/// `legitimacy` / `external_pressure` into `world.global_metrics`, where nothing
/// reads them. Constantinople's actors get no base drift on any of the five, and
/// have not for the whole history of the project — the ninth instance of the
/// metric-scoping class, and the one no guard could see because the *shape* of the
/// key is perfectly legal.
///
/// It is listed rather than fixed because fixing it turns five dead auto_deltas on
/// in a scenario whose balance is calibrated with them off (Задача 6): a balance
/// change, and Задача 13's only acceptance criterion is byte-identical output.
/// Tracked as a separate finding.
const KNOWN_INERT_GLOBAL_NAMES: &[(&str, &str)] = &[
    ("constantinople_1430", "population"),
    ("constantinople_1430", "military_size"),
    ("constantinople_1430", "cohesion"),
    ("constantinople_1430", "legitimacy"),
    ("constantinople_1430", "external_pressure"),
    ("constantinople_1430", "treasury"),
    ("constantinople_1430", "economic_output"),
];

fn check_name(scenario_id: &str, r: &MetricRef, ctx: &str, failures: &mut Vec<String>) {
    let (allowed, name, kind) = match r {
        MetricRef::Actor { metric, .. } => (ENGINE_ACTOR_METRICS, metric.as_str(), "actor"),
        MetricRef::Family { key } => (ENGINE_FAMILY_METRICS, key.as_str(), "family"),
        MetricRef::Global { key } => (ENGINE_GLOBAL_METRICS, key.as_str(), "global"),
    };
    if kind == "global" && KNOWN_INERT_GLOBAL_NAMES.contains(&(scenario_id, name)) {
        return;
    }
    if !allowed.contains(&name) {
        failures.push(format!(
            "{ctx}: unknown {kind} metric name '{name}' — the engine reads no such metric, \
             so this key is inert (a typo, or a metric only the engine should own)"
        ));
    }
}

fn check_bare_name(name: &MetricName, ctx: &str, failures: &mut Vec<String>) {
    if !ENGINE_ACTOR_METRICS.contains(&name.as_str()) {
        failures.push(format!(
            "{ctx}: unknown actor metric name '{name}' — the engine reads no such metric"
        ));
    }
}

#[test]
fn content_only_names_metrics_the_engine_knows() {
    let mut failures = Vec::new();
    for &id in SCENARIO_IDS {
        let s = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));

        for (i, d) in s.auto_deltas.iter().enumerate() {
            check_name(id, &d.metric, &format!("{id}: auto_delta[{i}]"), &mut failures);
            for c in &d.conditions {
                check_name(id, &c.metric, &format!("{id}: auto_delta[{i}].condition"), &mut failures);
            }
            for r in &d.ratio_conditions {
                check_name(id, &r.metric_a, &format!("{id}: auto_delta[{i}].ratio_a"), &mut failures);
                check_name(id, &r.metric_b, &format!("{id}: auto_delta[{i}].ratio_b"), &mut failures);
            }
        }

        for a in s.patron_actions.iter().chain(s.universal_actions.iter()) {
            for m in a.effects.keys().chain(a.cost.keys()) {
                check_name(id, m, &format!("{id}: action '{}'", a.id), &mut failures);
            }
        }

        for m in &s.milestone_events {
            if let Some(r) = m.condition.metric_ref() {
                check_name(id, r, &format!("{id}: milestone '{}'", m.id), &mut failures);
            }
            if let Some(cfg) = &m.spawn_actor {
                for k in cfg.initial_metrics.keys() {
                    check_bare_name(k, &format!("{id}: spawn '{}'", cfg.actor_id), &mut failures);
                }
            }
        }
        for rc in &s.rank_conditions {
            if let Some(r) = rc.condition.metric_ref() {
                check_name(id, r, &format!("{id}: rank '{}'", rc.region_id), &mut failures);
            }
        }

        for t in &s.tag_definitions {
            for k in t.metrics_modifier.keys() {
                check_bare_name(k, &format!("{id}: tag '{}'", t.id), &mut failures);
            }
        }
        for rb in &s.rank_bonuses {
            for e in &rb.effects {
                check_bare_name(&e.metric, &format!("{id}: rank_bonus"), &mut failures);
            }
        }
        for d in &s.dependencies {
            check_bare_name(&d.from, &format!("{id}: dependency '{}'.from", d.id), &mut failures);
            check_bare_name(&d.to, &format!("{id}: dependency '{}'.to", d.id), &mut failures);
        }

        for ind in &s.status_indicators {
            check_name(id, &ind.metric, &format!("{id}: status_indicator"), &mut failures);
        }
        for m in &s.narrative_config.key_metrics {
            check_name(id, m, &format!("{id}: narrative key_metric"), &mut failures);
        }
        if let Some(vc) = &s.victory_condition {
            check_name(id, &vc.metric, &format!("{id}: victory_condition"), &mut failures);
            for c in &vc.additional_conditions {
                check_name(id, &c.metric, &format!("{id}: victory additional_condition"), &mut failures);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Content names {} metric(s) the engine does not read:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
