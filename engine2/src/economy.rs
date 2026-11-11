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
use crate::ids::{ActorId, ProvinceId};
use crate::ledger::{Account, Source};
use crate::params::Params;
use crate::verbs::{ActorRef, VerbCall, VerbId};
use crate::world::{Institution, World, WorldError};

/// What a polity's tick is due, article by article, **with no treasury read** (plan §30.9
/// p. 1, §30.12; `World::tick_dues`): the take, the tribute to its lord and from its vassals,
/// the houses' seats, the court, the interest on its debt, the pay of its men. Every input is
/// open to anyone — people, the tax rate and legitimacy, vassalage, governors, the debt and
/// its default, the armies — but the pay of a campaign, which reads the units' orders: read
/// by where the units stand (`by_position`), that too is open. The budget of the tick
/// (`World::tick_budget`) is these articles bounded by what the treasury holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dues {
    /// Production of the tick, kg.
    pub production: i64,
    /// The tax rate and the share collected, parts per million, and the take they give, kg.
    pub rate: i64,
    pub share: i64,
    pub income: i64,
    /// The lord the tribute is due to (E0 Д2): none for a polity with no lord, at war with
    /// its lord — a rebel pays nothing — or whose lord has fallen. And the tribute, kg.
    pub lord: Option<ActorId>,
    pub tribute_due: i64,
    /// The tribute its vassals owe it, kg: each living vassal's not at war with it, by the
    /// vassal's take.
    pub tribute_in: i64,
    /// The houses' seats, kg (E0 Х1): each house's, in the order of `seat_incomes`.
    pub seats: Vec<(crate::ids::HouseId, i64)>,
    /// Court and administration, kg.
    pub court: i64,
    /// Interest on the debt carried in, kg.
    pub interest: i64,
    /// The pay of the polity's own men and the ruling house's retinue, kg.
    pub pay_due: i64,
}

impl Dues {
    /// What the tick leaves free once everything due is paid, kg, never below nothing: the
    /// take and the vassals' tribute, less the tribute to the lord, the houses' seats, the
    /// court, the interest and the pay. The loan is no income, and the debt's repayment is no
    /// charge: it takes the surplus.
    pub fn free(&self) -> i64 {
        (self.income + self.tribute_in - self.tribute_due - self.seats.iter().map(|(_, kg)| kg).sum::<i64>() - self.court - self.interest - self.pay_due).max(0)
    }
}

/// A polity's budget of the tick, as the economy computes it (`World::tick_budget`) and then
/// posts it (`phase_economy`): every article in the order the phase takes it, each bounded by
/// what the treasury holds when its turn comes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    /// Production of the tick, kg.
    pub production: i64,
    /// Tax taken, kg.
    pub income: i64,
    /// The lord the tribute goes to (E0 Д2): none for a polity with no lord, at war with
    /// its lord — a rebel pays nothing — or whose lord has fallen.
    pub lord: Option<ActorId>,
    /// Tribute due to the lord, kg, and what the treasury held of it.
    pub tribute_due: i64,
    pub tribute: i64,
    /// The houses' seats as paid, kg (E0 Х1): each house's, in the order of `seat_incomes`.
    pub seats: Vec<(crate::ids::HouseId, i64)>,
    /// Court and administration, kg: what is due, and what the treasury held of it.
    pub court: i64,
    pub court_paid: i64,
    /// Interest of the tick, kg: `interest_paid` of it from the treasury, the rest onto the
    /// debt.
    pub interest: i64,
    pub interest_paid: i64,
    /// The interest carried the debt past its ceiling: the polity defaults this tick — the
    /// debt is written off and the credit closed before the army is paid.
    pub defaults: bool,
    /// Pay the army asked for, kg.
    pub pay_due: i64,
    /// Borrowed this tick to meet the pay, kg (bank only).
    pub borrowed: i64,
    /// Pay actually given, kg.
    pub paid: i64,
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

/// The pay of `soldiers` for a tick, kg (E0 Э6: `army.pay_per_soldier_outputs` a year).
pub fn pay_per_tick(soldiers: i64, params: &Params) -> i64 {
    soldiers * params.pay_per_soldier_kg_per_year / 2
}

/// The pay of `soldiers` on campaign for a tick, kg: the pay times
/// `army.campaign_pay_factor` (plan §27.2).
pub fn campaign_pay_per_tick(soldiers: i64, params: &Params) -> i64 {
    pay_per_tick(soldiers, params) * params.campaign_pay_ppm / 1_000_000
}

