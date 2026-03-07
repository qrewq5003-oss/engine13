use std::collections::HashMap;

use crate::core::{
    Actor, ActorMetrics, AutoDelta, BorderType, ComparisonOperator, DeltaCondition,
    EventCondition, EventConditionType, MilestoneEvent, Neighbor,
    PatronAction, RankCondition, RankResult, Scenario, Successor,
};

/// Load the Constantinople 1430 scenario
pub fn load_constantinople_1430() -> Scenario {
    eprintln!("[SCENARIO] load_constantinople_1430 - starting");
    let scenario = Scenario {
        id: "constantinople_1430".to_string(),
        label: "Constantinople 1430 — Федерация".to_string(),
        description: "1430 год. Фессалоники пали. Константинополь стоит — но ненадолго.".to_string(),
        start_year: 1430,
        tempo: 0.7,
        tick_span: 1,
        era: crate::core::Era::LateMedieval,
        tick_label: "год".to_string(),
        actors: create_actors(),
        auto_deltas: create_auto_deltas(),
        patron_actions: create_patron_actions(),
        milestone_events: create_milestone_events(),
        rank_conditions: create_rank_conditions(),
        generation_mechanics: None,
        llm_context: create_llm_context(),
        consequence_context: create_consequence_context(),
        player_actor_id: None,
    };
    eprintln!("[SCENARIO] load_constantinople_1430 - loaded {} actors", scenario.actors.len());
    scenario
}

fn create_actors() -> Vec<Actor> {
    vec![
        create_byzantium(),
        create_ottomans(),
        create_venice(),
        create_genoa(),
        create_milan(),
        create_papacy(),
        create_hungary(),
        create_serbia(),
        create_trebizond(),
    ]
}

// ============================================================================
// Actor Definitions
// ============================================================================

fn create_byzantium() -> Actor {
    Actor {
        id: "byzantium".to_string(),
        name: "Византийская Империя".to_string(),
        name_short: "Византия".to_string(),
        region: "thrace".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "orthodoxy".to_string(),
            "greek_culture".to_string(),
            "siege_defense".to_string(),
        ],
        metrics: ActorMetrics {
            population: 50.0,      // ~50k in city
            military_size: 8.0,    // ~8k defenders
            military_quality: 55.0,
            economic_output: 25.0,
            cohesion: 45.0,
            legitimacy: 50.0,
            external_pressure: 60.0, // ottoman siege pressure
            treasury: 80.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "ottomans".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "venice".to_string(), distance: 2, border_type: BorderType::Sea },
            Neighbor { id: "genoa".to_string(), distance: 2, border_type: BorderType::Sea },
            Neighbor { id: "serbia".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "trebizond".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![
            Successor { id: "ottomans".to_string(), weight: 1.0 },
        ],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 41.0, lng: 28.9 }),
        is_successor_template: false,
        religion: crate::core::Religion::Orthodox,
        culture: crate::core::Culture::Greek,
        minimum_survival_ticks: Some(10), // Constantinople holds for at least 10 years
    }
}

fn create_ottomans() -> Actor {
    Actor {
        id: "ottomans".to_string(),
        name: "Османская Империя".to_string(),
        name_short: "Ottoman".to_string(),
        region: "anatolia".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "islam".to_string(),
            "ghazi".to_string(),
            "siege_warfare".to_string(),
            "janissaries".to_string(),
        ],
        metrics: ActorMetrics {
            population: 4000.0,
            military_size: 180.0,
            military_quality: 72.0,
            economic_output: 65.0,
            cohesion: 68.0,
            legitimacy: 75.0,
            external_pressure: 20.0,
            treasury: 400.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "byzantium".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "serbia".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "hungary".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "trebizond".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "venice".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 39.0, lng: 35.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Muslim,
        culture: crate::core::Culture::Turkic,
        minimum_survival_ticks: None,
    }
}

fn create_venice() -> Actor {
    Actor {
        id: "venice".to_string(),
        name: "Венецианская Республика".to_string(),
        name_short: "Венеция".to_string(),
        region: "veneto".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "maritime".to_string(),
            "trade_empire".to_string(),
            "catholic".to_string(),
        ],
        metrics: ActorMetrics {
            population: 180.0,
            military_size: 25.0,
            military_quality: 65.0,
            economic_output: 75.0,
            cohesion: 58.0,
            legitimacy: 70.0,
            external_pressure: 35.0,
            treasury: 600.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "genoa".to_string(), distance: 2, border_type: BorderType::Sea },
            Neighbor { id: "milan".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "papacy".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "hungary".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "byzantium".to_string(), distance: 2, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 45.4, lng: 12.3 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
    }
}

