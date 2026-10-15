//! Collapse and successors (brief §2 «распад», §2a, §5 item 6; plan §21): the core tongue of
//! a polity, the alienation of a province as the entry, the rising of a province and the
//! secession of a house with the provinces it governs into a polity of its own, the
//! successor that gets provinces and a war — not a copy of metrics — and the end of the
//! rising by the state of the war (§19: suppression, submission, freedom).
//!
//! **The core** (`Actor::core_culture`, owner's rule on §21). The faith of a province is
//! read against the **court** — a foreign dynasty took the faith of the land (#288); its
//! tongue is read against the **core**: the tongue of the majority of the polity's people,
//! not of the court (foreign-tongued courts were the norm before nationalism) and not of
//! the capital (a capital falls in four ticks of siege, and a core that moved with it would
//! alienate the whole heartland after one lost siege — a cascade of the rule, not of
//! history). The core is kept until another tongue outgrows it by `core.switch_margin` of
//! the polity's people, so a near-even polity does not flap. A province with one or two
//! marks of alienation (faith, tongue) is the entry of collapse; alienation touches the take
//! not at all (owner's answer on §20).
//!
//! **The rising (E0 В7).** One keyed draw per tick per province of a living polity — but
//! never its capital: a rising at the seat of the court is the coup of В6 — at
//! `DrawPoint::Revolt` on the province, against `В7_base/2 × (1 + marks) × (100 − L)/50`:
//! the form of В3 and В6, legitimacy acting through its norm, a loyal province able to rise
//! too (Volterra 1472 was a tax revolt) and one alienated twice over three times as readily.
//!
//! **A new polity is born only with a house to carry it** (owner's answers on §21 and
//! rule on #289: the secessions that made states were led by magnates and governors —
//! Magnus Maximus, the Adorno in Genoa; a rising without a leader — Volterra's tax revolt
//! — was put down and bore no state). If the province has a governor of a house that does
//! **not** rule, that house takes every province its members govern (the capital excepted)
//! into a polity of its own: the governor on its throne, `Acclaimed`, the parent's rule of
//! succession, the house changed of polity (E0 §3). **Seceding is the house's decision,
//! not the lot's** (owner's correction 1): the draw makes the chance, the house takes it or
//! not — in E2 every house is a bot and a bot takes it ([`World::house_would_secede`]); in
//! E3 it is the agent's decision, and the house of a player decides for itself, never
//! dragged by the lot into a war with its own lord. For the seat to exist the generator
//! gives governorships to the houses that do not rule, and a seat a governor leaves for
//! the throne goes to another house (`politics.rs`, owner's rule on #289).
//!
//! **A province with no such seat is in revolt** (`Province::revolt`), not a polity: it
//! pays no tax and gives no seat its income, its owner's men standing in it are fed as on
//! foreign land — they forage and ruin it (E0 П6) — and the ruler pays
//! `legitimacy.cost_revolt` once. The revolt ends when a unit of the owner has stood in
//! the province a tick (`RevoltEnded`); a province that changes hands is in revolt no more.
//! In E2 nobody marches without an order, so a revolt stands until the agents of E3 put it
//! down: lost income, not a polity in an endless war.
//!
//! **What the successor gets** (brief §2a): the provinces with their people
//! (`Transfer::Secession`), the parent's treasury split by the people who left
//! (`Source::Secession`), the parent's tax rate, institutions and rule of succession — not
//! the bank (an institution of the capital) nor the conversion policy (the court's); the
//! debt stays with the parent, the house treasury with the house. **A rising has strength**
//! (owner's correction 2): at birth the polity levies a militia from its own people up to
//! the ceiling of А1 through the ledger (`Source::Recruit`, a unit in every province), else
//! an undefended province would fall within the tick and every rising would be born and
//! dead in one. The house's own men (Х2) go with it. **Edges:** the graph of provinces, the
//! relation of every new pair at its norm, no treaty, no lord; **war with the parent**
//! without a price (`War.of = None`: a war for freedom, not an obligation), the parent's
//! allies, lord and vassals entering by §19.
//!
//! **The end of a rising** is the state of the war (§19): the parent took every province
//! back — the polity falls as any conquered one (`ActorDied`, the militia to the people);
//! peace from the parent at half the land — submission as its vassal (Genoa under Milan);
//! peace from the rising that won — freedom for the term of Д1. No «reabsorption» of its
//! own. In E2 nobody marches without an order (the agents of E3 do), so on the generated
//! worlds the rising stands and the war stays open: the row В7 measures the rate of risings,
//! not of their success.
//!
//! **No number for «unity»:** its readers — the rising, the secession — are already
//! expressed through alienation and legitimacy; a second number for the same would be a
//! dead field (brief §5 rule 7). No verbs: the list of thirteen is closed.
//!
//! **The phase** (after faith, before the people): the cores move; revolts with a unit of
//! the owner standing there end; then the risings, the provinces in canonical order, a
//! province in revolt or taken into a polity born this tick drawing no more.

