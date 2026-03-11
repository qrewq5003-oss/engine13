#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use engine13::application::actions::ActionInfo;
use engine13::commands::{self, AppState};
use engine13::db::Db;
use engine13::llm;
use std::sync::Mutex;
use tauri::State;

// Tauri command wrappers with debug logging
#[tauri::command]
fn cmd_get_world_state(state: State<Mutex<AppState>>) -> Result<Option<engine13::WorldState>, String> {
    eprintln!("[RUST] cmd_get_world_state - acquiring lock");
    let s = state.lock().map_err(|e| {
        eprintln!("[RUST] cmd_get_world_state - lock error: {}", e);
        e.to_string()
    })?;

    eprintln!("[RUST] cmd_get_world_state - returning state: {:?}", s.world_state.is_some());
    Ok(s.world_state.clone())
}

#[tauri::command]
fn cmd_get_status_indicators(
    state: State<Mutex<AppState>>,
) -> Result<Vec<commands::StatusIndicatorState>, String> {
    eprintln!("[RUST] cmd_get_status_indicators - acquiring lock");
    let s = state.lock().map_err(|e| e.to_string())?;

    let world_state = s.world_state.as_ref().ok_or("No world state")?;
    let scenario = s.current_scenario.as_ref().ok_or("No scenario")?;

    let indicators = commands::compute_status_indicators(world_state, scenario);
    Ok(indicators)
}

#[tauri::command]
fn cmd_advance_tick(
    state: State<Mutex<AppState>>,
    db: State<Mutex<Db>>,
    action: Option<commands::PlayerActionInput>,
) -> Result<commands::AdvanceTickResponse, String> {
    eprintln!("[RUST] cmd_advance_tick - acquiring locks");
    
    // First, advance the tick and get events
    let mut s = state.lock().map_err(|e| e.to_string())?;
    eprintln!("[RUST] cmd_advance_tick - calling advance_tick");
    let result = commands::advance_tick(&mut *s, action);
    eprintln!("[RUST] cmd_advance_tick - result: {:?}", result.is_ok());
    
    // If successful, write events and dead actors to database
    if let Ok(ref response) = result {
        let mut db_guard = db.lock().map_err(|e| e.to_string())?;

        // Write events to database
        if !response.events.is_empty() {
            if let Err(e) = (&mut *db_guard).insert_events_batch(&response.events) {
                eprintln!("[RUST] cmd_advance_tick - failed to write events to DB: {}", e);
            } else {
                eprintln!("[RUST] cmd_advance_tick - wrote {} events to DB", response.events.len());
            }
        }

        // Write dead actors to database
        if let Some(ref world_state) = s.world_state {
            if !world_state.dead_actors.is_empty() {
                for dead_actor in &world_state.dead_actors {
                    if let Err(e) = db_guard.insert_dead_actor_from_core(dead_actor) {
                        eprintln!("[RUST] cmd_advance_tick - failed to write dead actor to DB: {}", e);
                    } else {
                        eprintln!("[RUST] cmd_advance_tick - wrote dead actor {} to DB", dead_actor.id);
                    }
                }
            }
        }
    }
    
    result
}

#[tauri::command]
fn cmd_get_narrative_actors(state: State<Mutex<AppState>>) -> Result<Vec<engine13::Actor>, String> {
    eprintln!("[RUST] cmd_get_narrative_actors - acquiring lock");
    let s = state.lock().map_err(|e| e.to_string())?;
    let world_state = s.world_state.as_ref().ok_or("No active world state")?;
    let actors: Vec<_> = world_state.actors.values()
        .filter(|a| a.narrative_status == engine13::NarrativeStatus::Foreground)
        .cloned()
        .collect();
    eprintln!("[RUST] cmd_get_narrative_actors - found {} foreground actors", actors.len());
    Ok(actors)
}

#[tauri::command]
fn cmd_get_available_actions(state: State<Mutex<AppState>>) -> Result<Vec<engine13::PatronAction>, String> {
    eprintln!("[RUST] cmd_get_available_actions - acquiring lock");
    let s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::get_available_actions(&*s);
    eprintln!("[RUST] cmd_get_available_actions - result: {:?}", result.as_ref().map(|a| a.len()));
    result
}

#[tauri::command]
fn cmd_get_actions_with_availability(state: State<Mutex<AppState>>) -> Result<Vec<ActionInfo>, String> {
    eprintln!("[RUST] cmd_get_actions_with_availability - acquiring lock");
    let s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::get_actions_with_availability(&*s);
    eprintln!("[RUST] cmd_get_actions_with_availability - result: {:?}", result.as_ref().map(|a| a.len()));
    result
}

