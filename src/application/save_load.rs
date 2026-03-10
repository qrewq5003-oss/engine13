use std::collections::HashMap;
use std::time::SystemTime;

use crate::core::WorldState;
use crate::db::{Db, DbSave};
use crate::AppState;

/// Validate slot name - any alphanumeric string with underscores, max 32 chars
fn validate_slot(slot: &str) -> bool {
    !slot.is_empty() 
    && slot.len() <= 32 
    && slot.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Save current game state
/// slot: any valid slot name (default "auto")
/// Save ID format: "{scenario_id}__{slot}" - one save per slot per scenario
pub fn save_game(
    state: &mut AppState,
    db: &Db,
    slot: Option<String>,
) -> Result<crate::commands::SaveResponse, String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state to save")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    // Determine slot name
    let slot_name = slot.unwrap_or_else(|| "auto".to_string());

    // Validate slot name
    if !validate_slot(&slot_name) {
        return Err(format!("Invalid slot: {}. Must be alphanumeric with underscores, max 32 chars", slot_name));
    }

    // Save ID format: "{scenario_id}__{slot}" - double underscore as separator
    let save_id = format!("{}__{}", scenario.id, slot_name);
    let save_name = format!("Tick {} - Year {}", world_state.tick, world_state.year);

    // Serialize world_state to JSON
    let world_state_json = serde_json::to_string(&world_state)
        .map_err(|e| format!("Failed to serialize world state: {}", e))?;

    // Serialize family_state for player state
    let player_state_json = serde_json::to_string(&world_state.family_state)
        .map_err(|e| format!("Failed to serialize family state: {}", e))?;

    let db_save = DbSave {
        id: save_id.clone(),
        name: save_name,
        scenario_id: scenario.id.clone(),
        tick: world_state.tick,
        year: world_state.year,
        created_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        world_state_json,
        player_state_json,
    };

    db.insert_save(&db_save)
        .map_err(|e| format!("Failed to insert save: {}", e))?;

    Ok(crate::commands::SaveResponse { success: true, save_id: Some(save_id), error: None })
}

/// Load game from save
pub fn load_game(
    state: &mut AppState,
    db: &Db,
    save_id: String,
) -> Result<crate::commands::LoadResponse, String> {
    let db_save = db.get_save_by_id(&save_id)
        .map_err(|e| format!("Failed to get save: {}", e))?
        .ok_or_else(|| format!("Save '{}' not found", save_id))?;

    // Deserialize world_state from JSON
    let mut world_state: WorldState = serde_json::from_str(&db_save.world_state_json)
        .map_err(|e| format!("Failed to deserialize world state: {}", e))?;

    // Check save format version
    if world_state.save_version != crate::core::SAVE_FORMAT_VERSION {
        return Err(format!(
            "Save format version {} incompatible with current version {}",
            world_state.save_version,
            crate::core::SAVE_FORMAT_VERSION
        ));
    }

    // Deserialize family_state from player_state_json
    if let Ok(family_state) = serde_json::from_str::<Option<crate::core::FamilyState>>(&db_save.player_state_json) {
        world_state.family_state = family_state;
    }

    // Backward compatibility: recalculate year from tick (2 ticks per year)
    // Never trust saved year — always recalculate from tick
    let scenario_id = &db_save.scenario_id;
    let scenario = crate::scenarios::registry::load_by_id(scenario_id)
        .ok_or_else(|| format!("Unknown scenario: {}", scenario_id))?;
    world_state.year = scenario.start_year as i32 + (world_state.tick / 2) as i32;

    // Initialize RNG from world state seed
    // NOTE: RNG sequence restarts from seed after load, not from exact saved position.
    // This is an accepted limitation — save/load does not guarantee identical continuation.
    use rand::SeedableRng;
    state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(world_state.rng_seed));

    state.world_state = Some(world_state.clone());
    state.event_log = crate::engine::EventLog::new();

    if state.current_scenario.as_ref().map(|s| s.id.clone()) != Some(db_save.scenario_id.clone()) {
        let scenario = crate::scenarios::registry::load_by_id(&db_save.scenario_id)
            .ok_or_else(|| format!("Unknown scenario: {}", db_save.scenario_id))?;
        state.current_scenario = Some(scenario);
    }

    Ok(crate::commands::LoadResponse { success: true, world_state: Some(world_state), error: None })
}

