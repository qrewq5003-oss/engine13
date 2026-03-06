use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use tauri::Emitter;

use crate::core::{Actor, Event, Scenario, WorldState};
use crate::db::Db;
use crate::engine::{tick, EventLog};
use crate::scenarios::{load_rome_375, load_constantinople_1430};

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

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub api_key: Option<String>,
    pub model: String,
    pub base_url: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "lmstudio".to_string(),
            api_key: None,
            model: "local-model".to_string(),
            base_url: "http://localhost:1234".to_string(),
        }
    }
}

impl LlmConfig {
    pub fn default_base_url(provider: &str) -> String {
        match provider {
            "lmstudio" => "http://localhost:1234".to_string(),
            "ollama" => "http://localhost:11434".to_string(),
            "openai" => "https://api.openai.com".to_string(),
            "anthropic" => "https://api.anthropic.com".to_string(),
            "deepseek" => "https://api.deepseek.com".to_string(),
            "nanogpt" => "https://nano-gpt.com/api".to_string(),
            _ => "http://localhost:1234".to_string(),
        }
    }
}

/// LLM trigger type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    PlayerAction,
    ThresholdEvent,
    Time,
}

/// Action info for player_action trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    pub action_name: String,
    pub effects: HashMap<String, f64>,
    pub costs: HashMap<String, f64>,
}

/// Threshold context for threshold_event trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdContext {
    pub actor_id: String,
    pub actor_name: String,
    pub threshold_type: String, // "relevance_gained" | "metric_threshold" | "milestone"
    pub description: String,
}

/// LLM trigger response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTrigger {
    pub trigger_type: TriggerType,
    pub prompt: String,
    pub context: LlmContext,
    pub action_info: Option<ActionInfo>,           // for player_action
    pub threshold_context: Option<ThresholdContext>, // for threshold_event
    pub actor_deltas: Vec<crate::core::ActorDelta>, // for all triggers
}

