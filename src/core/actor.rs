use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap};

/// Default metric values for all actors
pub fn default_metrics() -> HashMap<String, f64> {
    [
        ("population", 1000.0),
        ("military_size", 50.0),
        ("military_quality", 50.0),
        ("economic_output", 50.0),
        ("cohesion", 50.0),
        ("legitimacy", 50.0),
        ("external_pressure", 30.0),
        ("treasury", 100.0),
    ].iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// Ensure all default metrics exist in the HashMap
pub fn ensure_default_metrics(metrics: &mut HashMap<String, f64>) {
    for (k, v) in default_metrics() {
        metrics.entry(k).or_insert(v);
    }
}

/// Convert metrics to snapshot with sorted keys for deterministic output
pub fn metrics_to_snapshot(metrics: &HashMap<String, f64>) -> HashMap<String, f64> {
    metrics.iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect()
}

/// Neighbor relationship with distance and border type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neighbor {
    pub id: String,
    pub distance: u32,
    pub border_type: BorderType,
}

/// Type of border between actors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BorderType {
    Land,
    Sea,
}

/// Successor definition for on_collapse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Successor {
    pub id: String,
    pub weight: f64,
}

/// Actor tag with metrics modifier and spread mechanics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorTag {
    pub metrics_modifier: HashMap<String, i32>,
    pub spreads_via: Vec<TagSpreadType>,
}

/// How a tag spreads between actors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TagSpreadType {
    War,
    Trade,
    Culture,
    Migration,
    Conquest,
}

/// Narrative status of an actor
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeStatus {
    Foreground,
    Background,
}

/// Era of an actor (determines available tags)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Era {
    Ancient,
    EarlyMedieval,
    HighMedieval,
    LateMedieval,
    EarlyModern,
}

/// Region rank (D → C → B → A → S)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "UPPERCASE")]
pub enum RegionRank {
    D,
    C,
    B,
    A,
    S,
}

/// Main Actor structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub id: String,
    pub name: String,
    pub name_short: String,
    pub region: String,
    pub region_rank: RegionRank,
    pub era: Era,
    pub narrative_status: NarrativeStatus,
    pub tags: Vec<String>,
    pub metrics: HashMap<String, f64>,
    pub scenario_metrics: HashMap<String, f64>,
    pub neighbors: Vec<Neighbor>,
    pub on_collapse: Vec<Successor>,
    pub actor_tags: HashMap<String, ActorTag>,
    pub center: Option<GeoCoordinate>,
    /// If true, this actor is a template for successor creation and should not be added to world.actors at start
    pub is_successor_template: bool,
    /// Religion of the actor
    pub religion: Religion,
    /// Culture of the actor
    pub culture: Culture,
    /// Minimum ticks this actor must survive before collapse is possible
    pub minimum_survival_ticks: Option<u32>,
    /// Static leader name for narrative purposes
    pub leader: Option<String>,
}

impl Actor {
    /// Get metric value (returns 0.0 if missing)
    pub fn get_metric(&self, key: &str) -> f64 {
        self.metrics.get(key).copied().unwrap_or(0.0)
    }

    /// Set metric value
    pub fn set_metric(&mut self, key: &str, value: f64) {
        self.metrics.insert(key.to_string(), value);
    }

    /// Add delta to metric (creates if missing)
    pub fn add_metric(&mut self, key: &str, delta: f64) {
        let v = self.metrics.entry(key.to_string()).or_insert(0.0);
        *v += delta;
    }

    /// Clamp metric to range (only if key exists - doesn't create missing metrics)
    pub fn clamp_metric(&mut self, key: &str, min: f64, max: f64) {
        if let Some(v) = self.metrics.get_mut(key) {
            *v = v.clamp(min, max);
        }
    }

    /// Calculate derived stability metric
    pub fn stability(&self) -> f64 {
        (self.get_metric("legitimacy") * 0.4 + self.get_metric("cohesion") * 0.4)
            - (self.get_metric("external_pressure") * 0.2)
    }

    /// Calculate derived power_projection metric
    /// Normalized relative to max military_size among living actors
    pub fn power_projection(&self, era_modifier: f64, max_military_size: f64) -> f64 {
        const TREASURY_NORM_CAP: f64 = 500.0; // treasury >= 500 counts as full contribution

        let military_size_norm = if max_military_size > 0.0 {
            (self.get_metric("military_size") / max_military_size).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let military_quality_norm = (self.get_metric("military_quality") / 100.0).clamp(0.0, 1.0);
        let treasury_norm = (self.get_metric("treasury") / TREASURY_NORM_CAP).clamp(0.0, 1.0);

        let power_projection =
            military_size_norm * 0.45 +
            military_quality_norm * 0.35 +
            treasury_norm * 0.20;

        power_projection * 100.0 * era_modifier
    }
}

/// Religion enum for actors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Religion {
    Catholic,
    Orthodox,
    Muslim,
    Pagan,
    Buddhist,
    Hindu,
    Other,
}

/// Culture enum for actors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Culture {
    Latin,
    Greek,
    Slavic,
    Germanic,
    Arabic,
    Turkic,
    Persian,
    Indian,
    EastAsian,
    Other,
}

/// Geographic coordinate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoCoordinate {
    pub lat: f64,
    pub lng: f64,
}