/// Load a scenario
pub fn load_scenario(
    state: &mut AppState,
    db: &Db,
    scenario_id: String,
) -> Result<crate::commands::SaveResponse, String> {
    // Delete events from previous playthrough of this scenario
    db.delete_events_for_scenario(&scenario_id)?;

    let scenario = crate::scenarios::registry::load_by_id(&scenario_id)
        .ok_or_else(|| format!("Unknown scenario: {}", scenario_id))?;

    let mut world_state = WorldState::new(scenario.id.clone(), scenario.start_year);
    // Only add actors that are not successor templates
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world_state.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    // Initialize family_state for family-based scenarios
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario.generation_mechanics
            .as_ref()
            .map(|g| g.patriarch_start_age)
            .unwrap_or(40) as u32;

        world_state.family_state = Some(crate::core::FamilyState {
            metrics: initial_metrics.clone(),
            patriarch_age,
            generation_count: 0,
        });
    }

    // Set generation_length from scenario
    world_state.generation_length = scenario.generation_length;

    // Set global_metrics_display from scenario
    world_state.global_metrics_display = scenario.global_metrics_display.clone();

    // Set generation_mechanics from scenario
    world_state.generation_mechanics = scenario.generation_mechanics.clone();

    // Initialize RNG from world state seed
    use rand::SeedableRng;
    state.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(world_state.rng_seed));

    state.current_scenario = Some(scenario);
    state.world_state = Some(world_state);
    state.event_log = crate::engine::EventLog::new();

    Ok(crate::commands::SaveResponse { success: true, save_id: None, error: None })
}

/// List all saves
pub fn list_saves(db: &Db) -> Vec<crate::commands::SaveData> {
    match db.list_saves() {
        Ok(db_saves) => {
            db_saves.iter().map(|db_save| crate::commands::SaveData {
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

/// Save data with slot information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveSlotData {
    pub id: String,
    pub name: String,
    pub scenario_id: String,
    pub tick: u32,
    pub year: i32,
    pub created_at: u64,
    pub slot: String, // any alphanumeric name (e.g., "auto", "slot_1", "quick_save")
}

/// Response from list_saves - grouped by slots
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveSlotList {
    pub auto: Option<SaveSlotData>,
    pub slots: HashMap<String, SaveSlotData>,
}

/// Parse slot from save_id format: "{scenario_id}__{slot}"
fn parse_slot(save_id: &str, scenario_id: &str) -> Option<String> {
    let prefix = format!("{}__", scenario_id);
    save_id.strip_prefix(&prefix).map(|s| s.to_string())
}

/// List saves grouped by slots for a specific scenario
/// Returns dynamic slot list based on existing saves
pub fn list_saves_with_slots(db: &Db, scenario_id: &str) -> Result<SaveSlotList, String> {
    let db_saves = db.list_saves_by_scenario(scenario_id)
        .map_err(|e| format!("Failed to list saves: {}", e))?;

    let mut auto: Option<SaveSlotData> = None;
    let mut slots: HashMap<String, SaveSlotData> = HashMap::new();

    for db_save in db_saves {
        // Extract slot from save_id format: "{scenario_id}__{slot}"
        if let Some(slot) = parse_slot(&db_save.id, scenario_id) {
            let save_data = SaveSlotData {
                id: db_save.id.clone(),
                name: db_save.name.clone(),
                scenario_id: db_save.scenario_id.clone(),
                tick: db_save.tick,
                year: db_save.year,
                created_at: db_save.created_at,
                slot: slot.clone(),
            };

            if slot == "auto" {
                auto = Some(save_data);
            } else {
                slots.insert(slot, save_data);
            }
        }
    }

    Ok(SaveSlotList { auto, slots })
}

/// Get scenario list from registry
pub fn get_scenario_list() -> Vec<crate::commands::ScenarioMeta> {
    crate::scenarios::registry::get_scenario_meta()
}
