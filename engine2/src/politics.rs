//! Internal politics, part one (brief §2 «Внутренняя политика», plan §18, PR 6a): the house
//! and the throne — who rules, who dies, who is born, who inherits.
//!
//! Per tick, after the supply and before the population, in canonical order:
//! 1. **Deaths (E0 В2):** every living person draws once (`DrawPoint::Mortality`, keyed by
//!    person) against the yearly rate of its age band — the Frier table (Model West,
//!    levels 2–3) by seven bands, times the elite factor (в), halved for the tick. The record
//!    of a dead person stays (no dangling ids); its seats are freed, a governorship passes
//!    within the house, a house with no living member is **extinct**.
//! 2. **Violent death of a ruler (E0 В3):** one draw per living ruler of a living polity
//!    (`DrawPoint::Violence`, keyed by polity) against `В3/2 × (100 − L)/50`: at legitimacy
//!    50 the base rate, at 100 none, at 0 twice the base. The rule of succession acts
//!    through the legitimacy norm, never as a multiplier (E0 §5.4).
//!
//! **2b. Usurpation (E0 В6):** one draw per living polity whose ruler is still alive
//! (`DrawPoint::Usurpation`, keyed by polity) against `В6_base/2 × (100 − L)/50`, the
//! same shape as В3. The usurper is an adult of **any house of the polity** (owner's rule
//! on §18): the regent first (Lodovico), then a governor, then the eldest adult; with no
//! candidate there is no usurpation — a ruler out of nothing is forbidden. The ruler is
//! executed (`RulerDied { cause: Execution }`), the usurper takes the throne with the
//! standing `Usurper` (`Succession { how: Usurper }`), and legitimacy falls to the
//! usurper's norm if it was above it. The rule of succession acts through the norm,
//! never as a multiplier (E0 §5.4).
//!
//! 3. **Succession:** a polity whose ruler died this tick gets its next ruler — the
//!    designated heir if alive, else by the rule ([`Succession`]): by blood the line by
//!    seniority (descendants first, elder lines first, then the nearest collateral line);
//!    by acclamation or election the eldest adult of the ruling house, a child only if no
//!    adult is left. When the ruling house has no living member the throne passes to
//!    **another house of the polity** (owner's rule on §18: the house role must be able to
//!    take the throne): governors first, then the eldest adult, then the eldest living
//!    member of any house of the polity; the standing is `Acclaimed` (or `Elected` under
//!    election). When no house of the polity has a living person, **a new house rises from
//!    the people** (owner's rule on #285: the extinction of a dynasty never killed a state —
//!    someone of the nobility or the army took the throne; a masterless land would be a
//!    `ReadAbsent`): a founder with a household of two or three adults (`DrawPoint::Rise`;
//!    new dynasties rose from standing families, not from one man — owner's decision after
//!    the measured chain of one-founder houses dying out) is posted off the population of
//!    the capital (`Source::Rise`, the one place persons leave the stocks for the records —
//!    no one out of nothing); the founder takes the throne `Acclaimed` (`NewDynasty`). A
//!    polity falls only by conquest (§17) and, later, by collapse. The one exception: a
//!    polity with nobody in any of its provinces has no people to rise from and falls.
//! 4. **Regency (E0 В1):** a ruler under `majority.age` has a regent — the eldest adult of
//!    the ruler's house, else of any house of the polity; the regency ends at majority
//!    (`RegencyEnded`) or passes on the regent's death. A general mechanism, no dates: who
//!    the regent is at the start is a fact of the scenario.
//! 5. **Births (E0 В4):** every living person of fertile age (16–45, the window the row's
//!    unit is defined on) draws once (`DrawPoint::Birth`, keyed by person) at
//!    `births.per_adult_per_year / 2`; a child is a new person of the same house, named in
//!    the world's `Names` (decision of #278: names live in the world). The rate is В4's
//!    children per ruler **per couple**, halved: the half that marries out of the house is
//!    not modelled (owner's rule on §18, so a house does not double every generation).
//! 6. **Legitimacy:** drawn toward the norm of the ruler's standing (hereditary, confirmed,
//!    elected, acclaimed; regency while the ruler is a minor) at `legitimacy.rate_per_tick`
//!    of the gap. The only dynamics of legitimacy in 6a; the prices come with 6b.
//!
//! **Legitimacy as a price (6b, plan §18.3).** The tax verbs: `raise_tax` moves the rate up
//! by `tax.step` inside the band of Э4 and costs `legitimacy.cost_raise_tax`; `lower_tax`
//! moves it down and gives `legitimacy.gain_lower_tax` back. The offer carries the price
//! (`VerbOffer::cost_legitimacy`); at the edge of the band the verb is closed with its
//! reason. **A lasting decision shifts the norm** (brief §2; review of #286): the norm falls
//! by `legitimacy.norm_per_tax_step` for every step of the rate above the tithe and rises
//! for every step below, as long as the rate holds — a high tax is never paid for once. A
//! default lowers the norm by `legitimacy.norm_penalty_in_default` for the whole term of
//! the closed credit (`economy.rs`). `enforce_order` buys legitimacy with the treasury:
//! `order.cost_share_of_revenue` of a year's revenue (`Source::Verb`) for `order.gain` — a
//! lift above the norm that decays back; closed when the treasury cannot pay. Legitimacy
//! stays inside 0–100. **Legitimacy costs the polity, not only the ruler** (review of
//! #286): the tax collected falls with legitimacy (`economy.rs`, the share collected), so
//! a greedy rate meets a real ceiling of income — Э8 — and not an abstract number.
//!
//! The seat income of a house (E0 Х1) is posted by the economy (`economy.rs`): a share of
//! the polity's tax for the throne, of the province's tax for a governorship, as a transfer
//! `Source::Seat` from the treasury to the house's treasury. **A house's treasury has no
//! spending in E2** (bribes went to diplomacy, house troops to the collapse system): it
//! grows without an outlet until then — recorded here so that the growth is not read as a
//! defect (owner's note on §18).

