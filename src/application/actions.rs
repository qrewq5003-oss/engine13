use std::collections::HashMap;

use crate::core::{MetricRef, PatronAction, WorldState};
use crate::db::Db;
use crate::engine::EventLog;
use crate::AppState;

/// Input for player action
#[derive(Debug, Clone)]
pub struct PlayerActionInput {
    pub action_id: String,
    pub target_actor_id: Option<String>,
}

/// Apply player action - unified for all scenarios via MetricRef
pub fn apply_player_action(
    state: &mut AppState,
    action_input: &PlayerActionInput,
) -> Result<(HashMap<String, f64>, HashMap<String, f64>), String> {
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

    // Apply effects with global metric weights from scenario
    let mut applied_effects = HashMap::new();
    for (metric, effect) in &action.effects {
        let metric_ref = MetricRef::parse(metric);
        
        // Get weight from scenario.global_metric_weights
        let weight = scenario.global_metric_weights
            .get(metric)
            .and_then(|weights| {
                action.source_actor_id.as_deref()
                    .and_then(|source| weights.get(source))
            })
            .copied()
            .unwrap_or(1.0);
        
        let weighted_effect = effect * weight;
        metric_ref.apply(world_state, weighted_effect);
        applied_effects.insert(metric.clone(), weighted_effect);
    }

    eprintln!("[DEBUG] apply_player_action - applied_effects: {:?}", applied_effects);
    eprintln!("[DEBUG] apply_player_action - family_metrics after effects: {:?}", world_state.family_metrics);

    // Record event - use first foreground actor or default
    let event_actor = world_state.actors.values()
        .find(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground)
        .map(|a| a.id.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let event = crate::core::Event::new(
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

/// Unified action availability check - works for all scenarios via MetricRef
fn is_action_available(action: &PatronAction, world_state: &WorldState) -> bool {
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

/// Get universal actions available in Consequences and Free modes
pub fn get_universal_actions(_world_state: &WorldState) -> Vec<PatronAction> {
    use crate::core::{ActionCondition, ComparisonOperator};

    let mut actions = Vec::new();

    // 1. Observe - always available, no effects, no cost
    actions.push(PatronAction {
        id: "observe".to_string(),
        name: "Наблюдать".to_string(),
        source_actor_id: None,
        available_if: ActionCondition::Always,
        effects: HashMap::new(),
        cost: HashMap::new(),
    });

    // 2. Support Stability - requires treasury > 50
    let mut support_effects = HashMap::new();
    support_effects.insert("family_cohesion".to_string(), 3.0);
    support_effects.insert("family_legitimacy".to_string(), 2.0);
    let mut support_cost = HashMap::new();
    support_cost.insert("treasury".to_string(), -50.0);

    actions.push(PatronAction {
        id: "support_stability".to_string(),
        name: "Поддержать стабильность".to_string(),
        source_actor_id: None,
        available_if: ActionCondition::Metric {
            metric: "treasury".to_string(),
            operator: ComparisonOperator::Greater,
            value: 50.0,
        },
        effects: support_effects,
        cost: support_cost,
    });

    // 3. Raise Taxes - always available
    let mut taxes_effects = HashMap::new();
    taxes_effects.insert("treasury".to_string(), 80.0);
    taxes_effects.insert("family_cohesion".to_string(), -3.0);
    taxes_effects.insert("family_legitimacy".to_string(), -5.0);

    actions.push(PatronAction {
        id: "raise_taxes".to_string(),
        name: "Повысить налоги".to_string(),
        source_actor_id: None,
        available_if: ActionCondition::Always,
        effects: taxes_effects,
        cost: HashMap::new(),
    });

    // 4. Recruit Soldiers - requires treasury > 100
    let mut recruit_effects = HashMap::new();
    recruit_effects.insert("rome.military_size".to_string(), 10.0);
    recruit_effects.insert("rome.military_quality".to_string(), -5.0);
    let mut recruit_cost = HashMap::new();
    recruit_cost.insert("treasury".to_string(), -100.0);

    actions.push(PatronAction {
        id: "recruit_soldiers".to_string(),
        name: "Нанять солдат".to_string(),
        source_actor_id: None,
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

/// Get available actions for the player
pub fn get_available_actions(state: &AppState) -> Result<Vec<PatronAction>, String> {
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

/// Submit a player action - applies effects/costs WITHOUT advancing tick
pub fn submit_action(state: &mut AppState, action_input: PlayerActionInput) -> Result<crate::commands::SubmitActionResponse, String> {
    let (effects, costs) = apply_player_action(state, &action_input)?;

    // Note: We do NOT call tick() here - action application is separate from time advancement
    let world_state = state.world_state.as_ref().ok_or("No active world state")?;

    Ok(crate::commands::SubmitActionResponse {
        success: true,
        effects,
        costs,
        new_state: world_state.clone(),
        llm_trigger: None,
        error: None,
    })
}
