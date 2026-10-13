//! The scenario schema: facts only (brief §0.2), `deny_unknown_fields` at every level.
//!
//! There is no field a coefficient could go into: a rule number in a scenario file is a
//! schema error, not a tuning knob. A province gives its **area and land**, never its
//! capacity: capacity is `area × density(land)` with the density a constant of
//! `params.toml` (E0 П1) — an authored capacity would tune growth per province, which is
//! the per-scenario fitting of brief §0.1.
//!
//! Geography is two graphs of facts: `neighbours` (land) and `sea_neighbours` (which coasts
//! face each other across the water). A sea link is what makes an island reachable; the
//! `coastal` mark alone is not a link (plan §14). Both are checked symmetric.
//!
//! A polity gives its capital and its starting army in soldiers (E0 А11); the army starts as
//! one unit at the capital.
//!
//! The engine receives the compiled [`Facts`] and the starting [`Names`], which then live in
//! the world (an heir born in the game is named there). What is presentation only — the
//! verbs' display names and the display currency — stays in [`Display`], and only `view.rs`
//! reads the currency (guard `display_currency_is_read_only_by_the_view`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, ProvinceId};
use crate::params::Params;
use crate::verbs::VerbId;
use crate::world::{ActorFact, Facts, Land, Names, ProvinceFact};

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
    /// The province id of the capital; must be one of the actor's own.
    pub capital: String,
    /// Starting army in soldiers, one unit at the capital.
    pub soldiers: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceData {
    pub name: String,
    pub owner: String,
    pub area_km2: u64,
    pub land: Land,
    /// Touches the sea.
    pub coastal: bool,
    pub population: u64,
    /// Land neighbours.
    pub neighbours: Vec<String>,
    /// Coasts faced across the water; only on coastal provinces.
    pub sea_neighbours: Vec<String>,
}

/// Presentation data that is not part of the world: verbs' display names and the money.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Display {
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
    #[error("province '{0}' and '{1}' disagree on being neighbours")]
    Asymmetric(String, String),
    #[error("province '{0}' has sea neighbours but is not coastal")]
    SeaWithoutCoast(String),
    #[error("actor '{0}' names capital '{1}', which is not a province it owns")]
    BadCapital(String, String),
    #[error("unknown verb '{0}'")]
    UnknownVerb(String),
    #[error("scenario has no actors or no provinces")]
    Empty,
    #[error("province '{province}' starts with {population} people on land that carries {capacity}")]
    OverCapacity { province: String, population: u64, capacity: u64 },
}

/// Everything a scenario compiles to.
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    pub facts: Facts,
    pub names: Names,
    pub display: Display,
}

impl ScenarioData {
    pub fn parse(toml_text: &str) -> Result<ScenarioData, SchemaError> {
        toml::from_str(toml_text).map_err(|e| SchemaError::Toml(e.to_string()))
    }

