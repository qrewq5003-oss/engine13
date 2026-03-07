use serde::{Deserialize, Serialize};
use crate::core::WorldState;

/// Reference to a metric in the world state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricRef {
    /// Actor-specific metric: "actor_id.metric" (e.g., "venice.treasury")
    Actor { actor_id: String, metric: String },
    /// Family metric: "family_*" (e.g., "family_influence")
    Family { key: String },
    /// Global metric: other keys (e.g., "federation_progress")
    Global { key: String },
}

impl MetricRef {
    /// Parse a string into a MetricRef
    /// - "family_*" → MetricRef::Family
    /// - "actor_id.metric" → MetricRef::Actor
    /// - other → MetricRef::Global
    pub fn parse(s: &str) -> Self {
        if s.starts_with("family_") {
            MetricRef::Family { key: s.to_string() }
        } else if s.contains('.') {
            let parts: Vec<&str> = s.splitn(2, '.').collect();
            MetricRef::Actor {
                actor_id: parts[0].to_string(),
                metric: parts[1].to_string(),
            }
        } else {
            MetricRef::Global { key: s.to_string() }
        }
    }

    /// Get the metric value from world_state
    pub fn get(&self, world_state: &WorldState) -> f64 {
        match self {
            MetricRef::Actor { actor_id, metric } => {
                world_state.actors.get(actor_id)
                    .map(|a| Self::get_actor_metric(&a.metrics, metric))
                    .unwrap_or(0.0)
            }
            MetricRef::Family { key } => {
                world_state.family_metrics.get(key)
                    .copied()
                    .unwrap_or(0.0)
            }
            MetricRef::Global { key } => {
                world_state.global_metrics.get(key)
                    .copied()
                    .unwrap_or(0.0)
            }
        }
    }

    /// Apply a delta to the metric in world_state
    pub fn apply(&self, world_state: &mut WorldState, delta: f64) {
        match self {
            MetricRef::Actor { actor_id, metric } => {
                if let Some(actor) = world_state.actors.get_mut(actor_id) {
                    Self::apply_actor_metric_delta(&mut actor.metrics, metric, delta);
                }
            }
            MetricRef::Family { key } => {
                let val = world_state.family_metrics.entry(key.clone()).or_insert(0.0);
                *val += delta;
            }
            MetricRef::Global { key } => {
                let val = world_state.global_metrics.entry(key.clone()).or_insert(0.0);
                *val = (*val + delta).clamp(0.0, 100.0);
            }
        }
    }

    /// Get actor metric value by name
    fn get_actor_metric(metrics: &crate::core::ActorMetrics, name: &str) -> f64 {
        match name {
            "population" => metrics.population,
            "military_size" => metrics.military_size,
            "military_quality" => metrics.military_quality,
            "economic_output" => metrics.economic_output,
            "cohesion" => metrics.cohesion,
            "legitimacy" => metrics.legitimacy,
            "external_pressure" => metrics.external_pressure,
            "treasury" => metrics.treasury,
            _ => 0.0,
        }
    }

    /// Apply delta to actor metric
    fn apply_actor_metric_delta(metrics: &mut crate::core::ActorMetrics, metric: &str, delta: f64) {
        match metric {
            "population" => metrics.population += delta,
            "military_size" => metrics.military_size += delta,
            "military_quality" => metrics.military_quality += delta,
            "economic_output" => metrics.economic_output += delta,
            "cohesion" => metrics.cohesion += delta,
            "legitimacy" => metrics.legitimacy += delta,
            "external_pressure" => metrics.external_pressure += delta,
            "treasury" => metrics.treasury += delta,
            _ => {}
        }
    }
}
