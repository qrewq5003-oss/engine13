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
    pub rng: Option<rand_chacha::ChaCha8Rng>,
    /// Narrative memory for anti-repetition across turns
    /// This does NOT affect simulation logic - only used for prompt generation
    pub narrative_memory: crate::llm::NarrativeMemory,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            world_state: None,
            event_log: EventLog::new(),
            current_scenario: None,
            rng: None,
            narrative_memory: crate::llm::NarrativeMemory::default(),
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

/// Response from advance_tick_silent (no LLM trigger)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceTickSilentResponse {
    pub world_state: WorldState,
    pub events: Vec<Event>,
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
    pub victory_title: Option<String>,
    pub victory_description: Option<String>,
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
    let rng = state.rng.as_mut().ok_or("No RNG initialized")?;

    tick(world_state, scenario, &mut state.event_log, rng);

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

/// Advance simulation by one tick without LLM trigger
pub fn advance_tick_silent(state: &mut AppState) -> Result<AdvanceTickSilentResponse, String> {
    let world_state = state.world_state.as_mut().ok_or("No active world state")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
    let rng = state.rng.as_mut().ok_or("No RNG initialized")?;

    tick(world_state, scenario, &mut state.event_log, rng);

    let events = state.event_log.events.clone();

    Ok(AdvanceTickSilentResponse {
        world_state: world_state.clone(),
        events,
    })
}

/// Get available actions for the player - delegates to application::actions
pub fn get_available_actions(state: &AppState) -> Result<Vec<crate::core::PatronAction>, String> {
    crate::application::get_available_actions(state)
}

/// Get all actions with availability status - delegates to application::actions
pub fn get_actions_with_availability(state: &AppState) -> Result<Vec<crate::application::actions::ActionInfo>, String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
    Ok(crate::application::actions::list_actions_with_availability(world_state, scenario))
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

/// Set actor metric value (debug command)
pub fn set_metric(state: &mut AppState, actor_id: String, metric: String, value: f64) -> Result<(), String> {
    // Validate value
    if value.is_nan() || value.is_infinite() {
        return Err("Invalid metric value: NaN or infinity".to_string());
    }

    let world_state = state.world_state.as_mut().ok_or("No active world state")?;
    let actor = world_state.actors.get_mut(&actor_id).ok_or_else(|| format!("Actor '{}' not found", actor_id))?;

    // Check if metric exists
    if !actor.metrics.contains_key(&metric) {
        return Err(format!("Metric '{}' not found for actor '{}'", metric, actor_id));
    }

    // Metric-specific validation
    let clamped_value = match metric.as_str() {
        "treasury" => value,  // can go negative (debts)
        "population" | "military_size" => value.max(0.0),  // no upper bound
        _ => value.clamp(0.0, 100.0),  // cohesion, legitimacy, etc.
    };
    actor.metrics.insert(metric, clamped_value);

    Ok(())
}

/// Force spawn a new actor (debug command)
pub fn force_spawn(
    state: &mut AppState,
    actor_id: String,
    label: String,
    lat: f64,
    lng: f64,
    initial_metrics: std::collections::HashMap<String, f64>,
) -> Result<(), String> {
    use crate::core::{Actor, GeoCoordinate, NarrativeStatus, RegionRank, Religion, Culture, Event, EventType};
    use std::collections::HashMap;

    // Validate actor_id format: only [a-z0-9_]+
    if !actor_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err("Invalid actor_id: only [a-z0-9_]+ allowed".to_string());
    }

    // Validate lat/lng
    if lat < -90.0 || lat > 90.0 {
        return Err("Invalid lat: must be between -90 and 90".to_string());
    }
    if lng < -180.0 || lng > 180.0 {
        return Err("Invalid lng: must be between -180 and 180".to_string());
    }

    let world_state = state.world_state.as_mut().ok_or("No active world state")?;

    // Check for duplicate
    if world_state.actors.contains_key(&actor_id) {
        return Err(format!("Actor '{}' already exists", actor_id));
    }
    if world_state.dead_actors.iter().any(|d| d.id == actor_id) {
        return Err(format!("Actor '{}' is already dead", actor_id));
    }

    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    // Create actor
    let actor = Actor {
        id: actor_id.clone(),
        name: label.clone(),
        name_short: label.clone(),
        region: actor_id.clone(),
        region_rank: RegionRank::C,
        era: scenario.era.clone(),
        narrative_status: NarrativeStatus::Background,
        tags: vec![],
        metrics: initial_metrics,
        scenario_metrics: HashMap::new(),
        neighbors: vec![],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(GeoCoordinate { lat, lng }),
        is_successor_template: false,
        religion: Religion::Orthodox,
        culture: Culture::Slavic,
        minimum_survival_ticks: None,
        leader: None,
    };

    world_state.actors.insert(actor_id.clone(), actor);

    // Create event
    let current_tick = world_state.tick;
    let current_year = world_state.year;
    let event = Event::new(
        format!("force_spawn_{}", actor_id),
        current_tick,
        current_year,
        actor_id.clone(),
        EventType::Milestone,
        true,
        format!("{} появился на сцене истории (debug).", label),
    );
    state.event_log.add(event);

    Ok(())
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

