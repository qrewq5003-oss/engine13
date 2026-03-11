use std::collections::HashMap;

use serde::Deserialize;

use crate::core::{
    Actor, AutoDelta, BorderType, ComparisonOperator,
    DependencyRule, EventCondition, EventConditionType, MapConfig, MilestoneEvent, Neighbor,
    PatronAction, RankBonusRule, RankCondition, RankResult, Scenario, Successor,
};

/// Dependencies file structure for TOML deserialization
#[derive(Deserialize)]
struct DependenciesFile {
    dependencies: Vec<DependencyRule>,
}

/// Actions file structure for TOML deserialization
#[derive(Deserialize)]
struct ActionsFile {
    patron_actions: Vec<PatronAction>,
    #[serde(default)]
    universal_actions: Vec<PatronAction>,
}

/// Rank bonuses file structure for TOML deserialization
#[derive(Deserialize)]
struct RankBonusesFile {
    rank_bonuses: Vec<RankBonusRule>,
}

/// Map config file structure for TOML deserialization
#[derive(Deserialize)]
struct MapFile {
    map: MapConfig,
}

/// Auto deltas file structure for TOML deserialization
#[derive(Deserialize)]
struct AutoDeltasFile {
    auto_deltas: Vec<AutoDelta>,
}

/// Milestone events file structure for TOML deserialization
#[derive(Deserialize)]
struct MilestoneEventsFile {
    milestone_events: Vec<MilestoneEvent>,
}

/// Known metrics for validation
const KNOWN_METRICS: &[&str] = &[
    "population",
    "military_size",
    "military_quality",
    "economic_output",
    "cohesion",
    "legitimacy",
    "external_pressure",
    "treasury",
    "global:federation_progress",
];

/// Known actor IDs for map validation
const KNOWN_ACTOR_IDS: &[&str] = &[
    "byzantium", "ottomans", "venice", "genoa", "milan",
    "papacy", "hungary", "serbia", "trebizond",
];

/// Load dependencies from TOML file
fn load_dependencies() -> Vec<DependencyRule> {
    let deps_file: DependenciesFile = toml::from_str(
        include_str!("constantinople_1430/dependencies.toml")
    ).expect("constantinople_1430/dependencies.toml parse error");

    crate::engine::validate_dependencies(&deps_file.dependencies, KNOWN_METRICS);

    deps_file.dependencies
}

/// Load actions from TOML file
fn load_actions() -> (Vec<PatronAction>, Vec<PatronAction>) {
    let actions_file: ActionsFile = toml::from_str(
        include_str!("constantinople_1430/actions.toml")
    ).expect("constantinople_1430/actions.toml parse error");

    crate::core::validate_patron_actions(&actions_file.patron_actions, KNOWN_METRICS);

    (actions_file.patron_actions, actions_file.universal_actions)
}

/// Load rank bonuses from TOML file
fn load_rank_bonuses() -> Vec<RankBonusRule> {
    let rank_file: RankBonusesFile = toml::from_str(
        include_str!("constantinople_1430/rank_bonuses.toml")
    ).expect("constantinople_1430/rank_bonuses.toml parse error");

    rank_file.rank_bonuses
}

/// Load map config from TOML file
fn load_map_config() -> Option<MapConfig> {
    let map_file: MapFile = toml::from_str(
        include_str!("constantinople_1430/map.toml")
    ).expect("constantinople_1430/map.toml parse error");

    crate::core::validate_map_config(&map_file.map, KNOWN_ACTOR_IDS);

    Some(map_file.map)
}

/// Load auto deltas from TOML file
fn load_auto_deltas() -> Vec<AutoDelta> {
    let file: AutoDeltasFile = toml::from_str(
        include_str!("constantinople_1430/auto_deltas.toml")
    ).expect("constantinople_1430/auto_deltas.toml parse error");
    file.auto_deltas
}

/// Load milestone events from TOML file
fn load_milestone_events() -> Vec<MilestoneEvent> {
    let file: MilestoneEventsFile = toml::from_str(
        include_str!("constantinople_1430/milestone_events.toml")
    ).expect("constantinople_1430/milestone_events.toml parse error");
    file.milestone_events
}