/// Context for LLM generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmContext {
    pub current_year: i32,
    pub current_tick: u32,
    pub narrative_actors: Vec<String>,
    pub recent_events: Vec<String>,
    pub scenario_context: String,
    pub ticks_since_last: u32, // for time trigger
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
    // Track action info for trigger
    let mut action_info: Option<(crate::core::PatronAction, HashMap<String, f64>, HashMap<String, f64>)> = None;
    
    if let Some(action_input) = action {
        let (effects, costs) = apply_player_action(state, &action_input)?;
        
        // Get action details for trigger
        let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
        if let Some(action) = scenario.patron_actions.iter().find(|a| a.id == action_input.action_id) {
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

/// Get available actions for the player
pub fn get_available_actions(state: &AppState) -> Result<Vec<crate::core::PatronAction>, String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state")?;

    // In Consequences and Free modes, return universal actions only
    if world_state.game_mode == crate::core::GameMode::Consequences
        || world_state.game_mode == crate::core::GameMode::Free
    {
        return Ok(get_universal_actions(world_state));
    }

    // In Scenario mode, use scenario-specific actions
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    // Handle scenarios without player_actor (e.g., Constantinople 1430)
    if scenario.player_actor_id.is_none() {
        let available_actions = scenario
            .patron_actions
            .iter()
            .filter(|action| is_action_available_no_player(action, world_state))
            .cloned()
            .collect();
        return Ok(available_actions);
    }

    // Scenarios with player_actor (e.g., Rome 375)
    let player_actor_id = scenario.player_actor_id.as_ref().ok_or("Player actor not found")?;
    let player_actor = world_state.actors.get(player_actor_id).ok_or("Player actor not found")?;

    let available_actions = scenario
        .patron_actions
        .iter()
        .filter(|action| is_action_available(action, player_actor, &world_state.family_metrics))
        .cloned()
        .collect();

    Ok(available_actions)
}

/// Get universal actions available in Consequences and Free modes
fn get_universal_actions(_world_state: &WorldState) -> Vec<crate::core::PatronAction> {
    use crate::core::{PatronAction, ActionCondition, ComparisonOperator};
    use std::collections::HashMap;
    
    let mut actions = Vec::new();
    
    // 1. Observe - always available, no effects, no cost
    actions.push(PatronAction {
        id: "observe".to_string(),
        name: "Наблюдать".to_string(),
        available_if: ActionCondition::Always,
        effects: HashMap::new(),
        cost: HashMap::new(),
    });
    
    // 2. Support Stability - requires treasury > 50
    // Effects: cohesion +3, legitimacy +2
    // Cost: treasury -50
    let mut support_effects = HashMap::new();
    support_effects.insert("family_cohesion".to_string(), 3.0);
    support_effects.insert("family_legitimacy".to_string(), 2.0);
    let mut support_cost = HashMap::new();
    support_cost.insert("treasury".to_string(), -50.0);

    actions.push(PatronAction {
        id: "support_stability".to_string(),
        name: "Поддержать стабильность".to_string(),
        available_if: ActionCondition::Metric {
            metric: "treasury".to_string(),
            operator: ComparisonOperator::Greater,
            value: 50.0,
        },
        effects: support_effects,
        cost: support_cost,
    });

    // 3. Raise Taxes - always available
    // Effects: treasury +80
    // Side effects: legitimacy -5, cohesion -3
    let mut taxes_effects = HashMap::new();
    taxes_effects.insert("treasury".to_string(), 80.0);
    taxes_effects.insert("family_cohesion".to_string(), -3.0);
    taxes_effects.insert("family_legitimacy".to_string(), -5.0);

    actions.push(PatronAction {
        id: "raise_taxes".to_string(),
        name: "Повысить налоги".to_string(),
        available_if: ActionCondition::Always,
        effects: taxes_effects,
        cost: HashMap::new(),
    });

    // 4. Recruit Soldiers - requires treasury > 100
    // Effects: military_size +10, military_quality -5
    // Cost: treasury -100
    let mut recruit_effects = HashMap::new();
    recruit_effects.insert("rome.military_size".to_string(), 10.0);
    recruit_effects.insert("rome.military_quality".to_string(), -5.0);
    let mut recruit_cost = HashMap::new();
    recruit_cost.insert("treasury".to_string(), -100.0);

    actions.push(PatronAction {
        id: "recruit_soldiers".to_string(),
        name: "Нанять солдат".to_string(),
        available_if: ActionCondition::Metric {
            metric: "treasury".to_string(),
            operator: ComparisonOperator::Greater,
            value: 100.0,
        },
        effects: recruit_effects,
        cost: recruit_cost,
    });
    
    actions
}

/// Submit a player action
pub fn submit_action(state: &mut AppState, action_id: String) -> Result<SubmitActionResponse, String> {
    let action_input = PlayerActionInput { action_id: action_id.clone(), target_actor_id: None };
    let (effects, costs) = apply_player_action(state, &action_input)?;

    // Get action details for trigger
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
    let action = scenario.patron_actions.iter().find(|a| a.id == action_id)
        .ok_or_else(|| format!("Action '{}' not found", action_id))?;
    let action_info = (action.clone(), effects.clone(), costs.clone());

    let world_state = state.world_state.as_mut().ok_or("No active world state")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    tick(world_state, scenario, &mut state.event_log);

    let scenario_clone = scenario.clone();
    let event_log_clone = state.event_log.clone();

    // Pass action info to trigger check - this will reset ticks_since_last_narrative if trigger fires
    let llm_trigger = check_llm_trigger_with_data(world_state, &scenario_clone, &event_log_clone, Some((&action_info.0, &action_info.1, &action_info.2)));

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
pub fn save_game(
    state: &mut AppState,
    db: &Db,
    slot: Option<String>,
    name: Option<String>,
) -> Result<SaveResponse, String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state to save")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    let save_id = slot.unwrap_or_else(|| format!("autosave_{}", world_state.tick));
    let save_name = name.unwrap_or_else(|| format!("Tick {} - Year {}", world_state.tick, world_state.year));

    // Serialize world_state to JSON
    let world_state_json = serde_json::to_string(&world_state)
        .map_err(|e| format!("Failed to serialize world state: {}", e))?;

    // Create player_state (for now just family_metrics, can be extended)
    let player_state_json = serde_json::to_string(&world_state.family_metrics)
        .map_err(|e| format!("Failed to serialize player state: {}", e))?;

    let db_save = crate::db::DbSave {
        id: save_id.clone(),
        name: save_name,
        scenario_id: scenario.id.clone(),
        tick: world_state.tick,
        year: world_state.year,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        world_state_json,
        player_state_json,
    };

    db.insert_save(&db_save)
        .map_err(|e| format!("Failed to insert save: {}", e))?;

    Ok(SaveResponse { success: true, save_id: Some(save_id), error: None })
}

/// Load game from save
pub fn load_game(
    state: &mut AppState,
    db: &Db,
    save_id: String,
) -> Result<LoadResponse, String> {
    let db_save = db.get_save_by_id(&save_id)
        .map_err(|e| format!("Failed to get save: {}", e))?
        .ok_or_else(|| format!("Save '{}' not found", save_id))?;

    // Deserialize world_state from JSON
    let world_state: WorldState = serde_json::from_str(&db_save.world_state_json)
        .map_err(|e| format!("Failed to deserialize world state: {}", e))?;

    // Deserialize player_state (family_metrics)
    let family_metrics: HashMap<String, f64> = serde_json::from_str(&db_save.player_state_json)
        .unwrap_or_else(|_| HashMap::new());

    state.world_state = Some(world_state.clone());
    state.event_log = EventLog::new();

    // Update family_metrics in world_state
    if let Some(ref mut ws) = state.world_state {
        ws.family_metrics = family_metrics;
    }

    if state.current_scenario.as_ref().map(|s| s.id.clone()) != Some(db_save.scenario_id.clone()) {
        match db_save.scenario_id.as_str() {
            "rome_375" => { state.current_scenario = Some(load_rome_375()); }
            _ => return Err(format!("Unknown scenario: {}", db_save.scenario_id)),
        }
    }

    Ok(LoadResponse { success: true, world_state: Some(world_state), error: None })
}

/// List all saves
pub fn list_saves(db: &Db) -> Vec<SaveData> {
    match db.list_saves() {
        Ok(db_saves) => {
            db_saves.iter().map(|db_save| SaveData {
                id: db_save.id.clone(),
                name: db_save.name.clone(),
                scenario_id: db_save.scenario_id.clone(),
                tick: db_save.tick,
                year: db_save.year,
                created_at: db_save.created_at,
                world_state: serde_json::from_str(&db_save.world_state_json).unwrap_or_else(|_| {
                    WorldState::new(db_save.scenario_id.clone(), db_save.year)
                }),
                event_log: vec![],
            }).collect()
        }
        Err(e) => {
            eprintln!("Failed to list saves: {}", e);
            vec![]
        }
    }
}

/// Get relevant events
pub fn get_relevant_events(db: &Db, actor_id: String) -> Result<Vec<Event>, String> {
    // For now, just get events for the actor (relevance scoring is next step)
    db.get_events_by_actor(&actor_id)
}

/// Set game mode - for manual transition from Consequences to Free
/// Scenario → Consequences is automatic only (via milestone with triggers_collapse)
/// Consequences → Free is manual (via this function)
/// Free → any is not allowed (one-way transition)
pub fn set_game_mode(
    state: &mut AppState,
    new_mode: crate::core::GameMode,
) -> Result<(), String> {
    let world_state = state.world_state.as_mut().ok_or("No active world state")?;
    let current_mode = world_state.game_mode;
    
    // Validate transitions
    match (current_mode, new_mode) {
        // Scenario → Consequences: automatic only, not allowed here
        (crate::core::GameMode::Scenario, _) => {
            return Err("Переход из Scenario возможен только автоматически при срабатывании milestone события".to_string());
        }
        // Consequences → Free: allowed
        (crate::core::GameMode::Consequences, crate::core::GameMode::Free) => {
            world_state.game_mode = crate::core::GameMode::Free;
            eprintln!("[GAME_MODE] Manual transition from Consequences to Free at tick {}", world_state.tick);
            Ok(())
        }
        // Any other transition: not allowed
        _ => {
            Err(format!("Недопустимый переход из {:?} в {:?}", current_mode, new_mode))
        }
    }
}

/// Load a scenario
pub fn load_scenario(state: &mut AppState, scenario_id: String) -> Result<SaveResponse, String> {
    let scenario = match scenario_id.as_str() {
        "rome_375" => load_rome_375(),
        "constantinople_1430" => load_constantinople_1430(),
        _ => return Err(format!("Unknown scenario: {}", scenario_id)),
    };

    let mut world_state = WorldState::new(scenario.id.clone(), scenario.start_year);
    // Only add actors that are not successor templates
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world_state.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    // Initialize family_metrics from player actor's scenario_metrics
    if let Some(ref player_actor_id) = scenario.player_actor_id {
        if let Some(player_actor) = world_state.actors.get(player_actor_id) {
            for (key, value) in &player_actor.scenario_metrics {
                if key.starts_with("family_") {
                    world_state.family_metrics.insert(key.clone(), *value);
                }
            }
        }
    }

    // Initialize patriarch_age from generation_mechanics if available
    if let Some(gen_mechanics) = &scenario.generation_mechanics {
        world_state.family_metrics.insert(
            "patriarch_age".to_string(),
            gen_mechanics.patriarch_start_age as f64,
        );
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
        ScenarioMeta {
            id: "constantinople_1430".to_string(),
            label: "Constantinople 1430 — Федерация".to_string(),
            description: "1430 год. Фессалоники пали. Константинополь стоит — но ненадолго.".to_string(),
            start_year: 1430,
        },
    ]
}

/// Get LLM config from ~/.config/engine13/config.json
pub fn get_llm_config() -> LlmConfig {
    let config_path: Option<std::path::PathBuf> = dirs::home_dir()
        .map(|mut p: std::path::PathBuf| {
            p.push(".config");
            p.push("engine13");
            p.push("config.json");
            p
        });

    if let Some(path) = config_path {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<LlmConfig>(&content) {
                    return config;
                }
            }
        }
    }

    LlmConfig::default()
}

