//! The world state and the tick (plan §4, §5, §14).
//!
//! Everything is in ordered collections and derives `Serialize`/`Deserialize`; the save
//! format is this struct (see `save.rs`). Systems fill [`World::tick`] one by one (brief §8);
//! E2 step 2 brought the province graph with land and sea edges and the units on it
//! (`movement.rs`). The population flow is still the E1 stand-in (toward capacity) until
//! the population system arrives.
//!
//! Contract decisions of the owner's review of PR #278:
//! - a stock that would cross zero is **clamped** at zero, the actual amount is posted, a
//!   `BoundHit` event is emitted and the tick goes on (brief §2: a bound is a sentinel flag,
//!   not a crash) — [`World::post`] is the one write path the systems use;
//! - the world carries the **scenario id and a hash of the compiled facts**, so a save of
//!   one scenario cannot be loaded into another (AGENTS §8);
//! - **names live in the world** and travel with the save: actors born during the game
//!   (heirs, E2) get a name here, not in the scenario.
//!
//! E2 step 6a (plan §18): **people, houses and seats.** A person has a house, a year of
//! birth and a parent; a house belongs to a polity and has a treasury; the throne of a
//! polity and the governorship of a province are seats a person holds. The rule of
//! succession is a fact of the polity (an institution by form, E0 §4). What happens to
//! them is `politics.rs`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::event::{Event, Stamped};
use crate::ids::{ActorId, HouseId, PersonId, ProvinceId, UnitId};
use crate::ledger::{Account, Ledger, LedgerError, Posting, Source};
use crate::params::Params;
use crate::rng::{self, Draw, DrawPoint, RngDraw};
use crate::time::Tick;

/// A disease the world may hold (brief §0.2: a fact of the scenario, like culture). Plague
/// strikes provinces by E0 П4 where it exists; between 270 and 541 it did not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Disease {
    Plague,
}

/// An institution a polity may hold: the engine's closed list (E0 §4, variant 3). Each
/// comes with its source and the rows it changes. `Bank` (E0 Э6, Э7): a polity with a bank
/// may borrow, down to the debt ceiling, at interest; without one its treasury never goes
/// below zero. Next: the succession rule (internal politics), gunpowder (army).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Institution {
    Bank,
}

/// The rule by which a polity recognises its next ruler (E0 §4, the second institution of
/// the closed list; sources В3/В6: Eisner 2011, Omissi 2018). Exactly one per polity, so a
/// field of its own rather than an element of `institutions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Succession {
    /// A dynasty: the line by seniority, no rite of acclamation.
    Blood,
    /// The army, the senate or an assembly proclaims the next ruler: the eldest adult of
    /// the house who can lead; a child only when no adult is left.
    Acclamation,
    /// A doge, a pope: chosen between houses. **E2 distortion (owner, §18):** until the
    /// agents of E3 vote, the choice falls on the eldest adult of the ruling house and only
    /// the legitimacy norm differs — Venice plays as a dynasty.
    Election,
    /// Blood recognised through acclamation (Rome 375: Valentinian II proclaimed by the
    /// army within days): the line by seniority, the norm between the two.
    Mixed,
}

/// How the ruler came to the throne: the standing the legitimacy norm is keyed by (E0
/// §1.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Standing {
    /// By blood, under a rule of blood.
    Hereditary,
    /// By blood, recognised by acclamation (the mixed rule): the norm between the two.
    Confirmed,
    /// Chosen under a rule of election.
    Elected,
    /// Proclaimed: by rule, or from another house when the ruling house died out.
    Acclaimed,
    /// Took the throne by force (E0 В6, E2 step 6b): the lowest norm.
    Usurper,
}

/// A person: a member of a house, alive or dead (the record stays: no dangling ids).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub house: HouseId,
    /// Year of birth; the age is derived.
    pub born: i32,
    /// The parent, for the line of blood; none for the founders of the starting facts.
    pub parent: Option<PersonId>,
    /// The tick of death, once dead.
    pub died: Option<Tick>,
}

