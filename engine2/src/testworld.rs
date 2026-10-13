//! The synthetic test world (plan §10, brief §0.3): five states on a 5×5 grid of provinces,
//! no aspirations, no institutions. Proof that the engine runs on facts alone.
//!
//! Lives in the library (not `tests/common`) because the determinism test runs it in a
//! child process of the test binary, and both halves must build the same world.

use std::collections::BTreeMap;

use crate::ids::{ActorId, ProvinceId};
use crate::scenario::{ActorData, DisplayCurrency, ProvinceData, ScenarioData};
use crate::world::Facts;

/// Five states, each a column of the 5×5 grid; province `i` has capacity
/// `10_000 + 1_000·i` and starts at 60 % of it; treasuries 100 000 kg each.
pub fn five_states() -> Facts {
    const SIDE: u32 = 5;
    let mut provinces = Vec::new();
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
            let capacity = 10_000 + 1_000 * u64::from(i);
            provinces.push((capacity, capacity * 6 / 10, ActorId(x), neighbours));
        }
    }
    Facts { start_year: 1477, provinces, treasuries: vec![100_000; SIDE as usize] }
}

/// The same world as scenario data: what E1b's `Simulation` is created from. Ids `s0..s4`
/// and `p00..p24` sort in the same order the facts above use, so both builders agree.
pub fn five_states_scenario() -> ScenarioData {
    let facts = five_states();
    let pid = |i: usize| format!("p{i:02}");
    let sid = |a: ActorId| format!("s{}", a.0);
    let mut actors = BTreeMap::new();
    for (i, t) in facts.treasuries.iter().enumerate() {
        actors.insert(sid(ActorId(i as u32)), ActorData { name: format!("State {i}"), treasury_kg: *t });
    }
    let mut provinces = BTreeMap::new();
    for (i, (capacity, population, owner, neighbours)) in facts.provinces.iter().enumerate() {
        provinces.insert(
            pid(i),
            ProvinceData {
                name: format!("Province {i}"),
                owner: sid(*owner),
                capacity: *capacity,
                population: *population,
                neighbours: neighbours.iter().map(|n| pid(n.index())).collect(),
            },
        );
    }
    let verbs = [("raise_tax", "Raise tax"), ("lower_tax", "Lower tax"), ("recruit", "Recruit")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    ScenarioData {
        id: "testworld".to_string(),
        start_year: facts.start_year,
        display_currency: DisplayCurrency { name: "ducat".to_string(), kg_per_unit: 100.0 },
        actors,
        provinces,
        verbs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[test]
    fn scenario_and_facts_builders_agree() {
        let (facts, names) = five_states_scenario().compile().unwrap();
        assert_eq!(facts, five_states());
        assert_eq!(names.actors.len(), 5);
        assert_eq!(names.provinces[7].0, "p07");
    }

    #[test]
    fn grid_is_connected_and_each_province_has_one_owner() {
        let w = World::from_facts(&five_states(), 0).unwrap();
        assert_eq!(w.provinces.len(), 25);
        assert_eq!(w.actors.len(), 5);
        let owned: usize = w.actors.values().map(|a| a.provinces.len()).sum();
        assert_eq!(owned, 25);
        // Reachability from province 0 over the neighbour lists.
        let mut seen = [false; 25];
        let mut stack = vec![ProvinceId(0)];
        while let Some(p) = stack.pop() {
            if std::mem::replace(&mut seen[p.index()], true) {
                continue;
            }
            stack.extend(w.provinces[&p].neighbours.iter().copied());
        }
        assert!(seen.iter().all(|s| *s));
    }
}
