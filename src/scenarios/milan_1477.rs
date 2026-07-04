use std::collections::HashMap;

use serde::Deserialize;

use crate::core::{
    Actor, ActorTag, AutoDelta, BorderType, ComparisonOperator,
    DependencyRule, EraDefinition, EventCondition, EventConditionType, MapConfig, MilestoneEvent,
    Neighbor, PatronAction, RankBonusRule, RankCondition, RankResult, Scenario,
    TagDefinition,
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

/// Tags file structure for TOML deserialization
#[derive(Deserialize)]
struct TagsFile {
    tags: Vec<TagDefinition>,
}

/// Eras file structure for TOML deserialization
#[derive(Deserialize)]
struct ErasFile {
    eras: Vec<EraDefinition>,
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
];

/// Known actor IDs for map validation
const KNOWN_ACTOR_IDS: &[&str] = &[
    "milan", "venice", "florence", "naples", "papacy",
    "genoa", "ferrara", "mantua", "siena", "urbino", "bologna", "savoy", "sicily",
    // Spawned actor (appears via milestone event, not guaranteed)
    "france",
];

/// Load dependencies from TOML file
fn load_dependencies() -> Vec<DependencyRule> {
    let deps_file: DependenciesFile = toml::from_str(
        include_str!("milan_1477/dependencies.toml")
    ).expect("milan_1477/dependencies.toml parse error");

    if let Err(errors) = crate::engine::validate_dependencies(&deps_file.dependencies, KNOWN_METRICS) {
        panic!(
            "scenario 'milan_1477': invalid dependencies.toml:\n  - {}",
            errors.join("\n  - ")
        );
    }

    deps_file.dependencies
}

/// Load actions from TOML file
fn load_actions() -> (Vec<PatronAction>, Vec<PatronAction>) {
    let actions_file: ActionsFile = toml::from_str(
        include_str!("milan_1477/actions.toml")
    ).expect("milan_1477/actions.toml parse error");

    crate::core::validate_patron_actions(&actions_file.patron_actions, KNOWN_METRICS);

    (actions_file.patron_actions, actions_file.universal_actions)
}

/// Load rank bonuses from TOML file
fn load_rank_bonuses() -> Vec<RankBonusRule> {
    let rank_file: RankBonusesFile = toml::from_str(
        include_str!("milan_1477/rank_bonuses.toml")
    ).expect("milan_1477/rank_bonuses.toml parse error");

    rank_file.rank_bonuses
}

/// Load map config from TOML file
fn load_map_config() -> Option<MapConfig> {
    let map_file: MapFile = toml::from_str(
        include_str!("milan_1477/map.toml")
    ).expect("milan_1477/map.toml parse error");

    crate::core::validate_map_config(&map_file.map, KNOWN_ACTOR_IDS);

    Some(map_file.map)
}

/// Load auto deltas from TOML file
fn load_auto_deltas() -> Vec<AutoDelta> {
    let file: AutoDeltasFile = toml::from_str(
        include_str!("milan_1477/auto_deltas.toml")
    ).expect("milan_1477/auto_deltas.toml parse error");
    file.auto_deltas
}

/// Load milestone events from TOML file
fn load_milestone_events() -> Vec<MilestoneEvent> {
    let file: MilestoneEventsFile = toml::from_str(
        include_str!("milan_1477/milestone_events.toml")
    ).expect("milan_1477/milestone_events.toml parse error");
    file.milestone_events
}

/// Load tag definitions from TOML file
fn load_tags() -> Vec<TagDefinition> {
    let tags_file: TagsFile = toml::from_str(
        include_str!("milan_1477/tags.toml")
    ).expect("milan_1477/tags.toml parse error");
    tags_file.tags
}

/// Load era definitions from TOML file
fn load_eras() -> Vec<EraDefinition> {
    let eras_file: ErasFile = toml::from_str(
        include_str!("milan_1477/eras.toml")
    ).expect("milan_1477/eras.toml parse error");
    eras_file.eras
}

/// Populate actor_tags from tag definitions based on each actor's tags vec
fn populate_actor_tags(actors: &mut [Actor], tag_defs: &[TagDefinition]) {
    let tag_map: std::collections::HashMap<&str, &TagDefinition> =
        tag_defs.iter().map(|t| (t.id.as_str(), t)).collect();
    for actor in actors.iter_mut() {
        for tag_str in &actor.tags {
            if let Some(td) = tag_map.get(tag_str.as_str()) {
                actor.actor_tags.insert(tag_str.clone(), ActorTag {
                    metrics_modifier: td.metrics_modifier.clone(),
                    spreads_via: td.spreads_via.clone(),
                });
            }
        }
    }
}

/// Load the Milan 1477 scenario
pub fn load_milan_1477() -> Scenario {
    eprintln!("[SCENARIO] load_milan_1477 - starting");
    let dependencies = load_dependencies();
    let (patron_actions, universal_actions) = load_actions();
    let rank_bonuses = load_rank_bonuses();
    let map = load_map_config();
    let tag_definitions = load_tags();
    let era_definitions = load_eras();

    let mut actors = create_actors();
    populate_actor_tags(&mut actors, &tag_definitions);

    let scenario = Scenario {
        id: "milan_1477".to_string(),
        label: "Milan 1477 — Регентство".to_string(),
        description: "1477 год. Галеаццо Мария Сфорца убит. Милан правит малолетний герцог — и все это знают.".to_string(),
        start_year: 1477,
        tempo: 0.75,
        tick_span: 1,
        era: crate::core::Era::LateMedieval,
        tick_label: "год".to_string(),
        actors,
        auto_deltas: load_auto_deltas(),
        milestone_events: load_milestone_events(),
        rank_conditions: create_rank_conditions(),
        generation_mechanics: None,
        llm_context: create_llm_context(),
        consequence_context: create_consequence_context(),
        player_actor_id: Some("milan".to_string()),
        status_indicators: create_status_indicators(),
        global_metric_weights: HashMap::new(),
        features: crate::core::ScenarioFeatures {
            family_panel: false,
            global_metrics_panel: false,
            patron_actions: true,
        },
        military_conflict_probability: 0.20,
        naval_conflict_probability: 0.12,
        random_events: create_random_events(),
        generation_length: None,
        actions_per_tick: 3,
        victory_condition: None,
        universal_actions,
        global_metrics_display: vec![],
        initial_family_metrics: None,
        max_random_events_per_tick: 3,
        narrative_config: crate::core::NarrativeConfig {
            key_metrics: vec![
                "milan.legitimacy".to_string(),
                "milan.cohesion".to_string(),
                "milan.external_pressure".to_string(),
                "naples.external_pressure".to_string(),
                "naples.cohesion".to_string(),
            ],
            narrative_axes: vec![
                "legitimacy vs force".to_string(),
                "marriage and money vs war".to_string(),
                "unity vs fragmentation".to_string(),
            ],
            tone_tags: vec![
                "court intrigue".to_string(),
                "renaissance grandeur".to_string(),
                "cynical realism".to_string(),
            ],
            forbidden_claims: vec![
                "Do not claim Milan has fallen unless milan is in dead_actors".to_string(),
                "Do not claim victory has been achieved unless victory_achieved is true".to_string(),
                "Do not mention specific numbers, percentages, or game metrics".to_string(),
                "Do not claim Italy has been unified unless the scenario explicitly states so".to_string(),
            ],
            paragraph_target: 6,
            output_length_hint: "detailed half-year chronicle, 6-8 paragraphs".to_string(),
        },
        dependencies,
        patron_actions,
        interaction_rules: vec![],
        rank_bonuses,
        map,
        tag_definitions,
        era_definitions,
    };
    eprintln!("[SCENARIO] load_milan_1477 - loaded {} actors", scenario.actors.len());
    scenario
}

fn create_actors() -> Vec<Actor> {
    vec![
        create_milan(),
        create_venice(),
        create_florence(),
        create_naples(),
        create_papacy(),
        create_genoa(),
        create_ferrara(),
        create_mantua(),
        create_siena(),
        create_urbino(),
        create_bologna(),
        create_savoy(),
        create_sicily(),
    ]
}

// ============================================================================
// Actor Definitions
// ============================================================================

fn create_milan() -> Actor {
    Actor {
        id: "milan".to_string(),
        name: "Миланское герцогство".to_string(),
        name_short: "Милан".to_string(),
        region: "lombardy".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "condottieri".to_string(),
            "banking".to_string(),
            "regency_crisis".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 260.0),
            ("military_size".to_string(), 40.0),
            ("military_quality".to_string(), 65.0),
            ("economic_output".to_string(), 75.0),
            ("cohesion".to_string(), 48.0),
            ("legitimacy".to_string(), 45.0),
            ("external_pressure".to_string(), 45.0),
            ("treasury".to_string(), 450.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "venice".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "genoa".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "savoy".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "mantua".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "ferrara".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 45.46, lng: 9.19 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Джан Галеаццо Сфорца (регент: Бона Савойская)".to_string()),
    }
}

fn create_venice() -> Actor {
    Actor {
        id: "venice".to_string(),
        name: "Венецианская республика".to_string(),
        name_short: "Венеция".to_string(),
        region: "veneto".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "maritime".to_string(),
            "trade_empire".to_string(),
            "oligarchy".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 190.0),
            ("military_size".to_string(), 30.0),
            ("military_quality".to_string(), 68.0),
            ("economic_output".to_string(), 85.0),
            ("cohesion".to_string(), 65.0),
            ("legitimacy".to_string(), 75.0),
            ("external_pressure".to_string(), 30.0),
            ("treasury".to_string(), 700.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "milan".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "mantua".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "ferrara".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "papacy".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "genoa".to_string(), distance: 2, border_type: BorderType::Sea },
            Neighbor { id: "naples".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 45.44, lng: 12.33 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Дож Андреа Вендрамин".to_string()),
    }
}

fn create_florence() -> Actor {
    Actor {
        id: "florence".to_string(),
        name: "Флорентийская республика".to_string(),
        name_short: "Флоренция".to_string(),
        region: "tuscany".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "banking".to_string(),
            "humanism".to_string(),
            "medici_faction".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 150.0),
            ("military_size".to_string(), 20.0),
            ("military_quality".to_string(), 60.0),
            ("economic_output".to_string(), 80.0),
            ("cohesion".to_string(), 55.0),
            ("legitimacy".to_string(), 60.0),
            ("external_pressure".to_string(), 35.0),
            ("treasury".to_string(), 550.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "siena".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "papacy".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "bologna".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "genoa".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "urbino".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 43.77, lng: 11.26 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Лоренцо Медичи".to_string()),
    }
}

fn create_naples() -> Actor {
    Actor {
        id: "naples".to_string(),
        name: "Неаполитанское королевство".to_string(),
        name_short: "Неаполь".to_string(),
        region: "campania".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "aragonese_crown".to_string(),
            "baronial_fronde".to_string(),
            "ottoman_frontier".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 550.0),
            ("military_size".to_string(), 45.0),
            ("military_quality".to_string(), 60.0),
            ("economic_output".to_string(), 55.0),
            ("cohesion".to_string(), 40.0),
            ("legitimacy".to_string(), 58.0),
            ("external_pressure".to_string(), 50.0),
            ("treasury".to_string(), 300.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "papacy".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "sicily".to_string(), distance: 1, border_type: BorderType::Sea },
            Neighbor { id: "florence".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 40.85, lng: 14.27 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Ферранте I Арагонский".to_string()),
    }
}

fn create_papacy() -> Actor {
    Actor {
        id: "papacy".to_string(),
        name: "Папская область".to_string(),
        name_short: "Папа".to_string(),
        region: "lazio".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "religious_authority".to_string(),
            "nepotism".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 90.0),
            ("military_size".to_string(), 15.0),
            ("military_quality".to_string(), 50.0),
            ("economic_output".to_string(), 55.0),
            ("cohesion".to_string(), 55.0),
            ("legitimacy".to_string(), 80.0),
            ("external_pressure".to_string(), 30.0),
            ("treasury".to_string(), 350.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "florence".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "venice".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "genoa".to_string(), distance: 3, border_type: BorderType::Sea },
            Neighbor { id: "bologna".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "urbino".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "siena".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "naples".to_string(), distance: 1, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 41.9, lng: 12.5 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Папа Сикст IV".to_string()),
    }
}

fn create_genoa() -> Actor {
    Actor {
        id: "genoa".to_string(),
        name: "Генуэзская республика".to_string(),
        name_short: "Генуя".to_string(),
        region: "liguria".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "maritime".to_string(),
            "trade_empire".to_string(),
            "factionalism".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 110.0),
            ("military_size".to_string(), 18.0),
            ("military_quality".to_string(), 58.0),
            ("economic_output".to_string(), 60.0),
            ("cohesion".to_string(), 42.0),
            ("legitimacy".to_string(), 50.0),
            ("external_pressure".to_string(), 45.0),
            ("treasury".to_string(), 380.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "milan".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "savoy".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "venice".to_string(), distance: 2, border_type: BorderType::Sea },
            Neighbor { id: "florence".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "papacy".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 44.41, lng: 8.93 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Дож Баттиста Кампофрегозо".to_string()),
    }
}

fn create_ferrara() -> Actor {
    Actor {
        id: "ferrara".to_string(),
        name: "Феррарское герцогство (Эсте)".to_string(),
        name_short: "Феррара".to_string(),
        region: "emilia_ferrara".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "este_court".to_string(),
            "humanism".to_string(),
            "patronage".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 40.0),
            ("military_size".to_string(), 15.0),
            ("military_quality".to_string(), 55.0),
            ("economic_output".to_string(), 40.0),
            ("cohesion".to_string(), 60.0),
            ("legitimacy".to_string(), 70.0),
            ("external_pressure".to_string(), 35.0),
            ("treasury".to_string(), 150.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "venice".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "milan".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "bologna".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "mantua".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "urbino".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 44.84, lng: 11.62 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Эрколе I д'Эсте".to_string()),
    }
}

fn create_mantua() -> Actor {
    Actor {
        id: "mantua".to_string(),
        name: "Мантуанское маркграфство (Гонзага)".to_string(),
        name_short: "Мантуя".to_string(),
        region: "mantua_region".to_string(),
        region_rank: crate::core::RegionRank::D,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "condottieri".to_string(),
            "gonzaga_court".to_string(),
            "patronage".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 25.0),
            ("military_size".to_string(), 12.0),
            ("military_quality".to_string(), 58.0),
            ("economic_output".to_string(), 30.0),
            ("cohesion".to_string(), 58.0),
            ("legitimacy".to_string(), 65.0),
            ("external_pressure".to_string(), 35.0),
            ("treasury".to_string(), 100.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "milan".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "venice".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "ferrara".to_string(), distance: 1, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 45.16, lng: 10.79 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Федерико I Гонзага".to_string()),
    }
}

fn create_siena() -> Actor {
    Actor {
        id: "siena".to_string(),
        name: "Сиенская республика".to_string(),
        name_short: "Сиена".to_string(),
        region: "tuscany_siena".to_string(),
        region_rank: crate::core::RegionRank::D,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "factionalism".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 35.0),
            ("military_size".to_string(), 10.0),
            ("military_quality".to_string(), 45.0),
            ("economic_output".to_string(), 35.0),
            ("cohesion".to_string(), 38.0),
            ("legitimacy".to_string(), 45.0),
            ("external_pressure".to_string(), 40.0),
            ("treasury".to_string(), 120.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "florence".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "papacy".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 43.32, lng: 11.33 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Синьория Сиены".to_string()),
    }
}

fn create_urbino() -> Actor {
    Actor {
        id: "urbino".to_string(),
        name: "Урбинское герцогство (Монтефельтро)".to_string(),
        name_short: "Урбино".to_string(),
        region: "marche".to_string(),
        region_rank: crate::core::RegionRank::D,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "condottieri".to_string(),
            "patronage".to_string(),
            "humanism".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 15.0),
            ("military_size".to_string(), 15.0),
            ("military_quality".to_string(), 72.0),
            ("economic_output".to_string(), 35.0),
            ("cohesion".to_string(), 60.0),
            ("legitimacy".to_string(), 68.0),
            ("external_pressure".to_string(), 30.0),
            ("treasury".to_string(), 200.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "papacy".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "florence".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "ferrara".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 43.73, lng: 12.64 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Федерико да Монтефельтро".to_string()),
    }
}

fn create_bologna() -> Actor {
    Actor {
        id: "bologna".to_string(),
        name: "Болонская синьория (Бентивольо)".to_string(),
        name_short: "Болонья".to_string(),
        region: "emilia_bologna".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "condottieri".to_string(),
            "papal_vicariate".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 40.0),
            ("military_size".to_string(), 14.0),
            ("military_quality".to_string(), 55.0),
            ("economic_output".to_string(), 40.0),
            ("cohesion".to_string(), 50.0),
            ("legitimacy".to_string(), 55.0),
            ("external_pressure".to_string(), 40.0),
            ("treasury".to_string(), 130.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "papacy".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "florence".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "ferrara".to_string(), distance: 1, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 44.49, lng: 11.34 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Джованни II Бентивольо".to_string()),
    }
}

fn create_savoy() -> Actor {
    Actor {
        id: "savoy".to_string(),
        name: "Савойское герцогство".to_string(),
        name_short: "Савойя".to_string(),
        region: "piedmont".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "french_orbit".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 120.0),
            ("military_size".to_string(), 20.0),
            ("military_quality".to_string(), 50.0),
            ("economic_output".to_string(), 35.0),
            ("cohesion".to_string(), 50.0),
            ("legitimacy".to_string(), 55.0),
            ("external_pressure".to_string(), 45.0),
            ("treasury".to_string(), 150.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "milan".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "genoa".to_string(), distance: 1, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 45.07, lng: 7.69 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Йоланда Савойская (регентство)".to_string()),
    }
}

fn create_sicily() -> Actor {
    Actor {
        id: "sicily".to_string(),
        name: "Сицилийское королевство".to_string(),
        name_short: "Сицилия".to_string(),
        region: "sicily".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::LateMedieval,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "aragonese_crown".to_string(),
            "separate_pole".to_string(),
            "maritime".to_string(),
            "catholic".to_string(),
        ],
        metrics: HashMap::from([
            ("population".to_string(), 200.0),
            ("military_size".to_string(), 25.0),
            ("military_quality".to_string(), 55.0),
            ("economic_output".to_string(), 45.0),
            ("cohesion".to_string(), 55.0),
            ("legitimacy".to_string(), 70.0),
            ("external_pressure".to_string(), 35.0),
            ("treasury".to_string(), 200.0)
        ]),
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "naples".to_string(), distance: 1, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 38.12, lng: 13.36 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Фердинанд II Арагонский (наместник)".to_string()),
    }
}