use std::collections::BTreeSet;

use crate::event::{Event, Transfer};
use crate::ids::{ActorId, CultureId, HouseId, PersonId, ProvinceId};
use crate::ledger::{Account, Source};
use crate::params::Params;
use crate::rng::DrawPoint;
use crate::world::{Actor, Institution, Standing, World, WorldError};

impl World {
    /// The people of a polity by tongue, in id order of the tongue.
    fn people_by_culture(&self, a: ActorId) -> std::collections::BTreeMap<CultureId, i64> {
        let mut out = std::collections::BTreeMap::new();
        for p in &self.actors[&a].provinces {
            *out.entry(self.provinces[p].culture).or_insert(0) += self.population(*p);
        }
        out
    }

    /// The tongue of the majority of a polity's people (ties: the lower id); the capital's
    /// when nobody lives in the polity.
    pub fn majority_culture(&self, a: ActorId) -> CultureId {
        self.people_by_culture(a)
            .into_iter()
            .max_by_key(|(c, n)| (*n, std::cmp::Reverse(*c)))
            .map(|(c, _)| c)
            .unwrap_or(self.provinces[&self.actors[&a].capital].culture)
    }

    /// The core moves to the majority's tongue when it outgrows the core's by the margin
    /// (a tongue the polity has nobody of any more counts as nothing). True when it moved.
    fn settle_core(&mut self, a: ActorId, params: &Params) -> bool {
        let by = self.people_by_culture(a);
        let total: i64 = by.values().sum();
        let core = self.actors[&a].core_culture;
        let Some((top, n_top)) = by.iter().map(|(c, n)| (*c, *n)).max_by_key(|(c, n)| (*n, std::cmp::Reverse(*c))) else { return false };
        if top == core {
            return false;
        }
        let n_core = by.get(&core).copied().unwrap_or(0);
        if (n_top - n_core) * 1_000_000 > params.core_switch_margin_ppm * total {
            self.actors.get_mut(&a).unwrap().core_culture = top;
            return true;
        }
        false
    }

    /// The marks of alienation of a province: one for a faith other than its owner's
    /// court's, one for a tongue other than its owner's core (0, 1 or 2).
    pub fn alienation_marks(&self, p: ProvinceId) -> u32 {
        let pr = &self.provinces[&p];
        let a = pr.owner;
        u32::from(pr.religion != self.polity_religion(a)) + u32::from(pr.culture != self.actors[&a].core_culture)
    }

    /// Whether a province is in revolt against its owner.
    pub fn in_revolt(&self, p: ProvinceId) -> bool {
        self.provinces[&p].revolt.is_some()
    }

    /// The chance per tick that a province rises: the base at legitimacy 50 and no mark,
    /// times one plus the marks, through the legitimacy of its owner's ruler (the form of
    /// В3 and В6). Nothing for a capital, a province in revolt already, or a province of a
    /// fallen polity.
    pub fn revolt_chance_per_tick(&self, p: ProvinceId, params: &Params) -> f64 {
        let owner = self.provinces[&p].owner;
        if !self.alive(owner) || self.actors[&owner].capital == p || self.in_revolt(p) {
            return 0.0;
        }
        let legitimacy = self.actors[&owner].legitimacy;
        params.revolt_per_tick * (1.0 + f64::from(self.alienation_marks(p))) * (100.0 - legitimacy) / 50.0
    }

    /// The house that would carry a rising of `p` into a polity of its own: the house of
    /// the province's governor when that house does not rule; `None` means the province
    /// rises alone under a new house.
    pub fn secession_carrier(&self, p: ProvinceId) -> Option<HouseId> {
        let owner = self.provinces[&p].owner;
        let ruling = self.ruling_house(owner);
        self.provinces[&p].governor.map(|g| self.persons[&g].house).filter(|h| *h != ruling)
    }

