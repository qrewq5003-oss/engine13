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
/// **Superseded in part by задача 29 (2026-08-24).** The fourth bullet no longer holds:
/// `early_transfer` has been removed from `rome_375` (`early_transfer: None`), so rome's
/// generation count does not depend on `external_pressure` at all any more, and the
/// guarded base is 3 transfers per 200 ticks, not 4. The measurement that killed it: at
/// the ticks where the gate actually decided anything `rome.external_pressure` read
/// exactly `100.0` in 80 runs of 80, i.e. the conjunct could not become false. The other
/// three bullets are untouched and the verdict below stands on them. See
/// `docs/investigation_early_transfer.md`.
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

/// `consequence_context` must not be written as alternatives ("either held or
/// fell"). The prompt that carries it also carries the metrics, the fired
/// milestones and the dead-actor list that answer every such sentence, so an
/// alternative adds no fact and reads as the chronicler not knowing its own
/// world. Measured before the rule: 94 % of rome prompts and 72 % of milan
/// prompts carried them (docs/investigation_consequence_context.md §2).
#[test]
fn consequence_context_states_no_alternatives() {
    for entry in engine13::scenarios::registry::get_registry() {
        let scenario = (entry.loader)();
        let text = scenario.consequence_context.to_lowercase();
        for marker in ["либо", " или "] {
            assert!(
                !text.contains(marker),
                "{}: consequence_context contains an alternative ({marker:?}): {text}",
                entry.id
            );
        }
    }
}

/// Fields authored in `Scenario` / `NarrativeConfig` must have a reader.
///
/// Four authored fields were found dead in a single cycle: the paragraph target and
/// its length hint, the forbidden-claims list, and the player's own actor id. The
/// shape is always the same: an author states an intention in content, no code
/// connects it, and
/// nothing fails. Rust cannot catch it (the fields are `pub` and are constructed), so
/// this test reads the crate's own sources and asks, for every declared field, whether
/// anything outside the declaration and outside the scenario files ever touches it.
///
/// It is a **lexical** check and says so: it looks for `.field` in Rust and TypeScript
/// sources. That is enough for the shape it guards, and the allow-list below carries a
/// written reason for every field that legitimately has no reader — including one that
/// is a known live defect rather than an exemption.
/// See docs/investigation_dead_authored_fields.md.
#[test]
fn authored_scenario_fields_have_readers() {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    // field -> why it may stay unread
    const ALLOWED: &[(&str, &str)] = &[
        ("tempo", "serialized shape only: declared in types/index.ts, never read; pacing is fixed at two ticks per year"),
        ("tick_span", "DEAD, recorded: the engine computes `year = start_year + tick / 2`, so the authored `tick_span: 5` is ignored — docs/investigation_dead_authored_fields.md §3"),
        ("tick_label", "serialized shape only: the UI writes its own half-year label"),
        ("features", "read off `WorldState`, which now carries a copy taken from the scenario at load — docs/investigation_world_features.md"),
    ];

    fn collect(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, exts, out);
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| exts.contains(&x)) {
                out.push(p);
            }
        }
    }

    let decl_path = Path::new("src/core/scenario.rs");
    let decl_src = std::fs::read_to_string(decl_path).expect("scenario.rs");

    let struct_block = |name: &str| -> (String, Vec<String>) {
        let head = format!("pub struct {name} {{");
        let start = decl_src.find(&head).unwrap_or_else(|| panic!("struct {name} not found"));
        let end = decl_src[start..].find("\n}").expect("struct end") + start + 2;
        let block = decl_src[start..end].to_string();
        let fields = block
            .lines()
            .filter_map(|l| l.trim().strip_prefix("pub "))
            .filter_map(|l| l.split(':').next())
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()))
            .map(|s| s.to_string())
            .collect();
        (block, fields)
    };

    let mut sources = Vec::new();
    collect(Path::new("src"), &["rs", "ts", "tsx"], &mut sources);
    collect(Path::new("src-tauri/src"), &["rs"], &mut sources);

    let mut dead: BTreeSet<String> = BTreeSet::new();
    for name in ["Scenario", "NarrativeConfig"] {
        let (block, fields) = struct_block(name);
        for field in fields {
            let needle = format!(".{field}");
            let mut has_reader = false;
            for path in &sources {
                let sp = path.to_string_lossy().replace('\\', "/");
                // scenario files construct the fields; probes are not the product
                if (sp.starts_with("src/scenarios/") && !sp.ends_with("registry.rs"))
                    || sp.starts_with("src/bin/")
                {
                    continue;
                }
                let Ok(mut text) = std::fs::read_to_string(path) else { continue };
                if sp == "src/core/scenario.rs" {
                    text = text.replace(&block, "");
                }
                if text
                    .lines()
                    .filter(|l| !l.trim_start().starts_with("//"))
                    .any(|l| l.contains(&needle))
                {
                    has_reader = true;
                    break;
                }
            }
            if !has_reader && !ALLOWED.iter().any(|(f, _)| *f == field) {
                dead.insert(format!("{name}.{field}"));
            }
        }
    }

    assert!(
        dead.is_empty(),
        "authored fields with no reader outside content: {dead:?}\n\
         Either connect them, delete them, or add them to ALLOWED with a written reason."
    );
}

