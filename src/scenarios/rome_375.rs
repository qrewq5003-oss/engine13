use std::collections::HashMap;

use crate::core::{
    Actor, ActorMetrics, AutoDelta, BorderType, ComparisonOperator, DeltaCondition,
    EventCondition, EventConditionType, GenerationMechanics, MilestoneEvent, Neighbor,
    PatronAction, RankCondition, RankResult, Scenario, Successor,
};

/// Load the Rome 375 scenario
pub fn load_rome_375() -> Scenario {
    eprintln!("[SCENARIO] load_rome_375 - starting");
    let scenario = Scenario {
        id: "rome_375".to_string(),
        label: "Rome 375 — Семья Ди Милано".to_string(),
        description: "375 год. Медиолан — фактическая столица Западной Империи. Гунны за горизонтом давят на готов.".to_string(),
        start_year: 375,
        tempo: 0.7,
        tick_span: 1,
        era: crate::core::Era::Ancient,
        tick_label: "год".to_string(),
        actors: create_actors(),
        auto_deltas: create_auto_deltas(),
        patron_actions: create_patron_actions(),
        milestone_events: create_milestone_events(),
        rank_conditions: create_rank_conditions(),
        generation_mechanics: Some(create_generation_mechanics()),
        llm_context: create_llm_context(),
        consequence_context: create_consequence_context(),
        player_actor_id: Some("rome".to_string()),
        status_indicators: create_status_indicators(),
        global_metric_weights: HashMap::new(),
        features: crate::core::ScenarioFeatures {
            family_panel: true,
            global_metrics_panel: false,
            patron_actions: false,
        },
        military_conflict_probability: 0.45,
        naval_conflict_probability: 0.10,
        random_events: create_random_events(),
        generation_length: Some(33),
        actions_per_tick: 2,
        victory_condition: Some(crate::core::VictoryCondition {
            metric: "family:influence".to_string(),
            threshold: 90.0,
            title: "Семья достигла величия".to_string(),
            description: "Ди Милано стали опорой угасающей империи.".to_string(),
            minimum_tick: 15,
            additional_conditions: vec![],
            sustained_ticks_required: 1,
        }),
        universal_actions: create_universal_actions(),
        global_metrics_display: vec![],
        initial_family_metrics: Some(HashMap::from([
            ("family:family_influence".to_string(), 60.0),
            ("family:family_knowledge".to_string(), 40.0),
            ("family:family_wealth".to_string(), 50.0),
            ("family:family_connections".to_string(), 45.0),
        ])),
        max_random_events_per_tick: 2,
        narrative_config: crate::core::NarrativeConfig {
            key_metrics: vec![
                "family:family_influence".to_string(),
                "family:family_knowledge".to_string(),
                "family:family_wealth".to_string(),
                "family:family_connections".to_string(),
                "rome.legitimacy".to_string(),
                "rome.cohesion".to_string(),
            ],
            narrative_axes: vec![
                "stability vs ambition".to_string(),
                "tradition vs adaptation".to_string(),
                "family honor vs political necessity".to_string(),
            ],
            tone_tags: vec![
                "formal chronicle".to_string(),
                "epic scope".to_string(),
                "intimate family drama".to_string(),
            ],
            forbidden_claims: vec![
                "Do not claim any actor has died unless they are in dead_actors list".to_string(),
                "Do not claim victory has been achieved unless victory_achieved is true".to_string(),
                "Do not mention specific numbers, percentages, or game metrics".to_string(),
                "Do not claim Rome has fallen unless rome is in dead_actors".to_string(),
            ],
            paragraph_target: 6,
            output_length_hint: "detailed half-year chronicle, 6-8 paragraphs".to_string(),
        },
    };
    eprintln!("[SCENARIO] load_rome_375 - loaded {} actors", scenario.actors.len());
    scenario
}

fn create_actors() -> Vec<Actor> {
    vec![
        create_rome(),
        create_huns(),
        create_visigoths(),
        create_ostrogoths(),
        create_sassanids(),
        create_vandals(),
        create_burgundians(),
        create_franks(),
        create_saxons(),
        create_alamanni(),
        create_berbers(),
        create_armenia(),
        create_kushans(),
        create_guptas(),
        create_eastern_jin(),
        // Successor templates (not added to world.actors at start)
        create_rome_west(),
        create_rome_east(),
        create_visigoth_kingdom_template(),
        create_ostrogoth_kingdom_template(),
        create_late_sassanids_template(),
        create_vandal_kingdom_africa_template(),
        create_frankish_kingdom_template(),
    ]
}

// ============================================================================
// Actor Definitions
// ============================================================================

fn create_rome() -> Actor {
    let mut scenario_metrics = HashMap::new();
    scenario_metrics.insert("family:family_influence".to_string(), 8.0);
    scenario_metrics.insert("family:family_knowledge".to_string(), 12.0);
    scenario_metrics.insert("family:family_wealth".to_string(), 22.0);
    scenario_metrics.insert("family:family_connections".to_string(), 15.0);

    Actor {
        id: "rome".to_string(),
        name: "Римская Империя".to_string(),
        name_short: "Рим".to_string(),
        region: "mediterranean".to_string(),
        region_rank: crate::core::RegionRank::S,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "bureaucracy".to_string(),
            "roman_law".to_string(),
            "trade_networks".to_string(),
            "coinage".to_string(),
            "christianity".to_string(),
        ],
        metrics: ActorMetrics {
            population: 8000.0,
            military_size: 350.0,
            military_quality: 58.0,
            economic_output: 48.0,
            cohesion: 42.0,
            legitimacy: 62.0,
            external_pressure: 38.0,
            treasury: 1800.0,
        },
        scenario_metrics,
        neighbors: vec![
            Neighbor { id: "visigoths".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "ostrogoths".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "sassanids".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "vandals".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "burgundians".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "franks".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "alamanni".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "saxons".to_string(), distance: 3, border_type: BorderType::Sea },
            Neighbor { id: "berbers".to_string(), distance: 2, border_type: BorderType::Sea },
            Neighbor { id: "armenia".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![
            Successor { id: "rome_west".to_string(), weight: 0.45 },
            Successor { id: "rome_east".to_string(), weight: 0.55 },
        ],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 41.9, lng: 12.5 }),
        is_successor_template: false,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: Some("Император Грациан".to_string()),
    }
}

fn create_huns() -> Actor {
    Actor {
        id: "huns".to_string(),
        name: "Гунны".to_string(),
        name_short: "Гунны".to_string(),
        region: "steppe".to_string(),
        region_rank: crate::core::RegionRank::D,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "nomadic".to_string(),
            "cavalry".to_string(),
            "raid_economy".to_string(),
            "pastoral".to_string(),
        ],
        metrics: ActorMetrics {
            population: 800.0,
            military_size: 120.0,
            military_quality: 88.0,
            economic_output: 15.0,
            cohesion: 72.0,
            legitimacy: 60.0,
            external_pressure: 5.0,
            treasury: 80.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "ostrogoths".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "visigoths".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "eastern_jin".to_string(), distance: 4, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 48.0, lng: 60.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Turkic,
        minimum_survival_ticks: None,
        leader: Some("Баламир".to_string()),
    }
}

