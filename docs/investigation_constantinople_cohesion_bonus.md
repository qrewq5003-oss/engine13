# Investigation — Constantinople 1430 cohesion-bonus coefficient (PR #12)

**Date:** 2026-07-11
**Scenario:** constantinople_1430
**Fix:** `external_pressure_legitimacy_to_cohesion_bonus` coefficient `5.0 → 0.1`
**Status:** mechanism fully explained by data (not hypothesis); fix merged.

---

## Symptom / blocker

The one-line coefficient fix (below) was blocked by an apparent mystery: with the
fix applied, byzantium `military_size` diverged at ~tick 19 between the two
coefficient values — **even though cohesion converges to 100 by tick 9 in both
runs**. A late divergence that outlives the cause looked like a hidden, possibly
non-deterministic coupling, i.e. "the fix breaks the scenario in an unexplained
way." This document records how that was run to ground so it does not resurface
as a mystery later.

The rule being changed:

```toml
[[dependencies]]
id = "external_pressure_legitimacy_to_cohesion_bonus"
from = "external_pressure"
to   = "cohesion"
coefficient = 5.0   # → 0.1
threshold = 65.0
mode = "bonus"      # from > 65 ⇒ cohesion += (from - 65) * coefficient
```

At 5.0, any actor under siege (`external_pressure > 65`) gained
`+5·(ep−65)` cohesion/tick — e.g. ep=75 ⇒ **+50 cohesion/tick** — pinning cohesion
at the 100 clamp. That is the real bug.

## Three refuted hypotheses

### H1 — RNG-count desync (refuted)
*Idea:* changing cohesion changes how many RNG draws happen per tick, desyncing the
stream so all downstream `noise`/combat diverges.
*Refutation:* across a 60-seed sweep (0–59), `military_size` for **both** byzantium
and ottomans is **byte-identical** between the two coefficients on **59 of 60
seeds**. If the stream had desynced, military (which consumes RNG every tick via
`noise` and combat rolls) would differ on *every* seed. It does not. (`run_scripted`
also carries `rng_word_pos` instrumentation that confirms stream position stays
aligned.)

### H2 — memory / history buffer (refuted)
*Idea:* an accumulator (streak/average/cumulative/history) records the cohesion
*trajectory* over ticks 1–8 and replays it into `military_size` after cohesion
converges.
*Refutation:* every stateful structure in the engine was traced:
- `metric_history` (`VecDeque`, last 5 ticks) → `check_actor_upheaval` (>30 swing) →
  `narrative_status`. This **is** a real cohesion-history buffer, but it dead-ends at
  `narrative_status`, which in constantinople only drives UI/logging: all 8 random
  events hardcode `EventTarget::Actor(...)` (the `foreground_ids.choose(rng)` pool is
  never used) and the scenario has no `Foreground/Background` conditions. It never
  reaches `military_size`.
- `collapse_warning_ticks` — requires `cohesion < 15`; both trajectories sit 57–100.
  Never arms.
- `vassalage_warning_ticks` — band is `cohesion 15–30`; never entered; tribute hits
  `treasury` anyway.
- `milestone_condition_ticks` — the only cohesion-gated milestone over byzantium is
  `constantinople_holds` (narrative-only: no metric effect in `apply_milestone_effects`,
  no `spawn_actor`).

No memory buffer connects cohesion to `military_size`.

### H3 — cohesion → legitimacy → military_quality → combat → military_size (refuted)
*Idea:* the early cohesion difference is carried forward by `military_quality` into
the combat outcome.
*Refutation, empirical and structural:* in the paired trace `military_quality`
diverges by **< 0.02** and then clamps to 100 by tick 7 — it carries essentially
nothing. And `military_quality` **does not appear in the combat formula at all**:
`calculate_military_interaction` / `effective_military` decide combat from
`military_size`, `external_pressure`, and static culture/religion affinity; losses
are pure RNG. Legitimacy/quality never enter it.

## Confirmed mechanism

Established with a paired tick-by-tick trace (seed 42), a 60-seed sweep, and an
event dump at the one seed that diverges (seed 40):

1. The coefficient directly changes **cohesion** from tick 2 (intended — 5.0 pins it
   at 100).
