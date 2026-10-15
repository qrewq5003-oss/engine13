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
//! one unit at the capital. It gives its ruler, its rule of succession, and — facts, when
//! they are facts — a designated heir and a regent (plan §18); a province may give its
//! governor. Houses say which polity they belong to; persons their house, their year of
//! birth and their parent. Every person listed is alive at the start.
//!
//! The engine receives the compiled [`Facts`] and the starting [`Names`], which then live in
//! the world (an heir born in the game is named there). What is presentation only — the
//! verbs' display names and the display currency — stays in [`Display`], and only `view.rs`
//! reads the currency (guard `display_currency_is_read_only_by_the_view`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, CultureId, HouseId, PersonId, ProvinceId, ReligionId};
use crate::params::Params;
use crate::verbs::VerbId;
use crate::world::{ActorFact, Disease, Facts, HouseFact, Institution, Land, Names, PersonFact, ProvinceFact, RelationFacts, Succession};

/// The scenario file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioData {
    pub id: String,
    pub start_year: i32,
    /// The diseases this world holds (brief §0.2: a fact, like culture); plague strikes by
    /// E0 П4 only where it is listed.
    pub diseases: Vec<Disease>,
    /// The money the presentation layer shows; the engine does not read it.
    pub display_currency: DisplayCurrency,
    /// Keyed by the scenario's string id of the actor.
    pub actors: BTreeMap<String, ActorData>,
    /// Keyed by the scenario's string id of the province.
    pub provinces: BTreeMap<String, ProvinceData>,
    /// Keyed by the scenario's string id of the house.
    pub houses: BTreeMap<String, HouseData>,
    /// Keyed by the scenario's string id of the person.
    pub persons: BTreeMap<String, PersonData>,
    /// Starting relations between polities (E2 step 7): wars, alliances, vassals — facts,
    /// a column of their own (E0 §6); absent means none.
    pub relations: Option<RelationsData>,
    /// Enabled verbs and their display names, keyed by `VerbId::key`.
    pub verbs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationsData {
    /// Pairs of actor ids at war.
    pub wars: Vec<[String; 2]>,
    pub alliances: Vec<AllianceData>,
    pub vassals: Vec<VassalData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllianceData {
    pub a: String,
    pub b: String,
    /// The year the alliance runs to (a fact: Lodi was made for 25 years).
    pub until_year: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VassalData {
    pub vassal: String,
    pub lord: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HouseData {
    pub name: String,
    /// The polity the house belongs to.
    pub polity: String,
    /// Starting treasury of the house in kilograms of wheat-equivalent.
    pub treasury_kg: i64,
    /// The tongue and the faith of the house (E2 step 8): facts, as strings the scenario
    /// chooses; the engine interns them and never reads which is which.
    pub culture: String,
    pub religion: String,
    /// The house's own men at the start (E0 Х2: the famiglia ducale, the despot's retinue):
    /// a unit of the polity at its capital, marked with the house.
    pub soldiers: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonData {
    pub name: String,
    pub house: String,
    /// Year of birth.
    pub born: i32,
    /// The parent, for the line of blood; absent for a founder.
    pub parent: Option<String>,
    /// The spouse at the start: named on both, of another house.
    pub spouse: Option<String>,
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
    /// The polity's institutions, from the engine's closed list (facts: a bank exists or not).
    pub institutions: Vec<Institution>,
    /// The person on the throne.
    pub ruler: String,
    /// A designated heir, when there is one.
    pub heir: Option<String>,
    /// The regent, when the ruler is a minor and one is in place.
    pub regent: Option<String>,
    /// The rule of succession (E0 §4).
    pub succession: Succession,
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
    /// The governor's seat, when a person holds it.
    pub governor: Option<String>,
    /// The tongue and the faith of the province's majority (E2 step 8).
    pub culture: String,
    pub religion: String,
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
    #[error("house '{0}' names unknown polity '{1}'")]
    UnknownHousePolity(String, String),
    #[error("person '{0}' names unknown house '{1}'")]
    UnknownHouse(String, String),
    #[error("person '{0}' names unknown parent '{1}'")]
    UnknownParent(String, String),
    #[error("person '{0}' is not younger than its parent '{1}'")]
    ParentNotOlder(String, String),
    #[error("person '{0}' is born after the start year")]
    BornInTheFuture(String),
    #[error("'{0}' names unknown person '{1}' for a seat")]
    UnknownPerson(String, String),
    #[error("'{0}' gives a seat to '{1}', who belongs to a house of another polity")]
    SeatOutsidePolity(String, String),
    #[error("person '{0}' names unknown spouse '{1}'")]
    UnknownSpouse(String, String),
    #[error("person '{0}' names spouse '{1}', who does not name '{0}' back")]
    SpouseNotMutual(String, String),
    #[error("person '{0}' and spouse '{1}' are of one house: a marriage binds two houses")]
    SpouseInSameHouse(String, String),
    #[error("relations name unknown actor '{0}'")]
    UnknownRelationActor(String),
    #[error("actor '{0}' is bound to itself")]
    RelationWithSelf(String),
    #[error("actors '{0}' and '{1}' are bound twice")]
    DuplicateRelation(String, String),
    #[error("alliance of '{0}' and '{1}' runs to {2}, not after the start year")]
    TreatyOverAtStart(String, String, i32),
    #[error("actor '{0}' has two lords")]
    TwoLords(String),
    #[error("actor '{0}' is a vassal and a lord at once")]
    VassalWithVassals(String),
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
        let house_index: BTreeMap<&str, HouseId> = self.houses.keys().enumerate().map(|(i, k)| (k.as_str(), HouseId(i as u32))).collect();
        let person_index: BTreeMap<&str, PersonId> = self.persons.keys().enumerate().map(|(i, k)| (k.as_str(), PersonId(i as u32))).collect();
        // Cultures and religions: the scenario's strings, interned in sorted order.
        let cultures: Vec<String> = {
            let mut v: Vec<String> = self.provinces.values().map(|p| p.culture.clone()).chain(self.houses.values().map(|h| h.culture.clone())).collect();
            v.sort();
            v.dedup();
            v
        };
        let religions: Vec<String> = {
            let mut v: Vec<String> = self.provinces.values().map(|p| p.religion.clone()).chain(self.houses.values().map(|h| h.religion.clone())).collect();
            v.sort();
            v.dedup();
            v
        };
        let culture = |s: &str| CultureId(cultures.iter().position(|c| c == s).expect("interned") as u32);
        let religion = |s: &str| ReligionId(religions.iter().position(|r| r == s).expect("interned") as u32);

        // Houses and persons: every reference must exist, a parent must be older, nobody
        // is born after the start.
        let mut houses = Vec::with_capacity(self.houses.len());
        for (key, h) in &self.houses {
            let polity = *actor_index.get(h.polity.as_str()).ok_or_else(|| SchemaError::UnknownHousePolity(key.clone(), h.polity.clone()))?;
            houses.push(HouseFact { polity, treasury_kg: h.treasury_kg, culture: culture(&h.culture), religion: religion(&h.religion), soldiers: h.soldiers });
        }
        let mut persons = Vec::with_capacity(self.persons.len());
        for (key, p) in &self.persons {
            let house = *house_index.get(p.house.as_str()).ok_or_else(|| SchemaError::UnknownHouse(key.clone(), p.house.clone()))?;
            if p.born > self.start_year {
                return Err(SchemaError::BornInTheFuture(key.clone()));
            }
            let parent = match &p.parent {
                None => None,
                Some(name) => {
                    let id = *person_index.get(name.as_str()).ok_or_else(|| SchemaError::UnknownParent(key.clone(), name.clone()))?;
                    if self.persons[name].born >= p.born {
                        return Err(SchemaError::ParentNotOlder(key.clone(), name.clone()));
                    }
                    Some(id)
                }
            };
            persons.push(PersonFact { house, born: p.born, parent, spouse: None });
        }
        // Marriages: named on both sides, between two houses.
        for (i, (key, p)) in self.persons.iter().enumerate() {
            let Some(name) = &p.spouse else { continue };
            let s = *person_index.get(name.as_str()).ok_or_else(|| SchemaError::UnknownSpouse(key.clone(), name.clone()))?;
            if self.persons[name].spouse.as_deref() != Some(key.as_str()) {
                return Err(SchemaError::SpouseNotMutual(key.clone(), name.clone()));
            }
            if persons[s.index()].house == persons[i].house {
                return Err(SchemaError::SpouseInSameHouse(key.clone(), name.clone()));
            }
            persons[i].spouse = Some(s);
        }
        // A seat of a polity goes to a person of one of its houses.
        let seat = |holder: &str, name: &str, polity: ActorId| -> Result<PersonId, SchemaError> {
            let id = *person_index.get(name).ok_or_else(|| SchemaError::UnknownPerson(holder.to_string(), name.to_string()))?;
            if houses[persons[id.index()].house.index()].polity != polity {
                return Err(SchemaError::SeatOutsidePolity(holder.to_string(), name.to_string()));
            }
            Ok(id)
        };

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
            let governor = p.governor.as_deref().map(|g| seat(key, g, owner)).transpose()?;
            provinces.push(ProvinceFact { area_km2: p.area_km2, land: p.land, coastal: p.coastal, population: p.population, owner, neighbours, sea_neighbours, governor, culture: culture(&p.culture), religion: religion(&p.religion) });
        }

        let mut actors = Vec::with_capacity(self.actors.len());
        for (key, a) in &self.actors {
            let capital = province_index.get(a.capital.as_str()).copied();
            let owned = capital.is_some_and(|_| self.provinces[&a.capital].owner == *key);
            let Some(capital) = capital.filter(|_| owned) else {
                return Err(SchemaError::BadCapital(key.clone(), a.capital.clone()));
            };
            let mut institutions = a.institutions.clone();
            institutions.sort();
            institutions.dedup();
            let id = actor_index[key.as_str()];
            let ruler = seat(key, &a.ruler, id)?;
            let heir = a.heir.as_deref().map(|h| seat(key, h, id)).transpose()?;
            let regent = a.regent.as_deref().map(|r| seat(key, r, id)).transpose()?;
            actors.push(ActorFact { treasury_kg: a.treasury_kg, capital, soldiers: a.soldiers, institutions, ruler, heir, regent, succession: a.succession });
        }

        // Starting relations: every actor known, nobody bound to itself, each pair bound
        // once (so nobody both allied and at war), a treaty still running, one lord a
        // vassal, no vassal a lord.
        let mut relations = RelationFacts::default();
        if let Some(r) = &self.relations {
            let actor = |name: &str| -> Result<ActorId, SchemaError> { actor_index.get(name).copied().ok_or_else(|| SchemaError::UnknownRelationActor(name.to_string())) };
            let mut bound: Vec<(ActorId, ActorId)> = Vec::new();
            let mut pair = |x: &str, y: &str| -> Result<(ActorId, ActorId), SchemaError> {
                let (a, b) = (actor(x)?, actor(y)?);
                if a == b {
                    return Err(SchemaError::RelationWithSelf(x.to_string()));
                }
                let key = (a.min(b), a.max(b));
                if bound.contains(&key) {
                    return Err(SchemaError::DuplicateRelation(x.to_string(), y.to_string()));
                }
                bound.push(key);
                Ok(key)
            };
            for [x, y] in &r.wars {
                relations.wars.push(pair(x, y)?);
            }
            for al in &r.alliances {
                let key = pair(&al.a, &al.b)?;
                if al.until_year <= self.start_year {
                    return Err(SchemaError::TreatyOverAtStart(al.a.clone(), al.b.clone(), al.until_year));
                }
                relations.alliances.push((key.0, key.1, al.until_year));
            }
            for v in &r.vassals {
                pair(&v.vassal, &v.lord)?;
                let (vassal, lord) = (actor(&v.vassal)?, actor(&v.lord)?);
                if relations.vassals.iter().any(|(x, _)| *x == vassal) {
                    return Err(SchemaError::TwoLords(v.vassal.clone()));
                }
                if relations.vassals.iter().any(|(x, l)| *x == lord || *l == vassal) {
                    return Err(SchemaError::VassalWithVassals(if relations.vassals.iter().any(|(x, _)| *x == lord) { v.lord.clone() } else { v.vassal.clone() }));
                }
                relations.vassals.push((vassal, lord));
            }
        }

        let mut verbs = Vec::new();
        for (key, name) in &self.verbs {
            let id = VerbId::from_key(key).ok_or_else(|| SchemaError::UnknownVerb(key.clone()))?;
            verbs.push((id, name.clone()));
        }
        verbs.sort_by_key(|(id, _)| *id);

        Ok(Compiled {
            facts: Facts { scenario_id: self.id.clone(), start_year: self.start_year, diseases: self.diseases.clone(), provinces, actors, houses, persons, relations },
            names: Names {
                actors: self.actors.iter().map(|(k, a)| (k.clone(), a.name.clone())).collect(),
                provinces: self.provinces.iter().map(|(k, p)| (k.clone(), p.name.clone())).collect(),
                houses: self.houses.iter().map(|(k, h)| (k.clone(), h.name.clone())).collect(),
                persons: self.persons.iter().map(|(k, p)| (k.clone(), p.name.clone())).collect(),
                cultures,
                religions,
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
diseases = ["Plague"]
[display_currency]
name = "ducat"
kg_per_unit = 100.0
[actors.b]
name = "B"
treasury_kg = 10
capital = "p2"
soldiers = 20
institutions = ["Bank"]
ruler = "bob"
succession = "Election"
[actors.a]
name = "A"
treasury_kg = 20
capital = "p1"
soldiers = 0
institutions = []
ruler = "ann"
heir = "ann_jr"
succession = "Blood"
[houses.ha]
name = "House A"
polity = "a"
treasury_kg = 5
culture = "west"
religion = "latin"
soldiers = 0
[houses.hb]
name = "House B"
polity = "b"
treasury_kg = 0
culture = "east"
religion = "greek"
soldiers = 4
[persons.ann]
name = "Ann"
house = "ha"
born = 1440
[persons.ann_jr]
name = "Ann the younger"
house = "ha"
born = 1470
parent = "ann"
[persons.bob]
name = "Bob"
house = "hb"
born = 1430
[provinces.p2]
name = "P2"
owner = "b"
area_km2 = 100
land = "Arable"
coastal = true
population = 50
neighbours = ["p1"]
sea_neighbours = ["p3"]
culture = "east"
religion = "greek"
[provinces.p1]
name = "P1"
owner = "a"
area_km2 = 100
land = "Hills"
coastal = false
population = 50
neighbours = ["p2"]
sea_neighbours = []
culture = "west"
religion = "latin"
[provinces.p3]
name = "P3"
owner = "a"
area_km2 = 100
land = "Arable"
coastal = true
population = 50
neighbours = []
sea_neighbours = ["p2"]
culture = "west"
religion = "greek"
governor = "ann_jr"
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
        assert_eq!(c.facts.actors[1].institutions, vec![Institution::Bank]);
        assert!(c.facts.actors[0].institutions.is_empty());
        assert!(ScenarioData::parse(&OK.replacen("institutions = [\"Bank\"]", "institutions = [\"Guild\"]", 1)).is_err());
        assert_eq!(c.facts.actors[1].capital, ProvinceId(1)); // p2
        assert_eq!(c.facts.provinces[0].owner, ActorId(0)); // p1 owned by a
        assert_eq!(c.facts.provinces[1].owner, ActorId(1)); // p2 owned by b
        assert_eq!(c.facts.provinces[0].land, Land::Hills);
        assert_eq!(c.facts.provinces[1].sea_neighbours, vec![ProvinceId(2)]);
        assert!(c.facts.provinces[1].coastal && !c.facts.provinces[0].coastal);
        assert_eq!(c.display.verbs, vec![(VerbId::RaiseTax, "Поднять налог".to_string())]);
        assert_eq!(c.facts.scenario_id, "t");
        // Houses and persons in sorted order of their ids; seats resolved to them.
        assert_eq!(c.names.houses, vec![("ha".to_string(), "House A".to_string()), ("hb".to_string(), "House B".to_string())]);
        assert_eq!(c.names.persons[0].0, "ann");
        assert_eq!(c.facts.houses[0], HouseFact { polity: ActorId(0), treasury_kg: 5, culture: CultureId(1), religion: ReligionId(1), soldiers: 0 });
        assert_eq!(c.facts.houses[1].soldiers, 4);
        // Cultures and religions interned in sorted order: east < west, greek < latin.
        assert_eq!(c.names.cultures, vec!["east".to_string(), "west".to_string()]);
        assert_eq!(c.names.religions, vec!["greek".to_string(), "latin".to_string()]);
        assert_eq!((c.facts.provinces[2].culture, c.facts.provinces[2].religion), (CultureId(1), ReligionId(0))); // p3: west, greek
        assert_eq!(c.facts.persons[1], PersonFact { house: HouseId(0), born: 1470, parent: Some(PersonId(0)), spouse: None });
        assert_eq!(c.facts.actors[0].ruler, PersonId(0));
        assert_eq!(c.facts.actors[0].heir, Some(PersonId(1)));
        assert_eq!(c.facts.actors[0].succession, Succession::Blood);
        assert_eq!(c.facts.actors[1].succession, Succession::Election);
        assert_eq!(c.facts.actors[1].heir, None);
        assert_eq!(c.facts.provinces[2].governor, Some(PersonId(1)));
        assert!(ScenarioData::parse(&OK.replacen("succession = \"Blood\"", "succession = \"Lottery\"", 1)).is_err());
        assert_eq!(c.facts.diseases, vec![Disease::Plague]);
        let no_plague = ScenarioData::parse(&OK.replacen("diseases = [\"Plague\"]", "diseases = []", 1)).unwrap();
        assert!(no_plague.compile(&params()).unwrap().facts.diseases.is_empty());
        assert!(ScenarioData::parse(&OK.replacen("diseases = [\"Plague\"]", "diseases = [\"Dragonpox\"]", 1)).is_err());
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
    fn seats_houses_and_parents_are_checked() {
        let p = params();
        let bad = OK.replacen("ruler = \"bob\"", "ruler = \"nobody\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::UnknownPerson(..))));
        // Bob's house belongs to b: he cannot sit on a's throne.
        let bad = OK.replacen("ruler = \"ann\"", "ruler = \"bob\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::SeatOutsidePolity(..))));
        let bad = OK.replacen("governor = \"ann_jr\"", "governor = \"bob\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::SeatOutsidePolity(..))));
        let bad = OK.replacen("polity = \"b\"", "polity = \"zzz\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::UnknownHousePolity(..))));
        let bad = OK.replacen("house = \"hb\"", "house = \"hz\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::UnknownHouse(..))));
        let bad = OK.replacen("parent = \"ann\"", "parent = \"eve\"", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::UnknownParent(..))));
        let bad = OK.replacen("born = 1470", "born = 1440", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::ParentNotOlder(..))));
        let bad = OK.replacen("born = 1430", "born = 1480", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&p), Err(SchemaError::BornInTheFuture(_))));
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
