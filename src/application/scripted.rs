//! Скриптованные стратегии: «сыгранный мир» без человека за рулём.
//!
//! Жили в `src/bin/sim.rs` и потому были недоступны никому, кроме самого `sim`: ни
//! один тест и ни одна проба не могли поднять мир, в котором действия вообще
//! применяются. Это делало правило «мерить по обоим мирам» неисполнимым для
//! распределений — обзорная пачка пишет выборку из 2–4 тиков, а не ряд.
//!
//! Перенос — чистый: тела `from_str`, `priority_actions` и `name` не тронуты,
//! изменилась только видимость. Вывод `sim` на трёх сценариях побайтово совпадает.
//! См. `docs/investigation_indicator_calibration.md`, пункт A28.

/// Scripted strategy for Constantinople and Rome
pub enum ScriptedStrategy {
    Balanced,
    Diplomacy,
    Military,
    RomeBalanced,
    RomeInfluence,
    RomeWealth,
    MilanAggressive,
}

impl ScriptedStrategy {
    pub fn from_str(s: &str, scenario_id: &str) -> Self {
        // Rome-specific strategies
        if scenario_id == "rome_375" {
            match s.to_lowercase().as_str() {
                "influence" | "influence_heavy" => ScriptedStrategy::RomeInfluence,
                "wealth" | "wealth_heavy" => ScriptedStrategy::RomeWealth,
                _ => ScriptedStrategy::RomeBalanced,
            }
        } else if scenario_id == "milan_1477" {
            // Milan 1477 - only one strategy defined so far (aggressive/expansionist),
            // used to test whether military_size can grow at all under real
            // player-like action-taking (see ENGINE13_SCENARIO3_DESIGN.md,
            // "Найдено при плейтесте C/D" for why this was needed).
            ScriptedStrategy::MilanAggressive
        } else {
            // Constantinople strategies
            match s.to_lowercase().as_str() {
                "diplomacy" | "diplomatic" => ScriptedStrategy::Diplomacy,
                "military" | "military_heavy" => ScriptedStrategy::Military,
                _ => ScriptedStrategy::Balanced,
            }
        }
    }
    
    pub fn priority_actions(&self) -> Vec<&'static str> {
        match self {
            // Constantinople strategies
            // `milan_legitimacy` (A38, owner's decision): the one action that raises Byzantium's
            // legitimacy — second in balanced and diplomacy, last in military. A player would
            // press it as soon as it is available; an honest bot plays the full set.
            ScriptedStrategy::Balanced => vec![
                "venice_diplomacy",
                "milan_legitimacy",
                "genoa_financial_aid",
                "milan_bankers",
                "venice_naval_support",
                "genoa_mercenaries",
                "milan_condottieri",
                "venice_trade_deal",
                "genoa_galata_garrison",
            ],
            ScriptedStrategy::Diplomacy => vec![
                "venice_diplomacy",
                "milan_legitimacy",
                "genoa_financial_aid",
                "milan_bankers",
                "venice_trade_deal",
                "genoa_galata_garrison",
                "venice_naval_support",
                "genoa_mercenaries",
                "milan_condottieri",
            ],
            ScriptedStrategy::Military => vec![
                "venice_naval_support",
                "genoa_mercenaries",
                "milan_condottieri",
                "genoa_galata_garrison",
                "venice_diplomacy",
                "genoa_financial_aid",
                "milan_bankers",
                "venice_trade_deal",
                "milan_legitimacy",
            ],
            // Rome strategies - using actual IDs from rome_375.rs
            // Note: Many actions have availability gates (e.g., family_wealth > 10)
            // Only gather_information and lay_low are available unconditionally on tick 0
            // New priority: exit resource loop early, get to outcome actions
            ScriptedStrategy::RomeBalanced => vec![
                "expand_network",      // FIRST: get connections for build_reputation (uses starting wealth 50)
                "build_reputation",    // PRIMARY: convert to influence (needs connections > 15)
                "support_city",        // SECONDARY: influence + cohesion (needs wealth > 15)
                "back_administration", // TERTIARY: legitimacy + more connections
                "fund_defense",        // LATE: influence + military_quality
                "lay_low",             // Only when influence is high enough to spare
                "invest_wealth",       // Only when connections are high
                "gather_information",  // Only when wealth is high (knowledge has legitimacy bridge now)
                "educate_family",      // Lowest priority: knowledge has no direct sink
            ],
            ScriptedStrategy::RomeInfluence => vec![
                "build_reputation",    // Priority: influence-focused
                "support_city",
                "fund_defense",
                "back_administration",
                "expand_network",
                "educate_family",
                "invest_wealth",
                "gather_information",
                "lay_low",
            ],
            ScriptedStrategy::RomeWealth => vec![
                "lay_low",             // Priority: wealth accumulation first
                "invest_wealth",
                "gather_information",
                "expand_network",
                "educate_family",
                "support_city",
                "back_administration",
                "build_reputation",
                "fund_defense",
            ],
            // Milan 1477 - aggressive/expansionist: raise troops (military_size),
            // pressure neighbours, destabilize Naples, hire condottieri
            // (military_quality), keep treasury flowing to afford gated actions.
            ScriptedStrategy::MilanAggressive => vec![
                "milan_raise_troops",
                "milan_pressure_genoa",
                "incite_baronial_revolt",
                "milan_hire_condottieri",
                "milan_hire_urbino_condottieri",
                "milan_lease_genoese_fleet",
                "milan_banking_deal_florence",
                "milan_bribe_curia",
                "milan_court_patronage",
                "milan_diplomacy_ferrara",
                "milan_marriage_venice",
                "milan_marriage_naples",
                "call_papal_arbitration",
                "milan_savoy_alliance",
            ],
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ScriptedStrategy::Balanced => "balanced",
            ScriptedStrategy::Diplomacy => "diplomacy",
            ScriptedStrategy::Military => "military",
            ScriptedStrategy::RomeBalanced => "balanced",
            ScriptedStrategy::RomeInfluence => "influence",
            ScriptedStrategy::RomeWealth => "wealth",
            ScriptedStrategy::MilanAggressive => "aggressive",
        }
    }
}

