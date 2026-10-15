//! Generated test worlds (brief §0.3 item 3 as refined by the owner, plan §12 item 11, §13):
//! the acceptance ground of every E2 system is a family of worlds drawn by seed, not one
//! 5×5 grid, plus a handful of hand-made extreme worlds.
//!
//! The generator produces **scenario data**, not a world: it goes through the same schema,
//! `compile` and `from_facts` as an authored scenario, so it cannot bypass a check an author
//! is held to (population ≤ capacity, closed land list, symmetric graphs, no rule numbers).
//!
//! **Irregular graphs (owner's review of PR #280).** A system of movement and supply proven
//! only on a rectangular grid silently assumes "at most four neighbours" and "there is always
//! a way round". So the map is a grid with sea carved out of it, diagonal links added, a
//! chokepoint (one passage between two parts) carved on purpose, and holdings without a land
//! link — an overseas island, an enclave — handed to a polity across the water. Degrees run
//! from 1 to 7. Sizes run up to ~15 polities and ~90 provinces: the scale the engine must
//! carry, and the first check of speed.
//!
//! **Sea links (plan §14).** Two coasts are linked if a path of at most `SEA_REACH` sea cells
//! joins them; every landmass beyond the main one is guaranteed a link to it, so no island or
//! colony is unreachable by design. Each polity gets a capital (its most populous province)
//! and a starting army there, sized by E0 А11 and kept within the capital's supply (E0 А8).
//!
//! **Houses (E2 step 6a, plan §18).** Every polity gets a ruling house of 3–8 persons with
//! ages (the ruler 20–60, children and siblings around it), a rule of succession by lot, in
//! some polities a designated heir and a governor of a province, and 2–4 houses that do not
//! rule — the owner's rule that the house role must be able to take the throne needs other
//! houses to exist as facts of the scenario, and a polity's nobility must not be thinner
//! than its ruling family (owner on #285: with 0–2 the extinction of every house was a
//! chain of new dynasties, not an exception).
//!
//! Everything drawn comes from the bands of E0 §5 where E0 has a row; what E0 has no row for
//! is a model choice (kind (в)) and is said so at the constant. The fingerprint of
//! `generate(1)` is pinned: an update of `rand` that changed every world would break
//! "fails on seed 12" silently otherwise.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::agents::GoalKind;
use crate::params::Params;
use crate::scenario::{ActorData, AllianceData, AspirationData, DisplayCurrency, HouseData, PersonData, ProvinceData, RegionData, RelationsData, ScenarioData, VassalData};
use crate::verbs::VerbId;
use crate::world::{Disease, Institution, Land, Succession};

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
/// Standing army as a share of the polity's people: E0 А11, peacetime 0.5–1.5 %.
const ARMY_SHARE: (f64, f64) = (0.005, 0.015);
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
/// Sea cells a crossing may span, kind (в): coasts further apart are not linked.
const SEA_REACH: u32 = 4;
/// Polities, bounded by one per three land cells.
const STATES: (u64, u64) = (3, 15);
/// Maximum land neighbours a province may have.
const MAX_DEGREE: usize = 7;
/// The ruler's age at the start, kind (в): an adult within the reigns of E0 В5.
const RULER_AGE: (u64, u64) = (20, 60);
/// Members of the ruling house, kind (в); E0 Х3 gives the size only through В2 and В4.
const RULING_HOUSE_SIZE: (u64, u64) = (3, 8);
/// Members of a house that does not rule, kind (в).
const OTHER_HOUSE_SIZE: (u64, u64) = (2, 5);
/// Houses that do not rule, per polity, kind (в).
const OTHER_HOUSES: (u64, u64) = (2, 4);
/// The chance a province that is not the capital has a governor, and the chance the seat
/// goes to a house that does not rule (E2 step 9, owner's rule on #289: the secession of a
/// house must be reachable — the Morea under a Palaiologos, Genoa under the Adorno), kind
/// (в), checked through В8 — the number of polities a century on.
const GOVERNOR_CHANCE: f64 = 0.3;
const GOVERNOR_FROM_OTHER_HOUSE: f64 = 0.7;
/// Provinces a region holds at most (E3): a kingdom's worth on a map of tens of provinces,
/// kind (в); the target of «unite» is a region, never a people (owner's rule on §23).
const REGION_SIZE: usize = 6;
/// Tongues and faiths per world (E2 step 8): each state draws one of each; a province that
/// touches another state takes that state's with `BORDER_FAITH_CHANCE` — the mixed march
/// where conversion and friction have something to work on. Houses take the capital's,
/// save a newcomer house now and then (`HOUSE_FAITH_CHANCE` of keeping the capital's).
const CULTURES: (u64, u64) = (2, 4);
const RELIGIONS: (u64, u64) = (2, 3);
const BORDER_FAITH_CHANCE: f64 = 0.3;
const HOUSE_FAITH_CHANCE: f64 = 0.8;
/// The chance a state starts with the conversion policy (E0: ascribed by the facts; here
/// by lot, so that both edges of Р2 — Iran and Crete — exist on the generated worlds).
const POLICY_CHANCE: f64 = 0.3;
/// Starting alliances per world (E2 step 7), between states whose land touches; the share
/// of states bound at the start is E0 Д3 — in E2 the row measures the generator, since the
/// starting treaties expire and nobody makes new ones until the agents of E3 (owner's rule
/// on §19).
const ALLIANCES: (u64, u64) = (1, 3);
/// The chance that a state at least twice the size of a touching neighbour (in provinces)
/// starts as its lord — one vassal a lord, a vassal bound by nothing else.
const VASSAL_CHANCE: f64 = 0.5;

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