use crate::event::{Cause, Event};
use crate::ids::{ActorId, HouseId, PersonId, ProvinceId};
use crate::ledger::{Account, Source};
use crate::params::Params;
use crate::economy::VerbError;
use crate::rng::{unit, Draw, DrawPoint};
use crate::verbs::{Target, VerbCall, VerbId};
use crate::world::{House, Person, Standing, Succession, World, WorldError};

/// The fertile window in years of age: the unit of `births.per_adult_per_year` (E0 В4 gives
/// children per couple over a reproductive life), not a tunable.
const FERTILE_AGE: (i32, i32) = (16, 45);

/// The age of the founder of a house risen from the people: the middle of the fertile
/// window — an adult with a reproductive life ahead, so the new house can last. Not a
/// tunable: no row of E0 gives the age of a usurper.
const FOUNDER_AGE: i32 = (FERTILE_AGE.0 + FERTILE_AGE.1) / 2;

/// The household that rises with the founder: two or three adults (owner's decision on
/// #285), their ages around the founder's.
const HOUSEHOLD: (u64, u64) = (2, 3);
const HOUSEHOLD_AGE_OFFSETS: [i32; 3] = [-5, 5, -10];

/// The standing a succession by rule gives.
pub fn standing_of_rule(rule: Succession) -> Standing {
    match rule {
        Succession::Blood => Standing::Hereditary,
        Succession::Mixed => Standing::Confirmed,
        Succession::Election => Standing::Elected,
        Succession::Acclamation => Standing::Acclaimed,
    }
}

impl World {
    /// Age in whole years at the current tick.
    pub fn age(&self, p: PersonId) -> i32 {
        self.year() - self.persons[&p].born
    }

    pub fn person_alive(&self, p: PersonId) -> bool {
        self.persons.get(&p).is_some_and(|x| x.died.is_none())
    }