    /// Whether a house takes the chance to secede the draw gave it. **E2: every house is a
    /// bot, and a bot takes it.** E3: the agent's decision; a player's house decides for
    /// itself, and the lot never drags it into a war with its own lord (owner's rule on §21).
    pub fn house_would_secede(&self, _house: HouseId) -> bool {
        true
    }

    /// A rising now, in `p`, outside the tick. For the tests only.
    #[cfg(feature = "testing")]
    pub fn rise_now(&mut self, p: ProvinceId, params: &Params) -> Result<Option<ActorId>, WorldError> {
        self.rise(p, params)
    }

    /// The rising of `p`. With a house to carry it the successor polity is born and takes
    /// its provinces, its share of the treasury, its militia, its edges and its war; with
    /// none the province is in revolt. `None` when no polity is born — a revolt, a capital,
    /// a fallen owner, a province with nobody in it, a house that declines.
    fn rise(&mut self, p: ProvinceId, params: &Params) -> Result<Option<ActorId>, WorldError> {
        let parent = self.provinces[&p].owner;
        let capital = self.actors[&parent].capital;
        if !self.alive(parent) || capital == p || self.population(p) < 1 || self.in_revolt(p) {
            return Ok(None);
        }
        let Some(house) = self.secession_carrier(p) else {
            // No house to carry it: the province is in revolt, the ruler pays.
            self.provinces.get_mut(&p).unwrap().revolt = Some(self.tick);
            self.pay_legitimacy(parent, params.legitimacy_cost_revolt);
            self.emit(Event::Revolt { province: p, from: parent });
            return Ok(None);
        };
        if !self.house_would_secede(house) {
            return Ok(None);
        }
        let provinces: Vec<ProvinceId> = self.actors[&parent].provinces.iter().copied().filter(|q| *q != capital && self.provinces[q].governor.is_some_and(|g| self.persons[&g].house == house)).collect();
        let people_total: i64 = self.actors[&parent].provinces.iter().map(|q| self.population(*q)).sum();
        let people_gone: i64 = provinces.iter().map(|q| self.population(*q)).sum();
        // The polity's record: a name in the world first (a house rising from the people
        // is named after its polity), then the house and the throne.
        let new = ActorId(self.actors.len() as u32);
        let (psid, pname) = self.names.actors[parent.index()].clone();
        self.names.actors.push((format!("{psid}.{}", new.0), format!("{pname} successor {}", new.0)));
        self.houses.get_mut(&house).unwrap().polity = new;
        let ruler: PersonId = self.provinces[&p].governor.expect("a carrier is the governor's house");
        let new_capital = provinces.iter().copied().max_by_key(|q| (self.population(*q), std::cmp::Reverse(*q))).expect("a rising has a province");
        let (succession, tax_rate_ppm, mut institutions) = {
            let pa = &self.actors[&parent];
            (pa.succession, pa.tax_rate_ppm, pa.institutions.clone())
        };
        // The bank is an institution of the capital, the policy one of the court.
        institutions.remove(&Institution::Bank);
        institutions.remove(&Institution::ConversionPolicy);
        self.actors.insert(
            new,
            Actor {
                provinces: Vec::new(),
                capital: new_capital,
                legitimacy: 0.0,
                institutions,
                tax_rate_ppm,
                paid_share_ppm: 1_000_000,
                ruler,
                heir: None,
                regent: None,
                succession,
                standing: Standing::Acclaimed,
                credit_closed_until: None,
                defaulted: false,
                vassal_of: None,
                core_culture: CultureId(0),
            },
        );
        self.ledger.open(self.tick, Account::Treasury(new), 0)?;
        self.ledger.open(self.tick, Account::Debt(new), 0)?;
        // The provinces, with their people; one event per province.
        for q in &provinces {
            self.transfer(*q, parent, new, Transfer::Secession)?;
            self.emit(Event::Secession { province: *q, from: parent, house });
        }
        // The treasury by the people who left; the debt stays with the parent.
        let share = self.treasury(parent) * people_gone / people_total.max(1);
        if share > 0 {
            self.post(Account::Treasury(parent), Source::Secession, -share * 1000)?;
            self.post(Account::Treasury(new), Source::Secession, share * 1000)?;
        }
        // Seats. One seat per person: the ruler leaves its governorships to its house. The
        // parent's heir or regent of the carrier house is set aside; a governor of the
        // carrier house left in the parent (its capital) loses the seat.
        for q in self.governed_by(ruler) {
            self.pass_governorship(q, ruler, params)?;
        }
        {
            let pa = self.actors.get_mut(&parent).unwrap();
            if pa.heir.is_some_and(|h| self.persons[&h].house == house) {
                pa.heir = None;
            }
            if pa.regent.is_some_and(|r| self.persons[&r].house == house) {
                pa.regent = None;
            }
        }
        for q in self.actors[&parent].provinces.clone() {
            if let Some(g) = self.provinces[&q].governor {
                if self.persons[&g].house == house {
                    self.provinces.get_mut(&q).unwrap().governor = None;
                    self.emit(Event::GovernorChanged { province: q, from: g, to: None });
                }
            }
        }
        // The core and the legitimacy of the new throne.
        let core = self.majority_culture(new);
        let legitimacy = self.standing_norm(new, params);
        {
            let na = self.actors.get_mut(&new).unwrap();
            na.core_culture = core;
            na.legitimacy = legitimacy;
        }
        // The house's own men go with it; the militia rises from the people.
        for u in self.units_of_house(house) {
            self.units.get_mut(&u).unwrap().owner = new;
        }
        self.levy_militia(new, params)?;
        // Edges: every pair at its norm — with the war with the parent, and the parent's
        // allies, lord and vassals in it by §19, counted in the norm.
        let others: Vec<ActorId> = self.actors.keys().copied().filter(|b| *b != new).collect();
        for b in &others {
            self.relations.insert(crate::army::pair(new, *b), 0.0);
        }
        self.open_war(new, parent)?;
        for b in &others {
            let norm = self.relation_norm(new, *b, params);
            self.relations.insert(crate::army::pair(new, *b), norm);
        }
        self.emit(Event::ActorBorn { actor: new, from: parent, house, provinces });
        Ok(Some(new))
    }

