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

use engine13::core::{
    ComparisonOperator, EventConditionType, EventTarget, MetricName, MetricRef, RandomEvent,
    RelativeCondition, RelativeMetricRef, Scenario, TagDefinition, TagSpreadType,
};
use engine13::scenarios::registry;
use std::collections::HashMap;
use std::process::Command;

const SCENARIO_IDS: &[&str] = &["rome_375", "constantinople_1430", "milan_1477"];

/// The clamped-to-`0..100` metrics a contagious tag may not write.
///
/// `legitimacy` and `cohesion` have been here since задача 1. `external_pressure`
/// was added by задача 28, which measured the whole channel end to end
/// (`docs/investigation_tag_channel.md`) and closed it **as a convention, not as a
/// balance change**: the nine tags that carry the pattern today are listed in
/// [`KNOWN_CONTAGIOUS_TAG_EXCEPTIONS`] with their numbers, and not one
/// `spread_chance` was touched.
///
/// The remaining two clamped metrics — `military_quality` (13 contagious pairs) and
/// `economic_output` (19) — are deliberately **not** here. Задача 28 §9 measured why
/// they are not the same case: `economic_output` sits at its clamp `84.9…94.7 %` of
/// actor-ticks (a step, like `external_pressure`), but `military_quality` sits there
/// only `18.9…56.9 %` — it is still a scale, and its one behavioural reader is
/// `power_projection`, i.e. the relevance channel задача 25 moved by `−50.9 %`.
/// Guarding it would be an unannounced balance change, so it is named and left.
const GUARDED_METRICS: &[&str] = &["legitimacy", "cohesion", "external_pressure"];

/// Contagious tags that write a guarded metric and are kept that way **on purpose**,
/// each with the number that justifies it — the same shape as
/// [`KNOWN_EVENT_ADDRESSING_EXCEPTIONS`] below, and for the same reason: the decision
/// stays visible, and a tenth such tag cannot be added silently.
///
/// The entry is per *(scenario, tag, metric)*, not per tag: a tag excused for
/// `external_pressure` is still caught the moment it starts writing `cohesion` or
/// `legitimacy`.
///
/// # Why these nine are excused
///
/// Задача 28 measured the counterfactual in shadow over nine configurations
/// (`314 026` actor-ticks, `254` collapses) and found that removing the tag channel
/// **entirely** — a far stronger edit than zeroing these nine chances —
///
/// * removes **0 of 254** deaths, on both branches of the bracket. 109 of them die
///   through `internal_collapse`, whose predicate does not read `external_pressure`
///   at all; for the other 145 the death-tick predicate stays true on the shadow
///   value in **every** case;
/// * leaves the vassalage band at **0 of 314 026** actor-ticks, exactly where it is
///   today — the band is bound by `legitimacy ∈ [10,25] ∧ cohesion ∈ [15,30]`, not by
///   pressure;
/// * does **not** make the metric readable as a scale: occupancy of the meaningful
///   `[70, 85]` band goes `1.82 % → 1.43 %`, i.e. it gets *worse*;
/// * does break something that is calibrated: rome's `early_transfer` gate falls from
///   `93.1 %` to `42.1 %` occupancy, which turns 4 generation transfers into 3 and
///   collides head-on with задача 15.
///
/// So the pattern is left running and written down. Per tag, in the units the walk
/// produced (`noplayer`, 10 seeds, 300 ticks, nominal `external_pressure` per game):
///
/// | scenario | tag | `+ep` | mass/game | delivered by | share of the scenario's `ep` inflow |
/// |---|---|---|---|---|---|
/// | constantinople | `crusade_caller` | +1 | 2 329 | **Culture alone**, 7.8 of 7.8 acquisitions | 61.6…75.1 % (all tags) |
/// | constantinople | `ottoman_frontier` | +1 | 1 844 | **War alone**, 4.6 of 5.6 | — |
/// | milan | `ottoman_frontier` | **+2** | 6 642 | **War alone**, 11.6 of 12.8 | 70.9…71.4 % (all tags) |
/// | milan | `french_orbit` | +1 | 3 303 | **Culture alone**, 12.0 of 12.1 | — |
/// | rome | `migrating` | +1 | 3 798 | **Migration alone**, 14.0 of 14.0 | 93.4…93.9 % (all tags) |
/// | rome | `roman_frontier` | +1 | 3 981 | Trade *or* War, **neither necessary** | — |
/// | rome | `roman_border` | +1 | 3 999 | Trade *or* War, **neither necessary** | — |
/// | rome | `rhine_border` | +1 | 1 244 | War, only **2.0** targets, first at tick **127.8** | — |
/// | rome | `persian_border` | +1 | 98 | War, **0.0** targets in `noplayer`/`wealth` | — |
///
/// Three of the nine also write a second clamped metric, and that is part of why the
/// chance was not zeroed: `ottoman_frontier` (constantinople) carries
/// `military_quality +1`, `roman_frontier` and `roman_border` carry
/// `economic_output +1`. Zeroing their spread would silently switch off contagion for
/// a metric this list does not guard — `87.7 %` of the tag-borne `military_quality`
/// mass in constantinople, `87.0 %` of the `economic_output` mass in rome.
///
/// The last two rows are the weakest cases and are marked as such: `persian_border`
/// and `rhine_border` are declared contagious and are all but inert
/// (`(D₆)` of задача 28). They are listed because they carry the pattern **today**,
/// not because the pattern earns its keep there.
const KNOWN_CONTAGIOUS_TAG_EXCEPTIONS: &[(&str, &str, &str)] = &[
    ("constantinople_1430", "crusade_caller", "external_pressure"),
    ("constantinople_1430", "ottoman_frontier", "external_pressure"),
    ("milan_1477", "ottoman_frontier", "external_pressure"),
    ("milan_1477", "french_orbit", "external_pressure"),
    ("rome_375", "migrating", "external_pressure"),
    ("rome_375", "rhine_border", "external_pressure"),
    ("rome_375", "persian_border", "external_pressure"),
    ("rome_375", "roman_frontier", "external_pressure"),
    ("rome_375", "roman_border", "external_pressure"),
];

