//! What a decision is worth (E3 (е1), owner's rules after #293, plan §24.4): **wheat over the
//! horizon**, one currency for every choice of a bot, in shares of the deciding actor's own
//! yearly income — the polity for the ruler's verbs, the house for the house's decisions
//! (owner's answers on §24.7). Reached through **the world's own formulas, called, never
//! copied**: the collection (`economy::collection_share`), the seats
//! (`governor_seat_income`, `house.income_share_of_seat`), the battle (`army::win_chance`),
//! the levy of a rising (`province_levy`), the hazards of usurpation, murder and a rising
//! (`politics::usurpation_hazard`, `violent_death_hazard`, `collapse::rising_hazard`), the
//! drift of legitimacy to its
//! norm (`politics::drift_toward`) and its saturation (`politics::saturate`), the push of a
//! relation (`diplomacy::pushed_relation`), the mortality table (`politics::natural_death_hazard`),
//! the migration by the fill the tax leaves (`migration_fills`, `migration_wanted`,
//! `migrants_to_close`).
//! A change of the world's
//! numbers changes the bots' worth by itself, with no change to the bots (tested by
//! injection, `tests/agents.rs`).
//!
//! **The horizon** is one for every worth (`goal.horizon_years`), without a discount (owner's
//! answer 1). **Legitimacy** is worth what the world turns it into for the polity: the take
//! it collects, and the usurpations, murders and risings it invites — each at what the world
//! then does to the polity (the usurper's norm, the successor's standing, the provinces a
//! house carries away, the blow of a revolt), not at the life of the person on the throne
//! (owner's answer 2). **The throne and independence** are worth the
//! seats they bring the house against the seats it gives up, and losing them — the execution,
//! the rising crushed — is the throne and the seats lost, with no price on a life (owner's
//! answer 3). Houses have no goals yet, so a house's «survive» does not enter (E4).
//!
//! **The bot sees what a player would see** (owner's rule 2026-10-17, plan §28.4, §28.7):
//! positions, owners, wars and who declared them, treaties, and what the view and the
//! tick's events show — relations, legitimacy, treasuries, population, the tax rate, the
//! dowry an offer names, the pay shortfalls and the battles. **Not** another's goals, its
//! house's treasury, its count of the dead (`manpower_deficit`) or its orders — the registry
//! of the hidden. A model of another from visible facts (what the throne is worth to a
//! candidate's house, `throne_worth`) is allowed: a player reasons so too; calling another's
//! decision on its hidden state is not. Guarded by injection and by the sources
//! (`tests/agents.rs`: `the_bot_reads_nothing_of_another_that_a_player_would_not_see`,
//! `the_bots_code_names_no_hidden_state_of_another`).
//!
//! **The war** (E3 (е2), plan §26–§28): what a war wins by the goals (`war_stake`) at the
//! odds of the battle formula over the sides of the war (`war_odds`; the sides by
//! `war_sides`, as the rule of the peace reads them), what it loses by the province the
//! enemy's units would reach first (`expected_target`), held as long as it is not taken back
//! nor rises; the soldiers the battle costs and the pay of the campaign; and the defence of
//! a threatened province (`defence_worth`) in the same currency — the bot's orders weigh the
//! three duties (`agents::bot_orders`).

use crate::army::{battle_losses_ppm, win_chance};
use crate::collapse::rising_hazard;
use crate::economy::{campaign_pay_per_tick, collection_share, pay_per_tick, tribute_due};
use crate::ids::{ActorId, HouseId, PersonId, ProvinceId};
use crate::params::Params;
use crate::politics::{drift_toward, natural_death_hazard, saturate, usurpation_hazard, violent_death_hazard};
use crate::world::{Standing, WarAim, World};

/// The terms of `a`'s war with `b` that do not move with the strength `a` puts in (E3 (е2),
/// plan §28.8 — read once per decision, `World::war_terms`; the worth at any strength is
/// `World::war_worth_of`): the stake of the goals (kg a year); the land at risk, the
/// quickest to fall first, each province's take as the enemy would hold it (kg a year) — the
/// enemy takes it one battle at a time, so the k-th falls with the odds of k battles lost
/// (plan §28.9, owner's rule (B)) — as many as the enemy can take within the war's expected
/// length, each a march and a siege (plan §28.12, `conquests_within`); the legitimacy of
/// ceding a province (years of income);
/// the tribute a submission would cost (kg a year) and the provinces still to lose before
/// it; the soldiers and what a head pays; the pay of the campaign; the war's expected length
/// in ticks.
#[derive(Debug, Clone, PartialEq)]
pub struct WarTerms {
    pub b: ActorId,
    pub ticks: f64,
    stake: f64,
    at_risk: Vec<f64>,
    blow: f64,
    tribute: f64,
    to_submit: u32,
    soldiers: f64,
    head: f64,
    /// Years of income the take the dead would have paid is short per head and per year of
    /// the shortfall; the ticks of the shortfall by the growth rule, in years.
    dead_years: f64,
    /// The pay of the campaign beyond the pay at home, years of income per unit of strength
    /// sent for the war's length; the strength the ruler would send (the units it commands).
    campaign_per_strength: f64,
    campaign_strength: f64,
    income: f64,
    years: f64,
    /// The relation of the pair at the threshold of the treaties: the rule of the peace
    /// (`diplomacy::peace_rule`) grants either side's peace at the first offer, so the war
    /// wins nothing — as with an unpaid army (plan §28.12, owner's decision 2026-10-20).
    at_threshold: bool,
}

/// What the risks legitimacy invites would cost a polity now, wheat — a chance of usurpation
/// taken, a murder of the ruler, a rising in each province that can rise (its marks and its
/// loss) — read once per decision (`World::risk_prices`): they depend on the state, not on
/// the course a decision compares.
#[derive(Debug, Clone, PartialEq)]
pub struct RiskPrices {
    pub usurpation: f64,
    pub murder: f64,
    pub risings: Vec<(u32, f64)>,
}

