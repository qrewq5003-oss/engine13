//! Verbs: the closed list of actions (brief §3), one set for the player and for agents.
//!
//! A scenario enables a subset and names them; it cannot add one or attach a number. The
//! effect of a verb is computed by the systems from the state (E2/E3). In E1b a verb call is
//! journaled as an event and has no effect: the pipeline exists, the systems do not yet.

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, HouseId, ProvinceId, UnitId};

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
    Unit(UnitId),
    /// So many men levied from a province — any number up to the tick's levy
    /// (`recruit_per_tick`; E3 (г), owner's decision 4 on §29.8: the world lets any size be
    /// levied, the bot picks the size whose last man still pays). `Province` alone levies the
    /// whole tick's levy.
    Levy(ProvinceId, u32),
}

/// A call: who, what, at whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerbCall {
    pub actor: ActorRef,
    pub verb: VerbId,
    pub target: Target,
}

/// An order to a unit (E3 (б), owner's answer 1 on §23): a channel of the contract, not a
/// verb of the closed list — the march has its price already (supply on foreign land,
/// foraging, the ruin of П6). A ruler orders the polity's units and the ruling house's
/// retinue; a house its own retinue. The player and the bots give orders through the same
/// channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub actor: ActorRef,
    pub unit: UnitId,
    pub to: ProvinceId,
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
    /// Cost in legitimacy (0–100), if the verb costs it; negative when the verb gives some
    /// back (E2 step 6b: legitimacy as a price, plan §18.3).
    pub cost_legitimacy: i64,
    /// Why it is not available, when it is not.
    pub why: Option<String>,
    /// For a proposal (alliance, marriage, peace from the losing side): the relation the
    /// other side needs to accept, 0–100 (E2 step 7, owner's rule on §19: the offer shows
    /// the relation and the threshold; `WorldView` carries the relations).
    pub threshold: Option<i64>,
}

/// An offer of a verb **at one target** (E5, plan §31.77): whether the world would take
/// this call, why not, and what it would cost — by the going over of the targets the bots
/// read too (`offers.rs`). The price is the one the application charges: the shell shows
/// it and computes nothing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TargetOffer {
    pub target: Target,
    pub available: bool,
    /// Why it is not available, when it is not: the caller's side first, then the target's.
    pub why: Option<String>,
    /// What the polity's treasury pays at the application, kg — of every verb alike. A
    /// marriage costs the treasury nothing: its dowry is `dowry_kg`.
    pub cost_kg: i64,
    /// What the ruler pays in legitimacy, in points before saturation — exactly what the
    /// application hands to the price (`politics::saturate`); negative when the verb gives
    /// some back. Of a declaration — **the price of this target** (`World::war_price`: the
    /// treaty with it and every alliance the war would set across its sides).
    pub cost_legitimacy: f64,
    /// For a proposal (peace, alliance, marriage): the relation the other side's polity
    /// must stand at, and where it stands now. The offer says «may be proposed», not «will be
    /// taken»: the answer is the other side's, by what only it knows.
    pub threshold: Option<i64>,
    pub relation: Option<f64>,
    /// Men: of a levy — how many this province can give this tick, any number up to it
    /// (`Target::Levy`); of a hire — how many the advance hires.
    pub up_to: Option<u32>,
    /// What a loan brings, kg: the world decides the sum.
    pub receives_kg: Option<i64>,
    /// The dowry a proposal of marriage names, kg: **not a price of the proposal** — nothing
    /// is paid when it is made; the proposer's house pays it out of its own treasury at the
    /// wedding, if the other house takes the match.
    pub dowry_kg: Option<i64>,
}

/// What an order to a unit would be (E5, plan §31.77): the steps of the path the world
/// finds now and the ticks of the march at the world's pace, if nothing stops it. The
/// shell asks at a click and computes neither path nor adjacency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OrderOffer {
    pub steps: usize,
    pub ticks: u32,
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