/// Every property the frontend's `WorldState` type promises must exist on the Rust
/// `WorldState` that the backend actually sends.
///
/// This locks the hole the field guard above admits it cannot see: a field that **is**
/// read, just not off the object that carries it. `Scenario.features` was authored in
/// all three scenarios and read in `App.tsx` as `worldState.features?.…`, while the
/// Rust `WorldState` had no such field — so the expression was always `undefined` and
/// the family panel in rome, the global-metrics panel in constantinople and the action
/// history never rendered. Nothing failed: `tsc` was satisfied by the TypeScript
/// interface, which promised a property the backend never sent.
///
/// Only one direction is checked. A Rust field the frontend does not know about is
/// fine; a TypeScript property with no Rust field behind it is a lie in the type.
/// See docs/investigation_world_features.md.
#[test]
fn frontend_world_state_type_matches_the_rust_struct() {
    use std::collections::BTreeSet;

    fn braced_block<'a>(src: &'a str, header: &str) -> &'a str {
        let start = src.find(header).unwrap_or_else(|| panic!("{header} not found"));
        let open = src[start..].find('{').expect("opening brace") + start + 1;
        let mut depth = 1usize;
        for (i, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return &src[open..open + i];
                    }
                }
                _ => {}
            }
        }
        panic!("unbalanced braces after {header}");
    }

    let ts = std::fs::read_to_string("src/types/index.ts").expect("types/index.ts");
    let rs = std::fs::read_to_string("src/core/world.rs").expect("core/world.rs");

    let ts_props: BTreeSet<String> = braced_block(&ts, "interface WorldState")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//") && !l.starts_with("*") && !l.starts_with("/*"))
        .filter_map(|l| l.split(':').next())
        .map(|n| n.trim_end_matches('?').trim().to_string())
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .collect();

    let rs_fields: BTreeSet<String> = braced_block(&rs, "pub struct WorldState")
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("pub "))
        .filter_map(|l| l.split(':').next())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .collect();

    assert!(!ts_props.is_empty() && !rs_fields.is_empty(), "parsing produced nothing: {ts_props:?} / {rs_fields:?}");

    let promised_but_absent: Vec<&String> = ts_props.difference(&rs_fields).collect();
    assert!(
        promised_but_absent.is_empty(),
        "the frontend's WorldState type promises properties the backend never sends: {promised_but_absent:?}\n\
         Either add them to the Rust struct (and fill them) or drop them from the TypeScript type."
    );
}

/// Bug class: a measuring device that samples world state at the tick boundary is
/// only as honest as what happens between the moment the engine reads a value and
/// the moment the probe reads it.
///
/// This was not hypothetical. `docs/investigation_pressure_military_form.md` §16.4
/// rests on `ottomans.cohesion` never reaching the `< 40` that would spawn `mamluks`;
/// the observed minimum is `47.97`, a margin of `7.97`. Inside `phase_events` —
/// between the engine's own evaluation of that gate and the probe's sample — sits
/// `apply_milestone_effects`, which lowers `ottomans.cohesion` by **10**. The
/// conclusion survives only because that write goes *down*, which makes the probe a
/// lower bound on what the engine saw. A second writer, or the same one with the
/// opposite sign, would quietly invalidate the measurement with no test failing.
///
/// So the invariant is not "phase_events writes little" (a size claim, and the wrong
/// kind) but "the set of things reachable from `phase_events` that change the world
/// is exactly this list" (a membership claim, mechanically checkable). The list is
/// keyed on *changing the world*, not only on writing metrics: the traversal also
/// found two functions that insert actors, which is precisely the mechanism the spawn
/// census measures.
const PHASE_EVENTS_MUTATORS: &[&str] = &[
    ".set_metric(",
    ".add_metric(",
    ".clamp_metric(",
    "actors.insert(",
    "actors.remove(",
    ".metrics.insert(",
];

/// Lexical: index every top-level `fn` in `src`, take the transitive closure of calls
/// starting at `entry`, and return those reachable functions whose body contains one of
/// the world-changing calls. Split out of the test so the companion test can feed it a
/// synthetic file and prove the guard actually fires.
fn world_writers_reachable_from(
    src: &str,
    entry: &str,
) -> std::collections::BTreeMap<String, Vec<&'static str>> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut bodies: BTreeMap<String, String> = BTreeMap::new();
    let mut idx = 0usize;
    while let Some(rel) = src[idx..].find("fn ") {
        let at = idx + rel;
        let line_start = src[..at].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let prefix = &src[line_start..at];
        if !prefix.trim_start().is_empty() && prefix.trim_start() != "pub " {
            idx = at + 3;
            continue;
        }
        let rest = &src[at + 3..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            idx = at + 3;
            continue;
        }
        let Some(open_rel) = src[at..].find('{') else { break };
        let open = at + open_rel;
        let mut depth = 0i32;
        let mut close = open;
        for (i, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        bodies.insert(name, src[open..=close].to_string());
        idx = open + 1;
    }

    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut stack = vec![entry.to_string()];
    while let Some(f) = stack.pop() {
        if !reached.insert(f.clone()) {
            continue;
        }
        let Some(body) = bodies.get(&f) else { continue };
        for name in bodies.keys() {
            if body.contains(&format!("{name}(")) && !reached.contains(name) {
                stack.push(name.clone());
            }
        }
    }

    let mut found: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    for f in &reached {
        let Some(body) = bodies.get(f) else { continue };
        let hits: Vec<&'static str> = PHASE_EVENTS_MUTATORS
            .iter()
            .copied()
            .filter(|m| body.contains(m))
            .collect();
        if !hits.is_empty() {
            found.insert(f.clone(), hits);
        }
    }
    found
}