    pub fn is_adult(&self, p: PersonId, params: &Params) -> bool {
        self.age(p) >= params.majority_age
    }

    /// Living members of a house, in id order.
    pub fn members(&self, h: HouseId) -> Vec<PersonId> {
        self.persons.iter().filter(|(_, p)| p.house == h && p.died.is_none()).map(|(id, _)| *id).collect()
    }

    /// The houses of a polity, in id order.
    pub fn houses_of(&self, a: ActorId) -> Vec<HouseId> {
        self.houses.iter().filter(|(_, h)| h.polity == a).map(|(id, _)| *id).collect()
    }

    /// Living persons of all houses of a polity, in id order.
    pub fn people_of_polity(&self, a: ActorId) -> Vec<PersonId> {
        self.persons.iter().filter(|(_, p)| p.died.is_none() && self.houses[&p.house].polity == a).map(|(id, _)| *id).collect()
    }

    /// Living adults of all houses of a polity, the ruler excluded: who can be regent.
    pub fn people_of_polity_adults(&self, a: ActorId, params: &Params) -> Vec<PersonId> {
        let ruler = self.actors[&a].ruler;
        self.people_of_polity(a).into_iter().filter(|p| *p != ruler && self.is_adult(*p, params)).collect()
    }

    /// The eldest of `candidates` (ties: the lower id), adults first when `adults_first`.
    fn eldest(&self, candidates: &[PersonId], adults_first: bool, params: &Params) -> Option<PersonId> {
        let pick = |xs: &[PersonId]| xs.iter().copied().min_by_key(|p| (self.persons[p].born, *p));
        if adults_first {
            let adults: Vec<PersonId> = candidates.iter().copied().filter(|p| self.is_adult(*p, params)).collect();
            if let Some(p) = pick(&adults) {
                return Some(p);
            }
        }
        pick(candidates)
    }

    /// The seats a person holds besides the throne: the provinces it governs.
    fn governed_by(&self, p: PersonId) -> Vec<ProvinceId> {
        self.provinces.iter().filter(|(_, pr)| pr.governor == Some(p)).map(|(id, _)| *id).collect()
    }

    /// The legitimacy norm a standing carries (E0 §1.7, kind (в) checked through В5).
    pub fn norm_of_standing(standing: Standing, params: &Params) -> f64 {
        match standing {
            Standing::Hereditary => params.legitimacy_norm_hereditary,
            Standing::Confirmed => (params.legitimacy_norm_hereditary + params.legitimacy_norm_acclaimed) / 2.0,
            Standing::Elected => params.legitimacy_norm_elected,
            Standing::Acclaimed => params.legitimacy_norm_acclaimed,
            Standing::Usurper => params.legitimacy_norm_usurper,
        }
    }

    /// The legitimacy norm of a polity's ruler by standing alone, the regency norm while
    /// the ruler is a minor: the norm before any decision of the ruler shifts it.
    pub fn standing_norm(&self, a: ActorId, params: &Params) -> f64 {
        let actor = &self.actors[&a];
        if self.person_alive(actor.ruler) && !self.is_adult(actor.ruler, params) {
            return params.legitimacy_norm_regency;
        }
        Self::norm_of_standing(actor.standing, params)
    }

    /// The norm of lawful rule in this polity: the standing's norm — or, for a usurper, the
    /// norm of the standing the polity's own rule of succession gives, since the usurper
    /// holds a throne that is owed to someone else. The share of the tax collected is
    /// measured against this (`economy.rs`): an elected or an acclaimed ruler at its own
    /// norm collects everything, as a hereditary one does (the owner's correction on
    /// #286), while a usurper is short of the collection by the whole gap to lawful rule.
    /// Measured on the test world: with the usurper's own norm (30) as the base, the greedy
    /// ruler — taxed to the top, soon usurped — collected 0.86 and gained a third over the
    /// plain one; against lawful rule it gains nothing, as before the correction.
    pub fn lawful_norm(&self, a: ActorId, params: &Params) -> f64 {
        let actor = &self.actors[&a];
        if actor.standing == Standing::Usurper {
            return Self::norm_of_standing(standing_of_rule(actor.succession), params);
        }
        self.standing_norm(a, params)
    }

