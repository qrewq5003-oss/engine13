//! The typed event stream (plan §6).
//!
//! No text and no ids parsed by prefix: every event carries typed ids, and the words are
//! made outside the engine from these data and the `Names` table. The chronicler receives
//! the same data through the chronicle snapshot. Variants are added as the systems of E2
//! arrive; E1b emits `TickPlayed` and `Verb`.

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, PersonId, ProvinceId, UnitId};
use crate::ledger::{Account, Source};
use crate::time::Tick;
use crate::verbs::{ActorRef, Target, VerbId};

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
    Battle { province: ProvinceId, attacker: UnitId, defender: UnitId, outcome: Outcome, losses: (u64, u64) },
    ProvinceChangedHands { province: ProvinceId, from: ActorId, to: ActorId, how: Transfer },
    War { a: ActorId, b: ActorId },
    Peace { a: ActorId, b: ActorId },
    RulerDied { actor: ActorId, person: PersonId, cause: Cause },
    ActorDied { actor: ActorId },
    /// A stock crossed zero or a bound; the ledger refused the posting. Kept as an event so
    /// the sentinel sees bound hits without reading engine internals.
    BoundHit { account: Account, source: Source },
}

/// An event with its place in the run: a global sequence number and the tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamped {
    pub seq: u64,
    pub tick: Tick,
    pub event: Event,
}