    /// Compile into engine facts, the starting names and the presentation data. Ids are
    /// assigned in the sorted order of the string ids (`BTreeMap` order), so the same file
    /// always compiles to the same numbers. A province that starts above the capacity the
    /// engine derives for its land is a schema error (owner's review of PR #278): the
    /// mismatch is caught when the scenario is written, not as famine on the first tick.
    pub fn compile(&self, params: &Params) -> Result<Compiled, SchemaError> {
        if self.actors.is_empty() || self.provinces.is_empty() {
            return Err(SchemaError::Empty);
        }
        let actor_index: BTreeMap<&str, ActorId> =
            self.actors.keys().enumerate().map(|(i, k)| (k.as_str(), ActorId(i as u32))).collect();
        let province_index: BTreeMap<&str, ProvinceId> =
            self.provinces.keys().enumerate().map(|(i, k)| (k.as_str(), ProvinceId(i as u32))).collect();

        let resolve = |key: &str, list: &[String]| -> Result<Vec<ProvinceId>, SchemaError> {
            let own = province_index[key];
            let mut out = Vec::with_capacity(list.len());
            for n in list {
                let id = *province_index.get(n.as_str()).ok_or_else(|| SchemaError::UnknownNeighbour(key.to_string(), n.clone()))?;
                out.push(id);
            }
            out.sort();
            if out.windows(2).any(|w| w[0] == w[1]) || out.contains(&own) {
                return Err(SchemaError::BadNeighbours(key.to_string()));
            }
            Ok(out)
        };

        // First every reference must exist (so the error names the dangling one), then the
        // graphs must be symmetric.
        let mut resolved: BTreeMap<&str, (Vec<ProvinceId>, Vec<ProvinceId>)> = BTreeMap::new();
        for (key, p) in &self.provinces {
            resolved.insert(key, (resolve(key, &p.neighbours)?, resolve(key, &p.sea_neighbours)?));
        }
        for (key, p) in &self.provinces {
            for n in p.neighbours.iter() {
                if !self.provinces[n].neighbours.contains(key) {
                    return Err(SchemaError::Asymmetric(key.clone(), n.clone()));
                }
            }
            for n in p.sea_neighbours.iter() {
                if !self.provinces[n].sea_neighbours.contains(key) {
                    return Err(SchemaError::Asymmetric(key.clone(), n.clone()));
                }
            }
        }
        let mut provinces = Vec::with_capacity(self.provinces.len());
        for (key, p) in &self.provinces {
            let owner = *actor_index.get(p.owner.as_str()).ok_or_else(|| SchemaError::UnknownOwner(key.clone(), p.owner.clone()))?;
            let (neighbours, sea_neighbours) = resolved.remove(key.as_str()).expect("resolved above");
            if !sea_neighbours.is_empty() && !p.coastal {
                return Err(SchemaError::SeaWithoutCoast(key.clone()));
            }
            let capacity = params.capacity(p.area_km2, p.land);
            if p.population > capacity {
                return Err(SchemaError::OverCapacity { province: key.clone(), population: p.population, capacity });
            }
            provinces.push(ProvinceFact { area_km2: p.area_km2, land: p.land, coastal: p.coastal, population: p.population, owner, neighbours, sea_neighbours });
        }

        let mut actors = Vec::with_capacity(self.actors.len());
        for (key, a) in &self.actors {
            let capital = province_index.get(a.capital.as_str()).copied();
            let owned = capital.is_some_and(|_| self.provinces[&a.capital].owner == *key);
            let Some(capital) = capital.filter(|_| owned) else {
                return Err(SchemaError::BadCapital(key.clone(), a.capital.clone()));
            };
            actors.push(ActorFact { treasury_kg: a.treasury_kg, capital, soldiers: a.soldiers });
        }

        let mut verbs = Vec::new();
        for (key, name) in &self.verbs {
            let id = VerbId::from_key(key).ok_or_else(|| SchemaError::UnknownVerb(key.clone()))?;
            verbs.push((id, name.clone()));
        }
        verbs.sort_by_key(|(id, _)| *id);

        Ok(Compiled {
            facts: Facts { scenario_id: self.id.clone(), start_year: self.start_year, provinces, actors },
            names: Names {
                actors: self.actors.iter().map(|(k, a)| (k.clone(), a.name.clone())).collect(),
                provinces: self.provinces.iter().map(|(k, p)| (k.clone(), p.name.clone())).collect(),
            },
            display: Display { verbs, currency: self.display_currency.clone() },
        })
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
capital = "p2"
soldiers = 20
[actors.a]
name = "A"
treasury_kg = 20
capital = "p1"
soldiers = 0
[provinces.p2]
name = "P2"
owner = "b"
area_km2 = 100
land = "Arable"
coastal = true
population = 50
neighbours = ["p1"]
sea_neighbours = ["p3"]
[provinces.p1]
name = "P1"
owner = "a"
area_km2 = 100
land = "Hills"
coastal = false
population = 50
neighbours = ["p2"]
sea_neighbours = []
[provinces.p3]
name = "P3"
owner = "a"
area_km2 = 100
land = "Arable"
coastal = true
population = 50
neighbours = []
sea_neighbours = ["p2"]
[verbs]
raise_tax = "Поднять налог"
"#;

    fn params() -> Params {
        Params::load().unwrap()
    }

    #[test]
    fn compiles_with_ids_in_sorted_order() {
        let s = ScenarioData::parse(OK).unwrap();
        let c = s.compile(&params()).unwrap();
        assert_eq!(c.names.actors[0].0, "a");
        assert_eq!(c.names.actors[1].0, "b");
        assert_eq!(c.facts.actors[0].treasury_kg, 20);
        assert_eq!(c.facts.actors[1].soldiers, 20);
        assert_eq!(c.facts.actors[1].capital, ProvinceId(1)); // p2
        assert_eq!(c.facts.provinces[0].owner, ActorId(0)); // p1 owned by a
        assert_eq!(c.facts.provinces[1].owner, ActorId(1)); // p2 owned by b
        assert_eq!(c.facts.provinces[0].land, Land::Hills);
        assert_eq!(c.facts.provinces[1].sea_neighbours, vec![ProvinceId(2)]);
        assert!(c.facts.provinces[1].coastal && !c.facts.provinces[0].coastal);
        assert_eq!(c.display.verbs, vec![(VerbId::RaiseTax, "Поднять налог".to_string())]);
        assert_eq!(c.facts.scenario_id, "t");
    }

    #[test]
    fn a_rule_number_has_nowhere_to_go() {
        let bad = OK.replacen("treasury_kg = 10", "treasury_kg = 10\ngrowth_rate = 0.02", 1);
        assert!(matches!(ScenarioData::parse(&bad), Err(SchemaError::Toml(_))));
        let bad = OK.replacen("start_year = 1477", "start_year = 1477\nconflict_probability = 0.3", 1);
        assert!(matches!(ScenarioData::parse(&bad), Err(SchemaError::Toml(_))));
        let bad = OK.replacen("coastal = true\npopulation = 50\nneighbours = [\"p1\"]", "coastal = true\ncapacity = 9000\npopulation = 50\nneighbours = [\"p1\"]", 1);
        assert!(matches!(ScenarioData::parse(&bad), Err(SchemaError::Toml(_))));
        let bad = OK.replacen("land = \"Hills\"", "land = \"Swamp\"", 1);
        assert!(matches!(ScenarioData::parse(&bad), Err(SchemaError::Toml(_))));
    }

    #[test]
    fn dangling_references_are_errors() {
        let p = params();
        let bad = OK.replacen("owner = \"b\"", "owner = \"zzz\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::UnknownOwner(..))));
        let bad = OK.replacen("neighbours = [\"p1\"]", "neighbours = [\"nowhere\"]", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::UnknownNeighbour(..))));
        let bad = OK.replacen("raise_tax", "invent_gunpowder", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::UnknownVerb(_))));
        let bad = OK.replacen("capital = \"p2\"", "capital = \"p1\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::BadCapital(..))));
    }

    #[test]
    fn graphs_must_be_symmetric_and_sea_links_need_a_coast() {
        let p = params();
        // p2 says p1 is a land neighbour; make p2 forget p1 while p1 still names p2.
        let bad = OK.replacen("neighbours = [\"p1\"]\nsea_neighbours = [\"p3\"]", "neighbours = []\nsea_neighbours = [\"p3\"]", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::Asymmetric(..))));
        // p1 is inland; give it a sea link (and the reverse, so symmetry is not the error).
        let bad = OK
            .replacen("coastal = false\npopulation = 50\nneighbours = [\"p2\"]\nsea_neighbours = []", "coastal = false\npopulation = 50\nneighbours = [\"p2\"]\nsea_neighbours = [\"p3\"]", 1)
            .replacen("neighbours = []\nsea_neighbours = [\"p2\"]", "neighbours = []\nsea_neighbours = [\"p1\", \"p2\"]", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::SeaWithoutCoast(_))));
    }

    #[test]
    fn a_province_above_its_capacity_is_a_schema_error_with_its_name() {
        // p1: 100 km² of hills carries 1 200 people at the shipped density.
        let bad = OK.replacen("land = \"Hills\"\ncoastal = false\npopulation = 50", "land = \"Hills\"\ncoastal = false\npopulation = 5000", 1);
        let err = ScenarioData::parse(&bad).unwrap().compile(&params()).unwrap_err();
        assert_eq!(err, SchemaError::OverCapacity { province: "p1".into(), population: 5000, capacity: 1200 });
    }
}