fn create_genoa() -> Actor {
    Actor {
        id: "genoa".to_string(),
        name: "Генуэзская Республика".to_string(),
        name_short: "Генуя".to_string(),
        region: "liguria".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "maritime".to_string(),
            "trade_empire".to_string(),
            "catholic".to_string(),
            "galaata".to_string(),
        ],
        metrics: ActorMetrics {
            population: 120.0,
            military_size: 18.0,
            military_quality: 62.0,
            economic_output: 65.0,
            cohesion: 52.0,
            legitimacy: 62.0,
            external_pressure: 40.0,
            treasury: 450.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "venice".to_string(), distance: 2, border_type: BorderType::Sea },
            Neighbor { id: "milan".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "papacy".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "byzantium".to_string(), distance: 2, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 44.4, lng: 8.9 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
    }
}

fn create_milan() -> Actor {
    Actor {
        id: "milan".to_string(),
        name: "Миланское Герцогство".to_string(),
        name_short: "Милан".to_string(),
        region: "lombardy".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "condottieri".to_string(),
            "catholic".to_string(),
            "banking".to_string(),
        ],
        metrics: ActorMetrics {
            population: 250.0,
            military_size: 35.0,
            military_quality: 68.0,
            economic_output: 70.0,
            cohesion: 55.0,
            legitimacy: 65.0,
            external_pressure: 30.0,
            treasury: 500.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "venice".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "genoa".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "papacy".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 45.5, lng: 9.2 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
    }
}

fn create_papacy() -> Actor {
    Actor {
        id: "papacy".to_string(),
        name: "Папство".to_string(),
        name_short: "Папа".to_string(),
        region: "rome".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "catholic".to_string(),
            "religious_authority".to_string(),
            "crusade_caller".to_string(),
        ],
        metrics: ActorMetrics {
            population: 80.0,
            military_size: 12.0,
            military_quality: 55.0,
            economic_output: 50.0,
            cohesion: 60.0,
            legitimacy: 85.0,
            external_pressure: 25.0,
            treasury: 300.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "venice".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "genoa".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "milan".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "hungary".to_string(), distance: 3, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 41.9, lng: 12.5 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
    }
}

fn create_hungary() -> Actor {
    Actor {
        id: "hungary".to_string(),
        name: "Королевство Венгрия".to_string(),
        name_short: "Венгрия".to_string(),
        region: "pannonia".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "catholic".to_string(),
            "kingdom".to_string(),
            "ottoman_frontier".to_string(),
        ],
        metrics: ActorMetrics {
            population: 800.0,
            military_size: 45.0,
            military_quality: 58.0,
            economic_output: 45.0,
            cohesion: 50.0,
            legitimacy: 62.0,
            external_pressure: 55.0,
            treasury: 200.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "ottomans".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "serbia".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "venice".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "papacy".to_string(), distance: 3, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 47.0, lng: 19.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Germanic,
        minimum_survival_ticks: None,
    }
}

fn create_serbia() -> Actor {
    Actor {
        id: "serbia".to_string(),
        name: "Сербское Деспотство".to_string(),
        name_short: "Сербия".to_string(),
        region: "serbia".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "orthodoxy".to_string(),
            "vassal".to_string(),
            "ottoman_frontier".to_string(),
        ],
        metrics: ActorMetrics {
            population: 300.0,
            military_size: 22.0,
            military_quality: 55.0,
            economic_output: 30.0,
            cohesion: 45.0,
            legitimacy: 52.0,
            external_pressure: 65.0,
            treasury: 100.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "byzantium".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "ottomans".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "hungary".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![
            Successor { id: "ottomans".to_string(), weight: 1.0 },
        ],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 44.0, lng: 21.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Orthodox,
        culture: crate::core::Culture::Slavic,
        minimum_survival_ticks: None,
    }
}