/// The check itself, over a slice of tags, so that it can be applied to real content
/// *and* to synthetic cases (see [`contagious_tag_check_catches_a_new_violator`]).
fn contagious_guarded_tag_violations(scenario_id: &str, tags: &[TagDefinition]) -> Vec<String> {
    let mut failures = Vec::new();
    for tag in tags {
        if tag.spread_chance == 0.0 {
            continue;
        }
        for metric in GUARDED_METRICS {
            if !tag
                .metrics_modifier
                .contains_key(&MetricName::new(metric).unwrap())
            {
                continue;
            }
            if KNOWN_CONTAGIOUS_TAG_EXCEPTIONS.contains(&(scenario_id, tag.id.as_str(), metric)) {
                continue;
            }
            failures.push(format!(
                "{scenario_id}: tag '{}' modifies the guarded metric '{metric}' but \
                 spread_chance = {} (must be 0.0, or be listed in \
                 KNOWN_CONTAGIOUS_TAG_EXCEPTIONS with the number that justifies it)",
                tag.id, tag.spread_chance
            ));
        }
    }
    failures
}

/// Bug class 1: a tag that modifies a guarded metric (legitimacy / cohesion /
/// external_pressure) must not spread (spread_chance must be 0.0) unless it is on
/// [`KNOWN_CONTAGIOUS_TAG_EXCEPTIONS`]. Cultural/trade/war contagion
/// across a dense neighbor graph stacks these modifiers on every actor
/// within a few dozen ticks, saturating the metric at its clamp - this is
/// exactly what made the vassalage band unreachable in Milan 1477 before
/// the tags were fixed (see tags.toml comment on the `oligarchy` tag there).
///
/// Задача 28 measured the same saturation for `external_pressure` and confirmed the
/// docstring above is describing a real mechanism, not a fear: every one of the
/// three scenarios reaches `ep = 100` on `85.5 %` of actor-ticks, and the tag channel
/// supplies `61.6…93.9 %` of the inflow that puts it there.
#[test]
fn tags_touching_guarded_metrics_do_not_spread() {
    let mut failures = Vec::new();
    for &id in SCENARIO_IDS {
        let scenario = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));
        failures.extend(contagious_guarded_tag_violations(id, &scenario.tag_definitions));
    }
    assert!(
        failures.is_empty(),
        "Contagious guarded-metric tag(s) found:\n{}",
        failures.join("\n")
    );
}

