//! The synthetic test world (plan §10, brief §0.3): five states on a 5×5 grid of provinces,
//! no aspirations, no institutions. Proof that the engine runs on facts alone.
//!
//! Lives in the library (not `tests/common`) because the determinism test runs it in a
//! child process of the test binary, and both halves must build the same world.

use crate::ids::{ActorId, ProvinceId};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

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
