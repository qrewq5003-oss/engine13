use std::collections::HashMap;

use crate::core::{ActorDelta, EventType, PatronAction, Scenario, WorldState};
use crate::db::Db;
use crate::engine::{calculate_actor_deltas, EventLog};
use crate::llm;
use crate::AppState;

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

/// Check LLM trigger with action info
pub fn check_llm_trigger_with_data(
    world_state: &mut WorldState,
    scenario: &Scenario,
    event_log: &EventLog,
    action_info: Option<(&PatronAction, &HashMap<String, f64>, &HashMap<String, f64>)>,
) -> Option<llm::LlmTrigger> {
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

        return Some(llm::LlmTrigger {
            trigger_type: llm::TriggerType::PlayerAction,
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
            context: llm::LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids,
                recent_events,
                scenario_context: scenario.llm_context.clone(),
                ticks_since_last,
            },
            action_info: Some(llm::ActionInfo {
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
        e.event_type == EventType::Threshold || e.event_type == EventType::Milestone
    });

    if let Some(event) = threshold_event {
        let ticks_since_last = world_state.ticks_since_last_narrative;
        world_state.ticks_since_last_narrative = 0; // Reset counter

        // Determine threshold type
        let threshold_type = if event.event_type == EventType::Milestone {
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

        return Some(llm::LlmTrigger {
            trigger_type: llm::TriggerType::ThresholdEvent,
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
            context: llm::LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids.clone(),
                recent_events,
                scenario_context: scenario.llm_context.clone(),
                ticks_since_last,
            },
            action_info: None,
            threshold_context: Some(llm::ThresholdContext {
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

        return Some(llm::LlmTrigger {
            trigger_type: llm::TriggerType::Time,
            prompt: generate_llm_prompt_with_trigger(
                world_state,
                scenario,
                &narrative_actor_ids,
                &recent_events,
                &actor_deltas,
                ticks_since_last,
                Some(&TriggerDetail::Time),
            ),
            context: llm::LlmContext {
                current_year: world_state.year,
                current_tick: world_state.tick,
                narrative_actors: narrative_actor_ids,
                recent_events,
                scenario_context: scenario.llm_context.clone(),
                ticks_since_last,
            },
            action_info: None,
            threshold_context: None,
            actor_deltas,
        });
    }

    None
}

/// Generate LLM prompt with trigger-specific sections
fn generate_llm_prompt_with_trigger(
    world_state: &WorldState,
    scenario: &Scenario,
    narrative_actor_ids: &[String],
    recent_events: &[String],
    actor_deltas: &[ActorDelta],
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
                actor.get_metric("population") / 1000.0,
                actor.get_metric("military_size") / 1000.0,
                actor.get_metric("military_quality")
            ));
            prompt.push_str(&format!(
                "  economy: {:.0}, cohesion: {:.0}, legitimacy: {:.0}, pressure: {:.0}\n",
                actor.get_metric("economic_output"),
                actor.get_metric("cohesion"),
                actor.get_metric("legitimacy"),
                actor.get_metric("external_pressure")
            ));
            prompt.push_str(&format!(
                "  treasury: {:.0}\n",
                actor.get_metric("treasury")
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
    let prompt = llm::generate_narrative_prompt(&snapshot, scenario, db, &state.narrative_memory);

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
