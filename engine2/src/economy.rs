//! The economy (brief §2 «Экономика», plan §16): the treasury as a flow in kilograms of
//! wheat-equivalent — production from people, tax, the court, the army's pay, loans and
//! interest — every entry through the ledger and signed with its source.
//!
//! Per polity, per tick, in canonical order of actors:
//! - **production** = Σ people × `production.output_per_head_kg` / 2 (E0 Э2) over the
//!   provinces not in revolt (E2 step 9: a province in revolt pays no tax and gives no seat
//!   its income);
//! - **tax** = production × the polity's tax rate (`Actor::tax_rate_ppm`, started at
//!   `tax.default_rate`, E0 Э4 — the tithe as the norm of an agrarian tax) × **the share
//!   collected** (E2 step 6b, review of #286): the share falls by
//!   `tax.evasion_per_legitimacy_point` for every point of legitimacy below **the norm of
//!   lawful rule** (`World::lawful_norm`: the ruler's standing before the ruler's own
//!   decisions shift it; for a usurper the standing the polity's rule of succession gives)
//!   — evasion, arrears, refusal. Lawful rule of any kind collects everything: an elected
//!   or an acclaimed ruler at its norm is not poorer than a hereditary one (the owner's
//!   correction on #286 — Venice must not be poorer than every monarchy); a polity at the
//!   top of Э4 with the legitimacy that rate leaves it stays inside Э8, since states never
//!   collected all they assigned → `Source::Tax`;
//! - **tribute** (E0 Д2, E2 step 7): a vassal sends `vassal.tribute_share` of its take to
//!   its lord, what the treasury holds of it, not while at war with the lord → a transfer
//!   signed `Source::Tribute`;
//! - **seats** (E0 Х1, `house.income_share_of_seat`): the ruling house takes its share of
//!   the tax, a governor's house its share of the province's tax — transfers to the houses'
//!   treasuries (`Source::Seat`, plan §18);
//! - **court** = tax × `court.share_of_revenue` (E0 Э5) → `Source::Court`;
//! - **interest** on the debt at `loan.interest_per_year` / 2 (E0 Э6) → `Source::Interest`:
//!   paid from the treasury when it can; what it cannot pay is added to the debt;
//! - **pay due** = soldiers × `army.pay_per_soldier_outputs` × output per head / 2 (E0 А3)
//!   over the polity's own men and the ruling house's retinue (E0 Х2, E2 step 9: the
//!   famiglia ducale was the duchy's charge — owner's rule on §21); the polity pays what the
//!   treasury holds, and — **only with a bank** — borrows the shortfall while the debt is
//!   under `loan.ceiling_years_of_revenue` × yearly revenue (E0 Э7). The paid share is the
//!   share of the army that is supplied at home (`movement.rs`); the rest melts by А7;
//! - **repayment**: what is left in the treasury after the pay goes against the debt
//!   (`Source::Repayment`), so a loan lives as long as it is needed and a polity never sits
//!   on a reserve while paying interest;
//! - **the houses' retinues** (E0 Х2): a house that does not rule pays its own men from its
//!   treasury (`Source::Upkeep` on `Account::HouseTreasury`), what it holds of the pay; its
//!   paid share is kept on the house and its unpaid men melt like the polity's
//!   (`HousePayShortfall`). The house's budget is its limit; no bank lends to a house.
//!
//! **Debt is explicit (review of PR #283).** No treasury is ever negative; what a polity
//! owes is the stock `Account::Debt`. A loan is two entries of the same amount — `+treasury`,
//! `+debt` — so the polity's net worth (treasury − debt) never grows from borrowing, and the
//! ceiling and the interest apply to the debt. Without a bank there is no debt at all
//! (owner's rule: a pay debt without a bank would be a free loan and would void the budget
//! as the limit). Interest can carry the debt past the ceiling; **default** (E2 step 6b,
//! plan §18.3): a debt past the ceiling whose interest of the tick the treasury could not
//! meet is written off (`Source::Default` on the debt alone), legitimacy pays
//! `legitimacy.cost_default`, and the bank lends nothing — not for the pay, not to the verb
//! — for `default.years_without_credit` (`Actor::credit_closed_until`; `CreditRestored`
//! when the term ends). The owner's choice on §18: a write-off with the credit closed, not
//! a suspension of interest. **The price lasts** (review of #286): the legitimacy norm sits
//! `legitimacy.norm_penalty_in_default` lower for the whole term, and a polity that
//! defaulted once pays the top of Э6 on every later loan (`Actor::defaulted`).
//!
//! The tax verbs are priced by internal politics (`politics.rs`, E2 step 6b: a step of the
//! rate inside Э4 for a price in legitimacy); `take_loan` needs the bank institution and
//! borrows up to a year of revenue within the ceiling — to spend on the same tick, since
//! the surplus is swept into repayment.
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
    /// Borrowed this tick to meet the pay, kg (bank only).
    pub borrowed: i64,
    /// Interest of the tick, kg: paid from the treasury or added to the debt.
    pub interest: i64,
    /// Debt paid down from the surplus, kg.
    pub repaid: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerbError {
    #[error("verb {0:?} is not available: {1}")]
    NotAvailable(VerbId, &'static str),
    #[error("{0:?} is not a ruler")]
    NotARuler(ActorRef),
    #[error("verb {0:?}: wrong target — {1}")]
    BadTarget(VerbId, &'static str),
    #[error("polity {0:?} has fallen")]
    Fallen(ActorId),
}

impl World {
    /// Production of a polity per tick (half a year), kg of wheat-equivalent.
    pub fn production_per_tick(&self, a: ActorId, params: &Params) -> i64 {
        self.taxable_people(a) * params.output_per_head_kg / 2
    }

    /// The people a polity taxes: those of its provinces not in revolt (E2 step 9: a
    /// province in revolt pays nothing).
    pub fn taxable_people(&self, a: ActorId) -> i64 {
        self.actors[&a].provinces.iter().filter(|p| !self.in_revolt(**p)).map(|p| self.population(*p)).sum()
    }

    /// Soldiers a polity's budget keeps paid: the revenue left after the court, over the pay
    /// of one soldier — per year, so the halves cancel. The measure of the economy (plan §16).
    pub fn sustainable_soldiers(&self, a: ActorId, params: &Params) -> i64 {
        let for_army = self.yearly_income(a, params) * (1_000_000 - params.court_share_ppm) / 1_000_000;
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
        self.yearly_income(a, params) * params.loan_ceiling_years_milli / 1000
    }

    /// How much more a polity may borrow: the ceiling less the debt it carries; nothing
    /// while the credit is closed after a default.
    pub fn loan_room(&self, a: ActorId, params: &Params) -> i64 {
        if self.credit_closed(a) {
            return 0;
        }
        (self.debt_ceiling(a, params) - self.debt(a)).max(0)
    }

    /// The interest this polity pays per tick: the band's top ever after a default.
    pub fn interest_ppm_per_tick(&self, a: ActorId, params: &Params) -> i64 {
        if self.actors[&a].defaulted {
            params.loan_interest_after_default_ppm_per_tick
        } else {
            params.loan_interest_ppm_per_tick
        }
    }

    /// Whether the bank lends nothing to this polity after its default.
    pub fn credit_closed(&self, a: ActorId) -> bool {
        self.actors[&a].credit_closed_until.is_some_and(|t| self.tick < t)
    }

    /// Default: write the debt off, pay in legitimacy, close the credit for the term.
    fn default(&mut self, a: ActorId, params: &Params) -> Result<(), WorldError> {
        let debt = self.debt(a);
        self.post(Account::Debt(a), Source::Default, -debt * 1000)?;
        let actor = self.actors.get_mut(&a).unwrap();
        actor.legitimacy = (actor.legitimacy - params.legitimacy_cost_default).clamp(0.0, 100.0);
        actor.credit_closed_until = Some(crate::time::Tick(self.tick.0 + params.default_ticks_without_credit));
        actor.defaulted = true;
        self.emit(Event::Default { actor: a, written_off_kg: debt as u64 });
        Ok(())
    }

    pub fn yearly_income(&self, a: ActorId, params: &Params) -> i64 {
        self.production_per_tick(a, params) * 2 * self.actors[&a].tax_rate_ppm / 1_000_000 * self.collection_share_ppm(a, params) / 1_000_000
    }

    /// The share of the assigned tax a polity collects, parts per million: everything at
    /// the norm of lawful rule and above, less by `tax.evasion_per_legitimacy_point` for
    /// every point of legitimacy below it. The gap is taken from the norm before the
    /// ruler's decisions shift it, so a tax above the tithe keeps costing collection: the
    /// greedy ruler sits at its shifted norm, far below the norm of lawful rule.
    ///
    /// **Below the bottom the shift keeps costing** (owner's rule on #292): legitimacy stops
    /// at 0, and so would the price of the next step up — the share no longer fell, the rate
    /// rose, and a usurper at legitimacy 0 raised the tax from 0.10 to 0.18 for nothing (the
    /// price vanishing at a bound, the third time after 6b). The points of the norm below
    /// zero (`World::norm_below_zero`) count as legitimacy lost: at the bottom the share is
    /// the one at the unclamped norm, and a step up there costs the share what it costs
    /// above zero.
    pub fn collection_share_ppm(&self, a: ActorId, params: &Params) -> i64 {
        let legitimacy = self.actors[&a].legitimacy - self.norm_below_zero(a, params);
        let gap = (self.lawful_norm(a, params) - legitimacy).max(0.0);
        (1_000_000 - (gap * params.tax_evasion_ppm_per_legitimacy_point as f64).round() as i64).max(0)
    }

    /// Borrow `amount` from the bank: the same figure on both accounts.
    fn borrow(&mut self, a: ActorId, amount: i64) -> Result<(), WorldError> {
        self.post(Account::Treasury(a), Source::Loan, amount * 1000)?;
        self.post(Account::Debt(a), Source::Loan, amount * 1000)?;
        self.emit(Event::Loan { actor: a, kg: amount as u64 });
        Ok(())
    }

    /// The economy phase of a tick. Returns each polity's budget; the paid share is stored
    /// on the actor for the supply phase.
    pub(crate) fn phase_economy(&mut self, params: &Params) -> Result<Vec<(ActorId, Budget)>, WorldError> {
        let ids: Vec<ActorId> = self.actors.keys().copied().collect();
        let mut out = Vec::with_capacity(ids.len());
        for a in ids {
            // A fallen polity has no economy: no ruler to tax for, no army to pay.
            if !self.alive(a) {
                continue;
            }
            // The term after a default ends: the bank lends again.
            if self.actors[&a].credit_closed_until.is_some_and(|t| self.tick >= t) {
                self.actors.get_mut(&a).unwrap().credit_closed_until = None;
                self.emit(Event::CreditRestored { actor: a });
            }
            let treasury = Account::Treasury(a);
            let production = self.production_per_tick(a, params);
            let rate = self.actors[&a].tax_rate_ppm;
            let share = self.collection_share_ppm(a, params);
            let income = production * rate / 1_000_000 * share / 1_000_000;
            if income > 0 {
                self.post(treasury, Source::Tax, income * 1000)?;
            }
            // Tribute (E0 Д2, E2 step 7): a vassal sends its lord a share of the take, what
            // the treasury holds of it; not while it is at war with the lord — a rebel pays
            // nothing — and not to a lord that has fallen.
            if let Some(lord) = self.actors[&a].vassal_of {
                if self.alive(lord) && !self.at_war(a, lord) {
                    let due = income * params.tribute_share_ppm / 1_000_000;
                    let paid = due.min(self.treasury(a));
                    if paid > 0 {
                        self.post(treasury, Source::Tribute, -paid * 1000)?;
                        self.post(Account::Treasury(lord), Source::Tribute, paid * 1000)?;
                    }
                    if paid < due {
                        self.emit(Event::TributeShort { vassal: a, lord, unpaid_kg: (due - paid) as u64 });
                    }
                }
            }
            // The houses' seats: the throne's share of the tax, a governorship's share of
            // its province's tax.
            let ruling_house = self.persons[&self.actors[&a].ruler].house;
            let mut seats: Vec<(crate::ids::HouseId, i64)> = vec![(ruling_house, income * params.seat_income_share_ppm / 1_000_000)];
            for p in self.actors[&a].provinces.clone() {
                if self.in_revolt(p) {
                    continue;
                }
                if let Some(g) = self.provinces[&p].governor {
                    let province_tax = self.population(p) * params.output_per_head_kg / 2 * rate / 1_000_000 * share / 1_000_000;
                    seats.push((self.persons[&g].house, province_tax * params.seat_income_share_ppm / 1_000_000));
                }
            }
            for (house, kg) in seats {
                let kg = kg.min(self.treasury(a));
                if kg > 0 {
                    self.post(treasury, Source::Seat, -kg * 1000)?;
                    self.post(Account::HouseTreasury(house), Source::Seat, kg * 1000)?;
                }
            }
            let court = income * params.court_share_ppm / 1_000_000;
            if court > 0 {
                self.post(treasury, Source::Court, -court * 1000)?;
            }
            // Interest on the debt carried in: from the treasury as far as it reaches, the
            // rest onto the debt.
            let interest = self.debt(a) * self.interest_ppm_per_tick(a, params) / 1_000_000;
            if interest > 0 {
                let from_treasury = interest.min(self.treasury(a));
                if from_treasury > 0 {
                    self.post(treasury, Source::Interest, -from_treasury * 1000)?;
                }
                if interest > from_treasury {
                    self.post(Account::Debt(a), Source::Interest, (interest - from_treasury) * 1000)?;
                    // Past the ceiling and unable to serve the debt: default.
                    if self.debt(a) > self.debt_ceiling(a, params) {
                        self.default(a, params)?;
                    }
                }
            }
            // The polity pays its own men and the ruling house's retinue; a house that does
            // not rule pays its own below (E0 Х2, owner's rule on §21).
            let units = self.units_of(a);
            let soldiers: i64 = units.iter().filter(|u| self.house_pays(**u).is_none()).map(|u| self.soldiers(*u)).sum();
            let pay_due = soldiers * params.pay_per_soldier_kg_per_year / 2;
            let shortfall = (pay_due - self.treasury(a)).max(0);
            let borrowed = shortfall.min(self.loan_room(a, params));
            if borrowed > 0 {
                self.borrow(a, borrowed)?;
            }
            let paid = pay_due.min(self.treasury(a));
            if paid > 0 {
                self.post(treasury, Source::Upkeep, -paid * 1000)?;
            }
            let share = if pay_due == 0 { 1_000_000 } else { paid * 1_000_000 / pay_due };
            let actor = self.actors.get_mut(&a).unwrap();
            actor.paid_share_ppm = share;
            actor.paid_share_history_ppm.push(share);
            let keep = params.agent_pay_memory_ticks;
            if actor.paid_share_history_ppm.len() > keep {
                actor.paid_share_history_ppm.drain(..actor.paid_share_history_ppm.len() - keep);
            }
            if paid < pay_due {
                self.emit(Event::PayShortfall { actor: a, unpaid_kg: (pay_due - paid) as u64 });
            }
            // The surplus goes against the debt.
            let repaid = self.treasury(a).min(self.debt(a));
            if repaid > 0 {
                self.post(treasury, Source::Repayment, -repaid * 1000)?;
                self.post(Account::Debt(a), Source::Repayment, -repaid * 1000)?;
            }
            // The retinues of the houses that do not rule (E0 Х2): each house pays its own
            // men from its treasury, what it holds of the pay; the unpaid share melts at
            // home by А7 (`movement.rs`) — the house's budget is its limit, as the polity's
            // is the polity's. No bank lends to a house.
            for h in self.houses_of(a) {
                let men: i64 = units.iter().filter(|u| self.house_pays(**u) == Some(h)).map(|u| self.soldiers(*u)).sum();
                let due = men * params.pay_per_soldier_kg_per_year / 2;
                let paid = due.min(self.house_treasury(h));
                if paid > 0 {
                    self.post(Account::HouseTreasury(h), Source::Upkeep, -paid * 1000)?;
                }
                self.houses.get_mut(&h).unwrap().paid_share_ppm = if due == 0 { 1_000_000 } else { paid * 1_000_000 / due };
                if paid < due {
                    self.emit(Event::HousePayShortfall { house: h, unpaid_kg: (due - paid) as u64 });
                }
            }
            out.push((a, Budget { production, income, court, pay_due, paid, borrowed, interest, repaid }));
        }
        Ok(out)
    }

    /// Apply a verb's effect (the systems compute it; E3 gives the agents). The tax verbs
    /// are internal politics' (`politics.rs`); `take_loan` needs a bank and borrows up to a
    /// year of revenue within the ceiling — on both accounts, so the polity's net worth
    /// does not change.
    pub fn apply_verb(&mut self, call: &VerbCall, params: &Params) -> Result<(), VerbError> {
        if crate::diplomacy::is_diplomatic(call.verb) {
            return self.apply_diplomacy_verb(call, params);
        }
        let ActorRef::Ruler(a) = call.actor else { return Err(VerbError::NotARuler(call.actor)) };
        if !self.alive(a) {
            return Err(VerbError::Fallen(a));
        }
        match call.verb {
            VerbId::RaiseTax | VerbId::LowerTax | VerbId::EnforceOrder => self.apply_politics_verb(call, a, params),
            VerbId::Recruit | VerbId::Disband | VerbId::HireMercenaries => self.apply_army_verb(call, a, params),
            VerbId::TakeLoan => {
                if !self.actors[&a].institutions.contains(&Institution::Bank) {
                    return Err(VerbError::NotAvailable(call.verb, "no bank: the polity has no institution to borrow from"));
                }
                if self.credit_closed(a) {
                    return Err(VerbError::NotAvailable(call.verb, "in default: no credit"));
                }
                let amount = self.yearly_income(a, params).min(self.loan_room(a, params));
                if amount <= 0 {
                    return Err(VerbError::NotAvailable(call.verb, "at the debt ceiling"));
                }
                self.borrow(a, amount).map_err(|_| VerbError::NotAvailable(call.verb, "ledger"))?;
                Ok(())
            }
            _ => Err(VerbError::NotAvailable(call.verb, "not implemented until its system (E2) and the agents (E3)")),
        }
    }
}
