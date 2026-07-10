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

use engine13::core::{EventConditionType, MetricRef, Scenario};
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

/// Bug class 2: a `type = "metric"` milestone/rank condition must use the
/// split `metric` + `actor_id` fields, not a merged "actor:X.metric" string.
/// `check_event_condition` (engine/mod.rs) reads `actor_id` as a required
/// `Option<String>` and does a raw `actor.metrics.get(metric)` lookup - if
/// `actor_id` is `None` the condition is unconditionally `false`, and if
/// `metric` still carries an "actor:"/"global:" prefix, the lookup key never
/// matches a real metric name either way. Either mistake makes the
/// milestone dead: it silently never fires, exactly like the bug found in
/// Milan 1477's original milestone_events.toml before it was split into
/// separate fields.
///
/// KNOWN, STILL-OPEN EXCEPTIONS (not covered by this test): `global:`- and
/// `family:`-scoped milestone conditions cannot be expressed correctly at
/// all right now, because `check_event_condition`/`check_rank_conditions`
/// only support the actor-scoped `metric` + `actor_id` lookup - unlike
/// `victory_condition`, which resolves via `MetricRef::parse` and does
/// support those prefixes. Fixing these needs an engine change (bringing
/// `check_event_condition` in line with `MetricRef::parse`), which is out of
/// scope for a content-only check. Tracked as a follow-up engine task, not
/// silently ignored:
///   - constantinople_1430: `mehmed_accelerates`, `outcome_best`,
///     `outcome_fell_federation` (all `global:federation_progress`; see the
///     header comment in constantinople_1430/milestone_events.toml). The
///     scenario's actual ending is unaffected - it fires through
///     `check_victory_condition`, which resolves `global:` correctly on its
///     own separate path.
///   - rome_375: `family_rises`, `family_falls` (both
///     `family:family_influence`; flavor-only, not on the collapse path).
const KNOWN_UNSCOPED_METRIC_EXCEPTIONS: &[(&str, &str)] = &[
    ("constantinople_1430", "mehmed_accelerates"),
    ("constantinople_1430", "outcome_best"),
    ("constantinople_1430", "outcome_fell_federation"),
    ("rome_375", "family_rises"),
    ("rome_375", "family_falls"),
];

#[test]
fn milestone_metric_conditions_use_split_actor_id_format() {
    let mut failures = Vec::new();
    for &id in SCENARIO_IDS {
        let scenario = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));
        for milestone in &scenario.milestone_events {
            if KNOWN_UNSCOPED_METRIC_EXCEPTIONS.contains(&(id, milestone.id.as_str())) {
                continue;
            }
            if let EventConditionType::Metric { metric, actor_id, .. } = &milestone.condition.condition_type {
                if actor_id.is_none() {
                    failures.push(format!(
                        "{id}: milestone '{}' has type=metric with no actor_id (metric = '{}') - \
                         check_event_condition returns false unconditionally in this case, so this \
                         milestone can never fire",
                        milestone.id, metric
                    ));
                } else if metric.contains(':') {
                    failures.push(format!(
                        "{id}: milestone '{}' metric '{}' still embeds an 'actor:'/'global:' prefix - \
                         use a bare metric name together with actor_id instead",
                        milestone.id, metric
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Milestone condition format violation(s) - these milestones are dead content:\n{}",
        failures.join("\n")
    );
}

/// Determine the scenario's protagonist actor: the one whose survival /
/// growth the scenario is actually about. Prefer the explicit
/// `player_actor_id`; scenarios that leave it `None` (e.g. a federation
/// scenario played through patrons) are inferred from the actor scope of
/// their `victory_condition`, which always names the at-risk actor.
fn protagonist_actor_id(scenario: &Scenario) -> Option<String> {
    if let Some(ref id) = scenario.player_actor_id {
        return Some(id.clone());
    }
    let vc = scenario.victory_condition.as_ref()?;
    if let MetricRef::Actor { actor_id, .. } = MetricRef::parse(&vc.metric) {
        return Some(actor_id);
    }
    for cond in &vc.additional_conditions {
        if let MetricRef::Actor { actor_id, .. } = MetricRef::parse(&cond.metric) {
            return Some(actor_id);
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