fn create_visigoths() -> Actor {
    Actor {
        id: "visigoths".to_string(),
        name: "Вестготы".to_string(),
        name_short: "Вестготы".to_string(),
        region: "balkans".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "tribal_confederation".to_string(),
            "christianity_arian".to_string(),
            "federati_potential".to_string(),
        ],
        metrics: ActorMetrics {
            population: 400.0,
            military_size: 48.0,
            military_quality: 62.0,
            economic_output: 22.0,
            cohesion: 52.0,
            legitimacy: 55.0,
            external_pressure: 65.0,
            treasury: 40.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "ostrogoths".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "burgundians".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![Successor { id: "visigoth_kingdom".to_string(), weight: 1.0 }],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 44.0, lng: 25.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Germanic,
        minimum_survival_ticks: None,
        leader: Some("Фритигерн".to_string()),
    }
}

fn create_ostrogoths() -> Actor {
    Actor {
        id: "ostrogoths".to_string(),
        name: "Остготы".to_string(),
        name_short: "Остготы".to_string(),
        region: "pontic_steppe".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Foreground,
        tags: vec![
            "tribal_confederation".to_string(),
            "steppe_adjacent".to_string(),
        ],
        metrics: ActorMetrics {
            population: 350.0,
            military_size: 55.0,
            military_quality: 65.0,
            economic_output: 18.0,
            cohesion: 60.0,
            legitimacy: 58.0,
            external_pressure: 78.0,
            treasury: 30.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "huns".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "visigoths".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "rome".to_string(), distance: 3, border_type: BorderType::Land },
        ],
        on_collapse: vec![Successor { id: "ostrogoth_kingdom".to_string(), weight: 1.0 }],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 47.0, lng: 32.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Germanic,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_sassanids() -> Actor {
    Actor {
        id: "sassanids".to_string(),
        name: "Сасанидская Персия".to_string(),
        name_short: "Персия".to_string(),
        region: "mesopotamia".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "bureaucracy".to_string(),
            "zoroastrianism".to_string(),
            "silk_road".to_string(),
            "cavalry_heavy".to_string(),
        ],
        metrics: ActorMetrics {
            population: 3000.0,
            military_size: 200.0,
            military_quality: 72.0,
            economic_output: 62.0,
            cohesion: 65.0,
            legitimacy: 75.0,
            external_pressure: 30.0,
            treasury: 900.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "armenia".to_string(), distance: 1, border_type: BorderType::Land },
            Neighbor { id: "kushans".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "guptas".to_string(), distance: 3, border_type: BorderType::Land },
        ],
        on_collapse: vec![Successor { id: "late_sassanids".to_string(), weight: 1.0 }],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 33.0, lng: 44.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Other,
        culture: crate::core::Culture::Persian,
        minimum_survival_ticks: None,
        leader: Some("Шапур II".to_string()),
    }
}

