//! The chronicler's memory, measured (owner's decision after the switch to v2, 2026-10-11).
//! Measurement only: the engine and the prompt are not touched.
//!
//! milan_1477 on v2 (as the content stands), with no player and in `aggressive`. The product's path,
//! as `cmd_get_narrative` runs it after every tick: `build_snapshot` → `generate_narrative_prompt` →
//! the live model (`generate_narrative_blocking`, the config of `get_llm_config`) → the game's book
//! (`record_chronicle`). Consecutive chronicles of one game are written to `<outdir>`, each with
//! its prompt and the engine's facts at that moment, so the input can be checked against the world:
//! v2's vassalages, alliances and the league, the player's actions over the whole game against the
//! five in the prompt, the fallen and the milestones.
//!
//! Usage:
//!   chronicle_probe scan <world> <seeds>                  — per seed: when v2's facts appear
//!   chronicle_probe live <world> <seed> <first> <n> <outdir> — n chronicles from tick `first`
//!   chronicle_probe dry  <world> <seed> <first> <n> <outdir> — the same, prompts only
//!     (`CHRONICLE_MODEL=<id>` runs `live` on another model of the same provider,
//!     `CHRONICLE_MAX_TOKENS=<n>` with another answer budget)
//!   chronicle_probe step <world> <seed> <first> <n> <outdir> — the same chain, written from outside: replays
//!     the game, puts every `<tag>.text.txt` already in `outdir` into the book, writes the next prompt and
//!     prints `NEXT <prompt> <text to write>`; prints `DONE` once all `n` are written (a chronicler that is
//!     not an API — the Sonnet subagents of step 2)
//!   chronicle_probe check <livedir> <outdir>              — the world's annals (owner's decision
//!     of 2026-10-11), checked without the model on the games of `livedir`: the same windows, the
//!     saved chronicles put into the book as they were written, each layer against the engine

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;
use std::fmt::Write as _;

const SC: &str = "milan_1477";

fn new_game(world: &str, seed: u64) -> (engine13::AppState, engine13::db::Db, Option<ScriptedStrategy>) {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, SC.to_string()).unwrap();
    assert!(st.current_scenario.as_ref().unwrap().features.economy_v2, "milan must play on v2");
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, SC));
    (st, db, strategy)
}

fn advance(st: &mut engine13::AppState, strategy: &Option<ScriptedStrategy>) {
    match strategy {
        Some(s) => { play_scripted_tick(st, s); }
        None => {
            let ws = st.world_state.as_mut().unwrap();
            let scn = st.current_scenario.as_ref().unwrap();
            engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
        }
    }
}

