#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use engine13::commands::{self, AppState};
use std::sync::Mutex;
use tauri::State;

// Tauri command wrappers with debug logging
#[tauri::command]
fn cmd_get_world_state(state: State<Mutex<AppState>>) -> Result<Option<engine13::WorldState>, String> {
    eprintln!("[RUST] cmd_get_world_state - acquiring lock");
    let mut s = state.lock().map_err(|e| {
        eprintln!("[RUST] cmd_get_world_state - lock error: {}", e);
        e.to_string()
    })?;
    
    // If family_metrics is empty, initialize with default values
    if let Some(ref mut world_state) = s.world_state {
        if world_state.family_metrics.is_empty() {
            world_state.family_metrics.insert("family_influence".to_string(), 0.0);
            world_state.family_metrics.insert("family_knowledge".to_string(), 0.0);
            world_state.family_metrics.insert("family_wealth".to_string(), 0.0);
            world_state.family_metrics.insert("family_connections".to_string(), 0.0);
        }
    }
    
    eprintln!("[RUST] cmd_get_world_state - returning state: {:?}", s.world_state.is_some());
    Ok(s.world_state.clone())
}

#[tauri::command]
fn cmd_advance_tick(state: State<Mutex<AppState>>, action: Option<commands::PlayerActionInput>) -> Result<commands::AdvanceTickResponse, String> {
    eprintln!("[RUST] cmd_advance_tick - acquiring lock");
    let mut s = state.lock().map_err(|e| e.to_string())?;
    eprintln!("[RUST] cmd_advance_tick - calling advance_tick");
    let result = commands::advance_tick(&mut *s, action);
    eprintln!("[RUST] cmd_advance_tick - result: {:?}", result.is_ok());
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
fn cmd_submit_action(state: State<Mutex<AppState>>, action_id: String) -> Result<commands::SubmitActionResponse, String> {
    eprintln!("[RUST] cmd_submit_action - acquiring lock, action_id: {}", action_id);
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::submit_action(&mut *s, action_id);
    eprintln!("[RUST] cmd_submit_action - result: {:?}", result.is_ok());
    result
}

#[tauri::command]
fn cmd_save_game(state: State<Mutex<AppState>>, slot: Option<String>, name: Option<String>) -> Result<commands::SaveResponse, String> {
    eprintln!("[RUST] cmd_save_game - acquiring lock");
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::save_game(&mut *s, slot, name);
    eprintln!("[RUST] cmd_save_game - result: {:?}", result);
    result
}

#[tauri::command]
fn cmd_load_game(state: State<Mutex<AppState>>, save_id: String) -> Result<commands::LoadResponse, String> {
    eprintln!("[RUST] cmd_load_game - acquiring lock, save_id: {}", save_id);
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::load_game(&mut *s, save_id);
    eprintln!("[RUST] cmd_load_game - result: {:?}", result.is_ok());
    result
}

#[tauri::command]
fn cmd_list_saves(state: State<Mutex<AppState>>) -> Result<Vec<commands::SaveData>, String> {
    eprintln!("[RUST] cmd_list_saves - acquiring lock");
    let s = state.lock().map_err(|e| e.to_string())?;
    let saves: Vec<_> = s.saves.values().cloned().collect();
    eprintln!("[RUST] cmd_list_saves - found {} saves", saves.len());
    Ok(saves)
}

#[tauri::command]
fn cmd_get_relevant_events(state: State<Mutex<AppState>>, actor_ids: Vec<String>) -> Result<Vec<engine13::Event>, String> {
    eprintln!("[RUST] cmd_get_relevant_events - acquiring lock");
    let s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::get_relevant_events(&*s, actor_ids);
    eprintln!("[RUST] cmd_get_relevant_events - result: {:?}", result.as_ref().map(|e| e.len()));
    result
}

#[tauri::command]
fn cmd_load_scenario(state: State<Mutex<AppState>>, scenario_id: String) -> Result<commands::SaveResponse, String> {
    eprintln!("[RUST] cmd_load_scenario - acquiring lock, scenario_id: {}", scenario_id);
    let mut s = state.lock().map_err(|e| {
        eprintln!("[RUST] cmd_load_scenario - lock error: {}", e);
        e.to_string()
    })?;
    eprintln!("[RUST] cmd_load_scenario - calling commands::load_scenario");
    let result = commands::load_scenario(&mut *s, scenario_id);
    eprintln!("[RUST] cmd_load_scenario - result: {:?}", result);
    result
}

#[tauri::command]
fn cmd_get_scenario_list() -> Vec<commands::ScenarioMeta> {
    eprintln!("[RUST] cmd_get_scenario_list - returning static list");
    commands::get_scenario_list()
}

#[tauri::command]
async fn cmd_get_narrative(state: State<'_, Mutex<AppState>>, app: tauri::AppHandle) -> Result<(), String> {
    eprintln!("[RUST] cmd_get_narrative - acquiring lock");
    let s = state.lock().map_err(|e| e.to_string())?;
    let result = commands::cmd_get_narrative(&*s, app).await;
    eprintln!("[RUST] cmd_get_narrative - result: {:?}", result.is_ok());
    result
}

#[tauri::command]
fn cmd_get_available_models(provider: String, base_url: String, api_key: Option<String>) -> Result<Vec<String>, String> {
    eprintln!("[RUST] cmd_get_available_models - provider: {}", provider);
    let result = commands::cmd_get_available_models(provider, base_url, api_key);
    eprintln!("[RUST] cmd_get_available_models - result: {:?}", result.as_ref().map(|m| m.len()));
    result
}

#[tauri::command]
fn cmd_save_llm_config(provider: String, base_url: String, api_key: Option<String>, model: String) -> Result<(), String> {
    eprintln!("[RUST] cmd_save_llm_config - provider: {}, model: {}", provider, model);
    let result = commands::cmd_save_llm_config(provider, base_url, api_key, model);
    eprintln!("[RUST] cmd_save_llm_config - result: {:?}", result.is_ok());
    result
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
fn main() {
    eprintln!("[RUST] Starting ENGINE13 Tauri v2 app");
    
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            cmd_get_world_state,
            cmd_advance_tick,
            cmd_get_narrative_actors,
            cmd_get_available_actions,
            cmd_submit_action,
            cmd_save_game,
            cmd_load_game,
            cmd_list_saves,
            cmd_get_relevant_events,
            cmd_load_scenario,
            cmd_get_scenario_list,
            cmd_get_narrative,
            cmd_get_available_models,
            cmd_save_llm_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