/// A polity's legitimacy as the worth follows it: the rating, the norm it moves toward, the
/// pressure the collection reads, at a tax rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Course {
    pub legitimacy: f64,
    pub norm: f64,
    pub pressure: f64,
    pub rate_ppm: i64,
}

impl World {
    /// The course of a polity as it stands.
    pub fn course(&self, a: ActorId, params: &Params) -> Course {
        Course { legitimacy: self.actors[&a].legitimacy, norm: self.legitimacy_norm(a, params), pressure: self.unclamped_legitimacy_norm(a, params), rate_ppm: self.actors[&a].tax_rate_ppm }
    }

    /// The course after a step of the tax: the rate moved, the norm and the pressure shifted
    /// by the step (saturating and summed, as `legitimacy_norm` and the pressure shift them),
    /// and — with `blow` — the step's own blow or gain applied as `pay_legitimacy` applies it.
    pub fn course_after_tax_step(&self, a: ActorId, up: bool, blow: bool, params: &Params) -> Course {
        let from = self.course(a, params);
        let (tax, default) = self.norm_shifts(a, params);
        let shift = if up { -params.legitimacy_norm_per_tax_step } else { params.legitimacy_norm_per_tax_step };
        let standing = self.standing_norm(a, params);
        let cost = self.tax_step_cost(a, up, params);
        Course {
            legitimacy: if blow { saturate(from.legitimacy, -cost) } else { from.legitimacy },
            norm: saturate(saturate(standing, tax + shift), default),
            pressure: standing + tax + shift + default,
            rate_ppm: from.rate_ppm + if up { params.tax_step_ppm } else { -params.tax_step_ppm },
        }
    }

    /// The take a year at a point of a course: the production a year at the rate, times the
    /// share collected at the legitimacy the collection reads.
    fn take_at(&self, a: ActorId, production_per_year: f64, legitimacy: f64, c: &Course, params: &Params) -> f64 {
        let share = self.collection_share_for(a, legitimacy - (c.norm - c.pressure), params) as f64 / 1e6;
        production_per_year * c.rate_ppm as f64 / 1e6 * share
    }

    /// Along a course over the horizon: the wheat taken, the expected chances of usurpation,
    /// the expected murders of the ruler, and the expected loss to risings (wheat; `risings`
    /// are the provinces' marks and losses, `rising_losses`), legitimacy drifting toward the
    /// norm tick by tick.
    fn along(&self, a: ActorId, c: &Course, risings: &[(u32, f64)], params: &Params) -> (f64, f64, f64, f64) {
        let production = self.production_per_tick(a, params) as f64 * 2.0;
        let (mut l, mut take, mut usurp, mut murder, mut risen) = (c.legitimacy, 0.0, 0.0, 0.0, 0.0);
        for _ in 0..params.goal_horizon_ticks {
            take += self.take_at(a, production, l, c, params) / 2.0;
            usurp += usurpation_hazard(l, params);
            murder += violent_death_hazard(l, params);
            risen += risings.iter().map(|(marks, loss)| rising_hazard(*marks, l, params) * loss).sum::<f64>();
            l = drift_toward(l, c.norm, params);
        }
        (take, usurp, murder, risen)
    }

    /// What a rising would cost the polity, wheat, for every province that can rise (not the
    /// capital, not in revolt), with its marks (`collapse.rs`): with a house to carry it that
    /// would take the chance (`independence_worth`), the take of the provinces the house
    /// governs, for half the horizon on average, times the house's chance to hold out; with
    /// none, the revolt's blow (`legitimacy.cost_revolt`) at the take it costs while the rating
    /// comes back (the tax the province withholds until it is put down is the suppression's,
    /// E3 (е2)). A house that would decline costs nothing.
    fn rising_losses(&self, a: ActorId, params: &Params) -> Vec<(u32, f64)> {
        let capital = self.actors[&a].capital;
        let course = self.course(a, params);
        let blow = Course { legitimacy: saturate(course.legitimacy, -params.legitimacy_cost_revolt), ..course };
        let revolt = self.along(a, &course, &[], params).0 - self.along(a, &blow, &[], params).0;
        let rate = self.actors[&a].tax_rate_ppm as f64 / 1e6;
        let share = self.collection_share_ppm(a, params) as f64 / 1e6;
        let years = f64::from(params.goal_horizon_ticks) / 4.0;
        let mut carried: Vec<(HouseId, f64)> = Vec::new();
        let mut out = Vec::new();
        for p in &self.actors[&a].provinces {
            if *p == capital || self.in_revolt(*p) || self.population(*p) < 1 {
                continue;
            }
            let loss = match self.secession_carrier(*p) {
                Some(h) => match carried.iter().find(|(x, _)| *x == h) {
                    Some((_, loss)) => *loss,
                    None => {
                        let loss = if self.independence_worth(h, a, *p, params) > 0.0 {
                            let gone: Vec<ProvinceId> = self.actors[&a].provinces.iter().copied().filter(|q| *q != capital && self.provinces[q].governor.is_some_and(|g| self.persons[&g].house == h)).collect();
                            let retinue: i64 = self.units_of_house(h).iter().map(|u| self.soldiers(*u)).sum();
                            let own = gone.iter().map(|q| self.province_levy(*q, params)).sum::<i64>() + retinue;
                            let parent = (self.army_size(a) - retinue).max(0);
                            let hold = if parent == 0 { 1.0 } else { win_chance(own as f64, parent as f64, params) };
                            hold * gone.iter().map(|q| self.province_production_per_tick(*q, params) as f64 * 2.0 * rate * share).sum::<f64>() * years
                        } else {
                            0.0
                        };
                        carried.push((h, loss));
                        loss
                    }
                },
                None => revolt,
            };
            out.push((self.alienation_marks(*p), loss));
        }
        out
    }

    /// The take a year at the equilibrium of a norm and a pressure (the rating at its norm).
    fn take_settled(&self, a: ActorId, norm: f64, pressure: f64, rate_ppm: i64, params: &Params) -> f64 {
        let c = Course { legitimacy: norm, norm, pressure, rate_ppm };
        self.take_at(a, self.production_per_tick(a, params) as f64 * 2.0, norm, &c, params)
    }

