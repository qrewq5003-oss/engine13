# ENGINE13 Simulation Baseline

**Recorded:** After Rome balanced fix and knowledge → legitimacy bridge
**Date:** 2026-03-09
**Commit:** Post "Fix: improve Rome balanced path and give knowledge a safe support role via legitimacy"

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