/// What one scripted turn did: the actions applied, in order, and how many were refused.
#[derive(Debug, Clone, Default)]
pub struct ScriptedTurn {
    pub applied: Vec<&'static str>,
    pub rejected: u32,
}

/// The treasury milan keeps back for `milan_raise_troops` — that action's own gate.
const MILAN_RAISE_TROOPS_GATE: f64 = 70.0;

/// Spends one turn the way the scripted player does, through the same path as the UI
/// (`apply_player_action`). The whole policy lives here (A29) — priority order, the
/// `actions_per_tick` cap, milan's reserve discipline — so a test or a probe that needs
/// the played world calls this instead of copying `sim.rs`. Moved from `sim.rs`
/// verbatim; `sim` output is byte-identical.
///
/// Milan: `milan_raise_troops` (the only `military_size` growth lever) always has first
/// claim on the treasury; everything else is funded only from the surplus above that
/// action's own gate, never at its expense. See ENGINE13_SCENARIO3_DESIGN.md, «Найдено
/// при плейтесте C/D» — the naive list (spend on everything every tick) couldn't sustain
/// growth.
pub fn apply_scripted_actions(
    state: &mut crate::commands::AppState,
    strategy: &ScriptedStrategy,
) -> ScriptedTurn {
    apply_scripted_priorities(state, &strategy.priority_actions())
}

/// [`apply_scripted_actions`] over an explicit priority list — the same policy, so a probe
/// can try a strategy changed in memory (A38: `milan_legitimacy` added) without copying the
/// loop (A29).
pub fn apply_scripted_priorities(
    state: &mut crate::commands::AppState,
    priority_actions: &[&'static str],
) -> ScriptedTurn {
    use crate::application::actions::{apply_player_action, PlayerActionInput};

    let scenario = state.current_scenario.as_ref().expect("scenario").clone();
    let mut turn = ScriptedTurn::default();
    let milan_treasury = |state: &crate::commands::AppState| {
        state.world_state.as_ref().unwrap()
            .actors.get("milan").map(|a| a.get_metric("treasury")).unwrap_or(0.0)
    };

    if scenario.id == "milan_1477" {
        let raise_input = PlayerActionInput {
            action_id: "milan_raise_troops".to_string(),
            target_actor_id: None,
        };
        if milan_treasury(state) > MILAN_RAISE_TROOPS_GATE {
            match apply_player_action(state, &raise_input) {
                Ok(_) => turn.applied.push("milan_raise_troops"),
                Err(_) => turn.rejected += 1,
            }
        }

        for action_id in priority_actions.iter().filter(|id| **id != "milan_raise_troops") {
            if turn.applied.len() as u32 >= scenario.actions_per_tick {
                break;
            }
            let surplus = milan_treasury(state) - MILAN_RAISE_TROOPS_GATE;
            if surplus <= 0.0 {
                break; // preserve the reserve - no discretionary spend below it
            }
            let cost = scenario.patron_actions.iter()
                .find(|a| a.id == *action_id)
                .and_then(|a| a.cost.get(&crate::core::MetricRef::literal("actor:milan.treasury")))
                .map(|c| -c) // cost values are negative deltas
                .unwrap_or(f64::MAX);
            if cost > surplus {
                continue; // can't afford this one without dipping into the reserve
            }
            let action_input = PlayerActionInput {
                action_id: action_id.to_string(),
                target_actor_id: None,
            };
            match apply_player_action(state, &action_input) {
                Ok(_) => turn.applied.push(action_id),
                Err(_) => turn.rejected += 1,
            }
        }
    } else {
        for action_id in priority_actions {
            if turn.applied.len() as u32 >= scenario.actions_per_tick {
                break;
            }
            let action_input = PlayerActionInput {
                action_id: action_id.to_string(),
                target_actor_id: None,
            };
            match apply_player_action(state, &action_input) {
                Ok(_) => turn.applied.push(action_id),
                Err(_) => turn.rejected += 1,
            }
        }
    }
    turn
}

/// One played half-year: the scripted turn, then the engine tick — the order `sim` and
/// the UI both use (actions are stamped with the tick they precede).
pub fn play_scripted_tick(
    state: &mut crate::commands::AppState,
    strategy: &ScriptedStrategy,
) -> ScriptedTurn {
    play_scripted_priorities_tick(state, &strategy.priority_actions())
}

/// [`play_scripted_tick`] over an explicit priority list (see [`apply_scripted_priorities`]).
pub fn play_scripted_priorities_tick(
    state: &mut crate::commands::AppState,
    priority_actions: &[&'static str],
) -> ScriptedTurn {
    let turn = apply_scripted_priorities(state, priority_actions);
    let ws = state.world_state.as_mut().expect("world");
    let sc = state.current_scenario.as_ref().expect("scenario");
    crate::engine::tick(ws, sc, &mut state.event_log, state.rng.as_mut().expect("rng"));
    turn
}