/// The engine's v2 facts and the game's history at this moment, as lines.
fn facts(st: &engine13::AppState) -> Vec<String> {
    let ws = st.world_state.as_ref().unwrap();
    let name = |id: &str| ws.actors.get(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());
    let mut out = Vec::new();
    for v in &ws.vassalages {
        out.push(format!("vassal: {} ({}) → overlord {} ({}), since tick {}", v.vassal_id, name(&v.vassal_id), v.overlord_id, name(&v.overlord_id), v.formed_tick));
    }
    for a in &ws.alliances {
        let mut m = a.actor_ids.clone();
        m.sort();
        out.push(format!("alliance: [{}] against {:?}, since tick {}", m.join(", "), a.common_enemy, a.formed_tick));
    }
    for (loser, winner) in &ws.conquered_by { out.push(format!("conquered: {loser} by {winner}")); }
    for d in &ws.dead_actors { out.push(format!("dead: {} ({}) tick {}", d.id, d.name, d.tick_death)); }
    let acts: Vec<String> = st.event_log.events.iter().filter(|e| matches!(e.event_type, engine13::core::EventType::PlayerAction)).map(|e| format!("{}@{}", e.id, e.tick)).collect();
    out.push(format!("player actions so far: {} — {}", acts.len(), acts.join(", ")));
    let mut ms: Vec<&String> = ws.milestone_events_fired.iter().collect();
    ms.sort();
    out.push(format!("milestones fired: {}", ms.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    let v2ev: Vec<String> = st.event_log.events.iter().filter(|e| e.id.starts_with("vassalage_") || e.id == "italian_league_against_milan" || e.id.starts_with("assault_")).map(|e| format!("{}@{}", e.id, e.tick)).collect();
    out.push(format!("v2 events in the log: {}", v2ev.join(", ")));
    out
}

fn scan(world: &str, seeds: u64) {
    println!("| seed | Florence → Siena (tick) | other vassalages | league turns on Milan | deaths |");
    println!("|---|---|---|---|---|");
    for seed in 0..seeds {
        let (mut st, _db, strategy) = new_game(world, seed);
        let mut vass: Vec<(String, u32)> = Vec::new();
        for _ in 0..300 {
            advance(&mut st, &strategy);
            let ws = st.world_state.as_ref().unwrap();
            for v in &ws.vassalages {
                let k = format!("{}→{}", v.overlord_id, v.vassal_id);
                if !vass.iter().any(|(x, _)| *x == k) { vass.push((k, v.formed_tick)); }
            }
        }
        let league = st.event_log.events.iter().find(|e| e.id == "italian_league_against_milan").map(|e| e.tick);
        let fs = vass.iter().find(|(k, _)| k == "florence→siena").map(|x| x.1);
        let other: Vec<String> = vass.iter().filter(|(k, _)| k != "florence→siena").map(|(k, t)| format!("{k}@{t}")).collect();
        println!("| {seed} | {fs:?} | {} | {league:?} | {} |", other.join(", "), st.world_state.as_ref().unwrap().dead_actors.len());
    }
}

fn chronicles(world: &str, seed: u64, first: u32, n: u32, outdir: &str, live: bool) {
    let (mut st, db, strategy) = new_game(world, seed);
    let mut cfg = engine13::llm::get_llm_config();
    // The model comparison (step 2): the same provider and key, another model.
    if let Ok(m) = std::env::var("CHRONICLE_MODEL") { cfg.model = m; }
    if let Some(t) = std::env::var("CHRONICLE_MAX_TOKENS").ok().and_then(|t| t.parse().ok()) { cfg.max_tokens = Some(t); }
    if live { eprintln!("[chronicle] provider={} model={} max_tokens={}", cfg.provider, cfg.model, cfg.max_tokens()); }
    std::fs::create_dir_all(outdir).unwrap();
    let mut book_md = format!("# milan_1477 / {world} / seed {seed} — chronicles {first}…{}\n\nmodel: {} ({})\n", first + n - 1, cfg.model, cfg.provider);
    for _ in 0..first { advance(&mut st, &strategy); }
    for i in 0..n {
        advance(&mut st, &strategy);
        let (prompt, stamp) = {
            let ws = st.world_state.as_ref().unwrap();
            let scn = st.current_scenario.as_ref().unwrap();
            let snapshot = engine13::llm::build_snapshot(ws, scn, &st.event_log);
            (engine13::llm::generate_narrative_prompt(&snapshot, scn, &db), engine13::llm::ChronicleStamp::of(ws, &snapshot))
        };
        let text = if live {
            match engine13::llm::generate_narrative_blocking(&prompt, &cfg, 5) {
                Ok(t) => Some(t),
                Err(e) => { eprintln!("[chronicle] tick {} FAILED: {e}", stamp.tick); None }
            }
        } else { None };
        let written = engine13::llm::record_chronicle(st.world_state.as_mut().unwrap(), &stamp, text.clone());
        let f = facts(&st);
        let tag = format!("{outdir}/{world}_s{seed}_{i:02}_t{}", stamp.tick);
        std::fs::write(format!("{tag}.prompt.txt"), &prompt).unwrap();
        std::fs::write(format!("{tag}.facts.txt"), f.join("\n")).unwrap();
        if let Some(t) = &text { std::fs::write(format!("{tag}.text.txt"), t).unwrap(); }
        let _ = write!(book_md, "\n---\n\n## tick {} — {} год, {} (in the book: {written})\n\n### facts\n\n{}\n\n### chronicle\n\n{}\n",
            stamp.tick, stamp.year, stamp.half_year, f.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n"), text.as_deref().unwrap_or("(dry)"));
        eprintln!("[chronicle] {world} seed {seed} tick {} done", stamp.tick);
    }
    let book = &st.world_state.as_ref().unwrap().chronicle_book;
    let _ = write!(book_md, "\n---\n\nbook: {} entries\n", book.len());
    std::fs::write(format!("{outdir}/{world}_s{seed}.md"), book_md).unwrap();
}

/// One line of a block of the prompt that starts with `- `, between `title` and the next blank line.
fn block<'a>(prompt: &'a str, title: &str) -> Vec<&'a str> {
    let Some(i) = prompt.find(title) else { return Vec::new() };
    prompt[i + title.len()..].lines().skip(1).take_while(|l| l.starts_with("- ")).collect()
}

/// The event window's lines: (lines, with an actor id, with an event key, with another Latin
/// token — a tag id, an engine word, with a digit, with a tag id, with an era's code name, with a
/// promotion's debug reason). Tokens are whole runs of ASCII letters, digits, `_`.
fn window_stats(prompt: &str, actor_ids: &[String], tag_ids: &[String]) -> [u32; 8] {
    const ERAS: [&str; 5] = ["Ancient", "EarlyMedieval", "HighMedieval", "LateMedieval", "EarlyModern"];
    const DEBUG: [&str; 3] = ["power_projection", "military contact", "internal upheaval"];
    let mut r = [0u32; 8];
    let Some(i) = prompt.find("=== НЕДАВНИЕ СОБЫТИЯ") else { return r };
    for l in prompt[i..].lines().skip(1).take_while(|l| l.starts_with("- ")) {
        r[0] += 1;
        let toks: Vec<&str> = l.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|t| !t.is_empty()).collect();
        let (head, _) = l[2..].split_once(": ").unwrap_or(("", ""));
        let key = head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !head.is_empty();
        if toks.iter().any(|t| actor_ids.iter().any(|a| a == t)) { r[1] += 1; }
        if key { r[2] += 1; }
        let body = if key { &l[2 + head.len() + 2..] } else { &l[2..] };
        let other = body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|t| !t.is_empty())
            .any(|t| !actor_ids.iter().any(|a| a == t) && t.chars().any(|c| c.is_ascii_alphabetic()));
        if other { r[3] += 1; }
        if body.chars().any(|c| c.is_ascii_digit()) { r[4] += 1; }
        if toks.iter().any(|t| tag_ids.iter().any(|a| a == t)) { r[5] += 1; }
        if toks.iter().any(|t| ERAS.contains(t)) { r[6] += 1; }
        if DEBUG.iter().any(|d| l.contains(d)) { r[7] += 1; }
    }
    r
}

