//! Integer stocks behind a ledger (plan §1, §7).
//!
//! A stock changes only through [`Ledger::post`], and every posting is signed with its
//! [`Source`]. The budget of any stock over a tick is therefore a sum, not a probe.
//!
//! **Remainder carry (owner's rule, plan §1).** Postings arrive in thousandths
//! (`milli`): the whole part moves the stock, the fractional part accumulates per account
//! and is carried into the next posting. A slow process such as 0.1 % a year on a province of
//! 400 people (0.2 people a tick) therefore accumulates instead of rounding to zero forever.
//! The carry is deterministic and needs no randomness.
//!
//! Invariant checked by [`Ledger::check`] and by the tests: for every account, over the
//! period since the last [`Ledger::drain_postings`],
//! `Σ postings (in milli) == (stock_after − stock_before) * 1000 + (carry_after − carry_before)`.
//! The ledger keeps the period's opening balances itself, so a posting made between two
//! ticks (a verb's cost, a famine the test injects) is covered like one made inside a tick.
//!
//! **No stock goes below zero (owner's rule, E1b; debt made explicit on the review of
//! PR #283).** A posting that would take a stock below zero is **clamped** — the stock stops
//! at zero, the amount that actually left is what gets journaled, and [`Posted::clamped`]
//! tells the caller so the world can emit a `BoundHit` (brief §2: a bound is a sentinel
//! flag, not a crash). A treasury is never negative: what a polity owes is a stock of its
//! own, [`Account::Debt`], so that a loan is two entries (+treasury, +debt) and no source
//! but the tax can put wheat into a treasury without a matching entry elsewhere.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, HouseId, ProvinceId, UnitId};
use crate::time::Tick;

/// A stock the ledger keeps. One variant per kind of owner so that a posting cannot be
/// addressed to the wrong table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Account {
    /// People living in a province.
    Population(ProvinceId),
    /// Soldiers of a unit.
    Soldiers(UnitId),
    /// A polity's treasury, kilograms of wheat-equivalent; never negative.
    Treasury(ActorId),
    /// What a polity owes its creditors, kilograms of wheat-equivalent: grows by loans and
    /// by interest the treasury could not pay, shrinks by repayment (E0 Э6, Э7).
    Debt(ActorId),
    /// A house's own treasury, kilograms of wheat-equivalent.
    HouseTreasury(HouseId),
}

/// Why a stock changed. The closed list of the engine's flows; systems of E2 use these,
/// a new flow adds a variant here, not a bare number elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Source {
    /// The scenario's starting fact.
    Opening,
    /// Natural growth of population toward capacity.
    Growth,
    Famine,
    Plague,
    MigrationIn,
    MigrationOut,
    Tax,
    /// Court and administration (E0 Э5).
    Court,
    /// A house's income from a seat it holds (E0 Х1): `−` on the polity's treasury and `+`
    /// on the house's, the same figure.
    Seat,
    /// The army's pay (E0 А3).
    Upkeep,
    Loot,
    /// The levy: `−` on a province's people and `+` on a unit's soldiers, the same figure.
    Recruit,
    /// Disbanding: a levy goes back to the people of the province (`−` soldiers, `+`
    /// people); mercenaries leave the world (`−` soldiers only, owner's rule on §17).
    Disband,
    /// Mercenaries hired: `+` soldiers from outside the world and `−` the advance on the
    /// treasury (E0 А4) — the one inflow of men without people behind it, signed as such.
    Hire,
    /// A province's people lost to foraging armies (E0 П6).
    Ruin,
    Desertion,
    BattleLoss,
    Tribute,
    /// Borrowing: `+` on the treasury and `+` on the debt, the same amount.
    Loan,
    /// Interest on the debt: `−` on the treasury when it can pay, else `+` on the debt.
    Interest,
    /// Paying the debt down: `−` on the treasury and `−` on the debt, the same amount.
    Repayment,
    /// A verb's own cost, e.g. a bribe.
    Verb,
    /// Used by tests of the ledger itself.
    Test,
}

/// One signed change of one account, in thousandths of the stock's unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Posting {
    pub tick: Tick,
    pub account: Account,
    pub source: Source,
    /// Thousandths: `+1500` is one and a half people / kilograms.
    pub milli: i64,
}