/// Generate narrative prompt from world state and scenario
pub fn generate_narrative_prompt(
    world_state: &WorldState,
    scenario: &Scenario,
    event_log: &EventLog,
    db: &Db,
) -> String {
    let mut prompt = String::new();
    
    // Section 1: Scenario context (depends on game mode)
    match world_state.game_mode {
        crate::core::GameMode::Consequences => {
            // Consequences mode: use consequence_context
            prompt.push_str(&scenario.consequence_context);
            prompt.push_str("\n\n");
        }
        crate::core::GameMode::Free => {
            // Free mode: no scenario context at all, just world state
            // Don't add any scenario context
        }
        _ => {
            // Scenario mode (default): use llm_context
            prompt.push_str(&scenario.llm_context);
            prompt.push_str("\n\n");
        }
    }

    // Section 2: World state - foreground actors only
    prompt.push_str("=== СОСТОЯНИЕ МИРА ===\n");
    prompt.push_str(&format!("Год: {} (тик {})\n\n", world_state.year, world_state.tick));

    let foreground_actors: Vec<_> = world_state
        .actors
        .values()
        .filter(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground)
        .collect();

    for actor in &foreground_actors {
        prompt.push_str(&format!(
            "{} ({}):\n",
            actor.name, actor.name_short
        ));
        prompt.push_str(&format!(
            "  population: {:.0}, military: {:.0}, quality: {:.0}\n",
            actor.metrics.population,
            actor.metrics.military_size,
            actor.metrics.military_quality
        ));
        prompt.push_str(&format!(
            "  economy: {:.0}, cohesion: {:.0}, legitimacy: {:.0}, pressure: {:.0}\n",
            actor.metrics.economic_output,
            actor.metrics.cohesion,
            actor.metrics.legitimacy,
            actor.metrics.external_pressure
        ));
        prompt.push_str(&format!(
            "  treasury: {:.0}\n",
            actor.metrics.treasury
        ));
        if !actor.tags.is_empty() {
            prompt.push_str(&format!("  tags: {}\n", actor.tags.join(", ")));
        }
        prompt.push('\n');
    }

    // Section 3: Recent events with relevance scoring
    prompt.push_str("=== ПОСЛЕДНИЕ СОБЫТИЯ ===\n");
    
    // Build query tags from current context
    let mut query_tags: Vec<String> = Vec::new();
    
    // Add narrative actor names (short) and regions
    for actor in &foreground_actors {
        query_tags.push(actor.name_short.clone());
        query_tags.push(actor.region.clone());
    }
    
    // Add interaction types from recent events
    let recent_event_types: HashSet<String> = event_log.events.iter()
        .filter(|e| e.is_key || foreground_actors.iter().any(|a| a.id == e.actor_id))
        .flat_map(|e| {
            match e.event_type {
                crate::core::EventType::War => Some("war".to_string()),
                crate::core::EventType::Migration => Some("migration".to_string()),
                crate::core::EventType::Trade => Some("trade".to_string()),
                _ => None,
            }
        })
        .collect();
    query_tags.extend(recent_event_types);
    
    // Get scored relevant events from database
    let narrative_actor_ids: Vec<String> = foreground_actors.iter().map(|a| a.id.clone()).collect();
    
    let relevant_events = db.get_relevant_events_scored(
        world_state.tick,
        &query_tags,
        &narrative_actor_ids,
    );
    
    let events_to_show = match relevant_events {
        Ok(events) => {
            eprintln!("[NARRATIVE] Got {} relevant events from DB", events.len());
            events
        }
        Err(e) => {
            eprintln!("[NARRATIVE] Failed to get relevant events from DB: {}", e);
            // Fallback to simple event_log query
            event_log.events.iter()
                .filter(|e| {
                    e.is_key || narrative_actor_ids.contains(&e.actor_id)
                })
                .cloned()
                .collect()
        }
    };

    if events_to_show.is_empty() {
        prompt.push_str("Нет недавних событий.\n");
    } else {
        for event in &events_to_show {
            // Calculate score for logging
            let ticks_ago = world_state.tick.saturating_sub(event.tick);
            let temporal_coeff = Db::temporal_coefficient(ticks_ago, event.is_key);
            let thematic_sim = Db::thematic_similarity(&event.tags, &query_tags);
            let score = temporal_coeff * thematic_sim;
            
            prompt.push_str(&format!(
                "{} (тик {}): {} [score: {:.2}]\n",
                event.year, event.tick, event.description, score
            ));
        }
    }

    prompt
}

