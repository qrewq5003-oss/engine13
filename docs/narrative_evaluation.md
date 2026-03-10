# Narrative Manual Evaluation Procedure

**Purpose:** This document defines the manual evaluation process for narrative quality assessment.

**Note:** This is a procedure document only. The actual review is conducted separately.

---

## Evaluation Checklist

For each narrative output, evaluate the following criteria:

| Criterion | Pass (1) | Fail (0) |
|-----------|----------|----------|
| **Factual accuracy**: no hallucinated events | | |
| **No hallucination**: no invented collapse/death/victory | | |
| **World-first focus**: describes world state before actors | | |
| **Scenario voice**: tone feels appropriate to scenario | | |
| **Strategy reflection**: player strategy is felt in text | | |
| **Repetition low vs previous output**: fresh framing | | |
| **Paragraph quality**: 2-4 substantial paragraphs | | |

### Scoring Scale

- **7/7** — excellent
- **6/7** — good
- **5/7** — acceptable
- **<5/7** — needs fixing

---

## Two Levels of Evaluation

### A. Single-Output Quality

Evaluate each narrative individually:

1. **Factual accuracy** — no invented events
2. **No hallucination** — no false collapse/victory/death
3. **World-first focus** — world state described before actors
4. **Scenario voice** — tone matches scenario context
5. **Paragraph quality** — 2-4 substantial paragraphs

### B. Across-Turn Quality

Evaluate pairs of adjacent half-year outputs:

1. **Repetition low** — not repeating same rhetorical frame
2. **No same actor focus without reason** — actor centering justified by state change
3. **Same rhetoric not reused** — fresh dramatic framing
4. **Half-year change visible** — narrative reflects state evolution

---

## Review Procedure

### Six Cases to Evaluate

#### Rome 375 BC
1. Scripted balanced
2. Scripted influence
3. Scripted wealth

#### Constantinople 1430
4. Scripted balanced
5. Scripted diplomacy
6. Scripted military

### For Each Case

1. Capture **2 adjacent half-year outputs** (e.g., First Half 375 + Second Half 375)
2. Evaluate each output using the checklist above
3. Evaluate across-turn quality between the two outputs
4. Record context snapshot for future reference
5. Note any regressions or quality issues

---

## Signs of Good Output

A good narrative output:

- ✓ Describes world change first, then connects to key actors
- ✓ Does not invent events (collapse, victory, death)
- ✓ Reflects actual player strategy
- ✓ Does not devolve into journal replay or UI-log
- ✓ Delivers 2-4 substantial paragraphs
- ✓ Shifts emphasis between adjacent half-years when state has genuinely changed
- ✓ Uses tone_tags and narrative_axes as felt framing, not just labels

---

## Signs of Bad Output

A bad narrative output:

- ✗ Actor-by-actor checklist ("Venice did X. Genoa did Y. Milan did Z.")
- ✗ Invented collapse / victory / death not in facts
- ✗ Same text on adjacent half-years without justification
- ✗ Same dramatization reused without new events
- ✗ Player strategy not felt in text
- ✗ Empty or repetitive paragraphs
- ✗ Long text without new meaning (filler inflation)

---

## Scenario Voice: What It Means

### Rome 375 BC

**Good Rome voice:**

> The narrative should feel like a chronicle of a world where family advancement occurs within broader political and institutional instability. The world is aging, institutions are weakening, and the family seeks windows of influence within this cracking structure.

**Example direction:**
> "Мир дряхлеет, институты слабеют, а семья ищет окно влияния внутри этой трещащей конструкции."

**Key markers:**
- Political erosion
- Institutional decay
- Family strategy within crumbling order
- Barbarian pressure as background force
- The sense of an era ending

---

### Constantinople 1430

**Good Constantinople voice:**

> The narrative should feel like a chronicle of a world under external pressure, where alliances, hesitations, and political-military coordination determine the city's fate. The world is shrinking under pressure, and the city's survival depends on fragile alliance coordination and the cost of delay.

**Example direction:**
> "Мир сужается под давлением, а судьба города зависит от хрупкой координации союзников и цены промедления."

**Key markers:**
- Fragile coalition
- External pressure mounting
- Diplomacy under threat
- Balance between salvation and collapse
- The price of hesitation

---

## Evaluation Template

Use this template for each case:

```markdown
### Case: [scenario] — [strategy]

#### Context Snapshot
- **Year:**
- **Half-year:**
- **Victory status:**
- **Foreground actors:**
- **Key world condition:**
- **Key player actions:**

#### Output A (First Half)
[Paste narrative text]

#### Output B (Second Half)
[Paste narrative text]

#### Checklist A
- Factual accuracy: [0/1]
- No hallucination: [0/1]
- World-first focus: [0/1]
- Scenario voice: [0/1]
- Strategy reflection: [0/1]
- Paragraph quality: [0/1]

**Subtotal A: _/6**

#### Checklist B
- Factual accuracy: [0/1]
- No hallucination: [0/1]
- World-first focus: [0/1]
- Scenario voice: [0/1]
- Strategy reflection: [0/1]
- Paragraph quality: [0/1]

**Subtotal B: _/6**

#### Across-Turn Check
- Repetition low: [0/1]
- Same actor focus without reason: [0/1] (0 = bad, 1 = good)
- Same rhetoric reused: [0/1] (0 = bad, 1 = good)
- Half-year change visible: [0/1]

**Across-turn subtotal: _/4**

#### Final Score
**Total: _/16**

Rating:
- 15-16/16 — excellent
- 13-14/16 — good
- 11-12/16 — acceptable
- <11/16 — needs fixing

#### Notes
[Any additional observations, regression flags, or quality comments]
```

---

## What This Review Does NOT Do

- Does NOT use LLM-as-judge
- Does NOT automate scoring
- Does NOT change prompt architecture
- Does NOT modify simulation logic
- Does NOT create new scenarios or UI

This is a **manual human review** to establish quality baseline and catch regressions.

---

## Next Steps After Review

1. Compile scores across all 6 cases
2. Identify patterns of regression
3. Flag any critical factuality breaches
4. Note which scenarios/strategies have weakest voice
5. Use findings to guide future prompt tuning
