//! Generated test worlds (brief §0.3 item 3 as refined by the owner, plan §12 item 11):
//! the acceptance ground of every E2 system is a family of worlds drawn by seed, not one
//! 5×5 grid, plus a handful of hand-made extreme worlds.
//!
//! The generator produces **scenario data**, not a world: it goes through the same schema,
//! `compile` and `from_facts` as an authored scenario, so it cannot bypass a check an author
//! is held to (population ≤ capacity, closed land list, no rule numbers).
//!
//! Everything drawn comes from the bands of E0 §5 where E0 has a row; what E0 has no row for
//! is a model choice (kind (в)) and is said so at the constant.

use std::collections::BTreeMap;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::scenario::{ActorData, DisplayCurrency, ProvinceData, ScenarioData};
use crate::verbs::VerbId;
use crate::world::Land;

/// Province area, km². E0 §2a: tens of provinces per scenario; Italy's ~300 000 km² over
/// ~25 provinces is ~12 000; the band leaves room for small city territories.
const AREA_KM2: (u64, u64) = (2_000, 15_000);
/// Starting population as a share of capacity. Below 1 by construction (the compile check
/// refuses more); the lower end is a world recovering from a crisis (E0 П4, П7).
const START_SHARE_OF_CAPACITY: (f64, f64) = (0.40, 0.95);
/// Yearly revenue per head in kg: E0 Э8 (3–16 % of output) × Э2 (520–1 000 kg per head).
const REVENUE_PER_HEAD_KG: (f64, f64) = (15.0, 160.0);
/// Treasury as years of revenue: from empty to two years in reserve (E0 Э7 gives debt
/// ceilings, not reserves — this end is a model choice, kind (в)).
const TREASURY_YEARS: (f64, f64) = (0.0, 2.0);
/// Mix of land types, kind (в): E0 П1 gives the density of each type, not how much of a
/// map is which; checked through П7 and Э8 once the systems exist.
const LAND_WEIGHTS: [(Land, u32); 4] = [(Land::Arable, 55), (Land::Hills, 25), (Land::Steppe, 12), (Land::Desert, 8)];
/// Grid side and number of states: tens of provinces, a handful of polities (brief §2a).
const GRID: (u32, u32) = (4, 7);
const STATES: (u32, u32) = (3, 8);

fn pick<R: Rng>(rng: &mut R, (lo, hi): (u64, u64)) -> u64 {
    rng.gen_range(lo..=hi)
}

fn pick_f<R: Rng>(rng: &mut R, (lo, hi): (f64, f64)) -> f64 {
    rng.gen_range(lo..=hi)
}

fn land<R: Rng>(rng: &mut R) -> Land {
    let total: u32 = LAND_WEIGHTS.iter().map(|(_, w)| w).sum();
    let mut x = rng.gen_range(0..total);
    for (l, w) in LAND_WEIGHTS {
        if x < w {
            return l;
        }
        x -= w;
    }
    Land::Arable
}

/// A province under construction.
#[derive(Debug, Clone)]
struct Cell {
    owner: usize,
    area_km2: u64,
    land: Land,
    coastal: bool,
    /// Share of capacity the province starts at.
    start_share: f64,
}

/// The raw layout before it becomes scenario data: a grid of cells, state count, names.
struct Layout {
    id: String,
    side: u32,
    cells: Vec<Cell>,
    states: usize,
    /// Extra provinces off the grid (an island), each with its neighbours among them.
    extra: Vec<(Cell, Vec<usize>)>,
}

impl Layout {
    fn grid_neighbours(&self, i: u32) -> Vec<u32> {
        let (x, y) = (i % self.side, i / self.side);
        let mut n = Vec::new();
        if x > 0 {
            n.push(i - 1);
        }
        if x + 1 < self.side {
            n.push(i + 1);
        }
        if y > 0 {
            n.push(i - self.side);
        }
        if y + 1 < self.side {
            n.push(i + self.side);
        }
        n
    }

    fn on_border(&self, i: u32) -> bool {
        let (x, y) = (i % self.side, i / self.side);
        x == 0 || y == 0 || x + 1 == self.side || y + 1 == self.side
    }

