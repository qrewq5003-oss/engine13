use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tauri::Emitter;

use crate::core::{ActorDelta, Scenario, WorldState};
use crate::db::Db;
use crate::engine::EventLog;

/// Half-year narrative unit for chronicle generation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
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
    /// Actors that collapsed this tick: (actor_name, successor_ids)
    pub collapsed_this_tick: Vec<(String, Vec<String>)>,
}

/// Minimal narrative memory for anti-repetition across turns
///
/// This stores just enough information to avoid repeating the same narrative patterns
/// when the world state hasn't changed significantly.
///
/// Memory is NOT used for simulation logic - only for prompt generation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NarrativeMemory {
    /// Gist of last narrative: last sentence of first paragraph, or first 150 chars
    pub last_narrative_gist: Option<String>,
    /// World focus label (e.g., "byzantium under siege", "rome consolidating")
    pub last_world_focus: Option<String>,
    /// Actors that were central in last narrative
    pub last_actor_focus: Vec<String>,
    /// Tone/framing markers that were used
    pub last_tone_markers: Vec<String>,
}

/// Extract gist from narrative text using deterministic rule
///
/// Rule: last sentence of first paragraph
/// Fallback: first 150 characters if paragraphs not clearly delimited
pub fn extract_narrative_gist(narrative: &str) -> String {
    // Try to find first paragraph (separated by double newline)
    let first_paragraph = narrative.split("\n\n").next().unwrap_or(narrative);
    
    // Try to find last sentence (ends with . ! ?)
    let sentences: Vec<&str> = first_paragraph.split(|c| c == '.' || c == '!' || c == '?').collect();
    
    if sentences.len() > 1 {
        // Get the last non-empty sentence
        let last_sentence = sentences.iter()
            .rev()
            .find(|s| !s.trim().is_empty())
            .unwrap_or(&"");
        
        if !last_sentence.trim().is_empty() {
            return last_sentence.trim().to_string();
        }
    }
    
    // Fallback: first 150 characters
    if narrative.len() > 150 {
        format!("{}...", &narrative[..150])
    } else {
        narrative.trim().to_string()
    }
}

/// Extract actor focus from narrative by matching against known actor names
pub fn extract_actor_focus(narrative: &str, known_actors: &[String]) -> Vec<String> {
    let mut focused_actors = Vec::new();
    
    for actor in known_actors {
        // Check if actor name appears in narrative (case-insensitive)
        if narrative.to_lowercase().contains(&actor.to_lowercase()) {
            focused_actors.push(actor.clone());
        }
    }
    
    // Limit to top 3 most prominent (by mention count or just first 3)
    focused_actors.truncate(3);
    focused_actors
}

/// Build memory update from narrative and snapshot
pub fn update_memory(
    narrative: &str,
    snapshot: &NarrativeWorldSnapshot,
    _previous_memory: &NarrativeMemory,
) -> NarrativeMemory {
    NarrativeMemory {
        last_narrative_gist: Some(extract_narrative_gist(narrative)),
        last_world_focus: Some(determine_world_focus(snapshot)),
        last_actor_focus: extract_actor_focus(narrative, &snapshot.foreground_actors),
        last_tone_markers: snapshot.tone_tags.iter().take(3).cloned().collect(),
    }
}

