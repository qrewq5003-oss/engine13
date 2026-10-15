//! Religion and culture (brief §2 «Религия и культура», plan §20): the tongue and the faith
//! of a province's majority and of a house; the friction of two courts in the relation of
//! their polities (`diplomacy.rs`); the alienation of a province from its owner as the entry
//! of collapse. Conversion along E0 Р1 and the spread of institutions follow in steps (б)
//! and (в).
//!
//! The engine never reads which tongue or faith is which: the scenario's strings are
//! interned to `CultureId`/`ReligionId` at compile time, and every rule compares ids for
//! equality alone. A polity speaks and prays as its **ruling house** does (owner's answer on
//! §20), so a new dynasty may change the faith of the court — a house raised from the people
//! takes its capital's.

use crate::ids::{ActorId, CultureId, ProvinceId, ReligionId};
use crate::world::World;

impl World {
    /// The faith of a polity's court: its ruling house's.
    pub fn polity_religion(&self, a: ActorId) -> ReligionId {
        self.houses[&self.persons[&self.actors[&a].ruler].house].religion
    }

    /// The tongue of a polity's court: its ruling house's.
    pub fn polity_culture(&self, a: ActorId) -> CultureId {
        self.houses[&self.persons[&self.actors[&a].ruler].house].culture
    }

    /// A province whose majority prays or speaks otherwise than its owner's court: the
    /// entry of collapse (В7, secession by culture and region), with no effect on the take
    /// in E2 (owner's answer on §20: Э8 stays clean).
    pub fn alienated(&self, p: ProvinceId) -> bool {
        let pr = &self.provinces[&p];
        pr.religion != self.polity_religion(pr.owner) || pr.culture != self.polity_culture(pr.owner)
    }
}
