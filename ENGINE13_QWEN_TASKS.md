# ENGINE13 Qwen Tasks

## Completed Phases (PR 1-7)

### Phase 1 — Time Contract ✅
- **2 ticks = 1 year** confirmed and documented
- `year = start_year + (tick / 2)`
- `half_year` from `tick % 2` (even = FirstHalf, odd = SecondHalf)
- Save/load compatibility preserved (year recalculated on load)
- Patriarch aging only on even ticks (1 year per 2 ticks)

### Phase 2 — Relevance Pipeline Unification ✅
- `db.rs::get_relevant_events_scored()` is canonical scorer
- `commands.rs::get_relevant_events()` is thin wrapper
- Single scoring formula: `thematic_similarity × temporal_coefficient`
- Deterministic selection: top 15 + last 5 + is_key events

### Phase 3 — Snapshot Contract ✅
- `build_snapshot()` accepts pre-selected events (no `Option<Db>`)
- Event selection happens BEFORE snapshot assembly
- Query tags built from foreground actors before snapshot
- Prompt builder reads only `NarrativeWorldSnapshot`

### Phase 4 — Family Continuity ✅
- `generation_mechanics` and `generation_length` restored on load
- `FamilyInfo` added to `NarrativeWorldSnapshot`
- Family state survives tick advance, generation transfer, save/load
- Narrative layer can see family continuity via `family_info`

### Phase 5 — Actor Collapse / Freeze Fix ✅
- Collapse check moved to Phase 0 (before metric modifications)
- Terminal decline is inevitable after 3 cumulative dangerous ticks
- Counter NOT reset on temporary recovery (prevents oscillation freeze)
- Fallback successor creation when template missing
- Collapse warning counters cleaned up after actor removal

### Phase 6 — Patron Action Pipeline ✅
- Full execution order verified
- Costs and effects applied correctly via `MetricRef::apply()`
- Effects persist through tick processing
- 4 regression tests added

### Phase 7 — Query Tag Enrichment ✅
- Enriched with: semantic tags, religion, culture, region rank
- Scenario context: `tone_tags`, `narrative_axes`
- All tags lowercased, deduplicated (HashSet), sorted
- Deterministic output for consistent retrieval

---

## Current Baseline

### Time Contract
```
year = start_year + (tick / 2)
tick % 2 == 0 → FirstHalf (January-June)
tick % 2 == 1 → SecondHalf (July-December)
```

### Relevance Pipeline
```
WorldState → narrative_actor_ids → query_tags → relevant_events → NarrativeWorldSnapshot
```

### Query Tags (enriched)
- Core: actor id, name, region
- Semantic: actor.tags
- Religion: "religion_orthodox", etc.
- Culture: "culture_greek", etc.
- Rank: "rank_s", "rank_a", etc.
- Scenario: tone_tags, narrative_axes

### Collapse Behavior
- Phase 0 check (before modifications)
- 3 cumulative dangerous ticks → inevitable collapse
- No reset on temporary recovery

### Family Continuity
- Persistent across simulation
- Survives save/load
- Visible in snapshot via `family_info`

### Patron Actions
- Cost and effects applied deterministically
- Persist through tick processing

---

## Remaining Priorities

None currently — all phases 1-7 complete.

---

## Notes

- Do not modify time contract without updating this document
- Any changes to tick/year logic must preserve save/load compatibility
- Generation timing must remain synchronized with half-year model
- Documentation should reflect implemented truth, not aspirational design
