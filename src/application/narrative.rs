use crate::db::Db;
use crate::llm;
use crate::AppState;

/// Get narrative from LLM with streaming
pub async fn cmd_get_narrative(
    state: &AppState,
    db: &Db,
    app: tauri::AppHandle,
    _half_year: crate::llm::HalfYear,  // Kept for API compatibility, now derived from snapshot
) -> Result<(), String> {
    let world_state = state.world_state.as_ref().ok_or("No active world state")?;
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;

    // Build snapshot from state
    let snapshot = crate::llm::build_snapshot(world_state, scenario, &state.event_log);

    let config = llm::get_llm_config();
    // Pass narrative memory for anti-repetition
    let prompt = llm::generate_narrative_prompt(&snapshot, scenario, db);

    // Generate placeholder narrative for when LLM is unavailable
    let placeholder = format!("{} {} года. Хроника продолжается.", snapshot.half_year.display_name(), snapshot.year);

    eprintln!("[NARRATIVE] Getting narrative for year {} ({:?})", snapshot.year, snapshot.half_year);
    eprintln!("[NARRATIVE] Provider: {}, URL: {}, Model: {}", config.provider, config.base_url, config.model);

    if config.provider == "anthropic" {
        // Anthropic format - streaming
        llm::stream_narrative_anthropic(prompt, placeholder, config, app).await
    } else {
        // OpenAI-compatible format - streaming
        llm::stream_narrative_openai(prompt, placeholder, config, app).await
    }
}