fn create_vandals() -> Actor {
    Actor {
        id: "vandals".to_string(),
        name: "Вандалы".to_string(),
        name_short: "Вандалы".to_string(),
        region: "dacia".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "christianity_arian".to_string(),
            "migrating".to_string(),
        ],
        metrics: ActorMetrics {
            population: 180.0,
            military_size: 28.0,
            military_quality: 60.0,
            economic_output: 20.0,
            cohesion: 58.0,
            legitimacy: 52.0,
            external_pressure: 55.0,
            treasury: 25.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![Successor { id: "vandal_kingdom_africa".to_string(), weight: 1.0 }],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 45.0, lng: 25.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Germanic,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_burgundians() -> Actor {
    Actor {
        id: "burgundians".to_string(),
        name: "Бургунды".to_string(),
        name_short: "Бургунды".to_string(),
        region: "rhine".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "rhine_border".to_string(),
        ],
        metrics: ActorMetrics {
            population: 120.0,
            military_size: 18.0,
            military_quality: 55.0,
            economic_output: 22.0,
            cohesion: 62.0,
            legitimacy: 58.0,
            external_pressure: 35.0,
            treasury: 20.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "visigoths".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 49.0, lng: 8.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Germanic,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_franks() -> Actor {
    Actor {
        id: "franks".to_string(),
        name: "Франки".to_string(),
        name_short: "Франки".to_string(),
        region: "gaul_north".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "rhine_border".to_string(),
            "roman_contact".to_string(),
        ],
        metrics: ActorMetrics {
            population: 200.0,
            military_size: 30.0,
            military_quality: 58.0,
            economic_output: 25.0,
            cohesion: 55.0,
            legitimacy: 50.0,
            external_pressure: 25.0,
            treasury: 30.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![Successor { id: "frankish_kingdom".to_string(), weight: 1.0 }],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 50.0, lng: 6.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Germanic,
        minimum_survival_ticks: None,
        leader: Some("Хлодион".to_string()),
    }
}

fn create_saxons() -> Actor {
    Actor {
        id: "saxons".to_string(),
        name: "Саксы".to_string(),
        name_short: "Саксы".to_string(),
        region: "germania_north".to_string(),
        region_rank: crate::core::RegionRank::D,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "seafaring".to_string(),
            "raid_economy".to_string(),
        ],
        metrics: ActorMetrics {
            population: 150.0,
            military_size: 20.0,
            military_quality: 55.0,
            economic_output: 18.0,
            cohesion: 60.0,
            legitimacy: 48.0,
            external_pressure: 15.0,
            treasury: 15.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 53.0, lng: 9.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Germanic,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_alamanni() -> Actor {
    Actor {
        id: "alamanni".to_string(),
        name: "Аламанны".to_string(),
        name_short: "Аламанны".to_string(),
        region: "rhine_upper".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "rhine_border".to_string(),
        ],
        metrics: ActorMetrics {
            population: 180.0,
            military_size: 28.0,
            military_quality: 60.0,
            economic_output: 20.0,
            cohesion: 58.0,
            legitimacy: 52.0,
            external_pressure: 30.0,
            treasury: 22.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 48.5, lng: 9.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Germanic,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_berbers() -> Actor {
    Actor {
        id: "berbers".to_string(),
        name: "Берберские племена".to_string(),
        name_short: "Берберы".to_string(),
        region: "north_africa".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "desert_warfare".to_string(),
            "roman_frontier".to_string(),
        ],
        metrics: ActorMetrics {
            population: 300.0,
            military_size: 35.0,
            military_quality: 55.0,
            economic_output: 28.0,
            cohesion: 45.0,
            legitimacy: 42.0,
            external_pressure: 20.0,
            treasury: 35.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 2, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 32.0, lng: 10.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Pagan,
        culture: crate::core::Culture::Arabic,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_armenia() -> Actor {
    Actor {
        id: "armenia".to_string(),
        name: "Армения".to_string(),
        name_short: "Армения".to_string(),
        region: "caucasus".to_string(),
        region_rank: crate::core::RegionRank::C,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "buffer_state".to_string(),
            "christianity".to_string(),
            "persian_border".to_string(),
            "roman_border".to_string(),
        ],
        metrics: ActorMetrics {
            population: 500.0,
            military_size: 40.0,
            military_quality: 58.0,
            economic_output: 35.0,
            cohesion: 55.0,
            legitimacy: 60.0,
            external_pressure: 55.0,
            treasury: 120.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "rome".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "sassanids".to_string(), distance: 1, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 40.0, lng: 45.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Other,
        culture: crate::core::Culture::Persian,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_kushans() -> Actor {
    Actor {
        id: "kushans".to_string(),
        name: "Кушанское царство".to_string(),
        name_short: "Кушаны".to_string(),
        region: "bactria".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "silk_road".to_string(),
            "buddhism".to_string(),
            "trade_networks".to_string(),
            "declining".to_string(),
        ],
        metrics: ActorMetrics {
            population: 800.0,
            military_size: 60.0,
            military_quality: 55.0,
            economic_output: 45.0,
            cohesion: 40.0,
            legitimacy: 45.0,
            external_pressure: 50.0,
            treasury: 300.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "sassanids".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "guptas".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "eastern_jin".to_string(), distance: 3, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 36.0, lng: 68.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Buddhist,
        culture: crate::core::Culture::Indian,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_guptas() -> Actor {
    Actor {
        id: "guptas".to_string(),
        name: "Гуптская империя".to_string(),
        name_short: "Гупты".to_string(),
        region: "india".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "silk_road".to_string(),
            "hinduism".to_string(),
            "trade_networks".to_string(),
            "golden_age".to_string(),
        ],
        metrics: ActorMetrics {
            population: 4000.0,
            military_size: 180.0,
            military_quality: 65.0,
            economic_output: 70.0,
            cohesion: 72.0,
            legitimacy: 78.0,
            external_pressure: 15.0,
            treasury: 1200.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "kushans".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "eastern_jin".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 24.0, lng: 82.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Hindu,
        culture: crate::core::Culture::Indian,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_eastern_jin() -> Actor {
    Actor {
        id: "eastern_jin".to_string(),
        name: "Восточная Цзинь".to_string(),
        name_short: "Китай".to_string(),
        region: "china".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "silk_road".to_string(),
            "confucianism".to_string(),
            "trade_networks".to_string(),
            "southern_exile".to_string(),
        ],
        metrics: ActorMetrics {
            population: 5000.0,
            military_size: 150.0,
            military_quality: 55.0,
            economic_output: 58.0,
            cohesion: 45.0,
            legitimacy: 55.0,
            external_pressure: 40.0,
            treasury: 800.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![
            Neighbor { id: "kushans".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "guptas".to_string(), distance: 3, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 32.0, lng: 118.0 }),
        is_successor_template: false,
        religion: crate::core::Religion::Buddhist,
        culture: crate::core::Culture::EastAsian,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_rome_west() -> Actor {
    let mut scenario_metrics = HashMap::new();
    scenario_metrics.insert("family:family_influence".to_string(), 8.0);
    scenario_metrics.insert("family:family_knowledge".to_string(), 12.0);
    scenario_metrics.insert("family:family_wealth".to_string(), 22.0);
    scenario_metrics.insert("family:family_connections".to_string(), 15.0);

    Actor {
        id: "rome_west".to_string(),
        name: "Западная Римская Империя".to_string(),
        name_short: "Зап. Рим".to_string(),
        region: "mediterranean_west".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "bureaucracy".to_string(),
            "roman_law".to_string(),
            "trade_networks".to_string(),
            "coinage".to_string(),
            "christianity".to_string(),
        ],
        metrics: ActorMetrics {
            population: 3600.0,  // 45% of 8000
            military_size: 157.0,  // 45% of 350
            military_quality: 50.0,  // degraded from parent
            economic_output: 40.0,
            cohesion: 20.0,  // trauma from collapse
            legitimacy: 30.0,  // new power not established
            external_pressure: 50.0,  // enemies sense weakness
            treasury: 810.0,  // 45% of 1800
        },
        scenario_metrics,
        neighbors: vec![
            Neighbor { id: "visigoths".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "ostrogoths".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "vandals".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "burgundians".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "franks".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "alamanni".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "saxons".to_string(), distance: 3, border_type: BorderType::Sea },
            Neighbor { id: "berbers".to_string(), distance: 2, border_type: BorderType::Sea },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 45.0, lng: 9.0 }),  // Mediolanum
        is_successor_template: true,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Latin,
        minimum_survival_ticks: None,
        leader: None,
    }
}

fn create_rome_east() -> Actor {
    let mut scenario_metrics = HashMap::new();
    scenario_metrics.insert("family:family_influence".to_string(), 8.0);
    scenario_metrics.insert("family:family_knowledge".to_string(), 12.0);
    scenario_metrics.insert("family:family_wealth".to_string(), 22.0);
    scenario_metrics.insert("family:family_connections".to_string(), 15.0);

    Actor {
        id: "rome_east".to_string(),
        name: "Восточная Римская Империя".to_string(),
        name_short: "Вост. Рим".to_string(),
        region: "mediterranean_east".to_string(),
        region_rank: crate::core::RegionRank::A,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "bureaucracy".to_string(),
            "roman_law".to_string(),
            "trade_networks".to_string(),
            "coinage".to_string(),
            "christianity".to_string(),
        ],
        metrics: ActorMetrics {
            population: 4400.0,  // 55% of 8000
            military_size: 192.0,  // 55% of 350
            military_quality: 55.0,  // degraded from parent
            economic_output: 45.0,
            cohesion: 20.0,  // trauma from collapse
            legitimacy: 30.0,  // new power not established
            external_pressure: 45.0,  // enemies sense weakness
            treasury: 990.0,  // 55% of 1800
        },
        scenario_metrics,
        neighbors: vec![
            Neighbor { id: "sassanids".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "armenia".to_string(), distance: 2, border_type: BorderType::Land },
            Neighbor { id: "visigoths".to_string(), distance: 3, border_type: BorderType::Land },
            Neighbor { id: "ostrogoths".to_string(), distance: 2, border_type: BorderType::Land },
        ],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 41.0, lng: 28.0 }),  // Constantinople
        is_successor_template: true,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Greek,
        minimum_survival_ticks: None,
        leader: Some("Феодосий I".to_string()),
    }
}

// ============================================================================
// Successor Template Functions
// ============================================================================

/// Template for Visigoth Kingdom successor creation
pub fn create_visigoth_kingdom_template() -> Actor {
    Actor {
        id: "visigoth_kingdom".to_string(),
        name: "Королевство вестготов".to_string(),
        name_short: "Вестготы".to_string(),
        region: "balkans".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "christianity_arian".to_string(),
            "successor_state".to_string(),
        ],
        metrics: ActorMetrics {
            population: 1200.0,
            military_size: 45.0,
            military_quality: 65.0,
            economic_output: 25.0,
            cohesion: 55.0,
            legitimacy: 40.0,
            external_pressure: 30.0,
            treasury: 60.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 44.0, lng: 25.0 }),
        is_successor_template: true,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Greek,
        minimum_survival_ticks: None,
        leader: None,
    }
}

/// Template for Ostrogoth Kingdom successor creation
pub fn create_ostrogoth_kingdom_template() -> Actor {
    Actor {
        id: "ostrogoth_kingdom".to_string(),
        name: "Королевство остготов".to_string(),
        name_short: "Остготы".to_string(),
        region: "pontic_steppe".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "steppe_adjacent".to_string(),
            "successor_state".to_string(),
        ],
        metrics: ActorMetrics {
            population: 1000.0,
            military_size: 50.0,
            military_quality: 62.0,
            economic_output: 20.0,
            cohesion: 50.0,
            legitimacy: 35.0,
            external_pressure: 35.0,
            treasury: 40.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 47.0, lng: 32.0 }),
        is_successor_template: true,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Greek,
        minimum_survival_ticks: None,
        leader: None,
    }
}

/// Template for Late Sassanids successor creation
pub fn create_late_sassanids_template() -> Actor {
    Actor {
        id: "late_sassanids".to_string(),
        name: "Поздние Сасаниды".to_string(),
        name_short: "Персия".to_string(),
        region: "persia".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "bureaucracy".to_string(),
            "zoroastrianism".to_string(),
            "successor_state".to_string(),
        ],
        metrics: ActorMetrics {
            population: 1800.0,
            military_size: 80.0,
            military_quality: 55.0,
            economic_output: 30.0,
            cohesion: 35.0,
            legitimacy: 40.0,
            external_pressure: 50.0,
            treasury: 300.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 33.0, lng: 44.0 }),
        is_successor_template: true,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Greek,
        minimum_survival_ticks: None,
        leader: None,
    }
}

/// Template for Vandal Kingdom Africa successor creation
pub fn create_vandal_kingdom_africa_template() -> Actor {
    Actor {
        id: "vandal_kingdom_africa".to_string(),
        name: "Вандальское королевство в Африке".to_string(),
        name_short: "Вандалы".to_string(),
        region: "north_africa".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "christianity_arian".to_string(),
            "successor_state".to_string(),
        ],
        metrics: ActorMetrics {
            population: 800.0,
            military_size: 30.0,
            military_quality: 60.0,
            economic_output: 28.0,
            cohesion: 45.0,
            legitimacy: 35.0,
            external_pressure: 25.0,
            treasury: 150.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 32.0, lng: 10.0 }),
        is_successor_template: true,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Greek,
        minimum_survival_ticks: None,
        leader: None,
    }
}

/// Template for Frankish Kingdom successor creation
pub fn create_frankish_kingdom_template() -> Actor {
    Actor {
        id: "frankish_kingdom".to_string(),
        name: "Франкское королевство".to_string(),
        name_short: "Франки".to_string(),
        region: "rhine_upper".to_string(),
        region_rank: crate::core::RegionRank::B,
        era: crate::core::Era::Ancient,
        narrative_status: crate::core::NarrativeStatus::Background,
        tags: vec![
            "tribal_confederation".to_string(),
            "rhine_border".to_string(),
            "successor_state".to_string(),
        ],
        metrics: ActorMetrics {
            population: 900.0,
            military_size: 35.0,
            military_quality: 58.0,
            economic_output: 22.0,
            cohesion: 60.0,
            legitimacy: 45.0,
            external_pressure: 20.0,
            treasury: 80.0,
        },
        scenario_metrics: HashMap::new(),
        neighbors: vec![],
        on_collapse: vec![],
        actor_tags: HashMap::new(),
        center: Some(crate::core::GeoCoordinate { lat: 50.0, lng: 6.0 }),
        is_successor_template: true,
        religion: crate::core::Religion::Catholic,
        culture: crate::core::Culture::Greek,
        minimum_survival_ticks: None,
        leader: None,
    }
}

// ============================================================================
// Auto Deltas
// ============================================================================

fn create_auto_deltas() -> Vec<AutoDelta> {
    use crate::core::DeltaConditionRatio;

    vec![
        // Actor auto-deltas for Rome
        AutoDelta {
            metric: "population".to_string(),
            base: 0.3,
            conditions: vec![
                DeltaCondition { metric: "economic_output".to_string(), operator: ComparisonOperator::Less, value: 20.0, delta: -0.5 },
                DeltaCondition { metric: "external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 70.0, delta: -0.3 },
                DeltaCondition { metric: "treasury".to_string(), operator: ComparisonOperator::Less, value: 0.0, delta: -0.2 },
            ],
            ratio_conditions: vec![],
            noise: 0.1,
            actor_id: Some("rome".to_string()),
        },
        AutoDelta {
            metric: "military_size".to_string(),
            base: -0.2,
            conditions: vec![
                DeltaCondition { metric: "treasury".to_string(), operator: ComparisonOperator::Less, value: 0.0, delta: -1.0 },
                DeltaCondition { metric: "external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 60.0, delta: 0.3 },
            ],
            ratio_conditions: vec![],
            noise: 0.3,
            actor_id: Some("rome".to_string()),
        },
        AutoDelta {
            metric: "military_quality".to_string(),
            base: -0.1,
            conditions: vec![
                DeltaCondition { metric: "treasury".to_string(), operator: ComparisonOperator::Greater, value: 200.0, delta: 0.2 },
                DeltaCondition { metric: "external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 70.0, delta: -0.3 },
            ],
            ratio_conditions: vec![],
            noise: 0.2,
            actor_id: Some("rome".to_string()),
        },
        AutoDelta {
            metric: "economic_output".to_string(),
            base: 0.1,
            conditions: vec![
                DeltaCondition { metric: "treasury".to_string(), operator: ComparisonOperator::Less, value: 0.0, delta: -0.4 },
                DeltaCondition { metric: "cohesion".to_string(), operator: ComparisonOperator::Less, value: 25.0, delta: -0.5 },
            ],
            ratio_conditions: vec![],
            noise: 0.4,
            actor_id: Some("rome".to_string()),
        },
        AutoDelta {
            metric: "cohesion".to_string(),
            base: -0.1,
            conditions: vec![
                DeltaCondition { metric: "legitimacy".to_string(), operator: ComparisonOperator::Greater, value: 70.0, delta: 0.1 },
                DeltaCondition { metric: "economic_output".to_string(), operator: ComparisonOperator::Less, value: 20.0, delta: -0.4 },
                DeltaCondition { metric: "external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 60.0, delta: -0.2 },
            ],
            ratio_conditions: vec![],
            noise: 0.2,
            actor_id: Some("rome".to_string()),
        },
        AutoDelta {
            metric: "legitimacy".to_string(),
            base: -0.1,
            conditions: vec![
                DeltaCondition { metric: "cohesion".to_string(), operator: ComparisonOperator::Greater, value: 60.0, delta: 0.1 },
                DeltaCondition { metric: "treasury".to_string(), operator: ComparisonOperator::Less, value: 0.0, delta: -0.3 },
                DeltaCondition { metric: "military_size".to_string(), operator: ComparisonOperator::Less, value: 10.0, delta: -0.2 },
                // Knowledge → legitimacy bridge (soft support role, not victory path)
                DeltaCondition { metric: "family:family_knowledge".to_string(), operator: ComparisonOperator::Greater, value: 40.0, delta: 0.1 },
            ],
            ratio_conditions: vec![],
            noise: 0.1,
            actor_id: Some("rome".to_string()),
        },
        // Rome external pressure from barbarians
        // Pressure grows slower if Rome maintains military parity
        AutoDelta {
            metric: "actor:rome.external_pressure".to_string(),
            base: 2.0,
            conditions: vec![],
            ratio_conditions: vec![
                DeltaConditionRatio {
                    metric_a: "actor:rome.military_size".to_string(),
                    metric_b: "actor:visigoths.military_size".to_string(),
                    ratio: 0.8, // Rome should maintain parity
                    operator: ComparisonOperator::Greater,
                    delta: -1.8,
                },
                DeltaConditionRatio {
                    metric_a: "actor:rome.military_size".to_string(),
                    metric_b: "actor:huns.military_size".to_string(),
                    ratio: 0.5,
                    operator: ComparisonOperator::Greater,
                    delta: -1.0,
                },
            ],
            noise: 0.3,
            actor_id: Some("rome".to_string()),
        },
        // Family auto-deltas (passive changes per tick)
        AutoDelta {
            metric: "family:family_influence".to_string(),
            base: -0.5, // passive decay
            conditions: vec![
                DeltaCondition { metric: "family:family_connections".to_string(), operator: ComparisonOperator::Greater, value: 30.0, delta: 0.3 },
                DeltaCondition { metric: "family:family_wealth".to_string(), operator: ComparisonOperator::Greater, value: 40.0, delta: 0.2 },
                DeltaCondition { metric: "actor:rome.legitimacy".to_string(), operator: ComparisonOperator::Greater, value: 60.0, delta: 0.1 },
                DeltaCondition { metric: "actor:rome.cohesion".to_string(), operator: ComparisonOperator::Less, value: 30.0, delta: -0.2 },
            ],
            ratio_conditions: vec![],
            noise: 0.1,
            actor_id: None,
        },
        AutoDelta {
            metric: "family:family_knowledge".to_string(),
            base: 0.2, // always grows
            conditions: vec![
                DeltaCondition { metric: "family:family_knowledge".to_string(), operator: ComparisonOperator::Greater, value: 50.0, delta: 0.1 },
            ],
            ratio_conditions: vec![],
            noise: 0.05,
            actor_id: None,
        },
        AutoDelta {
            metric: "family:family_wealth".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "family:family_connections".to_string(), operator: ComparisonOperator::Greater, value: 20.0, delta: 0.5 },
                DeltaCondition { metric: "family:family_connections".to_string(), operator: ComparisonOperator::Less, value: 5.0, delta: -0.5 },
                DeltaCondition { metric: "actor:rome.economic_output".to_string(), operator: ComparisonOperator::Greater, value: 60.0, delta: 0.2 },
            ],
            ratio_conditions: vec![],
            noise: 0.1,
            actor_id: None,
        },
        AutoDelta {
            metric: "family:family_connections".to_string(),
            base: -0.3, // need to maintain
            conditions: vec![
                DeltaCondition { metric: "actor:rome.external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 70.0, delta: -0.2 },
            ],
            ratio_conditions: vec![],
            noise: 0.1,
            actor_id: None,
        },
        // Rome → Family: when Rome struggles, family suffers
        AutoDelta {
            metric: "family:family_connections".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "actor:rome.cohesion".to_string(), operator: ComparisonOperator::Less, value: 40.0, delta: -1.0 },
            ],
            ratio_conditions: vec![],
            noise: 0.0,
            actor_id: None,
        },
        AutoDelta {
            metric: "family:family_wealth".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "actor:rome.external_pressure".to_string(), operator: ComparisonOperator::Greater, value: 60.0, delta: -1.0 },
                DeltaCondition { metric: "actor:rome.economic_output".to_string(), operator: ComparisonOperator::Less, value: 35.0, delta: -1.0 },
            ],
            ratio_conditions: vec![],
            noise: 0.0,
            actor_id: None,
        },
        AutoDelta {
            metric: "family:family_influence".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "actor:rome.legitimacy".to_string(), operator: ComparisonOperator::Less, value: 40.0, delta: -2.0 },
            ],
            ratio_conditions: vec![],
            noise: 0.0,
            actor_id: None,
        },
        // Family → Rome: when family thrives, Rome benefits
        AutoDelta {
            metric: "legitimacy".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "family:family_influence".to_string(), operator: ComparisonOperator::Greater, value: 40.0, delta: 0.5 },
            ],
            ratio_conditions: vec![],
            noise: 0.0,
            actor_id: Some("rome".to_string()),
        },
        AutoDelta {
            metric: "cohesion".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "family:family_connections".to_string(), operator: ComparisonOperator::Greater, value: 40.0, delta: 0.5 },
            ],
            ratio_conditions: vec![],
            noise: 0.0,
            actor_id: Some("rome".to_string()),
        },
        AutoDelta {
            metric: "economic_output".to_string(),
            base: 0.0,
            conditions: vec![
                DeltaCondition { metric: "family:family_knowledge".to_string(), operator: ComparisonOperator::Greater, value: 40.0, delta: 0.3 },
            ],
            ratio_conditions: vec![],
            noise: 0.0,
            actor_id: Some("rome".to_string()),
        },
    ]
}