/// Load the Constantinople 1430 scenario
pub fn load_constantinople_1430() -> Scenario {
    eprintln!("[SCENARIO] load_constantinople_1430 - starting");
    let dependencies = load_dependencies();
    let (patron_actions, universal_actions) = load_actions();
    let rank_bonuses = load_rank_bonuses();
    let map = load_map_config();

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
        auto_deltas: load_auto_deltas(),
        milestone_events: load_milestone_events(),
        rank_conditions: create_rank_conditions(),
        generation_mechanics: None,
        llm_context: create_llm_context(),
        consequence_context: create_consequence_context(),
        player_actor_id: None,
        status_indicators: create_status_indicators(),
        global_metric_weights: HashMap::from([
            ("global:federation_progress".to_string(), HashMap::from([
                ("venice".to_string(), 2.0),
                ("genoa".to_string(), 1.5),
                ("milan".to_string(), 1.0),
            ])),
        ]),
        features: crate::core::ScenarioFeatures {
            family_panel: false,
            global_metrics_panel: true,
            patron_actions: true,
        },
        military_conflict_probability: 0.35,
        naval_conflict_probability: 0.20,
        random_events: create_random_events(),
        generation_length: None,
        actions_per_tick: 3,
        victory_condition: Some(crate::core::VictoryCondition {
            metric: "global:federation_progress".to_string(),
            threshold: 80.0,
            title: "Федерация Севера основана".to_string(),
            description: "Торговые республики объединились. Константинополь получил шанс на спасение.".to_string(),
            minimum_tick: 40,  // 20 years × 2 ticks/year
            additional_conditions: vec![
                crate::core::Condition {
                    metric: "actor:byzantium.external_pressure".to_string(),
                    operator: crate::core::ComparisonOperator::Less,
                    value: 85.0,
                },
            ],
            sustained_ticks_required: 3,
        }),
        global_metrics_display: vec![
            crate::core::MetricDisplay {
                metric: "global:federation_progress".to_string(),
                label: "Прогресс федерации".to_string(),
                panel_title: "Федерация".to_string(),
                thresholds: vec![
                    crate::core::MetricThreshold { below: 20.0, text: "Разговоры ни к чему не обязывающие".to_string() },
                    crate::core::MetricThreshold { below: 50.0, text: "Первые договорённости, взаимное недоверие".to_string() },
                    crate::core::MetricThreshold { below: 80.0, text: "Реальный союз, совместные действия".to_string() },
                    crate::core::MetricThreshold { below: 101.0, text: "Федерация — исторически беспрецедентное событие".to_string() },
                ],
            },
        ],
        initial_family_metrics: None,
        max_random_events_per_tick: 3,
        narrative_config: crate::core::NarrativeConfig {
            key_metrics: vec![
                "federation_progress".to_string(),
                "byzantium.external_pressure".to_string(),
                "byzantium.legitimacy".to_string(),
                "byzantium.cohesion".to_string(),
                "ottomans.military_size".to_string(),
            ],
            narrative_axes: vec![
                "survival vs surrender".to_string(),
                "unity vs fragmentation".to_string(),
                "faith vs pragmatism".to_string(),
            ],
            tone_tags: vec![
                "formal chronicle".to_string(),
                "epic scope".to_string(),
                "tragic grandeur".to_string(),
            ],
            forbidden_claims: vec![
                "Do not claim Byzantium has fallen unless byzantium is in dead_actors".to_string(),
                "Do not claim victory has been achieved unless victory_achieved is true".to_string(),
                "Do not mention specific numbers, percentages, or game metrics".to_string(),
                "Do not claim the Ottomans have won unless the scenario explicitly states so".to_string(),
            ],
            paragraph_target: 6,
            output_length_hint: "detailed half-year chronicle, 6-8 paragraphs".to_string(),
        },
        dependencies,
        patron_actions,
        universal_actions,
        interaction_rules: vec![],
        rank_bonuses,
        map,
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
        metrics: HashMap::from([
            ("population".to_string(), 50.0),
            ("military_size".to_string(), 8.0),
            ("military_quality".to_string(), 55.0),
            ("economic_output".to_string(), 25.0),
            ("cohesion".to_string(), 45.0),
            ("legitimacy".to_string(), 50.0),
            ("external_pressure".to_string(), 60.0),
            ("treasury".to_string(), 80.0)
        ]),
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
        minimum_survival_ticks: None,
        leader: Some("Иоанн VIII Палеолог".to_string()),
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
        metrics: HashMap::from([
            ("population".to_string(), 4000.0),
            ("military_size".to_string(), 180.0),
            ("military_quality".to_string(), 72.0),
            ("economic_output".to_string(), 65.0),
            ("cohesion".to_string(), 68.0),
            ("legitimacy".to_string(), 75.0),
            ("external_pressure".to_string(), 20.0),
            ("treasury".to_string(), 400.0)
        ]),
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
        leader: Some("Мурад II".to_string()),
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
        metrics: HashMap::from([
            ("population".to_string(), 180.0),
            ("military_size".to_string(), 25.0),
            ("military_quality".to_string(), 65.0),
            ("economic_output".to_string(), 75.0),
            ("cohesion".to_string(), 58.0),
            ("legitimacy".to_string(), 70.0),
            ("external_pressure".to_string(), 35.0),
            ("treasury".to_string(), 600.0)
        ]),
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
        leader: Some("Дож Франческо Фоскари".to_string()),
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
        metrics: HashMap::from([
            ("population".to_string(), 120.0),
            ("military_size".to_string(), 18.0),
            ("military_quality".to_string(), 62.0),
            ("economic_output".to_string(), 65.0),
            ("cohesion".to_string(), 52.0),
            ("legitimacy".to_string(), 62.0),
            ("external_pressure".to_string(), 40.0),
            ("treasury".to_string(), 450.0)
        ]),
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
        leader: Some("Томмазо Кампофрегозо".to_string()),
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
        metrics: HashMap::from([
            ("population".to_string(), 250.0),
            ("military_size".to_string(), 35.0),
            ("military_quality".to_string(), 68.0),
            ("economic_output".to_string(), 70.0),
            ("cohesion".to_string(), 55.0),
            ("legitimacy".to_string(), 65.0),
            ("external_pressure".to_string(), 30.0),
            ("treasury".to_string(), 500.0)
        ]),
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
        leader: Some("Филиппо Мария Висконти".to_string()),
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
        metrics: HashMap::from([
            ("population".to_string(), 80.0),
            ("military_size".to_string(), 12.0),
            ("military_quality".to_string(), 55.0),
            ("economic_output".to_string(), 50.0),
            ("cohesion".to_string(), 60.0),
            ("legitimacy".to_string(), 85.0),
            ("external_pressure".to_string(), 25.0),
            ("treasury".to_string(), 300.0)
        ]),
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
        leader: Some("Папа Евгений IV".to_string()),
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
        metrics: HashMap::from([
            ("population".to_string(), 800.0),
            ("military_size".to_string(), 45.0),
            ("military_quality".to_string(), 58.0),
            ("economic_output".to_string(), 45.0),
            ("cohesion".to_string(), 50.0),
            ("legitimacy".to_string(), 62.0),
            ("external_pressure".to_string(), 55.0),
            ("treasury".to_string(), 200.0)
        ]),
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
        leader: Some("Янош Хуньяди".to_string()),
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
        metrics: HashMap::from([
            ("population".to_string(), 300.0),
            ("military_size".to_string(), 22.0),
            ("military_quality".to_string(), 55.0),
            ("economic_output".to_string(), 30.0),
            ("cohesion".to_string(), 45.0),
            ("legitimacy".to_string(), 52.0),
            ("external_pressure".to_string(), 65.0),
            ("treasury".to_string(), 100.0)
        ]),
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
        leader: Some("Ђурађ Бранковић".to_string()),
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
        metrics: HashMap::from([
            ("population".to_string(), 100.0),
            ("military_size".to_string(), 10.0),
            ("military_quality".to_string(), 50.0),
            ("economic_output".to_string(), 35.0),
            ("cohesion".to_string(), 48.0),
            ("legitimacy".to_string(), 55.0),
            ("external_pressure".to_string(), 50.0),
            ("treasury".to_string(), 120.0)
        ]),
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
        leader: Some("Иоанн IV Великий Комнин".to_string()),
    }
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
                    metric: "actor:ottomans.military_size".to_string(),
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
                    metric: "actor:venice.economic_output".to_string(),
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