2. Through the coupled dependency/event graph (the rule applies to *all* actors), this
   leaves a **persistent deterministic offset on ottoman `external_pressure`**
   — e.g. seed 40: `14.0` (coef 5.0) vs `22.5` (coef 0.1).
3. Combat firing probability includes `pressure_mod = attacker.external_pressure/100 · 0.2`.
   The ottoman-ep offset shifts `final_prob`, moving the tick at which the *fixed* RNG
   roll crosses the firing threshold.
4. Seed 40: `military_conflict_ottomans_byzantium` fires at **tick 12** (coef 5.0) vs
   **tick 10** (coef 0.1). Both actors' `military_size` diverge from that combat on.
5. **Rare:** only 1 of 60 seeds shows any `military_size` divergence, because the ep
   offset only occasionally flips *which tick* a combat lands on.

**Chain:** `coefficient → cohesion → deterministic ottoman external_pressure offset →
combat firing probability → combat tick → military_size`. RNG stays synchronous; the
carrier is a deterministic metric offset feeding a **discrete** combat threshold — not
RNG desync (H1), not a memory buffer (H2), not military_quality (H3).

The observed "tick 19" was simply where that seed's combat-timing flip surfaced; at
seed 40 the same signature lands at tick 10.

## Resolution

The fix is correct. The occasional delayed `military_size` shift is the expected
consequence of a deterministic perturbation crossing a discrete combat threshold —
explained, not mysterious. The original blocker ("breaks the scenario unexplainably")
is retired. PR #12 merged.

## Scope of rebalance needed (read this before starting the follow-up)

Post-fix scripted victory (seed 42, engine a79665d):

| strategy  | pre-fix (5.0, buggy) | post-fix (0.1) | note |
|-----------|----------------------|----------------|------|
| military  | win @43              | win @102       | large, robust shift |
| diplomacy | win @130             | win @131       | ~unchanged |
| balanced  | win @73              | no win ≤300 (also seeds 1/7/13/42/99) | knife-edge, see below |

The scenario is **not** globally unwinnable (military + diplomacy still win). But the
follow-up is a **victory-condition design decision (see below), NOT an isolated "tune
balanced" tweak, and NOT push-magnitude tuning.** Three experiments establish the scope,
and specifically refute the tempting "balanced was propped up by the bug pumping
federation" story:

**1. The passive cohesion→federation channel is not load-bearing.** Ablating the two
federation auto-delta terms the bug could pin (`venice.cohesion>65 → +0.5`,
`genoa.cohesion>55 → +0.3`) while keeping coef 5.0 leaves balanced winning **@73,
unchanged**. The win was never carried by "pinned cohesion → passive federation push."

**2. The early game is coefficient-insensitive.** At 50 ticks, coef 5.0 and coef 0.1
balanced runs apply an **identical** action economy (milan_bankers 50,
venice_diplomacy 44, genoa_mercenaries 41, …) and both reach federation **max 100.0**.
The besieged actor is byzantium; the patrons venice/genoa/milan are not under
`external_pressure > 65`, so the bonus rule never pinned *their* cohesion — the
coefficient only moved *byzantium* cohesion. So the difference is not the patron
action economy either.

**3. Under BOTH coefficients balanced is a knife-edge, never a robust win.**
`federation_progress` asymptotes at ~90–100 and oscillates against the hard 100 clamp
in *both* configs. Victory requires 3 consecutive ticks where **mid-tick**
(post-patron-action) federation ≥ 100; later-phase random events (cardinal_death −8,
etc.) plus the `byz external_pressure>70 → federation −2.0` drag knock it back below
100 within the same tick. (This is why the sim's *end-of-tick* `fed` display reads
<100 on ticks where `sustained` increments — `check_victory_condition` runs mid-tick,
the display is end-tick; not a bug.) The bug's byzantium-cohesion perturbation merely
tipped *which* bounces caught `sustained=3`: seed 42 caught it at tick 73 under 5.0
and never within 300 under 0.1. **So balanced's "win @73" was a lucky threshold catch
on a noisy bounce, not honest robust balance — neither a clean bug-pumped win nor a
strategy that "worked and then broke."**