/// A stock with its carried remainder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Balance {
    whole: i64,
    /// Always in `0..1000` (so a negative posting borrows from the whole part).
    carry: i64,
}

/// All stocks of the world and the postings of the current tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    /// Serialized as a sorted list of pairs: JSON keys must be strings, and `Account` is an
    /// enum. Order is the `BTreeMap` order, so the bytes are deterministic.
    #[serde(with = "pairs")]
    balances: BTreeMap<Account, Balance>,
    /// Balances when the current period began (the last drain), for the invariant.
    #[serde(with = "pairs")]
    at_period_start: BTreeMap<Account, Balance>,
    /// Postings of the period being played; drained into the tick report.
    postings: Vec<Posting>,
}

/// `BTreeMap<Account, Balance>` as a list of pairs (see the field doc).
mod pairs {
    use super::{Account, Balance};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S: Serializer>(m: &BTreeMap<Account, Balance>, s: S) -> Result<S::Ok, S::Error> {
        m.iter().map(|(a, b)| (*a, *b)).collect::<Vec<_>>().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<Account, Balance>, D::Error> {
        let v: Vec<(Account, Balance)> = Vec::deserialize(d)?;
        let mut m = BTreeMap::new();
        for (a, b) in v {
            if m.insert(a, b).is_some() {
                return Err(serde::de::Error::custom(format!("duplicate account {a:?}")));
            }
        }
        Ok(m)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LedgerError {
    #[error("account {0:?} is not open")]
    NotOpen(Account),
    #[error("account {0:?} is already open")]
    AlreadyOpen(Account),
    #[error("account {account:?}: Σ postings {sum} ≠ stock delta {delta} (milli)")]
    Unbalanced { account: Account, sum: i64, delta: i64 },
}

/// What a posting did: the thousandths actually applied (equal to the request unless the
/// stock was clamped at zero) and whether it was clamped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Posted {
    pub applied_milli: i64,
    pub clamped: bool,
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    pub fn new() -> Self {
        Ledger { balances: BTreeMap::new(), at_period_start: BTreeMap::new(), postings: Vec::new() }
    }

    /// Open an account with its starting stock, journaled as [`Source::Opening`].
    pub fn open(&mut self, tick: Tick, account: Account, whole: i64) -> Result<(), LedgerError> {
        if self.balances.contains_key(&account) {
            return Err(LedgerError::AlreadyOpen(account));
        }
        self.balances.insert(account, Balance { whole: 0, carry: 0 });
        self.at_period_start.insert(account, Balance { whole: 0, carry: 0 });
        self.post(tick, account, Source::Opening, whole * 1000).map(|_| ())
    }

    /// The whole stock of an account, `None` if it is not open.
    pub fn stock(&self, account: Account) -> Option<i64> {
        self.balances.get(&account).map(|b| b.whole)
    }

    /// The carried remainder of an account in thousandths, `None` if it is not open.
    pub fn carry(&self, account: Account) -> Option<i64> {
        self.balances.get(&account).map(|b| b.carry)
    }

    /// All open accounts in id order.
    pub fn accounts(&self) -> impl Iterator<Item = Account> + '_ {
        self.balances.keys().copied()
    }

    /// Post a signed change in thousandths. The only way a stock moves. A posting that would
    /// take a stock below zero is clamped: the stock and its carry go to zero, the journaled
    /// amount is what actually left, and the result says `clamped`.
    pub fn post(&mut self, tick: Tick, account: Account, source: Source, milli: i64) -> Result<Posted, LedgerError> {
        let b = self.balances.get_mut(&account).ok_or(LedgerError::NotOpen(account))?;
        let total = b.carry + milli;
        // Euclidean division keeps the carry in 0..1000 for negative totals too.
        let whole = b.whole + total.div_euclid(1000);
        let posted = if whole < 0 {
            // Everything there was leaves; nothing more can.
            let applied = -(b.whole * 1000 + b.carry);
            b.whole = 0;
            b.carry = 0;
            Posted { applied_milli: applied, clamped: true }
        } else {
            b.whole = whole;
            b.carry = total.rem_euclid(1000);
            Posted { applied_milli: milli, clamped: false }
        };
        self.postings.push(Posting { tick, account, source, milli: posted.applied_milli });
        Ok(posted)
    }

    /// Postings of the current tick, in posting order.
    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }

    /// Hand the period's postings over (to the tick report) and start the next period
    /// from the current balances.
    pub fn drain_postings(&mut self) -> Vec<Posting> {
        self.at_period_start = self.balances.clone();
        std::mem::take(&mut self.postings)
    }

    /// The ledger invariant for the period since the last drain.
    pub fn check(&self) -> Result<(), LedgerError> {
        let mut sums: BTreeMap<Account, i64> = BTreeMap::new();
        for p in &self.postings {
            *sums.entry(p.account).or_insert(0) += p.milli;
        }
        for (account, b) in &self.balances {
            let b0 = self.at_period_start.get(account).copied().unwrap_or(Balance { whole: 0, carry: 0 });
            let delta = (b.whole - b0.whole) * 1000 + (b.carry - b0.carry);
            let sum = sums.get(account).copied().unwrap_or(0);
            if sum != delta {
                return Err(LedgerError::Unbalanced { account: *account, sum, delta });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Account = Account::Population(ProvinceId(1));

    #[test]
    fn carry_accumulates_instead_of_rounding_to_zero() {
        let mut l = Ledger::new();
        l.open(Tick(0), P, 400).unwrap();
        // 0.1 % a year on 400 people = 0.4 a year = 0.2 a tick = 200 milli.
        for t in 1..=100 {
            l.post(Tick(t), P, Source::Growth, 200).unwrap();
        }
        assert_eq!(l.stock(P), Some(420));
        assert_eq!(l.carry(P), Some(0));
    }

    #[test]
    fn negative_postings_borrow_and_keep_carry_in_range() {
        let mut l = Ledger::new();
        l.open(Tick(0), P, 10).unwrap();
        l.post(Tick(1), P, Source::Test, -1500).unwrap();
        assert_eq!(l.stock(P), Some(8));
        assert_eq!(l.carry(P), Some(500));
        l.post(Tick(1), P, Source::Test, -500).unwrap();
        assert_eq!(l.stock(P), Some(8));
        assert_eq!(l.carry(P), Some(0));
    }

    #[test]
    fn invariant_holds_and_catches_tampering() {
        let mut l = Ledger::new();
        l.open(Tick(0), P, 100).unwrap();
        l.check().unwrap();
        l.drain_postings();
        l.post(Tick(1), P, Source::Test, 2345).unwrap();
        l.post(Tick(1), P, Source::Test, -45).unwrap();
        l.check().unwrap();
        // Tamper: a posting that never happened.
        l.postings.pop();
        assert!(matches!(l.check(), Err(LedgerError::Unbalanced { .. })));
    }

    #[test]
    fn people_are_clamped_at_zero_but_treasury_can_go_negative() {
        let mut l = Ledger::new();
        l.open(Tick(0), P, 3).unwrap();
        l.post(Tick(0), P, Source::Test, 250).unwrap(); // 3.25 people
        l.drain_postings();
        let posted = l.post(Tick(1), P, Source::Famine, -3500).unwrap();
        assert_eq!(posted, Posted { applied_milli: -3250, clamped: true });
        assert_eq!(l.stock(P), Some(0));
        assert_eq!(l.carry(P), Some(0));
        assert_eq!(l.postings()[0].milli, -3250, "the journal holds what actually left");
        l.check().unwrap();
        let exact = l.post(Tick(1), P, Source::Famine, 0).unwrap();
        assert!(!exact.clamped);
        // A treasury is clamped like any stock: debt is its own account, not a minus here.
        let t = Account::Treasury(ActorId(0));
        l.open(Tick(0), t, 10).unwrap();
        let overdrawn = l.post(Tick(1), t, Source::Upkeep, -25_000).unwrap();
        assert_eq!(overdrawn, Posted { applied_milli: -10_000, clamped: true });
        assert_eq!(l.stock(t), Some(0));
    }

    #[test]
    fn unknown_account_is_refused() {
        let mut l = Ledger::new();
        assert_eq!(l.post(Tick(0), P, Source::Test, 1), Err(LedgerError::NotOpen(P)));
        l.open(Tick(0), P, 1).unwrap();
        assert_eq!(l.open(Tick(0), P, 1), Err(LedgerError::AlreadyOpen(P)));
    }
}