#[test]
fn phase_events_world_writers_are_the_expected_set() {
    use std::collections::BTreeSet;

    // function -> why it is allowed to change the world from inside `phase_events`
    const EXPECTED: &[(&str, &str)] = &[
        (
            "apply_milestone_effects",
            "the only metric writer: `mehmed_accelerates` lowers ottomans military_quality/treasury/cohesion. \
             Every write here must be a DECREASE for the boundary sample to stay a lower bound — \
             see docs/investigation_pressure_military_form.md §18",
        ),
        (
            "check_milestone_events",
            "inserts spawned actors: this is the mechanism the spawn census measures, not a side effect",
        ),
        (
            "apply_seat_split",
            "inserts the heir seat when a scenario splits — docs/investigation_split_as_shrink.md",
        ),
    ];

    let src = std::fs::read_to_string("src/engine/mod.rs").expect("src/engine/mod.rs");
    let found = world_writers_reachable_from(&src, "phase_events");
    assert!(
        !found.is_empty(),
        "the lexical scan in this guard found nothing at all — it has drifted from the file"
    );

    let expected: BTreeSet<&str> = EXPECTED.iter().map(|(f, _)| *f).collect();
    let actual: BTreeSet<&str> = found.keys().map(|s| s.as_str()).collect();

    let unexpected: Vec<String> = actual
        .difference(&expected)
        .map(|f| format!("  {f} changes the world via {:?} and is not in the expected list", found[*f]))
        .collect();
    let gone: Vec<String> = expected
        .difference(&actual)
        .map(|f| format!("  {f} no longer changes the world — drop it from the list and say so"))
        .collect();

    assert!(
        unexpected.is_empty() && gone.is_empty(),
        "the set of world-changing functions reachable from `phase_events` has changed.\n\
         Every measuring device that samples at the tick boundary depends on this set, and on the \
         SIGN of what it writes: see docs/investigation_pressure_military_form.md §18.\n\
         Update the list here with a written reason, and re-check any conclusion that rests on a \
         boundary sample.\n{}{}",
        unexpected.join("\n"),
        gone.join("\n")
    );
}


/// The other half: prove the guard fires. A synthetic file whose `phase_events` reaches
/// a new writer two calls deep must be reported — otherwise the green test above means
/// only that the scan found nothing.
#[test]
fn phase_events_writer_check_catches_a_new_violator() {
    let synthetic = r#"
fn phase_events(world: &mut W) {
    check_threshold_effects(world);
}

fn check_threshold_effects(world: &mut W) {
    newly_added_helper(world);
}

fn newly_added_helper(world: &mut W) {
    world.actors.get_mut("ottomans").unwrap().add_metric("cohesion", 12.0);
}

fn not_reachable(world: &mut W) {
    world.actors.insert("x".to_string(), y);
}
"#;
    let found = world_writers_reachable_from(synthetic, "phase_events");
    assert!(
        found.contains_key("newly_added_helper"),
        "a writer two calls below phase_events was not reported: {found:?}"
    );
    assert!(
        !found.contains_key("not_reachable"),
        "a writer that phase_events cannot reach must not be reported: {found:?}"
    );
}

/// Strip Rust comments and the trailing `#[cfg(test)]` module, then return every
/// string literal left in the production code.
///
/// One scanner does both jobs because they are the same job: a `//` inside a string
/// is not a comment, and a `"` inside a comment does not open a string. Doing it with
/// two regexes gets this wrong in both directions — the first cross-check written by
/// hand for this guard reported `"rome"` and `"rome_375"` as engine knowledge of
/// content, and both were examples inside doc comments.
fn production_string_literals(src: &str) -> Vec<String> {
    let src = match src.find("#[cfg(test)]") {
        Some(i) => &src[..i],
        None => src,
    };
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            '/' if i + 1 < b.len() && b[i + 1] == '/' => {
                while i < b.len() && b[i] != '\n' {
                    i += 1;
                }
            }
            '/' if i + 1 < b.len() && b[i + 1] == '*' => {
                i += 2;
                let mut depth = 1;
                while i < b.len() && depth > 0 {
                    if b[i] == '/' && i + 1 < b.len() && b[i + 1] == '*' {
                        depth += 1;
                        i += 2;
                    } else if b[i] == '*' && i + 1 < b.len() && b[i + 1] == '/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            // A char literal containing a quote — `'"'` — desynchronises a naive
            // scanner: the quote opens a string and everything up to the next quote
            // becomes invisible. Measured: with `let _q = '"';` placed AFTER the last
            // expected name, a real hard-coded actor id went unreported and the guard
            // stayed green. The two-sided check only saves the case where the blind
            // spot swallows an expected name too.
            //
            // A lifetime (`&'a str`) starts the same way and must NOT be treated as a
            // literal, so the two are told apart by what closes them.
            '\'' => {
                if i + 1 < b.len() && b[i + 1] == '\\' {
                    i += 2;
                    while i < b.len() && b[i] != '\'' {
                        i += 1;
                    }
                    i += 1;
                } else if i + 2 < b.len() && b[i + 2] == '\'' {
                    i += 3;
                } else {
                    i += 1; // a lifetime
                }
            }
            // Raw strings: `r"..."`, `r#"..."#`, `r##"..."##`, and the byte-string forms
            // `br"..."` / `br#"..."#`. The closing quote needs the same number of
            // hashes, so an inner `"` does not end them.
            //
            // The `b` prefix has to be allowed explicitly: without it the token boundary
            // check sees `b` as an identifier character, rejects the raw string, and the
            // first inner `"` closes a plain string — blinding the scanner from there on.
            // Measured: `br#"raw byte with a " quote"#` swallowed the next three literals.
            'r' if i + 1 < b.len()
                && (b[i + 1] == '"' || b[i + 1] == '#')
                && {
                    let prev_is_b = i > 0 && b[i - 1] == 'b';
                    let boundary = if prev_is_b { i.checked_sub(2) } else { i.checked_sub(1) };
                    match boundary {
                        None => true,
                        Some(k) => !(b[k].is_alphanumeric() || b[k] == '_'),
                    }
                } =>
            {
                let mut j = i + 1;
                let mut hashes = 0usize;
                while j < b.len() && b[j] == '#' {
                    hashes += 1;
                    j += 1;
                }
                if j < b.len() && b[j] == '"' {
                    j += 1;
                    let start = j;
                    let closing: String =
                        std::iter::once('"').chain(std::iter::repeat_n('#', hashes)).collect();
                    let rest: String = b[j..].iter().collect();
                    match rest.find(&closing) {
                        Some(off) => {
                            let end = start + rest[..off].chars().count();
                            out.push(b[start..end].iter().collect());
                            i = end + closing.chars().count();
                        }
                        None => i = b.len(),
                    }
                } else {
                    i += 1;
                }
            }
            '"' => {
                i += 1;
                let mut lit = String::new();
                while i < b.len() && b[i] != '"' {
                    if b[i] == '\\' {
                        i += 1;
                        if i < b.len() {
                            lit.push(b[i]);
                            i += 1;
                        }
                        continue;
                    }
                    lit.push(b[i]);
                    i += 1;
                }
                i += 1;
                out.push(lit);
            }
            _ => i += 1,
        }
    }
    out
}

