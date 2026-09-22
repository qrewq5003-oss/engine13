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
            ScriptedStrategy::Balanced => vec![
                "venice_diplomacy",
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
