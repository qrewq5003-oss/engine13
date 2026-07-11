# Actor-scoped metric strings silently resolve to `global:`

**Status:** PR #1 (auto_deltas, sites 1–3) — this document.
PR #2 (common_events `self.*`, sites 4–5) — follows after this merges.

---

## The pattern

`MetricRef::parse` resolves an actor metric **only** from an explicit `actor:`
prefix:

```rust
"actor:rome.cohesion" => MetricRef::Actor  { actor_id: "rome", metric: "cohesion" }
"rome.cohesion"       => MetricRef::Global { key: "rome.cohesion" }   // <-- plain string
"cohesion"            => MetricRef::Global { key: "cohesion" }        // <-- plain string
```

Several subsystems keep the metric key as a **bare string** next to a *separate*
actor context (`auto_delta.actor_id`, an event's target actor). To combine the
two they concatenate by hand:

```rust
format!("{}.{}", actor_id, metric)          // engine/mod.rs, auto_deltas
metric.replace("self.", &format!("{}.", target_id))  // engine/mod.rs, events
```

Neither produces the `actor:` prefix. Both therefore resolve to `Global`, at a
key that **no other code reads or writes**. Reads return `0.0`; writes land in a
phantom global (clamped to 0..100) and are never observed. The failure is
completely silent: no panic, no validation error — `validate_scenario` only
checks actor ids *inside* refs that already parsed as `Actor`, so a ref that
degraded to `Global` is never examined.

## The five sites

| # | Location | Construction | Effect |
|---|----------|--------------|--------|
| 1 | `engine/mod.rs` auto_delta metric | `scope_metric_for_actor` | delta never reaches the actor |
| 2 | `engine/mod.rs` auto_delta conditions | `scope_metric_for_actor` | condition reads `0.0` |
| 3 | `engine/mod.rs` auto_delta ratio_conditions | `scope_metric_for_actor` | ratio reads `0.0 / 0.0` |
| 4 | `engine/mod.rs` event conditions | `replace("self.", …)` | condition reads `0.0` → event never fires |
| 5 | `engine/mod.rs` event effects | `replace("self.", …)` | effect never reaches the actor |

Sites 1–3 are fixed here; 4–5 in the follow-up PR.

### A sixth site — and the only reader of the phantom

`llm/mod.rs:250-274` **re-implements** `MetricRef::parse` by hand to resolve
`narrative_config.key_metrics`, including the same "no prefix → global" fallback.
Rome's config listed `"rome.legitimacy"` / `"rome.cohesion"`, so the narrative
layer was reading exactly the phantom globals that the broken auto_deltas were
writing. It is the one consumer of the phantom — which is why fixing sites 1–3
*alone* would have moved the narrative from "meaningless but non-zero" to a
constant `0.0`. The two strings are corrected to `actor:rome.*` in this PR.

## Which scenarios are affected (sites 1–3)

An audit over all three scenarios (`actor_id` set **and** the metric string bare,
i.e. rewritten by `scope_metric_for_actor` and still landing in `Global`):

| Scenario | Broken auto_delta blocks |
|---|---|
| `constantinople_1430` | **0** |
| `milan_1477` | **0** |
| `rome_375` | **9** |

Constantinople and Milan are unaffected because their authors wrote
fully-qualified `metric = "actor:ottomans.military_size"` alongside `actor_id`.
Rome wrote bare `metric = "population"` with `actor_id: Some("rome")` — so **all
of Rome's core actor dynamics were inert**: population, military_size,
military_quality, economic_output, cohesion, legitimacy (plus 3 conditional
blocks). An explicit `global:`/`family:` prefix on an actor-scoped block is
*intentional* (an actor's delta may gate on a global) and is left alone.

## The fix

One shared constructor — `MetricRef::parse_scoped(s, actor_id)` — replaces all
three hand-rolled string constructions. It returns a `MetricRef` directly, so no
call site concatenates a metric key any more:

- explicit `global:` / `family:` / `actor:` prefix → honoured as written
- bare `metric` → that metric on `actor_id`; global when `actor_id` is `None`

`scope_metric_for_actor` is deleted.

## Measured effect

Baselines: `sim <scenario> 50 batch` (100 seeds) and `sim <scenario> 200 <seed>`
for seeds 42/1/7.

**`constantinople_1430` and `milan_1477`: byte-identical, before and after**, on
every batch and single-seed run — exactly as the audit predicted, and the
strongest available check that the shared utility did not perturb the paths that
were already correct.

**`rome_375` — batch, 100 runs × 50 ticks, no-player:**

| Metric | Before | After | Δ |
|---|---|---|---|
| `military_size` (final avg) | 337.3 | **336.8** | −0.5 |
| `cohesion` (final avg) | 73.6 | **71.5** | −2.1 |
| `legitimacy` (final avg) | 45.6 | **41.0** | −4.6 |
| random events fired (avg) | 30.5 | **30.7** | +0.2 |

**`rome_375` — single seed, tick 199:**

| Seed | military (before → after) | cohesion (before → after) | legitimacy |
|---|---|---|---|
| 42 | 262.7 → **275.0** (+12.3) | 65.5 → **61.8** (−3.7) | 19.5 → 19.5 (rank floor) |
| 1 | 260.8 → **274.7** (+13.9) | 65.4 → **61.5** (−3.9) | 19.5 → 19.5 (rank floor) |
| 7 | 265.9 → **278.8** (+12.9) | 65.5 → **61.8** (−3.7) | 14.5 → 14.5 (rank floor) |

Reading the numbers:

- **Early ticks now move at all.** Rome's `military_size` sat at exactly `350.0`
  through tick 15 before (the `base: -0.2` drift never reached the actor); it now
  decays from tick 1 (`349.1` by tick 5).
- **Military ends *higher*, not lower.** The `external_pressure > 60 → +0.3`
  condition was reading `0.0` and never firing; it now reads Rome's real
  external_pressure, and its rearmament outweighs the `-0.2` base drift over 200
  ticks. This is the intended design finally executing.
- **Cohesion and legitimacy end lower**, since their negative base drifts
  (`-0.3`, `-0.1`) now actually apply. Legitimacy is unchanged at the final tick
  only because it is pinned at its region-rank floor in all three seeds.
- Military conflicts, foreground shifts and generation transitions are unchanged
  on seed 42; the small random-events delta follows from a changed foreground set.

Determinism is unchanged (this path is deterministic arithmetic): same seed →
byte-identical output across separate processes, all three scenarios, seeds
42/1/7.

## Open question (not addressed here)

Five sites of one pattern in a single codebase is not a coincidence — it is a
**structural weakness of the `MetricRef::parse` API**. The API demands an
explicit `actor:` prefix, while the string constructors that feed it live in
other subsystems that do not know that. Every one of these bugs was found
individually, after the fact, and each was silent in production for the entire
history of the project. `llm/mod.rs` even re-derives the parse rules by hand, so
the same class of bug can be reintroduced without touching `MetricRef` at all.

Worth deciding later, deliberately: **should metric keys be constructed through a
type-safe builder rather than by string concatenation**, so that "actor-relative
key without the `actor:` prefix" becomes unrepresentable instead of being caught
one instance per session? `parse_scoped` closes the five known sites but does not
make the class of bug impossible — a new subsystem can still `format!` its own key.