/// The guard on the guard: a contagious tag that writes a guarded metric and is
/// *not* on the allowlist must be caught, a listed one must not be, and the
/// allowlist must be per metric rather than per tag. Synthetic tags, so this keeps
/// working if the real content is someday changed.
#[test]
fn contagious_tag_check_catches_a_new_violator() {
    let tag = |id: &str, metric: &str, chance: f64| TagDefinition {
        id: id.to_string(),
        metrics_modifier: HashMap::from([(MetricName::new(metric).unwrap(), 1)]),
        spreads_via: vec![TagSpreadType::War],
        spread_cooldown_ticks: 6,
        spread_chance: chance,
        requires_era: None,
        unlocks: Vec::new(),
    };

    // clean: writes a guarded metric but does not spread
    assert!(
        contagious_guarded_tag_violations("synthetic", &[tag("quiet", "cohesion", 0.0)]).is_empty()
    );
    // clean: spreads, but writes nothing guarded
    assert!(contagious_guarded_tag_violations(
        "synthetic",
        &[tag("harmless", "military_quality", 0.25)]
    )
    .is_empty());

    // a new contagious `external_pressure` tag, not on the allowlist — the case
    // задача 28 exists to make impossible to add silently
    let new_ep = contagious_guarded_tag_violations(
        "rome_375",
        &[tag("new_frontier", "external_pressure", 0.25)],
    );
    assert_eq!(new_ep.len(), 1, "a new contagious ep tag must be caught: {new_ep:?}");
    assert!(new_ep[0].contains("KNOWN_CONTAGIOUS_TAG_EXCEPTIONS"));

    // the two older guarded metrics still behave as they did before задача 28
    for metric in ["legitimacy", "cohesion"] {
        let v =
            contagious_guarded_tag_violations("milan_1477", &[tag("new_court", metric, 0.2)]);
        assert_eq!(v.len(), 1, "a contagious {metric} tag must be caught: {v:?}");
    }

    // the allowlist is per metric: an excused tag that starts writing a *second*
    // guarded metric is caught for that one
    let mut widened = tag("migrating", "external_pressure", 0.35);
    widened
        .metrics_modifier
        .insert(MetricName::new("cohesion").unwrap(), -1);
    let v = contagious_guarded_tag_violations("rome_375", &[widened]);
    assert_eq!(v.len(), 1, "the ep exception must not excuse cohesion too: {v:?}");
    assert!(v[0].contains("'cohesion'"));

    // and it is per scenario: `ottoman_frontier` is excused in constantinople and in
    // milan, but the same id in rome would be a new violator
    let v = contagious_guarded_tag_violations(
        "rome_375",
        &[tag("ottoman_frontier", "external_pressure", 0.25)],
    );
    assert_eq!(v.len(), 1, "the exception must not travel between scenarios: {v:?}");

    // every listed exception must name a metric this test actually guards
    for (_, tag_id, metric) in KNOWN_CONTAGIOUS_TAG_EXCEPTIONS {
        assert!(
            GUARDED_METRICS.contains(metric),
            "exception for '{tag_id}' names '{metric}', which is not guarded"
        );
    }
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
    // rome writes these two through its universal actions `support_stability` and
    // `raise_taxes` (`rome_375/actions.toml`), and rome *does* have family_state, so
    // they land: `MetricRef::Family::apply` opens them with `or_insert` in the
    // Consequences/Free modes where those actions are offered. They are simply
    // metrics that no inheritance coefficient and no reader knows about — not a name
    // error. Kept here so this test fails on *unknown* names, not on these.
    //
    // An earlier note here credited them to milan and called them dead content; both
    // halves were wrong (milan has no family reference at all), and that
    // misattribution is what let §5.G count the runtime key space as four keys
    // instead of six. See `docs/investigation_typed_metric_keys.md` §5.G, the
    // задача 15 clarification block.
    "family_cohesion",
    "family_legitimacy",
];