/// A vassal's tribute on a take of `income`, kg (E0 Д2: `vassal.tribute_share`).
pub fn tribute_due(income: i64, params: &Params) -> i64 {
    income * params.tribute_share_ppm / 1_000_000
}

/// The share collected, parts per million, under a norm of lawful rule at a legitimacy as
/// the collection reads it: everything at the norm and above, less by
/// `tax.evasion_per_legitimacy_point` for every point below — the one formula of the
/// collection, for a polity that stands and for one the worth of a secession imagines.
pub fn collection_share(lawful: f64, legitimacy_for_collection: f64, params: &Params) -> i64 {
    let gap = (lawful - legitimacy_for_collection).max(0.0);
    (1_000_000 - (gap * params.tax_evasion_ppm_per_legitimacy_point as f64).round() as i64).max(0)
}

impl World {
    /// Production of a polity per tick (half a year), kg of wheat-equivalent.
    pub fn production_per_tick(&self, a: ActorId, params: &Params) -> i64 {
        self.taxable_people(a) * params.output_per_head_kg / 2
    }

    /// One province's production a tick, kg (E0 Э2): its people at the output per head.
    pub fn province_production_per_tick(&self, p: ProvinceId, params: &Params) -> i64 {
        self.population(p) * params.output_per_head_kg / 2
    }