    /// The take a year at the settled norm of a ruler of standing norm `base`, under the
    /// polity's lasting shifts as they stand.
    fn take_under(&self, a: ActorId, base: f64, params: &Params) -> f64 {
        let (tax, default) = self.norm_shifts(a, params);
        self.take_settled(a, saturate(saturate(base, tax), default), base + tax + default, self.actors[&a].tax_rate_ppm, params)
    }

    /// The take a year of the polity at its own settled norm.
    fn take_now(&self, a: ActorId, params: &Params) -> f64 {
        self.take_settled(a, self.legitimacy_norm(a, params), self.unclamped_legitimacy_norm(a, params), self.actors[&a].tax_rate_ppm, params)
    }

    /// Years a person is expected to live within the horizon by the mortality table (the
    /// elite's), with no other hazard.
    pub fn expected_years(&self, p: PersonId, params: &Params) -> f64 {
        let age = self.age(p);
        let mut alive = 1.0;
        let mut years = 0.0;
        for t in 0..params.goal_horizon_ticks {
            years += alive / 2.0;
            alive *= 1.0 - natural_death_hazard(age + (t / 2) as i32, params).clamp(0.0, 1.0);
        }
        years
    }

    /// What a chance of usurpation would cost the polity now, wheat: the take lost while the
    /// usurper reigns — the usurper's norm under the ruler's lasting shifts, against the
    /// polity's — for the candidate's expected years within the horizon. Nothing when no
    /// candidate exists or the candidate's house would decline (the world opens the chance,
    /// the house decides: `throne_worth`).
    pub fn usurpation_loss(&self, a: ActorId, params: &Params) -> f64 {
        let Some(c) = self.usurper_candidate(a, params) else { return 0.0 };
        if self.throne_worth(a, c, params) <= 0.0 {
            return 0.0;
        }
        (self.take_now(a, params) - self.take_under(a, params.legitimacy_norm_usurper, params)) * self.expected_years(c, params)
    }

    /// What a murder of the ruler would cost the polity now, wheat: the successor's standing
    /// against the polity's — a minor's regency until majority, an adult's standing of the
    /// rule for half the horizon on average (a usurper murdered gives way to lawful rule: a
    /// gain, negative). Nothing without a successor.
    pub fn murder_loss(&self, a: ActorId, params: &Params) -> f64 {
        let Some(next) = self.next_ruler(a, params) else { return 0.0 };
        let horizon_years = f64::from(params.goal_horizon_ticks) / 2.0;
        let (base, years) = if self.is_adult(next, params) {
            (World::norm_of_standing(crate::politics::standing_of_rule(self.actors[&a].succession), params), horizon_years / 2.0)
        } else {
            (params.legitimacy_norm_regency, f64::from((params.majority_age - self.age(next)).max(0)).min(horizon_years))
        };
        (self.take_now(a, params) - self.take_under(a, base, params)) * years
    }

    /// The prices of the risks of a polity as it stands.
    pub fn risk_prices(&self, a: ActorId, params: &Params) -> RiskPrices {
        RiskPrices { usurpation: self.usurpation_loss(a, params), murder: self.murder_loss(a, params), risings: self.rising_losses(a, params) }
    }

    /// The worth to a polity of moving from one course to another, in years of its income:
    /// the take gained over the horizon, less the usurpations, murders and risings the move
    /// invites, each at what it would cost the polity.
    pub fn course_worth(&self, a: ActorId, from: &Course, to: &Course, prices: &RiskPrices, params: &Params) -> f64 {
        let (t0, ..) = self.along(a, from, &[], params);
        let (t1, ..) = self.along(a, to, &[], params);
        (t1 - t0) / self.yearly_income(a, params).max(1) as f64 + self.course_risk(a, from, to, prices, params)
    }

    /// The risk part of a move alone — the usurpations, murders and risings it invites — for
    /// a decision whose take is valued as a flow already (the tax, #291).
    pub fn course_risk(&self, a: ActorId, from: &Course, to: &Course, prices: &RiskPrices, params: &Params) -> f64 {
        let (_, u0, m0, r0) = self.along(a, from, &prices.risings, params);
        let (_, u1, m1, r1) = self.along(a, to, &prices.risings, params);
        let income = self.yearly_income(a, params).max(1) as f64;
        (-(u1 - u0) * prices.usurpation - (m1 - m0) * prices.murder - (r1 - r0)) / income
    }

    /// The worth of a one-off blow (`points` > 0) or gain (< 0) of legitimacy to the polity,
    /// saturating as the world applies it.
    pub fn blow_worth(&self, a: ActorId, points: f64, prices: &RiskPrices, params: &Params) -> f64 {
        let from = self.course(a, params);
        let to = Course { legitimacy: saturate(from.legitimacy, -points), ..from };
        self.course_worth(a, &from, &to, prices, params)
    }