fn create_trebizond() -> Actor {
    Actor {
        id: "trebizond".to_string(),
        name: "Империя Трапезунд".to_string(),
        name_short: "Трапезунд".to_string(),
        region: "pontus".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "orthodoxy".to_string(),
            "greek_culture".to_string(),
            "trade".to_string(),
        ],
        metrics: ActorMetrics {
            population: 100.0,
            military_size: 10.0,
            military_quality: 50.0,
            economic_output: 35.0,
            cohesion: 48.0,
            legitimacy: 55.0,
            external_pressure: 50.0,
            treasury: 120.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "ottomans".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "byzantium".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![
            Successor { id: "ottomans".to_string(), weight: 1.0 },
        ],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 41.0, lng: 39.7 }),
        is_successor_template: false,
        religion: crate::core::Religion::Orthodox,
        culture: crate::core::Culture::Greek,
        minimum_survival_ticks: None,
    }
}

// ============================================================================
// Auto Deltas
// ============================================================================

fn create_auto_deltas() -> Vec<AutoDelta> {
    vec![
        // Actor auto-deltas
        AutoDelta {
            metric: "population".to_string(),
            base: 0.2,
            conditions: vec![
                DeltaCondition { metric: "economic_output".to_string(), operator: ComparisonOperator::Less, value: 20.0, delta: -0.3 },
                DeltaCondition { metric: "external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 70.0, delta: -0.2 },
            ],
            noise: 0.1,
            actor_id: None,
        },
        AutoDelta {
            metric: "military_size".to_string(),
            base: 0.3,
            conditions: vec![
                DeltaCondition { metric: "treasury".to_string(), operator: ComparisonOperator::Less, value: 50.0, delta: -0.5 },
                DeltaCondition { metric: "external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 60.0, delta: 0.4 },
            ],
            noise: 0.2,
            actor_id: None,
        },
        AutoDelta {
            metric: "cohesion".to_string(),
            base: -0.1,
            conditions: vec![
                DeltaCondition { metric: "legitimacy".to_string(), operator: ComparisonOperator::Greater, value: 70.0, delta: 0.2 },
                DeltaCondition { metric: "external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 70.0, delta: -0.3 },
            ],
            noise: 0.15,
            actor_id: None,
        },
        AutoDelta {
            metric: "legitimacy".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "cohesion".to_string(), operator: ComparisonOperator::Greater, value: 60.0, delta: 0.1 },
                DeltaCondition { metric: "treasury".to_string(), operator: ComparisonOperator::Less, value: 0.0, delta: -0.2 },
            ],
            noise: 0.1,
            actor_id: None,
        },
        AutoDelta {
            metric: "external_pressure".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "military_size".to_string(), operator: ComparisonOperator::Less, value: 20.0, delta: 5.0 },
            ],
            noise: 0.3,
            actor_id: None,
        },
        // Ottoman military growth (natural pressure)
        AutoDelta {
            metric: "ottomans.military_size".to_string(),
            base: 0.5,
            conditions: vec![],
            noise: 0.1,
            actor_id: Some("ottomans".to_string()),
        },
        // Byzantium external pressure growth (ottoman siege pressure)
        AutoDelta {
            metric: "byzantium.external_pressure".to_string(),
            base: 2.5,
            conditions: vec![
                // Acceleration if Ottomans are strong
                DeltaCondition { metric: "ottomans.military_size".to_string(), operator: ComparisonOperator::Greater, value: 150.0, delta: 1.5 },
            ],
            noise: 0.5,
            actor_id: Some("byzantium".to_string()),
        },
        // Federation progress auto-deltas
        AutoDelta {
            metric: "federation_progress".to_string(),
            base: 0.0,
            conditions: vec![
                // Venice + Genoa + Milan cooperation bonus
                DeltaCondition { metric: "venice.cohesion".to_string(), operator: ComparisonOperator::Greater, value: 65.0, delta: 0.5 },
                DeltaCondition { metric: "genoa.cohesion".to_string(), operator: ComparisonOperator::Greater, value: 55.0, delta: 0.3 },
                DeltaCondition { metric: "milan.legitimacy".to_string(), operator: ComparisonOperator::Greater, value: 58.0, delta: 0.2 },
                // Ottoman pressure penalty
                DeltaCondition { metric: "byzantium.external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 70.0, delta: -2.0 },
                DeltaCondition { metric: "ottomans.military_size".to_string(), operator: ComparisonOperator::Greater, value: 220.0, delta: -3.0 },
            ],
            noise: 0.2,
            actor_id: None,
        },
    ]
}

// ============================================================================
// Patron Actions
// ============================================================================

