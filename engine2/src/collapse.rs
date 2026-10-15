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
//! **The phase** (after faith, before the people): the cores move; then — E2 step 9 (б) —
//! the risings.

use crate::ids::{ActorId, CultureId, ProvinceId};
use crate::params::Params;
use crate::world::{World, WorldError};

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

    /// The phase of collapse: the cores of the living polities settle.
    pub(crate) fn phase_collapse(&mut self, params: &Params) -> Result<(), WorldError> {
        let ids: Vec<ActorId> = self.actors.keys().copied().filter(|a| self.alive(*a)).collect();
        for a in &ids {
            self.settle_core(*a, params);
        }
        Ok(())
    }
}
