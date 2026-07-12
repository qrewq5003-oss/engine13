# ENGINE13 Simulation Baseline

**Recorded:** After Rome balanced fix and knowledge → legitimacy bridge
**Date:** 2026-03-09
**Commit:** Post "Fix: improve Rome balanced path and give knowledge a safe support role via legitimacy"

---

## Post Combat Termination Guard (#17 fix)

**Date:** 2026-07-12.
**Change:** `src/engine/interactions.rs` — one guard in `calculate_military_interaction`:
a fight is not started if the defender's `military_size` is below
`MIN_DEFENSIBLE_MILITARY` (0.01). The loss formula, the probability formula and the
`effective_military` divisor are **untouched**.

Closes the defect diagnosed in
[`investigation_combat_self_destruction.md`](investigation_combat_self_destruction.md):
combat had no termination condition, so an army that had already been destroyed stayed
a legal target forever, and the attacker kept paying 5–15% of its own military per
fight to storm an empty field. 81–95% of *all* fights in the project's baselines were
these.

The threshold is not a new invented number: `0.01` is the same constant
`src/bin/combat_probe.rs` already used to classify "fight against an already-empty
army", and the probe now imports it from the engine. The set of fights the guard
removes is therefore exactly the set the investigation measured. (It has to be an
epsilon rather than `== 0.0`: losses are multiplicative, `mil * (1 - loss)`, so a
beaten army decays asymptotically and never lands on exactly zero.)

### Result (seed 42, 300 ticks, no-player, `combat_probe`)

| Scenario | Fights before → after | Phantom fights before → after | Attacker's own military destroyed in them |
|---|---|---|---|
| `constantinople_1430` | 37 → **2** | 35 → **0** | 244.90 → **0** |
| `rome_375` | 67 → **15** | 52 → **0** | 88.65 → **0** |
| `milan_1477` | 623 → **34** | 585 → **0** | 259.96 → **0** |

The winners stop destroying themselves: the Ottomans finish at **304.69** instead of
**54.88**, and in `milan_1477` armies exist at all at the end (venice 54.50, genoa
42.00, naples 29.50, milan 13.50 — previously all but three actors sat at 0.00).

### Victory condition: unaffected, measured, not argued