    /// The norm legitimacy moves towards: the standing's norm shifted by the ruler's
    /// lasting decisions — the tax above the tithe, the closed credit.
    pub fn legitimacy_norm(&self, a: ActorId, params: &Params) -> f64 {
        let actor = &self.actors[&a];
        let steps_above_tithe = (actor.tax_rate_ppm - params.tax_default_ppm) as f64 / params.tax_step_ppm as f64;
        let in_default = if self.credit_closed(a) { params.legitimacy_norm_penalty_in_default } else { 0.0 };
        (self.standing_norm(a, params) - steps_above_tithe * params.legitimacy_norm_per_tax_step - in_default).clamp(0.0, 100.0)
    }

    /// Who would take the throne by force: the regent, else a governor, else the eldest
    /// adult of any house of the polity; never the ruler, never a child.
    pub fn usurper_candidate(&self, a: ActorId, params: &Params) -> Option<PersonId> {
        let actor = &self.actors[&a];
        if let Some(r) = actor.regent.filter(|r| self.person_alive(*r) && self.is_adult(*r, params)) {
            return Some(r);
        }
        let adults = self.people_of_polity_adults(a, params);
        let governors: Vec<PersonId> = adults.iter().copied().filter(|p| !self.governed_by(*p).is_empty()).collect();
        self.eldest(&governors, true, params).or_else(|| self.eldest(&adults, true, params))
    }

    /// The usurpation itself: the ruler executed, the usurper seated.
    fn usurp(&mut self, a: ActorId, usurper: PersonId, params: &Params) -> Result<(), WorldError> {
        let dead = self.actors[&a].ruler;
        self.die(dead, Cause::Execution, params)?;
        {
            let actor = self.actors.get_mut(&a).unwrap();
            actor.ruler = usurper;
            actor.heir = None;
            actor.regent = None;
            actor.standing = Standing::Usurper;
            actor.legitimacy = actor.legitimacy.min(params.legitimacy_norm_usurper);
        }
        self.emit(Event::Succession { actor: a, from: dead, to: usurper, how: Standing::Usurper });
        for province in self.governed_by(usurper) {
            self.pass_governorship(province, usurper, params)?;
        }
        Ok(())
    }

    /// A usurpation before a tick, by the candidate the rule names. For the tests only:
    /// `None` when the polity has no candidate, and nothing happens then.
    #[cfg(feature = "testing")]
    pub fn usurp_now(&mut self, a: ActorId, params: &Params) -> Result<Option<PersonId>, WorldError> {
        let Some(u) = self.usurper_candidate(a, params) else { return Ok(None) };
        self.usurp(a, u, params)?;
        Ok(Some(u))
    }

    /// One keyed draw of a point that draws **once per id per tick** by construction (each
    /// loop of the phase visits an id once), as a fraction in `[0, 1)`; the ordinal is
    /// therefore 0, and `draws_once_per_person` in the tests holds the claim.
    fn draw_once(&mut self, point: DrawPoint, id: u32) -> f64 {
        let d = Draw { point, tick: self.tick, id, n: self.draw_ordinal(point, id) };
        debug_assert_eq!(d.n, 0, "{point:?} draws once per id per tick");
        let value = crate::rng::draw(self.seed, d);
        self.journal_draw(d, value);
        unit(value)
    }

    /// Kill a person before a tick. For the tests only: the succession of the tick then
    /// plays out on a death the test chose rather than one the draws gave.
    #[cfg(feature = "testing")]
    pub fn kill(&mut self, p: PersonId, cause: Cause, params: &Params) -> Result<(), WorldError> {
        if !self.person_alive(p) {
            return Err(WorldError::UnknownPerson(p));
        }
        self.die(p, cause, params)
    }