/// A house: a family inside a polity, holding seats and a treasury of its own. Its
/// members are the persons that name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct House {
    /// The polity the house belongs to (owner's rule on §18: a house must be able to take
    /// the throne of its polity, so it must say which one).
    pub polity: ActorId,
}

/// Land of a province: the closed list the capacity density (E0 П1) is keyed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Land {
    Arable,
    Hills,
    Steppe,
    Desert,
}

/// A polity as the engine sees it: its provinces, its capital and a legitimacy rating.
///
/// `legitimacy` is the one `f64` of E1; it exists so that save/load exercises the float
/// round trip the old engine needed `float_roundtrip` for (B31).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub provinces: Vec<ProvinceId>,
    pub capital: ProvinceId,
    pub legitimacy: f64,
    /// The polity's institutions (facts of the scenario).
    pub institutions: std::collections::BTreeSet<Institution>,
    /// Tax as parts per million of production; starts at `tax.default_rate` (E0 Э4). The
    /// tax verbs move it by `tax.step` inside the band of Э4 for a price in legitimacy
    /// (`politics.rs`, E2 step 6b).
    pub tax_rate_ppm: i64,
    /// Share of the army's pay met this tick, parts per million: the share of the army that
    /// is supplied at home (`movement.rs`).
    pub paid_share_ppm: i64,
    /// The person on the throne. A polity whose ruler is dead and unreplaced has fallen.
    pub ruler: PersonId,
    /// The designated heir, if any (a fact; the rule gives one otherwise).
    pub heir: Option<PersonId>,
    /// The regent while the ruler is a minor (E0 В1).
    pub regent: Option<PersonId>,
    /// The rule of succession (E0 §4).
    pub succession: Succession,
    /// How the ruler came to the throne: keys the legitimacy norm.
    pub standing: Standing,
    /// After a default the bank lends nothing until this tick (E2 step 6b).
    pub credit_closed_until: Option<Tick>,
}

/// A province as the engine sees it: owner, land, the capacity the engine derived, and its
/// edges — land neighbours and sea neighbours, both facts of geography.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Province {
    pub owner: ActorId,
    pub area_km2: u64,
    pub land: Land,
    /// Touches the sea: a fact of geography (a port is an institution, later).
    pub coastal: bool,
    /// People the land can carry: `area_km2 × density(land)` from `params.toml` (E0 П1).
    /// Derived, never authored (brief §0.1: an authored capacity tunes growth per province).
    pub capacity: u64,
    /// Land neighbours, sorted, no duplicates.
    pub neighbours: Vec<ProvinceId>,
    /// Coasts this coast faces across the water, sorted, no duplicates; only on coastal
    /// provinces. The link that makes an island or a colony reachable.
    pub sea_neighbours: Vec<ProvinceId>,
    /// The siege on it, if an enemy stands here unopposed (`army.rs`).
    pub siege: Option<Siege>,
    /// The governor's seat, if a person of the owner's polity holds it (the despot of the
    /// Morea, the Sforza fief). The seat passes within the holder's house.
    pub governor: Option<PersonId>,
}

/// An army on the graph: owner, position, quality, and where it is ordered to go. Its
/// soldiers are the ledger's `Account::Soldiers(id)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub owner: ActorId,
    pub position: ProvinceId,
    /// Quality rating, 0..100: a multiplier of strength in battle. Its dynamics (experience,
    /// idleness) are the army's line for E3 (plan §17).
    pub quality: f64,
    pub destination: Option<ProvinceId>,
    /// Hired men (E0 А4): they came from outside the world and leave it when disbanded;
    /// a levy goes back to the people.
    pub mercenary: bool,
}

/// A siege in progress: who besieges and for how many consecutive ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Siege {
    pub by: ActorId,
    pub ticks: u32,
}

/// The names the presentation layer shows, indexed by id. Part of the world and of the
/// save: an actor born in the game is named here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Names {
    /// Index = `ActorId`: `(scenario id, display name)`.
    pub actors: Vec<(String, String)>,
    /// Index = `ProvinceId`.
    pub provinces: Vec<(String, String)>,
    /// Index = `HouseId`.
    pub houses: Vec<(String, String)>,
    /// Index = `PersonId`; a person born in play is named here.
    pub persons: Vec<(String, String)>,
}