    /// A governorship's seat, kg a tick: its share of the province's tax at the rate and
    /// the share collected (E0 Х1) — the formula the economy pays by and the worth reads.
    pub fn governor_seat_income(&self, p: ProvinceId, rate: i64, share: i64, params: &Params) -> i64 {
        let province_tax = self.population(p) * params.output_per_head_kg / 2 * rate / 1_000_000 * share / 1_000_000;
        province_tax * params.seat_income_share_ppm / 1_000_000
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

    /// Whether a unit stands on land not its owner's: the half of `on_campaign` anyone sees
    /// (plan §30.9 p. 1, owner's decision 2026-11-11 — positions are open, orders are not).
    pub fn off_own_land(&self, u: crate::ids::UnitId) -> bool {
        let unit = &self.units[&u];
        self.provinces[&unit.position].owner != unit.owner
    }

    /// Whether a unit is on campaign (plan §27.2, owner's correction on #297): standing on
    /// land not its owner's (`off_own_land`), or ordered toward land not its owner's. A
    /// garrison at home is not, at war or not.
    pub fn on_campaign(&self, u: crate::ids::UnitId) -> bool {
        let unit = &self.units[&u];
        self.off_own_land(u) || unit.destination.is_some_and(|d| self.provinces[&d].owner != unit.owner)
    }

    /// A unit's pay for a tick, kg: the pay of its soldiers, on campaign
    /// `army.campaign_pay_factor` times as much (plan §27.2).
    pub fn unit_pay_per_tick(&self, u: crate::ids::UnitId, params: &Params) -> i64 {
        self.unit_pay_read(u, false, params)
    }

    /// A unit's pay for a tick in one of the two readings of the campaign: the rule of the
    /// world (`on_campaign` — the unit's own polity and the economy), or `by_position`, the
    /// half anyone sees (`off_own_land` — another's unit, whose orders are hidden; plan §30.9
    /// p. 1, owner's decision 2026-11-11). They part only for a unit ordered onto another's
    /// land and still standing on its own.
    fn unit_pay_read(&self, u: crate::ids::UnitId, by_position: bool, params: &Params) -> i64 {
        let soldiers = self.soldiers(u);
        let on_campaign = if by_position { self.off_own_land(u) } else { self.on_campaign(u) };
        if on_campaign {
            campaign_pay_per_tick(soldiers, params)
        } else {
            pay_per_tick(soldiers, params)
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
        self.loan_room_at(a, self.debt(a), self.credit_closed(a), params)
    }

    /// The room to borrow at a debt of `debt` with the credit `closed` or not: the one rule
    /// `loan_room` reads on the state and `tick_budget` on the debt as the tick's interest
    /// leaves it.
    fn loan_room_at(&self, a: ActorId, debt: i64, closed: bool, params: &Params) -> i64 {
        if closed {
            return 0;
        }
        (self.debt_ceiling(a, params) - debt).max(0)
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
        self.pay_legitimacy(a, params.legitimacy_cost_default);
        let actor = self.actors.get_mut(&a).unwrap();
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
    /// **The collection reads the pressure, not the saturated norm** (owner's rules on #292
    /// and #293): legitimacy saturates and never reaches zero, so the price of a step of the
    /// tax would fade near the bottom — the share would hardly fall while the rate rose (a
    /// usurper at the bottom raised the tax from 0.10 to 0.18 for nothing). The share reads
    /// legitimacy less the distance between the saturated norm and the summed pressure
    /// (`World::unclamped_legitimacy_norm`): at the norm that is the pressure itself, below
    /// zero too, and a step up costs the share what it costs anywhere — the same share as
    /// before the saturation, at every equilibrium.
    pub fn collection_share_ppm(&self, a: ActorId, params: &Params) -> i64 {
        let legitimacy = self.actors[&a].legitimacy - (self.legitimacy_norm(a, params) - self.unclamped_legitimacy_norm(a, params));
        self.collection_share_for(a, legitimacy, params)
    }

    /// The share collected at a legitimacy as the collection reads it (the rating less the
    /// distance between the saturated norm and the pressure): the one formula the economy
    /// and the bots' worth call (E3 (е1)).
    pub fn collection_share_for(&self, a: ActorId, legitimacy_for_collection: f64, params: &Params) -> i64 {
        collection_share(self.lawful_norm(a, params), legitimacy_for_collection, params)
    }

    /// The seats of a polity's houses, kg a tick (E0 Х1): the throne's share of the take, a
    /// governorship's share of its province's tax (provinces in revolt pay none) — given the
    /// take, the rate and the share collected; the one formula the economy pays by and the
    /// bots' worth reads (E3 (е1)).
    pub fn seat_incomes(&self, a: ActorId, income: i64, rate: i64, share: i64, params: &Params) -> Vec<(crate::ids::HouseId, i64)> {
        let ruling_house = self.persons[&self.actors[&a].ruler].house;
        let mut seats: Vec<(crate::ids::HouseId, i64)> = vec![(ruling_house, income * params.seat_income_share_ppm / 1_000_000)];
        for p in &self.actors[&a].provinces {
            if self.in_revolt(*p) {
                continue;
            }
            if let Some(g) = self.provinces[p].governor {
                seats.push((self.persons[&g].house, self.governor_seat_income(*p, rate, share, params)));
            }
        }
        seats
    }

    /// A house's income from its seats, kg a year, at its polity's take as it stands.
    pub fn house_income_per_year(&self, h: crate::ids::HouseId, params: &Params) -> i64 {
        let a = self.houses[&h].polity;
        if !self.alive(a) {
            return 0;
        }
        let (rate, share) = (self.actors[&a].tax_rate_ppm, self.collection_share_ppm(a, params));
        let income = self.production_per_tick(a, params) * rate / 1_000_000 * share / 1_000_000;
        2 * self.seat_incomes(a, income, rate, share, params).into_iter().filter(|(x, _)| *x == h).map(|(_, kg)| kg).sum::<i64>()
    }

    /// Borrow `amount` from the bank: the same figure on both accounts.
    fn borrow(&mut self, a: ActorId, amount: i64) -> Result<(), WorldError> {
        self.post(Account::Treasury(a), Source::Loan, amount * 1000)?;
        self.post(Account::Debt(a), Source::Loan, amount * 1000)?;
        self.emit(Event::Loan { actor: a, kg: amount as u64 });
        Ok(())
    }

    /// The take of a polity's tick: its production, the tax rate, the share collected, and
    /// the tax they give, kg — the one formula the economy collects by.
    fn take_per_tick(&self, a: ActorId, params: &Params) -> (i64, i64, i64, i64) {
        let production = self.production_per_tick(a, params);
        let rate = self.actors[&a].tax_rate_ppm;
        let share = self.collection_share_ppm(a, params);
        (production, rate, share, production * rate / 1_000_000 * share / 1_000_000)
    }

    /// The lord a polity's tribute is due to (E0 Д2, E2 step 7): its lord, while the lord
    /// lives and the two are not at war — a rebel pays nothing.
    fn tribute_lord(&self, a: ActorId) -> Option<ActorId> {
        self.actors[&a].vassal_of.filter(|lord| self.alive(*lord) && !self.at_war(a, *lord))
    }

    /// What a polity's tick is due, with no treasury read (`Dues`; plan §30.9 p. 1, §30.12):
    /// the articles `tick_budget` bounds by the treasury and `phase_economy` posts, and the
    /// tribute its vassals owe it. The pay of its men in one of two readings of the campaign
    /// (`unit_pay_read`): the rule of the world, or `by_position` — the reading of another's
    /// army, whose orders are hidden.
    pub fn tick_dues(&self, a: ActorId, by_position: bool, params: &Params) -> Dues {
        let (production, rate, share, income) = self.take_per_tick(a, params);
        // Tribute: a vassal sends its lord a share of the take; the lord's vassals send it
        // theirs.
        let lord = self.tribute_lord(a);
        let tribute_in: i64 = self.actors.keys().copied().filter(|v| self.alive(*v) && self.tribute_lord(*v) == Some(a)).map(|v| tribute_due(self.take_per_tick(v, params).3, params)).sum();
        let tribute_due = if lord.is_some() { tribute_due(income, params) } else { 0 };
        // The houses' seats: the throne's share of the tax, a governorship's share of its
        // province's tax.
        let seats = self.seat_incomes(a, income, rate, share, params);
        let court = income * params.court_share_ppm / 1_000_000;
        let interest = self.debt(a) * self.interest_ppm_per_tick(a, params) / 1_000_000;
        // The polity pays its own men and the ruling house's retinue; a house that does
        // not rule pays its own (E0 Х2, owner's rule on §21).
        let pay_due: i64 = self.units_of(a).iter().filter(|u| self.house_pays(**u).is_none()).map(|u| self.unit_pay_read(*u, by_position, params)).sum();
        Dues { production, rate, share, income, lord, tribute_due, tribute_in, seats, court, interest, pay_due }
    }

    /// The budget of a polity's tick on the state as it stands, **nothing posted** (plan §30.9
    /// p. 1, §30.11): the articles `phase_economy` posts, in its order, each bounded by what
    /// the treasury holds when its turn comes — the take; the tribute to the lord; the
    /// houses' seats; the court; the interest on the debt carried in, from the treasury as
    /// far as it reaches and the rest onto the debt, with the default past the ceiling; the
    /// pay of the polity's own men and the ruling house's retinue, with the loan that meets
    /// a shortfall within the room; the surplus against the debt. The retinues of the houses
    /// that do not rule are their houses' charge, not the polity's.
    ///
    /// **The phase calls this very function and posts what it returns**, a polity at a time
    /// in the order of the ids: a vassal's tribute is in its lord's treasury by the lord's
    /// turn only when the vassal's id is the lower (so `Dues::tribute_in` is no article of
    /// this budget: it comes in at the vassals' turns). Read at another moment it is the
    /// budget the polity would have were its turn now.
    pub fn tick_budget(&self, a: ActorId, params: &Params) -> Budget {
        // What the tick is due (`tick_dues`, the pay by the rule of the world), then each
        // article bounded by the treasury as the postings before it would leave it.
        let Dues { production, income, lord, tribute_due, tribute_in: _, seats, court, interest, pay_due, .. } = self.tick_dues(a, false, params);
        let (mut treasury, mut debt) = (self.treasury(a) + income.max(0), self.debt(a));
        // Tribute: what the treasury holds of the lord's share.
        let tribute = tribute_due.min(treasury).max(0);
        treasury -= tribute;
        // The houses' seats, each what the treasury still holds of it.
        let seats: Vec<(crate::ids::HouseId, i64)> = seats
            .into_iter()
            .map(|(house, kg)| {
                let kg = kg.min(treasury).max(0);
                treasury -= kg;
                (house, kg)
            })
            .collect();
        // The court takes what there is: a posting past the treasury stops at zero (the
        // ledger's bound).
        let court_paid = court.min(treasury).max(0);
        treasury -= court_paid;
        // Interest on the debt carried in: from the treasury as far as it reaches, the
        // rest onto the debt. Past the ceiling and unable to serve the debt: default — the
        // debt written off, the credit closed.
        let interest_paid = interest.min(treasury).max(0);
        treasury -= interest_paid;
        let mut closed = self.credit_closed(a);
        let mut defaults = false;
        if interest > interest_paid {
            debt += interest - interest_paid;
            if debt > self.debt_ceiling(a, params) {
                // The debt is written off and the credit closed for the term
                // (`default.years_without_credit`, ticks ahead): no loan this tick.
                defaults = true;
                debt = 0;
                closed = true;
            }
        }
        // The pay of the polity's men, with the loan that meets a shortfall within the room.
        let shortfall = (pay_due - treasury).max(0);
        let borrowed = shortfall.min(self.loan_room_at(a, debt, closed, params));
        treasury += borrowed;
        debt += borrowed;
        let paid = pay_due.min(treasury);
        treasury -= paid;
        // The surplus goes against the debt.
        let repaid = treasury.min(debt);
        Budget { production, income, lord, tribute_due, tribute, seats, court, court_paid, interest, interest_paid, defaults, pay_due, borrowed, paid, repaid }
    }

    /// The economy phase of a tick: every living polity's budget (`tick_budget`) posted, a
    /// polity at a time in the order of the ids. Returns each polity's budget; the paid
    /// share is stored on the actor for the supply phase.
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
            // The budget of the tick, computed on the state as the polities before this one
            // left it, and posted article by article in its order.
            let treasury = Account::Treasury(a);
            let budget = self.tick_budget(a, params);
            let (treasury_before, debt_before) = (self.treasury(a), self.debt(a));
            let Budget { income, lord, tribute_due: due, tribute, court, interest, interest_paid, defaults, pay_due, borrowed, paid, repaid, .. } = budget;
            if income > 0 {
                self.post(treasury, Source::Tax, income * 1000)?;
            }
            // Tribute (E0 Д2, E2 step 7): what the treasury holds of the lord's share.
            if let Some(lord) = lord {
                if tribute > 0 {
                    self.post(treasury, Source::Tribute, -tribute * 1000)?;
                    self.post(Account::Treasury(lord), Source::Tribute, tribute * 1000)?;
                }
                if tribute < due {
                    self.emit(Event::TributeShort { vassal: a, lord, unpaid_kg: (due - tribute) as u64 });
                }
            }
            // The houses' seats: the throne's share of the tax, a governorship's share of
            // its province's tax.
            for (house, kg) in &budget.seats {
                if *kg > 0 {
                    self.post(treasury, Source::Seat, -kg * 1000)?;
                    self.post(Account::HouseTreasury(*house), Source::Seat, kg * 1000)?;
                }
            }
            // The court is posted whole: past the treasury the ledger stops at zero and the
            // world records the bound (`BoundHit`).
            if court > 0 {
                self.post(treasury, Source::Court, -court * 1000)?;
            }
            // Interest on the debt carried in: from the treasury as far as it reaches, the
            // rest onto the debt.
            if interest > 0 {
                if interest_paid > 0 {
                    self.post(treasury, Source::Interest, -interest_paid * 1000)?;
                }
                if interest > interest_paid {
                    self.post(Account::Debt(a), Source::Interest, (interest - interest_paid) * 1000)?;
                    // Past the ceiling and unable to serve the debt: default.
                    if defaults {
                        self.default(a, params)?;
                    }
                }
            }
            // The polity pays its own men and the ruling house's retinue; a house that does
            // not rule pays its own below (E0 Х2, owner's rule on §21).
            let units = self.units_of(a);
            if borrowed > 0 {
                self.borrow(a, borrowed)?;
            }
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
            if repaid > 0 {
                self.post(treasury, Source::Repayment, -repaid * 1000)?;
                self.post(Account::Debt(a), Source::Repayment, -repaid * 1000)?;
            }
            // The budget is what was posted: the treasury and the debt stand where its
            // articles leave them.
            debug_assert_eq!(
                self.treasury(a),
                treasury_before + income.max(0) - tribute - budget.seats.iter().map(|(_, kg)| kg).sum::<i64>() - budget.court_paid - interest_paid + borrowed - paid - repaid,
                "the budget of {a:?} parts with the treasury's postings"
            );
            debug_assert_eq!(self.debt(a), if defaults { 0 } else { debt_before + interest - interest_paid } + borrowed - repaid, "the budget of {a:?} parts with the debt's postings");
            // The retinues of the houses that do not rule (E0 Х2): each house pays its own
            // men from its treasury, what it holds of the pay; the unpaid share melts at
            // home by А7 (`movement.rs`) — the house's budget is its limit, as the polity's
            // is the polity's. No bank lends to a house.
            for h in self.houses_of(a) {
                let due: i64 = units.iter().filter(|u| self.house_pays(**u) == Some(h)).map(|u| self.unit_pay_per_tick(*u, params)).sum();
                let paid = due.min(self.house_treasury(h));
                if paid > 0 {
                    self.post(Account::HouseTreasury(h), Source::Upkeep, -paid * 1000)?;
                }
                self.houses.get_mut(&h).unwrap().paid_share_ppm = if due == 0 { 1_000_000 } else { paid * 1_000_000 / due };
                if paid < due {
                    self.emit(Event::HousePayShortfall { house: h, unpaid_kg: (due - paid) as u64 });
                }
            }
            out.push((a, budget));
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
