use crate::core::{MetricRef, Scenario};
use std::collections::HashSet;

/// Scenario registry entry
pub struct ScenarioEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub year: i32,
    pub description: &'static str,
    pub loader: fn() -> Scenario,
}

/// Get the scenario registry
pub fn get_registry() -> Vec<ScenarioEntry> {
    vec![
        ScenarioEntry {
            id: "rome_375",
            name: "Rome 375 — Семья Анициев",
            year: 375,
            description: "375 год. Медиолан — фактическая столица Западной Империи.",
            loader: crate::scenarios::rome_375::load_rome_375,
        },
        ScenarioEntry {
            id: "constantinople_1430",
            name: "Constantinople 1430 — Федерация",
            year: 1430,
            description: "1430 год. Фессалоники пали. Константинополь стоит — но ненадолго.",
            loader: crate::scenarios::constantinople_1430::load_constantinople_1430,
        },
        ScenarioEntry {
            id: "milan_1477",
            name: "Milan 1477 — Регентство",
            year: 1477,
            description: "1477 год. Галеаццо Мария Сфорца убит. Милан правит малолетний герцог — и все это знают.",
            loader: crate::scenarios::milan_1477::load_milan_1477,
        },
    ]
}

/// Validate scenario for consistency
pub fn validate_scenario(scenario: &Scenario) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let actor_ids: HashSet<&str> = scenario.actors.iter().map(|a| a.id.as_str()).collect();

    // Check auto_deltas
    for delta in &scenario.auto_deltas {
        check_actor_exists(&delta.metric, &actor_ids, "auto_delta", &mut errors);
        for cond in &delta.conditions {
            check_actor_exists(&cond.metric, &actor_ids, "auto_delta.condition", &mut errors);
        }
        for ratio in &delta.ratio_conditions {
            check_actor_exists(&ratio.metric_a, &actor_ids, "ratio_condition.metric_a", &mut errors);
            check_actor_exists(&ratio.metric_b, &actor_ids, "ratio_condition.metric_b", &mut errors);
        }
    }

    // Check milestone effects
    for milestone in &scenario.milestone_events {
        if let Some(metric) = milestone.condition.metric_ref() {
            check_actor_exists(metric, &actor_ids, &format!("milestone '{}'", milestone.id), &mut errors);
        }
        // An `actor_state` condition names an actor, not a metric. Its actor id had
        // never been validated at all: it used to be routed through the metric check,
        // which ignored anything that wasn't already an `actor:` ref.
        if let Some(actor_id) = milestone.condition.actor_state_actor_id() {
            if !actor_ids.contains(actor_id) {
                errors.push(format!(
                    "milestone '{}': actor_state condition names unknown actor_id '{}'",
                    milestone.id, actor_id
                ));
            }
        }
    }

    // Check patron_actions
    for action in &scenario.patron_actions {
        if let Some(ref source) = action.source_actor_id {
            if !actor_ids.contains(source.as_str()) {
                errors.push(format!("patron_action '{}': unknown source_actor_id '{}'", action.id, source));
            }
        }
        for metric in action.effects.keys().chain(action.cost.keys()) {
            check_actor_exists(metric, &actor_ids, &format!("action '{}'", action.id), &mut errors);
        }
    }

    // Check status_indicators
    for indicator in &scenario.status_indicators {
        check_actor_exists(&indicator.metric, &actor_ids, &format!("status_indicator '{}'", indicator.label), &mut errors);
    }

    // Check narrative key_metrics. These feed the chronicler's prompt and were never
    // validated, which is why 13 of the 16 keys across the three scenarios had been
    // resolving to 0.0 unnoticed.
    for key_metric in &scenario.narrative_config.key_metrics {
        check_actor_exists(
            &key_metric.metric,
            &actor_ids,
            &format!("narrative_config.key_metrics '{}'", key_metric.label),
            &mut errors,
        );
        // Полосы: словарь обязан быть непустым и идти снизу вверх, иначе
        // `KeyMetric::band_for` вернёт слово не той полосы — молча, как всё в этом
        // блоке до задачи B19.
        if key_metric.bands.is_empty() {
            errors.push(format!(
                "narrative_config.key_metrics '{}': пустой словарь полос — \
                 летописцу нечего сказать об этой метрике",
                key_metric.label
            ));
        }
        if key_metric.bands.windows(2).any(|w| w[1].0 <= w[0].0) {
            errors.push(format!(
                "narrative_config.key_metrics '{}': границы полос не возрастают строго: {:?}",
                key_metric.label,
                key_metric.bands.iter().map(|(v, _)| *v).collect::<Vec<_>>()
            ));
        }
    }

    // Check on_collapse heirs. Every declared heir must be an actor of THIS
    // scenario — a template or a living power. The engine creates a successor only
    // when it finds one in `scenario.actors` and used to skip silently otherwise:
    // constantinople_1430 declared five `ottoman_*` heirs with a template for none,
    // so byzantium fell in 30 of 30 no-player runs and its heir never existed
    // (docs/investigation_successor_entry.md §2). A self-heir is rejected too.
    for actor in &scenario.actors {
        for heir in &actor.on_collapse {
            if heir.id == actor.id {
                errors.push(format!("actor '{}': on_collapse names itself as heir", actor.id));
            } else if !actor_ids.contains(heir.id.as_str()) {
                errors.push(format!(
                    "actor '{}': on_collapse names unknown heir '{}' — no template and no actor \
                     with that id in the scenario",
                    actor.id, heir.id
                ));
            }
        }
    }

    // Check the seat marker. At most one heir per actor may keep the seat, and the
    // marker only means anything when there is something to split off — a lone heir
    // that keeps the seat would shrink its parent and bear nobody.
    for actor in &scenario.actors {
        let seats = actor.on_collapse.iter().filter(|h| h.keeps_seat).count();
        if seats > 1 {
            errors.push(format!("actor '{}': {} heirs claim keeps_seat, at most one may", actor.id, seats));
        }
        if seats == 1 && actor.on_collapse.len() < 2 {
            errors.push(format!(
                "actor '{}': keeps_seat on a lone heir — nothing would separate",
                actor.id
            ));
        }
    }

    // Check `after` (A2): an existing milestone of this scenario, not the milestone itself.
    for m in &scenario.milestone_events {
        let Some(prev) = &m.after else { continue };
        if prev == &m.id {
            errors.push(format!("milestone '{}': after names itself", m.id));
        } else if !scenario.milestone_events.iter().any(|o| &o.id == prev) {
            errors.push(format!("milestone '{}': after names unknown milestone '{}'", m.id, prev));
        }
    }

    // Check the split target (A12): a milestone that splits names its actor explicitly,
    // and that actor must be splittable — exactly one heir keeps the seat.
    for m in &scenario.milestone_events {
        let Some(target) = &m.splits_actor else { continue };
        match scenario.actors.iter().find(|a| &a.id == target && !a.is_successor_template) {
            None => errors.push(format!("milestone '{}': splits_actor '{}' is not a starting actor", m.id, target)),
            Some(a) => {
                let seats = a.on_collapse.iter().filter(|h| h.keeps_seat).count();
                if seats != 1 {
                    errors.push(format!("milestone '{}': splits_actor '{}' has {} seat-keeping heirs, exactly one is needed", m.id, target, seats));
                }
            }
        }
    }

    // A victory's `requires_alive` names starting actors (A10) — a successor template is not
    // alive at the start and may never be, so the victory could never count.
    if let Some(vc) = &scenario.victory_condition {
        for id in &vc.requires_alive {
            if !scenario.actors.iter().any(|a| &a.id == id && !a.is_successor_template) {
                errors.push(format!("victory_condition: requires_alive '{id}' is not a starting actor"));
            }
        }
        // Every actor the victory reads must be required alive (B44): a dead actor's
        // metric reads 0.0, and `ottomans.military_size < 40` passed on a dead Ottoman
        // empire in all 26 of its wins under the coalition upkeep.
        let reads = std::iter::once(&vc.metric).chain(vc.additional_conditions.iter().map(|c| &c.metric));
        for m in reads {
            if let crate::core::MetricRef::Actor { actor_id, .. } = m {
                if !vc.requires_alive.iter().any(|id| id == actor_id.as_str()) {
                    errors.push(format!(
                        "victory_condition reads actor '{}' ({m}) but does not require it alive",
                        actor_id.as_str()
                    ));
                }
            }
        }
    }

    // The consequence context is read only in `Consequences`, which only a milestone with
    // `triggers_collapse` reaches. Text with no way to be shown is dead, and dead text lies
    // the day someone restores the flag: it must be present if and only if some milestone
    // ends the scenario.
    let ends = scenario.milestone_events.iter().any(|m| m.triggers_collapse);
    if ends == scenario.consequence_context.trim().is_empty() {
        errors.push(format!(
            "consequence_context must be non-empty exactly when a milestone triggers_collapse \
             (triggers_collapse: {ends}, context empty: {})",
            scenario.consequence_context.trim().is_empty()
        ));
    }

    // Check dependency thresholds. Centralized here so every scenario routed
    // through `load_by_id` is checked even if it omits a per-scenario
    // `validate_dependencies` call. Metric-name checks (from/to) stay per-scenario
    // because they need that scenario's `KNOWN_METRICS`.
    if let Err(mut dep_errors) = crate::engine::validate_dependency_thresholds(&scenario.dependencies) {
        errors.append(&mut dep_errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Referential-integrity check for a metric key.
///
/// The *shape* of a key is no longer checked here — `MetricRef` cannot be built
/// from a malformed string at all, so a dotted global key (the shape behind every
/// metric-scoping bug in this project's history: #19, #20, narrative `key_metrics`)
/// now fails at load, in `Deserialize`. What is left is the half a type cannot do: whether the actor a key names
/// exists in *this* scenario. Shape is guaranteed by `MetricRef` itself.
fn check_actor_exists(metric: &MetricRef, actor_ids: &HashSet<&str>, context: &str, errors: &mut Vec<String>) {
    match metric {
        MetricRef::Actor { actor_id, .. } => {
            if !actor_ids.contains(actor_id.as_str()) {
                errors.push(format!("{}: unknown actor_id '{}' in metric '{}'", context, actor_id, metric));
            }
        }
        MetricRef::Global { key } => {
            if actor_ids.contains(key.as_str()) {
                errors.push(format!(
                    "{}: metric '{}' resolves to a GLOBAL key that is an actor id — \
                     an actor-relative key is missing its 'actor:' prefix",
                    context, metric
                ));
            }
        }
        MetricRef::Family { .. } => {}
    }
}

/// Load a scenario by ID with validation
pub fn load_by_id(id: &str) -> Option<Scenario> {
    let scenario = get_registry()
        .iter()
        .find(|e| e.id == id)
        .map(|e| (e.loader)())?;

    // Validate scenario
    match validate_scenario(&scenario) {
        Ok(()) => eprintln!("[SCENARIO] {} validated OK", id),
        Err(errors) => {
            for e in &errors {
                eprintln!("[SCENARIO] VALIDATION ERROR: {}", e);
            }
            // In debug mode — panic, in release — only warning
            #[cfg(debug_assertions)]
            panic!("Scenario '{}' failed validation", id);
        }
    }
    Some(scenario)
}

/// Get scenario list for UI
pub fn get_scenario_list() -> Vec<(String, String, i32, String)> {
    get_registry()
        .iter()
        .map(|e| (e.id.to_string(), e.name.to_string(), e.year, e.description.to_string()))
        .collect()
}

/// Get scenario metadata
pub fn get_scenario_meta() -> Vec<crate::commands::ScenarioMeta> {
    use crate::commands::ScenarioMeta;
    get_registry()
        .iter()
        .map(|e| {
            let scenario = (e.loader)();
            ScenarioMeta {
                id: e.id.to_string(),
                label: e.name.to_string(),
                description: e.description.to_string(),
                start_year: e.year,
                victory_title: scenario.victory_condition.as_ref().map(|vc| vc.title.clone()),
                victory_description: scenario.victory_condition.as_ref().map(|vc| vc.description.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Successor;

    #[test]
    fn every_registered_scenario_validates() {
        for entry in get_registry() {
            let scenario = (entry.loader)();
            assert!(validate_scenario(&scenario).is_ok(), "{}: {:?}", entry.id, validate_scenario(&scenario));
        }
    }

    #[test]
    fn validate_rejects_unknown_heir() {
        let mut scenario = crate::scenarios::milan_1477::load_milan_1477();
        let savoy = scenario.actors.iter_mut().find(|a| a.id == "savoy").unwrap();
        savoy.on_collapse = vec![Successor { id: "ghost".to_string(), weight: 1.0, keeps_seat: false }];
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("'savoy'") && e.contains("'ghost'")),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_rejects_two_seat_keepers_and_a_lone_seat() {
        let mut scenario = crate::scenarios::rome_375::load_rome_375();
        {
            let rome = scenario.actors.iter_mut().find(|a| a.id == "rome").unwrap();
            rome.on_collapse[1].keeps_seat = true;
        }
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("keeps_seat") && e.contains("at most one")), "{errors:?}");

        let mut scenario = crate::scenarios::rome_375::load_rome_375();
        {
            let visigoths = scenario.actors.iter_mut().find(|a| a.id == "visigoths").unwrap();
            visigoths.on_collapse[0].keeps_seat = true;
        }
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("lone heir")), "{errors:?}");
    }

    #[test]
    fn validate_checks_the_split_target() {
        // A12: the split target is named, and must be splittable.
        let scenario = crate::scenarios::rome_375::load_rome_375();
        assert!(validate_scenario(&scenario).is_ok(), "rome as authored must validate");

        let mut scenario = crate::scenarios::rome_375::load_rome_375();
        scenario.milestone_events.iter_mut().find(|m| m.id == "rome_splits").unwrap().splits_actor = Some("visigoths".to_string());
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("rome_splits") && e.contains("seat-keeping")), "{errors:?}");

        // A split is not the end of a scenario: rome's split does not trigger_collapse, and
        // that is valid (the flag and the target no longer ride together).
        let scenario = crate::scenarios::rome_375::load_rome_375();
        assert!(!scenario.milestone_events.iter().find(|m| m.id == "rome_splits").unwrap().triggers_collapse);

        // Consequence text present iff some milestone ends the scenario.
        let mut scenario = crate::scenarios::rome_375::load_rome_375();
        scenario.consequence_context = "Сценарный период завершён.".to_string();
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("consequence_context")), "{errors:?}");
        let mut scenario = crate::scenarios::rome_375::load_rome_375();
        scenario.milestone_events.iter_mut().find(|m| m.id == "rome_splits").unwrap().triggers_collapse = true;
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("consequence_context")), "{errors:?}");
    }

    #[test]
    fn validate_requires_victory_actors_alive() {
        // Both ways: the Ottoman condition back without the Ottomans in `requires_alive`
        // is rejected; with them listed it passes.
        let mut scenario = crate::scenarios::constantinople_1430::load_constantinople_1430();
        let vc = scenario.victory_condition.as_mut().unwrap();
        vc.additional_conditions = vec![crate::core::Condition {
            metric: crate::core::MetricRef::literal("actor:ottomans.military_size"),
            operator: crate::core::ComparisonOperator::Less,
            value: 40.0,
        }];
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("reads actor 'ottomans'")), "{errors:?}");
        scenario.victory_condition.as_mut().unwrap().requires_alive.push("ottomans".to_string());
        assert!(validate_scenario(&scenario).is_ok());
    }

    #[test]
    fn validate_rejects_a_victory_requiring_a_template() {
        let mut scenario = crate::scenarios::rome_375::load_rome_375();
        scenario.victory_condition.as_mut().unwrap().requires_alive = vec!["rome_west".to_string()];
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("requires_alive 'rome_west'")), "{errors:?}");
    }

    #[test]
    fn validate_checks_milestone_after() {
        let mut scenario = crate::scenarios::rome_375::load_rome_375();
        scenario.milestone_events.iter_mut().find(|m| m.id == "family_falls").unwrap().after = Some("family_falls".to_string());
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("after names itself")), "{errors:?}");
        let mut scenario = crate::scenarios::rome_375::load_rome_375();
        scenario.milestone_events.iter_mut().find(|m| m.id == "family_falls").unwrap().after = Some("ghost".to_string());
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("unknown milestone 'ghost'")), "{errors:?}");
    }

    #[test]
    fn validate_rejects_self_heir() {
        let mut scenario = crate::scenarios::milan_1477::load_milan_1477();
        let savoy = scenario.actors.iter_mut().find(|a| a.id == "savoy").unwrap();
        savoy.on_collapse = vec![Successor { id: "savoy".to_string(), weight: 1.0, keeps_seat: false }];
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("names itself")), "{errors:?}");
    }
}