/// Get narrative from LLM with streaming
pub async fn cmd_get_narrative(
    state: &AppState,
    db: &Db,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
    let config = get_llm_config();
    let prompt = generate_narrative_prompt(world_state, scenario, &state.event_log, db);

    // Generate placeholder narrative for when LLM is unavailable
    let placeholder = format!("Медиолан, {} год. Семья наблюдает за судьбой Империи.", world_state.year);

    eprintln!("[NARRATIVE] Getting narrative for year {}", world_state.year);
    eprintln!("[NARRATIVE] Provider: {}, URL: {}, Model: {}", config.provider, config.base_url, config.model);

    if config.provider == "anthropic" {
        // Anthropic format - streaming
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Client build failed: {}", e))?;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "x-api-key",
            config.api_key.unwrap_or_default().parse().map_err(|e| format!("Invalid API key: {}", e))?,
        );
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());

        let body = serde_json::json!({
            "model": config.model,
            "max_tokens": 3000,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "stream": true
        });

        let url = format!("{}/v1/messages", config.base_url);
        let res = client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await;

        // Handle connection errors gracefully - emit placeholder
        let res = match res {
            Ok(r) => r,
            Err(_) => {
                eprintln!("[NARRATIVE] Connection failed, emitting placeholder");
                let _ = app.emit("narrative_chunk", placeholder.clone());
                let _ = app.emit("narrative_done", "");
                return Ok(());
            }
        };

        if !res.status().is_success() {
            let status = res.status();
            let error_body = res.text().await.unwrap_or_default();
            return Err(format!("API error ({}): {}", status, error_body));
        }

        // Stream SSE response
        let mut stream = res.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk_result) = stream.next().await {
            let chunk: bytes::Bytes = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[NARRATIVE] Stream error: {}", e);
                    break;
                }
            };

            if let Ok(text) = std::str::from_utf8(&chunk) {
                // Parse SSE data lines
                for line in text.lines() {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" {
                            break;
                        }
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(content) = json["content"][0]["text"].as_str() {
                                let _ = app.emit("narrative_chunk", content.to_string());
                            }
                        }
                    }
                }
            }
        }
    } else {
        // OpenAI-compatible format - streaming
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Client build failed: {}", e))?;
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(api_key) = &config.api_key {
            headers.insert(
                "Authorization",
                format!("Bearer {}", api_key).parse().map_err(|e| format!("Invalid API key: {}", e))?,
            );
        }

        let body = serde_json::json!({
            "model": config.model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": 3000,
            "stream": true
        });

        let url = format!("{}/v1/chat/completions", config.base_url);
        let res = client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await;

        // Handle connection errors gracefully - emit placeholder
        let res = match res {
            Ok(r) => r,
            Err(_) => {
                eprintln!("[NARRATIVE] Connection failed, emitting placeholder");
                let _ = app.emit("narrative_chunk", placeholder.clone());
                let _ = app.emit("narrative_done", "");
                return Ok(());
            }
        };

        if !res.status().is_success() {
            let status = res.status();
            let error_body = res.text().await.unwrap_or_default();
            return Err(format!("API error ({}): {}", status, error_body));
        }

        // Stream SSE response
        let mut stream = res.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk_result) = stream.next().await {
            let chunk: bytes::Bytes = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[NARRATIVE] Stream error: {}", e);
                    break;
                }
            };

            if let Ok(text) = std::str::from_utf8(&chunk) {
                // Parse SSE data lines
                for line in text.lines() {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" {
                            break;
                        }
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                                let _ = app.emit("narrative_chunk", content.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    eprintln!("[NARRATIVE] Streaming complete");
    let _ = app.emit("narrative_done", "");
    Ok(())
}

/// Stream narrative from Anthropic API
pub async fn stream_narrative_anthropic(
    prompt: String,
    placeholder: String,
    config: LlmConfig,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Client build failed: {}", e))?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-api-key",
        config.api_key.unwrap_or_default().parse().map_err(|e| format!("Invalid API key: {}", e))?,
    );
    headers.insert("anthropic-version", "2023-06-01".parse().unwrap());

    let body = serde_json::json!({
        "model": config.model,
        "max_tokens": 3000,
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ],
        "stream": true
    });

    let url = format!("{}/v1/messages", config.base_url);
    let res = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await;

    let res = match res {
        Ok(r) => r,
        Err(_) => {
            eprintln!("[NARRATIVE] Connection failed, emitting placeholder");
            let _ = app.emit("narrative_chunk", placeholder.clone());
            let _ = app.emit("narrative_done", "");
            return Ok(());
        }
    };

    if !res.status().is_success() {
        let status = res.status();
        let error_body = res.text().await.unwrap_or_default();
        return Err(format!("API error ({}): {}", status, error_body));
    }

    let mut stream = res.bytes_stream();
    use futures_util::StreamExt;

    while let Some(chunk_result) = stream.next().await {
        let chunk: bytes::Bytes = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[NARRATIVE] Stream error: {}", e);
                break;
            }
        };

        if let Ok(text) = std::str::from_utf8(&chunk) {
            for line in text.lines() {
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" {
                        break;
                    }
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(content) = json["content"][0]["text"].as_str() {
                            let _ = app.emit("narrative_chunk", content.to_string());
                        }
                    }
                }
            }
        }
    }

    eprintln!("[NARRATIVE] Streaming complete");
    let _ = app.emit("narrative_done", "");
    Ok(())
}