// ============================================================================
// Rank Conditions
// ============================================================================

fn create_rank_conditions() -> Vec<RankCondition> {
    vec![
        RankCondition {
            region_id: "lombardy".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "actor:milan.legitimacy".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 75.0,
                },
                duration: Some(8),
            },
            result: RankResult { rank: "S".to_string() },
            is_key: true,
        },
        RankCondition {
            region_id: "veneto".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "actor:venice.economic_output".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Greater,
                    value: 90.0,
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
    r#"СЦЕНАРИЙ: Милан 1477
НАРРАТИВ: Хроника от третьего лица. Без игрока внутри мира.

КОНТЕКСТ:
1477 год. Галеаццо Мария Сфорца убит заговорщиками в конце 1476 года.
На троне — малолетний Джан Галеаццо Сфорца. Вокруг регентства идёт
борьба: вдова герцога Бона Савойская и дядья покойного герцога,
прежде всего честолюбивый Лудовико Моро.

Это не военная осада — это политическая борьба за легитимность
при слабом центре. Военная сила вторична по отношению к союзам,
бракам и деньгам.

Венеция богата и стабильна — торговая олигархия, которая играет
в долгую. Флоренция — банковский дом Медичи под тонкой вуалью
республики. Неаполь трещит по швам — арагонская корона Ферранте
сталкивается с баронской фрондой изнутри и растущей османской
угрозой с моря (Отранто). Папство — источник легитимности,
не силы, и Сикст IV не стесняется пользоваться этим рычагом.

Второй тир — Феррара, Мантуя, Урбино — живут патронажем и
кондотьерским ремеслом, торгуя военной силой и культурным
престижем. Сиена и Болонья слабее и уязвимее. Савойя смотрит
на Альпы с тревогой: Франция ещё не вошла в игру, но уже
присматривается.

Юг — Сицилия, отдельный полюс силы под прямой властью
арагонской короны, не сателлит Неаполя.

ТОНАЛЬНОСТЬ:
Ренессансная Италия. Интрига важнее битвы. Брак важнее осады.
Деньги важнее меча — но меч всегда под рукой у того, кто платит.
Хроника охватывает весь регион — переговоры, браки, банковские
интриги, наём кондотьеров, придворные заговоры.
4-6 абзацев за тик.

НЕ ДЕЛАТЬ:
- Не предрешать судьбу регентства в Милане
- Не делать Неаполь обречённым заранее — баронская фронда и
  османская угроза серьёзны, но не фатальны автоматически
- Не игнорировать соперничество внутри Италии — общие интересы
  не отменяют взаимного недоверия
- Венеция и Генуя не друзья
- Папство важно как легитимность, но не как военная сила"#.to_string()
}

