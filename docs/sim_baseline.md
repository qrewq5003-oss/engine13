# ENGINE13 Simulation Baseline

**Recorded:** After Rome balanced fix and knowledge → legitimacy bridge
**Date:** 2026-03-09
**Commit:** Post "Fix: improve Rome balanced path and give knowledge a safe support role via legitimacy"

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