    /// Scenario data: capacity is left to the engine; population is `share × area ×
    /// density`, computed with the same params the compile check uses, so a generated world
    /// never starts above capacity.
    fn into_scenario(self, params: &crate::params::Params) -> ScenarioData {
        let pid = |i: usize| format!("p{i:02}");
        let sid = |a: usize| format!("s{a}");
        let mut provinces = BTreeMap::new();
        let mut pop_by_state: BTreeMap<usize, u64> = BTreeMap::new();
        let n = self.cells.len();
        let mut push = |i: usize, cell: &Cell, neighbours: Vec<String>, pop_by_state: &mut BTreeMap<usize, u64>| {
            let capacity = params.capacity(cell.area_km2, cell.land);
            let population = ((capacity as f64) * cell.start_share).floor() as u64;
            *pop_by_state.entry(cell.owner).or_insert(0) += population;
            provinces.insert(
                pid(i),
                ProvinceData {
                    name: format!("Province {i}"),
                    owner: sid(cell.owner),
                    area_km2: cell.area_km2,
                    land: cell.land,
                    coastal: cell.coastal,
                    population,
                    neighbours,
                },
            );
        };
        for (i, cell) in self.cells.iter().enumerate() {
            let neighbours = self.grid_neighbours(i as u32).into_iter().map(|j| pid(j as usize)).collect();
            push(i, cell, neighbours, &mut pop_by_state);
        }
        for (k, (cell, nb)) in self.extra.iter().enumerate() {
            let i = n + k;
            let neighbours = nb.iter().map(|j| pid(n + j)).collect();
            push(i, cell, neighbours, &mut pop_by_state);
        }
        let mut actors = BTreeMap::new();
        for s in 0..self.states {
            actors.insert(sid(s), ActorData { name: format!("State {s}"), treasury_kg: 0 });
        }
        let verbs = VerbId::ALL.iter().map(|v| (v.key().to_string(), v.key().replace('_', " "))).collect();
        let mut data = ScenarioData {
            id: self.id,
            start_year: 1000,
            display_currency: DisplayCurrency { name: "coin".to_string(), kg_per_unit: 100.0 },
            actors,
            provinces,
            verbs,
        };
        // Treasuries need the populations, and the treasury draw is part of the seed's story:
        // keep it in the layout's own stream by re-deriving from the id.
        let mut rng = ChaCha8Rng::seed_from_u64(fnv(&data.id) ^ 0x7ea5_u64);
        for s in 0..self.states {
            let pop = pop_by_state.get(&s).copied().unwrap_or(0) as f64;
            let kg = pop * pick_f(&mut rng, REVENUE_PER_HEAD_KG) * pick_f(&mut rng, TREASURY_YEARS);
            data.actors.get_mut(&sid(s)).unwrap().treasury_kg = kg.floor() as i64;
        }
        data
    }
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Grow `states` contiguous territories from random seed cells (BFS in a random order), so
/// polities are blobs of varied size rather than stripes. Only the first `width` columns
/// are settled: the rest is left for an external force, so no polity can be wiped out by
/// it (seed 12 of the first version lost a state that way).
fn assign_owners<R: Rng>(rng: &mut R, side: u32, width: u32, states: usize) -> Vec<usize> {
    let n = (side * side) as usize;
    let settled = (side * width) as usize;
    let mut owner = vec![usize::MAX; n];
    let mut frontier: Vec<Vec<u32>> = vec![Vec::new(); states];
    let mut used = std::collections::BTreeSet::new();
    for (s, front) in frontier.iter_mut().enumerate() {
        loop {
            let c = rng.gen_range(0..side * side);
            if c % side >= width {
                continue;
            }
            if used.insert(c) {
                owner[c as usize] = s;
                front.push(c);
                break;
            }
        }
    }
    let layout = Layout { id: String::new(), side, cells: Vec::new(), states, extra: Vec::new() };
    let mut remaining = settled - states;
    while remaining > 0 {
        let s = rng.gen_range(0..states);
        if frontier[s].is_empty() {
            continue;
        }
        let k = rng.gen_range(0..frontier[s].len());
        let c = frontier[s][k];
        let free: Vec<u32> = layout.grid_neighbours(c).into_iter().filter(|j| j % side < width && owner[*j as usize] == usize::MAX).collect();
        if free.is_empty() {
            frontier[s].swap_remove(k);
            if frontier.iter().all(Vec::is_empty) {
                // Cannot happen on a connected grid while cells remain, but never spin.
                for (i, o) in owner.iter_mut().enumerate() {
                    if *o == usize::MAX && (i as u32) % side < width {
                        *o = 0;
                    }
                }
                break;
            }
            continue;
        }
        let j = free[rng.gen_range(0..free.len())];
        owner[j as usize] = s;
        frontier[s].push(j);
        remaining -= 1;
    }
    owner
}

/// A world drawn by seed. Same seed, same world, on any platform (ChaCha8).
pub fn generate(seed: u64, params: &crate::params::Params) -> ScenarioData {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let side = pick(&mut rng, (u64::from(GRID.0), u64::from(GRID.1))) as u32;
    let max_states = STATES.1.min(side);
    let mut states = pick(&mut rng, (u64::from(STATES.0), u64::from(max_states))) as usize;
    let external_force = rng.gen_bool(0.5);
    let width = if external_force { side - 1 } else { side };
    let mut owner = assign_owners(&mut rng, side, width, states);
    let mut cells: Vec<Cell> = (0..side * side)
        .map(|i| Cell {
            owner: owner[i as usize],
            area_km2: pick(&mut rng, AREA_KM2),
            land: land(&mut rng),
            coastal: false,
            start_share: pick_f(&mut rng, START_SHARE_OF_CAPACITY),
        })
        .collect();
    let layout = Layout { id: format!("gen{seed}"), side, cells: Vec::new(), states, extra: Vec::new() };
    // Sea: a coast along the grid's border, half the border cells.
    for i in 0..side * side {
        if layout.on_border(i) && rng.gen_bool(0.5) {
            cells[i as usize].coastal = true;
        }
    }
    // An external force at the edge: the last column becomes steppe under its own polity.
    if external_force {
        let s = states;
        states += 1;
        for y in 0..side {
            let i = (y * side + side - 1) as usize;
            owner[i] = s;
            cells[i].owner = s;
            cells[i].land = Land::Steppe;
            cells[i].area_km2 = pick(&mut rng, (8_000, 15_000));
        }
    }
    Layout { id: format!("gen{seed}"), side, cells, states, extra: Vec::new() }.into_scenario(params)
}

/// The hand-made extreme worlds (owner, plan §12 item 11): each with its name.
pub fn extreme_worlds(params: &crate::params::Params) -> Vec<(&'static str, ScenarioData)> {
    let mut out = Vec::new();
    let cell = |owner: usize, area: u64, land: Land, coastal: bool, share: f64| Cell { owner, area_km2: area, land, coastal, start_share: share };

    // One empire against small polities: 6×6, the empire holds all but the four corners.
    let side = 6;
    let mut cells = Vec::new();
    for i in 0..side * side {
        let (x, y) = (i % side, i / side);
        let corner = (x == 0 || x == side - 1) && (y == 0 || y == side - 1);
        let owner = match (corner, x, y) {
            (false, ..) => 0,
            (true, 0, 0) => 1,
            (true, _, 0) => 2,
            (true, 0, _) => 3,
            (true, ..) => 4,
        };
        cells.push(cell(owner, 8_000, Land::Arable, false, 0.8));
    }
    out.push(("empire_vs_small", Layout { id: "x_empire_vs_small".into(), side, cells, states: 5, extra: Vec::new() }.into_scenario(params)));

    // An isolated island: a 4×4 mainland of three polities plus one island province with no
    // land neighbour at all, its own polity, coastal.
    let side = 4;
    let cells: Vec<Cell> = (0..side * side).map(|i| cell((i % 3) as usize, 6_000, Land::Arable, i % side == 0, 0.7)).collect();
    let island = cell(3, 3_000, Land::Hills, true, 0.9);
    out.push(("isolated_island", Layout { id: "x_isolated_island".into(), side, cells, states: 4, extra: vec![(island, vec![])] }.into_scenario(params)));

    // A polity without arable land: 5×5, state 0 holds the top two rows, all hills and steppe.
    let side = 5;
    let cells: Vec<Cell> = (0..side * side)
        .map(|i| {
            let y = i / side;
            if y < 2 {
                cell(0, 10_000, if i % 2 == 0 { Land::Hills } else { Land::Steppe }, false, 0.9)
            } else {
                cell(1 + ((i % side) % 2) as usize, 6_000, Land::Arable, false, 0.7)
            }
        })
        .collect();
    out.push(("no_arable", Layout { id: "x_no_arable".into(), side, cells, states: 3, extra: Vec::new() }.into_scenario(params)));

    // Nomads at the edge: three settled polities on the west, one steppe polity on the two
    // eastern columns with large sparse provinces.
    let side = 6;
    let cells: Vec<Cell> = (0..side * side)
        .map(|i| {
            let (x, y) = (i % side, i / side);
            if x >= 4 {
                cell(3, 15_000, Land::Steppe, false, 0.9)
            } else {
                cell((y / 2) as usize, 7_000, Land::Arable, x == 0, 0.75)
            }
        })
        .collect();
    out.push(("nomads_at_edge", Layout { id: "x_nomads_at_edge".into(), side, cells, states: 4, extra: Vec::new() }.into_scenario(params)));

    // Desert belt: a 5×5 with the middle row all desert, two polities on either side.
    let side = 5;
    let cells: Vec<Cell> = (0..side * side)
        .map(|i| {
            let y = i / side;
            match y {
                2 => cell(if i % 2 == 0 { 0 } else { 1 }, 12_000, Land::Desert, false, 0.5),
                0 | 1 => cell(0, 7_000, Land::Arable, y == 0, 0.8),
                _ => cell(1, 7_000, Land::Hills, y == 4, 0.8),
            }
        })
        .collect();
    out.push(("desert_belt", Layout { id: "x_desert_belt".into(), side, cells, states: 2, extra: Vec::new() }.into_scenario(params)));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Params;

    #[test]
    fn same_seed_same_world_and_different_seeds_differ() {
        let p = Params::load().unwrap();
        assert_eq!(generate(7, &p), generate(7, &p));
        assert_ne!(generate(7, &p), generate(8, &p));
    }

    #[test]
    fn generated_worlds_stay_inside_the_bands() {
        let p = Params::load().unwrap();
        for seed in 1..=20 {
            let s = generate(seed, &p);
            assert!((3..=9).contains(&s.actors.len()), "seed {seed}: {} states", s.actors.len());
            assert!((16..=49).contains(&s.provinces.len()), "seed {seed}: {} provinces", s.provinces.len());
            for (k, pr) in &s.provinces {
                assert!((AREA_KM2.0..=AREA_KM2.1).contains(&pr.area_km2), "seed {seed} {k}");
                let cap = p.capacity(pr.area_km2, pr.land);
                assert!(pr.population <= cap, "seed {seed} {k}: {} > {cap}", pr.population);
                assert!(pr.population as f64 >= (cap as f64 * START_SHARE_OF_CAPACITY.0).floor() - 1.0, "seed {seed} {k}");
            }
            // Every polity owns at least one province.
            for a in s.actors.keys() {
                assert!(s.provinces.values().any(|pr| &pr.owner == a), "seed {seed}: {a} owns nothing");
            }
        }
    }

    #[test]
    fn extreme_worlds_are_what_their_names_say() {
        let p = Params::load().unwrap();
        let worlds = extreme_worlds(&p);
        assert_eq!(worlds.len(), 5);
        let get = |name: &str| worlds.iter().find(|(n, _)| *n == name).map(|(_, s)| s).unwrap();
        let empire = get("empire_vs_small");
        assert_eq!(empire.provinces.values().filter(|p| p.owner == "s0").count(), 32);
        let island = get("isolated_island");
        assert!(island.provinces.values().any(|p| p.neighbours.is_empty() && p.coastal));
        let no_arable = get("no_arable");
        assert!(no_arable.provinces.values().filter(|p| p.owner == "s0").all(|p| p.land != Land::Arable));
        let nomads = get("nomads_at_edge");
        assert!(nomads.provinces.values().filter(|p| p.owner == "s3").all(|p| p.land == Land::Steppe));
    }
}