    /// What the flight a tax rate drives or draws is worth to a polity, in years of its income
    /// (owner's rule after #295: people flee the heavier tax): the people the world's
    /// migration would move across its border a tick (`migration_wanted`) at `rate_ppm`
    /// against the rate as it stands — each flow until the gap it follows closes to the
    /// threshold (`migrants_to_close`) — absent or present for the rest of the horizon, at
    /// what a head pays the polity at that rate.
    pub fn flight_worth(&self, a: ActorId, rate_ppm: i64, params: &Params) -> f64 {
        let people: i64 = self.actors[&a].provinces.iter().map(|p| self.population(*p)).sum();
        let now_rate = self.actors[&a].tax_rate_ppm;
        if people <= 0 || now_rate <= 0 {
            return 0.0;
        }
        let now = self.migration_fills(params);
        let mut after = now.clone();
        let mut involved: std::collections::BTreeSet<ProvinceId> = std::collections::BTreeSet::new();
        for p in &self.actors[&a].provinces {
            if !self.in_revolt(*p) {
                after.insert(*p, self.migration_fill_ppm(*p, rate_ppm, params));
            }
            involved.insert(*p);
            involved.extend(self.provinces[p].neighbours.iter().copied().filter(|n| self.provinces[n].owner != a));
        }
        let ticks = f64::from(params.goal_horizon_ticks);
        let burden = |q: ProvinceId, changed: bool| if changed && self.provinces[&q].owner == a && !self.in_revolt(q) { rate_ppm } else { self.tax_burden_ppm(q) };
        // People-ticks the polity gains (> 0) or loses over the horizon under `fills`.
        let held = |fills: &std::collections::BTreeMap<ProvinceId, i64>, changed: bool| -> f64 {
            let mut total = 0.0;
            for q in &involved {
                let Some((to, gap, milli)) = self.migration_wanted(*q, fills, params) else { continue };
                let (out, into) = (self.provinces[q].owner == a, self.provinces[&to].owner == a);
                if out == into || milli <= 0 {
                    continue;
                }
                let per_tick = milli as f64 / 1000.0;
                let movable = self.migrants_to_close(*q, to, gap, (burden(*q, changed), burden(to, changed)), params);
                let until = movable / per_tick;
                let person_ticks = if until >= ticks { per_tick * ticks * ticks / 2.0 } else { movable * (ticks - until / 2.0) };
                total += if into { person_ticks } else { -person_ticks };
            }
            total
        };
        let delta = held(&after, true) - held(&now, false);
        // A head pays the polity its share of the take: the yearly income over the people, at
        // the new rate; in years of income that is the people-ticks over twice the people.
        delta * rate_ppm as f64 / now_rate as f64 / (2.0 * people as f64)
    }

    // ---- war (E3 (е2), plan §26, §28) ----

    /// The take a province would give `a` a year: its production at `a`'s rate and share.
    pub(crate) fn take_of(&self, a: ActorId, q: ProvinceId, params: &Params) -> f64 {
        self.province_production_per_tick(q, params) as f64 * 2.0 * self.actors[&a].tax_rate_ppm as f64 / 1e6 * self.collection_share_ppm(a, params) as f64 / 1e6
    }

    /// What the live goals of `a` want of `b`, kg a year: the provinces of `b` they name, each
    /// once, at `a`'s take, held as long as `b` does not take them back nor they rise (plan
    /// §27.3, (б)); the tribute `b` would pay as a vassal; the tribute `a` pays `b` as its lord.
    pub fn war_stake(&self, a: ActorId, b: ActorId, params: &Params) -> f64 {
        use crate::agents::{GoalKind as K, GoalTarget as T};
        let mut wanted: std::collections::BTreeSet<ProvinceId> = std::collections::BTreeSet::new();
        let (mut subdue, mut free) = (false, false);
        let of_b = |q: &ProvinceId| self.provinces[q].owner == b;
        for g in &self.actors[&a].goals {
            if self.goal_progress(a, g) >= 1.0 - 1e-9 {
                continue;
            }
            match (g.kind, g.target) {
                (K::Take | K::Hold, T::Province(q)) if of_b(&q) => {
                    wanted.insert(q);
                }
                (K::Take, T::Route(q, r)) => wanted.extend([q, r].into_iter().filter(of_b)),
                (K::Take | K::Unite | K::Hold, T::Region(r)) => wanted.extend(self.regions[r.index()].iter().copied().filter(of_b)),
                (K::Take, T::Polity(x)) if x == b => wanted.extend(self.actors[&b].provinces.iter().copied()),
                (K::Subdue, T::Polity(x)) if x == b => subdue = true,
                (K::Free, _) if self.lord_of(a) == Some(b) => free = true,
                _ => {}
            }
        }
        let back = self.retake_odds(a, b, a, self.war_ticks(a, b, params), params);
        let mut kg: f64 = wanted.iter().map(|q| self.take_of(a, *q, params) * (1.0 - back) * (1.0 - self.revolt_share(a, *q, params))).sum();
        if subdue {
            kg += tribute_due(self.yearly_income(b, params), params) as f64 * (1.0 - back);
        }
        if free {
            kg += tribute_due(self.yearly_income(a, params), params) as f64;
        }
        kg
    }

    /// The odds that `b` takes back what `a` takes from it (plan §27.3, (б)), as `by` can
    /// know them: by the battle formula, each side's army and what it can levy over the
    /// war's expected length `ticks` — the rate of a tick (А2) times the ticks, within the
    /// room (plan §28.12, owner's decision 2: not the whole ceiling, which А2 reaches only
    /// over many ticks) — `by`'s own count exactly (`recruit_per_tick`, `recruit_room`),
    /// another's as anyone sees it (`levy_per_tick_seen`, `levy_room_seen`: the ceiling by
    /// its people less its dead by the battles seen, plan §28.7).
    pub fn retake_odds(&self, a: ActorId, b: ActorId, by: ActorId, ticks: f64, params: &Params) -> f64 {
        if !self.alive(b) {
            return 0.0;
        }
        let could = |x: ActorId| {
            let (per_tick, room) = if x == by { (self.recruit_per_tick(x, params), self.recruit_room(x, params)) } else { (self.levy_per_tick_seen(x, params), self.levy_room_seen(x, params)) };
            let raised = ((per_tick as f64 * ticks).floor() as i64).min(room).max(0);
            self.army_strength(x) + World::new_soldiers_strength(raised)
        };
        let (theirs, mine) = (could(b), could(a));
        if theirs <= 0.0 {
            return 0.0;
        }
        if mine <= 0.0 {
            return 1.0;
        }
        win_chance(theirs, mine, params)
    }

    /// The share of the horizon a province taken by `a` would stand in revolt (plan §27.3,
    /// (б)): its marks of alienation against `a`'s court and core, the chance of a rising at
    /// `a`'s legitimacy (`collapse::rising_hazard`) each tick, each rising lasting until `a`'s
    /// men reach it from the capital (`path`, `march_steps_per_tick`).
    pub fn revolt_share(&self, a: ActorId, q: ProvinceId, params: &Params) -> f64 {
        let pr = &self.provinces[&q];
        let marks = u32::from(pr.religion != self.polity_religion(a)) + u32::from(pr.culture != self.actors[&a].core_culture);
        let steps = self.path(a, self.actors[&a].capital, q).map(|p| p.len()).unwrap_or(0);
        let lasting = (steps.div_ceil(self.march_steps_per_tick(params).max(1)).max(1)) as f64;
        (rising_hazard(marks, self.actors[&a].legitimacy, params) * lasting).min(1.0)
    }