    /// A person dies: the record stays, the seats are freed, the governorship passes
    /// within the house, the house may be extinct. The throne is left to the succession.
    fn die(&mut self, p: PersonId, cause: Cause, params: &Params) -> Result<(), WorldError> {
        self.persons.get_mut(&p).unwrap().died = Some(self.tick);
        self.emit(Event::PersonDied { person: p });
        let actors: Vec<ActorId> = self.actors.keys().copied().collect();
        for a in actors {
            let actor = self.actors.get_mut(&a).unwrap();
            if actor.ruler == p {
                self.emit(Event::RulerDied { actor: a, person: p, cause });
            } else {
                if actor.heir == Some(p) {
                    actor.heir = None;
                }
                if actor.regent == Some(p) {
                    actor.regent = None;
                }
            }
        }
        for province in self.governed_by(p) {
            self.pass_governorship(province, p, params)?;
        }
        let house = self.persons[&p].house;
        if self.members(house).is_empty() {
            self.emit(Event::HouseExtinct { house });
        }
        Ok(())
    }

    /// A governorship passes within the house of the holder who left it: the eldest adult
    /// member with no seat; nobody — the seat is vacant.
    fn pass_governorship(&mut self, province: ProvinceId, from: PersonId, params: &Params) -> Result<(), WorldError> {
        let house = self.persons[&from].house;
        let owner = self.provinces[&province].owner;
        let free = |w: &World, q: PersonId| w.actors[&owner].ruler != q && w.governed_by(q).is_empty() && w.is_adult(q, params);
        let candidates: Vec<PersonId> = self.members(house).into_iter().filter(|q| *q != from && free(self, *q)).collect();
        let to = self.eldest(&candidates, false, params);
        self.provinces.get_mut(&province).unwrap().governor = to;
        self.emit(Event::GovernorChanged { province, from, to });
        Ok(())
    }

    /// The next ruler by the rule, inside the ruling house (`None` when the house has no
    /// living member).
    fn heir_by_rule(&self, dead: PersonId, rule: Succession, params: &Params) -> Option<PersonId> {
        let house = self.persons[&dead].house;
        let members = self.members(house);
        if members.is_empty() {
            return None;
        }
        match rule {
            Succession::Blood | Succession::Mixed => {
                // Descendants of the dead ruler by line, then the lines of each nearer
                // ancestor in turn, then whoever of the house is eldest.
                if let Some(p) = self.first_living_in_line(dead, Some(dead)) {
                    return Some(p);
                }
                let mut up = self.persons[&dead].parent;
                while let Some(anc) = up {
                    if let Some(p) = self.first_living_in_line(anc, None) {
                        return Some(p);
                    }
                    up = self.persons[&anc].parent;
                }
                self.eldest(&members, false, params)
            }
            Succession::Acclamation | Succession::Election => self.eldest(&members, true, params),
        }
    }

    /// Depth-first over the descendants of `root` (children elder first), the root itself
    /// included unless it is `skip`; the first living person.
    fn first_living_in_line(&self, root: PersonId, skip: Option<PersonId>) -> Option<PersonId> {
        if skip != Some(root) && self.person_alive(root) {
            return Some(root);
        }
        let mut children: Vec<PersonId> = self.persons.iter().filter(|(_, p)| p.parent == Some(root)).map(|(id, _)| *id).collect();
        children.sort_by_key(|c| (self.persons[c].born, *c));
        children.into_iter().find_map(|c| self.first_living_in_line(c, skip))
    }

