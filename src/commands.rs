use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::core::{Event, Scenario, WorldState};
use crate::db::Db;
use crate::engine::{tick, EventLog};

// ============================================================================
// Application State
// ============================================================================

/// Shared application state
pub struct AppState {
    pub world_state: Option<WorldState>,
    pub event_log: EventLog,
    pub current_scenario: Option<Scenario>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            world_state: None,
            event_log: EventLog::new(),
            current_scenario: None,
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

/// Response from advance_tick
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceTickResponse {
    pub world_state: WorldState,
    pub events: Vec<Event>,
    pub llm_trigger: Option<crate::llm::LlmTrigger>,
}

/// Response from submit_action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitActionResponse {
    pub success: bool,
    pub effects: HashMap<String, f64>,
    pub costs: HashMap<String, f64>,
    pub new_state: WorldState,
    pub llm_trigger: Option<crate::llm::LlmTrigger>,
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
// Core Command Functions (delegate to application modules)
// ============================================================================

/// Advance simulation by one tick
pub fn advance_tick(state: &mut AppState, action: Option<PlayerActionInput>) -> Result<AdvanceTickResponse, String> {
    use crate::application::{apply_player_action, check_llm_trigger_with_data};

    // Track action info for trigger
    let mut action_info: Option<(crate::core::PatronAction, HashMap<String, f64>, HashMap<String, f64>)> = None;

    if let Some(action_input) = action {
        let action_id = action_input.action_id.clone();
        // Convert commands::PlayerActionInput to application::actions::PlayerActionInput
        let app_action_input = crate::application::actions::PlayerActionInput {
            action_id: action_input.action_id,
            target_actor_id: action_input.target_actor_id,
        };
        let (effects, costs) = apply_player_action(state, &app_action_input)?;

        // Get action details for trigger
        let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
        if let Some(action) = scenario.patron_actions.iter().find(|a| a.id == action_id) {
            action_info = Some((action.clone(), effects, costs));
        }
    }

    let world_state = state.world_state.as_mut().ok_or("No active world state")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    tick(world_state, scenario, &mut state.event_log);

    let scenario_clone = scenario.clone();
    let event_log_clone = state.event_log.clone();

    // Pass action info to trigger check - this will reset ticks_since_last_narrative if trigger fires
    let action_info_ref = action_info.as_ref().map(|(a, e, c)| (a, e, c));
    let llm_trigger = check_llm_trigger_with_data(world_state, &scenario_clone, &event_log_clone, action_info_ref);
    let events = state.event_log.events.clone();

    Ok(AdvanceTickResponse {
        world_state: world_state.clone(),
        events,
        llm_trigger,
    })
}

/// Get available actions for the player - delegates to application::actions
pub fn get_available_actions(state: &AppState) -> Result<Vec<crate::core::PatronAction>, String> {
    crate::application::get_available_actions(state)
}

/// Submit a player action - delegates to application::actions
pub fn submit_action(state: &mut AppState, action_id: String) -> Result<SubmitActionResponse, String> {
    use crate::application::actions;

    let action_input = actions::PlayerActionInput {
        action_id,
        target_actor_id: None,
    };
    actions::submit_action(state, action_input)
}

/// Save current game state - delegates to application::save_load
pub fn save_game(state: &mut AppState, db: &Db, slot: Option<String>) -> Result<SaveResponse, String> {
    crate::application::save_game(state, db, slot)
}

/// Load game from save - delegates to application::save_load
pub fn load_game(state: &mut AppState, db: &Db, save_id: String) -> Result<LoadResponse, String> {
    crate::application::load_game(state, db, save_id)
}

/// List all saves - delegates to application::save_load
pub fn list_saves(db: &Db) -> Vec<SaveData> {
    crate::application::list_saves(db)
}

/// List saves with slots - delegates to application::save_load
pub fn list_saves_with_slots(db: &Db, scenario_id: &str) -> Result<crate::application::SaveSlotList, String> {
    crate::application::list_saves_with_slots(db, scenario_id)
}

/// Get relevant events for an actor
pub fn get_relevant_events(db: &Db, actor_id: String) -> Result<Vec<Event>, String> {
    db.get_events_by_actor(&actor_id)
        .map_err(|e| format!("Failed to get events: {}", e))
}

/// Status indicator state for UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusIndicatorState {
    pub label: String,
    pub value: f64,
    pub status_text: String,
    pub progress: f64, // 0.0-1.0
    pub invert: bool,
}