fn create_patron_actions() -> Vec<PatronAction> {
    vec![
        // Venice actions (3)
        PatronAction {
            id: "venice_naval_support".to_string(),
            name: "Венецианский флот".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "venice.treasury".to_string(), operator: ComparisonOperator::Greater, value: 100.0 },
            effects: HashMap::from([
                ("byzantium.military_size".to_string(), 5.0),
                ("byzantium.cohesion".to_string(), 3.0),
                ("venice.treasury".to_string(), -50.0),
            ]),
            cost: HashMap::new(),
        },
        PatronAction {
            id: "venice_trade_deal".to_string(),
            name: "Торговая сделка".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "venice.economic_output".to_string(), operator: ComparisonOperator::Greater, value: 60.0 },
            effects: HashMap::from([
                ("byzantium.economic_output".to_string(), 8.0),
                ("federation_progress".to_string(), 3.0),
                ("venice.economic_output".to_string(), -3.0),
            ]),
            cost: HashMap::from([("venice.treasury".to_string(), -20.0)]),
        },
        PatronAction {
            id: "venice_diplomacy".to_string(),
            name: "Венецианская дипломатия".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "venice.legitimacy".to_string(), operator: ComparisonOperator::Greater, value: 60.0 },
            effects: HashMap::from([
                ("federation_progress".to_string(), 5.0),
                ("genoa.cohesion".to_string(), 2.0),
            ]),
            cost: HashMap::from([("venice.treasury".to_string(), -30.0)]),
        },
        // Genoa actions (3)
        PatronAction {
            id: "genoa_galata_garrison".to_string(),
            name: "Гарнизон Галаты".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "genoa.military_size".to_string(), operator: ComparisonOperator::Greater, value: 15.0 },
            effects: HashMap::from([
                ("byzantium.military_size".to_string(), 4.0),
                ("byzantium.military_quality".to_string(), 5.0),
                ("genoa.military_size".to_string(), -3.0),
            ]),
            cost: HashMap::from([("genoa.treasury".to_string(), -30.0)]),
        },
        PatronAction {
            id: "genoa_financial_aid".to_string(),
            name: "Финансовая помощь".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "genoa.treasury".to_string(), operator: ComparisonOperator::Greater, value: 80.0 },
            effects: HashMap::from([
                ("byzantium.treasury".to_string(), 60.0),
                ("federation_progress".to_string(), 4.0),
                ("genoa.treasury".to_string(), -70.0),
            ]),
            cost: HashMap::new(),
        },
        PatronAction {
            id: "genoa_mercenaries".to_string(),
            name: "Генуэзские наёмники".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "genoa.cohesion".to_string(), operator: ComparisonOperator::Greater, value: 50.0 },
            effects: HashMap::from([
                ("byzantium.military_size".to_string(), 6.0),
                ("federation_progress".to_string(), 2.0),
            ]),
            cost: HashMap::from([("genoa.treasury".to_string(), -40.0)]),
        },
        // Milan actions (3)
        PatronAction {
            id: "milan_condottieri".to_string(),
            name: "Кондотьеры Милана".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "milan.treasury".to_string(), operator: ComparisonOperator::Greater, value: 100.0 },
            effects: HashMap::from([
                ("byzantium.military_quality".to_string(), 10.0),
                ("milan.treasury".to_string(), -80.0),
            ]),
            cost: HashMap::new(),
        },
        PatronAction {
            id: "milan_bankers".to_string(),
            name: "Миланские банкиры".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "milan.economic_output".to_string(), operator: ComparisonOperator::Greater, value: 60.0 },
            effects: HashMap::from([
                ("byzantium.treasury".to_string(), 80.0),
                ("federation_progress".to_string(), 3.0),
                ("milan.economic_output".to_string(), -5.0),
            ]),
            cost: HashMap::from([("milan.treasury".to_string(), -40.0)]),
        },
        PatronAction {
            id: "milan_legitimacy".to_string(),
            name: "Миланский престиж".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "milan.legitimacy".to_string(), operator: ComparisonOperator::Greater, value: 60.0 },
            effects: HashMap::from([
                ("byzantium.legitimacy".to_string(), 8.0),
                ("federation_progress".to_string(), 4.0),
            ]),
            cost: HashMap::from([("milan.treasury".to_string(), -25.0), ("milan.legitimacy".to_string(), -5.0)]),
        },
        // Destructive actions (2)
        PatronAction {
            id: "sabotage_federation".to_string(),
            name: "Саботаж федерации".to_string(),
            available_if: crate::core::ActionCondition::Always,
            effects: HashMap::from([
                ("federation_progress".to_string(), -15.0),
                ("venice.cohesion".to_string(), -5.0),
                ("genoa.cohesion".to_string(), -5.0),
            ]),
            cost: HashMap::from([("byzantium.legitimacy".to_string(), -10.0)]),
        },
        PatronAction {
            id: "ottoman_bribe".to_string(),
            name: "Османский подкуп".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "byzantium.treasury".to_string(), operator: ComparisonOperator::Greater, value: 50.0 },
            effects: HashMap::from([
                ("ottomans.external_pressure".to_string(), -10.0),
                ("byzantium.external_pressure".to_string(), -5.0),
                ("federation_progress".to_string(), -10.0),
            ]),
            cost: HashMap::from([("byzantium.treasury".to_string(), -50.0)]),
        },
    ]
}

