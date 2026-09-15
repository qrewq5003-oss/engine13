//! Measurement sink for what the engine actually applied.
//!
//! Every balance claim in this project is decided by a probe, and the probes kept
//! answering a different question than the engine asked. Two failures made the case for
//! this module:
//!
//! * `budget_probe` re-implements the dependency arithmetic **three times**. When the
//!   engine grew a mode, none of the copies knew; the compiler caught two of them and
//!   the third — a `match` behind `_ => 0.0` — silently priced the new mode as zero
//!   (`docs/investigation_silent_authored_content.md` §16).
//! * A probe that re-derived a value from a tick-boundary snapshot measured the wrong
//!   moment: the dependency phase runs mid-tick, after auto-deltas, region ranks and
//!   military recovery.
//!
//! The answer is not discipline but arithmetic that cannot drift: the engine **emits the
//! number it just used**, and the probe reads it. Nothing is computed twice.
//!
//! # Cost when disabled
//!
//! Each sink is a `thread_local` holding `None`. An emission site costs one thread-local
//! read and no allocation, draws no RNG and touches no state, so simulation output is
//! byte-identical with the sinks off — which is their default and the only state any
//! shipped code ever sees.

use std::cell::RefCell;

/// One application of a dependency rule to one actor.
#[derive(Debug, Clone)]
pub struct DependencyRow {
    pub tick: u32,
    pub actor: String,
    pub rule: String,
    /// The source metric's value at the moment the rule read it.
    pub from_val: f64,
    /// The target's value before the write — the denominator of any "share of stock"
    /// question, and not recoverable from a tick-boundary snapshot.
    pub to_before: f64,
    pub delta: f64,
}

/// One application of an auto-delta block.
///
/// Both numbers are emitted because they answer different questions: `authored` is what
/// the content asked for (`base` plus every satisfied condition's own delta), `applied`
/// is that plus this tick's noise. A criterion about authored intent wants the first; a
/// criterion about what the world felt wants the second.
#[derive(Debug, Clone)]
pub struct AutoDeltaRow {
    pub tick: u32,
    /// Index into `scenario.auto_deltas` — the block's identity, since blocks have no id.
    pub index: usize,
    pub metric: String,
    pub base: f64,
    pub authored: f64,
    pub applied: f64,
}

thread_local! {
    static DEPENDENCY: RefCell<Option<Vec<DependencyRow>>> = const { RefCell::new(None) };
    static AUTO_DELTA: RefCell<Option<Vec<AutoDeltaRow>>> = const { RefCell::new(None) };
}

/// Start recording both kinds. Rows accumulate until taken.
pub fn enable() {
    DEPENDENCY.with(|t| *t.borrow_mut() = Some(Vec::new()));
    AUTO_DELTA.with(|t| *t.borrow_mut() = Some(Vec::new()));
}

/// Stop recording and drop anything held.
pub fn disable() {
    DEPENDENCY.with(|t| *t.borrow_mut() = None);
    AUTO_DELTA.with(|t| *t.borrow_mut() = None);
}

/// Drain the dependency rows recorded so far, leaving recording on.
pub fn take_dependencies() -> Vec<DependencyRow> {
    DEPENDENCY.with(|t| t.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
}

/// Drain the auto-delta rows recorded so far, leaving recording on.
pub fn take_auto_deltas() -> Vec<AutoDeltaRow> {
    AUTO_DELTA.with(|t| t.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
}

pub(crate) fn record_dependency(row: impl FnOnce() -> DependencyRow) {
    DEPENDENCY.with(|t| {
        if let Some(rows) = t.borrow_mut().as_mut() {
            rows.push(row());
        }
    });
}

pub(crate) fn record_auto_delta(row: impl FnOnce() -> AutoDeltaRow) {
    AUTO_DELTA.with(|t| {
        if let Some(rows) = t.borrow_mut().as_mut() {
            rows.push(row());
        }
    });
}