// ============================================================================
// Patron Actions
// ============================================================================

fn create_patron_actions() -> Vec<PatronAction> {
    // Family actions
    let actions = vec![
        PatronAction {
            source_actor_id: None,
            id: "expand_network".to_string(),
            name: "Расширить связи".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "family:family_wealth".to_string(), operator: ComparisonOperator::Greater, value: 10.0 },
            effects: HashMap::from([("family:family_connections".to_string(), 6.0)]),
            cost: HashMap::from([("family:family_wealth".to_string(), -4.0)]),
        },
        PatronAction {
            source_actor_id: None,
            id: "gather_information".to_string(),
            name: "Собрать информацию".to_string(),
            available_if: crate::core::ActionCondition::Always,
            effects: HashMap::from([("family:family_knowledge".to_string(), 6.0)]),
            cost: HashMap::from([("family:family_wealth".to_string(), -2.0)]),
        },
        PatronAction {
            source_actor_id: None,
            id: "invest_wealth".to_string(),
            name: "Вложить средства".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "family:family_wealth".to_string(), operator: ComparisonOperator::Greater, value: 20.0 },
            effects: HashMap::from([("family:family_wealth".to_string(), 5.0)]),
            cost: HashMap::from([("family:family_connections".to_string(), -3.0)]),
        },
        PatronAction {
            source_actor_id: None,
            id: "build_reputation".to_string(),
            name: "Укрепить репутацию".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "family:family_connections".to_string(), operator: ComparisonOperator::Greater, value: 15.0 },
            effects: HashMap::from([("family:family_influence".to_string(), 6.0)]),
            cost: HashMap::from([("family:family_wealth".to_string(), -5.0)]),
        },
        PatronAction {
            source_actor_id: None,
            id: "educate_family".to_string(),
            name: "Образование семьи".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "family:family_wealth".to_string(), operator: ComparisonOperator::Greater, value: 10.0 },
            effects: HashMap::from([("family:family_knowledge".to_string(), 10.0)]),
            cost: HashMap::from([("family:family_wealth".to_string(), -6.0)]),
        },
        PatronAction {
            source_actor_id: None,
            id: "lay_low".to_string(),
            name: "Затаиться".to_string(),
            available_if: crate::core::ActionCondition::Always,
            effects: HashMap::from([("family:family_wealth".to_string(), 3.0)]),
            cost: HashMap::from([("family:family_influence".to_string(), -2.0)]),
        },
        // City support actions
        PatronAction {
            source_actor_id: None,
            id: "support_city".to_string(),
            name: "Поддержать город".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "family:family_wealth".to_string(), operator: ComparisonOperator::Greater, value: 15.0 },
            effects: HashMap::from([
                ("family:family_influence".to_string(), 4.0),
                ("actor:rome.economic_output".to_string(), 2.0),
                ("actor:rome.cohesion".to_string(), 1.0),
            ]),
            cost: HashMap::from([("family:family_wealth".to_string(), -8.0)]),
        },
        PatronAction {
            source_actor_id: None,
            id: "back_administration".to_string(),
            name: "Поддержать администрацию".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "family:family_connections".to_string(), operator: ComparisonOperator::Greater, value: 15.0 },
            effects: HashMap::from([
                ("family:family_connections".to_string(), 5.0),
                ("actor:rome.legitimacy".to_string(), 2.0),
            ]),
            cost: HashMap::from([("family:family_wealth".to_string(), -6.0)]),
        },
        PatronAction {
            source_actor_id: None,
            id: "fund_defense".to_string(),
            name: "Вложить в оборону".to_string(),
            available_if: crate::core::ActionCondition::Metric { metric: "family:family_wealth".to_string(), operator: ComparisonOperator::Greater, value: 20.0 },
            effects: HashMap::from([
                ("family:family_influence".to_string(), 3.0),
                ("actor:rome.military_quality".to_string(), 2.0),
            ]),
            cost: HashMap::from([("family:family_wealth".to_string(), -10.0)]),
        },
    ];

    actions
}