    /// A new house rises from the people of the polity: a founder and a household of adults
    /// leave the population of the capital (the first province of the polity with anybody
    /// in it) through the ledger and become a house of the polity; the household is as
    /// large as the province's people allow. `None` when no province of the polity has a
    /// person left.
    fn raise_house(&mut self, a: ActorId) -> Result<Option<PersonId>, WorldError> {
        let capital = self.actors[&a].capital;
        let provinces = self.actors[&a].provinces.clone();
        let Some(from) = std::iter::once(capital).chain(provinces.into_iter().filter(|p| *p != capital)).find(|p| self.population(*p) >= 1) else {
            return Ok(None);
        };
        let household = HOUSEHOLD.0 + self.draw(DrawPoint::Rise, a.0) % (HOUSEHOLD.1 - HOUSEHOLD.0 + 1);
        let persons = (1 + household).min(self.population(from) as u64);
        self.post(Account::Population(from), Source::Rise, -(persons as i64) * 1000)?;
        let house = HouseId(self.houses.len() as u32);
        let k = self.houses_of(a).len() + 1;
        let (asid, aname) = &self.names.actors[a.index()];
        let (hsid, hname) = (format!("{asid}.h{k}"), format!("{aname} new house {k}"));
        self.names.houses.push((hsid.clone(), hname.clone()));
        // The new house speaks and prays as its capital does (owner's answer on §20).
        let (culture, religion) = (self.provinces[&capital].culture, self.provinces[&capital].religion);
        self.houses.insert(house, House { polity: a, culture, religion });
        self.ledger.open(self.tick, Account::HouseTreasury(house), 0)?;
        let founder = PersonId(self.next_person);
        for i in 0..persons as usize {
            let id = PersonId(self.next_person);
            self.next_person += 1;
            self.names.persons.push((format!("{hsid}.{}", i + 1), format!("{hname} {}", i + 1)));
            let age = FOUNDER_AGE + if i == 0 { 0 } else { HOUSEHOLD_AGE_OFFSETS[(i - 1) % HOUSEHOLD_AGE_OFFSETS.len()] };
            self.persons.insert(id, Person { house, born: self.year() - age, parent: None, spouse: None, died: None });
        }
        self.emit(Event::NewDynasty { actor: a, house, founder, from, household: persons - 1 });
        Ok(Some(founder))
    }

    /// Seat the next ruler of `a` after the death of its ruler; with no house left, raise
    /// one from the people. The polity falls only when it has no people at all.
    fn succeed(&mut self, a: ActorId, params: &Params) -> Result<(), WorldError> {
        let dead = self.actors[&a].ruler;
        let rule = self.actors[&a].succession;
        let designated = self.actors[&a].heir.filter(|h| self.person_alive(*h));
        let (next, how) = if let Some(h) = designated {
            (h, standing_of_rule(rule))
        } else if let Some(p) = self.heir_by_rule(dead, rule, params) {
            (p, standing_of_rule(rule))
        } else {
            // The ruling house died out: another house of the polity, governors first.
            let people = self.people_of_polity(a);
            let governors: Vec<PersonId> = people.iter().copied().filter(|p| !self.governed_by(*p).is_empty() && self.is_adult(*p, params)).collect();
            let pick = self.eldest(&governors, true, params).or_else(|| self.eldest(&people, true, params));
            match pick {
                Some(p) => (p, if rule == Succession::Election { Standing::Elected } else { Standing::Acclaimed }),
                None => match self.raise_house(a)? {
                    Some(founder) => (founder, Standing::Acclaimed),
                    None => {
                        self.fall(a)?;
                        return Ok(());
                    }
                },
            }
        };
        {
            let actor = self.actors.get_mut(&a).unwrap();
            actor.ruler = next;
            actor.heir = None;
            actor.regent = None;
            actor.standing = how;
        }
        self.emit(Event::Succession { actor: a, from: dead, to: next, how });
        // One seat per person: a governor who takes the throne leaves the governorship.
        for province in self.governed_by(next) {
            self.pass_governorship(province, next, params)?;
        }
        Ok(())
    }

