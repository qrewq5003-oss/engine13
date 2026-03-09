use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tauri::Emitter;

use crate::core::{ActorDelta, Scenario, WorldState};
use crate::db::Db;
use crate::engine::EventLog;

/// Half-year narrative unit for chronicle generation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HalfYear {
    FirstHalf,   // January-June, first chronicle of the year
    SecondHalf,  // July-December, second chronicle of the year
}

impl HalfYear {
    pub fn from_tick(tick: u32) -> Self {
        // Even ticks = FirstHalf, Odd ticks = SecondHalf
        if tick % 2 == 0 {
            HalfYear::FirstHalf
        } else {
            HalfYear::SecondHalf
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            HalfYear::FirstHalf => "первая половина",
            HalfYear::SecondHalf => "вторая половина",
        }
    }

    pub fn display_name_en(&self) -> &'static str {
        match self {
            HalfYear::FirstHalf => "first half",
            HalfYear::SecondHalf => "second half",
        }
    }
}

/// Summary of a player action for narrative generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerActionSummary {
    pub id: String,
    pub name: String,
    pub key_effects: Vec<String>,
}

/// Complete narrative world snapshot for prompt generation
/// 
/// This is the single source of truth for narrative generation.
/// The prompt builder should only read from this snapshot, not from WorldState directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeWorldSnapshot {
    pub year: i32,
    pub half_year: HalfYear,
    pub alive_actors: Vec<String>,
    pub dead_actors: Vec<String>,
    pub victory_achieved: bool,
    pub foreground_actors: Vec<String>,
    pub key_milestones_fired: Vec<String>,
    pub recent_important_events: Vec<crate::core::Event>,
    pub recent_player_actions: Vec<PlayerActionSummary>,
    pub key_metrics: HashMap<String, f64>,
    pub narrative_axes: Vec<String>,
    pub tone_tags: Vec<String>,
    pub game_mode: crate::core::GameMode,
}

/// Build narrative world snapshot from game state
/// 
/// This is a pure function: it reads state but has no side effects.
/// It does NOT call LLM, modify state, or write to DB.
pub fn build_snapshot(
    world: &WorldState,
    scenario: &Scenario,
    event_log: &EventLog,
) -> NarrativeWorldSnapshot {
    // Half-year from tick
    let half_year = HalfYear::from_tick(world.tick);
    
    // Alive actors (not in dead_actors list)
    let alive_actors: Vec<String> = world.actors.keys()
        .filter(|id| !world.dead_actors.iter().any(|d| &d.id == *id))
        .cloned()
        .collect();
    
    // Dead actors
    let dead_actors: Vec<String> = world.dead_actors.iter()
        .map(|a| a.id.clone())
        .collect();
    
    // Foreground actors
    let foreground_actors: Vec<String> = world.actors.values()
        .filter(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground)
        .map(|a| a.id.clone())
        .collect();
    
    // Key milestones fired
    let key_milestones_fired: Vec<String> = scenario.milestone_events.iter()
        .filter(|m| world.milestone_events_fired.contains(&m.id))
        .map(|m| m.id.clone())
        .collect();
    
    // Recent important events (last 10, keyed events first)
    let mut recent_important_events: Vec<crate::core::Event> = event_log.events.iter()
        .filter(|e| e.is_key || foreground_actors.contains(&e.actor_id))
        .cloned()
        .collect();
    recent_important_events.truncate(10);
    
    // Recent player actions (last 5)
    let recent_player_actions: Vec<PlayerActionSummary> = event_log.events.iter()
        .filter(|e| matches!(e.event_type, crate::core::EventType::PlayerAction))
        .rev()
        .take(5)
        .map(|e| PlayerActionSummary {
            id: e.id.clone(),
            name: e.description.clone(),
            key_effects: {
                // Parse effects from metadata if available
                if !e.metadata.is_empty() {
                    serde_json::from_str::<HashMap<String, f64>>(&e.metadata)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(metric, delta)| format!("{}: {:+.1}", metric, delta))
                        .collect()
                } else {
                    vec![]
                }
            },
        })
        .collect();
    
    // Key metrics from narrative config
    let mut key_metrics: HashMap<String, f64> = HashMap::new();
    for metric_key in &scenario.narrative_config.key_metrics {
        // Try to get the metric value using existing patterns
        let value = if metric_key.starts_with("family:") {
            // Family metric
            world.family_state.as_ref()
                .and_then(|fs| fs.metrics.get(metric_key))
                .copied()
                .unwrap_or(0.0)
        } else if metric_key.starts_with("actor:") {
            // Actor metric: "actor:id.metric"
            if let Some((actor_id, metric)) = metric_key.strip_prefix("actor:").and_then(|s| s.split_once('.')) {
                world.actors.get(actor_id)
                    .map(|a| get_actor_metric(&a.metrics, metric))
                    .unwrap_or(0.0)
            } else {
                0.0
            }
        } else if metric_key.starts_with("global:") {
            // Global metric
            world.global_metrics.get(metric_key).copied().unwrap_or(0.0)
        } else {
            // Try as global metric without prefix
            world.global_metrics.get(metric_key).copied().unwrap_or(0.0)
        };
        key_metrics.insert(metric_key.clone(), value);
    }
    
    // Narrative axes and tone tags from config
    let narrative_axes = scenario.narrative_config.narrative_axes.clone();
    let tone_tags = scenario.narrative_config.tone_tags.clone();
    
    NarrativeWorldSnapshot {
        year: world.year,
        half_year,
        alive_actors,
        dead_actors,
        victory_achieved: world.victory_achieved,
        foreground_actors,
        key_milestones_fired,
        recent_important_events,
        recent_player_actions,
        key_metrics,
        narrative_axes,
        tone_tags,
        game_mode: world.game_mode,
    }
}