// ============================================================================
// Milestone Events
// ============================================================================

fn create_milestone_events() -> Vec<MilestoneEvent> {
    vec![
        MilestoneEvent {
            id: "family_rises".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "family:family_influence".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::GreaterOrEqual,
                    value: 60.0,
                },
                duration: None,
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Семья Ди Милано стала одной из значимых сил города. Их больше не игнорируют.".to_string(),
            cooldown_ticks: None,
        },
        MilestoneEvent {
            id: "rome_splits".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "cohesion".to_string(),
                    actor_id: Some("rome".to_string()),
                    operator: ComparisonOperator::Less,
                    value: 30.0,
                },
                duration: Some(5),
            },
            is_key: true,
            triggers_collapse: true,
            llm_context_shift: "Империя раскололась. Западная и Восточная части теперь идут разными путями.".to_string(),
            cooldown_ticks: None,
        },
        MilestoneEvent {
            id: "adrianople".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "external_pressure".to_string(),
                    actor_id: Some("rome".to_string()),
                    operator: ComparisonOperator::Greater,
                    value: 85.0,
                },
                duration: Some(3),
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Готы перешли черту. Адрианополь. Валент мёртв. Мир изменился навсегда.".to_string(),
            cooldown_ticks: None,
        },
        MilestoneEvent {
            id: "huns_visible".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "military_size".to_string(),
                    actor_id: Some("huns".to_string()),
                    operator: ComparisonOperator::Greater,
                    value: 200.0,
                },
                duration: None,
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Гунны больше не слухи. Их видели у Дуная. Паника нарастает.".to_string(),
            cooldown_ticks: None,
        },
        MilestoneEvent {
            id: "family_falls".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "family:family_influence".to_string(),
                    actor_id: None,
                    operator: ComparisonOperator::Less,
                    value: 5.0,
                },
                duration: None,
            },
            is_key: true,
            triggers_collapse: false,
            llm_context_shift: "Семья Ди Милано потеряла всё что нажила. Они снова никто.".to_string(),
            cooldown_ticks: None,
        },
    ]
}

