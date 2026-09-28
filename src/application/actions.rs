use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::core::{ComparisonOperator, Condition, PatronAction, Scenario, WorldState};
use crate::AppState;

/// Reason why an action is unavailable - runtime check result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UnavailableReason {
    InsufficientCost { required: f64, available: f64, resource: String },
    ActionsPerTickExhausted { limit: u32 },
    ConditionNotMet { description: String },
}

/// Action info with availability status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    pub action: PatronAction,
    pub available: bool,
    pub unavailable_reason: Option<UnavailableReason>,
}

/// Input for player action
#[derive(Debug, Clone)]
pub struct PlayerActionInput {
    pub action_id: String,
    pub target_actor_id: Option<String>,
}

/// (effects, costs) metric deltas applied by a player action
pub type ActionMetricDeltas = (HashMap<String, f64>, HashMap<String, f64>);

/// Apply player action - unified for all scenarios via MetricRef
pub fn apply_player_action(
    state: &mut AppState,
    action_input: &PlayerActionInput,
) -> Result<ActionMetricDeltas, String> {
    let scenario = state.current_scenario.as_ref().ok_or("No active scenario")?;
    let world_state = state.world_state.as_mut().ok_or("No active world state")?;

    let action = scenario.patron_actions.iter()
        .find(|a| a.id == action_input.action_id)
        .ok_or_else(|| format!("Action '{}' not found", action_input.action_id))?
        .clone();

    // The same rule the UI's list draws (B42): per-tick limit, condition, cost. The
    // cost used to be checked by the list only, so the scripted player — which goes
    // through this path — bought what a human could not click.
    if let Err(reason) = action_availability(&action, world_state, scenario) {
        return Err(describe_unavailable(&reason));
    }

    // Apply cost
    let mut applied_costs = HashMap::new();
    for (metric, cost) in &action.cost {
        metric.apply(world_state, *cost);
        applied_costs.insert(metric.to_string(), *cost);
    }

    // Apply effects with global metric weights from scenario
    let mut applied_effects = HashMap::new();
    for (metric, effect) in &action.effects {
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
        metric.apply(world_state, weighted_effect);
        applied_effects.insert(metric.to_string(), weighted_effect);
    }

    // Record event — attributed to the scenario's own player actor.
    //
    // It used to be "the first foreground actor", which meant the Huns in rome and
    // Florence in milan: a hostile steppe confederation and a rival city carrying the
    // player's deeds. (Before PR #69 "first" also meant "first in the HashMap", so the
    // attribution moved between runs; sorting made it reproducible but no less
    // arbitrary.) `scenario.player_actor_id` states the answer and was itself read by
    // nobody — the fourth authored field found dead in this cycle.
    //
    // The attribution is not cosmetic: `db::select_relevant_events` promotes `is_key`
    // events **of narrative actors** into the chronicler's recent-events block, so it
    // decided whether a player action was shown as part of some actor's story — and
    // stopped promoting it once that accidental actor died.
    //
    // `None` means the player has no actor in this scenario (constantinople: the
    // player is the coalition). Then the event belongs to the scenario, the same
    // literal the mode-change and milestone events use — and it is right that no
    // actor's story absorbs it. The action still reaches the prompt through its own
    // "ДЕЙСТВИЯ ИГРОКА" block, which never read the attribution.
    // See docs/investigation_player_action_attribution.md.
    let event_actor = scenario
        .player_actor_id
        .clone()
        .unwrap_or_else(|| "scenario".to_string());

    // Serialize effects to metadata for action history
    // Keys sorted (B9′): serialized straight from the `HashMap`, the string's key order
    // followed the per-instance hash key — four different strings for one action over
    // sixteen loads of rome's `support_city`. The log is saved with the game (B31).
    let effects_json = serde_json::to_string(
        &applied_effects.iter().collect::<std::collections::BTreeMap<_, _>>(),
    )
    .unwrap_or_default();

    let event = crate::core::Event::new(
        format!("player_action_{}", action_input.action_id),
        world_state.tick,
        world_state.year,
        event_actor,
        crate::core::EventType::PlayerAction,
        true,
        format!("Действие игрока: {}", action.name),
    )
    .with_metadata(effects_json);
    state.event_log.add(event);

    // Increment actions counter
    world_state.actions_this_tick += 1;

    Ok((applied_effects, applied_costs))
}