/// Stream narrative from OpenAI-compatible API
pub async fn stream_narrative_openai(
    prompt: String,
    placeholder: String,
    config: LlmConfig,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Client build failed: {}", e))?;
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(api_key) = &config.api_key {
        headers.insert(
            "Authorization",
            format!("Bearer {}", api_key).parse().map_err(|e| format!("Invalid API key: {}", e))?,
        );
    }

    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ],
        "max_tokens": 3000,
        "stream": true
    });

    let url = format!("{}/v1/chat/completions", config.base_url);
    let res = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await;

    let res = match res {
        Ok(r) => r,
        Err(_) => {
            eprintln!("[NARRATIVE] Connection failed, emitting placeholder");
            let _ = app.emit("narrative_chunk", placeholder.clone());
            let _ = app.emit("narrative_done", "");
            return Ok(());
        }
    };

    if !res.status().is_success() {
        let status = res.status();
        let error_body = res.text().await.unwrap_or_default();
        return Err(format!("API error ({}): {}", status, error_body));
    }

    let mut stream = res.bytes_stream();
    use futures_util::StreamExt;

    while let Some(chunk_result) = stream.next().await {
        let chunk: bytes::Bytes = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[NARRATIVE] Stream error: {}", e);
                break;
            }
        };

        if let Ok(text) = std::str::from_utf8(&chunk) {
            for line in text.lines() {
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" {
                        break;
                    }
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                            let _ = app.emit("narrative_chunk", content.to_string());
                        }
                    }
                }
            }
        }
    }

    eprintln!("[NARRATIVE] Streaming complete");
    let _ = app.emit("narrative_done", "");
    Ok(())
}

