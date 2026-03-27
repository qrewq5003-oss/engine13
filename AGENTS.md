# AGENTS.md

## ENGINE13 working rules

This repository contains a simulation-first historical strategy project.
Preserve canonical contracts. Do not "improve" adjacent systems while fixing a local bug.

---

## Core invariants

### 1. Time contract is fixed
- 2 ticks = 1 year
- `year = start_year + (tick / 2)`
- tick parity determines half-year
- do not change time progression, tick/year mapping, or related save/load assumptions unless the task is explicitly about time contract

### 2. Relevance pipeline is canonical
- Canonical flow:
  `WorldState -> narrative_actor_ids -> query_tags -> relevant_events -> NarrativeWorldSnapshot`
- `db.rs::get_relevant_events_scored()` is the canonical scoring path
- command-layer wrappers should stay thin
- do not invent alternate relevance selection paths unless explicitly requested

### 3. Snapshot contract is fixed
- events are selected before snapshot assembly
- prompt/narrative builders should consume `NarrativeWorldSnapshot`, not rebuild event selection ad hoc
- do not mix raw event stream with curated relevant events in the same contract

### 4. Family metrics must use one canonical runtime mutation path
- family metrics must mutate through the canonical runtime path
- UI must not be treated as a source of truth
- do not fix family metric bugs in display-only code if the real bug is in runtime state mutation
- family metrics are currently clamped to `0..100` in the canonical mutation path unless a task explicitly introduces a new mechanic

### 5. Family continuity must survive core transitions
- family state must survive ticks, generation transfer, save/load, and narrative snapshot use
- do not break family continuity while fixing unrelated bugs

### 6. Collapse check placement is fixed
- collapse checks belong to the early phase before later metric modifications
- do not move collapse logic casually
- do not reset dangerous-state accumulation casually

### 7. Tick order is not for opportunistic refactoring
- simulation phase order matters
- do not reorder engine phases unless the task explicitly requires it and acceptance criteria are provided

### 8. Fresh run and explicit load are different
- a fresh scenario start must create a fresh run
- an explicit load must restore the correct persisted run/history
- events must not leak across runs or scenarios
- do not "fix" mixed history by hiding it in the UI

### 9. Player-facing narrative visibility matters
- the player actor must not silently disappear from the visible narrative layer due to aggressive relevance demotion
- do not break player-facing narrative continuity while tweaking relevance logic

### 10. Rome 375 starts with unified Rome
- Rome 375 starts with a single `rome` actor
- `rome_west` / `rome_east` are successor actors only
- do not assume East/West split exists at scenario start
- pre-collapse logic must work through `rome`

### 11. Map config is frozen unless task is map-only
- do not change map config in ordinary fixes
- do not change coordinates, polygons, markers, viewport, map response shape, or rendering contract unless the task is explicitly map-only
- do not add fallback map generation or silent map replacement

### 12. UI must not mask model/data bugs
- if the bug is in runtime state, query scoping, scenario init, or persistence, fix it there
- do not claim a UI workaround is a real fix for a data/model bug

---

## Change scope rules

### Smallest patch first
- prefer the smallest correct patch
- do not refactor adjacent systems "while here"
- do not clean up unrelated code unless explicitly requested

### Backend vs frontend boundaries
- for backend bugfixes, do not touch frontend unless explicitly required
- for frontend bugfixes, do not touch backend unless explicitly required

### Save/load boundaries
- do not modify save/load behavior unless the task is specifically about save/load, fresh-run scoping, or persistence correctness

### Scenario boundaries
- do not modify scenario balance, thresholds, or initial values unless the task explicitly requires it

### Map boundaries
- map code is high-risk
- do not touch map-related code during unrelated fixes

---

## Runtime verification rules

When asked to verify a bug in the running app:
- run the app locally if the environment allows it
- prefer runtime verification over reasoning-only claims
- do not claim success from build success alone if the task explicitly requires live verification

When asked to inspect a UI issue:
- identify the actual scroll owner / state owner / render owner
- do not stop at a plausible CSS diff if the actual behavior is still wrong

When asked to inspect a data bug:
- verify source of truth, not only visible output
- distinguish fresh start vs explicit load
- distinguish raw events vs curated/relevant events

---

## Required output after every task

Provide:
1. exact files changed
2. exact root cause
3. exact functions/components changed
4. build/test result
5. runtime verification result if requested
6. what was intentionally not touched

---

## Forbidden habits

Do not:
- make broad refactors for a narrow bug
- change more files than necessary without a clear reason
- hide data bugs in the UI
- conflate fresh run with explicit load
- conflate raw event history with relevant/narrative event selection
- alter map behavior during unrelated work
- assume Rome 375 starts with East/West split
- claim success without verifying the requested acceptance criteria

---

## Preferred workflow

1. identify the exact source-of-truth layer for the bug
2. patch the smallest correct place
3. run build/tests
4. run the app if runtime verification is required
5. report exact scope and what was not touched
