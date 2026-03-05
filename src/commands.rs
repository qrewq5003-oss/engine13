use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::core::{Actor, Event, Scenario, WorldState};
use crate::engine::{tick, EventLog};
use crate::scenarios::load_rome_375;

// ============================================================================
// Application State
// ============================================================================

/// Shared application state
pub struct AppState {
    pub world_state: Option<WorldState>,
    pub event_log: EventLog,
    pub current_scenario: Option<Scenario>,
    pub saves: HashMap<String, SaveData>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            world_state: None,
            event_log: EventLog::new(),
            current_scenario: None,
            saves: HashMap::new(),
        }
    }
}

/// Save data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveData {
    pub id: String,
    pub name: String,
    pub scenario_id: String,
    pub tick: u32,
    pub year: i32,
    pub created_at: u64,
    pub world_state: WorldState,
    pub event_log: Vec<Event>,
}

/// LLM trigger response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTrigger {
    pub prompt: String,
    pub context: LlmContext,
}

/// Context for LLM generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmContext {
    pub current_year: i32,
    pub current_tick: u32,
    pub narrative_actors: Vec<String>,
    pub recent_events: Vec<String>,
    pub scenario_context: String,
}

/// Response from advance_tick
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceTickResponse {
    pub world_state: WorldState,
    pub events: Vec<Event>,
    pub llm_trigger: Option<LlmTrigger>,
}

/// Response from submit_action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitActionResponse {
    pub success: bool,
    pub effects: HashMap<String, f64>,
    pub costs: HashMap<String, f64>,
    pub new_state: WorldState,
    pub llm_trigger: Option<LlmTrigger>,
    pub error: Option<String>,
}

/// Response from save_game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveResponse {
    pub success: bool,
    pub save_id: Option<String>,
    pub error: Option<String>,
}

/// Response from load_game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadResponse {
    pub success: bool,
    pub world_state: Option<WorldState>,
    pub error: Option<String>,
}

/// Scenario metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioMeta {
    pub id: String,
    pub label: String,
    pub description: String,
    pub start_year: i32,
}

// ============================================================================
// Input Types
// ============================================================================

/// Input for player action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerActionInput {
    pub action_id: String,
    pub target_actor_id: Option<String>,
}

// ============================================================================
// Core Command Functions (work with &mut AppState)
// ============================================================================

/// Advance simulation by one tick
pub fn advance_tick(state: &mut AppState, action: Option<PlayerActionInput>) -> Result<AdvanceTickResponse, String> {
    if let Some(action_input) = action {
        apply_player_action(state, &action_input)?;
    }

    let world_state = state.world_state.as_mut().ok_or("No active world state")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    tick(world_state, scenario, &mut state.event_log);

    let world_state_clone = world_state.clone();
    let scenario_clone = scenario.clone();
    let event_log_clone = state.event_log.clone();

    let llm_trigger = check_llm_trigger_with_data(&world_state_clone, &scenario_clone, &event_log_clone);
    let events = state.event_log.events.clone();

    Ok(AdvanceTickResponse {
        world_state: world_state.clone(),
        events,
        llm_trigger,
    })
}

/// Get available actions for the player
pub fn get_available_actions(state: &AppState) -> Result<Vec<crate::core::PatronAction>, String> {
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
    let world_state = state.world_state.as_ref().ok_or("No active world state")?;

    let player_actor = world_state.actors.get("rome").ok_or("Player actor not found")?;

    let available_actions = scenario
        .patron_actions
        .iter()
        .filter(|action| is_action_available(action, player_actor))
        .cloned()
        .collect();

    Ok(available_actions)
}

