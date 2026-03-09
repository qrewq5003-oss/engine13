use std::collections::HashMap;
use crate::core::{RandomEvent, EventTarget, Condition, ComparisonOperator};

/// Common random events available to all scenarios
pub fn common_events() -> Vec<RandomEvent> {
    vec![
        RandomEvent {
            id: "plague".to_string(),
            probability: 0.10,
            target: EventTarget::Any,
            conditions: vec![
                Condition { metric: "self.population".to_string(), operator: ComparisonOperator::Greater, value: 500.0 },
                Condition { metric: "self.cohesion".to_string(), operator: ComparisonOperator::Less, value: 60.0 },
            ],
            effects: HashMap::from([
                ("self.population".to_string(), -25.0),
                ("self.cohesion".to_string(), -6.0),
                ("self.economic_output".to_string(), -5.0),
            ]),
            llm_context: "Эпидемия чумы опустошила регион".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "famine".to_string(),
            probability: 0.12,
            target: EventTarget::Any,
            conditions: vec![
                Condition { metric: "self.economic_output".to_string(), operator: ComparisonOperator::Less, value: 30.0 },
            ],
            effects: HashMap::from([
                ("self.treasury".to_string(), -60.0),
                ("self.cohesion".to_string(), -5.0),
                ("self.population".to_string(), -20.0),
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
                ("self.cohesion".to_string(), -15.0),
                ("self.economic_output".to_string(), -10.0),
            ]),
            llm_context: "Землетрясение разрушило часть города".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "court_conspiracy".to_string(),
            probability: 0.12,
            target: EventTarget::Any,
            conditions: vec![
                Condition { metric: "self.legitimacy".to_string(), operator: ComparisonOperator::Less, value: 60.0 },
            ],
            effects: HashMap::from([
                ("self.legitimacy".to_string(), -6.0),
                ("self.cohesion".to_string(), -5.0),
            ]),
            llm_context: "Заговор при дворе ослабил власть правителя".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "desertion".to_string(),
            probability: 0.09,
            target: EventTarget::Any,
            conditions: vec![
                Condition { metric: "self.military_size".to_string(), operator: ComparisonOperator::Greater, value: 50.0 },
                Condition { metric: "self.treasury".to_string(), operator: ComparisonOperator::Less, value: 200.0 },
            ],
            effects: HashMap::from([
                ("self.military_size".to_string(), -12.0),
                ("self.cohesion".to_string(), -5.0),
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
                ("self.treasury".to_string(), -50.0),
                ("self.economic_output".to_string(), -5.0),
            ]),
            llm_context: "Пираты нарушили торговые пути".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "mercenary_influx".to_string(),
            probability: 0.07,
            target: EventTarget::Any,
            conditions: vec![
                Condition { metric: "self.treasury".to_string(), operator: ComparisonOperator::Greater, value: 300.0 },
            ],
            effects: HashMap::from([
                ("self.military_size".to_string(), 30.0),
                ("self.treasury".to_string(), -100.0),
            ]),
            llm_context: "Отряд наёмников предложил услуги за золото".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "trade_boom".to_string(),
            probability: 0.10,
            target: EventTarget::Any,
            conditions: vec![
                Condition { metric: "self.economic_output".to_string(), operator: ComparisonOperator::Greater, value: 40.0 },
            ],
            effects: HashMap::from([
                ("self.treasury".to_string(), 80.0),
                ("self.economic_output".to_string(), 5.0),
            ]),
            llm_context: "Торговый подъём наполнил казну".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "popular_uprising".to_string(),
            probability: 0.08,
            target: EventTarget::Any,
            conditions: vec![
                Condition { metric: "self.cohesion".to_string(), operator: ComparisonOperator::Less, value: 30.0 },
                Condition { metric: "self.legitimacy".to_string(), operator: ComparisonOperator::Less, value: 40.0 },
            ],
            effects: HashMap::from([
                ("self.cohesion".to_string(), -8.0),
                ("self.legitimacy".to_string(), -6.0),
                ("self.economic_output".to_string(), -8.0),
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
                ("self.economic_output".to_string(), -12.0),
                ("self.population".to_string(), -15.0),
                ("self.cohesion".to_string(), -5.0),
            ]),
            llm_context: "Наводнение уничтожило урожай и разрушило дороги".to_string(),
            one_time: false,
        },
        RandomEvent {
            id: "charismatic_preacher".to_string(),
            probability: 0.09,
            target: EventTarget::Any,
            conditions: vec![],
            effects: HashMap::from([
                ("self.cohesion".to_string(), 6.0),
                ("self.legitimacy".to_string(), 5.0),
            ]),
            llm_context: "Харизматичный проповедник сплотил народ вокруг правителя".to_string(),
            one_time: false,
        },
    ]
}