#[derive(Default)]
struct Tally { facts: u32, missing: u32, examples: Vec<String> }

impl Tally {
    fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        self.facts += 1;
        if !ok {
            self.missing += 1;
            if self.examples.len() < 3 { self.examples.push(what()); }
        }
    }
    fn cell(&self) -> String { format!("{}/{}", self.facts - self.missing, self.facts) }
}

fn check(livedir: &str, outdir: &str) {
    use engine13::llm::annals::{words, CHRONOLOGY_RECENT_TICKS, PLAYER_RECENT_TICKS, STOP_WORDS_MAX};
    std::fs::create_dir_all(outdir).unwrap();
    // The games of the live run: (world, seed) → the chronicles' ticks.
    let mut games: std::collections::BTreeMap<(String, u64), Vec<u32>> = Default::default();
    for e in std::fs::read_dir(livedir).unwrap() {
        let n = e.unwrap().file_name().into_string().unwrap();
        let Some(stem) = n.strip_suffix(".text.txt") else { continue };
        let p: Vec<&str> = stem.split('_').collect();
        games.entry((p[0].to_string(), p[1][1..].parse().unwrap())).or_default().push(p[3][1..].parse().unwrap());
    }
    println!("| game | ticks | L1 alliances | L1 vassalages | L2 crises | L2 league | L2 defeats | L3 chronology | L5 old not recent | L5 old summed | L4 dossier lines | L6 stop-list (mean) | 7 words of the book: brought by the book / quoting the prompt | prompt before mean / max | after mean / max | window lines | actor ids before / after | event keys before / after | other Latin tokens before / after | digits before / after | tag ids before / after | era code names before / after | debug reasons before / after |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for ((world, seed), mut ticks) in games {
        ticks.sort();
        let (mut st, db, strategy) = new_game(&world, seed);
        for _ in 0..ticks[0] - 1 { advance(&mut st, &strategy); }
        let mut t: [Tally; 9] = Default::default();
        let (mut dossier, mut stop, mut leak, mut quoted) = (0usize, 0usize, 0u32, 0u32);
        let (mut before, mut after): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
        let (mut wb, mut wa) = ([0u32; 8], [0u32; 8]);
        let actor_ids: Vec<String> = st.current_scenario.as_ref().unwrap().actors.iter().map(|a| a.id.clone()).collect();
        let tag_ids: Vec<String> = st.current_scenario.as_ref().unwrap().tag_definitions.iter().map(|t| t.id.clone()).collect();
        for (i, tick) in ticks.iter().enumerate() {
            advance(&mut st, &strategy);
            let ws = st.world_state.as_ref().unwrap();
            assert_eq!(ws.tick, *tick);
            let scn = st.current_scenario.as_ref().unwrap();
            let snap = engine13::llm::build_snapshot(ws, scn, &st.event_log);
            let prompt = engine13::llm::generate_narrative_prompt(&snap, scn, &db);
            let stamp = engine13::llm::ChronicleStamp::of(ws, &snap);
            let tag = format!("{world}_s{seed}_{i:02}_t{tick}");
            std::fs::write(format!("{outdir}/{tag}.prompt.txt"), &prompt).unwrap();
            let old_prompt = std::fs::read_to_string(format!("{livedir}/{tag}.prompt.txt")).unwrap();
            before.push(old_prompt.chars().count());
            for (k, v) in window_stats(&old_prompt, &actor_ids, &tag_ids).into_iter().enumerate() { wb[k] += v; }
            for (k, v) in window_stats(&prompt, &actor_ids, &tag_ids).into_iter().enumerate() { wa[k] += v; }
            after.push(prompt.chars().count());
            let name = |id: &str| ws.actors.get(id).map(|a| a.name.clone()).or_else(|| ws.dead_actors.iter().find(|d| d.id == id).map(|d| d.name.clone())).unwrap_or_else(|| id.to_string());
            let period = ws.tick - 1;
            let year = |t: u32| scn.start_year + (t / 2) as i32;
            let date = |t: u32| format!("{} г., {}", year(t), engine13::HalfYear::from_tick(t).display_name());
            let rel = block(&prompt, "Отношения сейчас:");
            let stories = block(&prompt, "Открытые сюжеты (идут и сейчас):");
            let chron = block(&prompt, "Хронология партии:");
            // L1
            for a in &ws.alliances {
                t[0].check(rel.iter().any(|l| l.starts_with("- Союз:") && a.actor_ids.iter().all(|m| l.contains(&name(m)))), || format!("t{tick} alliance {:?}", a.actor_ids));
            }
            for v in &ws.vassalages {
                t[1].check(rel.iter().any(|l| l.contains(&format!("{} — вассал; сюзерен — {}", name(&v.vassal_id), name(&v.overlord_id)))), || format!("t{tick} {}→{}", v.vassal_id, v.overlord_id));
            }
            // L2
            let mut ids: Vec<&String> = ws.actors.keys().collect();
            ids.sort();
            for id in &ids {
                for tg in &ws.actors[*id].tags {
                    if let Some(label) = scn.tag_definitions.iter().find(|d| &d.id == tg).and_then(|d| d.crisis.as_deref()) {
                        t[2].check(stories.iter().any(|l| l.contains(&format!("{}: {label}", name(id)))), || format!("t{tick} {id} {tg}"));
                    }
                }
                if let Some((w, n)) = ws.war_streaks.get(*id) {
                    if *n >= 1 && ws.actors.contains_key(w) {
                        t[4].check(stories.iter().any(|l| l.contains(&format!("{} терпит поражение за поражением; победитель — {}", name(id), name(w)))), || format!("t{tick} {id}←{w}"));
                    }
                }
            }
            for a in ws.alliances.iter().filter(|a| a.common_enemy.is_some()) {
                let e = name(a.common_enemy.as_ref().unwrap());
                t[3].check(stories.iter().any(|l| l.contains(&format!("против общего врага ({e})"))), || format!("t{tick} league vs {e}"));
            }
            // L3: every death, vassalage change, milestone and player decision has its line.
            let recent_from = period.saturating_sub(CHRONOLOGY_RECENT_TICKS - 1);
            let mut fact_ticks: Vec<(u32, String)> = ws.dead_actors.iter().map(|d| (d.tick_death, format!("death {}", d.id))).collect();
            for e in st.event_log.events.iter().filter(|e| e.tick <= period) {
                let kind = matches!(e.event_type, engine13::core::EventType::PlayerAction) || (e.id.starts_with("vassalage_") && !e.id.starts_with("vassalage_tribute_"))
                    || ws.milestone_events_fired.contains(&e.id);
                if kind { fact_ticks.push((e.tick, e.id.clone())); }
            }
            for (ft, what) in fact_ticks {
                let ok = if ft >= recent_from {
                    chron.iter().any(|l| l.starts_with(&format!("- {} —", date(ft))))
                } else {
                    chron.iter().any(|l| l.starts_with(&format!("- {}-е:", year(ft) / 10 * 10)))
                };
                t[5].check(ok, || format!("t{tick} {what}@{ft}"));
            }
            // L5
            let recent = block(&prompt, "Решения последних двух лет:");
            let older = block(&prompt, "Прежние решения");
            let pr_from = period.saturating_sub(PLAYER_RECENT_TICKS - 1);
            for e in st.event_log.events.iter().filter(|e| matches!(e.event_type, engine13::core::EventType::PlayerAction) && e.tick < pr_from) {
                let n = e.description.trim_start_matches("Действие игрока: ");
                t[6].check(!recent.iter().any(|l| l.starts_with(&format!("- {}:", date(e.tick)))), || format!("t{tick} {} recent", e.id));
                t[7].check(older.iter().any(|l| l.starts_with(&format!("- {n} — "))), || format!("t{tick} {} not summed", e.id));
            }
            dossier += block(&prompt, "Ведущие державы — траектории:").len();
            stop += snap.annals.stop_phrases.len();
            // Seven words of the book in the prompt that the same prompt without the book does not
            // have: what the book itself brought in (a chronicle quoting the prompt — a milestone's
            // sentence — is not the book reaching the prompt).
            let pw = format!(" {} ", words(&prompt).join(" "));
            let mut bare = ws.clone();
            bare.chronicle_book.clear();
            let pw0 = format!(" {} ", words(&engine13::llm::generate_narrative_prompt(&engine13::llm::build_snapshot(&bare, scn, &st.event_log), scn, &db)).join(" "));
            for e in &ws.chronicle_book {
                for w in words(&e.text).windows(STOP_WORDS_MAX + 1) {
                    let w = format!(" {} ", w.join(" "));
                    if pw.contains(&w) {
                        quoted += 1;
                        if !pw0.contains(&w) { leak += 1; eprintln!("  book→prompt t{tick}:{w}"); }
                    }
                }
            }
            // The chronicle as the model wrote it goes into the book, as in the live run.
            let text = std::fs::read_to_string(format!("{livedir}/{tag}.text.txt")).ok();
            engine13::llm::record_chronicle(st.world_state.as_mut().unwrap(), &stamp, text);
        }
        let mm = |v: &Vec<usize>| format!("{} / {}", v.iter().sum::<usize>() / v.len(), v.iter().max().unwrap());
        let n = ticks.len();
        println!("| {world} s{seed} | {}–{} | {} | {} | {} | {} | {} | {} | {} | {} | {:.1} | {:.1} | {leak} / {quoted} | {} | {} | {} | {} / {} | {} / {} | {} / {} | {} / {} | {} / {} | {} / {} | {} / {} |",
            ticks[0], ticks[n - 1], t[0].cell(), t[1].cell(), t[2].cell(), t[3].cell(), t[4].cell(), t[5].cell(), t[6].cell(), t[7].cell(),
            dossier as f64 / n as f64, stop as f64 / n as f64, mm(&before), mm(&after), wa[0], wb[1], wa[1], wb[2], wa[2], wb[3], wa[3], wb[4], wa[4], wb[5], wa[5], wb[6], wa[6], wb[7], wa[7]);
        for x in &t { for e in &x.examples { eprintln!("  missing: {e}"); } }
    }
}

