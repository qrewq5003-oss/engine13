//! Religion and culture (brief §2 «Религия и культура», plan §20): the tongue and the faith
//! of a province's majority and of a house; conversion and assimilation along E0 Р1; the
//! friction of two courts in the relation of their polities (`diplomacy.rs`); the
//! alienation of a province from its owner as the entry of collapse.
//!
//! The engine never reads which tongue or faith is which: the scenario's strings are
//! interned to `CultureId`/`ReligionId` at compile time, and every rule compares ids for
//! equality alone. A polity speaks and prays as its **ruling house** does (owner's answer on
//! §20) — and a house that takes the throne takes the faith of the court (owner's rule on
//! #288: a foreign dynasty took the faith of the land; `politics.rs`), its tongue staying
//! its own, so the court's faith changes only by its own decision (the union, E3). A house
//! raised from the people prays as the court did and speaks as its capital does. Alienation
//! reads the court for the faith and the **core** — the tongue of the polity's majority
//! (`collapse.rs`) — for the tongue (owner's rule on §21).
//!
//! **Conversion** (Р1). A province keeps its majority and one minority on the move
//! (`Shift`): the share won to another faith grows along the logistic curve
//! `s' = s + r·s·(1 − s)` from `conversion.seed_share`; at half the majority turns
//! (`Converted`) and the share is spent — the former majority is gone (owner's note on
//! §20: one number, not a mix; the people are not moved between accounts, the sum of the
//! population is untouched). Two channels: **rule** — the owner's court prays otherwise
//! **and holds `Institution::ConversionPolicy`** (owner's correction on §20: rule converts
//! far from always — Venetian Crete, Ottoman Rumelia; Bulliet's curve is Iran, where
//! conversion was rewarded), at the full rate; **neighbourhood** — more than half of the
//! province's neighbours, by land and by sea, pray one other faith, at
//! `conversion.neighbourhood_factor` of the rate. Rule comes first; a change of target
//! starts the share again from the seed; with no channel the share stays where it is.
//! **Assimilation** of the tongue goes by the neighbourhood alone: no court has a policy
//! for the tongue in E2 (Crete stayed Greek-speaking as it stayed Orthodox).
//!
//! **Institutions by the same channels** (E0 §2 variant 3: one mechanism with faith). A
//! polity in contact with one that holds a practice — a neighbour by land or sea (trade),
//! an ally, a lord or a vassal (a treaty), a house married into (marriage) — takes it up
//! with `institution.adoption_per_year` by a keyed draw at `DrawPoint::Adoption`
//! (`Adopted`). The bank spreads; the conversion policy does not: a policy is a court's
//! decision, not a practice caught from a neighbour (E3 may make it one).
//!
//! **Phase** (after diplomacy, before the people): provinces in canonical order, then the
//! polities.

use crate::event::{Channel, Event};
use crate::ids::{ActorId, CultureId, ProvinceId, ReligionId};
use crate::params::Params;
use crate::rng::DrawPoint;
use crate::world::{Institution, Shift, World, WorldError};

/// The institutions that spread by contact (a practice, not a policy).
pub const SPREADING: [Institution; 1] = [Institution::Bank];

impl World {
    /// The faith of a polity's court: its ruling house's.
    pub fn polity_religion(&self, a: ActorId) -> ReligionId {
        self.houses[&self.persons[&self.actors[&a].ruler].house].religion
    }

    /// The tongue of a polity's court: its ruling house's.
    pub fn polity_culture(&self, a: ActorId) -> CultureId {
        self.houses[&self.persons[&self.actors[&a].ruler].house].culture
    }

    /// A province whose majority prays otherwise than its owner's **court**, or speaks
    /// otherwise than its owner's **core** — the tongue of the polity's majority
    /// (`collapse.rs`; owner's rule on §21: foreign-tongued courts were the norm, what fell
    /// away were the regions of another tongue and faith than the core): the entry of
    /// collapse (В7, secession by culture and region), with no effect on the take in E2
    /// (owner's answer on §20: Э8 stays clean).
    pub fn alienated(&self, p: ProvinceId) -> bool {
        self.alienation_marks(p) > 0
    }