**Corroboration that this is not balanced-specific:** military's **43 → 102** shift is
large and non-knife-edge, and diplomacy is ~unchanged (130 → 131). The coefficient
changes *when* every strategy approaches the federation ceiling — the whole scenario's
victory timing rode on pinned byzantium cohesion. The bug **masked** the fragility; it
did not create it.

### Root cause: the victory condition sits at the edge of the metric's reachable range

`federation_progress` is hard-clamped at 100. The victory condition is
`federation ≥ 100` **sustained for 3 consecutive ticks** — the threshold is placed
*exactly at the ceiling of the metric's reachable range*. Federation can touch 100 but
never rise above it, so it has no headroom to absorb the ±5–8 swings from random events
(cardinal_death −8, mehmed_threatens, …) plus the `byz ep>70 → −2.0` drag. Holding ≥100
for three ticks straight is therefore a matter of **luck against noise, not strategy** —
"win @73" was a lucky run of three quiet ticks at the top, not a stable balance point.
This is a **design** property of the condition, independent of the bug and of any
push-magnitude tuning.

> **NB (added 2026-07-11).** The specific numbers in this section are superseded: the
> victory condition is `federation_progress >= 80` (not 100), and its real blocker was the
> `byzantium.external_pressure < 85` gate, which ep saturation made *unreachable*. That
> does not change the class described here — a threshold placed outside the metric's
> genuinely traversable range — it only moves which metric the class applies to.
>
> **This "edge-of-range" class now has a SIBLING class: "unbounded flow"** — a metric
> pumped by uncapped patron actions runs away until every outcome depending on it becomes
> deterministic (Milan D peaking at exactly 64.00 in 15/15 seeds; Byzantium reaching 637
> and grinding the Ottomans to 0 in 5/5 seeds). The two are related but distinct: here the
> *condition* is broken while the metric is fine; there the *metric itself* is broken. Both
> recur, neither is caught by `cargo test`. Both are candidates for one v2 review of how the
> engine handles unbounded metrics and threshold conditions. See the "СИСТЕМНЫЙ КЛАСС"
> section of [`investigation_combat_asymmetry.md`](investigation_combat_asymmetry.md).

### One class of problem, with a second known instance

> **Victory/trigger conditions calibrated at the edge of a metric's reachable range are
> noise-fragile by construction.** When a threshold sits at a metric's ceiling — or in a
> narrow transient band it only passes through — ordinary per-tick noise, not player
> strategy, decides whether the condition catches. Success then looks random, and the
> condition looks "broken" under any recalibration.

Known instances:
1. **constantinople_1430** — `federation_progress ≥ 100` sustained 3 ticks, against the
   hard 100 clamp (this document).
2. **vassalage trigger** — the `external_pressure` 70–85 band is a *transient window
   during monotonic growth to saturation at 100*, not a stable reachable zone, so the
   trigger effectively never fires. Documented in `ENGINE13_SCENARIO3_DESIGN.md` →
   `vassalage-pressure-reachability`.

These are **not two separate bugs** — the same structural mistake in two places.

### Follow-up = a design decision, not magnitude tuning (v2-class)

Tuning patron-action `+federation` values or the drag treats the *symptom*. The real
choice is to move the condition off the edge of the range — exactly the decision the
vassalage trigger also needs:

- **lower the threshold below the ceiling** (e.g. `federation ≥ 90` sustained, leaving
  headroom for noise), **or**
- **drop the sustained-3-ticks requirement** in favour of a single achievement (reach
  100 once), removing the "hold at the ceiling through noise" demand entirely.

Either makes victory depend on the strategy reaching the target rather than on a quiet
noise window at the top. Both are **v2-class design work** (they change what winning
*means*), not a quick tweak — and constantinople and vassalage should be decided
together, because it is the same decision.

**Not traced here (belongs to that task, not this fix):** the exact byzantium-cohesion →
federation-perturbation micro-paths. That is the expensive part, deliberately left out.

## Method / reproduction

- Coefficient values compared by overriding `DependencyRule.coefficient` at runtime
  after `registry::load_by_id` (avoids recompiling per value), seed fixed.
- 60-seed sweep over `military_size` divergence; event dump at seed 40, ticks 8–12.
- The throwaway trace binary was removed after use; the procedure above reproduces it.