#[tauri::command]
fn cmd_submit_action(
    state: State<Mutex<AppState>>,
    db: State<Mutex<Db>>,
    action_id: String,
) -> Result<commands::SubmitActionResponse, String> {
    eprintln!("[RUST] cmd_submit_action - acquiring locks, action_id: {}", action_id);
    
    // First, submit the action and get response
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::submit_action(&mut *s, action_id);
    eprintln!("[RUST] cmd_submit_action - result: {:?}", result.is_ok());
    
    // If successful, write events to database
    if let Ok(ref _response) = result {
        let mut db_guard = db.lock().map_err(|e| e.to_string())?;
        // Get events from the event_log (submit_action doesn't return events directly)
        let events = s.event_log.events.clone();
        if !events.is_empty() {
            if let Err(e) = db_guard.insert_events_batch(&events) {
                eprintln!("[RUST] cmd_submit_action - failed to write events to DB: {}", e);
            } else {
                eprintln!("[RUST] cmd_submit_action - wrote {} events to DB", events.len());
            }
        }
    }
    
    result
}

#[tauri::command]
fn cmd_save_game(
    state: State<Mutex<AppState>>,
    db: State<Mutex<Db>>,
    slot: Option<String>,
) -> Result<commands::SaveResponse, String> {
    eprintln!("[RUST] cmd_save_game - acquiring locks");
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    let result = commands::save_game(&mut *s, &*db_guard, slot);
    eprintln!("[RUST] cmd_save_game - result: {:?}", result);
    result
}

#[tauri::command]
fn cmd_load_game(
    state: State<Mutex<AppState>>,
    db: State<Mutex<Db>>,
    save_id: String,
) -> Result<commands::LoadResponse, String> {
    eprintln!("[RUST] cmd_load_game - acquiring locks, save_id: {}", save_id);
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    let result = commands::load_game(&mut *s, &*db_guard, save_id);
    eprintln!("[RUST] cmd_load_game - result: {:?}", result.is_ok());
    result
}

#[tauri::command]
fn cmd_list_saves(db: State<Mutex<Db>>) -> Result<Vec<commands::SaveData>, String> {
    eprintln!("[RUST] cmd_list_saves - acquiring lock");
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    let saves = commands::list_saves(&*db_guard);
    eprintln!("[RUST] cmd_list_saves - found {} saves", saves.len());
    Ok(saves)
}

#[tauri::command]
fn cmd_list_saves_with_slots(
    db: State<Mutex<Db>>,
    scenario_id: String,
) -> Result<engine13::application::SaveSlotList, String> {
    eprintln!("[RUST] cmd_list_saves_with_slots - acquiring lock, scenario: {}", scenario_id);
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    let result = commands::list_saves_with_slots(&*db_guard, &scenario_id);
    eprintln!("[RUST] cmd_list_saves_with_slots - result: {:?}", result.is_ok());
    result
}

#[tauri::command]
fn cmd_get_relevant_events(
    db: State<Mutex<Db>>,
    actor_ids: Vec<String>,
) -> Result<Vec<engine13::Event>, String> {
    eprintln!("[RUST] cmd_get_relevant_events - acquiring lock");
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    let result = commands::get_relevant_events(&*db_guard, actor_ids);
    eprintln!("[RUST] cmd_get_relevant_events - result: {:?}", result.as_ref().map(|e| e.len()));
    result
}

#[tauri::command]
fn cmd_get_action_history(
    db: State<Mutex<Db>>,
    limit: usize,
) -> Result<Vec<commands::ActionHistoryEntry>, String> {
    eprintln!("[RUST] cmd_get_action_history - acquiring lock");
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    let result = commands::get_action_history(&*db_guard, limit);
    eprintln!("[RUST] cmd_get_action_history - result: {:?}", result.as_ref().map(|h| h.len()));
    result
}

#[tauri::command]
fn cmd_get_tick_explanation(
    state: State<Mutex<AppState>>,
) -> Result<engine13::TickExplanation, String> {
    eprintln!("[RUST] cmd_get_tick_explanation - acquiring lock");
    let s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::get_tick_explanation(&*s);
    eprintln!("[RUST] cmd_get_tick_explanation - result: {:?}", result.as_ref().map(|e| e.tick));
    result
}

#[tauri::command]
fn cmd_load_scenario(
    state: State<Mutex<AppState>>,
    db: State<Mutex<Db>>,
    scenario_id: String,
) -> Result<commands::SaveResponse, String> {
    eprintln!("[RUST] cmd_load_scenario - acquiring lock, scenario_id: {}", scenario_id);
    let mut s = state.lock().map_err(|e| {
        eprintln!("[RUST] cmd_load_scenario - lock error: {}", e);
        e.to_string()
    })?;
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    eprintln!("[RUST] cmd_load_scenario - calling commands::load_scenario");
    let result = commands::load_scenario(&mut *s, &*db_guard, scenario_id);
    eprintln!("[RUST] cmd_load_scenario - result: {:?}", result);
    result
}

#[tauri::command]
fn cmd_get_scenario_list() -> Vec<commands::ScenarioMeta> {
    eprintln!("[RUST] cmd_get_scenario_list - returning static list");
    commands::get_scenario_list()
}