/// The one availability rule (B42): the UI's list and the apply path both read it, so
/// the list shows exactly the reason the model would refuse. Order: per-tick limit,
/// authored condition, cost (a negative cost the actor cannot cover).
pub fn action_availability(
    action: &PatronAction,
    world_state: &WorldState,
    scenario: &Scenario,
) -> Result<(), UnavailableReason> {
    if scenario.actions_per_tick > 0 && world_state.actions_this_tick >= scenario.actions_per_tick {
        return Err(UnavailableReason::ActionsPerTickExhausted { limit: scenario.actions_per_tick });
    }
    if let crate::core::ActionCondition::Metric { metric, operator, value } = &action.available_if {
        if !compare_value(metric.get(world_state), operator, value) {
            return Err(UnavailableReason::ConditionNotMet {
                description: describe_condition(&Condition {
                    metric: metric.clone(),
                    operator: operator.clone(),
                    value: *value,
                }),
            });
        }
    }
    let mut costs: Vec<(&crate::core::MetricRef, &f64)> = action.cost.iter().collect();
    costs.sort_by_key(|(m, _)| m.to_string());
    for (metric, cost) in costs {
        let current = metric.get(world_state);
        if current < cost.abs() && *cost < 0.0 {
            // Human-readable resource name for the UI ("venice treasury").
            let key = metric.to_string();
            let resource = key.strip_prefix("actor:").unwrap_or(&key).replace(['.', '_'], " ");
            return Err(UnavailableReason::InsufficientCost { required: cost.abs(), available: current, resource });
        }
    }
    Ok(())
}

/// The refusal text the apply path returns for a reason from [`action_availability`].
fn describe_unavailable(reason: &UnavailableReason) -> String {
    match reason {
        UnavailableReason::ActionsPerTickExhausted { limit } => format!("Достигнут лимит действий за тик: {limit}/{limit}"),
        UnavailableReason::ConditionNotMet { description } => description.clone(),
        UnavailableReason::InsufficientCost { required, available, resource } => {
            format!("Недостаточно ресурса «{resource}»: нужно {required}, есть {available}")
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

/// Describe a condition in human-readable form
fn describe_condition(cond: &Condition) -> String {
    let metric = &cond.metric;
    let op_str = match cond.operator {
        ComparisonOperator::Less => "<",
        ComparisonOperator::LessOrEqual => "<=",
        ComparisonOperator::Greater => ">",
        ComparisonOperator::GreaterOrEqual => ">=",
        ComparisonOperator::Equal => "==",
    };
    
    // Extract resource name from metric (e.g., "actor:venice.treasury" -> "Venice treasury")
    let key = metric.to_string();
    let resource = key
        .strip_prefix("actor:")
        .unwrap_or(&key)
        .replace(['.', '_'], " ");
    
    // Capitalize first letter
    let mut chars = resource.chars();
    let resource = match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    };
    
    format!("Требует: {} {} {}", resource, op_str, cond.value)
}

/// List all actions with availability status and reasons
pub fn list_actions_with_availability(
    world_state: &WorldState,
    scenario: &Scenario,
) -> Vec<ActionInfo> {
    let mut actions = Vec::new();

    for action in &scenario.patron_actions {
        let (available, unavailable_reason) = match action_availability(action, world_state, scenario) {
            Ok(()) => (true, None),
            Err(reason) => (false, Some(reason)),
        };

        actions.push(ActionInfo {
            action: action.clone(),
            available,
            unavailable_reason,
        });
    }

    actions
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
        error: None,
    })
}

#[cfg(test)]
mod attribution_tests {
    use super::*;

    /// A player action belongs to the scenario's own player actor — not to whichever
    /// foreground actor happens to sort first. Before this rule rome attributed the
    /// player's deeds to `huns` and milan to `florence`
    /// (docs/investigation_player_action_attribution.md).
    #[test]
    fn player_action_is_attributed_to_the_scenarios_player_actor() {
        for (id, expected) in [
            ("rome_375", "rome"),
            ("milan_1477", "milan"),
            ("constantinople_1430", "scenario"),
        ] {
            let scenario = crate::scenarios::registry::load_by_id(id).expect("scenario");
            let actor = scenario
                .player_actor_id
                .clone()
                .unwrap_or_else(|| "scenario".to_string());
            assert_eq!(actor, expected, "{id}");
        }
    }
}
