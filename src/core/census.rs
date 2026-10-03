//! Census of reads on absent actors — stage 1 of B44 (docs/TRIAGE.md).
//!
//! `MetricRef::get` reads an absent actor (dead, or not yet spawned) as `0.0`; readers
//! then turn that `0.0` into whatever their comparison says. The census finds those
//! reads where they happen instead of listing known ids: with the `census` feature,
//! `get`/`try_get` carry `#[track_caller]` and every read of an absent actor is recorded
//! with its call site, key and tick. Condition sites annotate the read with the operator
//! and the result it produced.
//!
//! # Off by default, twice
//!
//! Without the feature every function here is the identity or a no-op and `get` has no
//! `#[track_caller]`: the shipped build is the build without it. With the feature,
//! recording is still off until [`enable`]; recording draws no RNG and writes no state.
//!
//! # Occupancy
//!
//! [`enable_occupancy`] counts, for every annotated condition, how often it was true —
//! at the point of evaluation, not from a tick-boundary snapshot (the treasury, for one,
//! moves inside the tick before the auto-deltas read it). Every condition, present actor
//! or absent.
//!
//! # Writes (A37)
//!
//! [`enable_writes`] records every write to an actor's `external_pressure` or `cohesion`:
//! call site, the source named by the writing site when it names one ([`write_source`]),
//! the delta asked for and the delta that landed after any clamp. Completeness is checked
//! by the probe, not assumed: the landed deltas of a tick must sum to the tick's change.
//!
//! # The counterfactual
//!
//! [`set_uniform_false`] makes every annotated condition whose read hit an absent actor
//! evaluate to `false` — the rule milestones with `actor_id` already follow. Data for a
//! decision, not a decision: nothing in shipped code can turn it on.

#[cfg(feature = "census")]
mod imp {
    use std::cell::{Cell, RefCell};

    /// One read of a metric on an actor that is not in `world.actors`.
    #[derive(Debug, Clone)]
    pub struct DeadRead {
        pub location: &'static std::panic::Location<'static>,
        pub key: String,
        pub tick: u32,
        /// `true` if the actor is in `dead_actor_ids`; `false` if it never entered the
        /// world (a spawn or successor not yet created).
        pub dead: bool,
        /// Which content was being evaluated (set by the reading site).
        pub context: String,
        /// For condition sites: how the read was used.
        pub condition: Option<Condition>,
    }

    #[derive(Debug, Clone)]
    pub struct Condition {
        /// Operator and threshold as authored, e.g. `> 70`.
        pub test: String,
        /// The result the engine computed from the default.
        pub result: bool,
        /// The result actually used (differs only under the counterfactual).
        pub used: bool,
    }

    /// (context, test) -> (times true, times evaluated).
    pub type Occupancy = std::collections::BTreeMap<(String, String), (u64, u64)>;

    thread_local! {
        static RECORDS: RefCell<Option<Vec<DeadRead>>> = const { RefCell::new(None) };
        static CONTEXT: RefCell<String> = const { RefCell::new(String::new()) };
        static PENDING: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
        static UNIFORM_FALSE: Cell<bool> = const { Cell::new(false) };
        static OCCUPANCY: RefCell<Option<Occupancy>> = const { RefCell::new(None) };
    }

    /// Start counting condition outcomes: (context, test) -> (times true, times evaluated).
    /// One write to a watched metric of an actor.
    #[derive(Debug, Clone)]
    pub struct Write {
        pub location: &'static std::panic::Location<'static>,
        pub source: Option<String>,
        pub actor: String,
        pub metric: String,
        pub requested: f64,
        pub applied: f64,
    }

    const WATCHED: &[&str] = &["external_pressure", "cohesion"];

    thread_local! {
        static WRITES: RefCell<Option<Vec<Write>>> = const { RefCell::new(None) };
        static WRITE_SOURCE: RefCell<Option<String>> = const { RefCell::new(None) };
        static DEP_CAP: RefCell<Option<(String, f64)>> = const { RefCell::new(None) };
        static WATCH_ONLY: Cell<bool> = const { Cell::new(true) };
    }

    pub fn enable_writes() {
        WRITES.with(|w| *w.borrow_mut() = Some(Vec::new()));
    }

    /// Record every metric, not only `external_pressure` and `cohesion` (A46: the clamp
    /// losses of all metrics; family metrics come as actor `family`, globals as `global`).
    pub fn watch_all_metrics(all: bool) {
        WATCH_ONLY.with(|w| w.set(!all));
    }

