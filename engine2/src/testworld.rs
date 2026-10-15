//! The synthetic test world (plan §10, brief §0.3): five states on a 5×5 grid of provinces,
//! no aspirations, no institutions. Proof that the engine runs on facts alone.
//!
//! Lives in the library (not `tests/common`) because the determinism test runs it in a
//! child process of the test binary, and both halves must build the same world.

use std::collections::BTreeMap;

use crate::ids::{ActorId, CultureId, HouseId, PersonId, ProvinceId, ReligionId};
use crate::params::Params;
use crate::scenario::{ActorData, DisplayCurrency, HouseData, PersonData, ProvinceData, ScenarioData};
use crate::world::{ActorFact, Facts, HouseFact, Land, Names, PersonFact, ProvinceFact, RelationFacts, Succession, World};

/// Five states, each a column of the 5×5 grid; all land arable; province `i` has an area
/// of `250 + 25·i` km², which at the arable density of `params.toml` (40 people per km²,
/// E0 П1) carries `10_000 + 1_000·i` people; it starts at 60 % of that; treasuries
/// 100 000 kg each. The top row is the coast, and each coastal province faces the coasts
/// two columns away across a bay (a sea link no land link duplicates). Each state's capital
/// is its top province, with 600 soldiers (≈ 1 % of its people, E0 А11). Each state has a
/// house `h<a>` with a ruler `r<a>` and a child `c<a>`; state 0 also has a brother `b0`
/// governing its second province, a designated heir (`c0`) and a second house `o0` of one
/// adult — the shapes succession must handle. Rules: blood, acclamation, election, mixed,
/// blood. Returns the facts and their names.
pub fn five_states() -> (Facts, Names) {
    const SIDE: u32 = 5;
    let mut provinces = Vec::new();
    let mut province_names = Vec::new();
    for y in 0..SIDE {
        for x in 0..SIDE {
            let i = y * SIDE + x;
            let mut neighbours = Vec::new();
            if x > 0 {
                neighbours.push(ProvinceId(i - 1));
            }
            if x + 1 < SIDE {
                neighbours.push(ProvinceId(i + 1));
            }
            if y > 0 {
                neighbours.push(ProvinceId(i - SIDE));
            }
            if y + 1 < SIDE {
                neighbours.push(ProvinceId(i + SIDE));
            }
            neighbours.sort();
            let coastal = y == 0;
            let mut sea_neighbours = Vec::new();
            if coastal {
                if x >= 2 {
                    sea_neighbours.push(ProvinceId(i - 2));
                }
                if x + 2 < SIDE {
                    sea_neighbours.push(ProvinceId(i + 2));
                }
            }
            let area_km2 = 250 + 25 * u64::from(i);
            let capacity = 10_000 + 1_000 * u64::from(i);
            provinces.push(ProvinceFact { area_km2, land: Land::Arable, coastal, population: capacity * 6 / 10, owner: ActorId(x), neighbours, sea_neighbours, governor: None, culture: CultureId(0), religion: ReligionId(0) });
            province_names.push((format!("p{i:02}"), format!("Province {i}")));
        }
    }
    // Houses and persons, keyed by string id and compiled in sorted order like a scenario.
    let house_list: Vec<(String, String, u32)> = (0..SIDE).map(|a| (format!("h{a}"), format!("House {a}"), a)).chain([("o0".to_string(), "Other 0".to_string(), 0)]).collect();
    let house_index = |sid: &str| HouseId(house_list.iter().position(|(k, _, _)| k == sid).unwrap() as u32);
    const RULER_AGE: [i32; 5] = [35, 50, 28, 62, 44];
    const CHILD_AGE: [i32; 5] = [12, 24, 5, 30, 20];
    // (sid, name, house, born, parent)
    let mut person_list: Vec<(String, String, String, i32, Option<String>)> = Vec::new();
    for a in 0..SIDE as usize {
        person_list.push((format!("r{a}"), format!("Ruler {a}"), format!("h{a}"), 1477 - RULER_AGE[a], None));
        person_list.push((format!("c{a}"), format!("Child {a}"), format!("h{a}"), 1477 - CHILD_AGE[a], Some(format!("r{a}"))));
    }
    person_list.push(("b0".to_string(), "Brother 0".to_string(), "h0".to_string(), 1447, None));
    person_list.push(("o0a".to_string(), "Other 0 adult".to_string(), "o0".to_string(), 1437, None));
    person_list.sort_by(|x, y| x.0.cmp(&y.0));
    let person_index = |sid: &str| PersonId(person_list.iter().position(|(k, ..)| k == sid).unwrap() as u32);
    let persons: Vec<PersonFact> = person_list.iter().map(|(_, _, h, born, parent)| PersonFact { house: house_index(h), born: *born, parent: parent.as_deref().map(person_index), spouse: None }).collect();
    let houses: Vec<HouseFact> = house_list.iter().map(|(_, _, a)| HouseFact { polity: ActorId(*a), treasury_kg: 0, culture: CultureId(0), religion: ReligionId(0), soldiers: 0 }).collect();
    provinces[5].governor = Some(person_index("b0"));
    const RULES: [Succession; 5] = [Succession::Blood, Succession::Acclamation, Succession::Election, Succession::Mixed, Succession::Blood];
    let actors = (0..SIDE)
        .map(|a| ActorFact {
            treasury_kg: 100_000,
            capital: ProvinceId(a),
            soldiers: 600,
            institutions: Vec::new(),
            ruler: person_index(&format!("r{a}")),
            heir: if a == 0 { Some(person_index("c0")) } else { None },
            regent: None,
            succession: RULES[a as usize],
            goals: Vec::new(),
        })
        .collect();
    let facts = Facts { scenario_id: "testworld".to_string(), start_year: 1477, diseases: Vec::new(), provinces, actors, houses, persons, relations: RelationFacts::default(), regions: Vec::new() };
    let names = Names {
        actors: (0..SIDE).map(|a| (format!("s{a}"), format!("State {a}"))).collect(),
        provinces: province_names,
        houses: house_list.iter().map(|(k, n, _)| (k.clone(), n.clone())).collect(),
        persons: person_list.iter().map(|(k, n, ..)| (k.clone(), n.clone())).collect(),
        // One tongue and one faith: the tests that need a second set it on a province or a
        // house (the fields are open), so every other test keeps its numbers.
        cultures: vec!["c0".to_string()],
        religions: vec!["f0".to_string()],
        regions: Vec::new(),
    };
    (facts, names)
}