/// A map under construction: cells on a grid, some of them sea, with explicit graphs.
struct Map {
    width: u32,
    height: u32,
    /// `None` is sea.
    cells: Vec<Option<Cell>>,
    /// Land graph, symmetric, by cell index.
    links: BTreeMap<u32, BTreeSet<u32>>,
    /// Sea graph, symmetric, by cell index, coastal cells only.
    sea_links: BTreeMap<u32, BTreeSet<u32>>,
}

impl Map {
    fn new(width: u32, height: u32, cells: Vec<Option<Cell>>) -> Map {
        Map { width, height, cells, links: BTreeMap::new(), sea_links: BTreeMap::new() }
    }

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

    fn sea_link(&mut self, a: u32, b: u32) {
        if a != b {
            self.sea_links.entry(a).or_default().insert(b);
            self.sea_links.entry(b).or_default().insert(a);
        }
    }

    fn degree(&self, i: u32) -> usize {
        self.links.get(&i).map_or(0, BTreeSet::len)
    }

    fn neighbours(&self, i: u32) -> Vec<u32> {
        self.links.get(&i).into_iter().flatten().copied().collect()
    }

    /// Orthogonal grid neighbours of any cell, land or sea.
    fn around(&self, i: u32) -> Vec<u32> {
        let (x, y) = self.xy(i);
        let mut out = Vec::new();
        if x > 0 {
            out.push(i - 1);
        }
        if x + 1 < self.width {
            out.push(i + 1);
        }
        if y > 0 {
            out.push(i - self.width);
        }
        if y + 1 < self.height {
            out.push(i + self.width);
        }
        out
    }