/// Get available models from LLM provider
pub fn cmd_get_available_models(provider: String, base_url: String, api_key: Option<String>) -> Result<Vec<String>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client build failed: {}", e))?;
    let mut headers = reqwest::header::HeaderMap::new();

    if provider == "anthropic" {
        headers.insert(
            "x-api-key",
            api_key.unwrap_or_default().parse().map_err(|e| format!("Invalid API key: {}", e))?,
        );
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
    } else if let Some(api_key) = &api_key {
        headers.insert(
            "Authorization",
            format!("Bearer {}", api_key).parse().map_err(|e| format!("Invalid API key: {}", e))?,
        );
    }

    let url = format!("{}/v1/models", base_url);

    let res = client
        .get(&url)
        .headers(headers)
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let error_body = res.text().unwrap_or_default();
        return Err(format!("API error ({}): {}", status, error_body));
    }

    let json: serde_json::Value = res
        .json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Parse models list - handle both OpenAI and Anthropic formats
    let models: Vec<String> = if let Some(data_arr) = json.get("data").and_then(|v: &serde_json::Value| v.as_array()) {
        // OpenAI format: { "data": [{ "id": "model-name", ... }, ...] }
        let data: &Vec<serde_json::Value> = data_arr;
        data.iter()
            .filter_map(|item: &serde_json::Value| item.get("id").and_then(|v: &serde_json::Value| v.as_str()).map(|s: &str| s.to_string()))
            .collect()
    } else if let Some(models_arr) = json.get("models").and_then(|v: &serde_json::Value| v.as_array()) {
        // Alternative format: { "models": [{ "name": "model-name", ... }, ...] }
        let models_array: &Vec<serde_json::Value> = models_arr;
        models_array.iter()
            .filter_map(|item: &serde_json::Value| item.get("name").and_then(|v: &serde_json::Value| v.as_str()).map(|s: &str| s.to_string()))
            .collect()
    } else {
        vec![]
    };

    Ok(models)
}