const ENGINE_GLOBAL_METRICS: &[&str] = &["federation_progress"];

/// Empty, and that is the point: it once carried the ninth site.
///
/// `constantinople_1430/auto_deltas.toml` used to open with five blocks that
/// omitted `actor_id`, under a comment reading *"actor_id omitted = None =
/// applies to all"*. The engine has no such mechanism: with no actor context a
/// bare key is a **global**, so those blocks wrote `population` / `military_size`
/// / `cohesion` / `legitimacy` / `external_pressure` into `world.global_metrics`,
/// where nothing read them — and their conditions read the same dead globals,
/// which is why `treasury` and `economic_output` were listed too. Constantinople's
/// actors got no base drift on any of the five, for the whole history of the
/// project.
///
/// Задача 13 listed them instead of fixing them: reviving five dead auto_deltas
/// changes the balance of a scenario calibrated with them off (Задача 6), and that
/// task's only acceptance criterion was byte-identical output. **Задача 18 closed
/// it by deletion, not revival** — reviving would add a mechanic rather than fix a
/// bug, and the `external_pressure` block (`+5.0` per tick to any actor below 20
/// military) collides with `classic_collapse` by construction. See задача 18 in
/// `ENGINE13_INFRASTRUCTURE_TASKS.md`.
///
/// Keep it empty. An entry here means content is writing a global the engine does
/// not read — the exact shape this guard exists to catch.
const KNOWN_INERT_GLOBAL_NAMES: &[(&str, &str)] = &[];
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

/// Bug class 6 (Задача 24, стадия 2): **an event whose target receives none of
/// its effects, or whose gate reads a third actor.**
///
/// `phase_random_events` (`engine/mod.rs:396–520`) carries addressing in *three*
/// slots, and they are not the same slot:
///   - `target` — eligibility and attribution. For `EventTarget::Actor(id)` the
///     only test is that the actor exists and is not dead; `Foreground` plays no
///     part. The chosen id becomes the `actor_id` of the logged `Event`, i.e. who
///     the chronicle says the event was about.
///   - `conditions` — frequency. Resolved through `RelativeMetricRef::resolve`
///     against the target, but that call binds to the target **only** for the
///     `self.<metric>` form; any other key is `Absolute` and ignores the target
///     entirely (`core/metric_ref.rs:404–413`).
///   - `effects` — the write, through the same call and the same dichotomy.
///
/// So content can name an actor in `target`, gate on a second, and write to a
/// third, and nothing anywhere complains. Two shapes of that are pathological,
/// and the walk in `docs/investigation_event_target_addressing.md` §3 found each
/// of them exactly once in 46 events:
///   1. **no effect reaches the target** — the event fires "on" an actor that
///      receives nothing, so the config cannot tell you who it hurts without
///      reading all three slots (`mehmed_threatens`);
///   2. **a condition reads a different actor** — the event's frequency is
///      controlled by someone who is neither its target nor (necessarily) its
///      victim, which is how a gate can be identically false forever without
///      anyone noticing (`barbarian_raid`).
///
/// What is deliberately NOT a violation, because the same walk first flagged it
/// and the flag was wrong (§3, "третий флаг обхода снят как ложный"): an effect
/// addressed to a *non-target actor* while some other effect does reach the
/// target — that is the idiom of nearly every scenario event (`crusade_call`
/// writes `hungary.military_size`, `ottoman_spy_caught` writes
/// `ottomans.external_pressure`) — and a `family:`/`global:` gate, which
/// addresses no actor at all and so cannot be "on the wrong one"
/// (`senator_bribe` gates on `family:wealth`).
#[derive(PartialEq, Clone, Copy, Debug)]
enum AddressingRule {
    /// at least one effect must land on the event's own target
    EffectsReachTarget,
    /// no condition may read a *different* actor
    GateStaysOnTarget,
}