/// Submit a player action
pub fn submit_action(state: &mut AppState, action_id: String) -> Result<SubmitActionResponse, String> {
    let action_input = PlayerActionInput { action_id, target_actor_id: None };
    let (effects, costs) = apply_player_action(state, &action_input)?;

    let world_state = state.world_state.as_mut().ok_or("No active world state")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    tick(world_state, scenario, &mut state.event_log);

    let world_state_clone = world_state.clone();
    let scenario_clone = scenario.clone();
    let event_log_clone = state.event_log.clone();

    let llm_trigger = check_llm_trigger_with_data(&world_state_clone, &scenario_clone, &event_log_clone);

    Ok(SubmitActionResponse {
        success: true,
        effects,
        costs,
        new_state: world_state.clone(),
        llm_trigger,
        error: None,
    })
}

/// Save current game state
pub fn save_game(state: &mut AppState, slot: Option<String>, name: Option<String>) -> Result<SaveResponse, String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state to save")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    let save_id = slot.unwrap_or_else(|| format!("autosave_{}", world_state.tick));
    let save_name = name.unwrap_or_else(|| format!("Tick {} - Year {}", world_state.tick, world_state.year));

    let save_data = SaveData {
        id: save_id.clone(),
        name: save_name,
        scenario_id: scenario.id.clone(),
        tick: world_state.tick,
        year: world_state.year,
        created_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        world_state: world_state.clone(),
        event_log: state.event_log.events.clone(),
    };

    state.saves.insert(save_id.clone(), save_data);

    Ok(SaveResponse { success: true, save_id: Some(save_id), error: None })
}

/// Load game from save
pub fn load_game(state: &mut AppState, save_id: String) -> Result<LoadResponse, String> {
    let save_data = state.saves.get(&save_id).cloned().ok_or_else(|| format!("Save '{}' not found", save_id))?;

    state.world_state = Some(save_data.world_state.clone());
    state.event_log = EventLog { events: save_data.event_log };

    if state.current_scenario.as_ref().map(|s| s.id.clone()) != Some(save_data.scenario_id.clone()) {
        match save_data.scenario_id.as_str() {
            "rome_375" => { state.current_scenario = Some(load_rome_375()); }
            _ => return Err(format!("Unknown scenario: {}", save_data.scenario_id)),
        }
    }

    Ok(LoadResponse { success: true, world_state: Some(save_data.world_state), error: None })
}

/// Get relevant events
pub fn get_relevant_events(state: &AppState, actor_ids: Vec<String>) -> Result<Vec<Event>, String> {
    let mut relevant_events: Vec<Event> = state
        .event_log
        .events
        .iter()
        .filter(|e| e.actor_id == "scenario" || actor_ids.contains(&e.actor_id) || e.involved_actors.iter().any(|a| actor_ids.contains(a)))
        .cloned()
        .collect();

    relevant_events.sort_by(|a, b| b.tick.cmp(&a.tick));
    relevant_events.truncate(20);

    Ok(relevant_events)
}

/// Load a scenario
pub fn load_scenario(state: &mut AppState, scenario_id: String) -> Result<SaveResponse, String> {
    let scenario = match scenario_id.as_str() {
        "rome_375" => load_rome_375(),
        _ => return Err(format!("Unknown scenario: {}", scenario_id)),
    };

    let mut world_state = WorldState::new(scenario.id.clone(), scenario.start_year);
    for actor in &scenario.actors {
        world_state.actors.insert(actor.id.clone(), actor.clone());
    }

    // Initialize family_metrics from Rome actor's scenario_metrics
    if let Some(rome_actor) = world_state.actors.get("rome") {
        for (key, value) in &rome_actor.scenario_metrics {
            if key.starts_with("family_") {
                world_state.family_metrics.insert(key.clone(), *value);
            }
        }
    }

    state.current_scenario = Some(scenario);
    state.world_state = Some(world_state);
    state.event_log = EventLog::new();

    Ok(SaveResponse { success: true, save_id: None, error: None })
}

/// Get scenario list
pub fn get_scenario_list() -> Vec<ScenarioMeta> {
    vec![
        ScenarioMeta {
            id: "rome_375".to_string(),
            label: "Rome 375 — Семья Ди Милано".to_string(),
            description: "375 год. Медиолан — фактическая столица Западной Империи.".to_string(),
            start_year: 375,
        },
    ]
}