/// Save LLM config to ~/.config/engine13/config.json
pub fn cmd_save_llm_config(provider: String, base_url: String, api_key: Option<String>, model: String) -> Result<(), String> {
    let config_dir = dirs::home_dir()
        .map(|mut p: std::path::PathBuf| {
            p.push(".config");
            p.push("engine13");
            p
        })
        .ok_or("Could not determine home directory")?;

    // Create directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    let config_path = config_dir.join("config.json");

    let config = LlmConfig {
        provider,
        api_key,
        model,
        base_url,
    };

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&config_path, json)
        .map_err(|e| format!("Failed to write config file: {}", e))?;

    eprintln!("[DEBUG] Config saved to {:?}", config_path);
    Ok(())
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

    if !is_action_available(action, player_actor, &world_state.family_metrics) {
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

fn is_action_available(action: &crate::core::PatronAction, player_actor: &Actor, family_metrics: &HashMap<String, f64>) -> bool {
    match &action.available_if {
        crate::core::ActionCondition::Always => true,
        crate::core::ActionCondition::Metric { metric, operator, value } => {
            let current = if metric.starts_with("family_") {
                family_metrics.get(metric).copied().unwrap_or(0.0)
            } else {
                get_metric_value(&player_actor.metrics, metric)
            };
            compare_value(current, operator, value)
        }
    }
}

/// Check if action is available for scenarios without player_actor (e.g., Constantinople 1430)
/// Metric format:
/// - "family_*" - from world_state.family_metrics
/// - "actor_id.metric" (e.g., "venice.treasury") - from world_state.actors.get(actor_id).metrics
/// - Other (e.g., "federation_progress") - from world_state.global_metrics
fn is_action_available_no_player(action: &crate::core::PatronAction, world_state: &WorldState) -> bool {
    match &action.available_if {
        crate::core::ActionCondition::Always => true,
        crate::core::ActionCondition::Metric { metric, operator, value } => {
            let current = get_metric_value_no_player(metric, world_state);
            compare_value(current, operator, value)
        }
    }
}

/// Get metric value for scenarios without player_actor
fn get_metric_value_no_player(metric: &str, world_state: &WorldState) -> f64 {
    if metric.starts_with("family_") {
        // Family metrics
        world_state.family_metrics.get(metric).copied().unwrap_or(0.0)
    } else if metric.contains('.') {
        // Actor-specific metric: "actor_id.metric" (e.g., "venice.treasury")
        let parts: Vec<&str> = metric.splitn(2, '.').collect();
        if parts.len() == 2 {
            let actor_id = parts[0];
            let metric_name = parts[1];
            world_state.actors.get(actor_id)
                .map(|actor| get_metric_value(&actor.metrics, metric_name))
                .unwrap_or(0.0)
        } else {
            0.0
        }
    } else {
        // Global metric (e.g., "federation_progress")
        world_state.global_metrics.get(metric).copied().unwrap_or(0.0)
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

fn check_llm_trigger_with_data(
    world_state: &mut WorldState,
    scenario: &Scenario,
    event_log: &EventLog,
    action_info: Option<(&crate::core::PatronAction, &HashMap<String, f64>, &HashMap<String, f64>)>, // (action, effects, costs)
) -> Option<LlmTrigger> {
    use crate::engine::calculate_actor_deltas;

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

    // Calculate actor deltas
    let actor_deltas = calculate_actor_deltas(world_state);

    // Priority 1: Check for player action trigger
    if let Some((action, effects, costs)) = action_info {
        let ticks_since_last = world_state.ticks_since_last_narrative;
        world_state.ticks_since_last_narrative = 0; // Reset counter
        
        return Some(LlmTrigger {
            trigger_type: TriggerType::PlayerAction,
            prompt: generate_llm_prompt_with_trigger(
                world_state,
                scenario,
                &narrative_actor_ids,
                &recent_events,
                &actor_deltas,
                ticks_since_last,
                Some(&TriggerDetail::PlayerAction {
                    action_name: action.name.clone(),
                    effects: effects.clone(),
                    costs: costs.clone(),
                }),
            ),
            context: LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids,
                recent_events,
                scenario_context: scenario.llm_context.clone(),
                ticks_since_last: ticks_since_last,
            },
            action_info: Some(ActionInfo {
                action_name: action.name.clone(),
                effects: effects.clone(),
                costs: costs.clone(),
            }),
            threshold_context: None,
            actor_deltas,
        });
    }

    // Priority 2: Check for threshold event trigger (relevance gain or milestone)
    let threshold_event = event_log.events.iter().find(|e| {
        e.event_type == crate::core::EventType::Threshold || e.event_type == crate::core::EventType::Milestone
    });

    if let Some(event) = threshold_event {
        let ticks_since_last = world_state.ticks_since_last_narrative;
        world_state.ticks_since_last_narrative = 0; // Reset counter
        
        // Determine threshold type
        let threshold_type = if event.event_type == crate::core::EventType::Milestone {
            "milestone".to_string()
        } else if event.description.contains("передний план") || event.description.contains("foreground") {
            "relevance_gained".to_string()
        } else {
            "metric_threshold".to_string()
        };

        // Extract actor info from event
        let (actor_id, actor_name) = world_state.actors.get(&event.actor_id)
            .map(|a| (a.id.clone(), a.name.clone()))
            .unwrap_or_else(|| (event.actor_id.clone(), event.actor_id.clone()));

        return Some(LlmTrigger {
            trigger_type: TriggerType::ThresholdEvent,
            prompt: generate_llm_prompt_with_trigger(
                world_state,
                scenario,
                &narrative_actor_ids,
                &recent_events,
                &actor_deltas,
                ticks_since_last,
                Some(&TriggerDetail::ThresholdEvent {
                    actor_id: actor_id.clone(),
                    actor_name: actor_name.clone(),
                    threshold_type: threshold_type.clone(),
                    description: event.description.clone(),
                }),
            ),
            context: LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids.clone(),
                recent_events,
                scenario_context: scenario.llm_context.clone(),
                ticks_since_last: ticks_since_last,
            },
            action_info: None,
            threshold_context: Some(ThresholdContext {
                actor_id,
                actor_name,
                threshold_type,
                description: event.description.clone(),
            }),
            actor_deltas,
        });
    }

    // Priority 3: Check for time-based trigger (every 5 ticks)
    if world_state.ticks_since_last_narrative >= 5 {
        let ticks_since_last = world_state.ticks_since_last_narrative;
        world_state.ticks_since_last_narrative = 0; // Reset counter
        
        return Some(LlmTrigger {
            trigger_type: TriggerType::Time,
            prompt: generate_llm_prompt_with_trigger(
                world_state,
                scenario,
                &narrative_actor_ids,
                &recent_events,
                &actor_deltas,
                ticks_since_last,
                Some(&TriggerDetail::Time),
            ),
            context: LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids,
                recent_events,
                scenario_context: scenario.llm_context.clone(),
                ticks_since_last: ticks_since_last,
            },
            action_info: None,
            threshold_context: None,
            actor_deltas,
        });
    }

    None
}

