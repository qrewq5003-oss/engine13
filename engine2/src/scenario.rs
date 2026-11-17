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

use crate::ids::{ActorId, CultureId, HouseId, PersonId, ProvinceId, RegionId, ReligionId};
use crate::params::Params;
use crate::verbs::VerbId;
use crate::world::{ActorFact, Disease, Facts, HouseFact, Institution, Land, Names, PersonFact, ProvinceFact, RegionFact, RelationFacts, Succession};

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
    /// Regions (E3): named sets of provinces, the target of the aspiration «unite» — a
    /// kingdom or an empire as the scenario names it, never a people (owner's rule on
    /// §23); absent means none. Keyed by the scenario's string id of the region.
    pub regions: Option<BTreeMap<String, RegionData>>,
    /// Enabled verbs and their display names, keyed by `VerbId::key`.
    pub verbs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionData {
    pub name: String,
    pub provinces: Vec<String>,
}

/// A starting aspiration (brief §4, E0 §6): a kind of the engine's closed list and a target
/// in the facts of the world — a province, a polity, a region, a faith, a route of two
/// provinces. Which of `target` / `route` is read depends on the kind.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AspirationData {
    pub kind: crate::agents::GoalKind,
    /// The target's scenario id: a province, an actor, a region or a religion string.
    pub target: Option<String>,
    /// For a route: the two provinces it runs between.
    pub route: Option<[String; 2]>,
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
    /// Starting aspirations (brief §4, E0 §6), in the facts of the world.
    pub aspirations: Vec<AspirationData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceData {
    pub name: String,
    pub owner: String,
    pub area_km2: u64,
    pub land: Land,
    /// The share of the area that carries the hills' density, 0 to 1, the rest carrying the
    /// arable's (E4, plan §31.43.2 p. 1; `docs/engine2_map.md` §12b, §17): only with
    /// `land = "Arable"` — a province all in hills is `land = "Hills"`, and the steppe, the
    /// desert, the irrigated valley and the dense land take no share. Absent, the class is
    /// the whole of it.
    pub hills: Option<f64>,
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
    #[error("province '{province}' gives a share of hills {hills}, outside 0 to 1")]
    HillsOutOfRange { province: String, hills: String },
    #[error("province '{province}' gives a share of hills on {land:?} land: only `land = \"Arable\"` takes one, through `hills`")]
    HillsOnOtherLand { province: String, land: Land },
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
    #[error("region '{0}' names unknown province '{1}'")]
    UnknownRegionProvince(String, String),
    #[error("actor '{0}': aspiration {1:?} names unknown target '{2}'")]
    UnknownAspirationTarget(String, crate::agents::GoalKind, String),
    #[error("actor '{0}': aspiration {1:?} needs {2}")]
    AspirationTarget(String, crate::agents::GoalKind, &'static str),
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

    /// Parse a scenario kept in two files (E4, plan §31.43.2 p. 3, §31.49): the scenario's own
    /// text — its top keys, actors, houses, persons, relations — and the table of provinces and
    /// regions the map's tools write. The two are joined as text, the scenario first, and parsed
    /// as the one schema by [`ScenarioData::parse`]: there is no second way to read a scenario.
    /// The engine opens no file and knows no scenario's name — the caller hands both texts.
    /// What does not join into one TOML is refused as any bad scenario is: a top key in the
    /// second text (it would fall into that text's last table), or a table given in both.
    pub fn parse_parts(scenario: &str, provinces: &str) -> Result<ScenarioData, SchemaError> {
        ScenarioData::parse(&format!("{scenario}\n{provinces}"))
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
            // The share of hills is given beside the class, never as the class (one form in the
            // files); a province with none is the plain class — the facts of a scenario with no
            // hills are what they were, and so is their hash (`Facts::hash`), which a save
            // carries and a load checks.
            let land = match (p.land, p.hills) {
                (Land::Mixed(_), _) => return Err(SchemaError::HillsOnOtherLand { province: key.clone(), land: p.land }),
                (class, None) => class,
                (_, Some(h)) if !(0.0..=1.0).contains(&h) => return Err(SchemaError::HillsOutOfRange { province: key.clone(), hills: h.to_string() }),
                (class, Some(_)) if class != Land::Arable => return Err(SchemaError::HillsOnOtherLand { province: key.clone(), land: class }),
                (class, Some(h)) => match (h * 1e6).round() as u32 {
                    0 => class,
                    hills_ppm => Land::Mixed(hills_ppm),
                },
            };
            let capacity = params.capacity(p.area_km2, land);
            if p.population > capacity {
                return Err(SchemaError::OverCapacity { province: key.clone(), population: p.population, capacity });
            }
            let governor = p.governor.as_deref().map(|g| seat(key, g, owner)).transpose()?;
            provinces.push(ProvinceFact { area_km2: p.area_km2, land, coastal: p.coastal, population: p.population, owner, neighbours, sea_neighbours, governor, culture: culture(&p.culture), religion: religion(&p.religion) });
        }

        // Regions (E3): every province known; ids in sorted order of the string ids.
        let region_index: BTreeMap<&str, RegionId> = self.regions.iter().flatten().enumerate().map(|(i, (k, _))| (k.as_str(), RegionId(i as u32))).collect();
        let mut regions = Vec::new();
        for (key, r) in self.regions.iter().flatten() {
            let mut provinces = Vec::with_capacity(r.provinces.len());
            for p in &r.provinces {
                provinces.push(*province_index.get(p.as_str()).ok_or_else(|| SchemaError::UnknownRegionProvince(key.clone(), p.clone()))?);
            }
            provinces.sort();
            provinces.dedup();
            regions.push(RegionFact { provinces });
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
            // Aspirations: the target resolved by the kind, every reference known.
            let mut goals = Vec::with_capacity(a.aspirations.len());
            for asp in &a.aspirations {
                use crate::agents::{GoalKind, GoalTarget};
                let unknown = |t: &String| SchemaError::UnknownAspirationTarget(key.clone(), asp.kind, t.clone());
                let target = match asp.kind {
                    GoalKind::Free => GoalTarget::None,
                    GoalKind::Union => {
                        let t = asp.target.as_ref().ok_or(SchemaError::AspirationTarget(key.clone(), asp.kind, "a religion"))?;
                        GoalTarget::Religion(religions.iter().position(|r| r == t).map(|i| ReligionId(i as u32)).ok_or_else(|| unknown(t))?)
                    }
                    GoalKind::Subdue | GoalKind::AllianceAgainst => {
                        let t = asp.target.as_ref().ok_or(SchemaError::AspirationTarget(key.clone(), asp.kind, "a polity"))?;
                        GoalTarget::Polity(*actor_index.get(t.as_str()).ok_or_else(|| unknown(t))?)
                    }
                    GoalKind::Unite => {
                        let t = asp.target.as_ref().ok_or(SchemaError::AspirationTarget(key.clone(), asp.kind, "a region"))?;
                        GoalTarget::Region(*region_index.get(t.as_str()).ok_or_else(|| unknown(t))?)
                    }
                    GoalKind::Hold | GoalKind::Take => {
                        if let Some([x, y]) = &asp.route {
                            let x = *province_index.get(x.as_str()).ok_or_else(|| unknown(x))?;
                            let y = *province_index.get(y.as_str()).ok_or_else(|| unknown(y))?;
                            GoalTarget::Route(x, y)
                        } else {
                            let t = asp.target.as_ref().ok_or(SchemaError::AspirationTarget(key.clone(), asp.kind, "a province, a polity, a region or a route"))?;
                            if let Some(p) = province_index.get(t.as_str()) {
                                GoalTarget::Province(*p)
                            } else if let Some(b) = actor_index.get(t.as_str()) {
                                GoalTarget::Polity(*b)
                            } else if let Some(r) = region_index.get(t.as_str()) {
                                GoalTarget::Region(*r)
                            } else {
                                return Err(unknown(t));
                            }
                        }
                    }
                };
                goals.push(crate::agents::GoalFact { kind: asp.kind, target });
            }
            actors.push(ActorFact { treasury_kg: a.treasury_kg, capital, soldiers: a.soldiers, institutions, ruler, heir, regent, succession: a.succession, goals });
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
            facts: Facts { scenario_id: self.id.clone(), start_year: self.start_year, diseases: self.diseases.clone(), provinces, actors, houses, persons, relations, regions },
            names: Names {
                actors: self.actors.iter().map(|(k, a)| (k.clone(), a.name.clone())).collect(),
                provinces: self.provinces.iter().map(|(k, p)| (k.clone(), p.name.clone())).collect(),
                houses: self.houses.iter().map(|(k, h)| (k.clone(), h.name.clone())).collect(),
                persons: self.persons.iter().map(|(k, p)| (k.clone(), p.name.clone())).collect(),
                cultures,
                religions,
                regions: self.regions.iter().flatten().map(|(k, r)| (k.clone(), r.name.clone())).collect(),
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
aspirations = [{ kind = "Take", target = "p1" }, { kind = "Hold", route = ["p2", "p3"] }]
[actors.a]
name = "A"
treasury_kg = 20
capital = "p1"
soldiers = 0
institutions = []
ruler = "ann"
heir = "ann_jr"
succession = "Blood"
aspirations = [{ kind = "Unite", target = "west" }, { kind = "Subdue", target = "b" }]
[regions.west]
name = "The West"
provinces = ["p1", "p3"]
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
        // Regions and aspirations (E3): resolved to ids by kind.
        use crate::agents::{GoalFact, GoalKind, GoalTarget};
        assert_eq!(c.names.regions, vec![("west".to_string(), "The West".to_string())]);
        assert_eq!(c.facts.regions[0].provinces, vec![ProvinceId(0), ProvinceId(2)]);
        assert_eq!(c.facts.actors[0].goals, vec![GoalFact { kind: GoalKind::Unite, target: GoalTarget::Region(RegionId(0)) }, GoalFact { kind: GoalKind::Subdue, target: GoalTarget::Polity(ActorId(1)) }]);
        assert_eq!(c.facts.actors[1].goals, vec![GoalFact { kind: GoalKind::Take, target: GoalTarget::Province(ProvinceId(0)) }, GoalFact { kind: GoalKind::Hold, target: GoalTarget::Route(ProvinceId(1), ProvinceId(2)) }]);
        let bad = OK.replacen("{ kind = \"Subdue\", target = \"b\" }", "{ kind = \"Subdue\", target = \"nobody\" }", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&params()), Err(SchemaError::UnknownAspirationTarget(..))));
        let bad = OK.replacen("provinces = [\"p1\", \"p3\"]", "provinces = [\"p1\", \"p9\"]", 1);
        assert!(matches!(ScenarioData::parse(&bad).unwrap().compile(&params()), Err(SchemaError::UnknownRegionProvince(..))));
        assert!(ScenarioData::parse(&OK.replacen("kind = \"Unite\"", "kind = \"Conquer\"", 1)).is_err());
        let no_plague = ScenarioData::parse(&OK.replacen("diseases = [\"Plague\"]", "diseases = []", 1)).unwrap();
        assert!(no_plague.compile(&params()).unwrap().facts.diseases.is_empty());
        assert!(ScenarioData::parse(&OK.replacen("diseases = [\"Plague\"]", "diseases = [\"Dragonpox\"]", 1)).is_err());
    }

    /// E4 (plan §31.43.2 p. 1, §31.44): a province gives the share of its area in hills —
    /// its capacity lies between the arable's and the hills' by that share — and only arable
    /// land takes one.
    #[test]
    fn a_share_of_hills_sets_the_capacity_between_the_arable_and_the_hills() {
        use crate::world::World;
        let with = |field: &str, people: u32| OK.replacen("land = \"Arable\"\ncoastal = true\npopulation = 50\nneighbours = [\"p1\"]", &format!("land = \"Arable\"\n{field}coastal = true\npopulation = {people}\nneighbours = [\"p1\"]"), 1);
        assert_ne!(with("hills = 0.25\n", 50), OK, "the fixture's province p2 is found");
        // 100 km²: three quarters at the arable's 50, a quarter at the hills' 12 — 4050 people.
        let c = ScenarioData::parse(&with("hills = 0.25\n", 4050)).unwrap().compile(&params()).unwrap();
        let p2 = c.names.provinces.iter().position(|(sid, _)| sid == "p2").unwrap();
        assert_eq!(c.facts.provinces[p2].land, Land::Mixed(250_000));
        let w = World::from_facts(&c.facts, c.names, &params(), 1).unwrap();
        let in_world = &w.provinces[&crate::ids::ProvinceId(p2 as u32)];
        assert_eq!((in_world.capacity, in_world.land), (4050, Land::Mixed(250_000)), "the world's capacity is derived with the share, and the world keeps the land it was derived from");
        assert_eq!(in_world.capacity, params().capacity(in_world.area_km2, in_world.land), "the capacity can be read again off the world's province");
        let over = ScenarioData::parse(&with("hills = 0.25\n", 4051)).unwrap().compile(&params()).unwrap_err();
        assert_eq!(over, SchemaError::OverCapacity { province: "p2".into(), population: 4051, capacity: 4050 });
        // Absent, the class is the whole of it; all in hills, the arable carries as the hills.
        // No hills — absent or nothing — is the plain class: the same facts, the same hash.
        let plain = ScenarioData::parse(&with("", 5000)).unwrap().compile(&params()).unwrap();
        let none = ScenarioData::parse(&with("hills = 0.0\n", 5000)).unwrap().compile(&params()).unwrap();
        assert_eq!(plain.facts.provinces[p2].land, Land::Arable);
        assert_eq!(none.facts, plain.facts);
        let untouched = ScenarioData::parse(OK).unwrap().compile(&params()).unwrap();
        assert_eq!(ScenarioData::parse(&with("hills = 0.0\n", 50)).unwrap().compile(&params()).unwrap().facts.hash(), untouched.facts.hash(), "no hills is no new fact: the scenario hashes as it did");
        assert_ne!(c.facts.hash(), ScenarioData::parse(&with("", 4050)).unwrap().compile(&params()).unwrap().facts.hash(), "a share of hills is another fact");
        assert!(ScenarioData::parse(&with("hills = 1.0\n", 1200)).unwrap().compile(&params()).is_ok());
        assert!(matches!(ScenarioData::parse(&with("hills = 1.0\n", 1201)).unwrap().compile(&params()), Err(SchemaError::OverCapacity { capacity: 1200, .. })));
        assert!(ScenarioData::parse(&with("hills = 0.0\n", 5000)).unwrap().compile(&params()).is_ok());
        // Outside 0 to 1, or on land that is not arable: a schema error, not a guess.
        for bad in ["hills = 1.2\n", "hills = -0.1\n"] {
            assert!(matches!(ScenarioData::parse(&with(bad, 50)).unwrap().compile(&params()), Err(SchemaError::HillsOutOfRange { .. })), "{bad}");
        }
        for land in ["Hills", "Steppe", "Desert", "Irrigated", "Dense"] {
            let other = OK.replacen("land = \"Hills\"\ncoastal = false", &format!("land = \"{land}\"\nhills = 0.5\ncoastal = false"), 1);
            assert_ne!(other, OK);
            assert!(matches!(ScenarioData::parse(&other).unwrap().compile(&params()), Err(SchemaError::HillsOnOtherLand { .. })), "{land}");
        }
        // The mix is given through `hills`, never as the class itself: one form in the files.
        let as_class = OK.replacen("land = \"Hills\"\ncoastal = false", "land = { Mixed = 250000 }\ncoastal = false", 1);
        assert_ne!(as_class, OK);
        assert!(matches!(ScenarioData::parse(&as_class).unwrap().compile(&params()), Err(SchemaError::HillsOnOtherLand { .. })));
    }

    /// The fixture as the two files of a real scenario: the provinces and the regions as the
    /// map's tools write them, everything else as the scenario's own text.
    fn in_two_files() -> (String, String) {
        let (mut scenario, mut provinces) = (String::new(), String::new());
        let mut of_the_map = false;
        for line in OK.lines() {
            if line.starts_with('[') {
                of_the_map = line.starts_with("[provinces.") || line.starts_with("[regions.");
            }
            let part = if of_the_map { &mut provinces } else { &mut scenario };
            part.push_str(line);
            part.push('\n');
        }
        (scenario, provinces)
    }

    #[test]
    fn a_scenario_in_two_files_is_the_scenario_in_one() {
        let (scenario, provinces) = in_two_files();
        // The regions lie with the provinces and not with the scenario, as in the map's data.
        assert!(provinces.contains("[regions.west]") && provinces.contains("[provinces.p1]"));
        assert!(!scenario.contains("[regions.") && !scenario.contains("[provinces.") && scenario.contains("[actors.a]"));
        let whole = ScenarioData::parse(OK).unwrap();
        let parts = ScenarioData::parse_parts(&scenario, &provinces).unwrap();
        assert_eq!(parts, whole, "two files give the scenario one file gives");
        let (one, two) = (whole.compile(&params()).unwrap(), parts.compile(&params()).unwrap());
        assert_eq!(two.facts, one.facts);
        assert_eq!(two.facts.hash(), one.facts.hash(), "the same facts, the same hash: a save of the one loads by the other");
        assert_eq!(two.facts.regions.len(), 1, "the regions of the second file are in the facts");
        // A first text that ends without a line break joins all the same.
        assert_eq!(ScenarioData::parse_parts(scenario.trim_end(), &provinces).unwrap(), whole);
        // The scenario comes first: its top keys must open the text, and after the provinces
        // they would fall into the last province's table.
        assert!(matches!(ScenarioData::parse_parts(&provinces, &scenario), Err(SchemaError::Toml(_))));
    }

    #[test]
    fn two_files_that_do_not_join_into_one_scenario_are_refused() {
        let (scenario, provinces) = in_two_files();
        // A top key in the second text falls into the last table of the first — refused there:
        // by the schema of that table at the parse, or, in the free table of the verbs, as a
        // verb the engine does not know.
        let number = format!("start_year = 1500\n{provinces}");
        assert!(matches!(ScenarioData::parse_parts(&scenario, &number), Err(SchemaError::Toml(_))));
        let word = format!("id = \"other\"\n{provinces}");
        let as_verb = ScenarioData::parse_parts(&scenario, &word).unwrap().compile(&params());
        assert!(matches!(as_verb, Err(SchemaError::UnknownVerb(ref key)) if key == "id"), "{as_verb:?}");
        // A table given in both texts.
        let twice = format!("{scenario}[regions.west]\nname = \"The West\"\nprovinces = [\"p1\", \"p3\"]\n");
        assert!(matches!(ScenarioData::parse_parts(&twice, &provinces), Err(SchemaError::Toml(_))));
        // The provinces left out of both: the schema has nothing to stand the actors on.
        assert!(ScenarioData::parse_parts(&scenario, "").is_err());
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
        // The irrigated valley is of the closed list (plan §30.7): it parses, and the engine
        // derives its capacity — 100 km² carry what no arable land of that area could.
        let valley = OK.replacen("land = \"Hills\"\ncoastal = false\npopulation = 50", "land = \"Irrigated\"\ncoastal = false\npopulation = 18400", 1);
        let c = ScenarioData::parse(&valley).unwrap().compile(&params()).unwrap();
        assert_eq!(c.facts.provinces[0].land, Land::Irrigated);
        let over = OK.replacen("land = \"Hills\"\ncoastal = false\npopulation = 50", "land = \"Irrigated\"\ncoastal = false\npopulation = 18401", 1);
        assert!(matches!(ScenarioData::parse(&over).unwrap().compile(&params()), Err(SchemaError::OverCapacity { .. })));
        // The dense land is of the closed list too (E4, plan §31.47): the same 100 km² carry
        // 7500 — more than the arable's 5000 and far less than the valley's.
        let dense = OK.replacen("land = \"Hills\"\ncoastal = false\npopulation = 50", "land = \"Dense\"\ncoastal = false\npopulation = 7500", 1);
        assert_ne!(dense, OK);
        let c = ScenarioData::parse(&dense).unwrap().compile(&params()).unwrap();
        let at = c.facts.provinces.iter().position(|f| f.land == Land::Dense).expect("the dense province is in the facts");
        let w = crate::world::World::from_facts(&c.facts, c.names, &params(), 1).unwrap();
        let in_world = &w.provinces[&crate::ids::ProvinceId(at as u32)];
        assert_eq!((in_world.capacity, in_world.land), (7500, Land::Dense), "the world derives the capacity of the dense land and keeps the land");
        let over = OK.replacen("land = \"Hills\"\ncoastal = false\npopulation = 50", "land = \"Dense\"\ncoastal = false\npopulation = 7501", 1);
        assert!(matches!(ScenarioData::parse(&over).unwrap().compile(&params()), Err(SchemaError::OverCapacity { capacity: 7500, .. })));
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
