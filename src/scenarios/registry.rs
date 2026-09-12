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
            name: "Rome 375 — Семья Ди Милано",
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
    for metric in &scenario.narrative_config.key_metrics {
        check_actor_exists(metric, &actor_ids, "narrative_config.key_metrics", &mut errors);
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
    fn validate_rejects_self_heir() {
        let mut scenario = crate::scenarios::milan_1477::load_milan_1477();
        let savoy = scenario.actors.iter_mut().find(|a| a.id == "savoy").unwrap();
        savoy.on_collapse = vec![Successor { id: "savoy".to_string(), weight: 1.0, keeps_seat: false }];
        let errors = validate_scenario(&scenario).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("names itself")), "{errors:?}");
    }
}