// ============================================================================
// Milestone Events
// ============================================================================

fn create_milestone_events() -> Vec<MilestoneEvent> {
    vec![
        // Church union
        MilestoneEvent {
            id: "church_union".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "byzantium.legitimacy".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 65.0,
                },
                duration: Some(3),
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Уния церквей подписана. Папа обещает помощь. Православные недовольны.".to_string(),
        },
        // Varna crusade
        MilestoneEvent {
            id: "varna_crusade".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "hungary.military_size".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 60.0,
                },
                duration: Some(2),
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Варненский крестовый поход собран. Венгрия ведёт католиков против осман.".to_string(),
        },
        // Mehmed accelerates due to federation
        MilestoneEvent {
            id: "mehmed_accelerates".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "federation_progress".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 60.0,
                },
                duration: Some(2),
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Мехмед форсирует подготовку. Федерация работает — османы торопятся.".to_string(),
        },
        // Mehmed rises naturally
        MilestoneEvent {
            id: "mehmed_rises".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "ottomans.military_size".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 250.0,
                },
                duration: Some(5),
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Мехмед II восходит на трон. Молодой амбициозный султан.".to_string(),
        },
        // Final assault
        MilestoneEvent {
            id: "final_assault".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "ottomans.military_size".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 280.0,
                },
                duration: Some(3),
            },
            is_key: true,
            triggers_collapse: true,
            llm_context_shift: "Финальный штурм Константинополя начался.".to_string(),
        },
        // Constantinople holds
        MilestoneEvent {
            id: "constantinople_holds".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "byzantium.cohesion".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 70.0,
                },
                duration: Some(5),
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Константинополь выстоял! Город непобедим.".to_string(),
        },
        // Outcome: Best case - byzantium alive AND federation >= 100
        MilestoneEvent {
            id: "outcome_best".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "federation_progress".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::GreaterOrEqual,
                    value: 100.0,
                },
                duration: Some(2),
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Север Италии — новый центр Запада. Черноморская торговля под контролем федерации. Константинополь как протекторат. Венеция, Генуя, Милан выходят из этого сильнее чем вошли.".to_string(),
        },
        // Outcome: Survived alone - byzantium alive but federation < 50
        // This fires when byzantium survives but federation is weak
        MilestoneEvent {
            id: "outcome_survived_alone".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::ActorState {
                    actor_id: "byzantium".to_string(),
                    state: crate::core::ActorState::Alive,
                },
                duration: None,
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Город выстоял — но случайно. Разрозненная помощь, никакой координации. Ottoman отступил но не сломлен. Через десять лет попробует снова.".to_string(),
        },
        // Outcome: Fell with federation - federation >= 80 but byzantium fell
        MilestoneEvent {
            id: "outcome_fell_federation".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "federation_progress".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::GreaterOrEqual,
                    value: 80.0,
                },
                duration: Some(2),
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Константинополь пал. Но федерация выжила. Греческие учёные бегут на север — в Венецию, в Милан. Знания, рукописи, мастера. Ренессанс ускоряется. Север Италии выигрывает от трагедии которую не смог предотвратить.".to_string(),
        },
        // Outcome: Historical - byzantium dead AND federation < 50
        MilestoneEvent {
            id: "outcome_historical".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::ActorState {
                    actor_id: "byzantium".to_string(),
                    state: crate::core::ActorState::Dead,
                },
                duration: None,
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Исторический исход. Город пал. Федерация не сложилась. Ottoman давит на Адриатику. Венеция платит дань. Генуя теряет Галату. Милан смотрит в сторону.".to_string(),
        },
    ]
}

