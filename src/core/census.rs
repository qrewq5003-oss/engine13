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

    thread_local! {
        static RECORDS: RefCell<Option<Vec<DeadRead>>> = const { RefCell::new(None) };
        static CONTEXT: RefCell<String> = const { RefCell::new(String::new()) };
        static PENDING: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
        static UNIFORM_FALSE: Cell<bool> = const { Cell::new(false) };
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
        RECORDS.with(|r| r.borrow().is_some())
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
        if pending.is_empty() {
            return result;
        }
        let used = if UNIFORM_FALSE.with(|u| u.get()) { false } else { result };
        let test = test();
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