    pub fn take_writes() -> Vec<Write> {
        WRITES.with(|w| w.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
    }

    /// Name the source of the writes that follow, until [`clear_write_source`].
    pub fn write_source(name: impl FnOnce() -> String) {
        if WRITES.with(|w| w.borrow().is_some()) {
            WRITE_SOURCE.with(|s| *s.borrow_mut() = Some(name()));
        }
    }

    pub fn clear_write_source() {
        WRITE_SOURCE.with(|s| *s.borrow_mut() = None);
    }

    pub fn metric_write(location: &'static std::panic::Location<'static>, actor: &str, metric: &str, requested: f64, before: f64, after: f64) {
        if WATCH_ONLY.with(|w| w.get()) && !WATCHED.contains(&metric) {
            return;
        }
        WRITES.with(|w| {
            if let Some(rows) = w.borrow_mut().as_mut() {
                rows.push(Write {
                    location,
                    source: WRITE_SOURCE.with(|s| s.borrow().clone()),
                    actor: actor.to_string(),
                    metric: metric.to_string(),
                    requested,
                    applied: after - before,
                });
            }
        });
    }

    /// Counterfactual (A37 (б)): the named dependency rule reads its source as
    /// `min(source, cap)`.
    pub fn set_dependency_cap(cap: Option<(String, f64)>) {
        DEP_CAP.with(|c| *c.borrow_mut() = cap);
    }

    pub fn dependency_source(rule: &str, from: f64) -> f64 {
        DEP_CAP.with(|c| match &*c.borrow() {
            Some((r, cap)) if r == rule => from.min(*cap),
            _ => from,
        })
    }

    pub fn enable_occupancy() {
        OCCUPANCY.with(|o| *o.borrow_mut() = Some(Default::default()));
    }

    pub fn take_occupancy() -> Occupancy {
        OCCUPANCY.with(|o| o.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
    }

    fn occupancy_on() -> bool {
        OCCUPANCY.with(|o| o.borrow().is_some())
    }

    pub fn enable() {
        RECORDS.with(|r| *r.borrow_mut() = Some(Vec::new()));
    }

    pub fn take() -> Vec<DeadRead> {
        RECORDS.with(|r| r.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
    }

    pub fn set_uniform_false(on: bool) {
        UNIFORM_FALSE.with(|u| u.set(on));
    }

    fn on() -> bool {
        RECORDS.with(|r| r.borrow().is_some()) || occupancy_on()
    }

    pub fn begin(context: impl FnOnce() -> String) {
        if !on() {
            return;
        }
        CONTEXT.with(|c| *c.borrow_mut() = context());
        PENDING.with(|p| p.borrow_mut().clear());
    }

    pub fn read(location: &'static std::panic::Location<'static>, key: impl FnOnce() -> String, tick: u32, dead: bool) {
        RECORDS.with(|r| {
            if let Some(rows) = r.borrow_mut().as_mut() {
                PENDING.with(|p| p.borrow_mut().push(rows.len()));
                rows.push(DeadRead {
                    location,
                    key: key(),
                    tick,
                    dead,
                    context: CONTEXT.with(|c| c.borrow().clone()),
                    condition: None,
                });
            }
        });
    }

    pub fn condition(test: impl FnOnce() -> String, result: bool) -> bool {
        let pending: Vec<usize> = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
        let counting = occupancy_on();
        if pending.is_empty() && !counting {
            return result;
        }
        let test = test();
        if counting {
            let ctx = CONTEXT.with(|c| c.borrow().clone());
            OCCUPANCY.with(|o| {
                if let Some(m) = o.borrow_mut().as_mut() {
                    let e = m.entry((ctx, test.clone())).or_default();
                    e.0 += result as u64;
                    e.1 += 1;
                }
            });
        }
        if pending.is_empty() {
            return result;
        }
        let used = if UNIFORM_FALSE.with(|u| u.get()) { false } else { result };
        RECORDS.with(|r| {
            if let Some(rows) = r.borrow_mut().as_mut() {
                for i in pending {
                    rows[i].condition = Some(Condition { test: test.clone(), result, used });
                }
            }
        });
        used
    }
}

#[cfg(feature = "census")]
pub use imp::*;

/// Name the source of the next metric writes (A37). No-op without the feature.
#[cfg(not(feature = "census"))]
#[inline(always)]
pub fn write_source(_name: impl FnOnce() -> String) {}

#[cfg(not(feature = "census"))]
#[inline(always)]
pub fn clear_write_source() {}

/// A dependency rule's source value; the identity without the feature.
#[cfg(not(feature = "census"))]
#[inline(always)]
pub fn dependency_source(_rule: &str, from: f64) -> f64 {
    from
}

/// Name the content the next reads belong to. No-op without the feature.
#[cfg(not(feature = "census"))]
#[inline(always)]
pub fn begin(_context: impl FnOnce() -> String) {}

/// Annotate the reads since [`begin`] as a condition; returns the result to use.
/// The identity without the feature.
#[cfg(not(feature = "census"))]
#[inline(always)]
pub fn condition(_test: impl FnOnce() -> String, result: bool) -> bool {
    result
}
