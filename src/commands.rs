use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::Emitter;

use crate::core::{Event, Scenario, WorldState};
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
// Save Slot Types
// ============================================================================

/// Save data with slot information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSlotData {
    pub id: String,
    pub name: String,
    pub scenario_id: String,
    pub tick: u32,
    pub year: i32,
    pub created_at: u64,
    pub slot: String, // "auto" | "slot_1" | "slot_2" | "slot_3"
}

/// Response from list_saves - grouped by slots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSlotList {
    pub auto: Option<SaveSlotData>,
    pub slots: [Option<SaveSlotData>; 3],
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

    // Unified action filtering - works for all scenarios via MetricRef
    let available_actions = scenario
        .patron_actions
        .iter()
        .filter(|action| is_action_available(action, world_state))
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

/// Submit a player action - applies effects/costs WITHOUT advancing tick
pub fn submit_action(state: &mut AppState, action_id: String) -> Result<SubmitActionResponse, String> {
    let action_input = PlayerActionInput { action_id: action_id.clone(), target_actor_id: None };
    let (effects, costs) = apply_player_action(state, &action_input)?;

    // Note: We do NOT call tick() here - action application is separate from time advancement
    let world_state = state.world_state.as_ref().ok_or("No active world state")?;

    Ok(SubmitActionResponse {
        success: true,
        effects,
        costs,
        new_state: world_state.clone(),
        llm_trigger: None,
        error: None,
    })
}

/// Save current game state
/// slot: "auto" | "slot_1" | "slot_2" | "slot_3"
/// Save ID format: "{scenario_id}_{slot}" - one save per slot per scenario
pub fn save_game(
    state: &mut AppState,
    db: &Db,
    slot: Option<String>,
    _name: Option<String>,
) -> Result<SaveResponse, String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state to save")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    // Determine slot name
    let slot_name = slot.unwrap_or_else(|| "auto".to_string());
    
    // Validate slot name
    if !["auto", "slot_1", "slot_2", "slot_3"].contains(&slot_name.as_str()) {
        return Err(format!("Invalid slot: {}. Must be 'auto', 'slot_1', 'slot_2', or 'slot_3'", slot_name));
    }

    // Save ID format: "{scenario_id}_{slot}" - ensures one save per slot per scenario
    let save_id = format!("{}_{}", scenario.id, slot_name);
    let save_name = format!("Tick {} - Year {}", world_state.tick, world_state.year);

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
            "constantinople_1430" => { state.current_scenario = Some(load_constantinople_1430()); }
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

