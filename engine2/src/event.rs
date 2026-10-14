//! The typed event stream (plan §6).
//!
//! No text and no ids parsed by prefix: every event carries typed ids, and the words are
//! made outside the engine from these data and the `Names` table. The chronicler receives
//! the same data through the chronicle snapshot. Variants are added as the systems of E2
//! arrive; E1b emits `TickPlayed` and `Verb`.

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, HouseId, PersonId, ProvinceId, UnitId};
use crate::ledger::{Account, Source};
use crate::movement::Way;
use crate::time::Tick;
use crate::verbs::{ActorRef, Target, VerbId};
use crate::world::Standing;

/// Outcome of a battle for the attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    AttackerWon,
    DefenderWon,
    Drawn,
}

/// Why a ruler died.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cause {
    Natural,
    Battle,
    Murder,
    Execution,
}

/// How a province changed hands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transfer {
    Conquest,
    Treaty,
    Secession,
    Inheritance,
}

/// What happened. Closed list of the engine; the scenario cannot add variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// The tick was played. Always the last event of a tick.
    TickPlayed,
    /// A verb was called by the player or an agent. Its effect is in the postings and in
    /// the other events of the tick, never a number in the event itself.
    Verb { actor: ActorRef, verb: VerbId, target: Target },
    /// One battle in a province between two polities at war (all their units there); the
    /// defender is the province's owner if it took part, else the lower id.
    /// `soldiers` are the sides' men before the battle, `losses` what each lost.
    Battle { province: ProvinceId, attacker: ActorId, defender: ActorId, outcome: Outcome, soldiers: (u64, u64), losses: (u64, u64) },
    ProvinceChangedHands { province: ProvinceId, from: ActorId, to: ActorId, how: Transfer },
    War { a: ActorId, b: ActorId },
    Peace { a: ActorId, b: ActorId },
    RulerDied { actor: ActorId, person: PersonId, cause: Cause },
    ActorDied { actor: ActorId },
    /// A stock was clamped at zero: the ledger posted what there was and stopped. Kept as an
    /// event so the sentinel sees bound hits without reading engine internals.
    BoundHit { account: Account, source: Source },
    /// A unit took one step along its path.
    UnitMoved { unit: UnitId, from: ProvinceId, to: ProvinceId, way: Way },
    /// A unit's order had no passable path and was dropped.
    NoPath { unit: UnitId, to: ProvinceId },
    /// A unit lost men for want of supply; `unsupplied` is how many stood beyond it.
    Attrition { unit: UnitId, unsupplied: u64 },
    /// A province above its capacity lost people.
    Famine { province: ProvinceId, dead: u64 },
    /// An outbreak took people of a province.
    Plague { province: ProvinceId, dead: u64 },
    /// People moved to an emptier neighbour; the sum of people is unchanged.
    Migration { from: ProvinceId, to: ProvinceId, people: u64 },
    /// The treasury could not meet the army's pay in full.
    PayShortfall { actor: ActorId, unpaid_kg: u64 },
    /// A polity with a bank borrowed.
    Loan { actor: ActorId, kg: u64 },
    /// A beaten unit fell back to the nearest province of its own.
    Retreat { unit: UnitId, to: ProvinceId },
    /// A beaten unit with nowhere to fall back to was destroyed.
    UnitDestroyed { unit: UnitId },
    /// A polity lost its capital and moved it to its most populous province.
    CapitalMoved { actor: ActorId, to: ProvinceId },
    /// Foraging armies took from a province's people; `foragers` are the soldiers it fed.
    Ruin { province: ProvinceId, foragers: u64 },
    /// A levy was raised from a province into a unit.
    Recruited { unit: UnitId, soldiers: u64 },
    /// A unit was disbanded: a levy back to the people, mercenaries out of the world.
    Disbanded { unit: UnitId, soldiers: u64 },
    /// Mercenaries were hired against an advance.
    Hired { unit: UnitId, soldiers: u64, advance_kg: u64 },
    /// A person died (a ruler's death also emits `RulerDied` with its cause).
    PersonDied { person: PersonId },
    /// A child was born to a member of a house.
    PersonBorn { person: PersonId, house: HouseId, parent: PersonId },
    /// A house has no living member left: the subject is lost (E0 §2.3).
    HouseExtinct { house: HouseId },
    /// No house of the polity has a living member: a new house rose from the people of
    /// `from` — the founder with `household` adults — and the founder took the throne
    /// (owner's rule on #285: the extinction of a dynasty does not kill a polity).
    NewDynasty { actor: ActorId, house: HouseId, founder: PersonId, from: ProvinceId, household: u64 },
    /// The throne passed; `how` is the standing the new ruler holds it by.
    Succession { actor: ActorId, from: PersonId, to: PersonId, how: Standing },
    /// A minor on the throne got a regent.
    Regency { actor: ActorId, ward: PersonId, regent: PersonId },
    /// The ruler came of age; the regency ended.
    RegencyEnded { actor: ActorId, ruler: PersonId },
    /// A governorship passed within the house (`to` is `None` when the seat fell vacant).
    GovernorChanged { province: ProvinceId, from: PersonId, to: Option<PersonId> },
    /// The ruler moved the tax rate (parts per million of production) for a price in
    /// legitimacy (E2 step 6b).
    TaxRateChanged { actor: ActorId, from_ppm: i64, to_ppm: i64 },
}

/// An event with its place in the run: a global sequence number and the tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamped {
    pub seq: u64,
    pub tick: Tick,
    pub event: Event,
}
