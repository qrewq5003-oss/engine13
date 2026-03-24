use serde::{Deserialize, Serialize};
use crate::core::WorldState;

/// Normalize family metric key to canonical short-key format.
/// 
/// Canonical external format: `family:<short_key>` (e.g., `family:influence`)
/// Legacy input format (backward-compat only): `family:family_<short_key>` (e.g., `family:family_influence`)
/// 
/// This function normalizes both to short-key for internal storage:
/// - "influence" -> "influence"
/// - "family_influence" -> "influence"
fn normalize_family_key(raw: &str) -> String {
    raw.strip_prefix("family_").unwrap_or(raw).to_string()
}

/// Reference to a metric in the world state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricRef {
    /// Actor-specific metric: "actor:id.metric"
    Actor { actor_id: String, metric: String },
    /// Family metric: "family:key" (canonical) or "family:family_key" (legacy, normalized)
    Family { key: String },
    /// Global metric: "global:key" or plain key
    Global { key: String },
}

impl MetricRef {
    /// Parse a string into a MetricRef
    ///
    /// Explicit prefixes only:
    /// - "family:key" → MetricRef::Family (key normalized to short-key)
    /// - "family:family_key" → MetricRef::Family (legacy, normalized to short-key)
    /// - "global:key" → MetricRef::Global
    /// - "actor:id.metric" → MetricRef::Actor
    /// - other → MetricRef::Global (plain key)
    ///
    /// Canonical format: `family:<short_key>` (e.g., `family:influence`)
    /// Legacy format supported for backward compatibility: `family:family_<short_key>`
    pub fn parse(s: &str) -> Self {
        // Check explicit prefixes first
        if let Some(key) = s.strip_prefix("family:") {
            // Normalize family key to canonical short-key format
            MetricRef::Family { key: normalize_family_key(key) }
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
                // Family metrics stored in family_state.metrics using short-key format
                // key is already normalized by parse()
                world_state.family_state.as_ref()
                    .and_then(|fs| fs.metrics.get(key))
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
                // Family metrics stored in family_state.metrics using short-key format
                // key is already normalized by parse()
                if let Some(ref mut fs) = world_state.family_state {
                    let val = fs.metrics.entry(key.clone()).or_insert(0.0);
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