/// One province of the starting facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvinceFact {
    pub area_km2: u64,
    pub land: Land,
    pub coastal: bool,
    pub population: u64,
    pub owner: ActorId,
    pub neighbours: Vec<ProvinceId>,
    pub sea_neighbours: Vec<ProvinceId>,
    pub governor: Option<PersonId>,
}

/// One polity of the starting facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActorFact {
    /// Starting treasury in kg of wheat-equivalent.
    pub treasury_kg: i64,
    pub capital: ProvinceId,
    /// Starting army, stationed at the capital as one unit.
    pub soldiers: u64,
    /// The polity's institutions, from the engine's closed list.
    pub institutions: Vec<Institution>,
    pub ruler: PersonId,
    pub heir: Option<PersonId>,
    pub regent: Option<PersonId>,
    pub succession: Succession,
}

/// One house of the starting facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HouseFact {
    pub polity: ActorId,
    /// Starting treasury of the house in kg of wheat-equivalent.
    pub treasury_kg: i64,
}

/// One person of the starting facts; all of them alive at the start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonFact {
    pub house: HouseId,
    pub born: i32,
    pub parent: Option<PersonId>,
}

/// The starting facts the world is built from — the compiled scenario (`scenario.rs`).
///
/// Only facts: counts in world units, geography, ownership. No rule numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Facts {
    pub scenario_id: String,
    pub start_year: i32,
    /// The diseases this world holds.
    pub diseases: Vec<Disease>,
    /// Index = `ProvinceId`.
    pub provinces: Vec<ProvinceFact>,
    /// Index = `ActorId`.
    pub actors: Vec<ActorFact>,
    /// Index = `HouseId`.
    pub houses: Vec<HouseFact>,
    /// Index = `PersonId`.
    pub persons: Vec<PersonFact>,
}

impl Facts {
    /// A stable fingerprint of the facts: FNV-1a over their canonical JSON. Two scenario
    /// files that compile to the same facts hash the same; a changed fact changes it.
    pub fn hash(&self) -> u64 {
        let bytes = serde_json::to_vec(self).expect("facts serialize");
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
}

/// The whole state of one run. **This is the save format**: see `save.rs` for the rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub scenario_id: String,
    pub facts_hash: u64,
    pub seed: u64,
    pub start_year: i32,
    pub tick: Tick,
    pub diseases: std::collections::BTreeSet<Disease>,
    pub actors: BTreeMap<ActorId, Actor>,
    pub provinces: BTreeMap<ProvinceId, Province>,
    pub units: BTreeMap<UnitId, Unit>,
    /// Next unit id; units born in play (recruitment, E2) number on from here.
    pub next_unit: u32,
    /// Polities at war, as pairs with the lower id first (`army.rs`). Opened only by the
    /// tests until diplomacy prices the declaration; closed by the fall of a side.
    pub wars: std::collections::BTreeSet<(ActorId, ActorId)>,
    pub houses: BTreeMap<HouseId, House>,
    pub persons: BTreeMap<PersonId, Person>,
    /// Next person id; persons born in play number on from here.
    pub next_person: u32,
    pub names: Names,
    pub ledger: Ledger,
    /// Draws of the tick being played; drained into the tick report. The ordinal `n` of a
    /// draw is its count among the earlier draws of the tick with the same point and id.
    draws: Vec<RngDraw>,
    /// The ordinals: how many draws of each point and id the tick has made. Cleared with
    /// the journal, so between ticks — where a save is taken — it is empty; it is saved
    /// as that empty map because `serde(skip)` is forbidden here (guard). Counting by a
    /// scan of the journal was quadratic in the draws of a tick (thousands once every
    /// person draws twice: #285 review).
    draw_counts: BTreeMap<DrawPoint, BTreeMap<u32, u32>>,
    /// Events of the tick being played; drained into the tick report.
    events: Vec<Stamped>,
    /// Next event sequence number; saved, so numbering continues across a load.
    next_seq: u64,
}

