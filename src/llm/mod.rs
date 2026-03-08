use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tauri::Emitter;

use crate::core::{ActorDelta, Event, Scenario, WorldState};
use crate::db::Db;
use crate::engine::EventLog;

/// Narrative season for dual-phase chronicle
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NarrativeSeason {
    Spring,  // Beginning of year, overview, anticipations
    Autumn,  // End of year, outcomes, consequences
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
    pub threshold_type: String,
    pub description: String,
}

/// LLM trigger response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTrigger {
    pub trigger_type: TriggerType,
    pub prompt: String,
    pub context: LlmContext,
    pub action_info: Option<ActionInfo>,
    pub threshold_context: Option<ThresholdContext>,
    pub actor_deltas: Vec<ActorDelta>,
}

/// Context for LLM generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmContext {
    pub current_year: i32,
    pub current_tick: u32,
    pub narrative_actors: Vec<String>,
    pub recent_events: Vec<String>,
    pub scenario_context: String,
    pub ticks_since_last: u32,
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

/// Save LLM config to ~/.config/engine13/config.json
pub fn save_llm_config(config: &LlmConfig) -> Result<(), String> {
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

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&config_path, json)
        .map_err(|e| format!("Failed to write config file: {}", e))?;

    Ok(())
}

/// System prompt for chronicler persona
fn system_prompt(season: NarrativeSeason) -> &'static str {
    match season {
        NarrativeSeason::Spring => {
            "Ты летописец XIV-XV века. Пишешь хронику одним абзацем — 4-6 предложений.

ОБЯЗАТЕЛЬНО:
- Первое предложение содержит год и сезон: 'Весной 1436 года...'
- Пиши с высоты птичьего полёта — видишь всю Европу, Анатолию, Балканы одновременно
- Передавай ощущение эпохи, движение сил, атмосферу — не перечисляй факты
- Упоминай правителей по имени когда это драматично
- НИКОГДА не перечисляй акторов по очереди
- НИКОГДА не называй числа и проценты
- НИКОГДА не пиши 'актор X имеет Y единиц'
- Пиши как будто читатель уже знает кто эти люди
- Фокус на предчувствиях, расстановке сил, напряжении"
        }
        NarrativeSeason::Autumn => {
            "Ты летописец XIV-XV века. Пишешь хронику одним абзацем — 4-6 предложений.

ОБЯЗАТЕЛЬНО:
- Первое предложение содержит год и сезон: 'Осенью 1435 года...'
- Пиши с высоты птичьего полёта — видишь всю Европу, Анатолию, Балканы одновременно
- Передавай ощущение эпохи, движение сил, атмосферу — не перечисляй факты
- Упоминай правителей по имени когда это драматично
- НИКОГДА не перечисляй акторов по очереди
- НИКОГДА не называй числа и проценты
- НИКОГДА не пиши 'актор X имеет Y единиц'
- Пиши как будто читатель уже знает кто эти люди
- Фокус на том что произошло за год, последствиях, итогах"
        }
    }
}

/// Generate narrative prompt from world state and scenario
pub fn generate_narrative_prompt(
    world_state: &WorldState,
    scenario: &Scenario,
    event_log: &EventLog,
    db: &Db,
    season: NarrativeSeason,
) -> String {
    let mut prompt = String::new();
    let current_year = world_state.year;

    // Section 0: System prompt - chronicler persona
    prompt.push_str(system_prompt(season));
    prompt.push_str("\n\n");

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

    // Section 1.5: Year and season anchoring instruction
    let season_name = match season {
        NarrativeSeason::Spring => "весна",
        NarrativeSeason::Autumn => "осень",
    };
    prompt.push_str(&format!(
        "=== ИНСТРУКЦИЯ ===\n\
         ТЕКУЩИЙ ГОД: {}, СЕЗОН: {}. \n\
         Пиши ТОЛЬКО про события этого года.\n\
         Не упоминай будущие годы.\n\
         Не экстраполируй за пределы {}.\n\n",
        current_year, season_name, current_year
    ));

    // Section 1.6: Strict narrative rules — no raw numbers
    prompt.push_str(
        "=== СТРОГИЕ ПРАВИЛА НАРРАТИВА ===\n\
         - НИКОГДА не упоминать числа, проценты, метрики, единицы\n\
         - НИКОГДА не писать \"+13.6\", \"legitimacy: 70\", \"military_size +15\"\n\
         - НИКОГДА не перечислять статистику акторов в тексте\n\
         - Описывать только события, решения, настроения, последствия\n\
         - \"Венеция переживала подъём\" — не \"economic_output вырос на 8\"\n\
         - \"Армия Византии окрепла\" — не \"military_size +15\"\n\
         - \"Казна Генуи истощилась\" — не \"treasury: -40\"\n\
         - Хроника — это литература, не таблица данных\n\n"
    );

    // Section 2: World state - foreground actors only
    prompt.push_str("=== СОСТОЯНИЕ МИРА ===\n");
    prompt.push_str(&format!("Год: {} (тик {})\n\n", world_state.year, world_state.tick));

    let foreground_actors: Vec<_> = world_state
        .actors
        .values()
        .filter(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground)
        .collect();

    for actor in &foreground_actors {
        // Format actor name with leader if present
        let actor_header = if let Some(ref leader) = actor.leader {
            format!("{} ({}):\n", actor.name, leader)
        } else {
            format!("{} ({}):\n", actor.name, actor.name_short)
        };
        prompt.push_str(&actor_header);
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
    let recent_event_types: std::collections::HashSet<String> = event_log.events.iter()
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

    let events_to_show: Vec<crate::core::Event> = match relevant_events {
        Ok(events) => {
            eprintln!("[NARRATIVE] Got {} relevant events from DB", events.len());
            // Filter out events from the future
            events.into_iter().filter(|e| e.year <= current_year).collect()
        }
        Err(e) => {
            eprintln!("[NARRATIVE] Failed to get relevant events from DB: {}", e);
            // Fallback to simple event_log query
            event_log.events.iter()
                .filter(|e| {
                    (e.is_key || narrative_actor_ids.contains(&e.actor_id)) && e.year <= current_year
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
pub fn get_available_models(provider: String, base_url: String, api_key: Option<String>) -> Result<Vec<String>, String> {
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