    /// The odds of `a` in its war with `b`, by the battle formula: the strength of its side
    /// (and `extra`) against the other's, each polity counted once. The sides are the war's
    /// (`war_sides`, plan §28.1 — the same the rule of the peace reads: for a war not yet
    /// open, those a declaration brings in by §19); against `a` besides them every other
    /// enemy it already fights, one army for every front; with `a` its allies, lord and
    /// vassals already at war with `b`.
    pub fn war_odds(&self, a: ActorId, b: ActorId, extra: f64, params: &Params) -> f64 {
        use std::collections::BTreeSet;
        let (side_a, side_b) = self.war_sides(a, b);
        let mut theirs: BTreeSet<ActorId> = side_b.into_iter().collect();
        theirs.extend(self.actors.keys().copied().filter(|c| self.alive(*c) && self.at_war(a, *c)));
        let mut mine: BTreeSet<ActorId> = side_a.into_iter().collect();
        let mut friends: Vec<ActorId> = self.allies_of(a);
        friends.extend(self.lord_of(a));
        friends.extend(self.vassals_of(a));
        mine.extend(friends.into_iter().filter(|x| self.alive(*x) && self.at_war(*x, b)));
        mine.retain(|x| !theirs.contains(x));
        let mine = (mine.iter().map(|x| self.army_strength(*x)).sum::<f64>() + extra).max(0.0);
        let theirs: f64 = theirs.iter().map(|x| self.army_strength(*x)).sum();
        if theirs <= 0.0 {
            return if mine > 0.0 { 1.0 } else { 0.5 };
        }
        if mine <= 0.0 {
            return 0.0;
        }
        win_chance(mine, theirs, params)
    }

    /// The terms of `a`'s war with `b` that do not move with the strength `a` puts in
    /// (`WarTerms`), read once per decision: what its goals want of `b` (`war_stake`); what
    /// a lost war costs (plan §28.9, owner's rule (B), all of it visible) — against a claim
    /// `b` stated, the province it named, or for the polity's submission the land within
    /// `b`'s reach and the tribute of Д2; with no claim against `a` (a war `a` declares or
    /// would declare, an entrant's war), the provinces of `a` within the reach of `b`'s units
    /// by the movement rule over the war's expected length (`land_in_reach`), as many as `b`
    /// can take in that length, a march and a siege each (`conquests_within`, plan §28.12) —
    /// each held by `b` as long as `a` does not take it back nor it rises (plan §27.3, (б)),
    /// and each ceded at the legitimacy the peace charges (`blow_per_province`, years of
    /// income); the
    /// soldiers the battle would cost and what a head pays; the pay of the campaign when `a`'s
    /// goals want the war (plan §27.2); the war's expected length (`war_ticks`).
    pub fn war_terms(&self, a: ActorId, b: ActorId, blow_per_province: f64, params: &Params) -> WarTerms {
        let people: i64 = self.actors[&a].provinces.iter().map(|q| self.population(*q)).sum();
        let income = self.yearly_income(a, params).max(1) as f64;
        let ticks = self.war_ticks(a, b, params);
        let key = crate::army::pair(a, b);
        let claim = self.claim_of(b, a);
        // The land at risk: a claimed province; else what the enemy's units reach in the war's length.
        let at_risk: Vec<ProvinceId> = match claim {
            Some(WarAim::Province(q)) => self.provinces[&q].owner.eq(&a).then_some(q).into_iter().collect(),
            _ => {
                // The quickest to fall first: the march to each (a tick at least, as `war_ticks`
                // counts a march) and its siege — a capital a step away stands its siege while
                // the land behind it falls within the tick (the test world's case).
                let per_tick = self.march_steps_per_tick(params).max(1);
                let mut reach: Vec<(usize, usize, ProvinceId)> = self.land_in_reach(b, a, ticks.ceil() as usize, params).into_iter().map(|(q, d)| (d.div_ceil(per_tick).max(1) + self.siege_ticks(q, params) as usize, d, q)).collect();
                reach.sort();
                let reach: Vec<(usize, ProvinceId)> = reach.into_iter().map(|(_, d, q)| (d, q)).collect();
                let n = self.conquests_within(ticks, &reach, params);
                reach.into_iter().take(n).map(|(_, q)| q).collect()
            }
        };
        let back = 1.0 - self.retake_odds(b, a, a, ticks, params);
        let at_risk: Vec<f64> = at_risk.iter().map(|q| self.take_of(a, *q, params) * back * (1.0 - self.revolt_share(b, *q, params))).collect();
        // A claim on the polity: the tribute of Д2 once `a` is down to half the land it held
        // when the war began (`make_peace_between`) — the provinces still to lose until then.
        let (tribute, to_submit) = match claim {
            Some(WarAim::Polity(_)) => {
                let held = self.wars.get(&key).map(|w| if a == key.0 { w.held.0 } else { w.held.1 }).unwrap_or(0);
                (tribute_due(self.yearly_income(a, params), params) as f64, (self.actors[&a].provinces.len() as u32).saturating_sub(held / 2).max(1))
            }
            _ => (0.0, 1),
        };
        WarTerms {
            b,
            ticks,
            stake: self.war_stake(a, b, params),
            at_risk,
            blow: blow_per_province,
            tribute,
            to_submit,
            soldiers: self.army_size(a) as f64,
            head: if people > 0 { self.yearly_income(a, params) as f64 / people as f64 } else { 0.0 },
            dead_years: self.regrowth_shortfall_ticks(a, params) / 2.0,
            campaign_per_strength: if self.war_goal_against(a, b) { self.campaign_extra_per_strength(a, ticks, params) } else { 0.0 },
            campaign_strength: self.units.keys().filter(|u| self.commands(crate::verbs::ActorRef::Ruler(a), **u)).map(|u| self.unit_strength(*u)).sum(),
            income,
            years: f64::from(params.goal_horizon_ticks) / 2.0,
            at_threshold: self.relation(a, b) >= params.treaty_accept_at_relation,
        }
    }