    /// A new person of `house`, child of `parent`, named in the world.
    fn born(&mut self, parent: PersonId) -> PersonId {
        let house = self.persons[&parent].house;
        let id = PersonId(self.next_person);
        self.next_person += 1;
        let k = self.persons.values().filter(|p| p.house == house).count() + 1;
        let (hsid, hname) = &self.names.houses[house.index()];
        self.names.persons.push((format!("{hsid}.{k}"), format!("{hname} {k}")));
        self.persons.insert(id, Person { house, born: self.year(), parent: Some(parent), spouse: None, died: None });
        self.emit(Event::PersonBorn { person: id, house, parent });
        id
    }

    /// Whether a tax verb has room inside the band of Э4; the reason when it has not.
    pub fn tax_room(&self, a: ActorId, verb: VerbId, params: &Params) -> Result<(), &'static str> {
        let rate = self.actors[&a].tax_rate_ppm;
        match verb {
            VerbId::RaiseTax if rate + params.tax_step_ppm > params.tax_band_ppm.1 => Err("at the top of the tax band (Э4)"),
            VerbId::LowerTax if rate - params.tax_step_ppm < params.tax_band_ppm.0 => Err("at the bottom of the tax band (Э4)"),
            _ => Ok(()),
        }
    }

    /// What `enforce_order` costs now: the share of a year's revenue, kg.
    pub fn order_cost(&self, a: ActorId, params: &Params) -> i64 {
        self.yearly_income(a, params) * params.order_cost_share_ppm / 1_000_000
    }

    /// Pay or receive legitimacy, kept inside 0–100.
    pub(crate) fn pay_legitimacy(&mut self, a: ActorId, cost: f64) {
        let actor = self.actors.get_mut(&a).unwrap();
        actor.legitimacy = (actor.legitimacy - cost).clamp(0.0, 100.0);
    }

    /// The verbs of internal politics: `raise_tax` / `lower_tax` / `enforce_order` (no
    /// target). Called by `apply_verb` for a living ruler.
    pub(crate) fn apply_politics_verb(&mut self, call: &VerbCall, a: ActorId, params: &Params) -> Result<(), VerbError> {
        match call.verb {
            VerbId::RaiseTax | VerbId::LowerTax => {
                if call.target != Target::None {
                    return Err(VerbError::BadTarget(call.verb, "the tax verbs take no target"));
                }
                self.tax_room(a, call.verb, params).map_err(|why| VerbError::NotAvailable(call.verb, why))?;
                let from_ppm = self.actors[&a].tax_rate_ppm;
                let (to_ppm, cost) = if call.verb == VerbId::RaiseTax {
                    (from_ppm + params.tax_step_ppm, params.legitimacy_cost_raise_tax)
                } else {
                    (from_ppm - params.tax_step_ppm, -params.legitimacy_gain_lower_tax)
                };
                self.actors.get_mut(&a).unwrap().tax_rate_ppm = to_ppm;
                self.pay_legitimacy(a, cost);
                self.emit(Event::TaxRateChanged { actor: a, from_ppm, to_ppm });
                Ok(())
            }
            VerbId::EnforceOrder => {
                if call.target != Target::None {
                    return Err(VerbError::BadTarget(call.verb, "enforce_order takes no target"));
                }
                let cost = self.order_cost(a, params);
                if cost <= 0 {
                    return Err(VerbError::NotAvailable(call.verb, "no revenue to pay from"));
                }
                if self.treasury(a) < cost {
                    return Err(VerbError::NotAvailable(call.verb, "cannot pay"));
                }
                self.post(Account::Treasury(a), Source::Verb, -cost * 1000).map_err(|_| VerbError::NotAvailable(call.verb, "ledger"))?;
                self.pay_legitimacy(a, -params.order_gain);
                self.emit(Event::OrderEnforced { actor: a, kg: cost as u64 });
                Ok(())
            }
            _ => Err(VerbError::NotAvailable(call.verb, "not a verb of internal politics")),
        }
    }

    /// The phase of the house and the throne.
    pub(crate) fn phase_people(&mut self, params: &Params) -> Result<(), WorldError> {
        let year = self.year();
        // 1. Deaths, every living person in id order.
        let living: Vec<PersonId> = self.persons.iter().filter(|(_, p)| p.died.is_none()).map(|(id, _)| *id).collect();
        for p in &living {
            let age = year - self.persons[p].born;
            let per_tick = params.mortality_per_year(age) * params.mortality_elite_factor / 2.0;
            if self.draw_once(DrawPoint::Mortality, p.0) < per_tick {
                self.die(*p, Cause::Natural, params)?;
            }
        }
        // 2. Violent death of the rulers of living polities.
        let actors: Vec<ActorId> = self.actors.keys().copied().collect();
        for a in &actors {
            if !self.alive(*a) {
                continue;
            }
            let legitimacy = self.actors[a].legitimacy;
            let per_tick = params.violent_death_per_tick * (100.0 - legitimacy) / 50.0;
            if self.draw_once(DrawPoint::Violence, a.0) < per_tick {
                let ruler = self.actors[a].ruler;
                self.die(ruler, Cause::Murder, params)?;
            }
        }
        // 2b. Usurpation: a living polity whose ruler survived the tick so far.
        for a in &actors {
            if !self.alive(*a) {
                continue;
            }
            let legitimacy = self.actors[a].legitimacy;
            let per_tick = params.usurpation_per_tick * (100.0 - legitimacy) / 50.0;
            if self.draw_once(DrawPoint::Usurpation, a.0) < per_tick {
                if let Some(u) = self.usurper_candidate(*a, params) {
                    self.usurp(*a, u, params)?;
                }
            }
        }
        // 3. Successions: a polity with land whose ruler died this tick.
        for a in &actors {
            let ruler = self.actors[a].ruler;
            let died_now = self.persons[&ruler].died == Some(self.tick);
            if died_now && !self.actors[a].provinces.is_empty() {
                self.succeed(*a, params)?;
            }
        }
        // 4. Regency: a minor on the throne has a regent (a dead regent was cleared by
        // `die`, so the seat is filled again in the same tick); majority ends it.
        for a in &actors {
            if !self.alive(*a) {
                continue;
            }
            let ruler = self.actors[a].ruler;
            let minor = !self.is_adult(ruler, params);
            let regent = self.actors[a].regent.filter(|r| self.person_alive(*r));
            if minor && regent.is_none() {
                let house = self.persons[&ruler].house;
                let of_house: Vec<PersonId> = self.members(house).into_iter().filter(|p| *p != ruler && self.is_adult(*p, params)).collect();
                let of_polity: Vec<PersonId> = self.people_of_polity(*a).into_iter().filter(|p| *p != ruler && self.is_adult(*p, params)).collect();
                let pick = self.eldest(&of_house, true, params).or_else(|| self.eldest(&of_polity, true, params));
                self.actors.get_mut(a).unwrap().regent = pick;
                if let Some(r) = pick {
                    self.emit(Event::Regency { actor: *a, ward: ruler, regent: r });
                }
            } else if !minor && self.actors[a].regent.is_some() {
                self.actors.get_mut(a).unwrap().regent = None;
                self.emit(Event::RegencyEnded { actor: *a, ruler });
            }
        }
        // 5. Births, every living person of fertile age in id order (persons born this tick
        // are not of age).
        for p in &living {
            if !self.person_alive(*p) {
                continue;
            }
            let age = year - self.persons[p].born;
            if !(FERTILE_AGE.0..=FERTILE_AGE.1).contains(&age) {
                continue;
            }
            if self.draw_once(DrawPoint::Birth, p.0) < params.births_per_adult_per_tick {
                self.born(*p);
            }
        }
        // 6. Legitimacy toward the norm.
        for a in &actors {
            if !self.alive(*a) {
                continue;
            }
            let norm = self.legitimacy_norm(*a, params);
            let actor = self.actors.get_mut(a).unwrap();
            actor.legitimacy += (norm - actor.legitimacy) * params.legitimacy_rate_per_tick;
        }
        Ok(())
    }
}