/// Determine world focus label from snapshot
fn determine_world_focus(snapshot: &NarrativeWorldSnapshot) -> String {
    // Simple heuristic based on key metrics and game state
    // This can be expanded later with more sophisticated logic
    
    if snapshot.victory_achieved {
        return "victory achieved".to_string();
    }
    
    // Check for high pressure situations
    for (key, value) in &snapshot.key_metrics {
        if key.contains("pressure") && *value > 80.0 {
            return "high external pressure".to_string();
        }
        if key.contains("cohesion") && *value < 30.0 {
            return "internal fragility".to_string();
        }
        if key.contains("legitimacy") && *value < 30.0 {
            return "legitimacy crisis".to_string();
        }
    }
    
    // Default: use first foreground actor as focus
    if let Some(first_actor) = snapshot.foreground_actors.first() {
        format!("{} centered", first_actor)
    } else {
        "general chronicle".to_string()
    }
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
                    .and_then(|a| a.metrics.get(metric))
                    .copied()
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

    // Collapsed actors this tick: filter event_log for EventType::Collapse
    // Deduplicate by actor_id, get successor_ids from world.dead_actors
    let mut collapsed_this_tick: Vec<(String, Vec<String>)> = Vec::new();
    let mut seen_collapse_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    
    for event in &event_log.events {
        if event.event_type == crate::core::EventType::Collapse 
            && !seen_collapse_ids.contains(&event.actor_id) 
        {
            seen_collapse_ids.insert(event.actor_id.clone());
            
            // Get successor_ids from dead_actors
            let successor_ids: Vec<String> = world.dead_actors.iter()
                .find(|d| d.id == event.actor_id)
                .map(|d| d.successor_ids.iter().map(|s| s.id.clone()).collect())
                .unwrap_or_default();
            
            // Get actor name from event description or use actor_id
            let actor_name = event.actor_id.clone();
            
            collapsed_this_tick.push((actor_name, successor_ids));
        }
    }

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
        collapsed_this_tick,
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
///
/// Prompt structure (optimized for model performance):
/// 1. Identity / role of narrative voice
/// 2. Hard factual rules (anti-hallucination)
/// 3. Previous narrative memory (soft anti-repetition guard)
/// 4. Scenario framing from tone_tags / narrative_axes (as instructions)
/// 5. Current world snapshot
/// 6. Key metrics
/// 7. Key milestones
/// 8. Recent important events (top 5, as evidence)
/// 9. Recent player actions (as narrative causes)
/// 10. Output instructions (2-4 paragraphs, world-first)
pub fn generate_narrative_prompt(
    snapshot: &NarrativeWorldSnapshot,
    scenario: &Scenario,
    _db: &Db,
    memory: &NarrativeMemory,
) -> String {
    let mut prompt = String::new();

    // ========================================================================
    // Section 1: Identity / Role — Chronicler Persona
    // ========================================================================
    prompt.push_str(system_prompt(snapshot.half_year));
    prompt.push_str("\n\n");

    // ========================================================================
    // Section 2: Hard Factual Rules — Anti-Hallucination Discipline
    // ========================================================================
    let factual_rules = format!(
        "=== ЖЁСТКИЕ ФАКТУАЛЬНЫЕ ПРАВИЛА (не нарушать) ===\n\
         Год: {}\n\
         Половина года: {}\n\
         Живые акторы: {}\n\
         Павшие акторы: {}\n\
         Победа достигнута: {}\n\n\
         ЗАПРЕЩЕНО:\n\
         - Писать, что актор пал, уничтожен или исчез, если его нет в списке павших.\n\
         - Писать о победе, если victory_achieved == false.\n\
         - Писать о коллапсе, смерти правителя, падении города, капитуляции, regime change — если этого нет в фактах.\n\
         - Превращать \"высокое давление\" в уже случившийся крах.\n\
         - Придумывать события, которых нет в snapshot или recent events.\n\n\
         РАЗРЕШЕНО (и правильно):\n\
         - \"под угрозой\", \"ослаблен\", \"теряет опору\", \"вынужден маневрировать\"\n\
         - \"позиции укрепляются\", \"баланс смещается\", \"напряжение растёт\"\n\
         - Описывать состояние как \"хрупкое\", \"нестабильное\", \"на грани\" — для живых акторов в упадке.\n\n\
         Пиши только о событиях, подтверждённых состоянием игры и списком событий.\n\n",
        snapshot.year,
        snapshot.half_year.display_name(),
        if snapshot.alive_actors.is_empty() { "нет".to_string() } else { snapshot.alive_actors.join(", ") },
        if snapshot.dead_actors.is_empty() { "нет".to_string() } else { snapshot.dead_actors.join(", ") },
        if snapshot.victory_achieved { "да" } else { "нет" },
    );
    prompt.push_str(&factual_rules);

    // ========================================================================
    // Section 2b: Events This Period — Collapses with Successors
    // ========================================================================
    if !snapshot.collapsed_this_tick.is_empty() {
        prompt.push_str("=== СОБЫТИЯ ЭТОГО ПЕРИОДА ===\n");
        for (actor_name, successors) in &snapshot.collapsed_this_tick {
            if successors.is_empty() {
                prompt.push_str(&format!("{} прекратил существование.\n", actor_name));
            } else {
                prompt.push_str(&format!("{} прекратил существование. Наследники: {}.\n", 
                    actor_name, successors.join(", ")));
            }
        }
        prompt.push('\n');
    }

    // ========================================================================
    // Section 2c: Fallen States — All dead actors accumulated
    // ========================================================================
    if !snapshot.dead_actors.is_empty() {
        prompt.push_str("=== ПАВШИЕ ДЕРЖАВЫ ===\n");
        prompt.push_str(&format!("{}\n\n", snapshot.dead_actors.join(", ")));
    }

    // ========================================================================
    // Section 3: Previous Narrative Memory — Soft Anti-Repetition Guard
    // ========================================================================
    if memory.last_narrative_gist.is_some() || !memory.last_actor_focus.is_empty() {
        prompt.push_str("=== ПАМЯТЬ ПРЕДЫДУЩЕГО НАРРАТИВА (мягкое ограничение) ===\n");
        
        if let Some(ref gist) = memory.last_narrative_gist {
            prompt.push_str(&format!("Последняя хроника: \"{}\"\n", gist));
        }
        
        if !memory.last_actor_focus.is_empty() {
            prompt.push_str(&format!("В центре внимания были: {}\n", memory.last_actor_focus.join(", ")));
        }
        
        if let Some(ref focus) = memory.last_world_focus {
            prompt.push_str(&format!("Мирофокус: {}\n", focus));
        }
        
        prompt.push_str("\n");
        prompt.push_str("Используй эту память чтобы избегать повторения тех же паттернов:\n");
        prompt.push_str("- Не ставь того же актора в центр без новой причины.\n");
        prompt.push_str("- Не используй ту же риторическую рамку, если состояние мира не требует этого.\n");
        prompt.push_str("- Если состояние мира значительно изменилось — похожий фокус допустим.\n");
        prompt.push_str("- Избегай повторения тех же формулировок и драматических каркасов.\n\n");
    }

    // ========================================================================
    // Section 3: Scenario Framing — tone_tags and narrative_axes as Instructions
    // ========================================================================
    // Convert tone_tags into actual instructional framing (not just labels)
    if !snapshot.tone_tags.is_empty() || !snapshot.narrative_axes.is_empty() {
        prompt.push_str("=== НАРРАТИВНЫЕ ИНСТРУКЦИИ ===\n");
        
        // Build instructional text from tone_tags
        let mut instructions: Vec<String> = Vec::new();
        
        for tag in &snapshot.tone_tags {
            match tag.as_str() {
                "political_decay" => instructions.push(
                    "Focus on signs of institutional erosion, weakening legitimacy, and the shrinking reliability of public order.".to_string()
                ),
                "family_chronicle" => instructions.push(
                    "Keep the narrative grounded in the experience of a family navigating wider political currents.".to_string()
                ),
                "coalition_fragility" => instructions.push(
                    "Focus on the fragility of alliances, diplomatic maneuvering under pressure, and the unstable balance between hope and collapse.".to_string()
                ),
                "siege_diplomacy" => instructions.push(
                    "Treat political coordination and strategic hesitation as central to the world's condition.".to_string()
                ),
                "imperial_decline" => instructions.push(
                    "Emphasize the slow unraveling of central authority, the rise of regional powers, and the sense of an era ending.".to_string()
                ),
                "barbarian_pressure" => instructions.push(
                    "Convey the mounting external threat, the strain on borders, and the inevitability of confrontation.".to_string()
                ),
                "trade_competition" => instructions.push(
                    "Frame events through the lens of commercial rivalry, economic leverage, and maritime dominance.".to_string()
                ),
                "religious_tension" => instructions.push(
                    "Highlight the role of faith, doctrinal conflict, and spiritual authority in shaping political choices.".to_string()
                ),
                _ => instructions.push(format!("Consider the theme: {}", tag)),
            }
        }
        
        // Build instructional text from narrative_axes
        for axis in &snapshot.narrative_axes {
            match axis.as_str() {
                "stability vs ambition" => instructions.push(
                    "Frame choices as tensions between maintaining order and pursuing opportunity.".to_string()
                ),
                "tradition vs adaptation" => instructions.push(
                    "Show how actors navigate inherited structures versus new realities.".to_string()
                ),
                "family honor vs political necessity" => instructions.push(
                    "Present decisions as conflicts between dynastic reputation and pragmatic survival.".to_string()
                ),
                "survival vs surrender" => instructions.push(
                    "Emphasize the precariousness of existence and the cost of each compromise.".to_string()
                ),
                "unity vs fragmentation" => instructions.push(
                    "Show the strain between collective action and divergent interests.".to_string()
                ),
                "faith vs pragmatism" => instructions.push(
                    "Frame decisions as tensions between spiritual conviction and practical necessity.".to_string()
                ),
                _ => instructions.push(format!("Consider the axis: {}", axis)),
            }
        }
        
        for instruction in instructions {
            prompt.push_str(&format!("{}\n", instruction));
        }
        prompt.push('\n');
    }

    // ========================================================================
    // Section 4: Scenario Context (game mode dependent)
    // ========================================================================
    match snapshot.game_mode {
        crate::core::GameMode::Consequences => {
            prompt.push_str(&scenario.consequence_context);
            prompt.push_str("\n\n");
        }
        crate::core::GameMode::Free => {
            // Free mode: no scenario context
        }
        _ => {
            prompt.push_str(&scenario.llm_context);
            prompt.push_str("\n\n");
        }
    }

    // ========================================================================
    // Section 5: World Snapshot — Key Metrics
    // ========================================================================
    prompt.push_str("=== СОСТОЯНИЕ МИРА ===\n");
    prompt.push_str(&format!("Год: {}\n\n", snapshot.year));

    if !snapshot.key_metrics.is_empty() {
        prompt.push_str("Ключевые метрики:\n");
        for (key, value) in &snapshot.key_metrics {
            prompt.push_str(&format!("  {}: {:.1}\n", key, value));
        }
        prompt.push('\n');
    }

    // ========================================================================
    // Section 6: Key Milestones Fired
    // ========================================================================
    if !snapshot.key_milestones_fired.is_empty() {
        prompt.push_str("Ключевые вехи:\n");
        for milestone in &snapshot.key_milestones_fired {
            prompt.push_str(&format!("  - {}\n", milestone));
        }
        prompt.push('\n');
    }

    // ========================================================================
    // Section 7: Recent Important Events (Top 5 as Evidence)
    // ========================================================================
    prompt.push_str("=== НЕДАВНИЕ СОБЫТИЯ (используй как доказательства, не как чек-лист) ===\n");
    
    // Limit to top 5 events
    let events_to_show: Vec<_> = snapshot.recent_important_events.iter().take(5).collect();
    
    if events_to_show.is_empty() {
        prompt.push_str("Нет недавних событий.\n");
    } else {
        for event in events_to_show {
            prompt.push_str(&format!("- {}: {}\n", event.id, event.description));
        }
    }
    prompt.push_str("\nИспользуй эти события как доказательства, а не как обязательный список для пересказа.\n");
    prompt.push_str("Не описывай их по одному, если одно событие явно доминирует в этой половине года.\n\n");

    // ========================================================================
    // Section 8: Recent Player Actions (as Narrative Causes)
    // ========================================================================
    if !snapshot.recent_player_actions.is_empty() {
        prompt.push_str("=== ДЕЙСТВИЯ ИГРОКА (как причины изменений) ===\n");
        for action in &snapshot.recent_player_actions {
            prompt.push_str(&format!("- {}\n", action.name));
            if !action.key_effects.is_empty() {
                for effect in &action.key_effects {
                    prompt.push_str(&format!("  → {}\n", effect));
                }
            }
        }
        prompt.push_str("\nИспользуй действия игрока как сигналы направления политики, причины изменений, сознательные ходы.\n");
        prompt.push_str("Не превращай их в UI-log или список транзакций.\n\n");
    }

    // ========================================================================
    // Section 9: Output Instructions — World-First, 2-4 Paragraphs
    // ========================================================================
    prompt.push_str("=== ИНСТРУКЦИИ ПО ВЫВОДУ ===\n");
    prompt.push_str("Напиши хронику этой половины года в формате 2–4 содержательных абзацев.\n\n");
    prompt.push_str("Предпочтительная структура:\n");
    prompt.push_str("1. Первый абзац — что изменилось в общей картине мира за эту половину года.\n");
    prompt.push_str("2. Второй абзац — что это значит политически / социально / военным образом.\n");
    prompt.push_str("3. Третий абзац (если нужно) — какое направление, риск или возможность формируются дальше.\n\n");
    prompt.push_str("ВАЖНО:\n");
    prompt.push_str("- Описывай сначала состояние мира, потом как это проявляется через ключевых акторов.\n");
    prompt.push_str("- НЕ превращай narrative в список обновлений по акторам (actor-by-actor checklist).\n");
    prompt.push_str("- Используй акторов только в той мере, в какой они формируют общую картину.\n");
    prompt.push_str("- Будь ярким, но конкретным. Не заполняй объём абстрактной \"исторической\" водой.\n");
    prompt.push_str("- Каждый абзац должен добавлять новый смысл, основанный на snapshot.\n");
    prompt.push_str("- НЕ повторяй одни и те же эмоции или формулировки.\n\n");
    prompt.push_str("ЗАПРЕЩЕНО:\n");
    prompt.push_str("- Один короткий абзац.\n");
    prompt.push_str("- Слепленный моноблок без структуры.\n");
    prompt.push_str("- Несколько абзацев, которые повторяют одно и то же.\n");
    prompt.push_str("- Перечисление акторов по очереди: \"Венеция сделала X. Генуя сделала Y. Милан сделал Z.\"\n\n");
    prompt.push_str("Помни: хроника — это литература, а не таблица данных или UI-log.\n");

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

    // Split prompt into system and user content
    // System: chronicler persona + factual rules (first two sections before === НАРРАТИВНЫЕ ИНСТРУКЦИИ ===)
    // User: everything else (snapshot, events, instructions)
    let system_end = prompt.find("=== НАРРАТИВНЫЕ ИНСТРУКЦИИ ===").unwrap_or(0);
    let system_content = if system_end > 0 {
        prompt[..system_end].trim().to_string()
    } else {
        // Fallback: use first 500 chars as system
        prompt.chars().take(500).collect()
    };
    let user_content = if system_end > 0 {
        prompt[system_end..].trim().to_string()
    } else {
        prompt
    };

    let body = serde_json::json!({
        "model": config.model,
        "max_tokens": 4000,
        "system": system_content,
        "messages": [
            {
                "role": "user",
                "content": user_content
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
                        // Anthropic streaming: content_block_delta with delta.text
                        if json["type"] == "content_block_delta" {
                            if let Some(content) = json["delta"]["text"].as_str() {
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
        "max_tokens": 4000,
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
