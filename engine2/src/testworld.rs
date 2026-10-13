//! The synthetic test world (plan §10, brief §0.3): five states on a 5×5 grid of provinces,
//! no aspirations, no institutions. Proof that the engine runs on facts alone.
//!
//! Lives in the library (not `tests/common`) because the determinism test runs it in a
//! child process of the test binary, and both halves must build the same world.

use std::collections::BTreeMap;

use crate::ids::{ActorId, ProvinceId};
use crate::params::Params;
use crate::scenario::{ActorData, DisplayCurrency, ProvinceData, ScenarioData};
use crate::world::{ActorFact, Facts, Land, Names, ProvinceFact, World};

/// Five states, each a column of the 5×5 grid; all land arable; province `i` has an area
/// of `250 + 25·i` km², which at the arable density of `params.toml` (40 people per km²,
/// E0 П1) carries `10_000 + 1_000·i` people; it starts at 60 % of that; treasuries
/// 100 000 kg each. The top row is the coast, and each coastal province faces the coasts
/// two columns away across a bay (a sea link no land link duplicates). Each state's capital
/// is its top province, with 600 soldiers (≈ 1 % of its people, E0 А11). Returns the facts
/// and their names.
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
            provinces.push(ProvinceFact { area_km2, land: Land::Arable, coastal, population: capacity * 6 / 10, owner: ActorId(x), neighbours, sea_neighbours });
            province_names.push((format!("p{i:02}"), format!("Province {i}")));
        }
    }
    let actors = (0..SIDE).map(|a| ActorFact { treasury_kg: 100_000, capital: ProvinceId(a), soldiers: 600 }).collect();
    let facts = Facts { scenario_id: "testworld".to_string(), start_year: 1477, diseases: Vec::new(), provinces, actors };
    let names = Names { actors: (0..SIDE).map(|a| (format!("s{a}"), format!("State {a}"))).collect(), provinces: province_names };
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
    let mut actors = BTreeMap::new();
    for (i, a) in facts.actors.iter().enumerate() {
        let (sid, name) = &names.actors[i];
        actors.insert(sid.clone(), ActorData { name: name.clone(), treasury_kg: a.treasury_kg, capital: pname(&a.capital), soldiers: a.soldiers });
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