// ============================================================================
// Helper Functions
// ============================================================================

fn apply_player_action(state: &mut AppState, action_input: &PlayerActionInput) -> Result<(HashMap<String, f64>, HashMap<String, f64>), String> {
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
    let world_state = state.world_state.as_mut().ok_or("No active world state")?;

    let action = scenario.patron_actions.iter()
        .find(|a| a.id == action_input.action_id)
        .ok_or_else(|| format!("Action '{}' not found", action_input.action_id))?;

    let player_actor = world_state.actors.get_mut("rome").ok_or("Player actor not found")?;

    if !is_action_available(action, player_actor) {
        return Err("Action is not available".to_string());
    }

    // Apply cost
    let mut applied_costs = HashMap::new();
    for (metric, cost) in &action.cost {
        if metric.starts_with("rome.") {
            let rome_metric = metric.strip_prefix("rome.").unwrap();
            apply_metric_delta(&mut player_actor.metrics, rome_metric, *cost);
            applied_costs.insert(metric.clone(), *cost);
        } else if metric.starts_with("family_") {
            // Family metrics cost - applied directly to world_state.family_metrics
            let current = world_state.family_metrics.get(metric).copied().unwrap_or(0.0);
            world_state.family_metrics.insert(metric.clone(), current + *cost);
            applied_costs.insert(metric.clone(), *cost);
        }
    }

    eprintln!("[DEBUG] apply_player_action - applied_costs: {:?}", applied_costs);
    eprintln!("[DEBUG] apply_player_action - family_metrics after cost: {:?}", world_state.family_metrics);

    // Apply effects
    let mut applied_effects = HashMap::new();
    for (metric, effect) in &action.effects {
        if metric.starts_with("rome.") {
            let rome_metric = metric.strip_prefix("rome.").unwrap();
            apply_metric_delta(&mut player_actor.metrics, rome_metric, *effect);
            applied_effects.insert(metric.clone(), *effect);
        } else if metric.starts_with("family_") {
            // Family metrics effects - applied directly to world_state.family_metrics
            let current = world_state.family_metrics.get(metric).copied().unwrap_or(0.0);
            world_state.family_metrics.insert(metric.clone(), current + *effect);
            applied_effects.insert(metric.clone(), *effect);
        }
    }

    eprintln!("[DEBUG] apply_player_action - applied_effects: {:?}", applied_effects);
    eprintln!("[DEBUG] apply_player_action - family_metrics after effects: {:?}", world_state.family_metrics);

    // Record event
    let event = Event::new(
        format!("player_action_{}", action_input.action_id),
        world_state.tick,
        world_state.year,
        "rome".to_string(),
        crate::core::EventType::PlayerAction,
        true,
        format!("Действие игрока: {}", action.name),
    );
    state.event_log.add(event);

    Ok((applied_effects, applied_costs))
}

fn is_action_available(action: &crate::core::PatronAction, player_actor: &Actor) -> bool {
    match &action.available_if {
        crate::core::ActionCondition::Always => true,
        crate::core::ActionCondition::Metric { metric, operator, value } => {
            let current = if metric.starts_with("family_") {
                player_actor.scenario_metrics.get(metric).copied().unwrap_or(0.0)
            } else {
                get_metric_value(&player_actor.metrics, metric)
            };
            compare_value(current, operator, value)
        }
    }
}

fn compare_value(value: f64, operator: &crate::core::ComparisonOperator, target: &f64) -> bool {
    match operator {
        crate::core::ComparisonOperator::Less => value < *target,
        crate::core::ComparisonOperator::LessOrEqual => value <= *target,
        crate::core::ComparisonOperator::Greater => value > *target,
        crate::core::ComparisonOperator::GreaterOrEqual => value >= *target,
        crate::core::ComparisonOperator::Equal => (value - target).abs() < 0.001,
    }
}