    /// The one value more than half of a province's neighbours (land and sea) share, if
    /// it differs from the province's own.
    fn neighbourhood_majority<T: Copy + Ord>(&self, p: ProvinceId, own: T, of: impl Fn(ProvinceId) -> T) -> Option<T> {
        let pr = &self.provinces[&p];
        let all: Vec<ProvinceId> = pr.neighbours.iter().chain(pr.sea_neighbours.iter()).copied().collect();
        if all.is_empty() {
            return None;
        }
        let mut counts: std::collections::BTreeMap<T, usize> = std::collections::BTreeMap::new();
        for n in &all {
            *counts.entry(of(*n)).or_insert(0) += 1;
        }
        counts.into_iter().find(|(v, c)| *v != own && *c * 2 > all.len()).map(|(v, _)| v)
    }

    /// The faith a province is drawn to and at what rate: the owner's court under the
    /// conversion policy first, else the neighbourhood's majority.
    pub fn conversion_channel(&self, p: ProvinceId, params: &Params) -> Option<(ReligionId, Channel, i64)> {
        let pr = &self.provinces[&p];
        let court = self.polity_religion(pr.owner);
        if court != pr.religion && self.actors[&pr.owner].institutions.contains(&crate::world::Institution::ConversionPolicy) {
            return Some((court, Channel::Rule, params.conversion_rate_ppm_per_tick));
        }
        self.neighbourhood_majority(p, pr.religion, |n| self.provinces[&n].religion)
            .map(|to| (to, Channel::Neighbourhood, (params.conversion_rate_ppm_per_tick as f64 * params.conversion_neighbourhood_factor).round() as i64))
    }

    /// The tongue a province is drawn to: its neighbourhood's majority, at the
    /// neighbourhood's rate — the rule has no channel for the tongue.
    pub fn assimilation_channel(&self, p: ProvinceId, params: &Params) -> Option<(CultureId, i64)> {
        let own = self.provinces[&p].culture;
        self.neighbourhood_majority(p, own, |n| self.provinces[&n].culture)
            .map(|to| (to, (params.conversion_rate_ppm_per_tick as f64 * params.conversion_neighbourhood_factor).round() as i64))
    }

    /// One step of the logistic curve on a share in parts per million.
    fn grow(share_ppm: i64, rate_ppm: i64) -> i64 {
        (share_ppm + rate_ppm * share_ppm / 1_000_000 * (1_000_000 - share_ppm) / 1_000_000).min(1_000_000)
    }

    /// Advance a minority on the move towards `to` at `rate`; true when the majority turns.
    fn advance<T: Copy + PartialEq>(shift: &mut Option<Shift<T>>, to: T, rate_ppm: i64, seed_ppm: i64) -> bool {
        let current = match shift {
            Some(s) if s.to == to => s.share_ppm,
            _ => seed_ppm,
        };
        let next = Self::grow(current, rate_ppm);
        if next >= 500_000 {
            *shift = None;
            true
        } else {
            *shift = Some(Shift { to, share_ppm: next });
            false
        }
    }

    /// The polities `a` is in contact with: neighbours by land or sea, allies, lord and
    /// vassals, and those a house of `a` is married into — the channels an institution
    /// spreads along.
    pub fn contacts(&self, a: ActorId) -> Vec<ActorId> {
        self.actors
            .keys()
            .copied()
            .filter(|b| *b != a && self.alive(*b) && (self.borders(a, *b) || self.allied(a, *b) || self.bound_by_vassalage(a, *b) || self.married_across(a, *b)))
            .collect()
    }

    /// A polity `a` could take `institution` from: the lowest id in contact that holds it;
    /// nothing for an institution that does not spread, or one `a` holds already.
    pub fn adoption_source(&self, a: ActorId, institution: Institution) -> Option<ActorId> {
        if !SPREADING.contains(&institution) || self.actors[&a].institutions.contains(&institution) {
            return None;
        }
        self.contacts(a).into_iter().find(|b| self.actors[b].institutions.contains(&institution))
    }

