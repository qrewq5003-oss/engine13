# Narrative Regression Baseline

**Purpose:** This document establishes baseline expectations for narrative quality and defines what counts as regression.

**Note:** The example narratives here are reference examples, not rigid golden texts. The goal is to capture quality characteristics, not to demand identical wording forever.

---

## What Counts as Narrative Regression

Regression is indicated by ANY of the following:

### Critical Regressions (must fix)
1. **Factuality breach**: Narrative claims collapse/death/victory that isn't in facts
2. **Actor-by-actor checklist**: Returns to listing actors one by one instead of world-first narrative
3. **Single paragraph collapse**: Output shrinks to one short block instead of 2-4 paragraphs
4. **Strategy reflection lost**: Different scripted strategies produce indistinguishable narratives

### Quality Regressions (should fix)
5. **Repetition across turns**: Adjacent half-year outputs use nearly identical framing/focus without state change justification
6. **Scenario voice lost**: tone_tags and narrative_axes stop being felt in the output
7. **Filler inflation**: Abstract "historical" padding replaces concrete meaning

---

## Automated Safety Rails

The following minimal checks are automated:

1. **Output not empty**
2. **Output has ≥2 paragraphs** (at least one `\n\n`)

These are necessary but not sufficient conditions.

---

## Evaluation Checklist

For each narrative output, evaluate:

| Criterion | Pass (1) | Fail (0) |
|-----------|----------|----------|
| Factual accuracy: no hallucinated events | | |
| No hallucination: no invented collapse/death/victory | | |
| World-first focus: describes world state before actors | | |
| Scenario voice: tone_tags/narrative_axes felt in text | | |
| Strategy reflection: different strategies feel different | | |
| Repetition low vs previous output | | |
| Paragraph quality: 2-4 substantial paragraphs | | |

**Scoring:**
- 7/7 — excellent
- 6/7 — good
- 5/7 — acceptable
- <5/7 — needs fixing

---

## Reference Examples by Scenario

### Rome 375 BC

**Snapshot context:**
- Year: 375-380
- Half-year: First/Second
- Key metrics: family_influence ~60-90, rome.legitimacy ~60-70
- Foreground actors: rome, visigoths, huns, ostrogoths

**Strategy: Balanced**
- Expected: Moderate influence growth, balanced resource use
- Narrative should feel: pragmatic, family-centered, navigating imperial decay

**Strategy: Influence**
- Expected: Aggressive influence spending, faster victory approach
- Narrative should feel: ambitious, politically active, risk-taking

**Strategy: Wealth**
- Expected: Wealth accumulation, no direct victory path
- Narrative should feel: economically focused, politically cautious, long-term positioning

---

### Constantinople 1430

**Snapshot context:**
- Year: 1430-1453
- Half-year: First/Second
- Key metrics: federation_progress ~0-100, byzantium.external_pressure ~60-100
- Foreground actors: byzantium, ottomans, venice, genoa, milan

**Strategy: Balanced**
- Expected: Mixed diplomacy/military support
- Narrative should feel: coalition-building, pragmatic alliance management

**Strategy: Diplomacy**
- Expected: Diplomacy-first approach, slower but steadier
- Narrative should feel: diplomatic maneuvering, economic leverage, maritime focus

**Strategy: Military**
- Expected: Military-first approach, faster pressure reduction
- Narrative should feel: martial urgency, direct confrontation, defensive consolidation

---

## Manual Evaluation Procedure

1. **Run 6 scripted scenarios** (Rome ×3, Constantinople ×3)
2. **Capture 2 adjacent half-year outputs** per scenario
3. **Evaluate each output** using the checklist above
4. **Compare adjacent outputs** for repetition
5. **Flag any regression** per the criteria above

---

## Notes for Future Improvement

This baseline is intentionally minimal. Future improvements may include:

- LLM-assisted repetition detection
- Semantic similarity scoring across turns
- More sophisticated actor-focus tracking
- Automated strategy differentiation scoring

For now, manual review with this checklist is the primary quality gate.