/// Get relevant events with scoring based on importance, actor relevance, temporal decay, and narrative status
/// Filters events by scenario_id to prevent cross-scenario event leakage.
pub fn get_relevant_events(
    db: &Db,
    actor_ids: Vec<String>,
    current_tick: u32,
    query_tags: Vec<String>,
    scenario_id: Option<&str>,
) -> Result<Vec<Event>, String> {
    // Fetch all events for the provided actor IDs, filtered by scenario if provided
    let mut all_events: Vec<Event> = Vec::new();
    for actor_id in &actor_ids {
        let events = db.get_events_by_actor_for_scenario(actor_id, scenario_id)
            .map_err(|e| format!("Failed to get events: {}", e))?;
        all_events.extend(events);
    }

    // Get key events for this scenario
    if let Some(sid) = scenario_id {
        let key_events = db.get_key_events_for_scenario(sid)
            .map_err(|e| format!("Failed to get key events: {}", e))?;
        for event in key_events {
            if !all_events.iter().any(|e| e.id == event.id) {
                all_events.push(event);
            }
        }
    }

    // Score and sort events
    let foreground_ids: Vec<String> = actor_ids;
    let mut scored_events: Vec<(Event, f64)> = all_events
        .into_iter()
        .map(|event| {
            let score = calculate_event_relevance(&event, &foreground_ids, current_tick, &query_tags);
            (event, score)
        })
        .collect();

    // Sort by score descending
    scored_events.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Return top 20 events
    Ok(scored_events.into_iter().take(20).map(|(e, _)| e).collect())
}

/// Calculate tag similarity between event tags and query tags
/// Returns Jaccard-like similarity: matches / max(len(event_tags), len(query_tags))
fn tag_similarity(event_tags: &[String], query_tags: &[String]) -> f64 {
    if query_tags.is_empty() {
        return 0.5;
    }
    
    // Normalize to lowercase for comparison and remove duplicates
    let query_set: std::collections::HashSet<String> = 
        query_tags.iter().map(|t| t.to_lowercase()).collect();
    let event_set: std::collections::HashSet<String> = 
        event_tags.iter().map(|t| t.to_lowercase()).collect();
    
    let matches = event_set.iter().filter(|t| query_set.contains(t.as_str())).count();
    let denom = event_set.len().max(query_set.len());
    
    if denom == 0 {
        return 0.5;
    }
    
    matches as f64 / denom as f64
}

/// Calculate relevance score for an event
/// score = importance_weight * temporal_decay * narrative_weight * (0.25 + 0.75 * tag_similarity)
fn calculate_event_relevance(event: &Event, foreground_ids: &[String], current_tick: u32, query_tags: &[String]) -> f64 {
    use crate::EventType;

    // Temporal decay - use step function based on age
    let age_in_ticks = current_tick.saturating_sub(event.tick);
    let temporal_decay = match age_in_ticks {
        0..=2 => 1.0,
        3..=5 => 0.7,
        6..=10 => 0.4,
        _ => 0.2,
    };

    // Event importance weight
    let importance_weight = match event.event_type {
        EventType::Collapse | EventType::Milestone => 3.0,
        EventType::PlayerAction | EventType::War => 2.0,
        _ => 1.0,
    };

    // Narrative status weight - foreground actors get higher weight
    let narrative_weight = if foreground_ids.contains(&event.actor_id) {
        1.5
    } else {
        1.0
    };

    // Tag similarity score
    let tag_sim = tag_similarity(&event.tags, query_tags);

    // Final score: base weights * (0.25 + 0.75 * tag_similarity)
    // This ensures tag similarity contributes 75% to the final score
    let base_score = importance_weight * temporal_decay * narrative_weight;
    base_score * (0.25 + 0.75 * tag_sim)
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
            format!("{}: {}{:.0}", metric, sign, delta)
        })
        .collect()
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
    half_year: crate::llm::HalfYear,
) -> Result<(), String> {
    crate::application::cmd_get_narrative(state, db, app, half_year).await
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

/// Get tick explanation for debug mode
pub fn get_tick_explanation(state: &AppState) -> Result<crate::engine::TickExplanation, String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state")?;
    Ok(crate::engine::generate_tick_explanation(world_state, &state.event_log))
}

/// Get map configuration for current scenario
#[tauri::command]
pub async fn cmd_get_map_config(
    state: tauri::State<'_, tokio::sync::Mutex<AppState>>,
) -> Result<Option<crate::core::MapConfig>, String> {
    let state = state.lock().await;
    Ok(state.current_scenario.as_ref().and_then(|s| s.map.clone()))
}