/// Action history entry for UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionHistoryEntry {
    pub tick: u32,
    pub year: i32,
    pub action_id: String,
    pub action_name: String,
    pub effects_summary: Vec<String>,
}

/// Get action history from database
pub fn get_action_history(db: &Db, limit: usize) -> Result<Vec<ActionHistoryEntry>, String> {
    let events = db.get_events_by_type("PlayerAction", limit)
        .map_err(|e| format!("Failed to get action history: {}", e))?;

    let history = events
        .into_iter()
        .map(|event| {
            let effects_summary = parse_effects_summary(&event.metadata);
            ActionHistoryEntry {
                tick: event.tick,
                year: event.year,
                action_id: event.id.clone(),
                action_name: event.description.clone(),
                effects_summary,
            }
        })
        .collect();

    Ok(history)
}

fn parse_effects_summary(metadata: &str) -> Vec<String> {
    if metadata.is_empty() {
        return vec![];
    }

    serde_json::from_str::<HashMap<String, f64>>(metadata)
        .unwrap_or_default()
        .into_iter()
        .map(|(metric, delta)| {
            let sign = if delta > 0.0 { "+" } else { "" };
            format!("{}: {}{:.0}", format_metric_name(&metric), sign, delta)
        })
        .collect()
}

fn format_metric_name(metric: &str) -> String {
    metric
        .replace("global:federation_progress", "Федерация")
        .replace("venice.treasury", "Казна Венеции")
        .replace("genoa.treasury", "Казна Генуи")
        .replace("milan.treasury", "Казна Милана")
        .replace(".treasury", " казна")
        .replace(".military_size", " армия")
        .replace(".legitimacy", " легитимность")
        .replace("family:influence", "Влияние семьи")
        .replace("family:wealth", "Богатство семьи")
}

/// Compute status indicators from world state and scenario
pub fn compute_status_indicators(
    world_state: &WorldState,
    scenario: &Scenario,
) -> Vec<StatusIndicatorState> {
    use crate::core::MetricRef;

    scenario.status_indicators.iter().map(|indicator| {
        let metric_ref = MetricRef::parse(&indicator.metric);
        let value = metric_ref.get(world_state);

        // Find current status text - last threshold where value >= threshold
        let mut status_text = indicator.thresholds.first()
            .map(|(_, text)| text.clone())
            .unwrap_or_else(|| "unknown".to_string());

        for (threshold, text) in &indicator.thresholds {
            if value >= *threshold {
                status_text = text.clone();
            }
        }

        // Calculate progress (value / max_threshold)
        let max_threshold = indicator.thresholds.last()
            .map(|(t, _)| *t)
            .unwrap_or(100.0);
        let progress = if max_threshold > 0.0 {
            (value / max_threshold).clamp(0.0, 1.0)
        } else {
            0.0
        };

        StatusIndicatorState {
            label: indicator.label.clone(),
            value,
            status_text,
            progress,
            invert: indicator.invert,
        }
    }).collect()
}

/// Set game mode - delegates to application::modes
pub fn set_game_mode(state: &mut AppState, new_mode: crate::core::GameMode) -> Result<(), String> {
    crate::application::set_game_mode(state, new_mode)
}

/// Load a scenario - delegates to application::save_load
pub fn load_scenario(state: &mut AppState, db: &Db, scenario_id: String) -> Result<SaveResponse, String> {
    crate::application::load_scenario(state, db, scenario_id)
}

/// Get scenario list - delegates to scenarios::registry
pub fn get_scenario_list() -> Vec<ScenarioMeta> {
    crate::scenarios::registry::get_scenario_meta()
}

/// Get narrative from LLM - delegates to application::narrative
pub async fn cmd_get_narrative(
    state: &AppState,
    db: &Db,
    app: tauri::AppHandle,
    season: crate::llm::NarrativeSeason,
) -> Result<(), String> {
    crate::application::cmd_get_narrative(state, db, app, season).await
}

/// Get available models from LLM provider - delegates to llm module
pub fn cmd_get_available_models(provider: String, base_url: String, api_key: Option<String>) -> Result<Vec<String>, String> {
    crate::llm::get_available_models(provider, base_url, api_key)
}

/// Save LLM config - delegates to llm module
pub fn cmd_save_llm_config(provider: String, base_url: String, api_key: Option<String>, model: String) -> Result<(), String> {
    let config = crate::llm::LlmConfig {
        provider,
        api_key,
        model,
        base_url,
    };
    crate::llm::save_llm_config(&config)
}