/// Bug class: the engine deciding membership by **enumerating authored names** instead
/// of asking for a property.
///
/// Found in the wild: `EventTarget::SeaActors` resolves "is this actor maritime" as
/// `tags.contains("maritime") || tags.contains("trade_empire")`. Rome's `saxons` carry
/// `seafaring` — a fully authored tag with its own `metrics_modifier` and `spreads_via`
/// — and the two vocabularies do not intersect, so the `piracy` event targets an empty
/// set in rome and cannot fire at all. No test failed; the content simply never reached
/// a player. See docs/investigation_dead_authored_content.md §7.
///
/// The class is small and closed, which is exactly why it is worth pinning: a fifth
/// name added tomorrow silently repeats the same failure.
#[test]
fn engine_knows_authored_content_only_by_these_names() {
    use std::collections::BTreeSet;

    // literal -> why the engine is allowed to know this authored name
    const EXPECTED: &[(&str, &str)] = &[
        (
            "maritime",
            "EventTarget::SeaActors, mod.rs — KNOWN DEFECT: enumerates names instead of asking \
             the tag for a property, which is why rome's `seafaring` actor is invisible to it",
        ),
        (
            "trade_empire",
            "the second half of the same SeaActors predicate, same defect",
        ),
        (
            "mehmed_accelerates",
            "apply_milestone_effects, mod.rs — one scenario's milestone hard-coded in the engine",
        ),
        (
            "ottomans",
            "the actor that same milestone writes to",
        ),
    ];

    // Authored vocabulary: ids of tags, actors, milestones, events.
    let mut authored: BTreeSet<String> = BTreeSet::new();
    fn walk(dir: &std::path::Path, out: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            for line in text.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("id = \"") {
                    if let Some(name) = rest.split('"').next() {
                        out.insert(name.to_string());
                    }
                } else if let Some(rest) = t.strip_prefix("id: \"") {
                    if let Some(name) = rest.split('"').next() {
                        out.insert(name.to_string());
                    }
                }
            }
        }
    }
    walk(std::path::Path::new("src/scenarios"), &mut authored);
    walk(std::path::Path::new("src/events"), &mut authored);
    assert!(
        authored.len() > 50,
        "authored vocabulary came out at {} names — the scan has drifted",
        authored.len()
    );

    // The engine's own metric vocabulary is not authored content: every scenario uses
    // the same words, and the engine is entitled to know them.
    let metrics: BTreeSet<&str> = [
        "population",
        "military_size",
        "military_quality",
        "economic_output",
        "cohesion",
        "legitimacy",
        "external_pressure",
        "treasury",
        "expansion_count",
        "federation_progress",
    ]
    .into_iter()
    .collect();

    let mut found: BTreeSet<String> = BTreeSet::new();
    for dir in ["src/engine", "src/core"] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            panic!("{dir} not readable")
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).expect("engine source");
            for lit in production_string_literals(&src) {
                if authored.contains(&lit) && !metrics.contains(lit.as_str()) {
                    found.insert(lit);
                }
            }
        }
    }

    let expected: BTreeSet<&str> = EXPECTED.iter().map(|(n, _)| *n).collect();
    let actual: BTreeSet<&str> = found.iter().map(|s| s.as_str()).collect();
    let extra: Vec<&&str> = actual.difference(&expected).collect();
    let gone: Vec<&&str> = expected.difference(&actual).collect();

    assert!(
        extra.is_empty() && gone.is_empty(),
        "the set of authored names hard-coded in the engine has changed.\n\
         Deciding membership by enumerating names is how `piracy` came to target an empty \
         set in rome (docs/investigation_dead_authored_content.md §7). Prefer asking the \
         content for a property.\n\
         newly hard-coded: {extra:?}\n\
         no longer present (drop from the list and say so): {gone:?}"
    );
}

/// The other half: the scanner must see code and must NOT see comments — the exact
/// trap that produced two false positives when this cross-check was first written by
/// hand.
#[test]
fn literal_scan_ignores_comments_and_test_modules() {
    let synthetic = r#"
/// A doc comment mentioning "rome_375" as an example.
// A line comment mentioning "ottomans".
/* A block comment mentioning "maritime". */
fn real_code() {
    let a = "seafaring";
    let url = "https://example.invalid//not-a-comment";
}
#[cfg(test)]
mod tests {
    const HIDDEN: &str = "trade_empire";
}
"#;
    let lits = production_string_literals(synthetic);
    assert!(lits.contains(&"seafaring".to_string()), "real literal missed: {lits:?}");
    assert!(
        lits.contains(&"https://example.invalid//not-a-comment".to_string()),
        "a `//` inside a string must not start a comment: {lits:?}"
    );
    for hidden in ["rome_375", "ottomans", "maritime", "trade_empire"] {
        assert!(
            !lits.contains(&hidden.to_string()),
            "{hidden} came from a comment or a test module: {lits:?}"
        );
    }
}