fn create_status_indicators() -> Vec<crate::core::StatusIndicator> {
    use crate::core::StatusIndicator;
    vec![
        StatusIndicator {
            label: "Константинополь".to_string(),
            metric: "actor:byzantium.external_pressure".to_string(),
            invert: true,
            thresholds: vec![
                (0.0, "держится".to_string()),
                (60.0, "под давлением".to_string()),
                (80.0, "критическое положение".to_string()),
            ],
        },
        StatusIndicator {
            label: "Федерация".to_string(),
            metric: "global:federation_progress".to_string(),
            invert: false,
            thresholds: vec![
                (0.0, "не сформирована".to_string()),
                (30.0, "формируется".to_string()),
                (60.0, "укрепляется".to_string()),
                (80.0, "готова".to_string()),
            ],
        },
        StatusIndicator {
            label: "Османская угроза".to_string(),
            metric: "actor:ottomans.military_size".to_string(),
            invert: true,
            thresholds: vec![
                (0.0, "сдержана".to_string()),
                (150.0, "нарастает".to_string()),
                (200.0, "готова к штурму".to_string()),
            ],
        },
    ]
}

fn create_random_events() -> Vec<crate::core::RandomEvent> {
    use crate::core::{Condition, EventTarget, ComparisonOperator, RandomEvent};
    use std::collections::HashMap;

    vec![
        RandomEvent {
            id: "cardinal_death".to_string(),
            probability: 0.06,
            target: EventTarget::Actor("papacy".to_string()),
            conditions: vec![],
            effects: HashMap::from([
                ("global:federation_progress".to_string(), -8.0),
                ("actor:papacy.legitimacy".to_string(), -5.0),
            ]),
            llm_context: "Смерть кардинала сорвала переговоры о федерации".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "ottoman_embassy".to_string(),
            probability: 0.08,
            target: EventTarget::Actor("byzantium".to_string()),
            conditions: vec![
                Condition { metric: "actor:byzantium.external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 60.0 },
            ],
            effects: HashMap::from([
                ("actor:byzantium.legitimacy".to_string(), -8.0),
                ("actor:byzantium.treasury".to_string(), -150.0),
            ]),
            llm_context: "Османское посольство потребовало унизительной дани".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "genoese_bankers".to_string(),
            probability: 0.07,
            target: EventTarget::Actor("genoa".to_string()),
            conditions: vec![],
            effects: HashMap::from([
                ("actor:genoa.treasury".to_string(), 200.0),
                ("global:federation_progress".to_string(), 3.0),
            ]),
            llm_context: "Генуэзские банкиры выделили займ на укрепление союза".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "greek_scholars_flee".to_string(),
            probability: 0.06,
            target: EventTarget::Actor("byzantium".to_string()),
            conditions: vec![
                Condition { metric: "actor:byzantium.external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 70.0 },
            ],
            effects: HashMap::from([
                ("actor:byzantium.cohesion".to_string(), -8.0),
                ("actor:byzantium.legitimacy".to_string(), -5.0),
                ("global:federation_progress".to_string(), 2.0),
            ]),
            llm_context: "Греческие учёные и философы бегут на Запад, унося с собой знания".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "ottoman_spy_caught".to_string(),
            probability: 0.07,
            target: EventTarget::Actor("byzantium".to_string()),
            conditions: vec![],
            effects: HashMap::from([
                ("actor:byzantium.legitimacy".to_string(), 5.0),
                ("actor:ottomans.external_pressure".to_string(), -3.0),
                ("global:federation_progress".to_string(), 4.0),
            ]),
            llm_context: "Пойманный османский шпион доказал угрозу — союзники насторожились".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "crusade_call".to_string(),
            probability: 0.05,
            target: EventTarget::Actor("papacy".to_string()),
            conditions: vec![
                Condition { metric: "actor:papacy.legitimacy".to_string(), operator: ComparisonOperator::Greater, value: 60.0 },
            ],
            effects: HashMap::from([
                ("global:federation_progress".to_string(), 10.0),
                ("actor:papacy.treasury".to_string(), -200.0),
                ("actor:hungary.military_size".to_string(), 20.0),
            ]),
            llm_context: "Папа призвал к новому крестовому походу против турок".to_string(),
            one_time: true,
        },
        RandomEvent {
            id: "venetian_fleet_storm".to_string(),
            probability: 0.06,
            target: EventTarget::Actor("venice".to_string()),
            conditions: vec![],
            effects: HashMap::from([
                ("actor:venice.military_size".to_string(), -25.0),
                ("actor:venice.treasury".to_string(), -100.0),
                ("global:federation_progress".to_string(), -5.0),
            ]),
            llm_context: "Буря разметала венецианский флот в Эгейском море".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "mehmed_threatens".to_string(),
            probability: 0.08,
            target: EventTarget::Actor("ottomans".to_string()),
            conditions: vec![
                Condition { metric: "actor:ottomans.military_size".to_string(), operator: ComparisonOperator::Greater, value: 150.0 },
            ],
            effects: HashMap::from([
                ("actor:byzantium.external_pressure".to_string(), 10.0),
                ("actor:byzantium.cohesion".to_string(), -8.0),
                ("global:federation_progress".to_string(), 5.0),
            ]),
            llm_context: "Открытые угрозы Мехмеда в адрес Константинополя встревожили Европу".to_string(),
            one_time: false,
        },
    ]
}