// ============================================================================
// Rank Conditions
// ============================================================================

fn create_rank_conditions() -> Vec<RankCondition> {
    vec![
        // Steppe grows with Hunnic horde size
        RankCondition {
            region_id: "steppe".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "military_size".to_string(),
                    actor_id: Some("huns".to_string()),
                    operator: ComparisonOperator::Greater,
                    value: 150.0,
                },
                duration: None,
            },
            result: RankResult { rank: "C".to_string() },
            is_key: false,
        },
        RankCondition {
            region_id: "steppe".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "military_size".to_string(),
                    actor_id: Some("huns".to_string()),
                    operator: ComparisonOperator::Greater,
                    value: 300.0,
                },
                duration: None,
            },
            result: RankResult { rank: "B".to_string() },
            is_key: false,
        },
        // Rome loses symbolic status on collapse
        RankCondition {
            region_id: "rome_city".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::Metric {
                    metric: "legitimacy".to_string(),
                    actor_id: Some("rome".to_string()),
                    operator: ComparisonOperator::Less,
                    value: 20.0,
                },
                duration: None,
            },
            result: RankResult { rank: "A".to_string() },
            is_key: true,
        },
        // Mediolanum falls if Rome collapses
        RankCondition {
            region_id: "milan".to_string(),
            condition: EventCondition {
                condition_type: EventConditionType::ActorState {
                    actor_id: "rome".to_string(),
                    state: crate::core::ActorState::Dead,
                },
                duration: None,
            },
            result: RankResult { rank: "B".to_string() },
            is_key: false,
        },
    ]
}

// ============================================================================
// Generation Mechanics
// ============================================================================