    /// How many of the provinces `reach` (the steps to each from the enemy's units, in the
    /// order they fall) the enemy can take within `ticks`, taking them one after another as the
    /// world does (plan §28.12, owner's decision 1): each costs the march to it from the one
    /// before by the movement rule (`march_steps_per_tick`, a tick at least — as `war_ticks`
    /// counts a march) and its siege (`siege_ticks`: a province falls within the tick, a
    /// capital after `siege.capital_ticks`). The k-th conquest needs k marches and k sieges:
    /// the chain of (1−p)^k is cut where the war's expected length runs out.
    pub fn conquests_within(&self, ticks: f64, reach: &[(usize, ProvinceId)], params: &Params) -> usize {
        let per_tick = self.march_steps_per_tick(params).max(1);
        let (mut spent, mut from) = (0.0, 0usize);
        for (k, (steps, q)) in reach.iter().enumerate() {
            spent += steps.saturating_sub(from).div_ceil(per_tick).max(1) as f64 + f64::from(self.siege_ticks(*q, params));
            if spent > ticks {
                return k;
            }
            from = *steps;
        }
        reach.len()
    }

    /// The legitimacy a province ceded at a peace would cost `a`, in years of its income
    /// (`diplomacy::ceded_province_cost` at what the world turns a blow of legitimacy into,
    /// `blow_worth`): the price of every province at risk in a war (plan §28.9).
    pub fn ceded_province_blow(&self, a: ActorId, risk: &RiskPrices, params: &Params) -> f64 {
        -self.blow_worth(a, Self::ceded_province_cost(params), risk, params)
    }

    /// What `a`'s war on the terms `t` is worth to `a` with `extra` strength put in, in years
    /// of its income (plan §26): won — the stake for the horizon; lost — the land at risk,
    /// the enemy taking it one battle at a time, the k-th province with the odds of k battles
    /// lost (as many as it can take in the war's length, `war_terms`), for the horizon as the
    /// enemy holds it, with the legitimacy of ceding it, and the
    /// tribute of a submission once enough is lost (plan §28.9); the soldiers the battle
    /// costs (А6), heads gone from the tax base while the people grow back by the world's
    /// growth rule (`regrowth_shortfall_ticks`; their place in the cohort of service is the
    /// ceiling's, `levy_ceiling`, counted once); the pay of the campaign for the strength sent
    /// (plan §28.10, owner's rules: each priced as the world charges it). The
    /// odds are the battle formula's over the sides of the war (`war_odds`). `unpaid`: an
    /// unpaid army is exhausted and the peace comes at the first offer — the goals are not
    /// reached; so at the threshold of the relation (`WarTerms::at_threshold`). `extra`:
    /// strength added to `a`'s (a levy, a hire; a unit sent away, negative).
    /// Leaving the war is a decision of its own — the peace, worth what going on would lose,
    /// granted by the rule of the peace — not a discount on the loss (plan §28.8): a bot that
    /// counted on leaving a lost war for nothing never left it, since the same valuation told
    /// it to fight on while the enemy took its land (on the test world a war at five to one
    /// stayed worth fighting).
    pub fn war_worth_of(&self, a: ActorId, t: &WarTerms, unpaid: bool, extra: f64, params: &Params) -> f64 {
        let p = self.war_odds(a, t.b, extra, params);
        let gain = if unpaid || t.at_threshold { 0.0 } else { p * t.stake * t.years };
        let q = 1.0 - p;
        let (mut lose, mut blow, mut odds) = (0.0, 0.0, 1.0);
        for take in &t.at_risk {
            odds *= q;
            lose += odds * take * t.years;
            blow += odds * t.blow;
        }
        lose += q.powi(t.to_submit as i32) * t.tribute * t.years;
        let lost = t.soldiers * (p * battle_losses_ppm(true, params) as f64 + (1.0 - p) * battle_losses_ppm(false, params) as f64) / 1e6;
        let campaign = t.campaign_per_strength * (t.campaign_strength + extra).max(0.0);
        (gain - lose - lost * t.head * t.dead_years) / t.income - campaign - blow
    }

    /// What `a`'s war with `b` is worth to `a` (`war_terms`, `war_worth_of`), the terms and
    /// the prices of the risks read here; a decision that asks several times reads them once.
    pub fn war_worth(&self, a: ActorId, b: ActorId, unpaid: bool, extra: f64, params: &Params) -> f64 {
        let blow = self.ceded_province_blow(a, &self.risk_prices(a, params), params);
        self.war_worth_of(a, &self.war_terms(a, b, blow, params), unpaid, extra, params)
    }

    /// What holding a threatened province `p` against `by` is worth to `a`, in years of its
    /// income (plan §28.2): its take for the horizon, held by `by` as long as `a` does not
    /// take it back nor it rises (the hold of §27.3), at the chance that defenders of
    /// `defenders` strength beat the `threat` that can reach it, by the battle formula —
    /// nothing without a defender (an unopposed enemy takes a province in a tick, E0 А9).
    pub fn defence_worth(&self, a: ActorId, p: ProvinceId, by: ActorId, defenders: f64, threat: f64, params: &Params) -> f64 {
        if defenders <= 0.0 || threat <= 0.0 {
            return 0.0;
        }
        let income = self.yearly_income(a, params).max(1) as f64;
        let years = f64::from(params.goal_horizon_ticks) / 2.0;
        let held = (1.0 - self.retake_odds(by, a, a, self.war_ticks(a, by, params), params)) * (1.0 - self.revolt_share(by, p, params));
        win_chance(defenders, threat, params) * self.take_of(a, p, params) * held * years / income
    }

    /// What a unit of strength on campaign for `ticks` costs a polity beyond its pay at home,
    /// in years of its income (`campaign_pay_per_tick` against `pay_per_tick`, as the world
    /// charges the units off their land for the march and the siege, plan §28.10).
    pub fn campaign_extra_per_strength(&self, a: ActorId, ticks: f64, params: &Params) -> f64 {
        let income = self.yearly_income(a, params).max(1) as f64;
        let n = 1000;
        (campaign_pay_per_tick(n, params) - pay_per_tick(n, params)) as f64 * ticks / income / World::new_soldiers_strength(n)
    }