/// The two events that violate the rules today, allowlisted the same way
/// [`KNOWN_INERT_GLOBAL_NAMES`] allowlists content задача 18 chose not to revive.
///
/// Both are **decisions, not oversights**, and both were taken on measurement:
///
/// * `mehmed_threatens` — задача 24 closed `(D₂)`-0 by confirming the addressing
///   as intentional in game terms. The anomaly is unique (1 event of 46) and its
///   price on the channel the task existed for is **zero**: redirecting the write
///   removes 0 of 102 decisive collapses and adds 25, all on `ottomans`. All four
///   alternatives measured worse. See `docs/investigation_event_target_addressing.md`
///   §5, §8 and ENGINE13_INFRASTRUCTURE_TASKS.md, задача 24 §7.12.
/// * `barbarian_raid` — the mirror case, and dead for a reason that is now known:
///   its gate is `actor:visigoths.military_size > 80` while `visigoths` start at
///   `48.0` (`rome_375.rs:404`), so the gate is identically false and the event
///   has never fired in any measured run (задача 24 §3.3: 0 firings / 30 games).
///   Reviving it would add a mechanic to a calibrated scenario, which is the
///   Задача 18 argument verbatim; it is listed, not fixed.
///
/// The entry is per *rule*, not per event: an event excused for one shape is
/// still checked for the other.
const KNOWN_EVENT_ADDRESSING_EXCEPTIONS: &[(&str, &str, AddressingRule)] = &[
    ("constantinople_1430", "mehmed_threatens", AddressingRule::EffectsReachTarget),
    ("rome_375", "barbarian_raid", AddressingRule::GateStaysOnTarget),
];

/// Does this key address `target`? True for `self.<metric>`, and for an absolute
/// key that happens to name the target. Resolved through the engine's own call,
/// so the answer is the engine's and not a re-reading of the key string.
fn addresses_target(key: &RelativeMetricRef, target: &str) -> bool {
    match key {
        RelativeMetricRef::SelfRelative(_) => true,
        RelativeMetricRef::Absolute(MetricRef::Actor { actor_id, .. }) => actor_id.as_str() == target,
        RelativeMetricRef::Absolute(_) => false,
    }
}

/// Names the actor a key addresses, when it addresses one at all and is not
/// bound to the target.
fn other_actor_named(key: &RelativeMetricRef, target: &str) -> Option<String> {
    match key {
        RelativeMetricRef::Absolute(MetricRef::Actor { actor_id, .. })
            if actor_id.as_str() != target =>
        {
            Some(actor_id.as_str().to_string())
        }
        _ => None,
    }
}

/// The check itself, over a slice of events, so that it can be applied to real
/// content *and* to synthetic cases (see
/// [`event_addressing_check_catches_a_new_violator`]).
fn event_addressing_violations(scenario_id: &str, events: &[RandomEvent]) -> Vec<String> {
    let mut failures = Vec::new();
    for ev in events {
        // Only a named target can be missed: `Any`/`SeaActors`/`All` draw their
        // victim at runtime, and every key of those events is `self.`-relative.
        let EventTarget::Actor(target) = &ev.target else { continue };

        let excused = |rule: AddressingRule| {
            KNOWN_EVENT_ADDRESSING_EXCEPTIONS.contains(&(scenario_id, ev.id.as_str(), rule))
        };

        if !excused(AddressingRule::EffectsReachTarget)
            && !ev.effects.keys().any(|k| addresses_target(k, target))
        {
            let elsewhere: Vec<String> = {
                let mut v: Vec<String> = ev
                    .effects
                    .keys()
                    .filter_map(|k| other_actor_named(k, target))
                    .collect();
                v.sort();
                v.dedup();
                v
            };
            failures.push(format!(
                "{scenario_id}: event '{}' targets '{target}' but no effect addresses it \
                 (effects land on {}) — the config cannot say who this event hurts without \
                 reading all three addressing slots",
                ev.id,
                if elsewhere.is_empty() { "no actor at all".to_string() } else { elsewhere.join(", ") }
            ));
        }

        if !excused(AddressingRule::GateStaysOnTarget) {
            let mut strangers: Vec<String> = ev
                .conditions
                .iter()
                .filter_map(|c| other_actor_named(&c.metric, target))
                .collect();
            strangers.sort();
            strangers.dedup();
            if !strangers.is_empty() {
                failures.push(format!(
                    "{scenario_id}: event '{}' targets '{target}' but its gate reads {} — \
                     the event's frequency is controlled by an actor it does not fire on, \
                     so the gate can be identically true or false forever without showing it",
                    ev.id,
                    strangers.join(", ")
                ));
            }
        }
    }
    failures
}