fn step(world: &str, seed: u64, first: u32, n: u32, outdir: &str) {
    let (mut st, db, strategy) = new_game(world, seed);
    std::fs::create_dir_all(outdir).unwrap();
    for _ in 0..first { advance(&mut st, &strategy); }
    let mut book_md = format!("# milan_1477 / {world} / seed {seed} — chronicles {first}…{}\n\nmodel: Sonnet (Claude Code subagent)\n", first + n - 1);
    for i in 0..n {
        advance(&mut st, &strategy);
        let (prompt, stamp) = {
            let ws = st.world_state.as_ref().unwrap();
            let scn = st.current_scenario.as_ref().unwrap();
            let snapshot = engine13::llm::build_snapshot(ws, scn, &st.event_log);
            (engine13::llm::generate_narrative_prompt(&snapshot, scn, &db), engine13::llm::ChronicleStamp::of(ws, &snapshot))
        };
        let tag = format!("{outdir}/{world}_s{seed}_{i:02}_t{}", stamp.tick);
        let text = std::fs::read_to_string(format!("{tag}.text.txt")).ok().filter(|t| !t.trim().is_empty());
        let Some(text) = text else {
            std::fs::write(format!("{tag}.prompt.txt"), &prompt).unwrap();
            std::fs::write(format!("{tag}.facts.txt"), facts(&st).join("\n")).unwrap();
            println!("NEXT {tag}.prompt.txt {tag}.text.txt");
            return;
        };
        std::fs::write(format!("{tag}.prompt.txt"), &prompt).unwrap();
        engine13::llm::record_chronicle(st.world_state.as_mut().unwrap(), &stamp, Some(text.clone()));
        let f = facts(&st);
        std::fs::write(format!("{tag}.facts.txt"), f.join("\n")).unwrap();
        let _ = write!(book_md, "\n---\n\n## tick {} — {} год, {}\n\n### facts\n\n{}\n\n### chronicle\n\n{}\n",
            stamp.tick, stamp.year, stamp.half_year, f.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n"), text);
    }
    std::fs::write(format!("{outdir}/{world}_s{seed}.md"), book_md).unwrap();
    println!("DONE");
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("scan") => scan(&a[2], a[3].parse().unwrap()),
        Some("check") => check(&a[2], &a[3]),
        Some("step") => step(&a[2], a[3].parse().unwrap(), a[4].parse().unwrap(), a[5].parse().unwrap(), &a[6]),
        Some(m @ ("live" | "dry")) => chronicles(&a[2], a[3].parse().unwrap(), a[4].parse().unwrap(), a[5].parse().unwrap(), &a[6], m == "live"),
        _ => eprintln!("usage: chronicle_probe scan|live|dry|check …"),
    }
}
