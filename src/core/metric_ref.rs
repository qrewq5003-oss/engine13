use serde::{Deserialize, Serialize};
use crate::core::WorldState;

/// Reference to a metric in the world state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricRef {
    /// Actor-specific metric: "actor:id.metric"
    Actor { actor_id: String, metric: String },
    /// Family metric: "family:key"
    Family { key: String },
    /// Global metric: "global:key" or plain key
    Global { key: String },
}

impl MetricRef {
    /// Parse a string into a MetricRef
    ///
    /// Explicit prefixes only:
    /// - "family:key" → MetricRef::Family
    /// - "global:key" → MetricRef::Global
    /// - "actor:id.metric" → MetricRef::Actor
    /// - other → MetricRef::Global (plain key)
    pub fn parse(s: &str) -> Self {
        // Check explicit prefixes first
        if let Some(key) = s.strip_prefix("family:") {
            MetricRef::Family { key: key.to_string() }
        } else if let Some(key) = s.strip_prefix("global:") {
            MetricRef::Global { key: key.to_string() }
        } else if let Some(rest) = s.strip_prefix("actor:") {
            // "actor:id.metric" format
            if let Some((actor_id, metric)) = rest.split_once('.') {
                MetricRef::Actor { actor_id: actor_id.to_string(), metric: metric.to_string() }
            } else {
                // Invalid format, treat as global
                MetricRef::Global { key: s.to_string() }
            }
        } else {
            // Plain string → Global
            MetricRef::Global { key: s.to_string() }
        }
    }

    /// Get the metric value from world_state
    pub fn get(&self, world_state: &WorldState) -> f64 {
        match self {
            MetricRef::Actor { actor_id, metric } => {
                world_state.actors.get(actor_id)
                    .and_then(|a| a.metrics.get(metric))
                    .copied()
                    .unwrap_or(0.0)
            }
            MetricRef::Family { key } => {
                // Family metrics stored in family_state.metrics
                // Handle both "family:key" format (key without prefix) and "family_*" format (key with prefix)
                let metric_key = key.strip_prefix("family_").unwrap_or(key);
                world_state.family_state.as_ref()
                    .and_then(|fs| fs.metrics.get(metric_key))
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
                    let metric_name = metric.as_str();
                    let current = actor.metrics.get(metric_name).copied().unwrap_or(0.0);
                    let new_value = match metric_name {
                        "treasury" => current + delta, // can go negative (debts)
                        "economic_output" | "military_size" | "population" => (current + delta).max(0.0),
                        _ => (current + delta).clamp(0.0, 100.0), // cohesion, legitimacy, etc.
                    };
                    actor.metrics.insert(metric_name.to_string(), new_value);
                }
            }
            MetricRef::Family { key } => {
                // Family metrics stored in family_state.metrics
                // Handle both "family:key" format (key without prefix) and "family_*" format (key with prefix)
                let metric_key = key.strip_prefix("family_").unwrap_or(key).to_string();
                if let Some(ref mut fs) = world_state.family_state {
                    let val = fs.metrics.entry(metric_key).or_insert(0.0);
                    *val += delta;
                }
            }
            MetricRef::Global { key } => {
                let val = world_state.global_metrics.entry(key.clone()).or_insert(0.0);
                *val = (*val + delta).clamp(0.0, 100.0);
            }
        }
    }
}
