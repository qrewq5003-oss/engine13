//! The economy (brief §2 «Экономика», plan §16): the treasury as a flow in kilograms of
//! wheat-equivalent — production from people, tax, the court, the army's pay, loans and
//! interest — every entry through the ledger and signed with its source.
//!
//! Per polity, per tick, in canonical order of actors:
//! - **production** = Σ people × `production.output_per_head_kg` / 2 (E0 Э2);
//! - **tax** = production × the polity's tax rate (`Actor::tax_rate_ppm`, started at
//!   `tax.default_rate`, E0 Э4 — the tithe as the norm of an agrarian tax) → `Source::Tax`;
//! - **court** = tax × `court.share_of_revenue` (E0 Э5) → `Source::Court`;
//! - **pay due** = soldiers × `army.pay_per_soldier_outputs` × output per head / 2 (E0 А3);
//!   the polity pays what it can: from the treasury, and — **only with a bank** — into debt
//!   down to `loan.ceiling_years_of_revenue` × yearly revenue (E0 Э7). **Without a bank the
//!   treasury never goes below zero** (owner's rule: a pay debt without a bank would be a
//!   free loan and would void the budget as the limit). The paid share is the share of the
//!   army that is supplied at home (`movement.rs`); the rest melts by А7;
//! - **interest** on a negative treasury at `loan.interest_per_year` / 2 (E0 Э6) → `Source::Interest`.
//!
//! The tax verbs are closed until internal politics gives them a price (plan §16 item 2);
//! `take_loan` needs the bank institution and borrows a year of revenue (plan §16 item 3).
//!
//! What this replaces: the E2 step 2 stand-in «the province feeds the unit» (plan §14). А8
//! stays as the polity-wide observed band; the measure of this system is the army a polity
//! can keep at the limit of its budget, which must fall inside А11 (owner's rule).

use crate::event::Event;
use crate::ids::ActorId;
use crate::ledger::{Account, Source};
use crate::params::Params;
use crate::verbs::{ActorRef, VerbCall, VerbId};
use crate::world::{Institution, World, WorldError};

/// A polity's budget of the tick, as the economy computed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Production of the tick, kg.
    pub production: i64,
    /// Tax taken, kg.
    pub income: i64,
    /// Court and administration, kg.
    pub court: i64,
    /// Pay the army asked for, kg.
    pub pay_due: i64,
    /// Pay actually given, kg.
    pub paid: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerbError {
    #[error("verb {0:?} is not available: {1}")]
    NotAvailable(VerbId, &'static str),
    #[error("{0:?} is not a ruler")]
    NotARuler(ActorRef),
}

impl World {
    /// Production of a polity per tick (half a year), kg of wheat-equivalent.
    pub fn production_per_tick(&self, a: ActorId, params: &Params) -> i64 {
        self.actors[&a].provinces.iter().map(|p| self.population(*p)).sum::<i64>() * params.output_per_head_kg / 2
    }

    /// Soldiers a polity's budget keeps paid: the revenue left after the court, over the pay
    /// of one soldier — per year, so the halves cancel. The measure of the economy (plan §16).
    pub fn sustainable_soldiers(&self, a: ActorId, params: &Params) -> i64 {
        let yearly_income = self.production_per_tick(a, params) * 2 * self.actors[&a].tax_rate_ppm / 1_000_000;
        let for_army = yearly_income * (1_000_000 - params.court_share_ppm) / 1_000_000;
        if params.pay_per_soldier_kg_per_year == 0 {
            0
        } else {
            for_army / params.pay_per_soldier_kg_per_year
        }
    }

    /// The debt a polity may carry: a bank's ceiling in years of revenue, nothing without one.
    pub fn debt_ceiling(&self, a: ActorId, params: &Params) -> i64 {
        if !self.actors[&a].institutions.contains(&Institution::Bank) {
            return 0;
        }
        let yearly_income = self.production_per_tick(a, params) * 2 * self.actors[&a].tax_rate_ppm / 1_000_000;
        yearly_income * params.loan_ceiling_years_milli / 1000
    }

    /// The economy phase of a tick. Returns each polity's budget; the paid share is stored
    /// on the actor for the supply phase.
    pub(crate) fn phase_economy(&mut self, params: &Params) -> Result<Vec<(ActorId, Budget)>, WorldError> {
        let ids: Vec<ActorId> = self.actors.keys().copied().collect();
        let mut out = Vec::with_capacity(ids.len());
        for a in ids {
            let treasury = Account::Treasury(a);
            let production = self.production_per_tick(a, params);
            let income = production * self.actors[&a].tax_rate_ppm / 1_000_000;
            if income > 0 {
                self.post(treasury, Source::Tax, income * 1000)?;
            }
            let court = income * params.court_share_ppm / 1_000_000;
            if court > 0 {
                self.post(treasury, Source::Court, -court * 1000)?;
            }
            let soldiers: i64 = self.units_of(a).iter().map(|u| self.soldiers(*u)).sum();
            let pay_due = soldiers * params.pay_per_soldier_kg_per_year / 2;
            let floor = -self.debt_ceiling(a, params);
            let available = (self.treasury(a) - floor).max(0);
            let paid = pay_due.min(available);
            if paid > 0 {
                self.post(treasury, Source::Upkeep, -paid * 1000)?;
            }
            let share = if pay_due == 0 { 1_000_000 } else { paid * 1_000_000 / pay_due };
            self.actors.get_mut(&a).unwrap().paid_share_ppm = share;
            if paid < pay_due {
                self.emit(Event::PayShortfall { actor: a, unpaid_kg: (pay_due - paid) as u64 });
            }
            let balance = self.treasury(a);
            if balance < 0 {
                let interest = -balance * params.loan_interest_ppm_per_tick / 1_000_000;
                if interest > 0 {
                    self.post(treasury, Source::Interest, -interest * 1000)?;
                }
            }
            out.push((a, Budget { production, income, court, pay_due, paid }));
        }
        Ok(out)
    }

    /// Apply a verb's effect (the systems compute it; E3 gives the agents). The tax verbs are
    /// closed until internal politics prices them; `take_loan` needs a bank and borrows a
    /// year of revenue if the ceiling allows.
    pub fn apply_verb(&mut self, call: &VerbCall, params: &Params) -> Result<(), VerbError> {
        let ActorRef::Ruler(a) = call.actor else { return Err(VerbError::NotARuler(call.actor)) };
        match call.verb {
            VerbId::RaiseTax | VerbId::LowerTax => Err(VerbError::NotAvailable(call.verb, "awaits internal politics: a tax without a price is a degeneration (plan §16)")),
            VerbId::TakeLoan => {
                if !self.actors[&a].institutions.contains(&Institution::Bank) {
                    return Err(VerbError::NotAvailable(call.verb, "no bank: the polity has no institution to borrow from"));
                }
                let yearly_income = self.production_per_tick(a, params) * 2 * self.actors[&a].tax_rate_ppm / 1_000_000;
                let ceiling = self.debt_ceiling(a, params);
                let room = self.treasury(a) + ceiling;
                let amount = yearly_income.min(room.max(0));
                if amount <= 0 {
                    return Err(VerbError::NotAvailable(call.verb, "at the debt ceiling"));
                }
                self.post(Account::Treasury(a), Source::Loan, amount * 1000).map_err(|_| VerbError::NotAvailable(call.verb, "ledger"))?;
                self.emit(Event::Loan { actor: a, kg: amount as u64 });
                Ok(())
            }
            _ => Err(VerbError::NotAvailable(call.verb, "not implemented until its system (E2) and the agents (E3)")),
        }
    }
}