// ============================================================================
// Rank Conditions
// ============================================================================

fn create_rank_conditions() -> Vec<RankCondition> {
    vec![
        // Ottoman growth
        RankCondition {
            region_id: "anatolia".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "ottomans.military_size".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 250.0,
                },
                duration: Some(5),
            },
            result: RankResult { rank: "S".to_string() },
            is_key: true,
        },
        // Venice trade dominance
        RankCondition {
            region_id: "veneto".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "venice.economic_output".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 85.0,
                },
                duration: Some(10),
            },
            result: RankResult { rank: "S".to_string() },
            is_key: false,
        },
    ]
}

// ============================================================================
// LLM Context
// ============================================================================

fn create_llm_context() -> String {
    r#"СЦЕНАРИЙ: Константинополь 1430
НАРРАТИВ: Хроника от третьего лица. Без игрока внутри мира.

КОНТЕКСТ:
1430 год. Фессалоники пали. Константинополь стоит — но ненадолго.
Папство обещает крестовый поход который не придёт.
Франция смотрит в сторону. Англия воюет сама с собой.
Священная Римская империя раздроблена.

Север Италии — Венеция, Генуя, Милан — единственное место
где есть реальные ресурсы, реальный флот, реальные деньги.
Они единственные кто реально может что-то сделать.
Вопрос не в силе — в воле.

Венеция и Генуя — соперники которых объединяет только угроза.
Милан далеко от моря но близко к деньгам.
Папство — катализатор легитимности но не силы.

FEDERATION_PROGRESS (0-100):
0-20: разговоры ни к чему не обязывающие
21-50: первые договорённости, взаимное недоверие
51-80: реальный союз, совместные действия
81-100: федерация — исторически беспрецедентное событие

Федерация ценна сама по себе даже если город падёт —
север Италии выходит из этого процесса новой силой.

ОСМАНСКИЙ ОТВЕТ:
Мехмед не пассивен. Если федерация растёт — он форсирует.
Форсирование это олл ин — риск для обеих сторон.
Слабая армия которая торопится может проиграть городу
который должна была взять легко.

ТОНАЛЬНОСТЬ:
Поздняя Византия. Греческий язык живой.
Православие — идентичность а не просто вера.
Итальянский прагматизм против византийской гордости.
Хроника охватывает весь регион — переговоры, морские сражения,
осадные работы, придворные интриги, бегство учёных.
4-6 абзацев за тик.

НЕ ДЕЛАТЬ:
- Не предрешать падение города
- Не делать османов карикатурными злодеями
- Не игнорировать соперничество внутри коалиции
- Венеция и Генуя не друзья
- Папство важно как легитимность но не как сила"#.to_string()
}

fn create_consequence_context() -> String {
    r#"Сценарный период завершён. Симуляция продолжается.
Константинополь выжил или пал — история продолжается.
Федерация итальянских государств либо сложилась либо распалась.
Османская империя продолжает экспансию или остановлена.
Нарратив охватывает более широкий период истории."#.to_string()
}

/// Calculate dynamic federation weight for an actor
/// Used for federation_progress effects weighting
pub fn federation_weight(actor_id: &str, world_state: &crate::core::WorldState) -> f64 {
    let actor = match world_state.actors.get(actor_id) {
        Some(a) => a,
        None => return 1.0,
    };
    match actor_id {
        "venice" => {
            if actor.metrics.treasury > 1000.0 && actor.metrics.cohesion > 70.0 { 2.0 }
            else if actor.metrics.treasury > 600.0 { 1.5 }
            else { 1.0 }
        }
        "genoa" => {
            if actor.metrics.cohesion > 65.0 && actor.metrics.military_size > 20.0 { 1.5 }
            else if actor.metrics.treasury > 500.0 { 1.0 }
            else { 0.5 }
        }
        "milan" => {
            if actor.metrics.legitimacy > 65.0 && actor.metrics.treasury > 700.0 { 1.5 }
            else if actor.metrics.legitimacy > 55.0 { 1.0 }
            else { 0.5 }
        }
        _ => 1.0,
    }
}
