//! The chronicler's «летопись мира» — six layers of memory, every one built from the engine's
//! facts (owner's decision of 2026-10-11, docs/investigation_chronicle_annals.md):
//!
//! 1. relations now — alliances, vassalages, declared conquests: a state, held while it lasts;
//! 2. open stories — wars of a pair, crises, a power on the brink — each with the year it began;
//! 3. the game's chronology — the last ten years by half-year, older ones a line per decade;
//! 4. the leading powers' trajectories, in words, from the world's annals (`core::Annals`);
//! 5. the player's policy — the last two years dated, older decisions summed up by kind;
//! 6. a stop-list of turns the game's own chronicles have already repeated.
//!
//! The past chronicles' **texts** never become facts: the only thing the book gives the prompt is
//! the stop-list, phrases of at most `STOP_WORDS_MAX` words, as a prohibition.
//! No numbers and no engine keys reach these lines: names, years and words only.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::core::{EventType, Scenario, WorldState, ANNALS_METRICS};
use crate::engine::EventLog;

/// The chronology's detailed window: the last ten years.
pub const CHRONOLOGY_RECENT_TICKS: u32 = 20;
/// The player's recent decisions: the last two years.
pub const PLAYER_RECENT_TICKS: u32 = 4;
/// A war: battles of one pair with gaps of at most this many ticks; open while the last one is
/// at most this many ticks old.
pub const WAR_GAP_TICKS: u32 = 6;
/// A war needs at least this many battles; a single clash is not a story.
pub const WAR_MIN_BATTLES: usize = 2;
/// At most this many wars are shown.
pub const WARS_MAX: usize = 8;
/// A turn of phrase repeated in this many chronicles of the book goes on the stop-list.
pub const STOP_MIN_CHRONICLES: usize = 2;
/// Stop-list phrases are 4 to 6 words long.
pub const STOP_WORDS_MIN: usize = 4;
pub const STOP_WORDS_MAX: usize = 6;
/// At most this many phrases on the stop-list.
pub const STOP_MAX: usize = 20;
/// The dossier: at most this many trajectories per power.
pub const DOSSIER_PER_ACTOR: usize = 3;

/// The six layers, already in words.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorldAnnals {
    pub relations: Vec<String>,
    pub open_stories: Vec<String>,
    /// One line per decade older than the detailed window, oldest first.
    pub chronology_decades: Vec<String>,
    /// One line per half-year of the last ten years that changed something, oldest first.
    pub chronology_recent: Vec<String>,
    pub dossier: Vec<String>,
    /// The player's decisions of the last two years, a line per half-year.
    pub player_recent: Vec<String>,
    /// Older decisions, one line per kind, the most recently used first.
    pub player_older: Vec<String>,
    pub stop_phrases: Vec<String>,
}

struct Ctx<'a> {
    world: &'a WorldState,
    scenario: &'a Scenario,
}

impl Ctx<'_> {
    fn name(&self, id: &str) -> String {
        if let Some(a) = self.world.actors.get(id) {
            return a.name.clone();
        }
        if let Some(d) = self.world.dead_actors.iter().find(|d| d.id == id).filter(|d| !d.name.is_empty()) {
            return d.name.clone();
        }
        self.scenario.actors.iter().find(|a| a.id == id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string())
    }
    fn alive(&self, id: &str) -> bool {
        self.world.actors.contains_key(id) && !self.world.dead_actor_ids.contains(id)
    }
    fn year(&self, tick: u32) -> i32 {
        self.scenario.start_year + (tick / 2) as i32
    }
    fn date(&self, tick: u32) -> String {
        format!("{} г., {}", self.year(tick), super::HalfYear::from_tick(tick).display_name())
    }
    fn names(&self, ids: &[String]) -> String {
        let mut v: Vec<String> = ids.iter().map(|i| self.name(i)).collect();
        v.sort();
        v.join(", ")
    }
}

/// The other actor of an engine event whose id is `<prefix><a>_<b>`, given the event's own actor:
/// `actor_first` — the actor is `<a>`, else `<b>`. Event ids join actor ids that may themselves
/// hold `_`; the event's `actor_id` makes the split unambiguous.
fn other_of(id: &str, prefix: &str, actor: &str, actor_first: bool) -> Option<String> {
    let rest = id.strip_prefix(prefix)?;
    let other = if actor_first {
        rest.strip_prefix(actor)?.strip_prefix('_')?
    } else {
        rest.strip_suffix(actor)?.strip_suffix('_')?
    };
    (!other.is_empty()).then(|| other.to_string())
}