/// Helper to get actor metric by name
fn get_actor_metric(metrics: &crate::core::ActorMetrics, name: &str) -> f64 {
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
fn system_prompt(_half_year: HalfYear) -> &'static str {
    // Generic chronicler persona - half-year specific framing done in prompt body
    "Ты летописец XIV-XV века. Пишешь подробную хронику событий.

ОБЯЗАТЕЛЬНО:
- Пиши с высоты птичьего полёта — видишь всю Европу, Анатолию, Балканы одновременно
- Передавай ощущение эпохи, движение сил, атмосферу — не перечисляй факты
- Упоминай правителей по имени когда это драматично
- НИКОГДА не перечисляй акторов по очереди
- НИКОГДА не называй числа и проценты
- НИКОГДА не пиши 'актор X имеет Y единиц'
- Пиши как будто читатель уже знает кто эти люди
- Фокус на предчувствиях, расстановке сил, напряжении"
}

/// Generate narrative prompt from world snapshot
/// 
/// This function reads ONLY from the NarrativeWorldSnapshot.
/// It does NOT read WorldState or Scenario directly.
pub fn generate_narrative_prompt(
    snapshot: &NarrativeWorldSnapshot,
    scenario: &Scenario,
    _db: &Db,
) -> String {
    let mut prompt = String::new();

    // Section 0: System prompt - chronicler persona
    prompt.push_str(system_prompt(snapshot.half_year));
    prompt.push_str("\n\n");

    // Section 0.5: Factual block - prevent hallucination (from snapshot)
    let factual_block = format!(
        "=== ВАЖНЫЕ ФАКТЫ ИГРЫ (не противоречь им) ===\n\
         Год: {}\n\
         Половина года: {}\n\
         Живые акторы: {}\n\
         Павшие акторы: {}\n\
         Передний план (foreground): {}\n\
         Ключевые вехи: {}\n\
         Недавние события: {}\n\
         Победа достигнута: {}\n\n\
         Правила:\n\
         - Пиши только о событиях, подтверждённых состоянием игры и списком событий.\n\
         - Не называй актора павшим, уничтоженным или исчезнувшим, если его нет в списке павших.\n\
         - Для живых акторов в упадке используй формулировки: \"под угрозой\", \"ослаблен\", \"на грани\" — но не \"пал\".\n\
         - Не придумывай победы, смерти правителей, падения городов или коллапсы, которых нет в фактах.\n\n",
        snapshot.year,
        snapshot.half_year.display_name(),
        if snapshot.alive_actors.is_empty() { "нет".to_string() } else { snapshot.alive_actors.join(", ") },
        if snapshot.dead_actors.is_empty() { "нет".to_string() } else { snapshot.dead_actors.join(", ") },
        if snapshot.foreground_actors.is_empty() { "нет".to_string() } else { snapshot.foreground_actors.join(", ") },
        if snapshot.key_milestones_fired.is_empty() { "нет".to_string() } else { snapshot.key_milestones_fired.join(", ") },
        if snapshot.recent_important_events.is_empty() { "нет".to_string() } else { 
            snapshot.recent_important_events.iter().map(|e| e.description.as_str()).collect::<Vec<_>>().join(", ")
        },
        if snapshot.victory_achieved { "да" } else { "нет" },
    );
    prompt.push_str(&factual_block);

    // Section 1: Scenario context (depends on game mode from snapshot)
    match snapshot.game_mode {
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

    // Section 1.5: Year and half-year anchoring instruction (from snapshot)
    prompt.push_str(&format!(
        "=== ИНСТРУКЦИЯ ===\n\
         ТЕКУЩИЙ ГОД: {}, ПОЛОВИНА: {}. \n\
         Пиши ТОЛЬКО про события этого года.\n\
         Не упоминай будущие годы.\n\
         Не экстраполируй за пределы {}.\n\n",
        snapshot.year, snapshot.half_year.display_name(), snapshot.year
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

    // Section 2: World state - foreground actors only (from snapshot key_metrics)
    prompt.push_str("=== СОСТОЯНИЕ МИРА ===\n");
    prompt.push_str(&format!("Год: {}\n\n", snapshot.year));

    // Key metrics from snapshot
    if !snapshot.key_metrics.is_empty() {
        prompt.push_str("Ключевые метрики:\n");
        for (key, value) in &snapshot.key_metrics {
            prompt.push_str(&format!("  {}: {:.1}\n", key, value));
        }
        prompt.push('\n');
    }

    // Section 3: Recent events from snapshot
    prompt.push_str("=== ПОСЛЕДНИЕ СОБЫТИЯ ===\n");
    
    for event in &snapshot.recent_important_events {
        prompt.push_str(&format!("- {}: {}\n", event.id, event.description));
    }
    prompt.push('\n');

    // Section 4: Recent player actions from snapshot
    if !snapshot.recent_player_actions.is_empty() {
        prompt.push_str("=== ДЕЙСТВИЯ ИГРОКА ===\n");
        for action in &snapshot.recent_player_actions {
            prompt.push_str(&format!("- {}\n", action.name));
            if !action.key_effects.is_empty() {
                for effect in &action.key_effects {
                    prompt.push_str(&format!("  → {}\n", effect));
                }
            }
        }
        prompt.push('\n');
    }

    // Section 5: Narrative guidance from config
    if !snapshot.narrative_axes.is_empty() {
        prompt.push_str("=== ТЕМАТИЧЕСКИЕ ОСИ ===\n");
        for axis in &snapshot.narrative_axes {
            prompt.push_str(&format!("- {}\n", axis));
        }
        prompt.push('\n');
    }

    if !snapshot.tone_tags.is_empty() {
        prompt.push_str("=== СТИЛЬ ПОВЕСТВОВАНИЯ ===\n");
        for tag in &snapshot.tone_tags {
            prompt.push_str(&format!("- {}\n", tag));
        }
        prompt.push('\n');
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