/// What one tick did: every event, every posting and every draw, nothing more (plan §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickReport {
    pub tick: Tick,
    pub events: Vec<Stamped>,
    pub postings: Vec<Posting>,
    pub draws: Vec<RngDraw>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorldError {
    #[error("province {0:?} is owned by unknown actor {1:?}")]
    UnknownOwner(ProvinceId, ActorId),
    #[error("province {0:?} lists unknown neighbour {1:?}")]
    UnknownNeighbour(ProvinceId, ProvinceId),
    #[error("province {0:?}: neighbours must be sorted and unique")]
    NeighboursUnsorted(ProvinceId),
    #[error("province {0:?} has a sea neighbour but is not coastal")]
    SeaWithoutCoast(ProvinceId),
    #[error("actor {0:?}: capital {1:?} is not one of its provinces")]
    CapitalNotOwned(ActorId, ProvinceId),
    #[error("names table has {names} entries for {items} {what}")]
    NamesMismatch { what: &'static str, names: usize, items: usize },
    #[error("unknown province {0:?}")]
    UnknownProvince(ProvinceId),
    #[error("unknown unit {0:?}")]
    UnknownUnit(UnitId),
    #[error("unknown actor {0:?}")]
    UnknownActor(ActorId),
    #[error("a polity cannot be at war with itself: {0:?}")]
    WarWithSelf(ActorId),
    #[error("unknown person {0:?}")]
    UnknownPerson(PersonId),
    #[error("house {0:?} belongs to unknown polity {1:?}")]
    UnknownHousePolity(HouseId, ActorId),
    #[error("person {0:?} belongs to unknown house {1:?}")]
    UnknownHouse(PersonId, HouseId),
    #[error("person {0:?} holds a seat of polity {1:?} but belongs to a house of another polity")]
    SeatOutsidePolity(PersonId, ActorId),
    #[error("person {0:?} is not older than its child {1:?}")]
    ParentNotOlder(PersonId, PersonId),
    #[error("person {0:?} is born after the start of the world")]
    BornInTheFuture(PersonId),
    #[error("ledger: {0}")]
    Ledger(#[from] LedgerError),
}

fn sorted_unique(v: &[ProvinceId]) -> bool {
    v.windows(2).all(|w| w[0] < w[1])
}

impl World {
    /// Build a world from compiled facts and their names. Fails on a dangling reference
    /// rather than carrying it (lesson of the old engine's dangling neighbours).
    pub fn from_facts(facts: &Facts, names: Names, params: &Params, seed: u64) -> Result<World, WorldError> {
        if names.actors.len() != facts.actors.len() {
            return Err(WorldError::NamesMismatch { what: "actors", names: names.actors.len(), items: facts.actors.len() });
        }
        if names.provinces.len() != facts.provinces.len() {
            return Err(WorldError::NamesMismatch { what: "provinces", names: names.provinces.len(), items: facts.provinces.len() });
        }
        if names.houses.len() != facts.houses.len() {
            return Err(WorldError::NamesMismatch { what: "houses", names: names.houses.len(), items: facts.houses.len() });
        }
        if names.persons.len() != facts.persons.len() {
            return Err(WorldError::NamesMismatch { what: "persons", names: names.persons.len(), items: facts.persons.len() });
        }
        let tick = Tick::ZERO;
        let mut ledger = Ledger::new();
        // Houses and persons first: the seats below refer to them.
        let n_actors = facts.actors.len() as u32;
        let mut houses: BTreeMap<HouseId, House> = BTreeMap::new();
        for (i, h) in facts.houses.iter().enumerate() {
            let id = HouseId(i as u32);
            if h.polity.0 >= n_actors {
                return Err(WorldError::UnknownHousePolity(id, h.polity));
            }
            houses.insert(id, House { polity: h.polity });
            ledger.open(tick, Account::HouseTreasury(id), h.treasury_kg)?;
        }
        let mut persons: BTreeMap<PersonId, Person> = BTreeMap::new();
        for (i, p) in facts.persons.iter().enumerate() {
            let id = PersonId(i as u32);
            if !houses.contains_key(&p.house) {
                return Err(WorldError::UnknownHouse(id, p.house));
            }
            if p.born > facts.start_year {
                return Err(WorldError::BornInTheFuture(id));
            }
            if let Some(parent) = p.parent {
                let pf = facts.persons.get(parent.index()).ok_or(WorldError::UnknownPerson(parent))?;
                if pf.born >= p.born {
                    return Err(WorldError::ParentNotOlder(parent, id));
                }
            }
            persons.insert(id, Person { house: p.house, born: p.born, parent: p.parent, died: None });
        }
        // A seat of a polity is held by a person of one of its houses.
        let seat = |person: PersonId, polity: ActorId| -> Result<(), WorldError> {
            let p = persons.get(&person).ok_or(WorldError::UnknownPerson(person))?;
            if houses[&p.house].polity != polity {
                return Err(WorldError::SeatOutsidePolity(person, polity));
            }
            Ok(())
        };
        let mut actors: BTreeMap<ActorId, Actor> = BTreeMap::new();
        for (i, a) in facts.actors.iter().enumerate() {
            let id = ActorId(i as u32);
            seat(a.ruler, id)?;
            for p in [a.heir, a.regent].into_iter().flatten() {
                seat(p, id)?;
            }
            let standing = crate::politics::standing_of_rule(a.succession);
            actors.insert(
                id,
                Actor {
                    provinces: Vec::new(),
                    capital: a.capital,
                    legitimacy: 0.0,
                    institutions: a.institutions.iter().copied().collect(),
                    tax_rate_ppm: params.tax_default_ppm,
                    paid_share_ppm: 1_000_000,
                    ruler: a.ruler,
                    heir: a.heir,
                    regent: a.regent,
                    succession: a.succession,
                    standing,
                    credit_closed_until: None,
                },
            );
            ledger.open(tick, Account::Treasury(id), a.treasury_kg)?;
            ledger.open(tick, Account::Debt(id), 0)?;
        }
        let mut provinces: BTreeMap<ProvinceId, Province> = BTreeMap::new();
        let n = facts.provinces.len() as u32;
        for (i, f) in facts.provinces.iter().enumerate() {
            let id = ProvinceId(i as u32);
            if !sorted_unique(&f.neighbours) || !sorted_unique(&f.sea_neighbours) {
                return Err(WorldError::NeighboursUnsorted(id));
            }
            if let Some(bad) = f.neighbours.iter().chain(f.sea_neighbours.iter()).find(|p| p.0 >= n) {
                return Err(WorldError::UnknownNeighbour(id, *bad));
            }
            if !f.sea_neighbours.is_empty() && !f.coastal {
                return Err(WorldError::SeaWithoutCoast(id));
            }
            let actor = actors.get_mut(&f.owner).ok_or(WorldError::UnknownOwner(id, f.owner))?;
            actor.provinces.push(id);
            if let Some(g) = f.governor {
                seat(g, f.owner)?;
            }
            let capacity = params.capacity(f.area_km2, f.land);
            provinces.insert(
                id,
                Province {
                    owner: f.owner,
                    area_km2: f.area_km2,
                    land: f.land,
                    coastal: f.coastal,
                    capacity,
                    neighbours: f.neighbours.clone(),
                    sea_neighbours: f.sea_neighbours.clone(),
                    siege: None,
                    governor: f.governor,
                },
            );
            ledger.open(tick, Account::Population(id), f.population as i64)?;
        }
        let mut units: BTreeMap<UnitId, Unit> = BTreeMap::new();
        let mut next_unit = 0u32;
        for (i, a) in facts.actors.iter().enumerate() {
            let id = ActorId(i as u32);
            if provinces.get(&a.capital).map(|p| p.owner) != Some(id) {
                return Err(WorldError::CapitalNotOwned(id, a.capital));
            }
            if a.soldiers > 0 {
                let uid = UnitId(next_unit);
                next_unit += 1;
                units.insert(uid, Unit { owner: id, position: a.capital, quality: 50.0, destination: None, mercenary: false });
                ledger.open(tick, Account::Soldiers(uid), a.soldiers as i64)?;
            }
        }
        // The opening postings belong to no tick report; the world starts settled.
        ledger.drain_postings();
        let next_person = persons.len() as u32;
        let mut world = World {
            scenario_id: facts.scenario_id.clone(),
            facts_hash: facts.hash(),
            seed,
            start_year: facts.start_year,
            tick,
            diseases: facts.diseases.iter().copied().collect(),
            actors,
            provinces,
            units,
            next_unit,
            wars: std::collections::BTreeSet::new(),
            houses,
            persons,
            next_person,
            names,
            ledger,
            draws: Vec::new(),
            draw_counts: BTreeMap::new(),
            events: Vec::new(),
            next_seq: 0,
        };
        // Legitimacy starts at the norm of the ruler's standing (a minor on the throne
        // starts at the regency norm).
        let ids: Vec<ActorId> = world.actors.keys().copied().collect();
        for a in ids {
            let norm = world.legitimacy_norm(a, params);
            world.actors.get_mut(&a).unwrap().legitimacy = norm;
        }
        Ok(world)
    }

    /// Append an event of the current tick.
    pub fn emit(&mut self, event: Event) {
        self.events.push(Stamped { seq: self.next_seq, tick: self.tick, event });
        self.next_seq += 1;
    }

    /// The one write path for stocks: post through the ledger, and if the stock was
    /// clamped at zero, say so with a `BoundHit` event. The tick goes on either way.
    pub fn post(&mut self, account: Account, source: Source, milli: i64) -> Result<(), WorldError> {
        let posted = self.ledger.post(self.tick, account, source, milli)?;
        if posted.clamped {
            self.emit(Event::BoundHit { account, source });
        }
        Ok(())
    }

    /// The ordinal the next draw of `(point, id)` gets this tick.
    pub(crate) fn draw_ordinal(&self, point: DrawPoint, id: u32) -> u32 {
        self.draw_counts.get(&point).and_then(|m| m.get(&id)).copied().unwrap_or(0)
    }

    /// Journal a draw made with [`rng::draw`] on a key built from [`draw_ordinal`]; the
    /// next draw of the same `(point, id)` gets the next ordinal.
    pub(crate) fn journal_draw(&mut self, d: Draw, value: u64) {
        debug_assert_eq!(d.n, self.draw_ordinal(d.point, d.id), "a draw journaled out of order");
        *self.draw_counts.entry(d.point).or_default().entry(d.id).or_insert(0) += 1;
        self.draws.push(RngDraw { draw: d, value });
    }

    /// A keyed draw for this world, journaled. Repeated draws of one point for one id in
    /// one tick get successive ordinals and therefore different numbers.
    pub fn draw(&mut self, point: DrawPoint, id: u32) -> u64 {
        let d = Draw { point, tick: self.tick, id, n: self.draw_ordinal(point, id) };
        let value = rng::draw(self.seed, d);
        self.journal_draw(d, value);
        value
    }

    /// Draws for several ids of one point, **in canonical order** — ascending id, duplicates
    /// collapsed — whatever order the caller holds them in. The ordinal `n` depends on the
    /// order of draws within a `(point, id)` pair, so systems must not let the traversal
    /// order of their code leak into the key (owner's rule after PR #277). Returns the
    /// values keyed by id.
    pub fn draw_each(&mut self, point: DrawPoint, ids: impl IntoIterator<Item = u32>) -> BTreeMap<u32, u64> {
        let sorted: std::collections::BTreeSet<u32> = ids.into_iter().collect();
        sorted.into_iter().map(|id| (id, self.draw(point, id))).collect()
    }

    /// Play one tick: one decision draw per actor (stand-in for E3), units move
    /// (`movement.rs`), battles and sieges (`army.rs`), the economy produces, taxes and
    /// pays (`economy.rs`), units are supplied to the paid share, the house and the throne
    /// (`politics.rs`), people grow, starve, sicken and migrate (`population.rs`), then
    /// `TickPlayed`.
    pub fn tick(&mut self, params: &Params) -> Result<TickReport, WorldError> {
        let actor_ids: Vec<u32> = self.actors.keys().map(|a| a.0).collect();
        let _ = self.draw_each(DrawPoint::Decision, actor_ids);
        self.phase_movement(params)?;
        self.phase_battle(params)?;
        self.phase_siege(params)?;
        let _ = self.phase_economy(params)?;
        self.phase_supply(params)?;
        self.phase_people(params)?;
        self.phase_population(params)?;
        self.ledger.check()?;
        self.emit(Event::TickPlayed);
        let report = TickReport {
            tick: self.tick,
            events: std::mem::take(&mut self.events),
            postings: self.ledger.drain_postings(),
            draws: std::mem::take(&mut self.draws),
        };
        self.draw_counts.clear();
        self.tick = self.tick.next();
        Ok(report)
    }

    pub fn year(&self) -> i32 {
        self.tick.year(self.start_year)
    }

    pub fn population(&self, p: ProvinceId) -> i64 {
        self.ledger.stock(Account::Population(p)).unwrap_or(0)
    }

    pub fn treasury(&self, a: ActorId) -> i64 {
        self.ledger.stock(Account::Treasury(a)).unwrap_or(0)
    }

    /// What the polity owes (E0 Э6, Э7); zero without a bank.
    pub fn debt(&self, a: ActorId) -> i64 {
        self.ledger.stock(Account::Debt(a)).unwrap_or(0)
    }

    pub fn soldiers(&self, u: UnitId) -> i64 {
        self.ledger.stock(Account::Soldiers(u)).unwrap_or(0)
    }

    pub fn house_treasury(&self, h: HouseId) -> i64 {
        self.ledger.stock(Account::HouseTreasury(h)).unwrap_or(0)
    }

    /// The units of an actor, in id order.
    pub fn units_of(&self, a: ActorId) -> Vec<UnitId> {
        self.units.iter().filter(|(_, u)| u.owner == a).map(|(id, _)| *id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld;

    fn params() -> Params {
        Params::load().unwrap()
    }

    fn world(seed: u64) -> World {
        testworld::world(seed)
    }

    #[test]
    fn dangling_owner_neighbour_capital_and_sea_are_refused() {
        let (mut facts, names) = testworld::five_states();
        facts.provinces[0].owner = ActorId(99);
        assert!(matches!(World::from_facts(&facts, names.clone(), &params(), 1), Err(WorldError::UnknownOwner(..))));
        let (mut facts, names) = testworld::five_states();
        facts.provinces[0].neighbours = vec![ProvinceId(999)];
        assert!(matches!(World::from_facts(&facts, names.clone(), &params(), 1), Err(WorldError::UnknownNeighbour(..))));
        let (mut facts, names) = testworld::five_states();
        facts.actors[0].capital = ProvinceId(1); // owned by state 1
        assert!(matches!(World::from_facts(&facts, names.clone(), &params(), 1), Err(WorldError::CapitalNotOwned(..))));
        let (mut facts, names) = testworld::five_states();
        facts.provinces[12].sea_neighbours = vec![ProvinceId(0)]; // inland cell
        assert!(matches!(World::from_facts(&facts, names, &params(), 1), Err(WorldError::SeaWithoutCoast(..))));
    }

    #[test]
    fn names_must_match_the_facts() {
        let (facts, mut names) = testworld::five_states();
        names.actors.pop();
        assert!(matches!(World::from_facts(&facts, names, &params(), 1), Err(WorldError::NamesMismatch { what: "actors", .. })));
    }

    #[test]
    fn capacity_is_derived_from_area_and_land() {
        let w = world(1);
        let p = &w.provinces[&ProvinceId(0)];
        assert_eq!(p.land, Land::Arable);
        assert_eq!(p.capacity, params().capacity(p.area_km2, Land::Arable));
        assert_eq!(p.capacity, 10_000);
    }

    #[test]
    fn facts_hash_changes_with_a_fact() {
        let (facts, _) = testworld::five_states();
        let mut other = facts.clone();
        other.actors[0].treasury_kg += 1;
        assert_ne!(facts.hash(), other.hash());
        assert_eq!(facts.hash(), testworld::five_states().0.hash());
    }

    #[test]
    fn every_actor_starts_with_one_unit_at_its_capital() {
        let w = world(1);
        for (a, actor) in &w.actors {
            let units = w.units_of(*a);
            assert_eq!(units.len(), 1);
            assert_eq!(w.units[&units[0]].position, actor.capital);
            assert!(w.soldiers(units[0]) > 0);
        }
    }

    #[test]
    fn tick_advances_time_and_reports_one_draw_per_actor() {
        let mut w = world(7);
        let r = w.tick(&params()).unwrap();
        assert_eq!(r.tick, Tick(0));
        assert_eq!(r.draws.iter().filter(|d| d.draw.point == DrawPoint::Decision).count(), w.actors.len());
        assert!(r.draws.iter().all(|d| matches!(d.draw.point, DrawPoint::Decision | DrawPoint::Mortality | DrawPoint::Violence | DrawPoint::Usurpation | DrawPoint::Birth)), "no system of the test world draws elsewhere");
        assert!(r.postings.iter().all(|p| matches!(p.source, Source::Growth | Source::MigrationIn | Source::MigrationOut | Source::Tax | Source::Seat | Source::Court | Source::Upkeep)), "every unit is paid at home: people move and wheat flows, nobody deserts");
        assert!(r.postings.iter().any(|p| p.source == Source::Growth));
        assert!(r.postings.iter().any(|p| p.source == Source::Tax));
        assert!(matches!(r.events.last().unwrap().event, Event::TickPlayed));
        assert_eq!(r.events[0].seq, 0);
        assert_eq!(w.tick, Tick(1));
        assert_eq!(w.year(), 1477);
        let r2 = w.tick(&params()).unwrap();
        assert_eq!(r2.events[0].seq, r.events.last().unwrap().seq + 1, "sequence numbers continue across ticks");
    }

    #[test]
    fn a_bound_hit_is_an_event_and_the_tick_is_played_out() {
        let mut w = world(5);
        let p = ProvinceId(0);
        let pop = w.population(p);
        w.post(Account::Population(p), Source::Famine, -(pop + 1_000) * 1000).unwrap();
        let r = w.tick(&params()).unwrap();
        assert!(r.events.iter().any(|e| matches!(e.event, Event::BoundHit { account, source: Source::Famine } if account == Account::Population(p))));
        assert!(matches!(r.events.last().unwrap().event, Event::TickPlayed));
        let famine = r.postings.iter().find(|q| q.source == Source::Famine).unwrap();
        assert_eq!(famine.milli, -pop * 1000, "the posted amount is what actually left");
        assert!(w.population(p) >= 0);
        assert_eq!(w.tick, Tick(1));
    }

    #[test]
    fn draw_each_is_canonical_whatever_the_callers_order() {
        let mut a = world(11);
        let mut b = world(11);
        let va = a.draw_each(DrawPoint::Battle, [4, 1, 3, 1, 2]);
        let vb = b.draw_each(DrawPoint::Battle, [2, 3, 1, 4]);
        assert_eq!(va, vb);
        assert_eq!(va.len(), 4);
        let va2 = a.draw_each(DrawPoint::Battle, [3, 4, 2, 1]);
        let vb2 = b.draw_each(DrawPoint::Battle, [1, 2, 3, 4]);
        assert_eq!(va2, vb2);
        assert_ne!(va, va2);
    }

    #[test]
    fn repeated_draws_of_one_point_in_one_tick_differ() {
        let mut w = world(7);
        let a = w.draw(DrawPoint::Battle, 3);
        let b = w.draw(DrawPoint::Battle, 3);
        assert_ne!(a, b);
        let r = w.tick(&params()).unwrap();
        let ordinals: Vec<u32> = r.draws.iter().filter(|d| d.draw.point == DrawPoint::Battle).map(|d| d.draw.n).collect();
        assert_eq!(ordinals, vec![0, 1]);
    }

    #[test]
    fn growth_moves_population_toward_capacity_and_keeps_a_remainder() {
        let mut w = world(7);
        let p = ProvinceId(0);
        let cap = w.provinces[&p].capacity as i64;
        let before = w.population(p);
        for _ in 0..10 {
            w.tick(&params()).unwrap();
        }
        let after = w.population(p);
        assert!(after > before && after <= cap, "{before} -> {after} <= {cap}");
        assert!(w.ledger.carry(Account::Population(p)).unwrap() > 0);
    }
}