/// Detail for trigger-specific prompt generation
enum TriggerDetail {
    PlayerAction {
        action_name: String,
        effects: HashMap<String, f64>,
        costs: HashMap<String, f64>,
    },
    ThresholdEvent {
        actor_id: String,
        actor_name: String,
        threshold_type: String,
        description: String,
    },
    Time,
}

/// Generate LLM prompt with trigger-specific sections
fn generate_llm_prompt_with_trigger(
    world_state: &WorldState,
    scenario: &Scenario,
    narrative_actor_ids: &[String],
    recent_events: &[String],
    actor_deltas: &[crate::core::ActorDelta],
    ticks_since_last: u32,
    trigger_detail: Option<&TriggerDetail>,
) -> String {
    let mut prompt = String::new();

    // Section 1: Scenario context
    prompt.push_str(&scenario.llm_context);
    prompt.push_str("\n\n");

    // Section 2: World state
    prompt.push_str("=== СОСТОЯНИЕ МИРА ===\n");
    prompt.push_str(&format!("Год: {} (тик {})\n\n", world_state.year, world_state.tick));

    for actor_id in narrative_actor_ids {
        if let Some(actor) = world_state.actors.get(actor_id) {
            prompt.push_str(&format!(
                "{} ({}):\n",
                actor.name, actor.name_short
            ));
            prompt.push_str(&format!(
                "  population: {:.0}k, military: {:.0}k, quality: {:.0}\n",
                actor.metrics.population / 1000.0,
                actor.metrics.military_size / 1000.0,
                actor.metrics.military_quality
            ));
            prompt.push_str(&format!(
                "  economy: {:.0}, cohesion: {:.0}, legitimacy: {:.0}, pressure: {:.0}\n",
                actor.metrics.economic_output,
                actor.metrics.cohesion,
                actor.metrics.legitimacy,
                actor.metrics.external_pressure
            ));
            prompt.push_str(&format!(
                "  treasury: {:.0}\n",
                actor.metrics.treasury
            ));
            prompt.push('\n');
        }
    }

    // Section 3: Recent events
    prompt.push_str("=== ПОСЛЕДНИЕ СОБЫТИЯ ===\n");
    if recent_events.is_empty() {
        prompt.push_str("Нет недавних событий.\n");
    } else {
        for event in recent_events {
            prompt.push_str(&format!("{}\n", event));
        }
    }
    prompt.push('\n');

    // Section 4: Actor deltas (changes this tick)
    if !actor_deltas.is_empty() {
        prompt.push_str("=== ИЗМЕНЕНИЯ ЗА ТИК ===\n");
        for delta in actor_deltas {
            if !delta.metric_changes.is_empty() {
                prompt.push_str(&format!("{}:\n", delta.actor_name));
                for (metric, change) in &delta.metric_changes {
                    let sign = if *change > 0.0 { "+" } else { "" };
                    prompt.push_str(&format!("  {}: {}{:.1}\n", metric, sign, change));
                }
            }
        }
        prompt.push('\n');
    }

    // Section 5: Trigger-specific section
    prompt.push_str("=== ТРИГГЕР ===\n");
    match trigger_detail {
        Some(TriggerDetail::PlayerAction { action_name, effects, costs }) => {
            prompt.push_str("ДЕЙСТВИЕ ИГРОКА:\n");
            prompt.push_str(&format!("Название: {}\n", action_name));
            prompt.push_str("Эффекты:\n");
            for (metric, value) in effects {
                let sign = if *value > 0.0 { "+" } else { "" };
                prompt.push_str(&format!("  {}: {}{:.1}\n", metric, sign, value));
            }
            if !costs.is_empty() {
                prompt.push_str("Стоимость:\n");
                for (metric, value) in costs {
                    prompt.push_str(&format!("  {}: {:.1}\n", metric, value));
                }
            }
        }
        Some(TriggerDetail::ThresholdEvent { actor_id, actor_name, threshold_type, description }) => {
            prompt.push_str("ПОРОГОВОЕ СОБЫТИЕ:\n");
            prompt.push_str(&format!("Актор: {} (ID: {})\n", actor_name, actor_id));
            prompt.push_str(&format!("Тип: {}\n", threshold_type));
            prompt.push_str(&format!("Описание: {}\n", description));
        }
        Some(TriggerDetail::Time) => {
            prompt.push_str("ВРЕМЕННОЙ СРЕЗ:\n");
            prompt.push_str(&format!("Тиков с последнего нарратива: {}\n", ticks_since_last));
            prompt.push_str("Суммарные изменения за период:\n");
            for delta in actor_deltas {
                if !delta.metric_changes.is_empty() {
                    prompt.push_str(&format!("  {}:\n", delta.actor_name));
                    for (metric, change) in &delta.metric_changes {
                        let sign = if *change > 0.0 { "+" } else { "" };
                        prompt.push_str(&format!("    {}: {}{:.1}\n", metric, sign, change));
                    }
                }
            }
        }
        None => {
            prompt.push_str("Нет специфического триггера.\n");
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
