//! The scenario schema: facts only (brief §0.2), `deny_unknown_fields` at every level.
//!
//! There is no field a coefficient could go into: a rule number in a scenario file is a
//! schema error, not a tuning knob. The engine receives the compiled [`Facts`] and never the
//! strings; the strings go to [`Names`] for the presentation layer.
//!
//! `display_currency` is presentation data: `compile` moves it into [`Names`] next to the
//! strings, and only `view.rs` reads it from there (guard `display_currency_is_read_only_by_the_view`).
//! The engine counts in kilograms of wheat-equivalent (E0 §5); the chronicler and the UI
//! show ducats or solidi.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, ProvinceId};
use crate::verbs::VerbId;
use crate::world::Facts;

/// The scenario file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioData {
    pub id: String,
    pub start_year: i32,
    /// The money the presentation layer shows; the engine does not read it.
    pub display_currency: DisplayCurrency,
    /// Keyed by the scenario's string id of the actor.
    pub actors: BTreeMap<String, ActorData>,
    /// Keyed by the scenario's string id of the province.
    pub provinces: BTreeMap<String, ProvinceData>,
    /// Enabled verbs and their display names, keyed by `VerbId::key`.
    pub verbs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayCurrency {
    pub name: String,
    /// Kilograms of wheat-equivalent one unit buys (E0 §5, Э1б) — a display rate.
    pub kg_per_unit: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorData {
    pub name: String,
    /// Starting treasury in kilograms of wheat-equivalent.
    pub treasury_kg: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceData {
    pub name: String,
    pub owner: String,
    pub capacity: u64,
    pub population: u64,
    pub neighbours: Vec<String>,
}

/// The strings the engine dropped, for the presentation layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Names {
    /// Index = `ActorId`: `(scenario id, display name)`.
    pub actors: Vec<(String, String)>,
    /// Index = `ProvinceId`.
    pub provinces: Vec<(String, String)>,
    /// Enabled verbs with their display names, in `VerbId` order.
    pub verbs: Vec<(VerbId, String)>,
    /// The money to show; a display rate, never an engine input.
    pub currency: DisplayCurrency,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SchemaError {
    #[error("scenario is not valid TOML of the schema: {0}")]
    Toml(String),
    #[error("province '{0}' names unknown owner '{1}'")]
    UnknownOwner(String, String),
    #[error("province '{0}' names unknown neighbour '{1}'")]
    UnknownNeighbour(String, String),
    #[error("province '{0}' lists itself or a neighbour twice")]
    BadNeighbours(String),
    #[error("unknown verb '{0}'")]
    UnknownVerb(String),
    #[error("scenario has no actors or no provinces")]
    Empty,
}

impl ScenarioData {
    pub fn parse(toml_text: &str) -> Result<ScenarioData, SchemaError> {
        toml::from_str(toml_text).map_err(|e| SchemaError::Toml(e.to_string()))
    }

    /// Compile into engine facts and the names table. Ids are assigned in the sorted order
    /// of the string ids (`BTreeMap` order), so the same file always compiles to the same
    /// numbers.
    pub fn compile(&self) -> Result<(Facts, Names), SchemaError> {
        if self.actors.is_empty() || self.provinces.is_empty() {
            return Err(SchemaError::Empty);
        }
        let actor_index: BTreeMap<&str, ActorId> =
            self.actors.keys().enumerate().map(|(i, k)| (k.as_str(), ActorId(i as u32))).collect();
        let province_index: BTreeMap<&str, ProvinceId> =
            self.provinces.keys().enumerate().map(|(i, k)| (k.as_str(), ProvinceId(i as u32))).collect();

        let mut provinces = Vec::with_capacity(self.provinces.len());
        for (key, p) in &self.provinces {
            let owner = *actor_index.get(p.owner.as_str()).ok_or_else(|| SchemaError::UnknownOwner(key.clone(), p.owner.clone()))?;
            let mut neighbours = Vec::with_capacity(p.neighbours.len());
            for n in &p.neighbours {
                let id = *province_index.get(n.as_str()).ok_or_else(|| SchemaError::UnknownNeighbour(key.clone(), n.clone()))?;
                neighbours.push(id);
            }
            neighbours.sort();
            let own = province_index[key.as_str()];
            if neighbours.windows(2).any(|w| w[0] == w[1]) || neighbours.contains(&own) {
                return Err(SchemaError::BadNeighbours(key.clone()));
            }
            provinces.push((p.capacity, p.population, owner, neighbours));
        }
        let treasuries = self.actors.values().map(|a| a.treasury_kg).collect();

        let mut verbs = Vec::new();
        for (key, name) in &self.verbs {
            let id = VerbId::from_key(key).ok_or_else(|| SchemaError::UnknownVerb(key.clone()))?;
            verbs.push((id, name.clone()));
        }
        verbs.sort_by_key(|(id, _)| *id);

        let names = Names {
            actors: self.actors.iter().map(|(k, a)| (k.clone(), a.name.clone())).collect(),
            provinces: self.provinces.iter().map(|(k, p)| (k.clone(), p.name.clone())).collect(),
            verbs,
            currency: self.display_currency.clone(),
        };
        Ok((Facts { start_year: self.start_year, provinces, treasuries }, names))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"
id = "t"
start_year = 1477
[display_currency]
name = "ducat"
kg_per_unit = 100.0
[actors.b]
name = "B"
treasury_kg = 10
[actors.a]
name = "A"
treasury_kg = 20
[provinces.p2]
name = "P2"
owner = "b"
capacity = 100
population = 50
neighbours = ["p1"]
[provinces.p1]
name = "P1"
owner = "a"
capacity = 100
population = 50
neighbours = ["p2"]
[verbs]
raise_tax = "Поднять налог"
"#;

    #[test]
    fn compiles_with_ids_in_sorted_order() {
        let s = ScenarioData::parse(OK).unwrap();
        let (facts, names) = s.compile().unwrap();
        assert_eq!(names.actors[0].0, "a");
        assert_eq!(names.actors[1].0, "b");
        assert_eq!(facts.treasuries, vec![20, 10]);
        assert_eq!(facts.provinces[0].2, ActorId(0)); // p1 owned by a
        assert_eq!(facts.provinces[1].2, ActorId(1)); // p2 owned by b
        assert_eq!(names.verbs, vec![(VerbId::RaiseTax, "Поднять налог".to_string())]);
    }

    #[test]
    fn a_rule_number_has_nowhere_to_go() {
        let bad = OK.replacen("treasury_kg = 10", "treasury_kg = 10\ngrowth_rate = 0.02", 1);
        assert!(matches!(ScenarioData::parse(&bad), Err(SchemaError::Toml(_))));
        let bad = OK.replacen("start_year = 1477", "start_year = 1477\nconflict_probability = 0.3", 1);
        assert!(matches!(ScenarioData::parse(&bad), Err(SchemaError::Toml(_))));
    }

    #[test]
    fn dangling_references_are_errors() {
        let bad = OK.replacen("owner = \"b\"", "owner = \"zzz\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(), Err(SchemaError::UnknownOwner(..))));
        let bad = OK.replacen("neighbours = [\"p1\"]", "neighbours = [\"nowhere\"]", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(), Err(SchemaError::UnknownNeighbour(..))));
        let bad = OK.replacen("raise_tax", "invent_gunpowder", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(), Err(SchemaError::UnknownVerb(_))));
    }
}
