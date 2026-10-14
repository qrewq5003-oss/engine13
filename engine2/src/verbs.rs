//! Verbs: the closed list of actions (brief §3), one set for the player and for agents.
//!
//! A scenario enables a subset and names them; it cannot add one or attach a number. The
//! effect of a verb is computed by the systems from the state (E2/E3). In E1b a verb call is
//! journaled as an event and has no effect: the pipeline exists, the systems do not yet.

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, HouseId, ProvinceId};

/// Who acts: a ruler of a polity or a house inside one (the two roles of E0 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ActorRef {
    Ruler(ActorId),
    House(HouseId),
}

/// The engine's verbs. Order is the key of the scenario's `verbs` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VerbId {
    Recruit,
    Disband,
    HireMercenaries,
    DeclareWar,
    MakePeace,
    ProposeAlliance,
    ProposeMarriage,
    Bribe,
    TakeLoan,
    RaiseTax,
    LowerTax,
    EnforceOrder,
    SendEmbassy,
}

impl VerbId {
    /// Every verb, in key order.
    pub const ALL: [VerbId; 13] = [
        VerbId::Recruit,
        VerbId::Disband,
        VerbId::HireMercenaries,
        VerbId::DeclareWar,
        VerbId::MakePeace,
        VerbId::ProposeAlliance,
        VerbId::ProposeMarriage,
        VerbId::Bribe,
        VerbId::TakeLoan,
        VerbId::RaiseTax,
        VerbId::LowerTax,
        VerbId::EnforceOrder,
        VerbId::SendEmbassy,
    ];

    /// The key the scenario uses to enable and name the verb.
    pub const fn key(self) -> &'static str {
        match self {
            VerbId::Recruit => "recruit",
            VerbId::Disband => "disband",
            VerbId::HireMercenaries => "hire_mercenaries",
            VerbId::DeclareWar => "declare_war",
            VerbId::MakePeace => "make_peace",
            VerbId::ProposeAlliance => "propose_alliance",
            VerbId::ProposeMarriage => "propose_marriage",
            VerbId::Bribe => "bribe",
            VerbId::TakeLoan => "take_loan",
            VerbId::RaiseTax => "raise_tax",
            VerbId::LowerTax => "lower_tax",
            VerbId::EnforceOrder => "enforce_order",
            VerbId::SendEmbassy => "send_embassy",
        }
    }

    pub fn from_key(key: &str) -> Option<VerbId> {
        VerbId::ALL.into_iter().find(|v| v.key() == key)
    }
}

/// What a verb is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    None,
    Actor(ActorId),
    Province(ProvinceId),
    House(HouseId),
}

/// A call: who, what, at whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerbCall {
    pub actor: ActorRef,
    pub verb: VerbId,
    pub target: Target,
}

/// An offer the engine makes: this verb, available or not, at this cost. One function
/// computes it for the player and for agents (lesson B42).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerbOffer {
    pub verb: VerbId,
    pub available: bool,
    /// Cost in kg of wheat-equivalent, if the verb costs treasury.
    pub cost_kg: i64,
    /// Why it is not available, when it is not.
    pub why: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_are_unique() {
        for v in VerbId::ALL {
            assert_eq!(VerbId::from_key(v.key()), Some(v));
        }
        let mut keys: Vec<&str> = VerbId::ALL.iter().map(|v| v.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), VerbId::ALL.len());
    }
}