    /// Orthogonal links between land cells.
    fn build_orthogonal_links(&mut self) {
        self.links.clear();
        for i in 0..self.cells.len() as u32 {
            if !self.is_land(i) {
                continue;
            }
            for j in self.around(i) {
                if j > i && self.is_land(j) {
                    self.link(i, j);
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

    /// Sea links: from every coastal cell, breadth-first over sea cells up to `SEA_REACH`;
    /// every land cell touching a reached sea cell is a coast faced across the water.
    fn build_sea_links(&mut self) {
        self.sea_links.clear();
        let land_cells: Vec<u32> = (0..self.cells.len() as u32).filter(|i| self.is_land(*i)).collect();
        for c in land_cells {
            let mut depth: BTreeMap<u32, u32> = BTreeMap::new();
            let mut queue = VecDeque::new();
            for s in self.around(c) {
                if !self.is_land(s) {
                    depth.insert(s, 1);
                    queue.push_back(s);
                }
            }
            while let Some(s) = queue.pop_front() {
                let d = depth[&s];
                for t in self.around(s) {
                    if self.is_land(t) {
                        self.sea_link(c, t);
                    } else if d < SEA_REACH && !depth.contains_key(&t) {
                        depth.insert(t, d + 1);
                        queue.push_back(t);
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

    /// Every landmass beyond the main one gets a sea link to the main one if the reach gave
    /// it none: an island nobody can sail to is not a shape worth testing.
    fn guarantee_sea_links(&mut self, comps: &[Vec<u32>]) {
        let main: BTreeSet<u32> = comps[0].iter().copied().collect();
        for comp in &comps[1..] {
            let linked = comp.iter().any(|c| self.sea_links.get(c).into_iter().flatten().any(|t| main.contains(t)));
            if linked {
                continue;
            }
            // Nearest pair by grid distance, ties by lower ids.
            let mut best: Option<(u32, u32, u32)> = None;
            for c in comp {
                let (cx, cy) = self.xy(*c);
                for m in &main {
                    let (mx, my) = self.xy(*m);
                    let d = cx.abs_diff(mx) + cy.abs_diff(my);
                    if best.is_none_or(|(bd, bc, bm)| (d, *c, *m) < (bd, bc, bm)) {
                        best = Some((d, *c, *m));
                    }
                }
            }
            if let Some((_, c, m)) = best {
                self.cell_mut(c).coastal = true;
                self.cell_mut(m).coastal = true;
                self.sea_link(c, m);
            }
        }
    }

    /// Mark the coast: a land cell next to sea or to the map's edge, or with a sea link.
    fn mark_coast(&mut self) {
        for i in 0..self.cells.len() as u32 {
            if !self.is_land(i) {
                continue;
            }
            let (x, y) = self.xy(i);
            let edge = x == 0 || y == 0 || x + 1 == self.width || y + 1 == self.height;
            let sea_next = self.around(i).iter().any(|j| !self.is_land(*j));
            let linked = self.sea_links.get(&i).is_some_and(|s| !s.is_empty());
            self.cell_mut(i).coastal = edge || sea_next || linked;
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
    /// never starts above capacity. The capital is the most populous province (ties: lowest
    /// id); the army is E0 А11's share of the polity's people, kept within the capital's
    /// supply (E0 А8). Province ids are cell indices, so the map stays legible.
    fn into_scenario(self, id: String, states: usize, diseases: Vec<Disease>, bank_for: &dyn Fn(usize) -> bool, bonds: bool, params: &Params) -> ScenarioData {
        let pid = |i: u32| format!("p{i:03}");
        let sid = |a: usize| format!("s{a:02}");
        // Tongues and faiths (E2 step 8): a draw of its own, so the earlier draws stay.
        let mut faith_rng = ChaCha8Rng::seed_from_u64(fnv(id.as_bytes()) ^ 0xfa17_u64);
        let (n_cultures, n_religions) = if bonds { (pick(&mut faith_rng, CULTURES) as usize, pick(&mut faith_rng, RELIGIONS) as usize) } else { (1, 1) };
        // The first states take each tongue and faith once, so every one drawn exists in the
        // world; the rest draw by lot.
        let state_culture: Vec<usize> = (0..states).map(|s| if s < n_cultures { s } else { faith_rng.gen_range(0..n_cultures) }).collect();
        let state_religion: Vec<usize> = (0..states).map(|s| if s < n_religions { s } else { faith_rng.gen_range(0..n_religions) }).collect();
        let tongue = |c: usize| format!("tongue{c}");
        let faith = |r: usize| format!("faith{r}");
        let mut provinces = BTreeMap::new();
        let mut pop_by_state: BTreeMap<usize, u64> = BTreeMap::new();
        let mut capital: BTreeMap<usize, (u64, u32)> = BTreeMap::new();
        for (i, cell) in self.cells.iter().enumerate() {
            let Some(cell) = cell else { continue };
            let i = i as u32;
            let capacity = params.capacity(cell.area_km2, cell.land);
            let population = ((capacity as f64) * cell.start_share).floor() as u64;
            *pop_by_state.entry(cell.owner).or_insert(0) += population;
            let best = capital.entry(cell.owner).or_insert((population, i));
            if population > best.0 {
                *best = (population, i);
            }
            let raw_neighbours = self.neighbours(i);
            let neighbours = raw_neighbours.iter().copied().map(pid).collect();
            let sea_neighbours = self.sea_links.get(&i).into_iter().flatten().map(|j| pid(*j)).collect();
            // A border province may speak and pray as the state across the border does.
            let (mut culture, mut religion) = (state_culture[cell.owner], state_religion[cell.owner]);
            let across: Vec<usize> = raw_neighbours.iter().filter_map(|j| self.cells[*j as usize].as_ref()).map(|c| c.owner).filter(|o| *o != cell.owner).collect();
            if let Some(o) = across.first() {
                if bonds && faith_rng.gen_bool(BORDER_FAITH_CHANCE) {
                    culture = state_culture[*o];
                }
                if bonds && faith_rng.gen_bool(BORDER_FAITH_CHANCE) {
                    religion = state_religion[*o];
                }
            }
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
                    sea_neighbours,
                    governor: None,
                    culture: tongue(culture),
                    religion: faith(religion),
                },
            );
        }
        let mut rng = ChaCha8Rng::seed_from_u64(fnv(id.as_bytes()) ^ 0x7ea5_u64);
        let mut actors = BTreeMap::new();
        let mut houses = BTreeMap::new();
        let mut persons = BTreeMap::new();
        let start_year = 1000;
        for s in 0..states {
            let pop = pop_by_state.get(&s).copied().unwrap_or(0);
            let kg = pop as f64 * pick_f(&mut rng, REVENUE_PER_HEAD_KG) * pick_f(&mut rng, TREASURY_YEARS);
            let (_, cap_i) = capital.get(&s).copied().unwrap_or((0, 0));
            // The army the budget keeps paid at the starting tax rate (economy.rs), per year:
            // people × output × tax × (1 − court) ÷ pay per soldier.
            let yearly_income = pop as i64 * params.output_per_head_kg * params.tax_default_ppm / 1_000_000;
            let for_army = yearly_income * (1_000_000 - params.court_share_ppm) / 1_000_000;
            let budget_limit = if params.pay_per_soldier_kg_per_year == 0 { 0 } else { for_army / params.pay_per_soldier_kg_per_year };
            let wanted = (pop as f64 * pick_f(&mut rng, ARMY_SHARE)).floor() as i64;
            let soldiers = wanted.min(budget_limit * 9 / 10).max(0) as u64;
            // The capital prays and speaks as the state does, whatever its border drew: the
            // ruling house takes the capital's faith and the court is the state's.
            if let Some(cap) = provinces.get_mut(&pid(cap_i)) {
                cap.culture = tongue(state_culture[s]);
                cap.religion = faith(state_religion[s]);
            }
            let mut institutions = if bank_for(s) { vec![Institution::Bank] } else { Vec::new() };
            if bonds && faith_rng.gen_bool(POLICY_CHANCE) {
                institutions.push(Institution::ConversionPolicy);
            }
            // The ruling house and the houses beside it. The houses' own men (E0 Х2) are
            // carved out of the army of А11, not added to it: the ruling house's retinue by
            // the row, the others' by the (в) share.
            let n_other = pick(&mut rng, OTHER_HOUSES) as usize;
            let ruling_retinue = soldiers * params.retinue_share_ppm as u64 / 1_000_000;
            let other_retinue = soldiers * params.other_retinue_share_ppm as u64 / 1_000_000;
            let own_soldiers = soldiers - ruling_retinue - other_retinue * n_other as u64;
            let mut adults_by_house: Vec<(String, Vec<String>)> = Vec::new();
            let mut ruler = String::new();
            let mut heir = None;
            for h in 0..=n_other {
                let hid = format!("h{s:02}{}", (b'a' + h as u8) as char);
                let size = pick(&mut rng, if h == 0 { RULING_HOUSE_SIZE } else { OTHER_HOUSE_SIZE }) as usize;
                let seat_income = yearly_income as f64 * params.seat_income_share_ppm as f64 / 1_000_000.0;
                let treasury_kg = if h == 0 { (seat_income * pick_f(&mut rng, (0.0, 1.0))).floor() as i64 } else { 0 };
                let (house_culture, house_religion) = if h == 0 || !bonds || faith_rng.gen_bool(HOUSE_FAITH_CHANCE) {
                    (state_culture[s], state_religion[s])
                } else {
                    (faith_rng.gen_range(0..n_cultures), faith_rng.gen_range(0..n_religions))
                };
                let retinue = if h == 0 { ruling_retinue } else { other_retinue };
                houses.insert(hid.clone(), HouseData { name: format!("House {s}{}", (b'A' + h as u8) as char), polity: sid(s), treasury_kg, culture: tongue(house_culture), religion: faith(house_religion), soldiers: retinue });
                let head_age = pick(&mut rng, RULER_AGE) as i32;
                let head = format!("{hid}p00");
                persons.insert(head.clone(), PersonData { name: format!("{hid} head"), house: hid.clone(), born: start_year - head_age, parent: None, spouse: None });
                let mut adults = vec![head.clone()];
                let mut children: Vec<(String, i32)> = Vec::new();
                for k in 1..size {
                    let key = format!("{hid}p{k:02}");
                    // A child of the head where the head's age allows, else a sibling.
                    let (born, parent) = if head_age > 17 && rng.gen_bool(0.7) {
                        (start_year - rng.gen_range(0..=(head_age - 17).min(44)), Some(head.clone()))
                    } else {
                        (start_year - rng.gen_range(RULER_AGE.0 as i32..=RULER_AGE.1 as i32), None)
                    };
                    persons.insert(key.clone(), PersonData { name: format!("{hid} {k}"), house: hid.clone(), born, parent: parent.clone(), spouse: None });
                    if start_year - born >= params.majority_age {
                        adults.push(key.clone());
                    }
                    if parent.is_some() {
                        children.push((key, born));
                    }
                }
                if h == 0 {
                    ruler = head.clone();
                    // Sometimes the eldest child is the designated heir (a fact, as Gian
                    // Galeazzo under Galeazzo Maria).
                    if rng.gen_bool(0.3) {
                        heir = children.iter().min_by_key(|(_, born)| *born).map(|(k, _)| k.clone());
                    }
                }
                adults_by_house.push((hid, adults));
            }
            let succession = match rng.gen_range(0..4) {
                0 => Succession::Blood,
                1 => Succession::Acclamation,
                2 => Succession::Election,
                _ => Succession::Mixed,
            };
            actors.insert(sid(s), ActorData { name: format!("State {s}"), treasury_kg: kg.floor() as i64, capital: pid(cap_i), soldiers: own_soldiers, institutions, ruler: ruler.clone(), heir, regent: None, succession, aspirations: Vec::new() });
            // Governors of the provinces that are not the capital (owner's rule on #289:
            // the secession of a house must be reachable, so the houses that do not rule
            // hold seats): each province with `GOVERNOR_CHANCE`, the seat to a free adult
            // of a house that does not rule with `GOVERNOR_FROM_OTHER_HOUSE`, else of the
            // ruling house; one seat per person.
            let mut own: Vec<String> = provinces.iter().filter(|(_, p)| p.owner == sid(s) && *p != &provinces[&pid(cap_i)]).map(|(k, _)| k.clone()).collect();
            own.sort();
            let mut free_ruling: Vec<String> = adults_by_house[0].1.iter().filter(|p| **p != ruler).cloned().collect();
            let mut free_other: Vec<String> = adults_by_house[1..].iter().flat_map(|(_, a)| a.iter().cloned()).collect();
            for p in own {
                if !rng.gen_bool(GOVERNOR_CHANCE) {
                    continue;
                }
                let pool = if !free_other.is_empty() && (free_ruling.is_empty() || rng.gen_bool(GOVERNOR_FROM_OTHER_HOUSE)) { &mut free_other } else { &mut free_ruling };
                if pool.is_empty() {
                    continue;
                }
                let g = pool.remove(rng.gen_range(0..pool.len()));
                provinces.get_mut(&p).unwrap().governor = Some(g);
            }
        }
        // Starting relations (E2 step 7): alliances between touching states and a vassal
        // of a large state, by lot; no wars (the tests open them, decision on #284). The
        // hand-made extreme worlds are sketches of geography and start with no bond.
        let state_of = |owner: &str| -> usize { owner[1..].parse().unwrap_or(0) };
        let mut touching: std::collections::BTreeSet<(usize, usize)> = std::collections::BTreeSet::new();
        let mut count: BTreeMap<usize, usize> = BTreeMap::new();
        for p in provinces.values() {
            let s = state_of(&p.owner);
            *count.entry(s).or_insert(0) += 1;
            for n in p.neighbours.iter().chain(p.sea_neighbours.iter()) {
                let t = state_of(&provinces[n].owner);
                if s != t {
                    touching.insert((s.min(t), s.max(t)));
                }
            }
        }
        let touching: Vec<(usize, usize)> = touching.into_iter().collect();
        let mut bound: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        let mut alliances = Vec::new();
        let mut vassals = Vec::new();
        let mut candidates = touching.clone();
        let term = (params.treaty_term_years_band[0] as u64, params.treaty_term_years_band[1] as u64);
        for _ in 0..if bonds { pick(&mut rng, ALLIANCES) } else { 0 } {
            if candidates.is_empty() {
                break;
            }
            let (a, b) = candidates.remove(rng.gen_range(0..candidates.len()));
            if bound.contains(&a) || bound.contains(&b) {
                continue;
            }
            bound.insert(a);
            bound.insert(b);
            alliances.push(AllianceData { a: sid(a), b: sid(b), until_year: start_year + pick(&mut rng, term) as i32 });
        }
        let mut lords: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        for (a, b) in &touching {
            let (big, small) = if count[a] >= count[b] { (*a, *b) } else { (*b, *a) };
            if bonds && count[&big] >= 2 * count[&small] && !bound.contains(&small) && !lords.contains(&small) && !bound.contains(&big) && !lords.contains(&big) && rng.gen_bool(VASSAL_CHANCE) {
                bound.insert(small);
                lords.insert(big);
                vassals.push(VassalData { vassal: sid(small), lord: sid(big) });
            }
        }
        // Regions (E3): a kingdom's worth of provinces — each landmass cut into blocks of
        // up to `REGION_SIZE` by breadth-first walk from its lowest id; every province in
        // exactly one region.
        let mut regions: BTreeMap<String, RegionData> = BTreeMap::new();
        {
            let mut seen: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
            let mut k = 0usize;
            let mut keys: Vec<&String> = provinces.keys().collect();
            keys.sort();
            for start in keys {
                if seen.contains(start) {
                    continue;
                }
                let mut block: Vec<String> = Vec::new();
                let mut queue = std::collections::VecDeque::from([start]);
                seen.insert(start);
                while let Some(c) = queue.pop_front() {
                    block.push(c.clone());
                    if block.len() >= REGION_SIZE {
                        break;
                    }
                    for n in &provinces[c].neighbours {
                        if seen.insert(n) {
                            queue.push_back(n);
                        }
                    }
                }
                // Whatever was queued but not taken is free again for the next block.
                for n in queue {
                    seen.remove(n);
                }
                regions.insert(format!("r{k:02}"), RegionData { name: format!("Region {k}"), provinces: block });
                k += 1;
            }
        }
        // Starting aspirations (E3, owner's answer 3 on §23): a weighted lot over the
        // reachable kinds — hold the largest province, an alliance against the largest
        // neighbour, take the largest bordering province, subdue the smallest neighbour,
        // unite the capital's region, the union with the faith of the largest neighbour whose
        // court prays otherwise (owner's rule on #293); a vassal wants to be free. Two per
        // polity, distinct.
        if bonds {
            let mut asp_rng = ChaCha8Rng::seed_from_u64(fnv(id.as_bytes()) ^ 0xa5b1_u64);
            let region_of = |p: &str| regions.iter().find(|(_, r)| r.provinces.iter().any(|q| q == p)).map(|(k, _)| k.clone());
            for s in 0..states {
                let me = sid(s);
                let mine: Vec<(&String, &ProvinceData)> = provinces.iter().filter(|(_, p)| p.owner == me).collect();
                let neighbours: Vec<(String, u64)> = {
                    let mut by: BTreeMap<String, u64> = BTreeMap::new();
                    for (_, p) in &mine {
                        for n in p.neighbours.iter().chain(p.sea_neighbours.iter()) {
                            let o = &provinces[n].owner;
                            if *o != me {
                                *by.entry(o.clone()).or_insert(0) += provinces[n].population;
                            }
                        }
                    }
                    by.into_iter().collect()
                };
                let mut goals: Vec<AspirationData> = Vec::new();
                if vassals.iter().any(|v| v.vassal == me) {
                    goals.push(AspirationData { kind: GoalKind::Free, target: None, route: None });
                }
                let total: f64 = params.aspiration_weights.iter().map(|(_, w)| w).sum();
                let mut tried = 0;
                while goals.len() < 2 && tried < 8 {
                    tried += 1;
                    let mut x = asp_rng.gen_range(0.0..total);
                    let mut kind = GoalKind::Hold;
                    for (k, w) in params.aspiration_weights {
                        if x < w {
                            kind = k;
                            break;
                        }
                        x -= w;
                    }
                    if goals.iter().any(|g| g.kind == kind) {
                        continue;
                    }
                    let largest_own = mine.iter().max_by_key(|(k, p)| (p.population, std::cmp::Reverse((*k).clone()))).map(|(k, _)| (*k).clone());
                    let largest_neighbour = neighbours.iter().max_by_key(|(k, pop)| (*pop, std::cmp::Reverse(k.clone()))).map(|(k, _)| k.clone());
                    let smallest_neighbour = neighbours.iter().min_by_key(|(k, pop)| (*pop, k.clone())).map(|(k, _)| k.clone());
                    let bordering_province = mine
                        .iter()
                        .flat_map(|(_, p)| p.neighbours.iter().chain(p.sea_neighbours.iter()))
                        .filter(|n| provinces[*n].owner != me)
                        .max_by_key(|n| (provinces[*n].population, std::cmp::Reverse((*n).clone())))
                        .cloned();
                    let goal = match kind {
                        GoalKind::Hold => largest_own.map(|t| AspirationData { kind, target: Some(t), route: None }),
                        GoalKind::AllianceAgainst => largest_neighbour.map(|t| AspirationData { kind, target: Some(t), route: None }),
                        GoalKind::Take => bordering_province.map(|t| AspirationData { kind, target: Some(t), route: None }),
                        GoalKind::Subdue => smallest_neighbour.map(|t| AspirationData { kind, target: Some(t), route: None }),
                        GoalKind::Unite => region_of(&pid(capital.get(&s).map(|c| c.1).unwrap_or(0))).map(|t| AspirationData { kind, target: Some(t), route: None }),
                        GoalKind::Union => neighbours
                            .iter()
                            .filter_map(|(k, pop)| k[1..].parse::<usize>().ok().filter(|o| state_religion[*o] != state_religion[s]).map(|o| (o, *pop, k.clone())))
                            .max_by_key(|(_, pop, k)| (*pop, std::cmp::Reverse(k.clone())))
                            .map(|(o, _, _)| AspirationData { kind, target: Some(faith(state_religion[o])), route: None }),
                        _ => None,
                    };
                    if let Some(g) = goal {
                        goals.push(g);
                    }
                }
                actors.get_mut(&me).unwrap().aspirations = goals;
            }
        }
        let verbs = VerbId::ALL.iter().map(|v| (v.key().to_string(), v.key().replace('_', " "))).collect();
        ScenarioData {
            id,
            start_year,
            diseases,
            display_currency: DisplayCurrency { name: "coin".to_string(), kg_per_unit: 100.0 },
            actors,
            provinces,
            houses,
            persons,
            relations: Some(RelationsData { wars: Vec::new(), alliances, vassals }),
            regions: Some(regions),
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
    let mut map = Map::new(width, height, (0..n).map(|_| Some(fresh_cell(&mut rng))).collect());

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
    map.build_sea_links();
    let comps = map.components();
    map.guarantee_sea_links(&comps);
    map.mark_coast();

    // Polities: grow blobs over the largest landmass; smaller landmasses (islands) go either
    // to a new polity of their own or overseas to an existing one.
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
            // Overseas holding of an existing polity: the power whose coast faces it across
            // the water (so the colony is reachable by the rules), or any polity if none does.
            // Only coasts already assigned count (seed 56: a facing island not yet handed
            // out would hand down its unset owner).
            let mut facing: Vec<usize> = comp
                .iter()
                .flat_map(|c| map.sea_links.get(c).into_iter().flatten().copied().collect::<Vec<_>>())
                .filter(|t| !comp.contains(t))
                .map(|t| map.owner(t))
                .filter(|o| *o != usize::MAX)
                .collect();
            facing.sort();
            facing.dedup();
            let owner = if facing.is_empty() { rng.gen_range(0..states) } else { facing[rng.gen_range(0..facing.len())] };
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
    // Plague in the world's pool of diseases: a fact drawn by seed, half the worlds have it.
    let diseases = if rng.gen_bool(0.5) { vec![Disease::Plague] } else { Vec::new() };
    // Institutions: a bank for some polities and not others (E0 §4, constraint 4).
    let banks: Vec<bool> = (0..states).map(|_| rng.gen_bool(0.5)).collect();
    map.into_scenario(format!("gen{seed}"), states, diseases, &|s| banks.get(s).copied().unwrap_or(false), true, params)
}

/// A hand-made map from an ASCII sketch: `.` is sea, a letter is the owning polity
/// (`a` = 0); orthogonal land links, sea links by reach. For the extreme worlds.
fn sketch(id: &str, rows: &[&str], land_of: impl Fn(usize) -> Land, area: u64, share: f64, bank_for: &dyn Fn(usize) -> bool, params: &Params) -> ScenarioData {
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
    let mut map = Map::new(width, height, cells);
    map.build_orthogonal_links();
    map.build_sea_links();
    let comps = map.components();
    map.guarantee_sea_links(&comps);
    map.mark_coast();
    map.into_scenario(id.to_string(), states, Vec::new(), bank_for, false, params)
}

/// The hand-made extreme worlds (owner, plan §12 item 11): each with its name.
pub fn extreme_worlds(params: &Params) -> Vec<(&'static str, ScenarioData)> {
    let none = |_: usize| false;
    let all = |_: usize| true;
    vec![
        // One empire against small polities: the empire holds everything but the corners.
        ("empire_vs_small", sketch("x_empire_vs_small", &["baaaac", "aaaaaa", "aaaaaa", "aaaaaa", "aaaaaa", "daaaae"], |_| Land::Arable, 8_000, 0.8, &none, params)),
        // An isolated island: a mainland of three polities and an island with no land link.
        ("isolated_island", sketch("x_isolated_island", &["aabb..", "aabb..", "cccc..", "cccc.d"], |o| if o == 3 { Land::Hills } else { Land::Arable }, 6_000, 0.7, &none, params)),
        // A polity without arable land: polity `a` on the top rows is all hills.
        ("no_arable", sketch("x_no_arable", &["aaaaa", "aaaaa", "bcbcb", "bcbcb", "bcbcb"], |o| if o == 0 { Land::Hills } else { Land::Arable }, 7_000, 0.8, &none, params)),
        // Nomads at the edge: three settled polities west, a steppe polity on the two eastern columns.
        ("nomads_at_edge", sketch("x_nomads_at_edge", &["aaaadd", "aaaadd", "bbbbdd", "bbbbdd", "ccccdd", "ccccdd"], |o| if o == 3 { Land::Steppe } else { Land::Arable }, 9_000, 0.75, &none, params)),
        // A chokepoint: two halves joined by a single passage, and a peninsula on the east.
        ("chokepoint", sketch("x_chokepoint", &["aaa.bbb", "aaa.bbb", "aaaabbb", "aaa.bbb", "aaa.b..", "....b..", "....b.."], |_| Land::Arable, 6_000, 0.8, &none, params)),
        // An overseas colony and an enclave: `b` holds a coastal strip (its capital), an
        // island across the bay and a cell inside `a`.
        ("colony_and_enclave", sketch("x_colony_and_enclave", &["bb....", "aaaa.b", "abaa.b", "aaaa..", "cccc..", "cccc.."], |_| Land::Arable, 6_000, 0.8, &none, params)),
        // Institutions at both ends (E0 §4, constraint 4): nobody has a bank (the worlds above),
        // everybody has one.
        ("banks_everywhere", sketch("x_banks_everywhere", &["aaabbb", "aaabbb", "cccddd", "cccddd"], |_| Land::Arable, 7_000, 0.8, &all, params)),
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

    /// Pinned on the union-aspiration version of the generator (E3 (в), owner's rule on #293;
    /// before: the regions-and-aspirations version of E3 (а), the governors version of E2
    /// step 9, the houses version of step 6a, the sea-links version of step 2). If this changes, every seed
    /// plays a different world and "fails on seed N" stops reproducing: bump it only with a
    /// deliberate change of the generator, never with a dependency update.
    #[test]
    fn world_one_is_pinned() {
        let p = Params::load().unwrap();
        assert_eq!(fingerprint(&generate(1, &p)), PINNED_WORLD_ONE);
    }

    const PINNED_WORLD_ONE: u64 = 8_417_907_754_458_249_782;

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
                for nb in &pr.sea_neighbours {
                    assert!(pr.coastal && s.provinces[nb].coastal, "seed {seed}: sea link {k}–{nb} off the coast");
                    assert!(s.provinces[nb].sea_neighbours.contains(k), "seed {seed}: sea link {k}–{nb} not symmetric");
                }
            }
            for (a, actor) in &s.actors {
                assert!(s.provinces.values().any(|pr| &pr.owner == a), "seed {seed}: {a} owns nothing");
                assert_eq!(&s.provinces[&actor.capital].owner, a, "seed {seed}: capital of {a} not its own");
                let people: u64 = s.provinces.values().filter(|pr| &pr.owner == a).map(|pr| pr.population).sum();
                let retinues: u64 = s.houses.values().filter(|h| &h.polity == a).map(|h| h.soldiers).sum();
                assert!((actor.soldiers + retinues) as f64 <= people as f64 * ARMY_SHARE.1 + 1.0, "seed {seed}: {a} over the army band");
                let ruler = &s.persons[&actor.ruler];
                assert_eq!(&s.houses[&ruler.house].polity, a, "seed {seed}: the ruler of {a} is of another polity's house");
                let age = s.start_year - ruler.born;
                assert!((RULER_AGE.0 as i32..=RULER_AGE.1 as i32).contains(&age), "seed {seed}: ruler of {a} aged {age}");
                let members = s.persons.values().filter(|p| p.house == ruler.house).count();
                assert!((RULING_HOUSE_SIZE.0 as usize..=RULING_HOUSE_SIZE.1 as usize).contains(&members), "seed {seed}: ruling house of {a} has {members}");
                let houses = s.houses.values().filter(|h| &h.polity == a).count();
                assert!((1 + OTHER_HOUSES.0 as usize..=1 + OTHER_HOUSES.1 as usize).contains(&houses), "seed {seed}: {a} has {houses} houses");
                if let Some(h) = &actor.heir {
                    assert_eq!(s.persons[h].parent.as_deref(), Some(actor.ruler.as_str()), "seed {seed}: the heir of {a} is not the ruler's child");
                }
            }
            for (k, p) in &s.persons {
                if let Some(parent) = &p.parent {
                    assert!(s.persons[parent].born < p.born, "seed {seed}: {k} is not younger than its parent");
                }
                assert!(p.born <= s.start_year, "seed {seed}: {k} born in the future");
            }
            for (k, pr) in &s.provinces {
                if let Some(g) = &pr.governor {
                    assert_eq!(s.houses[&s.persons[g].house].polity, pr.owner, "seed {seed}: governor of {k} from another polity");
                    assert_ne!(&s.actors[&pr.owner].ruler, g, "seed {seed}: the ruler governs {k} as well");
                }
            }
        }
        assert!(max_states >= 12, "largest world has only {max_states} states");
        assert!(max_provinces >= 70, "largest world has only {max_provinces} provinces");
        // The shapes succession must meet exist in the family: governors, designated heirs,
        // houses beside the ruling one, every rule.
        let all: Vec<ScenarioData> = (1..=50).map(|seed| generate(seed, &p)).collect();
        assert!(all.iter().any(|s| s.provinces.values().any(|pr| pr.governor.is_some())));
        assert!(all.iter().any(|s| s.actors.values().any(|a| a.heir.is_some())));
        assert!(all.iter().any(|s| s.houses.len() > s.actors.len()));
        for rule in [Succession::Blood, Succession::Acclamation, Succession::Election, Succession::Mixed] {
            assert!(all.iter().any(|s| s.actors.values().any(|a| a.succession == rule)), "{rule:?} never drawn");
        }
    }

    #[test]
    fn extreme_worlds_are_what_their_names_say() {
        let p = Params::load().unwrap();
        let worlds = extreme_worlds(&p);
        assert_eq!(worlds.len(), 7);
        assert!(worlds.iter().find(|(n, _)| *n == "banks_everywhere").unwrap().1.actors.values().all(|a| a.institutions == vec![Institution::Bank]));
        assert!(worlds.iter().filter(|(n, _)| *n != "banks_everywhere").all(|(_, s)| s.actors.values().all(|a| a.institutions.is_empty())));
        let get = |name: &str| worlds.iter().find(|(n, _)| *n == name).map(|(_, s)| s).unwrap();
        let empire = get("empire_vs_small");
        assert_eq!(empire.provinces.values().filter(|p| p.owner == "s00").count(), 32);
        let island = get("isolated_island");
        let d = island.provinces.values().find(|p| p.owner == "s03").unwrap();
        assert!(d.neighbours.is_empty() && d.coastal && !d.sea_neighbours.is_empty(), "the island is linked by sea");
        let no_arable = get("no_arable");
        assert!(no_arable.provinces.values().filter(|p| p.owner == "s00").all(|p| p.land != Land::Arable));
        let nomads = get("nomads_at_edge");
        assert!(nomads.provinces.values().filter(|p| p.owner == "s03").all(|p| p.land == Land::Steppe));
        let choke = get("chokepoint");
        assert_eq!(choke.provinces.len(), 31);
        let colony = get("colony_and_enclave");
        assert_eq!(colony.actors["s01"].capital, "p000", "the capital is the coastal strip, not the enclave");
        assert!(colony.provinces.values().any(|p| p.owner == "s01" && !p.neighbours.is_empty() && p.neighbours.iter().all(|n| colony.provinces[n].owner == "s00")));
    }
}