fn get_metric_value(metrics: &crate::core::ActorMetrics, name: &str) -> f64 {
    match name {
        "population" => metrics.population,
        "military_size" => metrics.military_size,
        "military_quality" => metrics.military_quality,
        "economic_output" => metrics.economic_output,
        "cohesion" => metrics.cohesion,
        "legitimacy" => metrics.legitimacy,
        "external_pressure" => metrics.external_pressure,
        "treasury" => metrics.treasury,
        _ => 0.0,
    }
}

fn apply_metric_delta(metrics: &mut crate::core::ActorMetrics, metric: &str, delta: f64) {
    match metric {
        "population" => metrics.population += delta,
        "military_size" => metrics.military_size += delta,
        "military_quality" => metrics.military_quality += delta,
        "economic_output" => metrics.economic_output += delta,
        "cohesion" => metrics.cohesion += delta,
        "legitimacy" => metrics.legitimacy += delta,
        "external_pressure" => metrics.external_pressure += delta,
        "treasury" => metrics.treasury += delta,
        _ => {}
    }
}

fn check_llm_trigger_with_data(world_state: &WorldState, scenario: &Scenario, event_log: &EventLog) -> Option<LlmTrigger> {
    let narrative_actor_ids: Vec<String> = world_state
        .actors
        .values()
        .filter(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground)
        .map(|a| a.id.clone())
        .collect();

    let recent_events: Vec<String> = event_log
        .events
        .iter()
        .filter(|e| e.is_key || narrative_actor_ids.contains(&e.actor_id))
        .take(5)
        .map(|e| e.description.clone())
        .collect();

    let should_trigger = !recent_events.is_empty()
        || event_log.events.iter().any(|e| e.event_type == crate::core::EventType::Milestone);

    if should_trigger {
        Some(LlmTrigger {
            prompt: generate_llm_prompt(world_state, scenario, &narrative_actor_ids),
            context: LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids,
                recent_events,
                scenario_context: scenario.llm_context.clone(),
            },
        })
    } else {
        None
    }
}

fn generate_llm_prompt(world_state: &WorldState, scenario: &Scenario, narrative_actor_ids: &[String]) -> String {
    let mut prompt = format!("Год: {}\nСценарий: {}\n\n", world_state.year, scenario.label);
    prompt.push_str("Активные акторы:\n");
    for actor_id in narrative_actor_ids {
        if let Some(actor) = world_state.actors.get(actor_id) {
            prompt.push_str(&format!(
                "- {}: население {:.0}k, армия {:.0}k, сплочённость {:.0}, легитимность {:.0}\n",
                actor.name_short,
                actor.metrics.population / 1000.0,
                actor.metrics.military_size / 1000.0,
                actor.metrics.cohesion,
                actor.metrics.legitimacy
            ));
        }
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> AppState {
        let mut state = AppState::default();
        let scenario = load_rome_375();
        let mut world_state = WorldState::new(scenario.id.clone(), scenario.start_year);
        for actor in &scenario.actors {
            world_state.actors.insert(actor.id.clone(), actor.clone());
        }
        state.current_scenario = Some(scenario);
        state.world_state = Some(world_state);
        state
    }

    #[test]
    fn test_advance_tick() {
        let mut state = create_test_state();
        let initial_tick = state.world_state.as_ref().unwrap().tick;
        let result = advance_tick(&mut state, None);
        assert!(result.is_ok());
        assert_eq!(state.world_state.as_ref().unwrap().tick, initial_tick + 1);
    }

    #[test]
    fn test_get_available_actions() {
        let state = create_test_state();
        let result = get_available_actions(&state);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let mut state = create_test_state();
        advance_tick(&mut state, None).unwrap();
        save_game(&mut state, Some("test".to_string()), None).unwrap();
        advance_tick(&mut state, None).unwrap();
        let tick_after = state.world_state.as_ref().unwrap().tick;
        load_game(&mut state, "test".to_string()).unwrap();
        assert_eq!(state.world_state.as_ref().unwrap().tick, tick_after - 1);
    }
}
