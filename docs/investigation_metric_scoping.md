# Actor-scoped metric strings silently resolve to `global:`

**Status:** PR #1 (auto_deltas, sites 1–3) — merged (`09337fc`).
PR #2 (common_events `self.*`, sites 4–5) — see "Sites 4–5" below.

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

---

# Sites 4–5 — `self.*` in random events (PR #2)

Random events name their target actor as `self.`, resolved by
`metric.replace("self.", &format!("{}.", target_id))` — which yields
`"venice.population"`, i.e. the same phantom `Global`. Conditions read `0.0`;
effects are swallowed. Both sites now call `MetricRef::parse_scoped`, which
grows a `self.` arm; the string `replace` is gone.

An audit of every metric string in all three scenarios' *own* `random_events`
found **zero** bare keys (all carry an explicit `actor:`/`global:`/`family:`
prefix), so the bare→actor rule cannot capture a string that was meant to be
global. Only `common_events` uses `self.`.

## What was actually broken — *not* "11 dormant events"

The natural assumption is that the 11 common events never fired. Measured, the
truth splits in two, because a condition reading `0.0` is not simply false:

- **A `>` gate is never satisfied** (`0.0 > 50` is false) → the event **never fires**:
  `plague`, `desertion`, `mercenary_influx`, `trade_boom`.
- **A `<` gate is *always* satisfied** (`0.0 < 30` is true, always) → the event is
  **degenerate**: it fires at full probability against *any* target, regardless of
  that actor's real state, and then its effects vanish into the phantom global:
  `famine`, `court_conspiracy`, `popular_uprising`, `charismatic_preacher`.
- Three events are unconditional anyway (`earthquake`, `piracy`, `flood`): they
  fired, but their effects were swallowed too.

So the chronicle has been narrating *"a popular uprising shook the capital"* for
an actor at cohesion 80, while nothing whatsoever happened mechanically. Fires
per event, summed over seeds 42/1/7 × 200 ticks:

| Event | Gate | const. before → after | rome before → after | milan before → after |
|---|---|---|---|---|
| `plague` | `pop > 500`, `coh < 60` | 0 → **0** | 0 → **8** | 0 → **1** |
| `desertion` | `mil > 50`, `treas < 200` | 0 → **3** | 0 → 0 | 0 → 0 |
| `mercenary_influx` | `treas > 300` | 0 → **20** | 0 → **8** | 0 → **23** |
| `trade_boom` | `eco > 40` | 0 → **54** | 0 → **40** | 0 → **70** |
| `famine` | `eco < 30` | 74 → **0** | 57 → **0** | 75 → **0** |
| `court_conspiracy` | `leg < 60` | 61 → 38 | 60 → 68 | 73 → 21 |
| `popular_uprising` | `coh < 30`, `leg < 40` | 49 → 3 | 46 → 1 | 57 → **0** |
| `charismatic_preacher` | `coh < 40` | 28 → **0** | 27 → 1 | 21 → 10 |
| `earthquake` / `flood` / `piracy` | (none) | fired; effects now land | | |

The four `> `-gated events wake up. The `<`-gated ones stop firing indiscriminately
and start tracking real actor state — `famine` (`economic_output < 30`) drops to
zero on all three scenarios because no actor's economy actually falls that far;
it had been firing ~70×/run purely on the `0.0` read. `desertion` stays at 0 in
rome/milan and `plague` at 0 in constantinople for genuine reasons (treasury never
dips below 200; population never exceeds 500 while cohesion < 60), not broken ones.

## Measured effect (PR #2)

Baseline "before" = `main` **after PR #1**. All three scenarios change, as expected
— common events are shared by all of them.

**Batch, 100 runs × 50 ticks, no-player:**

| Scenario | random events (avg) | other |
|---|---|---|
| `constantinople_1430` | 73.4 → **67.3** | — |
| `milan_1477` | 67.8 → **59.8** | — |
| `rome_375` | 30.7 → **25.5** | military_size 336.8 → **343.6**, cohesion 71.5 → **72.5**, legitimacy 41.0 → **39.2**, family_influence 0.7 → **0.4** |

Total event volume drops (the degenerate always-on `famine`/`preacher` fires
outweigh the four newly-woken events), while Rome's military ends *higher* —
`mercenary_influx` (+30 military) and `trade_boom` (+80 treasury) now actually pay out.

**Single seed, 200 ticks:** byzantium survival, generation transitions and
foreground shifts are unchanged; military conflicts move slightly (milan seed 42:
449 → 439). Constantinople's `federation_progress` shifts within its existing
range (seed 42 max 15.2 → 10.0; seed 7 max 7.8 → 12.2).

**Victory regression check.** The constantinople victory gate
(`ottomans.military_size < 40`, PR #14) was calibrated in a world where these
events were inert, so it is the obvious thing to break. Scripted `balanced`,
300 ticks — **still wins on all five seeds**, at ticks 65 / 55 / 54 / 59 / 48
(was 53 / 54 / 51 / 49 / 59). Within the existing seed-to-seed spread.

## Determinism (D3 re-check)

Unlike the auto_delta path (deterministic arithmetic), this is RNG-consuming code,
and these 11 events had **never** competed for RNG draws before — so every
"byte-identical" result in the project's history (C1–D5) was obtained in a world
where they structurally could not fire.

Re-audited for new nondeterminism of the D3 class (unsorted `HashMap` iteration
feeding an RNG draw):

- `foreground_ids` (event target selection, `choose(rng)`) is **already sorted** —
  the D3 fix is present and is what makes target choice reproducible.
- `event.effects` is a `HashMap`, and its iteration order *does* vary per process
  (`RandomState` is seeded per process). It is **safe**: the loop consumes no RNG,
  and a `HashMap`'s keys are unique, so each effect writes a distinct metric —
  application is commutative. No RNG draw depends on the order.
- `apply_treasury` / `apply_actor_tags` iterate `world.actors` unsorted, but mutate
  each actor independently and draw no RNG.

Verified empirically rather than by argument: **5 separate processes × 3 scenarios
× seeds 42/1/7 = 45 runs, byte-identical output hashes per (scenario, seed)**.
Separate processes matter — `RandomState` is per-process, so an in-process repeat
cannot detect this class of bug. Determinism holds.

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
one instance per session? `parse_scoped` closes the six known sites but does not
make the class of bug impossible — a new subsystem can still `format!` its own key,
and `llm/mod.rs` still re-derives the parse rules by hand rather than calling
`MetricRef`.

Two cheap, non-structural guards worth considering in the meantime (neither is in
these PRs): have `validate_scenario` **reject** a `Global` ref whose key contains a
`.` or matches a known actor id (every one of these bugs produced exactly such a
key), and make `llm/mod.rs` call `MetricRef::parse` instead of its hand-rolled copy.
