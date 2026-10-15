//! The typed event stream (plan §6).
//!
//! No text and no ids parsed by prefix: every event carries typed ids, and the words are
//! made outside the engine from these data and the `Names` table. The chronicler receives
//! the same data through the chronicle snapshot. Variants are added as the systems of E2
//! arrive; E1b emits `TickPlayed` and `Verb`.

use serde::{Deserialize, Serialize};

use crate::agents::Goal;
use crate::ids::{CultureId, ReligionId, ActorId, HouseId, PersonId, ProvinceId, UnitId};
use crate::ledger::{Account, Source};
use crate::movement::Way;
use crate::time::Tick;
use crate::verbs::{ActorRef, Target, VerbId};
use crate::world::{Institution, Standing, TreatyKind};

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
    /// The ruler bought legitimacy with the treasury (`enforce_order`, E2 step 6b).
    OrderEnforced { actor: ActorId, kg: u64 },
    /// The polity defaulted: a debt past the ceiling whose interest the treasury could not
    /// meet is written off, legitimacy pays, the credit closes for a term (E2 step 6b).
    Default { actor: ActorId, written_off_kg: u64 },
    /// The term after a default ended: the bank lends again.
    CreditRestored { actor: ActorId },
    /// A treaty ran out at its term (E0 Д1; E2 step 7).
    TreatyExpired { a: ActorId, b: ActorId, kind: TreatyKind },
    /// A vassal is a vassal no more: its lord fell, or made the peace that set it free.
    Released { vassal: ActorId, lord: ActorId },
    /// A vassal's treasury did not cover the tribute due (E0 Д2).
    TributeShort { vassal: ActorId, lord: ActorId, unpaid_kg: u64 },
    /// An alliance made, running to `until` (E0 Д1).
    Alliance { a: ActorId, b: ActorId, until: Tick },
    /// A proposal the other side did not accept: the relation was below the threshold, or
    /// the state of the pair forbade it; the embassy's gifts are spent all the same.
    Refused { actor: ActorId, verb: VerbId, target: ActorId },
    /// A treaty broken by a declaration of war before its term: priced as a breach.
    TreatyBroken { a: ActorId, b: ActorId, kind: TreatyKind },
    /// An embassy sent: gifts spent for a push of the relation.
    Embassy { from: ActorId, to: ActorId, kg: u64 },
    /// A gift to another polity's treasury (E0 Д2, the subsidy).
    Gift { from: ActorId, to: ActorId, kg: u64 },
    /// Two houses bound by a marriage, with the dowry that went with it.
    Marriage { a: PersonId, b: PersonId, dowry_kg: u64 },
    /// A peace of submission: the loser of the war became the winner's vassal.
    Submission { vassal: ActorId, lord: ActorId },
    /// A province's majority turned to another faith (E2 step 8, E0 Р1): by the rule of a
    /// court with the conversion policy, or by the drift of its neighbourhood.
    Converted { province: ProvinceId, from: ReligionId, to: ReligionId, by: Channel },
    /// A province's majority took the tongue of its neighbourhood.
    Assimilated { province: ProvinceId, from: CultureId, to: CultureId },
    /// A polity took up an institution along a channel of contact — trade with a neighbour,
    /// a marriage, a treaty — from a polity that holds it (E0 §2 variant 3; E2 step 8).
    Adopted { actor: ActorId, institution: Institution, from: ActorId },
    /// A house that took a throne took the faith of the court with it (owner's rule on
    /// #288: a foreign dynasty took the faith of the land — «Paris is worth a mass»); its
    /// tongue stays its own.
    HouseConverted { house: HouseId, from: ReligionId, to: ReligionId },
    /// A house's treasury could not meet the pay of its own men in full (E0 Х2, E2 step 9).
    HousePayShortfall { house: HouseId, unpaid_kg: u64 },
    /// A province rose and a house that does not rule took it — with every province its
    /// members govern — into a polity of its own (E0 В7, E2 step 9); one event per province.
    Secession { province: ProvinceId, from: ActorId, house: HouseId },
    /// A province with no house to carry it rose and is in revolt (owner's rule on #289):
    /// no tax, no supply for its owner's men, who forage there; the ruler pays in
    /// legitimacy. It stays so until a unit of the owner has stood there a tick.
    Revolt { province: ProvinceId, from: ActorId },
    /// A revolt put down: a unit of the owner stood in the province a tick.
    RevoltEnded { province: ProvinceId, actor: ActorId },
    /// A polity was born out of another: the house that leads it and the provinces it took
    /// (brief §2a: the successor gets provinces, not a copy of metrics).
    ActorBorn { actor: ActorId, from: ActorId, house: HouseId, provinces: Vec<ProvinceId> },
    /// A polity set itself a goal (E3): at the start from the facts, later by the rules.
    GoalSet { actor: ActorId, goal: Goal },
    /// A goal's progress reached one: what was to be taken is held, what was to be subdued
    /// is a vassal… The goal goes on as «hold» where there is something to hold.
    GoalReached { actor: ActorId, goal: Goal },
    /// A goal that made no progress over `goal.horizon_years` was put aside.
    GoalDropped { actor: ActorId, goal: Goal },
    /// A call — the player's or a bot's — closed by an earlier call of the same tick (two
    /// houses asking for the same bride): everyone decided on the state at the tick's
    /// start, and the calls are applied in one order for all (E3).
    Preempted { actor: ActorRef, verb: VerbId, target: Target },
    /// An opportunity opened for its decider, to be answered in the next turn (E3 (в)).
    OpportunityOpened { opportunity: crate::opportunity::Opportunity },
    /// An opportunity taken; its effect is in the other events of the tick.
    OpportunityTaken { id: u32, by: ActorRef },
    OpportunityDeclined { id: u32, by: ActorRef },
    /// Nobody answered: the opportunity is gone.
    OpportunityLapsed { id: u32, to: ActorRef },
    /// Taken, but the world changed since it was opened (a province lost, a candidate dead).
    OpportunityVoid { id: u32, why: String },
}

/// The channel a conversion or an assimilation came by (plan §20.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    /// The owner's court, under `Institution::ConversionPolicy`.
    Rule,
    /// Most of the province's neighbours, by land and by sea.
    Neighbourhood,
}

/// An event with its place in the run: a global sequence number and the tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamped {
    pub seq: u64,
    pub tick: Tick,
    pub event: Event,
}
