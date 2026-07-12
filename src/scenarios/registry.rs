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
        if let MetricRef::Actor { ref actor_id, .. } = MetricRef::parse(&delta.metric) {
            if !actor_ids.contains(actor_id.as_str()) {
                errors.push(format!("auto_delta: unknown actor_id '{}'", actor_id));
            }
        }
        for cond in &delta.conditions {
            validate_metric_ref(&cond.metric, &actor_ids, "auto_delta.condition", &mut errors);
        }
        for ratio in &delta.ratio_conditions {
            validate_metric_ref(&ratio.metric_a, &actor_ids, "ratio_condition.metric_a", &mut errors);
            validate_metric_ref(&ratio.metric_b, &actor_ids, "ratio_condition.metric_b", &mut errors);
        }
    }

    // Check milestone effects
    for milestone in &scenario.milestone_events {
        for metric in milestone.condition.to_metric_strings().iter() {
            validate_metric_ref(metric, &actor_ids, &format!("milestone '{}'", milestone.id), &mut errors);
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
            validate_metric_ref(metric, &actor_ids, &format!("action '{}'", action.id), &mut errors);
        }
    }

    // Check status_indicators
    for indicator in &scenario.status_indicators {
        validate_metric_ref(&indicator.metric, &actor_ids, &format!("status_indicator '{}'", indicator.label), &mut errors);
    }

    // Check narrative key_metrics. These feed the chronicler's prompt and were never
    // validated, which is why 13 of the 16 keys across the three scenarios had been
    // resolving to 0.0 unnoticed.
    for metric in &scenario.narrative_config.key_metrics {
        validate_metric_ref(metric, &actor_ids, "narrative_config.key_metrics", &mut errors);
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

fn validate_metric_ref(metric: &str, actor_ids: &HashSet<&str>, context: &str, errors: &mut Vec<String>) {
    match MetricRef::parse(metric) {
        MetricRef::Actor { ref actor_id, .. } => {
            if !actor_ids.contains(actor_id.as_str()) {
                errors.push(format!("{}: unknown actor_id '{}' in metric '{}'", context, actor_id, metric));
            }
        }
        // A `Global` ref that carries a `.` or names an actor is an actor-relative key
        // that lost its `actor:` prefix. `MetricRef::parse` resolves it to a global key
        // nothing reads or writes, so the condition reads 0.0 and the effect goes
        // nowhere — silently, forever. Every metric-scoping bug in this project's history
        // (#19, #20, and the narrative key_metrics) produced exactly such a key, and each
        // was found one instance per session, by hand. This is the load-time choke point
        // that makes the shape unrepresentable in content.
        MetricRef::Global { ref key } => {
            if key.contains('.') {
                errors.push(format!(
                    "{}: metric '{}' resolves to a GLOBAL key containing '.', which no \
                     subsystem reads or writes — did you mean 'actor:{}'?",
                    context, metric, key
                ));
            } else if actor_ids.contains(key.as_str()) {
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