fn create_generation_mechanics() -> GenerationMechanics {
    let mut inheritance_coefficients = HashMap::new();
    inheritance_coefficients.insert("family:family_influence".to_string(), 0.85);
    inheritance_coefficients.insert("family:family_knowledge".to_string(), 1.0);
    inheritance_coefficients.insert("family:family_wealth".to_string(), 1.0);
    inheritance_coefficients.insert("family:family_connections".to_string(), 0.8);

    GenerationMechanics {
        tick_span: 5,
        patriarch_start_age: 42,
        patriarch_end_age: 75,
        generation_length: 33,
        inheritance_coefficients,
        panel_label: "Семья Ди Милано".to_string(),
        era_texts: vec![
            crate::core::EraText { from_year: 375, to_year: 410, text: "Рим трещит по швам. Семья держит позиции при дворе.".to_string() },
            crate::core::EraText { from_year: 410, to_year: 455, text: "Западная империя агонизирует. Влияние семьи — последний якорь.".to_string() },
            crate::core::EraText { from_year: 455, to_year: 500, text: "Из пепла рождается новый порядок.".to_string() },
        ],
    }
}

// ============================================================================
// LLM Context
// ============================================================================

fn create_llm_context() -> String {
    r#"СЦЕНАРИЙ: Рим 375 — Семья Ди Милано
РОЛЬ ИГРОКА: Глава незаметной семьи в Медиолане. Не Валент, не Амброзий. Человек который видит.

КОНТЕКСТ:
375 год. Медиолан — фактическая столица Западной Империи.
Гунны за горизонтом давят на готов. Готы просятся за Дунай.
Через три года Адрианополь. Но это ещё не случилось.
Гунны в 375 году — слухи на краю ойкумены, не факт.

СЕМЬЯ ДИ МИЛАНО:
Никто не знает кто они. Незаметные. Читающие. Осторожные.
Если Рим выстоит — семья поднимается. Но начинают с нуля.

МЕТРИКИ СЕМЬИ:
family_influence (0-100): политический вес в городе и при дворе
family_knowledge (0-100): накопленная учёность, архивы, контакты
family_wealth (0-100): финансовая база, торговые связи
family_connections (0-100): сеть людей которые тебе должны

МЕХАНИКА ПОКОЛЕНИЙ:
Tick_span = 5 лет. Главы семьи сменяются.
Patriarch начинает в 42 года. При ~75 — передача власти.
Новый глава наследует метрики семьи, характер генерируется заново.

ТРИ ПУТИ:
1. Рим выстоял — германо-римский синтез, семья в новой элите
2. Классический распад — семья выживает в хаосе варварских королевств
3. Катастрофа — семья теряется

ТОНАЛЬНОСТЬ:
Поздняя античность. Латынь живая. Христианство новое но уже власть.
Рим ещё существует — но что-то изменилось, люди это чувствуют.
Нарратив от третьего лица, через конкретные сцены жизни семьи.
Имена персонажей латинские. 3–5 абзацев за тик.

НЕ ДЕЛАТЬ:
- Не предрешать падение Рима
- Не игнорировать масштаб семьи — они малые люди в большой истории
- Гунны в 375 году невидимы для большинства"#.to_string()
}

fn create_status_indicators() -> Vec<crate::core::StatusIndicator> {
    use crate::core::StatusIndicator;
    vec![
        StatusIndicator {
            label: "Западная Империя".to_string(),
            metric: "actor:rome.external_pressure".to_string(),
            invert: true,
            thresholds: vec![
                (0.0, "стабильна".to_string()),
                (50.0, "под давлением".to_string()),
                (75.0, "распадается".to_string()),
            ],
        },
        StatusIndicator {
            label: "Натиск варваров".to_string(),
            metric: "actor:visigoths.military_size".to_string(),
            invert: true,
            thresholds: vec![
                (0.0, "слабый".to_string()),
                (80.0, "опасный".to_string()),
                (150.0, "неудержимый".to_string()),
            ],
        },
        StatusIndicator {
            label: "Семья Ди Милано".to_string(),
            metric: "family:family_influence".to_string(),
            invert: false,
            thresholds: vec![
                (0.0, "незначительна".to_string()),
                (30.0, "заметна".to_string()),
                (60.0, "влиятельна".to_string()),
            ],
        },
    ]
}

