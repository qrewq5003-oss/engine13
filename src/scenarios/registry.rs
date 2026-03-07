use crate::core::Scenario;

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

/// Load a scenario by ID
pub fn load_by_id(id: &str) -> Option<Scenario> {
    get_registry()
        .iter()
        .find(|e| e.id == id)
        .map(|e| (e.loader)())
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