    /// The ticks `a`'s war on `b` is expected to take: the march from `a`'s capital to the
    /// province it aims at, as the march goes in that war whether or not it is on
    /// (`path_at_war`, `march_steps_per_tick`), and the siege there (`siege_ticks`).
    pub fn war_ticks(&self, a: ActorId, b: ActorId, params: &Params) -> f64 {
        let target = self.war_target(a, b);
        let steps = self.path_at_war(a, b, self.actors[&a].capital, target).map(|p| p.len()).unwrap_or(0);
        (steps.div_ceil(self.march_steps_per_tick(params).max(1)).max(1) as f64) + f64::from(self.siege_ticks(target, params))
    }

    /// What `n` soldiers kept under arms for `ticks` cost a polity, in years of its income:
    /// their pay (`pay_per_tick`) and the take of the heads they are away from the land.
    pub fn soldiers_cost(&self, a: ActorId, n: i64, ticks: f64, params: &Params) -> f64 {
        let income = self.yearly_income(a, params).max(1) as f64;
        let people: i64 = self.actors[&a].provinces.iter().map(|q| self.population(*q)).sum();
        let head = if people > 0 { income / people as f64 } else { 0.0 };
        (pay_per_tick(n, params) as f64 * ticks + n as f64 * head * ticks / 2.0) / income
    }

    // ---- the houses ----

    /// A house's yearly income as the denominator of its worth (at least a kilogram: a house
    /// with no seat values its chances in wheat all the same).
    fn house_income(&self, h: HouseId, params: &Params) -> f64 {
        self.house_income_per_year(h, params).max(1) as f64
    }

    /// The share of a polity's take that is the throne's seat (E0 Х1).
    fn throne_share(params: &Params) -> f64 {
        params.seat_income_share_ppm as f64 / 1e6
    }

    /// What the throne is worth to the house of `candidate`, in years of the house's income
    /// (owner's answer 3 on §24.7): the throne's seat over the horizon while the house holds
    /// it — at the usurper's legitimacy, drawn to the usurper's norm, the hold lost at the next
    /// chance of usurpation (a murder or a natural death leaves the throne to the house's
    /// heir) — against the candidate's governorships, given up at the accession for the
    /// horizon. No price on a life: the execution is the throne and the seats lost.
    pub fn throne_worth(&self, a: ActorId, candidate: PersonId, params: &Params) -> f64 {
        let h = self.persons[&candidate].house;
        let (tax, default) = self.norm_shifts(a, params);
        let base = params.legitimacy_norm_usurper;
        let c = Course { legitimacy: self.actors[&a].legitimacy.min(base), norm: saturate(saturate(base, tax), default), pressure: base + tax + default, rate_ppm: self.actors[&a].tax_rate_ppm };
        let production = self.production_per_tick(a, params) as f64 * 2.0;
        let (mut l, mut hold, mut gain) = (c.legitimacy, 1.0, 0.0);
        for _ in 0..params.goal_horizon_ticks {
            gain += hold * self.take_at(a, production, l, &c, params) / 2.0 * Self::throne_share(params);
            hold *= 1.0 - usurpation_hazard(l, params).clamp(0.0, 1.0);
            l = drift_toward(l, c.norm, params);
        }
        let (rate, share) = (self.actors[&a].tax_rate_ppm, self.collection_share_ppm(a, params));
        let seats: f64 = self.governed_by(candidate).into_iter().map(|p| 2.0 * self.governor_seat_income(p, rate, share, params) as f64).sum();
        (gain - seats * f64::from(params.goal_horizon_ticks) / 2.0) / self.house_income(h, params)
    }

    /// What a polity of its own is worth to the house that would carry a secession from `a`
    /// (`collapse::secede`), in years of the house's income: what the house would hold there
    /// — the new throne's seat on the take of the provinces it governs, and the governorships
    /// its members keep (the new ruler's own pass to a free member of the house, if there is
    /// one) — the new polity acclaimed, collecting at the parent's rate under the acclaimed
    /// norm with the rate's shift; times the chance to hold out (the battle formula: the
    /// militia of its provinces at the ceiling of А1 and the house's men against the parent's
    /// army); against every seat the house holds now (gone in either outcome).
    pub fn independence_worth(&self, h: HouseId, a: ActorId, province: ProvinceId, params: &Params) -> f64 {
        let capital = self.actors[&a].capital;
        let provinces: Vec<ProvinceId> = self.actors[&a].provinces.iter().copied().filter(|q| *q != capital && self.provinces[q].governor.is_some_and(|g| self.persons[&g].house == h)).collect();
        let Some(ruler) = self.provinces[&province].governor else { return 0.0 };
        let retinue: i64 = self.units_of_house(h).iter().map(|u| self.soldiers(*u)).sum();
        let own = provinces.iter().map(|q| self.province_levy(*q, params)).sum::<i64>() + retinue;
        let parent = (self.army_size(a) - retinue).max(0);
        let hold = if parent == 0 { 1.0 } else { win_chance(own as f64, parent as f64, params) };
        let (tax, _) = self.norm_shifts(a, params);
        let acclaimed = World::norm_of_standing(Standing::Acclaimed, params);
        let rate = self.actors[&a].tax_rate_ppm;
        let share = collection_share(acclaimed, acclaimed + tax, params);
        let take: f64 = provinces.iter().map(|q| self.province_production_per_tick(*q, params) as f64 * 2.0 * rate as f64 / 1e6 * share as f64 / 1e6).sum();
        let free_member = self.members(h).into_iter().any(|p| p != ruler && self.is_adult(p, params) && self.governed_by(p).is_empty());
        let kept: f64 = provinces.iter().filter(|q| free_member || self.provinces[q].governor != Some(ruler)).map(|q| 2.0 * self.governor_seat_income(*q, rate, share, params) as f64).sum();
        let years = f64::from(params.goal_horizon_ticks) / 2.0;
        let now = self.house_income_per_year(h, params) as f64;
        (hold * (take * Self::throne_share(params) + kept) * years - now * years) / self.house_income(h, params)
    }

