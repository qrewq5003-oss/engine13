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

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_metric_ref(metric: &str, actor_ids: &HashSet<&str>, context: &str, errors: &mut Vec<String>) {
    if let MetricRef::Actor { ref actor_id, .. } = MetricRef::parse(metric) {
        if !actor_ids.contains(actor_id.as_str()) {
            errors.push(format!("{}: unknown actor_id '{}' in metric '{}'", context, actor_id, metric));
        }
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
        .map(|e| ScenarioMeta {
            id: e.id.to_string(),
            label: e.name.to_string(),
            description: e.description.to_string(),
            start_year: e.year,
        })
        .collect()
}
