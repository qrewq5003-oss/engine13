use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Base metrics for every actor (any era)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorMetrics {
    pub population: f64,        // thousands
    pub military_size: f64,     // thousands of soldiers
    pub military_quality: f64,  // 0-100
    pub economic_output: f64,   // 0-100
    pub cohesion: f64,          // 0-100
    pub legitimacy: f64,        // 0-100
    pub external_pressure: f64, // 0-100
    pub treasury: f64,          // absolute, can be negative
}

impl Default for ActorMetrics {
    fn default() -> Self {
        Self {
            population: 1000.0,
            military_size: 50.0,
            military_quality: 50.0,
            economic_output: 50.0,
            cohesion: 50.0,
            legitimacy: 50.0,
            external_pressure: 30.0,
            treasury: 100.0,
        }
    }
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
    pub metrics: ActorMetrics,
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

impl Actor {
    /// Calculate derived stability metric
    pub fn stability(&self) -> f64 {
        (self.metrics.legitimacy * 0.4 + self.metrics.cohesion * 0.4)
            - (self.metrics.external_pressure * 0.2)
    }

    /// Calculate derived power_projection metric
    pub fn power_projection(&self, era_modifier: f64) -> f64 {
        let treasury_modifier = if self.metrics.treasury > 500.0 {
            1.2
        } else if self.metrics.treasury > 0.0 {
            1.0
        } else {
            0.7
        };

        (self.metrics.military_size * 0.4 + self.metrics.military_quality * 0.4 + treasury_modifier * 0.2)
            * era_modifier
    }
}