/// A vassalage formed (`Some((vassal, overlord, true))`) or ended (`false`) by this event.
fn vassalage_change(e: &crate::core::Event) -> Option<(String, String, bool)> {
    let v = e.actor_id.as_str();
    if e.id.starts_with("vassalage_tribute_") {
        return None;
    }
    if let Some(o) = other_of(&e.id, "vassalage_end_", v, false) {
        return Some((v.to_string(), o, false));
    }
    if let Some(o) = other_of(&e.id, "vassalage_form_", v, false) {
        return Some((v.to_string(), o, true));
    }
    if e.id.starts_with("vassalage_end_") || e.id.starts_with("vassalage_form_") {
        return None;
    }
    other_of(&e.id, "vassalage_", v, false).map(|o| (v.to_string(), o, true))
}

fn player_action_name(e: &crate::core::Event) -> String {
    e.description.strip_prefix("Действие игрока: ").unwrap_or(&e.description).to_string()
}

fn first_sentence(s: &str) -> String {
    let s = s.trim();
    match s.find(". ") {
        Some(i) => s[..=i].to_string(),
        None => s.to_string(),
    }
}

/// Build the six layers for the half-year `period_tick` (the one the chronicle narrates).
pub fn world_annals(world: &WorldState, scenario: &Scenario, event_log: &EventLog, period_tick: u32) -> WorldAnnals {
    let c = Ctx { world, scenario };
    let mut out = WorldAnnals {
        relations: relations(&c, event_log),
        open_stories: open_stories(&c, event_log, period_tick),
        dossier: dossier(&c, period_tick),
        stop_phrases: stop_phrases(&world.chronicle_book.iter().map(|e| e.text.as_str()).collect::<Vec<_>>()),
        ..Default::default()
    };
    chronology(&c, event_log, period_tick, &mut out);
    player_policy(&c, event_log, period_tick, &mut out);
    out
}

// ---------------------------------------------------------------------------
// 1. Relations now
// ---------------------------------------------------------------------------

fn relations(c: &Ctx, log: &EventLog) -> Vec<String> {
    let mut out = Vec::new();
    for a in &c.world.alliances {
        let mut line = format!("Союз: {} (с {} года)", c.names(&a.actor_ids), c.year(a.formed_tick));
        if let Some(e) = &a.common_enemy {
            line.push_str(&format!("; общий враг — {}", c.name(e)));
            if let Some(t) = a.enemy_since {
                line.push_str(&format!(" (с {} года)", c.year(t)));
            }
        }
        out.push(line);
    }
    let mut vass: Vec<&crate::core::Vassalage> = c.world.vassalages.iter().collect();
    vass.sort_by(|a, b| a.formed_tick.cmp(&b.formed_tick).then(a.vassal_id.cmp(&b.vassal_id)));
    for v in vass {
        out.push(format!("{} — вассал; сюзерен — {} (с {} года)", c.name(&v.vassal_id), c.name(&v.overlord_id), c.year(v.formed_tick)));
    }
    for (att, tgt) in &c.world.conquests {
        if !c.alive(att) || !c.alive(tgt) {
            continue;
        }
        let mut line = format!("Война на завоевание: {} против {}", c.name(att), c.name(tgt));
        if let Some(t) = conquest_tick(c, log, att, tgt) {
            line.push_str(&format!(" (объявлена в {} году)", c.year(t)));
        }
        out.push(line);
    }
    out
}

/// The tick the milestone that declared this war of conquest (`begins_conquest`) first fired.
fn conquest_tick(c: &Ctx, log: &EventLog, att: &str, tgt: &str) -> Option<u32> {
    c.scenario.milestone_events.iter()
        .filter(|m| m.begins_conquest.as_ref().is_some_and(|b| b.attacker == att && b.target == tgt))
        .filter_map(|m| log.events.iter().find(|e| e.id == m.id).map(|e| e.tick))
        .min()
}

// ---------------------------------------------------------------------------
// 2. Open stories
// ---------------------------------------------------------------------------