    // ---- bonds ----

    /// The threat an alliance of `a` would answer, as `a` knows it: the polity its own goal
    /// of «alliance against» names, else the threat anyone sees (`seen_threat`).
    fn own_threat(&self, a: ActorId) -> Option<ActorId> {
        let goal = self.actors[&a].goals.iter().find_map(|g| match (g.kind, g.target) {
            (crate::agents::GoalKind::AllianceAgainst, crate::agents::GoalTarget::Polity(x)) if self.alive(x) && x != a => Some(x),
            _ => None,
        });
        goal.or_else(|| self.seen_threat(a))
    }

    /// The threat to `x` that anyone sees — a fact of the state, not of `x`'s goals (owner's
    /// rule 2026-10-17: the bot sees what a player would, and `x`'s goals are its own): the
    /// strongest enemy at war with `x` that outnumbers it (the threat of a neighbour not at
    /// war is valued in (г)).
    fn seen_threat(&self, x: ActorId) -> Option<ActorId> {
        self.actors.keys().copied().filter(|b| self.at_war(x, *b) && self.alive(*b) && self.army_size(*b) > self.army_size(x)).max_by_key(|b| (self.army_size(*b), std::cmp::Reverse(*b)))
    }

    /// The take a year of the provinces of `a` that border `t`: what a war with `t` puts at
    /// stake.
    fn stake(&self, a: ActorId, t: ActorId, params: &Params) -> f64 {
        let rate = self.actors[&a].tax_rate_ppm as f64 / 1e6;
        let share = self.collection_share_ppm(a, params) as f64 / 1e6;
        self.actors[&a]
            .provinces
            .iter()
            .filter(|p| self.provinces[p].neighbours.iter().chain(self.provinces[p].sea_neighbours.iter()).any(|n| self.provinces[n].owner == t))
            .map(|p| self.province_production_per_tick(*p, params) as f64 * 2.0 * rate * share)
            .sum()
    }

    /// What an alliance with `b` is worth to `a`, in years of its income (§19: each enters
    /// the other's war): against `a`'s threat, the battle formula's chance with `b`'s army
    /// and without it, times the take at stake there over the horizon; less, against `b`'s
    /// threat, the chance of losing with both armies times what `a` would put at stake there.
    pub fn alliance_worth(&self, a: ActorId, b: ActorId, params: &Params) -> f64 {
        if a == b || !self.alive(b) {
            return 0.0;
        }
        let army = |x: ActorId| self.army_size(x) as f64;
        let years = f64::from(params.goal_horizon_ticks) / 2.0;
        let gain = match self.own_threat(a).filter(|t| *t != b) {
            Some(t) if army(t) > 0.0 => {
                let alone = if army(a) > 0.0 { win_chance(army(a), army(t), params) } else { 0.0 };
                (win_chance(army(a) + army(b), army(t), params) - alone) * self.stake(a, t, params)
            }
            _ => 0.0,
        };
        let cost = match self.seen_threat(b).filter(|t| *t != a && !self.at_war(a, *t)) {
            Some(t) if army(t) > 0.0 => win_chance(army(t), army(a) + army(b), params) * self.stake(a, t, params),
            _ => 0.0,
        };
        (gain - cost) * years / self.yearly_income(a, params).max(1) as f64
    }

    /// The worth to `a` of pushing its relation with `b` by `push`: the alliance's worth, by
    /// the share of the way to the threshold of acceptance the push covers (the relation's
    /// other readers are its norms).
    pub fn relation_push_worth(&self, a: ActorId, b: ActorId, push: f64, params: &Params) -> f64 {
        let r = self.relation(a, b);
        let threshold = params.treaty_accept_at_relation;
        if r >= threshold {
            return 0.0;
        }
        let covered = ((crate::diplomacy::pushed_relation(r, push) - r) / (threshold - r)).min(1.0);
        covered * self.alliance_worth(a, b, params)
    }

    /// The bond a match between houses of `mine` and `theirs` brings the house ruling `mine`,
    /// in years of that house's income: the alliance a lifted norm of the relation
    /// (`relation.norm_marriage`) would open, at the throne's seat of what the alliance is
    /// worth to the polity. Nothing within one polity, between polities already bound, or
    /// for a house that does not rule.
    fn bond(&self, h: HouseId, mine: ActorId, theirs: ActorId, params: &Params) -> f64 {
        if mine == theirs || !self.alive(mine) || !self.alive(theirs) || self.ruling_house(mine) != h || self.married_across(mine, theirs) {
            return 0.0;
        }
        let norm = self.relation_norm(mine, theirs, params);
        if norm >= params.treaty_accept_at_relation || norm + params.relation_norm_marriage < params.treaty_accept_at_relation {
            return 0.0;
        }
        self.alliance_worth(mine, theirs, params) * self.yearly_income(mine, params) as f64 * Self::throne_share(params) / self.house_income(h, params)
    }

    /// What a match offered by house `from` is worth to house `g`, in years of `g`'s income:
    /// the dowry the offer names (`dowry_kg` — the offer, not the proposer's treasury, which
    /// `g` does not see) and the bond.
    pub fn marriage_worth(&self, g: HouseId, from: HouseId, dowry_kg: i64, params: &Params) -> f64 {
        let (mine, theirs) = (self.houses[&g].polity, self.houses[&from].polity);
        dowry_kg as f64 / self.house_income(g, params) + self.bond(g, mine, theirs, params)
    }

    /// What offering a match to house `g` is worth to house `h`, in years of `h`'s income:
    /// the bond, less the dowry it would pay.
    pub fn proposal_worth(&self, h: HouseId, g: HouseId, params: &Params) -> f64 {
        let (mine, theirs) = (self.houses[&h].polity, self.houses[&g].polity);
        self.bond(h, mine, theirs, params) - self.dowry(h, params) as f64 / self.house_income(h, params)
    }
}
