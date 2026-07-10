# ENGINE13 — Investigation: does `external_pressure` need a decay path?

**Task 2 of `ENGINE13_INFRASTRUCTURE_TASKS.md`. Diagnosis-first (D3 discipline): confirm
the hypothesis experimentally on all three scenarios before deciding whether/how to fix.**

**Verdict: the proposed remedy (add an `external_pressure` decay) is REFUTED by
experiment. It does not make vassalage reachable, because the unreachability is not
caused by the ep axis. Recommendation: do NOT add ep decay; the vassalage *trigger*
needs redesign, which is a v2 design decision and out of scope for the debt-closing
pass.** See "Recommendation" below.

---

## Method

Throwaway harness (`src/bin/ep_diag.rs`, not committed) replicating the `sim` world
setup, run over **all three scenarios × 20 seeds × 400 ticks**. Per actor / seed it
records: max ep, whether ep entered the vassalage band `70–85`, whether the *full*
band was entered (`ep 70–85 ∧ legitimacy 10–25 ∧ cohesion 15–30`, the exact predicate
`interactions::in_vassalage_band`), ticks spent in the ep band, and any vassalage
formation. It then classifies, during ep-band ticks, whether legitimacy/cohesion are
too healthy / in-band / crashed; and a counterfactual "weakness window" (leg & coh both
in their sub-bands at *any* ep) with the ep distribution at those moments. An optional
per-tick ep decay applied to every living actor lets us test the fix directly.

Reproduce: `cargo run --bin ep_diag -- <ticks> <seeds> [decay_per_tick]`.

---

## Finding 1 — the symptom generalizes to all three scenarios

The Milan-1477 observation (vassalage never forms) is **not Milan-specific**. Across
rome_375, constantinople_1430 and milan_1477, over 20 seeds × 400 ticks:

- `external_pressure` saturates at **100** for essentially every actor (mean final ep ≈ 100).
- The ep band `70–85` **is** transited by nearly every actor — but briefly (e.g. Milan
  actors spend only ~20–64 of 400 ticks there). It is a transient window during a
  monotonic rise, exactly as hypothesized.
- The **full** vassalage band is reached **0 / 20 seeds for every actor in every scenario.**
- **Zero vassalages form** in any scenario, any seed.

## Finding 2 — the engine core has no ep relief; only rome_375 authors any decay

- Engine core (`phase_region_ranks`, `phase_actor_tags`, `phase_era_progression`, all of
  `interactions.rs`) **never subtracts** `external_pressure`. Combat adds `+15–25` to the
  defender (`interactions.rs:431`); migration only adds (`:653`). There is no relief path.
- The **only** ep-decreasing mechanism anywhere is scenario `auto_deltas`. Only
  **rome_375** authors one: `actor:rome.external_pressure`, base `+2.0`, with ratio
  conditions netting **−0.8/tick when Rome keeps military parity** (`rome_375.rs:1290`).
  `milan_1477` and `constantinople_1430` reference `external_pressure` **zero times** in
  their `create_auto_deltas` — pressure is a pure monotonic ratchet toward 100.
- So a decay mechanism *already exists at the content layer*; two of three scenarios
  simply never author one. But even rome's authored decay is overwhelmed: it saturates at
  100 anyway, because combat's `+15–25` per event dwarfs a `±2` auto_delta and Rome loses
  parity as the barbarians grow. **Decay magnitude is an order below the growth sources.**

## Finding 3 (the real cause) — ep and legitimacy/cohesion are temporally decoupled

The full band is missed **not** because ep fails to enter `70–85`, but because during
those ep-band ticks the other two metrics are in the wrong place:

| scenario | during ep∈[70,85]: legitimacy | during ep∈[70,85]: cohesion |
|---|---|---|
| rome_375 | `>25` (too healthy) **100%** | `>30` (too healthy) **100%** |
| constantinople_1430 | `>25` **100%** | `>30` **100%** |
| milan_1477 | `>25` **100%** | `>30` 87% / in-band 12% / crashed 1% |

Counterfactual weakness window (`leg 10–25 ∧ coh 15–30`, ignoring ep) — where is ep when
an actor *is* structurally weak?

