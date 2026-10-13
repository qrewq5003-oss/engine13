//! Generated test worlds (brief §0.3 item 3 as refined by the owner, plan §12 item 11, §13):
//! the acceptance ground of every E2 system is a family of worlds drawn by seed, not one
//! 5×5 grid, plus a handful of hand-made extreme worlds.
//!
//! The generator produces **scenario data**, not a world: it goes through the same schema,
//! `compile` and `from_facts` as an authored scenario, so it cannot bypass a check an author
//! is held to (population ≤ capacity, closed land list, no rule numbers).
//!
//! **Irregular graphs (owner's review of PR #280).** A system of movement and supply proven
//! only on a rectangular grid silently assumes "at most four neighbours" and "there is always
//! a way round". So the map is a grid with sea carved out of it, diagonal links added, a
//! chokepoint (one passage between two parts) carved on purpose, and holdings without a land
//! link — an overseas island, an enclave — handed to a polity across the water. Degrees run
//! from 1 to 7. Sizes run up to ~15 polities and ~85 provinces: the scale the engine must
//! carry, and the first check of speed.
//!
//! Everything drawn comes from the bands of E0 §5 where E0 has a row; what E0 has no row for
//! is a model choice (kind (в)) and is said so at the constant. The fingerprint of
//! `generate(1)` is pinned: an update of `rand` that changed every world would break
//! "fails on seed 12" silently otherwise.

use std::collections::{BTreeMap, BTreeSet};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::params::Params;
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
/// Map width and height in cells, before the sea is carved: up to ~90 land provinces.
const WIDTH: (u64, u64) = (4, 12);
const HEIGHT: (u64, u64) = (4, 10);
/// Share of cells carved out as sea, kind (в): shapes the coastline, peninsulas, islands.
const SEA_SHARE: (f64, f64) = (0.05, 0.30);
/// Probability that a diagonal pair of land cells is linked, kind (в). Raises degrees to 7.
const DIAGONAL_LINK: f64 = 0.35;
/// Polities, bounded by one per three land cells.
const STATES: (u64, u64) = (3, 15);
/// Maximum neighbours a province may have.
const MAX_DEGREE: usize = 7;

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

