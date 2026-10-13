//! The chronicler's «летопись мира» — six layers of memory, every one built from the engine's
//! facts (owner's decision of 2026-10-11, docs/investigation_chronicle_annals.md):
//!
//! 1. relations now — alliances, vassalages, declared conquests: a state, held while it lasts;
//! 2. open stories — wars of a pair, crises, a power on the brink — each with the year it began;
//! 3. the game's chronology — only what is over (fallen powers, ended wars and vassalages); the
//!    last ten years by half-year, older ones a line per decade;
//! 4. the most notable trajectories of the powers (at most `DOSSIER_MAX`), in words, from the
//!    world's annals (`core::Annals`);
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
/// The dossier: the most notable changes of the world, at most this many per half-year, one of a
/// kind first (`dossier_pick`; owner's decision of 2026-10-13: before, up to three per power and
/// ten powers — 664 lines in 60 prompts, 18 % of them echoed in the chronicle).
pub const DOSSIER_MAX: usize = 3;

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

/// The wars of a pair: the engine's battles in the log, as runs with gaps of at most
/// `WAR_GAP_TICKS` and at least `WAR_MIN_BATTLES` battles — (a, b, first battle, last battle,
/// open). A run is open while it is the pair's last, its last battle is at most `WAR_GAP_TICKS`
/// old, both are alive and the engine still lets them fight (vassalage under Ц7, alliance under
/// Ц9 — the same checks as `interactions`); any other run is a war that is over.
fn wars(c: &Ctx, log: &EventLog, period_tick: u32) -> Vec<(String, String, u32, u32, bool)> {
    use crate::engine::interactions::{alliances_on, allied, bound_by_vassalage, conquest_on};
    let mut battles: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
    for e in log.events.iter().filter(|e| e.tick <= period_tick) {
        if let Some(def) = other_of(&e.id, "military_conflict_", &e.actor_id, true) {
            let key = if e.actor_id < def { (e.actor_id.clone(), def) } else { (def, e.actor_id.clone()) };
            battles.entry(key).or_default().push(e.tick);
        }
    }
    let mut out = Vec::new();
    for ((a, b), mut ticks) in battles {
        ticks.sort();
        ticks.dedup();
        let at_peace = (conquest_on(c.scenario) && bound_by_vassalage(c.world, &a, &b)) || (alliances_on(c.scenario) && allied(c.world, &a, &b));
        let mut start = 0;
        for i in 1..=ticks.len() {
            if i < ticks.len() && ticks[i] - ticks[i - 1] <= WAR_GAP_TICKS {
                continue;
            }
            if i - start >= WAR_MIN_BATTLES {
                let last = ticks[i - 1];
                let open = i == ticks.len() && period_tick.saturating_sub(last) <= WAR_GAP_TICKS && c.alive(&a) && c.alive(&b) && !at_peace;
                out.push((a.clone(), b.clone(), ticks[start], last, open));
            }
            start = i;
        }
    }
    out
}