#[tauri::command]
async fn cmd_get_narrative(
    state: State<'_, Mutex<AppState>>,
    db: State<'_, Mutex<Db>>,
    app: tauri::AppHandle,
    _half_year: engine13::llm::HalfYear,  // Kept for API compatibility, now derived from snapshot
) -> Result<(), String> {
    eprintln!("[RUST] cmd_get_narrative - acquiring locks");

    // Clone AppState and generate prompt before await (releases locks)
    let (prompt, placeholder, config, year) = {
        let s = state.lock().map_err(|e| e.to_string())?;
        let db_guard = db.lock().map_err(|e| e.to_string())?;
        let world_state = s.world_state.as_ref().ok_or("No active world state")?;
        let scenario = s.current_scenario.as_ref().ok_or("No active scenario")?;
        
        // Build snapshot from state (includes half_year)
        let snapshot = engine13::llm::build_snapshot(world_state, scenario, &s.event_log);
        
        // Generate prompt using snapshot and narrative memory
        let prompt = engine13::llm::generate_narrative_prompt(&snapshot, scenario, &*db_guard, &s.narrative_memory);
        let placeholder = format!("{} {} года. Хроника продолжается.", snapshot.half_year.display_name(), snapshot.year);
        let config = engine13::llm::get_llm_config();
        let year = snapshot.year;
        (prompt, placeholder, config, year)
    }; // All locks released here

    // Now do the async HTTP requests without holding any locks
    eprintln!("[NARRATIVE] Getting narrative for year {} ({:?})", year, _half_year);
    eprintln!("[NARRATIVE] Provider: {}, URL: {}, Model: {}", config.provider, config.base_url, config.model);

    let result = if config.provider == "anthropic" {
        engine13::llm::stream_narrative_anthropic(prompt, placeholder, config, app).await
    } else {
        engine13::llm::stream_narrative_openai(prompt, placeholder, config, app).await
    };

    eprintln!("[RUST] cmd_get_narrative - result: {:?}", result.is_ok());
    result
}

#[tauri::command]
fn cmd_set_game_mode(
    state: State<Mutex<AppState>>,
    mode: String,
) -> Result<(), String> {
    eprintln!("[RUST] cmd_set_game_mode - acquiring lock, mode: {}", mode);
    let mut s = state.lock().map_err(|e| e.to_string())?;

    let new_mode = match mode.as_str() {
        "free" => engine13::GameMode::Free,
        "scenario" => engine13::GameMode::Scenario,
        "consequences" => engine13::GameMode::Consequences,
        _ => return Err(format!("Unknown game mode: {}", mode)),
    };

    let result = commands::set_game_mode(&mut *s, new_mode);
    eprintln!("[RUST] cmd_set_game_mode - result: {:?}", result);
    result
}

#[tauri::command]
fn cmd_get_available_models(provider: String, base_url: String, api_key: Option<String>) -> Result<Vec<String>, String> {
    eprintln!("[RUST] cmd_get_available_models - provider: {}", provider);
    let result = llm::get_available_models(provider, base_url, api_key);
    eprintln!("[RUST] cmd_get_available_models - result: {:?}", result.as_ref().map(|m| m.len()));
    result
}

#[tauri::command]
fn cmd_save_llm_config(provider: String, base_url: String, api_key: Option<String>, model: String) -> Result<(), String> {
    eprintln!("[RUST] cmd_save_llm_config - provider: {}, model: {}", provider, model);
    let config = engine13::llm::LlmConfig {
        provider,
        api_key,
        model,
        base_url,
    };
    let result = llm::save_llm_config(&config);
    eprintln!("[RUST] cmd_save_llm_config - result: {:?}", result.is_ok());
    result
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
fn main() {
    eprintln!("[RUST] Starting ENGINE13 Tauri v2 app");

    // Initialize database
    let db_path = Db::default_path().unwrap_or_else(|e| {
        eprintln!("[RUST] Failed to get default db path: {}, using fallback", e);
        std::path::PathBuf::from("engine13.db")
    });
    eprintln!("[RUST] Database path: {:?}", db_path);

    let db = Db::open(&db_path).unwrap_or_else(|e| {
        panic!("[RUST] Failed to open database at {:?}: {}", db_path, e);
    });
    eprintln!("[RUST] Database initialized successfully");

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .manage(Mutex::new(AppState::default()))
        .manage(Mutex::new(db))
        .invoke_handler(tauri::generate_handler![
            cmd_get_world_state,
            cmd_get_status_indicators,
            cmd_advance_tick,
            cmd_get_narrative_actors,
            cmd_get_available_actions,
            cmd_get_actions_with_availability,
            cmd_submit_action,
            cmd_save_game,
            cmd_load_game,
            cmd_list_saves,
            cmd_list_saves_with_slots,
            cmd_get_relevant_events,
            cmd_get_action_history,
            cmd_get_tick_explanation,
            cmd_load_scenario,
            cmd_get_scenario_list,
            cmd_get_narrative,
            cmd_set_game_mode,
            cmd_get_available_models,
            cmd_save_llm_config,
            commands::cmd_get_map_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