/// The dangerous direction: not "the scanner sees too much" but "the scanner goes
/// blind from line N onward and reports nothing after it".
///
/// `assert!(!found.is_empty())` in the guard above catches a scanner that died
/// completely; it cannot catch one that died halfway. This test puts each
/// resynchronisation hazard in the MIDDLE and an expected name AFTER it, so a scanner
/// that loses its place fails here instead of silently passing the real guard.
///
/// Measured before the fix: `let _q = '"';` placed after the last expected name made a
/// genuine hard-coded actor id invisible and left the guard green.
#[test]
fn literal_scan_resynchronises_after_char_literals_and_raw_strings() {
    let synthetic = r##"
fn hazards() {
    let quote_char = '"';
    let after_char_literal = "name_after_char";
    let escaped = ''';
    let after_escaped = "name_after_escape";
    let backslash = '\';
    let after_backslash = "name_after_backslash";
    let raw = r#"a raw string with a " quote and a // slash"#;
    let after_raw = "name_after_raw";
    let byte_string = b"a byte string";
    let after_byte_string = "name_after_byte_string";
    let raw_byte = br#"a raw byte string with a " quote"#;
    let after_raw_byte = "name_after_raw_byte";
    let byte_char = b'"';
    let after_byte_char = "name_after_byte_char";
    let lifetime: &'static str = "name_after_lifetime";
    let nested = /* outer /* inner "hidden_in_nested" */ still comment */ "name_after_nested";
}
"##;
    let lits = production_string_literals(synthetic);
    for expected in [
        "name_after_char",
        "name_after_escape",
        "name_after_backslash",
        "name_after_raw",
        "name_after_byte_string",
        "name_after_raw_byte",
        "name_after_byte_char",
        "name_after_lifetime",
        "name_after_nested",
    ] {
        assert!(
            lits.contains(&expected.to_string()),
            "scanner lost its place before `{expected}` — it goes blind from there on, \
             and the main guard would stay green while real hard-coded names slip past: {lits:?}"
        );
    }
    assert!(
        !lits.contains(&"hidden_in_nested".to_string()),
        "a name inside a nested block comment must not be reported: {lits:?}"
    );
}

/// Returns the inheritance coefficients that exceed `1.0`, with their metric keys.
///
/// Split out so the companion test can feed it a synthetic map: the real scenarios
/// currently pass, and a guard that has never been seen to fail is not a guard.
fn inheritance_coefficients_over_one(
    coefficients: &std::collections::HashMap<String, f64>,
) -> Vec<String> {
    let mut over: Vec<String> = coefficients
        .iter()
        .filter(|(_, c)| **c > 1.0)
        .map(|(k, c)| format!("{k} = {c}"))
        .collect();
    over.sort();
    over
}

/// Bug class: an invariant that holds because of the *values* in content, while the
/// *code* that would enforce it does not exist.
///
/// Family metrics are clamped to `0..100` on the canonical write path
/// (`MetricRef::add`, `metric_ref.rs`). They are **not** clamped on the second write
/// path — generation inheritance multiplies every family metric by its coefficient and
/// inserts the product directly (`engine/mod.rs`, `check_generation_transfer`). The
/// ceiling therefore holds only while every coefficient is `<= 1.0`; rome's are
/// `0.85, 1.0, 1.0, 0.8` and the engine's default for an unlisted metric is `0.7`.
///
/// This is load-bearing, not cosmetic. `docs/investigation_silent_authored_content.md`
/// §7 concludes that `recruit_soldiers` (`family_wealth > 100`) and `senator_bribe`
/// (`> 200`) are dead **structurally** — gated above a ceiling no state can reach. A
/// coefficient of `1.2` written tomorrow lifts family metrics past `100` on a path with
/// no clamp, and that conclusion silently becomes false with no test failing.
#[test]
fn inheritance_coefficients_never_exceed_one() {
    let mut failures = Vec::new();
    for &id in SCENARIO_IDS {
        let scenario = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));
        let Some(gen) = &scenario.generation_mechanics else {
            continue;
        };
        let over = inheritance_coefficients_over_one(&gen.inheritance_coefficients);
        if !over.is_empty() {
            failures.push(format!("{id}: {}", over.join(", ")));
        }
    }
    assert!(
        failures.is_empty(),
        "an inheritance coefficient above 1.0 lifts a family metric on the ONE write path \
         that does not clamp (`check_generation_transfer` inserts `value * coefficient` \
         directly). The `0..100` ceiling is what makes `recruit_soldiers` and \
         `senator_bribe` structurally dead — see \
         docs/investigation_silent_authored_content.md §7. If a coefficient above 1.0 is \
         intended, that conclusion has to be re-measured first.\n{}",
        failures.join("\n")
    );
}

/// The other half: the guard must fire. Fed a map with a coefficient above one.
#[test]
fn inheritance_coefficient_check_catches_a_new_violator() {
    use std::collections::HashMap;
    let mut coefficients: HashMap<String, f64> = HashMap::new();
    coefficients.insert("family_influence".to_string(), 0.85);
    coefficients.insert("family_wealth".to_string(), 1.0);
    assert!(
        inheritance_coefficients_over_one(&coefficients).is_empty(),
        "coefficients at or below 1.0 must pass"
    );
    coefficients.insert("family_connections".to_string(), 1.2);
    let over = inheritance_coefficients_over_one(&coefficients);
    assert_eq!(
        over,
        vec!["family_connections = 1.2".to_string()],
        "a coefficient above 1.0 must be reported, and named"
    );
}

/// Returns the authored strings a specification quotes that no longer exist in the
/// scenario's content. Split out so the companion test can feed it synthetic input.
///
/// "Authored string" is narrow on purpose: a quoted run of at least 16 characters
/// containing Cyrillic. Everything else a specification quotes — `region_rank: "S"`,
/// `id: "rome"`, prose in its own pseudo-notation — is **not** a verbatim quote of code
/// and never was. Measured before the guard was written: over the whole file, 92 of 158
/// quoted lines differ by notation alone, while of the 16 authored strings exactly 3
/// had drifted, and all 3 were real.
fn spec_strings_missing_from_content(spec: &str, content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes: Vec<char> = spec.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == '"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != '"' && bytes[j] != '\n' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == '"' {
                let lit: String = bytes[start..j].iter().collect();
                let cyrillic = lit.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c));
                if lit.chars().count() >= 16 && cyrillic && !content.contains(&lit) {
                    out.push(lit);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out.sort();
    out.dedup();
    out
}

/// Bug class: a **specification** drifting from the code it specifies.
///
/// A record ("here is what was generated") stays true untouched; a specification ("here
/// is what the scenario is") becomes false the moment the code changes under it, and the
/// next reader takes the stale value from the normative source rather than from an
/// archive. `ROME_375_SCENARIO.md` had drifted in three authored strings across three
/// separate merged tasks — the family name, the `rome_splits` narrative text, and (found
/// alongside, not covered by this guard) its `duration`.
///
/// Only one root document is checked, and that is deliberate: the other two are prose
/// design discussions whose quotation marks carry emphasis, not content. Running this
/// predicate over `ENGINE13_SCENARIO3_DESIGN.md` reports 16 of 18 "missing" strings, all
/// false. A guard is worth having only where it is precise.
#[test]
fn scenario_specifications_quote_content_that_still_exists() {
    // spec file -> (scenario id, why this file is a specification and not a record)
    const SPECS: &[(&str, &str, &str)] = &[(
        "ROME_375_SCENARIO.md",
        "rome_375",
        "quotes authored content verbatim and is used as the normative description of the scenario",
    )];

    let mut failures = Vec::new();
    for (spec_path, scenario_id, _why) in SPECS {
        let Ok(spec) = std::fs::read_to_string(spec_path) else {
            failures.push(format!("{spec_path}: not readable"));
            continue;
        };
        let mut content =
            std::fs::read_to_string(format!("src/scenarios/{scenario_id}.rs")).unwrap_or_default();
        if let Ok(dir) = std::fs::read_dir(format!("src/scenarios/{scenario_id}")) {
            for e in dir.flatten() {
                if e.path().extension().and_then(|x| x.to_str()) == Some("toml") {
                    content.push_str(&std::fs::read_to_string(e.path()).unwrap_or_default());
                }
            }
        }
        assert!(
            content.len() > 1000,
            "{spec_path}: scenario content for {scenario_id} came out empty — the guard has drifted"
        );
        for missing in spec_strings_missing_from_content(&spec, &content) {
            failures.push(format!("  {spec_path}: {missing}"));
        }
    }
    assert!(
        failures.is_empty(),
        "a specification quotes authored text the scenario no longer contains. Either the \
         code changed and the specification was not updated, or the quotation was never \
         accurate. A stale specification is worse than a stale record: the next reader \
         takes the old value from the normative source.\n{}",
        failures.join("\n")
    );
}

/// The other half: the guard must fire, and must stay silent on notation.
#[test]
fn spec_drift_check_catches_a_changed_string_and_ignores_notation() {
    let spec = r#"
    region_rank: "S"
    id: "rome"
    llm_context_shift: "Семья Анициев стала одной из значимых сил города."
    llm_context_shift: "Строка, которой в контенте нет совсем."
    "#;
    let content = r#"
        region_rank: RegionRank::S,
        id: "rome".to_string(),
        llm_context_shift: "Семья Анициев стала одной из значимых сил города.".to_string(),
    "#;
    let missing = spec_strings_missing_from_content(spec, content);
    assert_eq!(
        missing,
        vec!["Строка, которой в контенте нет совсем.".to_string()],
        "the guard must report the drifted authored string and nothing else: {missing:?}"
    );
}

/// Parse the numeric blocks of a scenario specification: per actor id, the `metrics:`
/// section, plus the single `scenario_metrics:` block if present.
///
/// Returns `(actor metrics, family metrics)`. Notation-tolerant by construction: it
/// reads only `key: number` lines inside the named sections, so the pseudo-notation the
/// specification uses elsewhere (`region_rank: "S"`, tag modifier tables) cannot leak in.
/// A cruder predicate that scanned every `key: number` in the file reported 39
/// discrepancies, all false — it was picking up tag modifiers as actor metrics.
#[allow(clippy::type_complexity)]
fn spec_numbers(
    spec: &str,
) -> (
    std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>,
    std::collections::BTreeMap<String, f64>,
) {
    use std::collections::BTreeMap;
    let mut actors: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let mut family: BTreeMap<String, f64> = BTreeMap::new();

    let mut current_id: Option<String> = None;
    let mut section: Option<&str> = None;
    for line in spec.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("### ") {
            current_id = None;
            section = None;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("id: \"") {
            if let Some(id) = rest.split('"').next() {
                current_id = Some(id.to_string());
            }
            section = None;
            continue;
        }
        if trimmed == "metrics:" {
            section = Some("metrics");
            continue;
        }
        if trimmed == "scenario_metrics:" {
            section = Some("family");
            continue;
        }
        // A line that is not indented under a section ends it.
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if !indented {
            section = None;
            continue;
        }
        let Some(sec) = section else { continue };
        let Some((key, rest)) = trimmed.split_once(':') else {
            section = None;
            continue;
        };
        let value_text: String = rest
            .trim()
            .split("//")
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let Ok(value) = value_text.parse::<f64>() else {
            continue;
        };
        match sec {
            "metrics" => {
                if let Some(id) = &current_id {
                    actors.entry(id.clone()).or_default().insert(key.trim().to_string(), value);
                }
            }
            _ => {
                family.insert(key.trim().to_string(), value);
            }
        }
    }
    (actors, family)
}

/// Bug class, numeric half: a specification whose **numbers** have drifted from the code.
///
/// The string half is `scenario_specifications_quote_content_that_still_exists`. Numbers
/// are the more expensive half and were left unguarded at first: a drifted name a reader
/// notices, a drifted `duration` they do not — and a stale number is exactly what someone
/// will compute from. Kept separate because the two predicates fail for different reasons
/// and should say so separately.
///
/// Authoritative by construction: the code side comes from `registry::load_by_id`, not
/// from parsing the source.
#[test]
fn scenario_specifications_quote_numbers_that_still_match() {
    let spec = std::fs::read_to_string("ROME_375_SCENARIO.md").expect("ROME_375_SCENARIO.md");
    let scenario = registry::load_by_id("rome_375").expect("rome_375");
    let (spec_actors, spec_family) = spec_numbers(&spec);

    assert!(
        spec_actors.len() >= 10,
        "parsed only {} actor blocks out of the specification — the parser has drifted",
        spec_actors.len()
    );

    let mut failures = Vec::new();
    for (id, metrics) in &spec_actors {
        let Some(actor) = scenario.actors.iter().find(|a| a.id == *id) else {
            failures.push(format!("  actor `{id}` is specified but absent from the scenario"));
            continue;
        };
        for (key, spec_value) in metrics {
            let code_value = actor.metrics.get(key.as_str()).copied();
            match code_value {
                Some(v) if (v - spec_value).abs() < 1e-9 => {}
                Some(v) => failures.push(format!(
                    "  {id}.{key}: specification says {spec_value}, code has {v}"
                )),
                None => failures.push(format!(
                    "  {id}.{key}: specified as {spec_value}, absent from the actor"
                )),
            }
        }
    }
    // The family block is a KNOWN, unresolved disagreement, and it is pinned rather than
    // hidden. The specification says `8 / 12 / 22 / 15`; the code says `0 / 0 / 0 / 0`;
    // and the first version of the code said `60 / 40 / 50 / 45` before a commit about
    // tag spreading zeroed it (`e235fb8`, unrelated to families). Three different sets:
    // the two were never in agreement, so this is not drift from a merged decision.
    //
    // It is not silently reconciled here because which side is right is a content
    // question with measured consequences: starting at zero is consistent with
    // `family_rises` (`influence >= 60`) never firing without a player, with
    // `senator_bribe` (`wealth > 200`) never firing, with `recruit_soldiers`
    // (`wealth > 100`) never being available, and with the four family-conditioned
    // auto-delta modifiers that never apply — see
    // docs/investigation_silent_authored_content.md §12.
    //
    // Pinning both sides means the test fires the moment either changes, which forces the
    // decision to be made rather than absorbed.
    {
        const SPEC_SIDE: [(&str, f64); 4] = [
            ("family_influence", 8.0),
            ("family_knowledge", 12.0),
            ("family_wealth", 22.0),
            ("family_connections", 15.0),
        ];
        let authored = scenario.initial_family_metrics.clone().unwrap_or_default();
        for (key, expected_spec) in SPEC_SIDE {
            let in_spec = spec_family.get(key).copied();
            let in_code = authored
                .iter()
                .find(|(k, _)| k.ends_with(key))
                .map(|(_, v)| *v);
            if in_spec != Some(expected_spec) {
                failures.push(format!(
                    "  family {key}: the specification changed ({in_spec:?} instead of \
                     {expected_spec}) — resolve the disagreement recorded in \
                     docs/investigation_silent_authored_content.md §12 instead of editing one side"
                ));
            }
            if in_code != Some(0.0) {
                failures.push(format!(
                    "  family {key}: the code changed ({in_code:?} instead of 0.0) — if the \
                     starting values are being restored, the specification and the measured \
                     consequences in §12 both need updating"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "the specification's numbers no longer match the scenario. A stale number in a \
         normative document is worse than a stale name: nobody notices it, and it is what \
         the next reader will compute from.\n{}",
        failures.join("\n")
    );
}

/// Bug class: a renderer that assumes the sign of a number the content is free to make
/// negative.
///
/// `ControlPanel.tsx` shows a player the cost and the effects of an action before they
/// choose it. The cost block computed the sign; the effects block hard-coded a plus.
/// Fifteen actions across the three scenarios carry negative values in `effects`, so the
/// card rendered `+-50`, `+-80`, `+-15`. The cheapest example is `raise_taxes`, the one
/// unconditional source of family wealth: it showed `Cohesion: +-3`.
///
/// This is the first member of the "works and lies" class that a guard can catch at all
/// — unlike a family named after the wrong city, it needs no knowledge of the world
/// outside the repository. The predicate is: either no authored effect is negative, or
/// the renderer computes the sign. The first disjunct is content's business and changes
/// freely; the second is checkable here.
#[test]
fn action_effects_are_rendered_with_a_computed_sign() {
    let panel = std::fs::read_to_string("src/components/ControlPanel.tsx")
        .expect("src/components/ControlPanel.tsx");

    // How many authored effects are negative — the reason the guard exists.
    let mut negative = 0usize;
    for &id in SCENARIO_IDS {
        let scenario = registry::load_by_id(id).unwrap_or_else(|| panic!("{id}: failed to load"));
        for action in scenario.universal_actions.iter().chain(scenario.patron_actions.iter()) {
            if action.effects.values().any(|v| *v < 0.0) {
                negative += 1;
            }
        }
    }

    let hard_coded_plus = panel.contains(": +{value.toFixed(0)}");
    assert!(
        !hard_coded_plus,
        "the action card hard-codes a leading `+` for effects while {negative} authored \
         actions carry negative values there — the player is shown `+-50`. Compute the \
         sign, as the cost block does."
    );

    let computed = panel.matches("value > 0 ? '+' : ''").count();
    assert!(
        computed >= 2,
        "expected the sign to be computed in both the cost and the effects block, found \
         {computed} site(s) — the guard has drifted from the component"
    );
}

/// The other half: the predicate must reject the shape that shipped.
#[test]
fn effect_sign_check_rejects_a_hard_coded_plus() {
    let broken = "{formatMetricName(metric)}: +{value.toFixed(0)}";
    assert!(
        broken.contains(": +{value.toFixed(0)}"),
        "the pattern the guard looks for must match the shape that actually shipped"
    );
    let fixed = "{formatMetricName(metric)}: {value > 0 ? '+' : ''}{value.toFixed(0)}";
    assert!(
        !fixed.contains(": +{value.toFixed(0)}"),
        "the corrected shape must not match"
    );
}

/// Count `match` expressions that dispatch on `DependencyMode`.
///
/// The first version of this predicate counted the literal `DependencyMode::Deficit =>`
/// and therefore counted **three** of the five sites in `budget_probe`: it missed a tuple
/// match (`match (&r.mode, r.threshold)`, whose arms are written
/// `(&DependencyMode::Deficit, Some(t))`) and a match with no `Deficit` arm at all. A
/// fourth copy that simply never mentioned `Deficit` in that spelling appeared with the
/// guard staying green — the claim "a fourth one cannot appear unnoticed" was false.
///
/// Counting the *dispatch* instead of one variant name closes both: any `match` whose
/// body mentions the enum is a place that has to be kept in step with the engine,
/// whatever shape its arms take. A `match` on something else entirely (`match mode` over
/// a CLI string) mentions nothing and is not counted.
fn dependency_mode_dispatch_sites(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0usize;
    let mut i = 0usize;
    while let Some(rel) = text[i..].find("match ") {
        let at = i + rel;
        // byte index -> char index is avoided by scanning on the char vector from a
        // recomputed position; `find` gives bytes, so re-derive the char offset.
        let char_at = text[..at].chars().count();
        let Some(open_rel) = chars[char_at..].iter().position(|c| *c == '{') else {
            break;
        };
        let open = char_at + open_rel;
        let mut depth = 0i32;
        let mut close = open;
        for (k, c) in chars[open..].iter().enumerate() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + k;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body: String = chars[open..=close].iter().collect();
        if body.contains("DependencyMode::") {
            count += 1;
        }
        i = at + "match ".len();
    }
    count
}

/// Bug class: the engine's arithmetic re-implemented outside the engine.
///
/// `budget_probe` prices dependency rules offline, and to do it, it keeps its own
/// `match` over `DependencyMode` — three of them. That is not a style complaint: when the
/// engine grew `ExcessProportional`, none of those copies knew, and the only thing that
/// noticed was the compiler refusing a non-exhaustive match. A copy that had used a
/// catch-all arm would have kept running and quietly priced the new mode as zero.
///
/// The measuring devices of this project decide what gets merged, so a probe that
/// computes something *close to* what the engine computes is worse than no probe. The
/// rule the project already follows is "call the engine, do not re-derive it"
/// (`dependency_probe` reads the engine's own trace; `spec` guards load through
/// `registry`). Where a copy is unavoidable, it is listed here with a reason, and a
/// fourth one cannot appear unnoticed.
#[test]
fn engine_arithmetic_is_re_implemented_only_where_listed() {
    use std::collections::BTreeMap;

    // file -> (match sites, why a copy is tolerated here)
    const EXPECTED: &[(&str, usize, &str)] = &[
        (
            "src/engine/mod.rs",
            2,
            "the original arithmetic (`apply_dependency_rule`) plus the load-time validator \
             (`validate_dependency_thresholds`), which dispatches on the mode to decide whether a \
             threshold is required. Both are in the engine and are the thing everything else must \
             be kept in step with; the widened predicate counts the validator too, and that is \
             correct — a new mode has to be considered there as well",
        ),
        (
            "src/bin/budget_probe.rs",
            5,
            "offline pricing of dependency rules for the treasury/population investigations. \
             FIVE sites, not the three the first version of this guard could see: three flat \
             `match rule.mode`, one tuple `match (&r.mode, r.threshold)`, and one that arms only \
             `Excess` and `Bonus` behind `_ => 0.0`. That last one prices every other mode — \
             `Deficit`, `Linear`, both proportional forms — as ZERO, silently, and it is the very \
             shape this guard's own text warned about. It is filtered to `external_pressure` \
             rules, all of which are `excess`/`bonus` today, so nothing merged is contaminated; \
             a proportional `ep` rule would have been priced as zero. \
             See docs/investigation_silent_authored_content.md §16",
        ),
    ];

    fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    collect(std::path::Path::new("src"), &mut files);
    files.sort();

    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        let sites = dependency_mode_dispatch_sites(&text);
        if sites > 0 {
            found.insert(f.to_string_lossy().replace('\\', "/"), sites);
        }
    }

    let mut failures = Vec::new();
    for (path, expected_sites, _why) in EXPECTED {
        match found.get(*path) {
            Some(n) if n == expected_sites => {}
            Some(n) => failures.push(format!(
                "  {path}: {n} match site(s) over DependencyMode, expected {expected_sites}"
            )),
            None => failures.push(format!(
                "  {path}: no longer matches over DependencyMode — drop it from the list and say so"
            )),
        }
    }
    for (path, n) in &found {
        if !EXPECTED.iter().any(|(p, _, _)| p == path) {
            failures.push(format!(
                "  {path}: {n} new match site(s) over DependencyMode — call the engine instead of \
                 mirroring it, or add an entry here with a written reason"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "the engine's dependency arithmetic is mirrored somewhere new, or an existing mirror \
         changed shape. A mirror that drifts prices the world differently from the engine, and \
         the measurements built on it decide what gets merged.\n{}",
        failures.join("\n")
    );
}