fn create_consequence_context() -> String {
    r#"Сценарный период завершён. Симуляция продолжается.
Семья Ди Милано пережила первый кризис — или не пережила.
Нарратив охватывает более широкий период истории.
Роль игрока — наблюдатель с ограниченным влиянием.
Семья продолжает существовать в том мире который сложился."#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::WorldState;
    use crate::engine::{tick, EventLog};
    use rand::SeedableRng;

    #[test]
    fn test_load_rome_375_has_actors() {
        let scenario = load_rome_375();
        // 15 base actors + 7 successor templates = 22 total
        assert_eq!(scenario.actors.len(), 22);
    }

    #[test]
    fn test_successor_templates_not_in_world() {
        let scenario = load_rome_375();
        // Verify successor templates are marked correctly
        let rome_west = scenario.actors.iter().find(|a| a.id == "rome_west").unwrap();
        assert!(rome_west.is_successor_template);
        
        let visigoth_kingdom = scenario.actors.iter().find(|a| a.id == "visigoth_kingdom").unwrap();
        assert!(visigoth_kingdom.is_successor_template);
        
        // Base actors should not be templates
        let rome = scenario.actors.iter().find(|a| a.id == "rome").unwrap();
        assert!(!rome.is_successor_template);
    }

    #[test]
    fn test_rome_has_correct_metrics() {
        let scenario = load_rome_375();
        let rome = scenario.actors.iter().find(|a| a.id == "rome").unwrap();
        assert_eq!(rome.metrics.population, 8000.0);
        assert_eq!(rome.metrics.military_size, 350.0);
        assert_eq!(rome.metrics.treasury, 1800.0);
    }

    #[test]
    fn test_scenario_has_milestone_events() {
        let scenario = load_rome_375();
        assert_eq!(scenario.milestone_events.len(), 5);
    }

    #[test]
    fn test_scenario_has_patron_actions() {
        let scenario = load_rome_375();
        assert_eq!(scenario.patron_actions.len(), 9);
    }

    #[test]
    fn test_rome_economic_output_population_bonus_reduced() {
        // Test that the population bonus coefficient reduction is working
        // Old: (8000-5000) * 0.0005 = 1.5 per tick from population alone
        // New: (8000-3000) * 0.00005 = 0.25 per tick from population
        let scenario = load_rome_375();
        let mut world = WorldState::new(scenario.id.clone(), scenario.start_year);
        
        // Initialize world with scenario actors (clone to preserve scenario)
        for actor in &scenario.actors {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
        
        let mut event_log = EventLog::new();

        let initial_economic_output = world.actors.get("rome").unwrap().metrics.economic_output;

        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        for _ in 0..10 {
            tick(&mut world, &scenario, &mut event_log, &mut rng);
        }

        let final_economic_output = world.actors.get("rome").unwrap().metrics.economic_output;
        
        // With the reduced coefficient, economic output growth should be limited
        // Growth should be less than 60 points over 10 ticks (was much higher before)
        // Old coefficient would give 1.5/tick from population alone = 15+ points
        // New coefficient gives 0.25/tick from population = 2.5 points
        let growth = final_economic_output - initial_economic_output;
        assert!(growth < 60.0,
            "Economic output growth should be limited: grew by {} (from {} to {})",
            growth, initial_economic_output, final_economic_output);
    }
}

fn create_universal_actions() -> Vec<crate::core::PatronAction> {
    use crate::core::{PatronAction, ActionCondition, ComparisonOperator};
    use std::collections::HashMap;

    vec![
        // 1. Observe - always available, no effects, no cost
        PatronAction {
            id: "observe".to_string(),
            name: "Наблюдать".to_string(),
            source_actor_id: None,
            available_if: ActionCondition::Always,
            effects: HashMap::new(),
            cost: HashMap::new(),
        },
        // 2. Support Stability - requires family_wealth > 50
        PatronAction {
            id: "support_stability".to_string(),
            name: "Поддержать стабильность".to_string(),
            source_actor_id: None,
            available_if: ActionCondition::Metric {
                metric: "family:family_wealth".to_string(),
                operator: ComparisonOperator::Greater,
                value: 50.0,
            },
            effects: HashMap::from([
                ("family:family_cohesion".to_string(), 3.0),
                ("family:family_legitimacy".to_string(), 2.0),
            ]),
            cost: HashMap::from([
                ("family:family_wealth".to_string(), -50.0),
            ]),
        },
        // 3. Raise Taxes - always available
        PatronAction {
            id: "raise_taxes".to_string(),
            name: "Повысить налоги".to_string(),
            source_actor_id: None,
            available_if: ActionCondition::Always,
            effects: HashMap::from([
                ("family:family_wealth".to_string(), 80.0),
                ("family:family_cohesion".to_string(), -3.0),
                ("family:family_legitimacy".to_string(), -5.0),
            ]),
            cost: HashMap::new(),
        },
        // 4. Recruit Soldiers - requires family_wealth > 100
        PatronAction {
            id: "recruit_soldiers".to_string(),
            name: "Нанять солдат".to_string(),
            source_actor_id: None,
            available_if: ActionCondition::Metric {
                metric: "family:family_wealth".to_string(),
                operator: ComparisonOperator::Greater,
                value: 100.0,
            },
            effects: HashMap::from([
                ("actor:rome.military_size".to_string(), 10.0),
                ("actor:rome.military_quality".to_string(), -5.0),
            ]),
            cost: HashMap::from([
                ("family:family_wealth".to_string(), -100.0),
            ]),
        },
    ]
}

fn create_random_events() -> Vec<crate::core::RandomEvent> {
    use crate::core::{Condition, EventTarget, ComparisonOperator, RandomEvent};
    use std::collections::HashMap;

    vec![
        RandomEvent {
            id: "legate_betrayal".to_string(),
            probability: 0.06,
            target: EventTarget::Actor("rome".to_string()),
            conditions: vec![],
            effects: HashMap::from([
                ("family:influence".to_string(), -10.0),
                ("actor:rome.legitimacy".to_string(), -5.0),
            ]),
            llm_context: "Предательство легата ослабило позиции семьи при дворе".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "barbarian_raid".to_string(),
            probability: 0.10,
            target: EventTarget::Actor("rome".to_string()),
            conditions: vec![
                Condition { metric: "actor:visigoths.military_size".to_string(), operator: ComparisonOperator::Greater, value: 80.0 },
            ],
            effects: HashMap::from([
                ("actor:rome.cohesion".to_string(), -10.0),
                ("actor:rome.economic_output".to_string(), -8.0),
                ("actor:rome.external_pressure".to_string(), 5.0),
            ]),
            llm_context: "Варварский набег разорил приграничные провинции".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "oracle_revelation".to_string(),
            probability: 0.04,
            target: EventTarget::Actor("rome".to_string()),
            conditions: vec![],
            effects: HashMap::from([
                ("actor:rome.legitimacy".to_string(), 8.0),
                ("family:influence".to_string(), 5.0),
            ]),
            llm_context: "Пророчество оракула укрепило авторитет власти".to_string(),
            one_time: true,
        },
        RandomEvent {
            id: "senator_bribe".to_string(),
            probability: 0.07,
            target: EventTarget::Actor("rome".to_string()),
            conditions: vec![
                Condition { metric: "family:wealth".to_string(), operator: ComparisonOperator::Greater, value: 200.0 },
            ],
            effects: HashMap::from([
                ("family:influence".to_string(), 8.0),
                ("family:wealth".to_string(), -100.0),
                ("actor:rome.legitimacy".to_string(), 3.0),
            ]),
            llm_context: "Подкуп сенаторов укрепил позиции семьи в Риме".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "gladiator_revolt".to_string(),
            probability: 0.05,
            target: EventTarget::Actor("rome".to_string()),
            conditions: vec![
                Condition { metric: "actor:rome.cohesion".to_string(), operator: ComparisonOperator::Less, value: 40.0 },
            ],
            effects: HashMap::from([
                ("actor:rome.cohesion".to_string(), -12.0),
                ("actor:rome.legitimacy".to_string(), -8.0),
                ("family:influence".to_string(), -5.0),
            ]),
            llm_context: "Восстание гладиаторов обнажило слабость императорской власти".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "silk_road_caravan".to_string(),
            probability: 0.06,
            target: EventTarget::Actor("rome".to_string()),
            conditions: vec![
                Condition { metric: "actor:rome.economic_output".to_string(), operator: ComparisonOperator::Greater, value: 30.0 },
            ],
            effects: HashMap::from([
                ("family:wealth".to_string(), 120.0),
                ("family:connections".to_string(), 5.0),
                ("actor:rome.economic_output".to_string(), 3.0),
            ]),
            llm_context: "Богатый торговый караван с Востока принёс редкие товары и новые связи".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "army_mutiny".to_string(),
            probability: 0.06,
            target: EventTarget::Actor("rome".to_string()),
            conditions: vec![
                Condition { metric: "actor:rome.military_size".to_string(), operator: ComparisonOperator::Greater, value: 100.0 },
                Condition { metric: "actor:rome.treasury".to_string(), operator: ComparisonOperator::Less, value: 150.0 },
            ],
            effects: HashMap::from([
                ("actor:rome.military_size".to_string(), -30.0),
                ("actor:rome.legitimacy".to_string(), -12.0),
                ("actor:rome.cohesion".to_string(), -8.0),
            ]),
            llm_context: "Мятеж легионов потряс Рим — солдаты требуют жалования".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "divine_omen".to_string(),
            probability: 0.04,
            target: EventTarget::Actor("rome".to_string()),
            conditions: vec![],
            effects: HashMap::from([
                ("actor:rome.cohesion".to_string(), 12.0),
                ("family:influence".to_string(), 8.0),
                ("actor:rome.legitimacy".to_string(), 6.0),
            ]),
            llm_context: "Знамение богов укрепило веру народа в предназначение Рима".to_string(),
            one_time: true,
        },
    ]
}