#[test]
fn event_target_matches_gate_and_effects() {
    let mut failures = Vec::new();
    // The shared pool first: it is the same object for all three scenarios, so it
    // is walked once, under its own label.
    failures.extend(event_addressing_violations("common_events", &engine13::events::common_events()));
    for &id in SCENARIO_IDS {
        let s = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));
        failures.extend(event_addressing_violations(id, &s.random_events));
    }
    assert!(
        failures.is_empty(),
        "Event addressing violation(s) — target, gate and effects disagree about who the \
         event is about:\n{}\n\nIf this is deliberate, add it to \
         KNOWN_EVENT_ADDRESSING_EXCEPTIONS with the measurement that justifies it, the way \
         задача 24 did for `mehmed_threatens`.",
        failures.join("\n")
    );
}

/// The guard on the guard: a violator that is *not* on the allowlist must be
/// caught, and a clean event must not be. Synthetic events, so this keeps working
/// after the two real cases are someday fixed or removed.
#[test]
fn event_addressing_check_catches_a_new_violator() {
    let ev = |id: &str, target: &str, effect: &str, cond: &str| RandomEvent {
        id: id.to_string(),
        probability: 0.1,
        target: EventTarget::Actor(target.to_string()),
        conditions: vec![RelativeCondition {
            metric: RelativeMetricRef::literal(cond),
            operator: ComparisonOperator::Greater,
            value: 1.0,
        }],
        effects: HashMap::from([(RelativeMetricRef::literal(effect), -1.0)]),
        llm_context: String::new(),
        one_time: false,
    };

    // clean: both slots on the target, in either spelling
    assert!(event_addressing_violations(
        "synthetic",
        &[ev("clean_self", "alpha", "self.cohesion", "self.legitimacy")]
    )
    .is_empty());
    assert!(event_addressing_violations(
        "synthetic",
        &[ev("clean_literal", "alpha", "actor:alpha.cohesion", "actor:alpha.legitimacy")]
    )
    .is_empty());

    // class 1: effects miss the target — the `mehmed_threatens` shape
    let effects_miss = event_addressing_violations(
        "synthetic",
        &[ev("new_violator", "alpha", "actor:beta.cohesion", "self.legitimacy")],
    );
    assert_eq!(effects_miss.len(), 1, "an off-target effect must be caught: {effects_miss:?}");
    assert!(effects_miss[0].contains("no effect addresses it"));

    // class 2: the gate reads a stranger — the `barbarian_raid` shape
    let gate_strays = event_addressing_violations(
        "synthetic",
        &[ev("new_violator", "alpha", "self.cohesion", "actor:beta.military_size")],
    );
    assert_eq!(gate_strays.len(), 1, "an off-target gate must be caught: {gate_strays:?}");
    assert!(gate_strays[0].contains("its gate reads"));

    // the allowlist is per rule: the real exception excuses one shape, not both
    assert!(
        !KNOWN_EVENT_ADDRESSING_EXCEPTIONS
            .contains(&("constantinople_1430", "mehmed_threatens", AddressingRule::GateStaysOnTarget)),
        "the effects exception for mehmed_threatens must not excuse its gate too"
    );

    // a `global:`/`family:` gate addresses no actor and must not be flagged —
    // the false positive the addressing walk found and corrected (§3)
    assert!(event_addressing_violations(
        "synthetic",
        &[ev("family_gated", "alpha", "self.legitimacy", "family:wealth")]
    )
    .is_empty());
}
