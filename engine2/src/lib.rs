//! engine2 — the new ENGINE13 engine (`docs/engine2_brief.md`, plan `docs/engine2_e1_plan.md`).
//!
//! Step E1a: the skeleton — typed ids, the time contract, keyed deterministic randomness
//! with a draw journal, integer stocks behind a ledger with remainder carry, the world
//! state, bit-exact save/load, and a synthetic test world.
//! Step E1b: the typed event stream, the `Simulation` contract, the scenario schema (facts
//! only, `deny_unknown_fields`), the constants file with kinds, the presentation model.
//! Step E1c: `Observed` for the sentinel crate (`observe.rs`), the short run in CI.
//! E2 step 2: the province graph with land and sea edges, units that move and are supplied
//! on it (`movement.rs`). E2 step 3: the population system (`population.rs`) — the first
//! randomness (plague). E2 step 4: the economy (`economy.rs`) — the treasury as a flow,
//! the budget as the army's limit, the bank as the first institution. E2 step 5: the army
//! (`army.rs`) — war, march, battle, siege, the fall of polities. E2 step 6a: the house and
//! the throne (`politics.rs`) — people, houses, seats, deaths, births, succession, regency.
//! E2 step 9: collapse (`collapse.rs`) — provinces rise, a house takes its land into a
//! polity of its own, the successor gets provinces and a war, not a copy of metrics.
//! E3 (а): agents (`agents.rs`) — goals as state, utility without learning, softmax over
//! the offers, every bot deciding on the snapshot before anything is applied.
//! No systems yet: a tick advances time, journals one decision draw per actor, runs one
//! stand-in flow (population toward capacity) and keeps the ledger balanced.
//!
//! Design rules enforced by type or by test (plan §1, §3, §9):
//! - ids are numbers; the engine never sees an authored name;
//! - stocks are integers and change only through [`ledger::Ledger`];
//! - state lives in ordered collections (`BTreeMap`, `Vec`); `HashMap` is forbidden;
//! - randomness comes from [`rng::draw`], keyed by `(seed, point, tick, id)`, never from a
//!   shared stream, so two runs that differ in one place agree everywhere else.

#![forbid(unsafe_code)]

pub mod agents;
pub mod army;
pub mod collapse;
pub mod diplomacy;
pub mod economy;
pub mod event;
pub mod faith;
pub mod ids;
pub mod ledger;
pub mod movement;
#[cfg(feature = "sentinel")]
pub mod observe;
pub mod opportunity;
pub mod par;
pub mod params;
pub mod politics;
pub mod population;
pub mod rng;
pub mod save;
pub mod scenario;
pub mod sim;
pub mod testworld;
pub mod time;
pub mod verbs;
pub mod view;
pub mod world;
#[cfg(feature = "worldgen")]
pub mod worldgen;

pub use agents::{Goal, GoalFact, GoalKind, GoalTarget};
pub use event::{Event, Stamped};
pub use ids::{ActorId, CultureId, HouseId, PersonId, ProvinceId, RegionId, ReligionId, UnitId};
pub use ledger::{Account, Ledger, Posting, Source};
pub use opportunity::{Answer, Chance, Opportunity, Reply};
pub use params::Params;
pub use rng::{Draw, DrawPoint, RngDraw};
pub use save::{from_save, to_save, SaveError, SAVE_FORMAT};
pub use scenario::{Compiled, Display, ScenarioData};
pub use sim::{Game, Simulation, Turn};
pub use time::Tick;
pub use verbs::{ActorRef, Order, Target, VerbCall, VerbId, VerbOffer};
pub use world::{ActorFact, Disease, Facts, House, HouseFact, Institution, Land, Names, Person, PairMap, PersonFact, ProvinceFact, RelationFacts, Shift, Siege, Standing, Succession, TickReport, Treaty, TreatyKind, Unit, War, World};