fn create_consequence_context() -> String {
    r#"Сценарный период завершён. Симуляция продолжается.
Регентство в Милане либо укрепилось, либо распалось.
Италия либо движется к объединению, либо остаётся раздробленной
россыпью соперничающих держав. Османская угроза на юге
продолжает нарастать или была остановлена.
Нарратив охватывает более широкий период истории."#.to_string()
}

fn create_status_indicators() -> Vec<crate::core::StatusIndicator> {
    use crate::core::StatusIndicator;
    vec![
        StatusIndicator {
            label: "Регентство в Милане".to_string(),
            metric: "actor:milan.legitimacy".to_string(),
            invert: false,
            thresholds: vec![
                (0.0, "на грани распада".to_string()),
                (30.0, "оспаривается".to_string()),
                (55.0, "удерживается".to_string()),
                (75.0, "укрепилось".to_string()),
            ],
        },
        StatusIndicator {
            label: "Неаполь".to_string(),
            metric: "actor:naples.external_pressure".to_string(),
            invert: true,
            thresholds: vec![
                (0.0, "спокоен".to_string()),
                (50.0, "под угрозой".to_string()),
                (70.0, "Отранто в опасности".to_string()),
            ],
        },
        StatusIndicator {
            label: "Баронская фронда".to_string(),
            metric: "actor:naples.cohesion".to_string(),
            invert: true,
            thresholds: vec![
                (0.0, "под контролем".to_string()),
                (40.0, "нарастает".to_string()),
                (60.0, "мятеж".to_string()),
            ],
        },
    ]
}

fn create_random_events() -> Vec<crate::core::RandomEvent> {
    // Reserved for tasks C (patronage) and D (coalition) - deliberately empty here.
    vec![]
}