fn open_stories(c: &Ctx, log: &EventLog, period_tick: u32) -> Vec<String> {
    let mut out = Vec::new();
    let player = c.scenario.player_actor_id.as_deref();
    let mut wars_open: Vec<(bool, u32, String, (String, String))> = Vec::new();
    let mut in_war: BTreeSet<(String, String)> = BTreeSet::new();
    for (a, b, start, _, open) in wars(c, log, period_tick) {
        if !open {
            continue;
        }
        let mut line = format!("Война: {} и {} — бои идут с {} года", c.name(&a), c.name(&b), c.year(start));
        for (loser, winner) in [(&a, &b), (&b, &a)] {
            if c.world.war_streaks.get(loser).is_some_and(|(w, n)| w == winner && *n >= 1) {
                line.push_str(&format!("; {} терпит поражение за поражением; победитель — {}", c.name(loser), c.name(winner)));
            }
        }
        in_war.insert((a.clone(), b.clone()));
        let with_player = player.is_some_and(|p| p == a || p == b);
        wars_open.push((!with_player, start, line, (a, b)));
    }
    wars_open.sort();
    out.extend(wars_open.into_iter().take(WARS_MAX).map(|w| w.2));
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

/// Only what is over. What still holds stands in its own block and nowhere here: a standing
/// alliance and a vassalage in force in «Отношения», an open war in «Открытые сюжеты», the
/// player's decisions in «Политика игрока», the scenario's milestones in «Что изменилось с начала
/// сценария» (measured before: 353 of 402 chronology items repeated another block of the same
/// prompt). What is left — fallen powers, a power entering the stage, ended vassalages, ended
/// wars, an alliance's member turned its enemy once that alliance is gone — is what no other
/// block keeps: on a short game the layer is nearly empty, on a long one it holds the closed past.
fn chronology(c: &Ctx, log: &EventLog, period_tick: u32, out: &mut WorldAnnals) {
    // (tick, order within the tick, item)
    let mut items: Vec<(u32, u8, String)> = Vec::new();
    for d in &c.world.dead_actors {
        let by = c.world.conquered_by.get(&d.id).map(|w| format!(", завоёвана: {}", c.name(w))).unwrap_or_default();
        let name = if d.name.is_empty() { c.name(&d.id) } else { d.name.clone() };
        items.push((d.tick_death, 0, format!("пала держава {name}{by}")));
    }
    // Vassalages: (vassal, overlord) → the ticks it formed and ended.
    let mut vass: BTreeMap<(String, String), Vec<(u32, bool)>> = BTreeMap::new();
    for e in log.events.iter().filter(|e| e.tick <= period_tick) {
        if matches!(e.event_type, EventType::Birth) || e.id.starts_with("spawn_") {
            let d = super::chronicler_description(e, c.scenario);
            let d = d.trim().trim_end_matches('.');
            // Inside a line of the chronology, after «;»: the template's capital goes.
            let mut ch = d.chars();
            let d = ch.next().map(|f| f.to_lowercase().chain(ch).collect::<String>()).unwrap_or_default();
            items.push((e.tick, 1, d));
        } else if let Some((v, o, formed)) = vassalage_change(e) {
            vass.entry((v, o)).or_default().push((e.tick, formed));
        } else if let Some(id) = c.scenario.random_events.iter().find(|r| r.id == e.id).and_then(|r| r.leaves_alliance_as_enemy.as_ref()) {
            // Turned against its alliance: kept once no alliance stands against it any more.
            if !c.world.alliances.iter().any(|a| a.common_enemy.as_deref() == Some(id.as_str())) {
                items.push((e.tick, 3, format!("{} выходит из союза и становится его общим врагом", c.name(id))));
            }
        }
    }
    for ((v, o), changes) in vass {
        let holds = c.world.vassalages.iter().any(|x| x.vassal_id == v && x.overlord_id == o);
        let last_formed = changes.iter().rev().find(|x| x.1).map(|x| x.0);
        for (t, formed) in changes {
            // The vassalage in force is in «Отношения» with its year; only its earlier spells are past.
            if formed && holds && Some(t) == last_formed {
                continue;
            }
            let line = if formed {
                format!("{} становится вассалом; сюзерен — {}", c.name(&v), c.name(&o))
            } else {
                format!("{} выходит из-под власти сюзерена ({})", c.name(&v), c.name(&o))
            };
            items.push((t, 2, line));
        }
    }
    for (a, b, first, last, _) in wars(c, log, period_tick).into_iter().filter(|w| !w.4) {
        let years = if c.year(first) == c.year(last) { format!("в {} году", c.year(first)) } else { format!("с {} по {} год", c.year(first), c.year(last)) };
        items.push((last, 2, format!("война: {} и {} — бои {years}, окончена", c.name(&a), c.name(&b))));
    }
    items.sort();
    let recent_from = period_tick.saturating_sub(CHRONOLOGY_RECENT_TICKS - 1);
    // Older: a line per decade.
    let mut decades: BTreeMap<i32, Vec<String>> = BTreeMap::new();
    for (t, _, s) in items.iter().filter(|x| x.0 < recent_from) {
        let v = decades.entry(c.year(*t) / 10 * 10).or_default();
        if !v.contains(s) {
            v.push(s.clone());
        }
    }
    for (d, parts) in decades {
        out.chronology_decades.push(format!("{d}-е: {}", parts.join("; ")));
    }
    // Recent: a line per half-year.
    let mut recent: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for (t, _, s) in items.iter().filter(|x| x.0 >= recent_from) {
        recent.entry(*t).or_default().push(s.clone());
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

/// The size of a change from `start` to `cur` in words, and its strength in thresholds (1.0 at
/// the threshold) to rank it across metrics; `None` below the threshold of a notable change:
/// ×1.5 (or ÷1.5) for a ratio-scaled metric, 15 points for a bounded one. The thresholds are
/// recorded in docs/investigation_chronicle_annals.md.
fn magnitude(ratio: bool, start: f64, cur: f64) -> Option<(&'static str, f64)> {
    if ratio {
        let (s, c) = (start.max(1.0), cur.max(1.0));
        let f = if c >= s { c / s } else { s / c };
        let strength = f.ln() / 1.5f64.ln();
        if f >= 3.0 { Some(("втрое", strength)) } else if f >= 2.0 { Some(("вдвое", strength)) } else if f >= 1.5 { Some(("заметно", strength)) } else { None }
    } else {
        let d = (cur - start).abs();
        if d >= 30.0 { Some(("резко", d / 15.0)) } else if d >= 15.0 { Some(("заметно", d / 15.0)) } else { None }
    }
}

/// The `DOSSIER_MAX` changes shown: the strongest of each kind first (a metric), then the
/// strongest of the rest. Strength alone gave three armies melting away in 38 of 56 prompts — an
/// army changes by multiples, nothing else outranks it — and no line on who grows rich or whose
/// realm falls apart.
pub fn dossier_pick(all: &[(f64, usize, String)]) -> Vec<String> {
    let mut picked: Vec<usize> = Vec::new();
    let mut kinds: BTreeSet<usize> = BTreeSet::new();
    for (i, (_, kind, _)) in all.iter().enumerate() {
        if picked.len() < DOSSIER_MAX && kinds.insert(*kind) {
            picked.push(i);
        }
    }
    for i in 0..all.len() {
        if picked.len() < DOSSIER_MAX && !picked.contains(&i) {
            picked.push(i);
        }
    }
    picked.sort();
    picked.into_iter().map(|i| all[i].2.clone()).collect()
}

fn dossier(c: &Ctx, period_tick: u32) -> Vec<String> {
    dossier_pick(&dossier_candidates(c, period_tick))
}

/// Every notable change of the powers, strongest first: (strength, kind — the metric, line).
pub fn dossier_all(world: &WorldState, scenario: &Scenario, period_tick: u32) -> Vec<(f64, usize, String)> {
    dossier_candidates(&Ctx { world, scenario }, period_tick)
}

fn dossier_candidates(c: &Ctx, period_tick: u32) -> Vec<(f64, usize, String)> {
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
    // (strength, the power's place — the player first, then by id —, metric, line)
    let mut found: Vec<(f64, usize, usize, String)> = Vec::new();
    for (place, id) in ids.iter().enumerate() {
        let actor = &c.world.actors[id];
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
            found.push((score, place, i, format!("{}: {} {since} ({size})", actor.name, if dir > 0 { up } else { down })));
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    found.into_iter().map(|f| (f.0, f.2, f.3)).collect()
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
    words_cased(text).into_iter().map(|w| w.0).collect()
}

/// `words`, each with whether it was written with a capital letter.
fn words_cased(text: &str) -> Vec<(String, bool)> {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '-'))
        .map(|w| w.trim_matches('-'))
        .filter(|w| !w.is_empty())
        .map(|w| (w.to_lowercase(), w.chars().next().is_some_and(char::is_uppercase)))
        .collect()
}

/// A word that may join names in a list of powers: a conjunction, or the alliance itself.
fn joins_names(w: &str) -> bool {
    matches!(w, "и" | "или" | "а" | "да" | "с" | "со" | "также") || w.starts_with("союз") || w.starts_with("лиг") || w.starts_with("коалиц")
}

/// Turns of phrase the book has already repeated: openings shared by `STOP_MIN_CHRONICLES` or
/// more chronicles, and n-grams of `STOP_WORDS_MIN` to `STOP_WORDS_MAX` words found in that many
/// chronicles. Taken greedily — openings first, then the most repeated, the longest, the most
/// recent — and a phrase is skipped if it is only names of powers joined by «и» or the alliance's
/// name, or if, in any chronicle, it overlaps or adjoins one already taken: two pieces of one
/// repeated sentence would otherwise stand side by side on the list and give the prompt back the
/// sentence (measured: 48 seven-word runs of the book in one game). At
/// most `STOP_MAX`.
pub fn stop_phrases(book: &[&str]) -> Vec<String> {
    let cased: Vec<Vec<(String, bool)>> = book.iter().map(|t| words_cased(t)).collect();
    let texts: Vec<Vec<String>> = cased.iter().map(|w| w.iter().map(|x| x.0.clone()).collect()).collect();
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
        // Only names of powers and the words joining them («Венеции, Неаполя, Рима, Сицилии и
        // Флоренции»): forbidding it would leave no way to name the alliance (measured: 7 of the 9
        // stop-list turns the chronicles repeated were such lists).
        let names_only = |&(i, s, e): &(usize, usize, usize)| {
            let w = &cased[i][s..e];
            w.iter().filter(|x| x.1).count() >= 2 && w.iter().all(|x| x.1 || joins_names(&x.0))
        };
        if sp.iter().all(names_only) {
            continue;
        }
        taken.extend(sp);
        out.push(p.join(" "));
    }
    out
}