fn fresh_cell<R: Rng>(rng: &mut R) -> Cell {
    Cell { owner: usize::MAX, area_km2: pick(rng, AREA_KM2), land: land(rng), coastal: false, start_share: pick_f(rng, START_SHARE_OF_CAPACITY) }
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

/// A map under construction: cells on a grid, some of them sea, with an explicit graph.
struct Map {
    width: u32,
    height: u32,
    /// `None` is sea.
    cells: Vec<Option<Cell>>,
    /// Land graph, symmetric, by cell index.
    links: BTreeMap<u32, BTreeSet<u32>>,
}

impl Map {
    fn xy(&self, i: u32) -> (u32, u32) {
        (i % self.width, i / self.width)
    }

    fn index(&self, x: u32, y: u32) -> u32 {
        y * self.width + x
    }

    fn is_land(&self, i: u32) -> bool {
        self.cells[i as usize].is_some()
    }

    fn cell_mut(&mut self, i: u32) -> &mut Cell {
        self.cells[i as usize].as_mut().expect("land cell")
    }

    fn owner(&self, i: u32) -> usize {
        self.cells[i as usize].as_ref().expect("land cell").owner
    }

    fn link(&mut self, a: u32, b: u32) {
        self.links.entry(a).or_default().insert(b);
        self.links.entry(b).or_default().insert(a);
    }

    fn degree(&self, i: u32) -> usize {
        self.links.get(&i).map_or(0, BTreeSet::len)
    }

    fn neighbours(&self, i: u32) -> Vec<u32> {
        self.links.get(&i).into_iter().flatten().copied().collect()
    }

    /// Orthogonal links between land cells.
    fn build_orthogonal_links(&mut self) {
        self.links.clear();
        for y in 0..self.height {
            for x in 0..self.width {
                let i = self.index(x, y);
                if !self.is_land(i) {
                    continue;
                }
                if x + 1 < self.width && self.is_land(i + 1) {
                    self.link(i, i + 1);
                }
                if y + 1 < self.height && self.is_land(i + self.width) {
                    self.link(i, i + self.width);
                }
            }
        }
    }

    /// Diagonal links by chance, keeping every degree ≤ 7.
    fn add_diagonal_links<R: Rng>(&mut self, rng: &mut R) {
        for y in 0..self.height.saturating_sub(1) {
            for x in 0..self.width {
                let i = self.index(x, y);
                if !self.is_land(i) {
                    continue;
                }
                for dx in [-1i64, 1] {
                    let nx = x as i64 + dx;
                    if nx < 0 || nx >= self.width as i64 {
                        continue;
                    }
                    let j = self.index(nx as u32, y + 1);
                    if self.is_land(j) && rng.gen_bool(DIAGONAL_LINK) && self.degree(i) < MAX_DEGREE && self.degree(j) < MAX_DEGREE {
                        self.link(i, j);
                    }
                }
            }
        }
    }

    /// Connected components of the land graph, largest first.
    fn components(&self) -> Vec<Vec<u32>> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for start in (0..self.cells.len() as u32).filter(|i| self.is_land(*i)) {
            if !seen.insert(start) {
                continue;
            }
            let mut comp = vec![start];
            let mut stack = vec![start];
            while let Some(c) = stack.pop() {
                for n in self.neighbours(c) {
                    if seen.insert(n) {
                        comp.push(n);
                        stack.push(n);
                    }
                }
            }
            comp.sort();
            out.push(comp);
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.len()));
        out
    }

    /// Mark the coast: a land cell next to sea or to the map's edge.
    fn mark_coast(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let i = self.index(x, y);
                if !self.is_land(i) {
                    continue;
                }
                let edge = x == 0 || y == 0 || x + 1 == self.width || y + 1 == self.height;
                let sea_next = [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    nx >= 0 && ny >= 0 && nx < self.width as i64 && ny < self.height as i64 && !self.is_land(self.index(nx as u32, ny as u32))
                });
                self.cell_mut(i).coastal = edge || sea_next;
            }
        }
    }

    /// Grow `states` contiguous territories over `cells` (BFS in a random order over the
    /// land graph), owners numbered from `first`, so polities are blobs of varied size
    /// rather than stripes.
    fn grow<R: Rng>(&mut self, rng: &mut R, cells: &[u32], first: usize, states: usize) {
        let mut frontier: Vec<Vec<u32>> = vec![Vec::new(); states];
        let mut free: BTreeSet<u32> = cells.iter().copied().collect();
        for (k, front) in frontier.iter_mut().enumerate() {
            let pool: Vec<u32> = free.iter().copied().collect();
            let c = pool[rng.gen_range(0..pool.len())];
            free.remove(&c);
            self.cell_mut(c).owner = first + k;
            front.push(c);
        }
        while !free.is_empty() {
            if frontier.iter().all(Vec::is_empty) {
                // Cells unreachable from every frontier: never on one component, but never spin.
                for c in &free {
                    self.cell_mut(*c).owner = first;
                }
                break;
            }
            let k = rng.gen_range(0..states);
            if frontier[k].is_empty() {
                continue;
            }
            let at = rng.gen_range(0..frontier[k].len());
            let c = frontier[k][at];
            let open: Vec<u32> = self.neighbours(c).into_iter().filter(|j| free.contains(j)).collect();
            if open.is_empty() {
                frontier[k].swap_remove(at);
                continue;
            }
            let j = open[rng.gen_range(0..open.len())];
            free.remove(&j);
            self.cell_mut(j).owner = first + k;
            frontier[k].push(j);
        }
    }

    /// Scenario data: capacity is left to the engine; population is `share × area ×
    /// density`, computed with the same params the compile check uses, so a generated world
    /// never starts above capacity. Province ids are cell indices, so the map stays legible.
    fn into_scenario(self, id: String, states: usize, params: &Params) -> ScenarioData {
        let pid = |i: u32| format!("p{i:03}");
        let sid = |a: usize| format!("s{a:02}");
        let mut provinces = BTreeMap::new();
        let mut pop_by_state: BTreeMap<usize, u64> = BTreeMap::new();
        for (i, cell) in self.cells.iter().enumerate() {
            let Some(cell) = cell else { continue };
            let i = i as u32;
            let capacity = params.capacity(cell.area_km2, cell.land);
            let population = ((capacity as f64) * cell.start_share).floor() as u64;
            *pop_by_state.entry(cell.owner).or_insert(0) += population;
            let neighbours = self.neighbours(i).into_iter().map(pid).collect();
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
        }
        let mut rng = ChaCha8Rng::seed_from_u64(fnv(id.as_bytes()) ^ 0x7ea5_u64);
        let mut actors = BTreeMap::new();
        for s in 0..states {
            let pop = pop_by_state.get(&s).copied().unwrap_or(0) as f64;
            let kg = pop * pick_f(&mut rng, REVENUE_PER_HEAD_KG) * pick_f(&mut rng, TREASURY_YEARS);
            actors.insert(sid(s), ActorData { name: format!("State {s}"), treasury_kg: kg.floor() as i64 });
        }
        let verbs = VerbId::ALL.iter().map(|v| (v.key().to_string(), v.key().replace('_', " "))).collect();
        ScenarioData {
            id,
            start_year: 1000,
            display_currency: DisplayCurrency { name: "coin".to_string(), kg_per_unit: 100.0 },
            actors,
            provinces,
            verbs,
        }
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A world drawn by seed. Same seed, same world, on any platform (ChaCha8).
pub fn generate(seed: u64, params: &Params) -> ScenarioData {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let width = pick(&mut rng, WIDTH) as u32;
    let height = pick(&mut rng, HEIGHT) as u32;
    let n = width * height;
    let mut map = Map { width, height, cells: (0..n).map(|_| Some(fresh_cell(&mut rng))).collect(), links: BTreeMap::new() };

    // Sea: carve a share of cells, biased to the edges so coasts, peninsulas and islands appear.
    let sea_share = pick_f(&mut rng, SEA_SHARE);
    for y in 0..height {
        for x in 0..width {
            let edge = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
            let p = if edge { sea_share * 2.0 } else { sea_share * 0.6 };
            if rng.gen_bool(p.min(0.95)) {
                let i = map.index(x, y);
                map.cells[i as usize] = None;
            }
        }
    }
    // Chokepoint: a column of sea with a single passage, between two parts of the map.
    if width >= 6 && rng.gen_bool(0.5) {
        let col = rng.gen_range(2..width - 2);
        let pass = rng.gen_range(0..height);
        for y in 0..height {
            let i = map.index(col, y);
            if y != pass {
                map.cells[i as usize] = None;
            } else if map.cells[i as usize].is_none() {
                map.cells[i as usize] = Some(fresh_cell(&mut rng));
            }
        }
    }
    // Keep at least a dozen land cells.
    let mut land_count = (0..n).filter(|i| map.is_land(*i)).count();
    let mut k = 0;
    while land_count < 12 && k < n {
        if map.cells[k as usize].is_none() {
            map.cells[k as usize] = Some(fresh_cell(&mut rng));
            land_count += 1;
        }
        k += 1;
    }
    map.build_orthogonal_links();
    map.add_diagonal_links(&mut rng);
    map.mark_coast();

    // Polities: grow blobs over the largest landmass; smaller landmasses (islands) go either
    // to a new polity of their own or overseas to an existing one.
    let comps = map.components();
    let main = comps[0].clone();
    let max_states = (STATES.1 as usize).min((main.len() / 3).max(1));
    let mut settled_states = pick(&mut rng, (STATES.0.min(max_states as u64), max_states as u64)) as usize;
    let mut states = settled_states;

    // External force: the eastmost land cells of the main landmass become steppe under their
    // own polity, excluded from the growth of the settled ones (seed 12 of the first version
    // lost a polity to an external force painted over it).
    let mut settled: Vec<u32> = main.clone();
    if width >= 6 && rng.gen_bool(0.5) {
        let east: Vec<u32> = main.iter().copied().filter(|i| map.xy(*i).0 + 1 >= width - 1).collect();
        if !east.is_empty() && east.len() + 3 <= main.len() {
            let nomads = states;
            states += 1;
            for i in &east {
                let c = map.cell_mut(*i);
                c.owner = nomads;
                c.land = Land::Steppe;
                c.area_km2 = pick(&mut rng, (8_000, 15_000));
            }
            settled.retain(|i| !east.contains(i));
            settled_states = settled_states.min(settled.len());
        }
    }
    map.grow(&mut rng, &settled, 0, settled_states);
    for comp in comps.iter().skip(1) {
        if states < STATES.1 as usize && rng.gen_bool(0.5) {
            map.grow(&mut rng, comp, states, 1);
            states += 1;
        } else {
            // Overseas holding of an existing polity: a coastal power across the water.
            let owner = rng.gen_range(0..states);
            for i in comp {
                map.cell_mut(*i).owner = owner;
            }
        }
    }
    // Enclave: an interior cell of one polity handed to another, with no land link to its
    // new owner's territory.
    if states >= 2 && rng.gen_bool(0.5) {
        let candidates: Vec<u32> = main
            .iter()
            .copied()
            .filter(|i| {
                let o = map.owner(*i);
                let ns = map.neighbours(*i);
                ns.len() >= 3 && ns.iter().all(|j| map.owner(*j) == o)
            })
            .collect();
        if !candidates.is_empty() {
            let i = candidates[rng.gen_range(0..candidates.len())];
            let o = map.owner(i);
            let other = (o + 1 + rng.gen_range(0..states - 1)) % states;
            map.cell_mut(i).owner = other;
        }
    }
    map.into_scenario(format!("gen{seed}"), states, params)
}

/// A hand-made map from an ASCII sketch: `.` is sea, a letter is the owning polity
/// (`a` = 0); orthogonal links only. For the extreme worlds.
fn sketch(id: &str, rows: &[&str], land_of: impl Fn(usize) -> Land, area: u64, share: f64, params: &Params) -> ScenarioData {
    let height = rows.len() as u32;
    let width = rows[0].len() as u32;
    let mut states = 0usize;
    let cells: Vec<Option<Cell>> = rows
        .iter()
        .flat_map(|r| r.chars().collect::<Vec<_>>())
        .map(|ch| {
            if ch == '.' {
                None
            } else {
                let owner = (ch as u8 - b'a') as usize;
                states = states.max(owner + 1);
                Some(Cell { owner, area_km2: area, land: land_of(owner), coastal: false, start_share: share })
            }
        })
        .collect();
    let mut map = Map { width, height, cells, links: BTreeMap::new() };
    map.build_orthogonal_links();
    map.mark_coast();
    map.into_scenario(id.to_string(), states, params)
}

/// The hand-made extreme worlds (owner, plan §12 item 11): each with its name.
pub fn extreme_worlds(params: &Params) -> Vec<(&'static str, ScenarioData)> {
    vec![
        // One empire against small polities: the empire holds everything but the corners.
        ("empire_vs_small", sketch("x_empire_vs_small", &["baaaac", "aaaaaa", "aaaaaa", "aaaaaa", "aaaaaa", "daaaae"], |_| Land::Arable, 8_000, 0.8, params)),
        // An isolated island: a mainland of three polities and an island with no land link.
        ("isolated_island", sketch("x_isolated_island", &["aabb..", "aabb..", "cccc..", "cccc.d"], |o| if o == 3 { Land::Hills } else { Land::Arable }, 6_000, 0.7, params)),
        // A polity without arable land: polity `a` on the top rows is all hills.
        ("no_arable", sketch("x_no_arable", &["aaaaa", "aaaaa", "bcbcb", "bcbcb", "bcbcb"], |o| if o == 0 { Land::Hills } else { Land::Arable }, 7_000, 0.8, params)),
        // Nomads at the edge: three settled polities west, a steppe polity on the two eastern columns.
        ("nomads_at_edge", sketch("x_nomads_at_edge", &["aaaadd", "aaaadd", "bbbbdd", "bbbbdd", "ccccdd", "ccccdd"], |o| if o == 3 { Land::Steppe } else { Land::Arable }, 9_000, 0.75, params)),
        // A chokepoint: two halves joined by a single passage, and a peninsula on the east.
        ("chokepoint", sketch("x_chokepoint", &["aaa.bbb", "aaa.bbb", "aaaabbb", "aaa.bbb", "aaa.b..", "....b..", "....b.."], |_| Land::Arable, 6_000, 0.8, params)),
        // An overseas colony and an enclave: `b` holds an island and a cell inside `a`.
        ("colony_and_enclave", sketch("x_colony_and_enclave", &["aaaa..", "abaa.b", "aaaa.b", "aaaa..", "cccc..", "cccc.."], |_| Land::Arable, 6_000, 0.8, params)),
    ]
}

/// A stable fingerprint of a scenario's text form, for pinning worlds against silent
/// changes of the generator or of `rand`.
pub fn fingerprint(s: &ScenarioData) -> u64 {
    fnv(format!("{s:?}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_world_and_different_seeds_differ() {
        let p = Params::load().unwrap();
        assert_eq!(generate(7, &p), generate(7, &p));
        assert_ne!(generate(7, &p), generate(8, &p));
    }

    /// Pinned on first implementation of the irregular generator. If this changes, every
    /// seed plays a different world and "fails on seed N" stops reproducing: bump it only
    /// with a deliberate change of the generator, never with a dependency update.
    #[test]
    fn world_one_is_pinned() {
        let p = Params::load().unwrap();
        assert_eq!(fingerprint(&generate(1, &p)), PINNED_WORLD_ONE);
    }

    const PINNED_WORLD_ONE: u64 = 13_861_495_800_295_043_542;

    #[test]
    fn generated_worlds_stay_inside_the_bands() {
        let p = Params::load().unwrap();
        let mut max_states = 0;
        let mut max_provinces = 0;
        for seed in 1..=50 {
            let s = generate(seed, &p);
            max_states = max_states.max(s.actors.len());
            max_provinces = max_provinces.max(s.provinces.len());
            assert!((3..=STATES.1 as usize + 1).contains(&s.actors.len()), "seed {seed}: {} states", s.actors.len());
            assert!((12..=110).contains(&s.provinces.len()), "seed {seed}: {} provinces", s.provinces.len());
            for (k, pr) in &s.provinces {
                assert!((AREA_KM2.0..=AREA_KM2.1).contains(&pr.area_km2), "seed {seed} {k}");
                let cap = p.capacity(pr.area_km2, pr.land);
                assert!(pr.population <= cap, "seed {seed} {k}: {} > {cap}", pr.population);
                assert!(pr.neighbours.len() <= MAX_DEGREE, "seed {seed} {k}: degree {}", pr.neighbours.len());
                for nb in &pr.neighbours {
                    assert!(s.provinces[nb].neighbours.contains(k), "seed {seed}: link {k}–{nb} not symmetric");
                }
            }
            for a in s.actors.keys() {
                assert!(s.provinces.values().any(|pr| &pr.owner == a), "seed {seed}: {a} owns nothing");
            }
        }
        // The scale the engine must carry is reached somewhere in the family.
        assert!(max_states >= 12, "largest world has only {max_states} states");
        assert!(max_provinces >= 70, "largest world has only {max_provinces} provinces");
    }

    #[test]
    fn extreme_worlds_are_what_their_names_say() {
        let p = Params::load().unwrap();
        let worlds = extreme_worlds(&p);
        assert_eq!(worlds.len(), 6);
        let get = |name: &str| worlds.iter().find(|(n, _)| *n == name).map(|(_, s)| s).unwrap();
        let empire = get("empire_vs_small");
        assert_eq!(empire.provinces.values().filter(|p| p.owner == "s00").count(), 32);
        let island = get("isolated_island");
        assert!(island.provinces.values().any(|p| p.neighbours.is_empty() && p.coastal && p.owner == "s03"));
        let no_arable = get("no_arable");
        assert!(no_arable.provinces.values().filter(|p| p.owner == "s00").all(|p| p.land != Land::Arable));
        let nomads = get("nomads_at_edge");
        assert!(nomads.provinces.values().filter(|p| p.owner == "s03").all(|p| p.land == Land::Steppe));
        let choke = get("chokepoint");
        assert_eq!(choke.provinces.len(), 31);
        let colony = get("colony_and_enclave");
        assert!(colony.provinces.values().any(|p| p.owner == "s01" && !p.neighbours.is_empty() && p.neighbours.iter().all(|n| colony.provinces[n].owner == "s00")));
    }
}