/// List saves grouped by slots for a specific scenario
pub fn list_saves_with_slots(db: &Db, scenario_id: &str) -> Result<SaveSlotList, String> {
    let db_saves = db.list_saves_by_scenario(scenario_id)?;
    
    let mut auto: Option<SaveSlotData> = None;
    let mut slots: [Option<SaveSlotData>; 3] = [None, None, None];
    
    for db_save in db_saves {
        // Extract slot from save_id format: "{scenario_id}_{slot}"
        let slot = if db_save.id.starts_with(&format!("{}_auto", scenario_id)) {
            "auto".to_string()
        } else if db_save.id.starts_with(&format!("{}_slot_1", scenario_id)) {
            "slot_1".to_string()
        } else if db_save.id.starts_with(&format!("{}_slot_2", scenario_id)) {
            "slot_2".to_string()
        } else if db_save.id.starts_with(&format!("{}_slot_3", scenario_id)) {
            "slot_3".to_string()
        } else {
            continue; // Skip saves with invalid format
        };
        
        let save_data = SaveSlotData {
            id: db_save.id.clone(),
            name: db_save.name.clone(),
            scenario_id: db_save.scenario_id.clone(),
            tick: db_save.tick,
            year: db_save.year,
            created_at: db_save.created_at,
            slot: slot.clone(),
        };
        
        match slot.as_str() {
            "auto" => auto = Some(save_data),
            "slot_1" => slots[0] = Some(save_data),
            "slot_2" => slots[1] = Some(save_data),
            "slot_3" => slots[2] = Some(save_data),
            _ => {}
        }
    }
    
    Ok(SaveSlotList { auto, slots })
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
pub fn load_scenario(
    state: &mut AppState,
    db: &Db,
    scenario_id: String,
) -> Result<SaveResponse, String> {
    // Delete events from previous playthrough of this scenario
    db.delete_events_for_scenario(&scenario_id)?;

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

// ============================================================================
// Helper Functions
// ============================================================================

/// Apply player action - unified for all scenarios via MetricRef
fn apply_player_action(state: &mut AppState, action_input: &PlayerActionInput) -> Result<(HashMap<String, f64>, HashMap<String, f64>), String> {
    use crate::core::MetricRef;

    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
    let world_state = state.world_state.as_mut().ok_or("No active world state")?;

    let action = scenario.patron_actions.iter()
        .find(|a| a.id == action_input.action_id)
        .ok_or_else(|| format!("Action '{}' not found", action_input.action_id))?
        .clone();

    // Check availability using unified function
    if !is_action_available(&action, world_state) {
        return Err("Action is not available".to_string());
    }

    // Apply cost
    let mut applied_costs = HashMap::new();
    for (metric, cost) in &action.cost {
        let metric_ref = MetricRef::parse(metric);
        metric_ref.apply(world_state, *cost);
        applied_costs.insert(metric.clone(), *cost);
    }

    eprintln!("[DEBUG] apply_player_action - applied_costs: {:?}", applied_costs);
    eprintln!("[DEBUG] apply_player_action - family_metrics after cost: {:?}", world_state.family_metrics);

    // Apply effects with dynamic federation weights
    let mut applied_effects = HashMap::new();
    for (metric, effect) in &action.effects {
        let metric_ref = MetricRef::parse(metric);
        if metric == "federation_progress" {
            // Apply federation_progress with dynamic weight based on the action's source
            let weight = determine_action_source_weight(&action, world_state);
            let weighted_effect = effect * weight;
            metric_ref.apply(world_state, weighted_effect);
            applied_effects.insert(metric.clone(), weighted_effect);
        } else {
            metric_ref.apply(world_state, *effect);
            applied_effects.insert(metric.clone(), *effect);
        }
    }

    eprintln!("[DEBUG] apply_player_action - applied_effects: {:?}", applied_effects);
    eprintln!("[DEBUG] apply_player_action - family_metrics after effects: {:?}", world_state.family_metrics);

    // Record event - use first foreground actor or default
    let event_actor = world_state.actors.values()
        .find(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground)
        .map(|a| a.id.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let event = Event::new(
        format!("player_action_{}", action_input.action_id),
        world_state.tick,
        world_state.year,
        event_actor,
        crate::core::EventType::PlayerAction,
        true,
        format!("Действие игрока: {}", action.name),
    );
    state.event_log.add(event);

    Ok((applied_effects, applied_costs))
}

/// Determine the weight multiplier for federation_progress based on action source
fn determine_action_source_weight(action: &crate::core::PatronAction, world_state: &WorldState) -> f64 {
    // Extract source actor from action ID (e.g., "venice_naval_support" -> "venice")
    let source_actor = action.id.split('_').next().unwrap_or("");
    crate::scenarios::constantinople_1430::federation_weight(source_actor, world_state)
}

/// Unified action availability check - works for all scenarios via MetricRef
fn is_action_available(action: &crate::core::PatronAction, world_state: &WorldState) -> bool {
    use crate::core::MetricRef;

    match &action.available_if {
        crate::core::ActionCondition::Always => true,
        crate::core::ActionCondition::Metric { metric, operator, value } => {
            let metric_ref = MetricRef::parse(metric);
            let current = metric_ref.get(world_state);
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

fn check_llm_trigger_with_data(
    world_state: &mut WorldState,
    scenario: &Scenario,
    event_log: &EventLog,
    action_info: Option<(&crate::core::PatronAction, &HashMap<String, f64>, &HashMap<String, f64>)>, // (action, effects, costs)
) -> Option<crate::llm::LlmTrigger> {
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
        
        return Some(crate::llm::LlmTrigger {
            trigger_type: crate::llm::TriggerType::PlayerAction,
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
            context: crate::llm::LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids,
                recent_events,
                scenario_context: scenario.llm_context.clone(),
                ticks_since_last: ticks_since_last,
            },
            action_info: Some(crate::llm::ActionInfo {
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

        return Some(crate::llm::LlmTrigger {
            trigger_type: crate::llm::TriggerType::ThresholdEvent,
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
            context: crate::llm::LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids.clone(),
                recent_events,
                scenario_context: scenario.llm_context.clone(),
                ticks_since_last: ticks_since_last,
            },
            action_info: None,
            threshold_context: Some(crate::llm::ThresholdContext {
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
        
        return Some(crate::llm::LlmTrigger {
            trigger_type: crate::llm::TriggerType::Time,
            prompt: generate_llm_prompt_with_trigger(
                world_state,
                scenario,
                &narrative_actor_ids,
                &recent_events,
                &actor_deltas,
                ticks_since_last,
                Some(&TriggerDetail::Time),
            ),
            context: crate::llm::LlmContext {
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