fn open_stories(c: &Ctx, log: &EventLog, period_tick: u32) -> Vec<String> {
    let mut out = Vec::new();
    // Wars of a pair: the engine's battles in the log.
    let mut battles: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
    for e in log.events.iter().filter(|e| e.tick <= period_tick) {
        if let Some(def) = other_of(&e.id, "military_conflict_", &e.actor_id, true) {
            let key = if e.actor_id < def { (e.actor_id.clone(), def) } else { (def, e.actor_id.clone()) };
            battles.entry(key).or_default().push(e.tick);
        }
    }
    let player = c.scenario.player_actor_id.as_deref();
    let mut wars: Vec<(bool, u32, String, (String, String))> = Vec::new();
    let mut in_war: BTreeSet<(String, String)> = BTreeSet::new();
    for ((a, b), mut ticks) in battles {
        if !c.alive(&a) || !c.alive(&b) {
            continue;
        }
        ticks.sort();
        ticks.dedup();
        let last = *ticks.last().unwrap();
        if period_tick.saturating_sub(last) > WAR_GAP_TICKS {
            continue;
        }
        let mut start = ticks.len() - 1;
        while start > 0 && ticks[start] - ticks[start - 1] <= WAR_GAP_TICKS {
            start -= 1;
        }
        if ticks.len() - start < WAR_MIN_BATTLES {
            continue;
        }
        let mut line = format!("Война: {} и {} — бои идут с {} года", c.name(&a), c.name(&b), c.year(ticks[start]));
        for (loser, winner) in [(&a, &b), (&b, &a)] {
            if c.world.war_streaks.get(loser).is_some_and(|(w, n)| w == winner && *n >= 1) {
                line.push_str(&format!("; {} терпит поражение за поражением; победитель — {}", c.name(loser), c.name(winner)));
            }
        }
        in_war.insert((a.clone(), b.clone()));
        let with_player = player.is_some_and(|p| p == a || p == b);
        wars.push((!with_player, ticks[start], line, (a, b)));
    }
    wars.sort();
    out.extend(wars.into_iter().take(WARS_MAX).map(|w| w.2));
    // Alliances standing against a common enemy.
    for a in c.world.alliances.iter().filter(|a| a.common_enemy.is_some()) {
        let e = a.common_enemy.as_deref().unwrap();
        let since = a.enemy_since.map(|t| format!(" — с {} года", c.year(t))).unwrap_or_default();
        out.push(format!("Противостояние: союз {} против общего врага ({}){}", c.names(&a.actor_ids), c.name(e), since));
    }
    // Crises: tags declaring one, with the year the actor took them.
    let mut ids: Vec<&String> = c.world.actors.keys().filter(|id| c.alive(id)).collect();
    ids.sort();
    for id in &ids {
        for tag in &c.world.actors[*id].tags {
            let Some(label) = c.scenario.tag_definitions.iter().find(|t| &t.id == tag).and_then(|t| t.crisis.as_deref()) else { continue };
            let since = c.world.annals.tags_since.get(*id).and_then(|m| m.get(tag)).copied().unwrap_or(0);
            out.push(format!("{}: {} — с {} года", c.name(id), label, c.year(since)));
        }
    }
    // A power on the brink: the engine's danger counters, and a run of crushing defeats outside
    // an open war.
    for id in &ids {
        let since = |n: u32| c.year((period_tick + 1).saturating_sub(n));
        if let Some(n) = c.world.collapse_warning_ticks.get(*id).filter(|n| **n > 0) {
            out.push(format!("{} на грани гибели — с {} года", c.name(id), since(*n)));
        }
        if let Some(n) = c.world.vassalage_warning_ticks.get(*id).filter(|n| **n > 0) {
            out.push(format!("{} на грани подчинения — с {} года", c.name(id), since(*n)));
        }
        if let Some((w, n)) = c.world.war_streaks.get(*id) {
            let pair = if id.as_str() < w.as_str() { ((*id).clone(), w.clone()) } else { (w.clone(), (*id).clone()) };
            if *n >= 1 && c.alive(w) && !in_war.contains(&pair) {
                out.push(format!("{} терпит поражение за поражением; победитель — {}", c.name(id), c.name(w)));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 3. Chronology
// ---------------------------------------------------------------------------

fn chronology(c: &Ctx, log: &EventLog, period_tick: u32, out: &mut WorldAnnals) {
    // (tick, order within the tick, item)
    let mut items: Vec<(u32, u8, String)> = Vec::new();
    for d in &c.world.dead_actors {
        let by = c.world.conquered_by.get(&d.id).map(|w| format!(", завоёвана: {}", c.name(w))).unwrap_or_default();
        let name = if d.name.is_empty() { c.name(&d.id) } else { d.name.clone() };
        items.push((d.tick_death, 0, format!("пала держава {name}{by}")));
    }
    for e in log.events.iter().filter(|e| e.tick <= period_tick) {
        if matches!(e.event_type, EventType::Birth) || e.id.starts_with("spawn_") {
            items.push((e.tick, 1, e.description.trim().trim_end_matches('.').to_string()));
        } else if let Some((v, o, formed)) = vassalage_change(e) {
            let line = if formed {
                format!("{} — вассал; сюзерен — {}", c.name(&v), c.name(&o))
            } else {
                format!("{} вышла из-под власти сюзерена ({})", c.name(&v), c.name(&o))
            };
            items.push((e.tick, 2, line));
        }
    }
    for a in &c.world.alliances {
        match (&a.common_enemy, a.enemy_since) {
            (Some(e), Some(t)) if t == a.formed_tick => items.push((t, 3, format!("союз против общего врага ({}): {}", c.name(e), c.names(&a.actor_ids)))),
            (Some(e), Some(t)) => {
                items.push((a.formed_tick, 3, format!("союз: {}", c.names(&a.actor_ids))));
                items.push((t, 3, format!("союз обращён против общего врага ({}): {}", c.name(e), c.names(&a.actor_ids))));
            }
            _ => items.push((a.formed_tick, 3, format!("союз: {}", c.names(&a.actor_ids)))),
        }
    }
    for m in c.scenario.milestone_events.iter().filter(|m| c.world.milestone_events_fired.contains(&m.id)) {
        if let Some(e) = log.events.iter().find(|e| e.id == m.id && e.tick <= period_tick) {
            let what = if m.llm_context_shift.trim().is_empty() { m.id.clone() } else { first_sentence(&m.llm_context_shift) };
            items.push((e.tick, 4, format!("веха: {}", what.trim_end_matches('.'))));
        }
    }
    let mut player: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for e in log.events.iter().filter(|e| matches!(e.event_type, EventType::PlayerAction) && e.tick <= period_tick) {
        let n = player_action_name(e);
        let v = player.entry(e.tick).or_default();
        if !v.contains(&n) {
            v.push(n);
        }
    }
    items.sort();
    let recent_from = period_tick.saturating_sub(CHRONOLOGY_RECENT_TICKS - 1);
    // Older: a line per decade.
    let mut decades: BTreeMap<i32, Vec<String>> = BTreeMap::new();
    let mut decade_player: BTreeMap<i32, Vec<String>> = BTreeMap::new();
    for (t, _, s) in items.iter().filter(|x| x.0 < recent_from) {
        let v = decades.entry(c.year(*t) / 10 * 10).or_default();
        if !v.contains(s) {
            v.push(s.clone());
        }
    }
    for (t, names) in player.range(..recent_from) {
        let v = decade_player.entry(c.year(*t) / 10 * 10).or_default();
        for n in names {
            if !v.contains(n) {
                v.push(n.clone());
            }
        }
    }
    let all: BTreeSet<i32> = decades.keys().chain(decade_player.keys()).copied().collect();
    for d in all {
        let mut parts = decades.remove(&d).unwrap_or_default();
        if let Some(p) = decade_player.remove(&d) {
            parts.push(format!("решения игрока: {}", p.join(", ")));
        }
        out.chronology_decades.push(format!("{d}-е: {}", parts.join("; ")));
    }
    // Recent: a line per half-year.
    let mut recent: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for (t, _, s) in items.iter().filter(|x| x.0 >= recent_from) {
        recent.entry(*t).or_default().push(s.clone());
    }
    for (t, names) in player.range(recent_from..) {
        recent.entry(*t).or_default().push(format!("решения игрока: {}", names.join(", ")));
    }
    for (t, parts) in recent {
        out.chronology_recent.push(format!("{} — {}", c.date(t), parts.join("; ")));
    }
}

// ---------------------------------------------------------------------------
// 4. Dossier of the leading powers
// ---------------------------------------------------------------------------

/// How a metric moves, in words: (rising, falling, ratio-scaled?).
const METRIC_WORDS: [(&str, &str, bool); 5] = [
    ("богатеет", "беднеет", true),
    ("армия растёт", "армия тает", true),
    ("население растёт", "население убывает", true),
    ("власть крепнет", "власть теряет признание", false),
    ("внутреннее единство крепнет", "раздоры растут", false),
];

/// A step too small to break or start a run: under 5 % for a ratio-scaled metric, under 2 points
/// for a bounded one.
fn flat(ratio: bool, a: f64, b: f64) -> bool {
    if ratio { (b - a).abs() / a.max(1.0) < 0.05 } else { (b - a).abs() < 2.0 }
}

/// The size of a change from `start` to `cur` in words, and a score to rank it; `None` below the
/// threshold of a notable change: ×1.5 (or ÷1.5) for a ratio-scaled metric, 15 points for a
/// bounded one. The thresholds are recorded in docs/investigation_chronicle_annals.md.
fn magnitude(ratio: bool, start: f64, cur: f64) -> Option<(&'static str, u8)> {
    if ratio {
        let (s, c) = (start.max(1.0), cur.max(1.0));
        let f = if c >= s { c / s } else { s / c };
        if f >= 3.0 { Some(("втрое", 3)) } else if f >= 2.0 { Some(("вдвое", 2)) } else if f >= 1.5 { Some(("заметно", 1)) } else { None }
    } else {
        let d = (cur - start).abs();
        if d >= 30.0 { Some(("резко", 2)) } else if d >= 15.0 { Some(("заметно", 1)) } else { None }
    }
}

fn dossier(c: &Ctx, period_tick: u32) -> Vec<String> {
    let mut ids: Vec<String> = c.world.actors.values()
        .filter(|a| a.narrative_status == crate::core::NarrativeStatus::Foreground && c.alive(&a.id))
        .map(|a| a.id.clone())
        .collect();
    if let Some(p) = c.scenario.player_actor_id.as_ref().filter(|p| c.alive(p)) {
        ids.retain(|x| x != p);
        ids.sort();
        ids.insert(0, p.clone());
    } else {
        ids.sort();
    }
    let mut out = Vec::new();
    for id in &ids {
        let actor = &c.world.actors[id];
        let mut found: Vec<(u8, usize, String)> = Vec::new();
        for (i, metric) in ANNALS_METRICS.iter().enumerate() {
            let (up, down, ratio) = METRIC_WORDS[i];
            let mut series: Vec<(u32, f64)> = c.world.annals.samples.iter()
                .filter(|s| s.tick <= period_tick)
                .filter_map(|s| s.actors.get(id).map(|v| (s.tick, v[i])))
                .collect();
            let cur = actor.get_metric(metric);
            if series.last().is_none_or(|l| l.0 < period_tick) {
                series.push((period_tick, cur));
            }
            if series.len() < 2 {
                continue;
            }
            // Walk back from now while the steps keep one direction (flat steps continue it).
            let mut dir = 0i8;
            let mut start = series.len() - 1;
            while start > 0 {
                let (a, b) = (series[start - 1].1, series[start].1);
                if !flat(ratio, a, b) {
                    let d = if b > a { 1 } else { -1 };
                    if dir == 0 { dir = d } else if d != dir { break }
                }
                start -= 1;
            }
            if dir == 0 {
                continue;
            }
            let Some((size, score)) = magnitude(ratio, series[start].1, cur) else { continue };
            let since = if series[start].0 == 0 {
                format!("с {} года", c.scenario.start_year)
            } else {
                format!("с {}-х", c.year(series[start].0) / 10 * 10)
            };
            found.push((score, i, format!("{} {since} ({size})", if dir > 0 { up } else { down })));
        }
        if found.is_empty() {
            continue;
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let parts: Vec<String> = found.into_iter().take(DOSSIER_PER_ACTOR).map(|f| f.2).collect();
        out.push(format!("{}: {}", actor.name, parts.join("; ")));
    }
    out
}

// ---------------------------------------------------------------------------
// 5. The player's policy
// ---------------------------------------------------------------------------

fn player_policy(c: &Ctx, log: &EventLog, period_tick: u32, out: &mut WorldAnnals) {
    let recent_from = period_tick.saturating_sub(PLAYER_RECENT_TICKS - 1);
    let mut recent: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    // kind → (name, first tick, last tick, count)
    let mut older: BTreeMap<String, (String, u32, u32, u32)> = BTreeMap::new();
    for e in log.events.iter().filter(|e| matches!(e.event_type, EventType::PlayerAction)) {
        let name = player_action_name(e);
        if e.tick >= recent_from {
            let v = recent.entry(e.tick).or_default();
            if !v.contains(&name) {
                v.push(name);
            }
        } else {
            let k = older.entry(e.id.clone()).or_insert((name, e.tick, e.tick, 0));
            k.1 = k.1.min(e.tick);
            k.2 = k.2.max(e.tick);
            k.3 += 1;
        }
    }
    for (t, names) in recent {
        out.player_recent.push(format!("{}: {}", c.date(t), names.join("; ")));
    }
    let mut older: Vec<(String, u32, u32, u32)> = older.into_values().collect();
    older.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    for (name, first, last, n) in older {
        let how = if n == 1 { "однажды" } else { "неоднократно" };
        let (y1, y2) = (c.year(first), c.year(last));
        let when = if y1 == y2 { format!("{y1} г.") } else { format!("{y1}–{y2} гг.") };
        out.player_older.push(format!("{name} — {how}, {when}"));
    }
}

// ---------------------------------------------------------------------------
// 6. Stop-list
// ---------------------------------------------------------------------------

/// The words of a text, lower-cased: letters and digits, a hyphen inside a word kept.
pub fn words(text: &str) -> Vec<String> {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '-'))
        .map(|w| w.trim_matches('-').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Turns of phrase the book has already repeated: openings shared by `STOP_MIN_CHRONICLES` or
/// more chronicles, and n-grams of `STOP_WORDS_MIN` to `STOP_WORDS_MAX` words found in that many
/// chronicles. Taken greedily — openings first, then the most repeated, the longest, the most
/// recent — and a phrase is skipped if, in any chronicle, it overlaps or adjoins one already
/// taken: two pieces of one repeated sentence would otherwise stand side by side on the list and
/// give the prompt back the sentence (measured: 48 seven-word runs of the book in one game). At
/// most `STOP_MAX`.
pub fn stop_phrases(book: &[&str]) -> Vec<String> {
    let texts: Vec<Vec<String>> = book.iter().map(|t| words(t)).collect();
    // (class: 0 opening / 1 phrase, count reversed, length reversed, last chronicle reversed, words)
    let mut cands: BTreeSet<(u8, usize, usize, usize, Vec<String>)> = BTreeSet::new();
    let mut openings: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
    for (i, w) in texts.iter().enumerate() {
        if w.len() >= STOP_WORDS_MIN {
            openings.entry(w[..STOP_WORDS_MIN].to_vec()).or_default().push(i);
        }
    }
    for idx in openings.values().filter(|v| v.len() >= STOP_MIN_CHRONICLES) {
        let mut n = STOP_WORDS_MIN;
        while n < STOP_WORDS_MAX && idx.iter().all(|i| texts[*i].len() > n && texts[*i][n] == texts[idx[0]][n]) {
            n += 1;
        }
        cands.insert((0, usize::MAX - idx.len(), usize::MAX - n, usize::MAX - idx.last().unwrap(), texts[idx[0]][..n].to_vec()));
    }
    for n in STOP_WORDS_MIN..=STOP_WORDS_MAX {
        let mut seen: BTreeMap<&[String], Vec<usize>> = BTreeMap::new();
        for (i, w) in texts.iter().enumerate() {
            let grams: BTreeSet<&[String]> = w.windows(n).collect();
            for g in grams {
                seen.entry(g).or_default().push(i);
            }
        }
        for (g, v) in seen.into_iter().filter(|(_, v)| v.len() >= STOP_MIN_CHRONICLES) {
            cands.insert((1, usize::MAX - v.len(), usize::MAX - n, usize::MAX - v.last().unwrap(), g.to_vec()));
        }
    }
    // Where a phrase stands in the book: (chronicle, first word, end).
    let spans = |p: &[String]| -> Vec<(usize, usize, usize)> {
        texts.iter().enumerate()
            .flat_map(|(i, w)| w.windows(p.len()).enumerate().filter(|(_, g)| *g == p).map(move |(s, _)| (i, s, s + p.len())))
            .collect()
    };
    let mut taken: Vec<(usize, usize, usize)> = Vec::new();
    let mut out = Vec::new();
    for (_, _, _, _, p) in cands {
        if out.len() == STOP_MAX {
            break;
        }
        let sp = spans(&p);
        if sp.iter().any(|(i, s, e)| taken.iter().any(|(j, ts, te)| i == j && s <= te && ts <= e)) {
            continue;
        }
        taken.extend(sp);
        out.push(p.join(" "));
    }
    out
}