    /// The militia of a polity born this tick: every province levies up to the ceiling of
    /// А1 from its own people into a unit standing there, the whole within the polity's
    /// room under the ceiling.
    fn levy_militia(&mut self, a: ActorId, params: &Params) -> Result<(), WorldError> {
        let mut room = self.recruit_room(a, params);
        for q in self.actors[&a].provinces.clone() {
            let n = (self.population(q) * params.recruit_ceiling_ppm / 1_000_000).min(room);
            if n <= 0 {
                continue;
            }
            let unit = self.new_unit(a, q, false)?;
            self.post(Account::Population(q), Source::Recruit, -n * 1000)?;
            self.post(Account::Soldiers(unit), Source::Recruit, n * 1000)?;
            self.emit(Event::Recruited { unit, soldiers: n as u64 });
            room -= n;
        }
        Ok(())
    }

    /// The phase of collapse: the cores of the living polities settle; a revolt with a unit
    /// of the owner standing in the province ends; then every province of a living polity
    /// but its capital draws once for a rising, in canonical order; a province in revolt or
    /// taken into a polity born this tick draws no more.
    pub(crate) fn phase_collapse(&mut self, params: &Params) -> Result<(), WorldError> {
        let ids: Vec<ActorId> = self.actors.keys().copied().filter(|a| self.alive(*a)).collect();
        for a in &ids {
            self.settle_core(*a, params);
        }
        let put_down: Vec<(ProvinceId, ActorId)> = self
            .provinces
            .iter()
            .filter(|(q, pr)| pr.revolt.is_some() && self.units.values().any(|u| u.position == **q && u.owner == pr.owner))
            .map(|(q, pr)| (*q, pr.owner))
            .collect();
        for (q, owner) in put_down {
            self.provinces.get_mut(&q).unwrap().revolt = None;
            self.emit(Event::RevoltEnded { province: q, actor: owner });
        }
        let provinces: Vec<ProvinceId> = self.provinces.keys().copied().filter(|q| self.revolt_chance_per_tick(*q, params) > 0.0 || self.alive(self.provinces[q].owner)).collect();
        let mut born: BTreeSet<ProvinceId> = BTreeSet::new();
        for q in provinces {
            if born.contains(&q) {
                continue;
            }
            let chance = self.revolt_chance_per_tick(q, params);
            if chance <= 0.0 {
                continue;
            }
            if self.draw_once(DrawPoint::Revolt, q.0) < chance {
                if let Some(new) = self.rise(q, params)? {
                    born.extend(self.actors[&new].provinces.iter().copied());
                }
            }
        }
        Ok(())
    }
}
