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

use engine13::core::{ComparisonOperator, EventConditionType, MetricRef, Scenario};
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
                .any(|m| tag.metrics_modifier.contains_key(*m));
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

/// Return a violation reason if a split `metric` + `actor_id` condition cannot
/// be resolved to a real metric by the engine, mirroring `eval_metric_condition`.
fn metric_condition_violation(metric: &str, actor_id: &Option<String>) -> Option<String> {
    match actor_id {
        Some(_) => {
            if metric.contains(':') {
                Some(format!(
                    "actor_id is set but metric '{metric}' embeds a scope prefix - \
                     use a bare metric name (e.g. 'legitimacy') together with actor_id"
                ))
            } else {
                None
            }
        }
        None => match MetricRef::parse(metric) {
            // Explicit global:/family: scope, or a well-formed actor:id.metric string.
            MetricRef::Family { .. } => None,
            MetricRef::Actor { .. } => None,
            MetricRef::Global { .. } => {
                if metric.starts_with("global:") {
                    None
                } else {
                    Some(format!(
                        "metric '{metric}' has no actor_id and no explicit scope prefix - \
                         it resolves to global:{metric} and silently reads 0.0, so this \
                         condition can never fire"
                    ))
                }
            }
        },
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
    if let MetricRef::Actor { actor_id, .. } = MetricRef::parse(&vc.metric) {
        return Some(actor_id);
    }
    // Additional conditions may name either the protagonist (a survival gate,
    // e.g. `external_pressure < N`) or an *antagonist* (a suppression gate, e.g.
    // `ottomans.military_size < 40`). A scenario never gates its own victory on
    // the protagonist's military *shrinking*, so a `Less`/`LessOrEqual` bound on
    // `military_size` names the enemy — skip it, don't mistake it for the hero.
    for cond in &vc.additional_conditions {
        if let MetricRef::Actor { actor_id, metric } = MetricRef::parse(&cond.metric) {
            let is_antagonist_suppression = metric == "military_size"
                && matches!(cond.operator, ComparisonOperator::Less | ComparisonOperator::LessOrEqual);
            if !is_antagonist_suppression {
                return Some(actor_id);
            }
        }
    }
    // Federation/patron scenarios whose victory is a global metric gated only by
    // antagonist suppression don't name the protagonist anywhere in the victory
    // condition. Fall back to the survival status indicator: an `invert: true`
    // gauge (lower-is-better, e.g. external_pressure) marks the at-risk actor.
    for ind in &scenario.status_indicators {
        if ind.invert {
            if let MetricRef::Actor { actor_id, .. } = MetricRef::parse(&ind.metric) {
                return Some(actor_id);
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
        let key = format!("actor:{protagonist}.military_size");
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