    /// A polity whose court could be offered the policy of conversion: the lowest id in
    /// contact that holds it (E3 (в): the court decides, `opportunity.rs`).
    pub fn policy_source(&self, a: ActorId) -> Option<ActorId> {
        let policy = Institution::ConversionPolicy;
        if self.actors[&a].institutions.contains(&policy) {
            return None;
        }
        self.contacts(a).into_iter().find(|b| self.actors[b].institutions.contains(&policy))
    }

    /// Institutions spread by contact: one keyed draw per polity and tick. A practice (the
    /// bank) is taken up; the policy of conversion, met the same way, is offered to the
    /// court, which decides at the next turn (E3 (в)).
    fn phase_adoption(&mut self, params: &Params) -> Result<(), WorldError> {
        let ids: Vec<ActorId> = self.actors.keys().copied().filter(|a| self.alive(*a)).collect();
        for a in ids {
            let mut wanted: Vec<(Institution, ActorId)> = SPREADING.iter().filter_map(|i| self.adoption_source(a, *i).map(|from| (*i, from))).collect();
            wanted.extend(self.policy_source(a).map(|from| (Institution::ConversionPolicy, from)));
            if wanted.is_empty() {
                continue;
            }
            let u = crate::rng::unit(self.draw(DrawPoint::Adoption, a.0));
            if u < params.institution_adoption_per_tick {
                let (institution, from) = wanted[0];
                if institution == Institution::ConversionPolicy {
                    self.open_opportunity(crate::verbs::ActorRef::Ruler(a), crate::opportunity::Chance::Policy { polity: a, from });
                    continue;
                }
                self.actors.get_mut(&a).unwrap().institutions.insert(institution);
                self.emit(Event::Adopted { actor: a, institution, from });
            }
        }
        Ok(())
    }

    /// The courts with a goal of union with another faith are offered it (E3 (в)).
    fn open_unions(&mut self) {
        let ids: Vec<ActorId> = self.actors.keys().copied().filter(|a| self.alive(*a)).collect();
        for a in ids {
            let court = self.polity_religion(a);
            let wanted = self.actors[&a].goals.iter().find_map(|g| match (g.kind, g.target) {
                (crate::agents::GoalKind::Union, crate::agents::GoalTarget::Religion(f)) if f != court => Some(f),
                _ => None,
            });
            if let Some(faith) = wanted {
                self.open_opportunity(crate::verbs::ActorRef::Ruler(a), crate::opportunity::Chance::Union { polity: a, faith });
            }
        }
    }

    /// The share of a polity's people whose province prays `faith`.
    pub fn people_share_praying(&self, a: ActorId, faith: ReligionId) -> f64 {
        let (mut of, mut all) = (0i64, 0i64);
        for p in &self.actors[&a].provinces {
            let n = self.population(*p);
            all += n;
            if self.provinces[p].religion == faith {
                of += n;
            }
        }
        if all == 0 { 0.0 } else { of as f64 / all as f64 }
    }

    /// The phase of faith and tongue, then of the institutions that spread by contact.
    pub(crate) fn phase_faith(&mut self, params: &Params) -> Result<(), WorldError> {
        let provinces: Vec<ProvinceId> = self.provinces.keys().copied().collect();
        for p in provinces {
            if let Some((to, by, rate)) = self.conversion_channel(p, params) {
                let pr = self.provinces.get_mut(&p).unwrap();
                let from = pr.religion;
                if Self::advance(&mut pr.converting, to, rate, params.conversion_seed_ppm) {
                    pr.religion = to;
                    self.emit(Event::Converted { province: p, from, to, by });
                }
            }
            if let Some((to, rate)) = self.assimilation_channel(p, params) {
                let pr = self.provinces.get_mut(&p).unwrap();
                let from = pr.culture;
                if Self::advance(&mut pr.assimilating, to, rate, params.conversion_seed_ppm) {
                    pr.culture = to;
                    self.emit(Event::Assimilated { province: p, from, to });
                }
            }
        }
        self.phase_adoption(params)?;
        self.open_unions();
        Ok(())
    }
}
