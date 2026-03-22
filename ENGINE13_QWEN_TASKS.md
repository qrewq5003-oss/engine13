# ENGINE13 Qwen Tasks

## Time Contract (Phase 1 — Completed)

### Current Model: 2 Ticks = 1 Year

The project uses a unified time contract:

```rust
// Year calculation
year = start_year + (tick / 2)

// Half-year determination
half_year = if tick % 2 == 0 { FirstHalf } else { SecondHalf }
```

### Tick/Half-Year Mapping

| Tick | Year | Half-Year |
|------|------|-----------|
| 0 | start_year | FirstHalf |
| 1 | start_year | SecondHalf |
| 2 | start_year + 1 | FirstHalf |
| 3 | start_year + 1 | SecondHalf |
| 4 | start_year + 2 | FirstHalf |
| ... | ... | ... |

### Implementation Details

#### WorldState

- `tick: u32` — current simulation tick
- `year: i32` — derived from tick (stored for convenience, recalculated on load)
- `scenario_start_year: Option<i32>` — original scenario start year

#### HalfYear Enum

```rust
pub enum HalfYear {
    FirstHalf,   // tick % 2 == 0 (even ticks)
    SecondHalf,  // tick % 2 == 1 (odd ticks)
}
```

#### Save/Load Compatibility

- `year` is serialized in saves for backward compatibility
- On load, `year` is **recalculated** from `tick` and `scenario.start_year`
- This ensures no drift even if old saves have incorrect year values

#### Generation Timing (Family Scenarios)

- Patriarch ages **only on even ticks** (FirstHalf)
- This ensures 1 year of aging per 2 ticks
- Generation transfer triggers at correct age thresholds

### Files Involved

| File | Purpose |
|------|---------|
| `src/core/world.rs` | WorldState struct with tick/year fields |
| `src/llm/mod.rs` | HalfYear enum and from_tick() |
| `src/engine/mod.rs` | Tick advancement, year calculation, patriarch aging |
| `src/application/save_load.rs` | Save/load with year recalculation |

### Validation

```rust
// tick 0 => start_year, FirstHalf
// tick 1 => start_year, SecondHalf  
// tick 2 => start_year + 1, FirstHalf
// tick 3 => start_year + 1, SecondHalf
```

---

## Pending Tasks

### Phase 2 — Narrative Pipeline (Optional)

- [ ] Review relevance scoring pipeline
- [ ] Verify snapshot builder consistency
- [ ] Check LLM provider integration

### Phase 3 — UI Polish (Optional)

- [ ] Review ControlPanel half-year display
- [ ] Verify narrative panel integration

---

## Notes

- Do not modify time contract without updating this document
- Any changes to tick/year logic must preserve save/load compatibility
- Generation timing must remain synchronized with half-year model