/// The test world as a `World`, with the crate's params.
pub fn world(seed: u64) -> World {
    let (facts, names) = five_states();
    World::from_facts(&facts, names, &Params::load().expect("params.toml"), seed).expect("test world")
}

/// The same world as scenario data: what `Simulation` is created from. Ids `s0..s4` and
/// `p00..p24` sort in the same order the facts above use, so both builders agree.
pub fn five_states_scenario() -> ScenarioData {
    let (facts, names) = five_states();
    let pname = |p: &ProvinceId| names.provinces[p.index()].0.clone();
    let person = |p: &PersonId| names.persons[p.index()].0.clone();
    let mut actors = BTreeMap::new();
    for (i, a) in facts.actors.iter().enumerate() {
        let (sid, name) = &names.actors[i];
        actors.insert(
            sid.clone(),
            ActorData {
                name: name.clone(),
                treasury_kg: a.treasury_kg,
                capital: pname(&a.capital),
                soldiers: a.soldiers,
                institutions: a.institutions.clone(),
                ruler: person(&a.ruler),
                heir: a.heir.as_ref().map(person),
                regent: a.regent.as_ref().map(person),
                succession: a.succession,
                aspirations: Vec::new(),
            },
        );
    }
    let mut houses = BTreeMap::new();
    for (i, h) in facts.houses.iter().enumerate() {
        let (sid, name) = &names.houses[i];
        houses.insert(sid.clone(), HouseData { name: name.clone(), polity: names.actors[h.polity.index()].0.clone(), treasury_kg: h.treasury_kg, culture: names.cultures[h.culture.index()].clone(), religion: names.religions[h.religion.index()].clone(), soldiers: h.soldiers });
    }
    let mut persons = BTreeMap::new();
    for (i, p) in facts.persons.iter().enumerate() {
        let (sid, name) = &names.persons[i];
        persons.insert(sid.clone(), PersonData { name: name.clone(), house: names.houses[p.house.index()].0.clone(), born: p.born, parent: p.parent.as_ref().map(person), spouse: p.spouse.as_ref().map(person) });
    }
    let mut provinces = BTreeMap::new();
    for (i, f) in facts.provinces.iter().enumerate() {
        let (pid, name) = &names.provinces[i];
        provinces.insert(
            pid.clone(),
            ProvinceData {
                name: name.clone(),
                owner: names.actors[f.owner.index()].0.clone(),
                area_km2: f.area_km2,
                land: f.land,
                coastal: f.coastal,
                population: f.population,
                neighbours: f.neighbours.iter().map(pname).collect(),
                sea_neighbours: f.sea_neighbours.iter().map(pname).collect(),
                governor: f.governor.as_ref().map(person),
                culture: names.cultures[f.culture.index()].clone(),
                religion: names.religions[f.religion.index()].clone(),
            },
        );
    }
    let verbs = [("raise_tax", "Raise tax"), ("lower_tax", "Lower tax"), ("recruit", "Recruit")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    ScenarioData {
        id: facts.scenario_id.clone(),
        start_year: facts.start_year,
        diseases: facts.diseases.clone(),
        display_currency: DisplayCurrency { name: "ducat".to_string(), kg_per_unit: 100.0 },
        actors,
        provinces,
        houses,
        persons,
        relations: None,
        regions: None,
        verbs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_and_facts_builders_agree() {
        let c = five_states_scenario().compile(&Params::load().unwrap()).unwrap();
        let (facts, names) = five_states();
        assert_eq!(c.facts, facts);
        assert_eq!(c.names, names);
        assert_eq!(c.names.provinces[7].0, "p07");
    }

    #[test]
    fn grid_is_connected_and_each_province_has_one_owner() {
        let w = world(0);
        assert_eq!(w.provinces.len(), 25);
        assert_eq!(w.actors.len(), 5);
        let owned: usize = w.actors.values().map(|a| a.provinces.len()).sum();
        assert_eq!(owned, 25);
        let mut seen = [false; 25];
        let mut stack = vec![ProvinceId(0)];
        while let Some(p) = stack.pop() {
            if std::mem::replace(&mut seen[p.index()], true) {
                continue;
            }
            stack.extend(w.provinces[&p].neighbours.iter().copied());
        }
        assert!(seen.iter().all(|s| *s));
        // The sea links are symmetric and only on the coast.
        for (p, pr) in &w.provinces {
            for s in &pr.sea_neighbours {
                assert!(pr.coastal && w.provinces[s].coastal);
                assert!(w.provinces[s].sea_neighbours.contains(p));
            }
        }
    }
}