| scenario | weakness-window ticks | ep at those moments |
|---|---|---|
| rome_375 | **0** | window never reached — actors are healthy or dead, never "weak but alive" |
| constantinople_1430 | 200 | ep **< 70 in 100%** — pressure *lags* the weakness |
| milan_1477 | 120 | ep **> 85 in 100%** — pressure *overshoots* before leg/coh crash |

The three-metric simultaneity the band requires is essentially measure-zero, and it fails
**differently in each scenario** — rome never gets weak-but-alive, constantinople is weak
while ep is still low, milan blows past 85 before it gets weak. Nothing in the engine
couples pressure to legitimacy/cohesion, so their trajectories don't co-move.

## Finding 4 (the experimental refutation) — ep decay does not fix it

Injecting a uniform per-tick ep decay on every living actor and sweeping magnitude:

| decay/tick | vassalages formed (any scenario, seed) |
|---|---|
| 0 | 0 |
| 1 | 0 |
| 2 | 0 |
| 3 | 0 |
| 5 | 0 |
| 8 | 0 |

At **decay = 2**, rome finally reaches the weakness window (429 ticks) — but decay has
pushed ep **< 70 in 100%** of them: it overshoots *past* the band downward. At **decay = 8**
(absurdly strong, far beyond anything calibratable from `interactions.rs` magnitudes),
milan touches the full band for all of **5 ticks** — still 0 vassalages, because formation
needs **3 consecutive** in-band ticks (`check_vassalage`, `counter < 3`) and the fragile
coincidence never holds that long. Stronger decay also *shrinks* the weakness window
(milan 66→40→23 ticks as decay 3→5→8), because relieving pressure changes the combat and
collapse dynamics that produce weakness in the first place — so cranking decay is
self-defeating.

Decay moves ep on an axis that is independent of *when* leg/coh are weak; it cannot be
tuned to manufacture a coincidence it does not control.

---

## Recommendation

1. **Do not add an `external_pressure` decay. The hypothesis "decay solves it" is
   empirically REFUTED.** Sweep of a uniform per-tick ep decay from **1 to 8/tick**
   produced **0 vassalages at every magnitude**, on all three scenarios (see Finding 4).
   It is the wrong lever: it addresses one axis of a three-axis timing problem. This result
   is recorded here specifically so the "just add decay" idea is not re-attempted in the
   future without re-deriving the same negative result via the same sweep. (It closes
   task-2 step 2's architectural question with a negative answer, and makes step 3 — "if
   the decision is to add decay" — moot.)

2. **The real cause is a measure-zero intersection.** The vassalage *trigger*
   (`in_vassalage_band`: simultaneous `ep 70–85 ∧ leg 10–25 ∧ coh 15–30`, held 3 ticks)
   requires three metrics to be in their bands *at the same time*. They are temporally
   desynchronised — pressure ramps and saturates on a different, uncoupled clock from
   legitimacy/cohesion degradation — so the simultaneous intersection is effectively
   measure-zero (rome never gets weak-but-alive; constantinople is weak while ep < 70;
   milan blows past ep 85 before it gets weak). This is a property of the trigger, not of
   the ep axis, which is why moving the ep axis (decay) cannot fix it.

3. **Candidate direction for a future redesign (NOT a decision): (a) couple ep to
   legitimacy/cohesion erosion** — i.e. sustained high `external_pressure` erodes
   `legitimacy`/`cohesion`, so pressure and structural weakness arrive together instead of
   on independent clocks. This is recorded as the leading *candidate* to investigate first
   if/when the mechanic is revisited, **not** an accepted solution. An alternative (b) —
   redefine the trigger so it does not require three-metric simultaneity (a pressure-driven
   single-condition submission, or a sequential rather than simultaneous predicate) —
   remains open. Choosing between (a) and (b), and calibrating either, is out of scope here.

4. **Any such redesign is v2 reactive-system work**, which `ENGINE13_INFRASTRUCTURE_TASKS.md`
   explicitly defers, and is balance-affecting (its own before/after baseline on all three
   scenarios). This investigation deliberately stops at the diagnosis and does **not**
   implement a redesign: doing so unprompted would be the "engine outruns content" mistake
   this whole debt pass exists to avoid. Requires a maintainer design decision before any
   code change.

## What was intentionally not touched

No engine, scenario, or balance code changed. This task is a documented investigation
only. The throwaway `src/bin/ep_diag.rs` is removed after producing the numbers above; the
method is recorded here so the result is reproducible.
