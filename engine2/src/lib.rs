//! engine2 — the new ENGINE13 engine (`docs/engine2_brief.md`, plan `docs/engine2_e1_plan.md`).
//!
//! Step E1a: the skeleton — typed ids, the time contract, keyed deterministic randomness
//! with a draw journal, integer stocks behind a ledger with remainder carry, the world
//! state, bit-exact save/load, and a synthetic test world. No systems yet: a tick only
//! advances time, journals one decision draw per actor and keeps the ledger balanced.
//!
//! Design rules enforced by type or by test (plan §1, §3, §9):
//! - ids are numbers; the engine never sees an authored name;
//! - stocks are integers and change only through [`ledger::Ledger`];
//! - state lives in ordered collections (`BTreeMap`, `Vec`); `HashMap` is forbidden;
//! - randomness comes from [`rng::draw`], keyed by `(seed, point, tick, id)`, never from a
//!   shared stream, so two runs that differ in one place agree everywhere else.

#![forbid(unsafe_code)]

pub mod ids;
pub mod ledger;
pub mod rng;
pub mod save;
pub mod testworld;
pub mod time;
pub mod world;

pub use ids::{ActorId, HouseId, PersonId, ProvinceId, UnitId};
pub use ledger::{Account, Ledger, Posting, Source};
pub use rng::{Draw, DrawPoint, RngDraw};
pub use save::{from_save, to_save, SaveError, SAVE_FORMAT};
pub use time::Tick;
pub use world::{Facts, TickReport, World};