`ottomans.military_size < 40` (PR #14) was the obvious thing to break. Scripted
`balanced`, 300 ticks, seeds 42/1/7/13/99: **victory on all five seeds, at exactly the
same ticks as before — 110 / 91 / 134 / 88 / 97.** Not "within the spread": identical.

This is the orthogonality claimed by the investigation, now demonstrated. In scripted
mode Byzantium is continuously armed (+5.5/tick), so its `military_size` never falls
below the threshold, and **the guard never fires at all**. Phantom fights are a
property of the no-player world — which is precisely where the balance baselines are
built.

60 tests green.

### The consequence that matters: actor mortality was a product of the defect

**This is a real behavioral regression and it is not cosmetic.** Collapse rates in the
no-player world:

| Scenario | Collapses before | Collapses after |
|---|---|---|
| `rome_375` | ostrogoths 99/100 runs, ostrogoth_kingdom 89, armenia 82 | **late_sassanids 1, sassanids 1** |
| `constantinople_1430` | 4% of runs | **0** |

Both collapse paths in `check_collapses` (`engine/mod.rs`) require **low cohesion** —
`legitimacy<10 && cohesion<15 && external_pressure>85`, or `legitimacy<5 && cohesion<8`.
The only thing that pushed cohesion down was `cohesion_loss = 10–20`, applied to the
defender **on every fight** — including every phantom one. Remove the phantom fights and
nothing drives cohesion down any more.

Traced on the ostrogoths (`rome_375`, seed 42) with the guard active: military `0.00`
from tick 41, legitimacy `0.00` from tick 101, external_pressure saturated at `100` —
and cohesion **recovers to 82** (`calculate_diplomatic_interaction` pushes it up, and
nothing pushes back). Two of the three collapse gates are wide open; cohesion alone
holds the actor alive. The same mechanism now keeps **byzantium** — the protagonist —
alive at `military_size 0.00` in the 300-tick no-player probe, where it used to die.

So a defenceless actor is now **immortal**, which is the opposite failure of the one
this PR fixes, and it lives in a different place: the collapse condition, not the combat
loop. The pre-fix mortality was real in the sense that actors did die — but it was
produced by armies that did not exist being ground down by battles that were not
happening.

**Not fixed here, deliberately.** Restoring mortality by a mechanism that isn't a
phantom battle (re-tune the collapse gate so an actor with no army, no legitimacy and
saturated pressure dies regardless of cohesion — or give a defenceless neighbour a real
end via conquest/vassalage) is a separate balance change with its own baseline. It is
tracked as "Пересчёт смертности (guard/conquest)" in
[`ENGINE13_INFRASTRUCTURE_TASKS.md`](../ENGINE13_INFRASTRUCTURE_TASKS.md).

---

## Post Constantinople Military-Flow Caps (task 5, level 1 fix)

**Date:** 2026-07-11, **recalibrated 2026-07-12** (task 6, rebased onto `main` after #19/#20).
**Change:** `constantinople_1430/auto_deltas.toml` only — two new/extended auto_deltas.
No engine change, no action change, no victory-threshold change.

Caps the inflow asymmetry found in
[`investigation_combat_asymmetry.md`](investigation_combat_asymmetry.md): Byzantium's
military inflow was **uncapped** (+5.5/tick from three patron actions) against an Ottoman
trickle (+0.3/tick) — an ~18× asymmetry that drove Byzantium to 637 by tick 200 and ground
the Ottomans deterministically to 0 in **every** seed.

**It caps the asymmetry; it does not make the outcome non-degenerate.** The Ottomans are
still ground to `0.00` in 27 of 30 runs — see "Result" below, which is the *recalibrated*
measurement, not the one this fix was originally written against.

1. **Byzantium military upkeep (diminishing returns).** Cumulative attrition tiers on
   `byzantium.military_size`: `>50 → −1.5`, `>90 → −2.5`, `>130 → −5.0`. A besieged rump
   state cannot sustain an unbounded field army. Chosen over a hard cap because
   `available_if` is a *single* condition — a cap gate would have to displace the existing
   patron-treasury gate. Precedent: stacked negative auto_delta conditions, used throughout
   this same file.
2. **Ottoman growth lever.** Extends their existing auto_delta (was a flat +0.5): `+1.2`
   when `global:federation_progress > 40`, and `+2.0` when `byz/ott military ratio > 0.4`.
   Straight from the scenario's own `llm_context` — *«Мехмед не пассивен. Если федерация
   растёт — он форсирует»*. Precedent: the byzantium-pressure auto_delta already uses both
   a `global:` condition and exactly this byz/ott ratio pair.

Levels 2 (`effective_military` divisor) and 3 (attacker/defender loss asymmetry) were
**deliberately not touched** — the investigation's counterfactual proved neither is the
root cause, and level 2 risks all three scenarios.

### Result — recalibrated on today's `main` (task 6, after #19 / #20)

> **These numbers replace the ones this section originally carried.** The original
> calibration was measured on a `main` where the metric-scoping bugs were still live, so
> **no common event could affect any actor** (see
> [`investigation_metric_scoping.md`](investigation_metric_scoping.md)). #19 and #20 fixed
> that, which changed the very inflows this fix caps — so every number below was re-derived
> from scratch, not re-checked. What the flow caps do and do not achieve changed materially.

Victory tick, `scripted`, 300 ticks, `−` = no victory:

| strategy | s1 | s7 | s13 | s42 | s99 | s3 | s21 | s55 | s77 | s88 |
|-----------|----|----|-----|-----|-----|----|-----|-----|-----|-----|
| balanced  | 91 | 134 | 88 | 110 | 97 | 118 | 140 | 144 | 95 | 132 |
| military  | 101 | **−** | 129 | 166 | 145 | 130 | 113 | 142 | 131 | 84 |
| diplomacy | 160 | 158 | **−** | 151 | 159 | 219 | 140 | **−** | 225 | 190 |

**What the fix does achieve:**

- **Byzantium is bounded.** Peak `military_size` **96–127** across seeds, against **637 by
  tick 200** uncapped. The ~18× inflow asymmetry is genuinely capped — this is the core of
  the fix and it holds.
- **Victory stays achievable but is no longer guaranteed:** 27 of 30 runs win, 3 end with
  no victory (military s7, diplomacy s13/s55). Before any fix, *every* strategy won on
  *every* seed.
- **Timing spread appeared:** balanced was a 49–59 band (10 ticks) before any fix, and
  48–65 (17 ticks) on `main` today; it is now **88–144** (a 56-tick band).

**What it does NOT achieve — and this section previously claimed it did:**

- **The Ottomans are still ground to exactly `0.00`, in 27 of 30 runs.** The original claim
  — *"Ottomans are a real force: they oscillate around 130–140 instead of being ground to 0"*
  — **does not hold on today's `main`**. The ottoman parity it reported was measured in a
  world where `plague` could not hit the Ottomans and `mercenary_influx` could not feed
  Byzantium (+30 `military_size` a fire). With those events live, the growth lever no longer
  holds the Ottomans up.

Tier `>130 → −5.0` is a **near-binding guardrail**, not dead weight: byzantium peaks at 127.
It matters *more* now than at original calibration, precisely because `mercenary_influx`
adds +30 spikes on top of the patron inflow it caps.

### The lever cannot be tuned out of this — the endgame is bimodal

Recalibration sweep on the ottoman growth lever (5 seeds, `scripted balanced`):

| variant | victory ticks | ottoman outcome |
|---|---|---|
| as authored (`fed>40 → +1.2`, `ratio>0.4 → +2.0`) | 88–134, 5/5 win | `0.00` in 5/5 |
| stronger arms race (`ratio>0.4 → +3.5`) | 88–233, **4/5** win | `0.00` in 4/5; **s7: Ottomans run away to 211** |
| + ottoman war chest (`treasury>500 → +2.0`) | 153–262, **4/5** win | `0.00` in 4/5; **s7: Ottomans run away to 229** |

Feeding the Ottomans harder does not produce a *stable balance of power* — it **flips which
side is annihilated**. There is no stable middle: either the coalition grinds the Ottomans
to 0, or the Ottomans run away and the coalition never wins. That bimodality is the
no-termination combat cliff documented in
[`investigation_combat_self_destruction.md`](investigation_combat_self_destruction.md)
(task 5, part 2): once either side gains the advantage, the loser drops from ~90 to 0 in
roughly ten ticks. **It cannot be fixed by auto_delta calibration**, and this fix does not
claim to fix it. The thresholds are therefore kept **as authored** — they are the best of
the tested options, and strengthening the lever only trades one degenerate mode for another.

### Victory threshold `ottomans.military_size < 40` — re-verified, still NOT recalibrated

The ottoman *minimum* per run remains cleanly **bimodal**: **`0.00`** when the coalition
breaks through (27/30 runs) or **65–148** when the Ottomans hold (3/30 — exactly the three
no-victory runs). Across all 30 runs nothing lands in 40–65, so the threshold 40 still sits
in a clean gap between the two modes rather than on a knife-edge. **No recalibration needed**
— the conclusion survives, on new numbers.

### Verification

`cargo test` green (60 passed); no Rust changed (TOML only), so clippy is untouched.
`rome_375` and `milan_1477` sim output **byte-identical** before/after (batch, 100 seeds ×
50 ticks; plus single seeds 42/1/7 × 200 ticks) — they use neither these actors nor these
auto_deltas. Constantinople's no-player batch is materially unchanged (random events/run
67.3 → 67.4); the fix bites in `scripted` mode, where the patron inflow it caps exists.

---

## Post Constantinople Victory-Condition Redesign (external_pressure → ottomans.military_size)

**Date:** 2026-07-11
**Change:** constantinople_1430 `victory_condition.additional_conditions`:
`actor:byzantium.external_pressure < 85` → `actor:ottomans.military_size < 40`.
Main metric (`global:federation_progress >= 80`), `minimum_tick: 40`, and
`sustained_ticks_required: 3` are unchanged.

### Why

The old `external_pressure < 85` gate was **structurally unreachable under
balanced play**: byzantium external_pressure saturates *upward* toward ~100
within the first few ticks and has no decay path, so it never falls below 85.
`federation_progress` holds `>= 80` on 100% of ticks from tick 40 onward, yet
balanced never won on any of seeds 1/7/13/42/99 (≤300 ticks) — the ep gate
alone blocked it every tick. `ottomans.military_size` instead *decays* through
combat losses (180 → ~0 by tick ~150), making `< 40` reachable. Full diagnosis:
the ep and military-size traces in this PR's description.

### Post-change victory ticks (300 ticks, seeds 1 / 7 / 13 / 42 / 99)

| Strategy  | OLD gate `ep < 85`                          | NEW gate `ottomans.military_size < 40` |
|-----------|---------------------------------------------|----------------------------------------|
| military  | no / no / no / 101 / 93                      | **42 / 42 / 42 / 42 / 42** |
| diplomacy | no / 126 / 131 / 130 / 161                   | **76 / 78 / 70 / 76 / 76** |
| balanced  | no / no / no / no / no                       | **53 / 54 / 51 / 49 / 59** |

("no" = no victory within 300 ticks.) Balanced — the previously unwinnable
flagship path — now wins robustly in every seed. Military wins at tick 42
(= `minimum_tick` 40 + `sustained` 3, the earliest legal tick, since it breaks
the Ottoman army below 40 before tick 40). Diplomacy wins ~70-78.

### ⚠ Warning 1 — this gate now controls TIMING, not ACHIEVABILITY

`ottomans.military_size` marches deterministically to ~0 in **every** balanced
run of **every** seed (it is combat-driven and monotone downward, floored at 0).
Consequently **any** threshold in the reachable band (empirically ~10–150) yields
a **100% win rate** under balanced play — the threshold only moves the victory
*tick*, it does not decide *whether* the player wins. This is the opposite
failure mode of the old ep gate (0% win rate, unreachable). If a future design
goal is "balanced should sometimes NOT win," a single `military_size` threshold
cannot express that; a composite condition would be required. `< 40` was chosen
because it sits mid-band (window traversed is 0–88 during the fed≥80 window),
giving ~10-20 ticks of play past `minimum_tick` rather than an instant win.

### ⚠ Warning 2 — deeper coalition-vs-Ottoman combat imbalance (NOT fixed here)

The reason `military_size` "works" as a gate is that the coalition annihilates
the 180-strong Ottoman field army down to 0 by ~tick 150 in *every* balanced
run — the Ottomans, the scenario's nominal looming threat, are deterministically
destroyed rather than posing a sustained danger. This is a separate balance
concern, logged as **Задача 5** in `ENGINE13_INFRASTRUCTURE_TASKS.md` (not
addressed in this PR).

---

## Post Constantinople Cohesion-Bonus Coefficient Fix (PR #12)

**Date:** 2026-07-11
**Change:** `external_pressure_legitimacy_to_cohesion_bonus` coefficient `5.0 → 0.1`
(constantinople_1430 `dependencies.toml`). See
[`investigation_constantinople_cohesion_bonus.md`](investigation_constantinople_cohesion_bonus.md)
for the full diagnosis.

### Why the older constantinople victory ticks below are invalid

At coefficient 5.0 the bonus rule dumped `+5·(external_pressure−65)` cohesion/tick
onto **every** actor under siege (`external_pressure > 65`), pinning cohesion at the
100 clamp. Because the rule is global, venice/genoa cohesion were also pinned at 100,
which fed `federation_progress` a constant artificial tailwind
(`venice.cohesion>65 → +0.5`, `genoa.cohesion>55 → +0.3`, i.e. **+0.8/tick**). Every
"Victory Tick" recorded for constantinople in the sections below was sampled under
that pinned-cohesion bug and is **not a valid balance baseline**. The values here
replace them.

### Post-fix deterministic baseline (seed 42, engine a79665d)

| Strategy  | Pre-fix (5.0, buggy) | Post-fix (0.1) | Status |
|-----------|----------------------|----------------|--------|
| military  | win @43              | win @102       | ✓ still winnable, delayed |
| diplomacy | win @130             | win @131       | ✓ ~unchanged |
| balanced  | win @73              | **no win ≤300** (also no win at seeds 1/7/13/42/99, 200 ticks) | ⚠ regressed — see note |

Measured on current engine (a79665d) by comparing the toml at 5.0 vs 0.1, seed 42,
`scripted` mode. The stale D3 table below (2026-07-03) predates recent engine changes
and its constantinople numbers should not be diffed against these.

### Is the scenario still winnable? (answer: yes)

**Yes — via military (tick 102) and diplomacy (tick 131).** The scenario is *not*
globally unwinnable. The military delay (43 → 102) and the ~unchanged diplomacy path
are the expected consequence of removing the artificial cohesion pin, and are
acceptable ("victory moved later but is achievable").

The **balanced** strategy no longer reaches victory (unwinnable at 300 ticks and
across seeds 1/7/13/42/99). Crucially this is **not** an isolated balanced tweak:
experiments show balanced was a knife-edge under *both* coefficients (federation
oscillates at ~90–100 against the hard 100 clamp; the old "win @73" was a lucky
`sustained=3` catch, not robust balance), and the coefficient shifts victory timing
scenario-wide (military 43→102 is the same coupling). The follow-up is therefore a
**scenario-level federation/victory calibration**, comparable to calibrating Milan
1477 from scratch — see the "Scope of rebalance needed" section in
[`investigation_constantinople_cohesion_bonus.md`](investigation_constantinople_cohesion_bonus.md).
It does **not** block the coefficient fix, which is correct on its own merits.

---

## Post Determinism Fix (D3) Baseline

**Date:** 2026-07-03
**Changes:** Made fixed-seed simulation reproducible run-to-run (task D3)

### Root cause

The `sim` binary already runs on an explicitly seeded `ChaCha8Rng`
(`ChaCha8Rng::seed_from_u64(seed)`), yet the same seed produced different
numbers on every process launch. Cause: `world.actors` is a `HashMap`, whose
iteration order is randomized per process (Rust `RandomState`). Two RNG-consuming
engine phases iterated that map and let its order decide the order/target of RNG
draws:

1. `engine/interactions.rs::get_neighbor_pairs` — built the neighbor-pair list
   by iterating `world.actors`; each pair then consumes `rng` (military rolls,
   etc.), so pair order changed the RNG draw sequence.
2. `engine/mod.rs::phase_random_events` — built `foreground_ids` from
   `world.actors.values()`; `foreground_ids.choose(rng)` then picks by index,
   so the map order decided which actor an event hit.

A third, harness-only source lived in `bin/sim.rs`: the "actions/actors by count"
reports sorted a `HashMap` view by count only, so equal-count entries tied in
random order (display order only — not simulation state).

### Fix

- `get_neighbor_pairs`: `pairs.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)))`
  before returning (deterministic canonical pair order).
- `phase_random_events`: collect `foreground_ids` into a `mut` Vec and `.sort()`.
- `bin/sim.rs`: count sorts get a secondary tie-break on the key
  (`b.1.cmp(a.1).then_with(|| a.0.cmp(b.0))`).

No engine phase order, formula, threshold, or scenario balance was changed — only
iteration order within two RNG-consuming phases was pinned. Fixed-seed mode itself
already existed (`sim <scenario> <ticks> <seed>` → `run_single`); no new mode was
needed.

### Verification method (now seed-based, not eyeballed ranges)

Reproducibility is now checked by byte-identical output under a fixed seed, not by
comparing value ranges by hand. Each command is run 3× and the outputs `md5sum`-ed;
all hashes must be identical:

```bash
for cmd in \
  "rome_375 50 42" "rome_375 50 scripted balanced" "rome_375 50 scripted influence" \
  "rome_375 50 scripted wealth" "rome_375 50 batch" \
  "constantinople_1430 50 42" "constantinople_1430 50 batch" \
  "constantinople_1430 25 scripted balanced" "constantinople_1430 25 scripted diplomacy" \
  "constantinople_1430 25 scripted military"; do
  h=$(for r in 1 2 3; do cargo run --bin sim $cmd 2>/dev/null | md5sum; done | sort -u | wc -l)
  [ "$h" = 1 ] && echo "IDENTICAL  | sim $cmd" || echo "DIVERGENT  | sim $cmd"
done
```

Result: all 10 scenario/mode combinations `IDENTICAL x3`. `cargo test --workspace`
→ 46/46 pass. `cargo clippy --workspace` → no new warnings.

### Deterministic baseline values (seed 42 / fixed batch seeds 0–99)

| Run | Result |
|-----|--------|
| rome_375 50 42 | random events 759, gen transitions 1, foreground shifts 15, FINAL military 337.4 / cohesion 73.7 / legitimacy 52.6 |
| rome scripted balanced (50) | Victory tick 31 (influence → 99.8) |
| rome scripted influence (50) | Victory tick 35 |
| rome scripted wealth (50) | no victory (tempo-resource path, as designed) |
| constantinople scripted military (50) | Victory tick 43 |
| constantinople scripted balanced / diplomacy (50) | no victory within 50 ticks |
| constantinople batch (50) | random events avg 50.9 |

> **Note on older tables below:** every "Victory Tick" recorded before this section
> was sampled from a *non-deterministic* run (one arbitrary HashMap order), so those
> numbers were never reproducible and should not be diffed against. This section is
> the first reproducible baseline.

### Did the fix change any scenario outcome? (regression audit)

A determinism fix must only make the outcome *reproducible*, never silently flip
*who wins*. Audited by characterizing the pre-fix (non-deterministic) victory
distribution — 15 runs per scripted strategy — against the post-fix deterministic
outcome (internal seed 42):

| Strategy | Pre-fix wins (15 runs) | Pre-fix victory tick(s) | Post-fix (deterministic) | Verdict |
|----------|------------------------|-------------------------|--------------------------|---------|
| constantinople military | 15/15 | all 43 | win @ 43 | ✓ same |
| constantinople diplomacy | 0/15 | never | no win | ✓ same |
| constantinople balanced | 3/15 (~20%) | 43 when it wins | no win | ⚠ marginal — see below |
| rome balanced | 15/15 | 31–43 | win @ 31 | ✓ same (in range) |
| rome influence | 12/15 (~80%) | 31–44 | win @ 35 | ✓ same (in range) |
| rome wealth | 0/15 | never | no win | ✓ same |

**No strategy flipped from always-win to never-win.** For 5 of 6 strategies the
deterministic outcome equals the pre-fix unanimous/majority outcome (and any pinned
victory tick falls inside the pre-fix range). The single nuance is
**constantinople scripted balanced**: in the *current* code it is already a marginal
~20%-win strategy (NOT the reliable "tick 24" from the stale PR-D table above —
balance drifted between 2026-03-11 and now, independently of this fix). Determinism
necessarily commits a mixed-outcome strategy to one sample; seed 42's pinned order
lands on the ~80% majority "no sustained victory" outcome. No formula, threshold, or
balance value was touched by this fix — only iteration order was pinned — so this is
sampling of a pre-existing distribution, not a balance regression.

---

## Post PR-D Baseline

**Date:** 2026-03-11
**Changes:** Actions and rank bonuses migrated from hardcoded Rust to TOML

### Summary of Changes
- **RankBonusEffect struct:** metric, delta (flat), floor (optional minimum value)
- **RankBonusRule struct:** rank (RegionRank), effects (Vec<RankBonusEffect>)
- **Scenario.rank_bonuses:** Vec<RankBonusRule> loaded from rank_bonuses.toml per scenario
- **ActionCondition serde:** Changed to internally tagged enum with `type = "always" | "metric"`
- **phase_region_ranks:** Now iterates scenario.rank_bonuses instead of hardcoded match
- **Files added:** `src/scenarios/rome_375/actions.toml`, `src/scenarios/rome_375/rank_bonuses.toml`
- **Files added:** `src/scenarios/constantinople_1430/actions.toml`, `src/scenarios/constantinople_1430/rank_bonuses.toml`
- **Removed:** `fn create_patron_actions()` from both scenarios
- **Removed:** `fn create_universal_actions()` from Rome 375

### Baseline Results

| Run | Victory Tick | Victory Year | Notes |
|-----|-------------|--------------|-------|
| rome scripted balanced (100 ticks) | Tick 31 | Year 385 (15.5yr) | ✓ Matches PR-C baseline |
| rome scripted influence (100 ticks) | Tick 31 | Year 385 (15.5yr) | ✓ Matches PR-C baseline |
| constantinople balanced (50 ticks) | Tick 43 | Year 1451 (21.5yr) | ✓ Matches PR-C baseline |

### Verification
- All 37 tests pass
- `rg "create_patron_actions" src/` — empty (no old functions)
- `cargo check` — no errors
- Victory ticks match PR-C baseline exactly

---

## Post PR-C Baseline

**Date:** 2026-03-11
**Changes:** Interaction rules infrastructure added (no existing interactions migrated)

### Summary of Changes
- **InteractionRule struct:** Added to `scenario.rs` with fields: id, max_distance, border_type, cooldown_ticks, conditions, effects, event_type, event_threshold
- **InteractionCondition struct:** actor (Source/Target), metric, operator, value
- **InteractionEffect struct:** actor (Source/Target), metric, delta (flat constant)
- **ConditionActor enum:** Source, Target — which actor a condition/effect applies to
- **Scenario.interaction_rules:** Vec<InteractionRule> loaded per scenario (empty for Rome/Constantinople)
- **validate_interaction_rules:** Validates at load time — unique IDs, max_distance >= 1, valid border_type, event consistency, non-empty effects, known metrics
- **apply_interaction_rule:** Order: distance → border → cooldown → conditions → effects; unknown border_type = panic
- **Pipeline integration:** Data-driven rules called between migration and cultural interactions
- **ComparisonOperator serde:** Uses snake_case ("less", "less_or_equal", "greater", "greater_or_equal", "equal")

### What PR-C Does NOT Do
- Does NOT migrate Trade interaction (dynamic formula: `(eco_a + eco_b) * 0.002 * modifier`)
- Does NOT migrate Diplomatic interaction (asymmetric stronger/weaker logic)
- Does NOT migrate Migration interaction (dynamic: `(pressure - 65.0) * 0.2 / distance`)
- All three require formula-based effects; will be addressed in PR H for third scenario

### Baseline Results

| Run | Victory Tick | Victory Year | Notes |
|-----|-------------|--------------|-------|
| rome scripted balanced (100 ticks) | Tick 31 | Year 385 (15.5yr) | ✓ Matches PR-B baseline |
| constantinople balanced (50 ticks) | Tick 43 | Year 1451 (21.5yr) | ✓ Matches PR-B baseline |

### Verification
- All 37 tests pass
- `cargo check` — no errors
- Hardcoded interactions (Military, Trade, Diplomatic, Migration, Cultural) unchanged

---

## Post PR-B Baseline

**Date:** 2026-03-11
**Changes:** Dependency graph migrated from hardcoded Rust constants (COEF/THRESH) to TOML-based dependency rules

### Summary of Changes
- **DependencyRule struct:** Added to `scenario.rs` with fields: id, from, to, coefficient, threshold, mode
- **DependencyMode enum:** Deficit, Excess, Bonus, Linear modes for different dependency types
- **Scenario.dependencies:** Vec<DependencyRule> loaded from dependencies.toml per scenario
- **validate_dependencies:** Validates rules at load time — typos in metric names or missing thresholds fail fast
- **apply_dependency_rule:** Sequential mutation semantics — each rule reads current actor state (modified by previous rules)
- **phase_apply_dependencies:** Rules applied in strict file order — order is part of simulation logic
- **Removed:** struct Coefficients, struct Thresholds, static COEF, static THRESH, apply_dependency_graph function
- **Files added:** `src/scenarios/rome_375/dependencies.toml`, `src/scenarios/constantinople_1430/dependencies.toml`

### Baseline Results

| Run | Victory Tick | Victory Year | Notes |
|-----|-------------|--------------|-------|
| rome scripted balanced (100 ticks) | Tick 31 | Year 385 (15.5yr) | ✓ Matches PR-A baseline |
| rome scripted influence (100 ticks) | Tick 31 | Year 385 (15.5yr) | ✓ Matches PR-A baseline |
| constantinople balanced (50 ticks) | Tick 43 | Year 1451 (21.5yr) | ✓ Matches PR-A baseline |

### Verification
- All 37 tests pass
- `rg "COEF\.|THRESH\." src/` — empty (no old constants)
- `rg "apply_dependency_graph|struct Coefficients|struct Thresholds" src/` — empty (no old code)
- `cargo check` — no errors

---

## Post Half-Year Baseline

**Date:** 2026-03-10
**Changes:** 2 ticks per year globally, year derived from tick/2, patriarch ages on even ticks only

### Summary of Changes
- **Year formula:** `year = start_year + (tick / 2)` — 2 ticks = 1 year
- **HalfYear:** Computed from `tick % 2` (even = FirstHalf, odd = SecondHalf)
- **Patriarch aging:** +1 age on even ticks only (FirstHalf)
- **Victory minimum_tick:** Rome 15yr→30, Constantinople 20yr→40
- **UI display:** "Year AD — Первая/Вторая половина года"
- **Backward compatibility:** Year recalculated from tick on save load

### Baseline Results

| Run | Victory Tick | Victory Year | Gen Transfers | Notes |
|-----|-------------|--------------|---------------|-------|
| rome batch (100 ticks) | — | 50 years | 2.0 avg | 100 ticks = 50 years |
| rome scripted balanced (100 ticks) | Tick 31 | Year 385 (15.5yr) | 0 | Victory at 15.5 years (min 15yr) |
| constantinople balanced (50 ticks) | Tick 43 | Year 1451 (21.5yr) | — | Victory at 21.5 years (min 20yr) |

### Balance Review
- **Victory timing correct** — Rome victory at tick 31 (~15.5 years), after minimum 30 ticks (15 years)
- **Victory timing correct** — Constantinople victory at tick 43 (~21.5 years), after minimum 40 ticks (20 years)
- **Generation transfers working** — 2.0 avg per 100-tick (50 year) batch run
- **Half-year model stable** — No issues with year/half calculation

---

## Post Generation-Mechanics Baseline

**Date:** 2026-03-10
**Changes:** Early transfer trigger, generation_count tracking, sim reporting by event_id

### Summary of Changes
- **EarlyTransfer struct:** Allows generation transfer before normal age if conditions met
  - Rome 375: age >= 65 AND rome.external_pressure > 70
- **generation_count:** Tracks number of transfers in FamilyState
- **Strict transfer order:** increment count → apply coefficients → reset age → log event
- **Event ID:** Strictly "generation_transfer" for sim counting
- **tick_span:** Changed from 5 to 1 for Rome 375 (1 tick = 1 year)

### Baseline Results

| Run | Victory Tick | Gen Transfers | Transfer Ticks | Notes |
|-----|-------------|---------------|----------------|-------|
| rome batch | — | 2.0 avg | ~33 | Normal transfer expected at tick 33 (patriarch 42→75) |
| rome scripted balanced | Tick 16 | 0 | — | Run too short (16 ticks) for transfer |
| rome scripted influence | Tick 16 | 0 | — | Run too short (16 ticks) for transfer |

### Balance Review
- **Generation transfers working** - 2.0 avg per 50-tick batch run
- **Transfer timing correct** - Expected at tick ~33 (patriarch starts 42, ends 75, +1/tick)
- **Victory timing unchanged** - Still Tick 16 for scripted strategies
- **No critical bugs** - Transfers occurring as expected

---

## Post Region-Rank Baseline

**Date:** 2026-03-10
**Changes:** Fixed economic_output delta per tick, legitimacy floor for rank S

### Summary of Changes
- **economic_output deltas (per tick):**
  - Rank S: +0.5
  - Rank A: +0.3
  - Rank B: +0.1
  - Rank C: 0.0
  - Rank D: -0.2
- **legitimacy floor:** Rank S actors cannot drop below 20.0
- Fixed deltas are non-compounding (constant, not % of current value)

### Baseline Results

| Run | Victory Tick | Collapses | Notes |
|-----|-------------|-----------|-------|
| rome batch | — | ostrogoths: 29 | Collapses stable vs collapse-fix baseline (29 vs 26) |
| rome scripted influence | Tick 16 | none | Victory tick unchanged |
| constantinople balanced | Tick 24 | none | Victory tick unchanged, +2 actions applied |

### Balance Review
- **Victory timing stable** - no shift in victory tick
- **Collapse count stable** - ostrogoths 29 vs 26 (within normal variance)
- **No saturation observed** - no actor holds economic_output = 100 for >10 ticks
- **No balance review required** - region rank bonuses work as intended without disrupting existing balance

---

## Post Collapse-Fix Baseline

**Date:** 2026-03-10
**Changes:** AND logic for collapse conditions, 3-tick counter, dual collapse paths

### Summary of Changes
- Collapse now requires **3 consecutive ticks** in dangerous state (not instant)
- Two collapse paths:
  - **Classic:** legitimacy < 10 AND cohesion < 15 AND external_pressure > 85
  - **Internal:** legitimacy < 5 AND cohesion < 8
- Counter resets if actor exits dangerous state
- `external_pressure × 1.3` applied to successors

### Baseline Results

| Run | Victory | Collapsed Actors | Ticks | Notes |
|-----|---------|-----------------|-------|-------|
| rome batch | - | ostrogoths: 26, armenia: 1 | 50 | **Collapses reduced** from ~100 to 26 runs |
| rome balanced | ✓ Tick 16 | none | 50 | Victory tick unchanged |
| rome influence | ✓ Tick 16 | none | 50 | Victory tick unchanged |
| constantinople balanced | ✓ Tick 24 | none | 25 | Victory tick unchanged |

### Balance Review
- **Collapses significantly rarer** - ostrogoths now 26% vs previously ~100%
- **Victory timing stable** - no significant shift in victory tick (< 5 ticks)
- **No balance review required** - changes improve realism without breaking existing balance

---

## Rome 375 BC (Pre-Fix Reference)

### Strategy Roles

| Strategy  | Role                              | Victory Path |
|-----------|-----------------------------------|--------------|
| Balanced  | Moderate influence path           | ✓ Tick 16    |
| Influence | Aggressive influence path         | ✓ Tick 16    |
| Wealth    | Tempo-resource path, no victory   | ✗            |

### Scripted Strategy Summary Table

| Strategy  | Victory     | Influence Δ | Wealth Δ   | Connections Δ | Key Actions |
|-----------|-------------|-------------|------------|---------------|-------------|
| Balanced  | ✓ Tick 16   | +84.7       | -161.0     | +80.0         | build_reputation:16, back_admin:11, expand:5 |
| Influence | ✓ Tick 16   | +96.5       | -175.0     | +60.2         | build_reputation:16, back_admin:13, support:3 |
| Wealth    | ✗           | -92.7       | +327.6     | -142.8        | invest_wealth:50, lay_low:50 |

### Batch Report (100 runs, 50 ticks each, no-player)

```
=== BALANCE REPORT: ROME 375 (100 runs, 50 ticks each, no-player) ===
This report reflects autonomous world behavior without player actions.

Rome core metrics (final avg):
  military_size:   338.0
  cohesion:        87.1
  legitimacy:      47.8

Family metrics (final avg):
  family_influence: -28.8

Dynamics (avg per run):
  generation transitions: 0.0
  foreground shifts:      14.7

Most common collapsed actors:
  - ostrogoth_kingdom: 100 runs
  - ostrogoths: 100 runs
  - armenia: 72 runs
```

### Important: Rome Batch Interpretation

> **Rome batch = no-player run**
>
> Without player actions, family influence shows slow decline:
> - **~ -28.8 influence over 50 ticks** (from 60.0 starting)
> - This is the baseline "drift" without player intervention
> - Used to compare against scripted strategies

### Design Status

- **Primary win path:** `family:influence >= 90` (unchanged)
- **Wealth role:** tempo-resource (not victory path)
- **Knowledge role:** support-resource via legitimacy bridge (+0.1 legitimacy/tick at knowledge > 40)
- **Balanced:** no longer a trap path — now wins at tick 16
- **Rome batch:** interpreted as no-player baseline for comparison

---

## Constantinople 1430

### Strategy Roles

| Strategy  | Role                        | Victory Path |
|-----------|-----------------------------|--------------|
| Balanced  | Mixed diplomacy/military    | ✓ Tick 24    |
| Diplomacy | Diplomacy-first approach    | ✓ Tick 25    |
| Military  | Military-first approach     | ✓ Tick 23    |

### Scripted Strategy Summary Table

| Strategy  | Victory     | Federation Δ | Pressure Δ | Actions Applied | Key Actions |
|-----------|-------------|--------------|------------|-----------------|-------------|
| Balanced  | ✓ Tick 24   | 0 → 98.8     | 0 → 79.6   | 62              | venice_diplomacy:24, genoa_mercenaries:19 |
| Diplomacy | ✓ Tick 25   | 0 → 99.0     | 0 → 80.0   | 62              | venice_diplomacy:25, genoa_mercenaries:18 |
| Military  | ✓ Tick 23   | 0 → 99.1     | 0 → 74.9   | 60              | genoa_mercenaries:23, venice_diplomacy:17 |

### Batch Report (100 runs, 50 ticks each)

```
=== SIMULATION REPORT (100 runs, 50 ticks each) ===
Ticks completed: 50
Random events fired (avg): 41.1

Collapses: 73 runs (73%)
  median collapse tick: 35
  collapses before tick 10: 0
  collapses before tick 20: 3
```

### Design Status

- **Primary win path:** `federation_progress >= 100` sustained for 3 ticks
- **All three scripted strategies are valid** and achieve victory
- **Military strategy** achieves fastest victory (tick 23) but requires more rejections
- **Diplomacy strategy** is slightly slower (tick 25) but more efficient early
- **Balanced strategy** is middle ground (tick 24)

---

## Notes

### Rome Victory Condition (Unchanged)

```rust
victory_condition: VictoryCondition {
    metric: "family:influence".to_string(),
    threshold: 90.0,
    minimum_tick: 15,
    sustained_ticks_required: 1,
}
```

### Rome Knowledge → Legitimacy Bridge

Added soft support role for knowledge (not a second victory path):

```rust
AutoDelta {
    metric: "legitimacy".to_string(),
    base: -0.1,
    conditions: vec![
        // ... other conditions ...
        DeltaCondition {
            metric: "family:family_knowledge".to_string(),
            operator: ComparisonOperator::Greater,
            value: 40.0,
            delta: 0.1,  // +0.1 legitimacy/tick when knowledge > 40
        },
    ],
}
```

### Rome Balanced Policy Fix

Changed priority from resource-loop trap to outcome-focused:

```rust
// BEFORE (trap):
vec!["gather_information", "lay_low", "expand_network", ...]

// AFTER (wins):
vec!["expand_network", "build_reputation", "support_city", ...]
```

---

## Baseline Verification Commands

```bash
# Rome
cargo run --bin sim rome_375 50 batch 2>/dev/null
cargo run --bin sim rome_375 50 scripted balanced 2>/dev/null
cargo run --bin sim rome_375 50 scripted influence 2>/dev/null
cargo run --bin sim rome_375 50 scripted wealth 2>/dev/null

# Constantinople
cargo run --bin sim constantinople_1430 50 batch 2>/dev/null
cargo run --bin sim constantinople_1430 25 scripted balanced 2>/dev/null
cargo run --bin sim constantinople_1430 25 scripted diplomacy 2>/dev/null
cargo run --bin sim constantinople_1430 25 scripted military 2>/dev/null
```
