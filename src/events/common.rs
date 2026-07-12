use std::collections::HashMap;
use crate::core::{RandomEvent, EventTarget, RelativeCondition, ComparisonOperator};

/// Common random events available to all scenarios
pub fn common_events() -> Vec<RandomEvent> {
    vec![
        RandomEvent {
            id: "plague".to_string(),
            probability: 0.10,
            target: EventTarget::Any,
            conditions: vec![
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.population"), operator: ComparisonOperator::Greater, value: 500.0 },
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.cohesion"), operator: ComparisonOperator::Less, value: 60.0 },
            ],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.population"), -25.0),
                (crate::core::RelativeMetricRef::literal("self.cohesion"), -6.0),
                (crate::core::RelativeMetricRef::literal("self.economic_output"), -5.0),
            ]),
            llm_context: "Эпидемия чумы опустошила регион".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "famine".to_string(),
            probability: 0.12,
            target: EventTarget::Any,
            conditions: vec![
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.economic_output"), operator: ComparisonOperator::Less, value: 30.0 },
            ],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.treasury"), -60.0),
                (crate::core::RelativeMetricRef::literal("self.cohesion"), -5.0),
                (crate::core::RelativeMetricRef::literal("self.population"), -20.0),
            ]),
            llm_context: "Неурожай вызвал голод и волнения".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "earthquake".to_string(),
            probability: 0.03,
            target: EventTarget::Any,
            conditions: vec![],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.cohesion"), -15.0),
                (crate::core::RelativeMetricRef::literal("self.economic_output"), -10.0),
            ]),
            llm_context: "Землетрясение разрушило часть города".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "court_conspiracy".to_string(),
            probability: 0.12,
            target: EventTarget::Any,
            conditions: vec![
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.legitimacy"), operator: ComparisonOperator::Less, value: 60.0 },
            ],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.legitimacy"), -6.0),
                (crate::core::RelativeMetricRef::literal("self.cohesion"), -5.0),
            ]),
            llm_context: "Заговор при дворе ослабил власть правителя".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "desertion".to_string(),
            probability: 0.09,
            target: EventTarget::Any,
            conditions: vec![
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.military_size"), operator: ComparisonOperator::Greater, value: 50.0 },
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.treasury"), operator: ComparisonOperator::Less, value: 200.0 },
            ],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.military_size"), -12.0),
                (crate::core::RelativeMetricRef::literal("self.cohesion"), -5.0),
            ]),
            llm_context: "Солдаты дезертировали из-за нехватки жалования".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "piracy".to_string(),
            probability: 0.11,
            target: EventTarget::SeaActors,
            conditions: vec![],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.treasury"), -50.0),
                (crate::core::RelativeMetricRef::literal("self.economic_output"), -5.0),
            ]),
            llm_context: "Пираты нарушили торговые пути".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "mercenary_influx".to_string(),
            probability: 0.07,
            target: EventTarget::Any,
            conditions: vec![
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.treasury"), operator: ComparisonOperator::Greater, value: 300.0 },
            ],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.military_size"), 30.0),
                (crate::core::RelativeMetricRef::literal("self.treasury"), -100.0),
            ]),
            llm_context: "Отряд наёмников предложил услуги за золото".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "trade_boom".to_string(),
            probability: 0.10,
            target: EventTarget::Any,
            conditions: vec![
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.economic_output"), operator: ComparisonOperator::Greater, value: 40.0 },
            ],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.treasury"), 80.0),
                (crate::core::RelativeMetricRef::literal("self.economic_output"), 5.0),
            ]),
            llm_context: "Торговый подъём наполнил казну".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "popular_uprising".to_string(),
            probability: 0.08,
            target: EventTarget::Any,
            conditions: vec![
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.cohesion"), operator: ComparisonOperator::Less, value: 30.0 },
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.legitimacy"), operator: ComparisonOperator::Less, value: 40.0 },
            ],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.cohesion"), -8.0),
                (crate::core::RelativeMetricRef::literal("self.legitimacy"), -6.0),
                (crate::core::RelativeMetricRef::literal("self.economic_output"), -8.0),
            ]),
            llm_context: "Народное восстание потрясло столицу".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "flood".to_string(),
            probability: 0.08,
            target: EventTarget::Any,
            conditions: vec![],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.economic_output"), -12.0),
                (crate::core::RelativeMetricRef::literal("self.population"), -15.0),
                (crate::core::RelativeMetricRef::literal("self.cohesion"), -5.0),
            ]),
            llm_context: "Наводнение уничтожило урожай и разрушило дороги".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "charismatic_preacher".to_string(),
            probability: 0.05,
            target: EventTarget::Any,
            conditions: vec![
                RelativeCondition { metric: crate::core::RelativeMetricRef::literal("self.cohesion"), operator: ComparisonOperator::Less, value: 40.0 },
            ],
            effects: HashMap::from([
                (crate::core::RelativeMetricRef::literal("self.cohesion"), 3.0),
                (crate::core::RelativeMetricRef::literal("self.legitimacy"), 2.0),
            ]),
            llm_context: "Харизматичный проповедник сплотил народ вокруг правителя".to_string(),
            one_time: false,
        },
    ]
}
