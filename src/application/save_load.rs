use std::collections::HashMap;
use std::time::SystemTime;

use crate::core::{Scenario, WorldState};
use crate::db::{Db, DbSave};
use crate::scenarios::{load_constantinople_1430, load_rome_375};
use crate::AppState;

/// Save current game state
/// slot: "auto" | "slot_1" | "slot_2" | "slot_3"
/// Save ID format: "{scenario_id}_{slot}" - one save per slot per scenario
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
    let world_state: WorldState = serde_json::from_str(&db_save.world_state_json)
        .map_err(|e| format!("Failed to deserialize world state: {}", e))?;

    // Deserialize player_state (family_metrics)
    let family_metrics: HashMap<String, f64> = serde_json::from_str(&db_save.player_state_json)
        .unwrap_or_else(|_| HashMap::new());

    state.world_state = Some(world_state.clone());
    state.event_log = crate::engine::EventLog::new();

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
    pub slot: String, // "auto" | "slot_1" | "slot_2" | "slot_3"
}

/// Response from list_saves - grouped by slots
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveSlotList {
    pub auto: Option<SaveSlotData>,
    pub slots: [Option<SaveSlotData>; 3],
}

/// List saves grouped by slots for a specific scenario
pub fn list_saves_with_slots(db: &Db, scenario_id: &str) -> Result<SaveSlotList, String> {
    let db_saves = db.list_saves_by_scenario(scenario_id)
        .map_err(|e| format!("Failed to list saves: {}", e))?;

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

/// Get scenario list
pub fn get_scenario_list() -> Vec<crate::commands::ScenarioMeta> {
    vec![
        crate::commands::ScenarioMeta {
            id: "rome_375".to_string(),
            label: "Rome 375 — Семья Ди Милано".to_string(),
            description: "375 год. Медиолан — фактическая столица Западной Империи.".to_string(),
            start_year: 375,
        },
        crate::commands::ScenarioMeta {
            id: "constantinople_1430".to_string(),
            label: "Constantinople 1430 — Федерация".to_string(),
            description: "1430 год. Фессалоники пали. Константинополь стоит — но ненадолго.".to_string(),
            start_year: 1430,
        },
    ]
}
