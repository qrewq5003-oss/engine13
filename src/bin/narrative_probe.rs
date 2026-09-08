//! Narrative probe — задача 31.
//!
//! Инструмент ОЦЕНКИ, не правка. Он не трогает движок и не меняет контент:
//! он воспроизводит ровно тот путь, по которому игрок получает текст в UI:
//!
//!     advance tick  ->  build_snapshot(world, scenario, event_log)
//!                   ->  generate_narrative_prompt(snapshot, scenario, db, memory)
//!                   ->  POST /v1/chat/completions
//!
//! (см. `application::narrative::cmd_get_narrative` и `App.tsx::handleAdvanceTick`,
//! который зовёт `refreshNarrative` КАЖДЫЙ тик).
//!
//! Режимы:
//!   dry    — полная партия, собрать промпты по всем тикам, без обращения к LLM
//!   live   — то же + сгенерировать текст на лестнице сэмплов (пары соседних полугодий)
//!   window — равновесный расчёт окна событий, план (A) (§14)
//!   deaths — критерий плана (B) (§15): блок «события периода» против настоящих
//!            гибелей и рождений, с контрфактом прежнего правила на той же партии
//!   context — критерий плана (F) (§17): привязанные ко времени утверждения премисы
//!            против состоявшихся фактов мира, плюс замер даты хроники (§15.8 п.3)
//!
//! Usage:
//!   cargo run --release --bin narrative_probe -- <scenario> <ticks> <strategy> <seed> <dry|live> [outdir]

use engine13::{
    application::actions::{apply_player_action, PlayerActionInput},
    commands::AppState,
    core::WorldState,
    engine::{tick, EventLog},
    scenarios::registry,
};
use rand::SeedableRng;

// ---------------------------------------------------------------------------
// Scripted strategies — копия приоритетов из src/bin/sim.rs, чтобы промпт
// строился на партии, в которой игрок реально что-то делает.
// ---------------------------------------------------------------------------
fn priority_actions(scenario_id: &str, strategy: &str) -> Vec<&'static str> {
    match (scenario_id, strategy) {
        ("constantinople_1430", "diplomacy") => vec![
            "venice_diplomacy", "genoa_financial_aid", "milan_bankers",
            "venice_trade_deal", "genoa_galata_garrison", "venice_naval_support",
            "genoa_mercenaries", "milan_condottieri",
        ],
        ("constantinople_1430", "military") => vec![
            "venice_naval_support", "genoa_mercenaries", "milan_condottieri",
            "genoa_galata_garrison", "venice_diplomacy", "genoa_financial_aid",
            "milan_bankers", "venice_trade_deal",
        ],
        ("constantinople_1430", _) => vec![
            "venice_diplomacy", "genoa_financial_aid", "milan_bankers",
            "venice_naval_support", "genoa_mercenaries", "milan_condottieri",
            "venice_trade_deal", "genoa_galata_garrison",
        ],
        ("rome_375", "influence") => vec![
            "build_reputation", "expand_network", "back_administration",
            "gather_information", "invest_wealth", "lay_low",
        ],
        ("rome_375", "wealth") => vec![
            "invest_wealth", "gather_information", "expand_network",
            "build_reputation", "back_administration", "lay_low",
        ],
        ("rome_375", _) => vec![
            "build_reputation", "invest_wealth", "expand_network",
            "gather_information", "back_administration", "lay_low",
        ],
        ("milan_1477", _) => vec![
            "milan_raise_troops", "milan_marriage_alliance", "milan_papal_favor",
            "milan_hire_condottieri", "milan_tax_reform", "milan_patronage",
        ],
        _ => vec![],
    }
}

struct TurnRecord {
    tick: u32,
    year: i32,
    half_year: String,
    victory: bool,
    prompt: String,
    actions: Vec<String>,
    foreground: Vec<String>,
    dead: Vec<String>,
    events_shown: Vec<String>,
    narrative: Option<String>,
    /// Режим `deaths` (план (B)): блок «=== СОБЫТИЯ ЭТОГО ПЕРИОДА ===» как он ушёл
    /// в промпт, построчно и дословно. Читается из самого промпта, а не из снапшота:
    /// критерий (B) сформулирован по тому, что видит модель.
    period_block: Vec<String>,
    /// Акторы, погибшие ИМЕННО в этом полугодии — истина, с которой сверяется блок.
    /// Считается по `world.dead_actors`, а не по снапшоту нарратива.
    deaths_now: Vec<String>,
    /// Их человеческие имена (снятые ДО тика, пока актор ещё жив).
    deaths_names: Vec<String>,
    /// Акторы, появившиеся в мире ИМЕННО в этом полугодии (наследники и спавны) —
    /// вторая половина критерия (B). Считается диффом ключей `world.actors`, то есть
    /// независимо от того, логирует ли движок это появление.
    births_now: Vec<String>,
    births_names: Vec<String>,
    /// Появления, объявленные спавном (`spawn_*`) — у них свой канал в промпте.
    spawned_now: Vec<String>,
    /// Событий `EventType::Birth` в логе за это полугодие: критерий «ровно одно
    /// событие на каждое рождение наследника» считается по ним.
    birth_events_now: Vec<String>,
    /// Контрфакт: что напечатало бы ПРЕЖНЕЕ правило (`EventType::Collapse`, весь лог).
    base_block: Vec<String>,
    /// Наследники, объявленные в `on_collapse` погибшего, но НЕ появившиеся в мире:
    /// у них нет шаблона в `scenario.actors`, и движок молча их не создаёт.
    phantom_heirs: Vec<String>,
}

/// Вырезать блок «события этого периода» из готового промпта.
fn extract_period_block(prompt: &str) -> Vec<String> {
    const HEAD: &str = "=== СОБЫТИЯ ЭТОГО ПЕРИОДА ===";
    let Some(start) = prompt.find(HEAD) else { return Vec::new() };
    prompt[start + HEAD.len()..]
        .lines()
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let scenario_id = a.get(1).cloned().unwrap_or_else(|| "rome_375".into());
    let ticks: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(150);
    let strategy = a.get(3).cloned().unwrap_or_else(|| "balanced".into());
    let seed: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(42);
    let mode = a.get(5).cloned().unwrap_or_else(|| "dry".into());
    let outdir = a.get(6).cloned().unwrap_or_else(|| "/tmp/narrative_probe".into());

    std::fs::create_dir_all(&outdir).expect("mkdir outdir");

    let scenario = registry::load_by_id(&scenario_id).expect("Unknown scenario");
    let db = engine13::db::Db::open_in_memory().expect("in-memory db");

    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario
            .generation_mechanics
            .as_ref()
            .map(|g| g.patriarch_start_age)
            .unwrap_or(40) as u32;
        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
        });
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let mut state = AppState {
        world_state: Some(world),
        event_log: EventLog::new(),
        current_scenario: Some(scenario.clone()),
        rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
        narrative_memory: engine13::llm::NarrativeMemory::default(),
    };

    let prio = priority_actions(&scenario_id, &strategy);
    let mut records: Vec<TurnRecord> = Vec::new();
    let mut victory_tick: Option<u32> = None;

    for tick_num in 0..ticks {
        // --- player actions, тем же путём, что и UI ---
        let mut applied = 0u32;
        let mut actions_applied: Vec<String> = Vec::new();

        if scenario_id == "milan_1477" {
            const GATE: f64 = 70.0;
            let treasury = state.world_state.as_ref().unwrap()
                .actors.get("milan").map(|a| a.get_metric("treasury")).unwrap_or(0.0);
            if treasury > GATE {
                let input = PlayerActionInput { action_id: "milan_raise_troops".into(), target_actor_id: None };
                if apply_player_action(&mut state, &input).is_ok() {
                    applied += 1;
                    actions_applied.push("milan_raise_troops".into());
                }
            }
            for id in prio.iter().filter(|i| **i != "milan_raise_troops") {
                if applied >= scenario.actions_per_tick { break; }
                let now = state.world_state.as_ref().unwrap()
                    .actors.get("milan").map(|a| a.get_metric("treasury")).unwrap_or(0.0);
                if now - GATE <= 0.0 { break; }
                let input = PlayerActionInput { action_id: id.to_string(), target_actor_id: None };
                if apply_player_action(&mut state, &input).is_ok() {
                    applied += 1;
                    actions_applied.push(id.to_string());
                }
            }
        } else {
            for id in &prio {
                if applied >= scenario.actions_per_tick { break; }
                let input = PlayerActionInput { action_id: id.to_string(), target_actor_id: None };
                if apply_player_action(&mut state, &input).is_ok() {
                    applied += 1;
                    actions_applied.push(id.to_string());
                }
            }
        }

        // Мир ДО тика: по ключам считается рождение (наследники движок не логирует),
        // по именам — сверка блока. Имя берётся ЗДЕСЬ, а не из `scenario.actors`:
        // спавненные акторы (`wallachia`, `poland_lithuania`, `mamluks`, `france`)
        // в сценарии не описаны, их имя приходит из `SpawnActorConfig.label`, и
        // первая версия этой сверки объявила три верных срабатывания ложными
        // ровно потому, что искала их в сценарии.
        let names_before: std::collections::HashMap<String, String> = state
            .world_state.as_ref().unwrap().actors.iter()
            .map(|(id, a)| (id.clone(), a.name.clone()))
            .collect();
        let actors_before: std::collections::HashSet<String> =
            names_before.keys().cloned().collect();

        // --- advance tick ---
        {
            let ws = state.world_state.as_mut().unwrap();
            let sc = state.current_scenario.as_ref().unwrap();
            let rng = state.rng.as_mut().unwrap();
            tick(ws, sc, &mut state.event_log, rng);
        }

        // --- ровно то, что делает cmd_get_narrative ---
        let ws = state.world_state.as_ref().unwrap();
        let snapshot = engine13::llm::build_snapshot(ws, &scenario, &state.event_log);
        let prompt = engine13::llm::generate_narrative_prompt(
            &snapshot, &scenario, &db, &state.narrative_memory,
        );

        if snapshot.victory_achieved && victory_tick.is_none() {
            victory_tick = Some(tick_num);
        }

        // Истина о жизни и смерти этого полугодия — из движка, не из нарратива.
        // NB: `phase_advance` увеличивает `world.tick` в конце тика, поэтому гибель,
        // случившаяся в только что отработавшем полугодии, помечена `tick_death`
        // на единицу меньше, чем `ws.tick` в момент снятия снапшота.
        let mut deaths_now: Vec<String> = ws.dead_actors.iter()
            .filter(|d| d.tick_death + 1 == ws.tick)
            .map(|d| d.id.clone())
            .collect();
        deaths_now.sort();
        let deaths_names: Vec<String> = deaths_now.iter()
            .map(|id| names_before.get(id).cloned().unwrap_or_default())
            .collect();
        let mut births_now: Vec<String> = ws.actors.keys()
            .filter(|id| !actors_before.contains(*id))
            .cloned()
            .collect();
        births_now.sort();
        let births_names: Vec<String> = births_now.iter()
            .map(|id| ws.actors.get(id).map(|a| a.name.clone()).unwrap_or_default())
            .collect();
        let period_tick = ws.tick.saturating_sub(1);
        let mut spawned_now: Vec<String> = state.event_log.events.iter()
            .filter(|e| e.tick == period_tick && e.id.starts_with("spawn_"))
            .map(|e| e.actor_id.clone())
            .collect();
        spawned_now.sort();
        let mut birth_events_now: Vec<String> = state.event_log.events.iter()
            .filter(|e| e.tick == period_tick && e.event_type == engine13::core::EventType::Birth)
            .map(|e| e.actor_id.clone())
            .collect();
        birth_events_now.sort();
        let mut phantom_heirs: Vec<String> = ws.dead_actors.iter()
            .filter(|d| d.tick_death + 1 == ws.tick)
            .flat_map(|d| d.successor_ids.iter())
            .filter(|s| !ws.actors.contains_key(&s.id))
            .map(|s| s.id.clone())
            .collect();
        phantom_heirs.sort();
        let period_block = extract_period_block(&prompt);

        // Контрфакт: что напечатало бы ПРЕЖНЕЕ правило на этом же тике этой же партии.
        // Дословно прежний код `build_snapshot`: весь лог, фильтр `EventType::Collapse`,
        // дедупликация по `actor_id`, строка `{actor_id} прекратил существование.`
        // Считается здесь, а не вторым прогоном старого бинарника: правка не трогает
        // ни один вход этого правила (`Collapse` пишется в одном месте и не менялся),
        // поэтому расчёт по текущему логу равен измерению по прежнему коду — и это
        // проверено: на seed 42 контрфакт обязан дать 64 (const/diplomacy) и 74 (milan),
        // то есть ровно те 138 из 1050, что измерены прежним бинарником.
        let mut seen_collapse: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut base_block: Vec<String> = Vec::new();
        for e in &state.event_log.events {
            if e.event_type == engine13::core::EventType::Collapse
                && seen_collapse.insert(e.actor_id.clone())
            {
                base_block.push(format!("{} прекратил существование.", e.actor_id));
            }
        }

        records.push(TurnRecord {
            tick: tick_num,
            year: snapshot.year,
            half_year: format!("{:?}", snapshot.half_year),
            victory: snapshot.victory_achieved,
            prompt,
            actions: actions_applied,
            foreground: snapshot.foreground_actors.clone(),
            dead: snapshot.dead_actors.clone(),
            events_shown: snapshot.recent_important_events.iter().take(5)
                .map(|e| format!("{}@t{}", e.id, e.tick)).collect(),
            narrative: None,
            period_block,
            deaths_now,
            deaths_names,
            births_now,
            births_names,
            spawned_now,
            birth_events_now,
            base_block,
            phantom_heirs,
        });
    }

    // ------------------------------------------------------------------
    // Режим `context` — критерий плана (F), считается по логу, без LLM.
    //
    // Меряет не «стало ли лучше на вид», а два проверяемых факта:
    //   1. АНАХРОНИЗМ: промпт утверждает X, тогда как ретракция X уже является
    //      состоявшимся фактом мира (сработала веха, которая её и написала).
    //   2. САМОПРОТИВОРЕЧИЕ: в ОДНОМ И ТОМ ЖЕ промпте лежит и утверждение X,
    //      и текст его ретракции — то есть промпт спорит сам с собой.
    // Плюс расхождение двух дат: замороженной внутри премисы и живой в блоке
    // состояния мира.
    // ------------------------------------------------------------------
    if mode == "context" {
        // Перепись «привязанное ко времени утверждение → событие мира, которое его
        // отменяет». Составлена чтением контента, не угадана: для вех текст ретракции —
        // это их собственный `llm_context_shift`, написанный автором ровно как
        // опровержение соответствующей строки премисы.
        //
        // ВАЖНО про третье поле. Это фрагмент, по которому проверяется, что ретракция
        // лежит в ТОМ ЖЕ промпте, и он обязан отсутствовать в самой премисе — иначе
        // предикат самопротиворечия срабатывает на премисе и меряет сам себя. Первая
        // версия взяла фрагмент «Адрианополь», который стоит в премисе, и получила
        // 126 из 126 самопротиворечий там, где их могло не быть ни одного.
        enum Retracted { Milestone(&'static str), Death(&'static str) }
        use Retracted::*;
        let claims: Vec<(&str, Retracted, &str)> = match scenario_id.as_str() {
            // (фрагмент премисы, что его отменяет, фрагмент ретракции — НЕ из премисы)
            "rome_375" => vec![
                ("Через три года Адрианополь. Но это ещё не случилось.",
                 Milestone("adrianople"), "Валент мёртв"),
                ("Гунны в 375 году — слухи на краю ойкумены, не факт.",
                 Milestone("huns_visible"), "Гунны больше не слухи"),
                ("Гунны в 375 году невидимы для большинства",
                 Milestone("huns_visible"), "Гунны больше не слухи"),
            ],
            "milan_1477" => vec![
                ("Франция ещё не вошла в игру, но уже\nприсматривается.",
                 Milestone("france_intervenes"), "Французская корона обращает взор"),
            ],
            // У constantinople премиса написана прошедшим временем и стареет лучше —
            // кроме одной строки, которую отменяет не веха, а сама гибель актора.
            "constantinople_1430" => vec![
                ("Константинополь стоит — но ненадолго.",
                 Death("byzantium"), "Византийская Империя (пала"),
                ("- Не предрешать падение города",
                 Death("byzantium"), "Византийская Империя (пала"),
                ("Мехмед не пассивен.",
                 Death("ottomans"), "Османская Империя (пала"),
            ],
            _ => vec![],
        };

        // Год, замороженный в тексте премисы (первое число вида «NNNN год»).
        let frozen_year: i32 = scenario.llm_context.split_whitespace()
            .zip(scenario.llm_context.split_whitespace().skip(1))
            .find(|(_, w)| w.starts_with("год"))
            .and_then(|(y, _)| y.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok())
            .unwrap_or(scenario.start_year);

        // Тик, на котором ретракция стала фактом мира — по логу движка, не по календарю.
        let fired_at = |r: &Retracted| -> Option<(String, u32)> {
            match r {
                Milestone(mid) => state.event_log.events.iter()
                    .find(|e| e.id == *mid)
                    .map(|e| (format!("веха `{}`", mid), e.tick)),
                Death(aid) => state.world_state.as_ref().unwrap().dead_actors.iter()
                    .find(|d| d.id == *aid)
                    .map(|d| (format!("гибель `{}`", aid), d.tick_death)),
            }
        };

        println!("=== КРИТЕРИЙ (F): {} / {} / seed {} / {} тиков ===", scenario_id, strategy, seed, ticks);
        println!("замороженный год в премисе: {}, стартовый год сценария: {}", frozen_year, scenario.start_year);

        let mut anach_total = 0usize;
        let mut contra_total = 0usize;
        let mut unretracted_total = 0usize;
        for (claim, trigger, retraction) in &claims {
            let short: String = claim.chars().take(46).collect();
            let Some((what, t0)) = fired_at(trigger) else {
                println!("  «{}…»\n    ретракция за партию НЕ наступила — анахронизма нет", short);
                continue;
            };
            // Анахронизм: полугодия ПОСЛЕ того, как ретракция стала фактом мира,
            // в промпте которых утверждение всё ещё стоит.
            let anach = records.iter()
                .filter(|r| r.tick > t0 && r.prompt.contains(claim))
                .count();
            // Самопротиворечие: те из них, где рядом, в том же промпте, лежит и
            // текст ретракции.
            let contra = records.iter()
                .filter(|r| r.tick > t0 && r.prompt.contains(claim) && r.prompt.contains(retraction))
                .count();
            // Доходит ли текст ретракции до промпта ВООБЩЕ — независимо от того,
            // стоит ли рядом опровергаемое утверждение. Это решающее число для
            // развилки: оно отвечает, достаточно ли уже сделанного окна событий (A).
            let reach = records.iter()
                .filter(|r| r.tick > t0 && r.prompt.contains(retraction))
                .count();
            // КРИТЕРИЙ: полугодия, где утверждение стоит, ретракция уже факт мира,
            // а текста ретракции в промпте НЕТ. Это и есть неатрибутированная
            // анахроничная ложь — модель не может её распознать, потому что в
            // промпте нет ничего, что бы её опровергало.
            let unretracted = records.iter()
                .filter(|r| r.tick > t0 && r.prompt.contains(claim) && !r.prompt.contains(retraction))
                .count();
            unretracted_total += unretracted;
            anach_total += anach;
            contra_total += contra;
            let rest = records.len() as u32 - t0 - 1;
            println!("  «{}…»", short);
            // Год — движковый (start_year + tick/2), а НЕ `records[t0].year`:
            // последний берётся из снапшота, датированного на полугодие вперёд
            // (§15.8 п.3), и печатал бы 387 там, где движок записал 386.
            println!("    отменена: {} на тике {} (год {})", what, t0,
                scenario.start_year + (t0 / 2) as i32);
            println!("    анахроничных полугодий:  {} из {} оставшихся", anach, rest);
            println!("    самопротиворечивых:      {}", contra);
            println!("    текст ретракции вообще в промпте: {} из {}", reach, rest);
            // Канал ретракции печатается явно: `веха` — то, что чинит (F);
            // `гибель` — то, что уже починил (B) через постоянный список павших.
            // Без этого разделения эффект (F) неотличим от унаследованного.
            println!("    НЕОПРОВЕРГНУТЫХ (критерий):       {} из {}  [канал: {}]",
                unretracted, rest,
                match trigger { Milestone(_) => "веха", Death(_) => "гибель" });
        }
        // Замороженная дата против живой.
        let gaps: Vec<i32> = records.iter()
            .filter(|r| r.prompt.contains(&format!("{} год", frozen_year)))
            .map(|r| r.year - frozen_year)
            .collect();
        let max_gap = gaps.iter().copied().max().unwrap_or(0);
        println!("промптов, где премиса называет {} год:  {} из {}", frozen_year, gaps.len(), records.len());
        println!("  максимальный разрыв с живой датой:    {} лет", max_gap);

        // Вторая, независимая рассинхронизация: шапка промпта против полугодия,
        // чьи события в нём лежат (§15.8 п.3). Считается отдельно ИМЕННО чтобы
        // показать, что это другой дефект: движковый год тика T равен
        // start_year + T/2, а снапшот берёт world.year уже после phase_advance.
        let head_ahead = records.iter()
            .filter(|r| r.year != scenario.start_year + (r.tick / 2) as i32)
            .count();
        println!("промптов, где шапка датирована ПОЗЖЕ своего полугодия: {} из {} (§15.8 п.3)",
            head_ahead, records.len());

        println!("АНАХРОНИЧНЫХ ПОЛУГОДИЙ ВСЕГО:          {}", anach_total);
        println!("  из них с ретракцией в том же промпте: {}", contra_total);
        println!("НЕОПРОВЕРГНУТЫХ ВСЕГО (КРИТЕРИЙ):      {}", unretracted_total);
        return;
    }

    // ------------------------------------------------------------------
    // Режим `deaths` — критерий плана (B), считается по логу, без LLM.
    //
    // Печатает три величины и построчную выкладку каждого срабатывания блока
    // «события этого периода», чтобы «верно» и «неверно» можно было проверить
    // глазами, а не принимать на слово агрегата.
    // ------------------------------------------------------------------
    if mode == "deaths" {
        let fired: Vec<&TurnRecord> = records.iter().filter(|r| !r.period_block.is_empty()).collect();
        let death_ticks: Vec<&TurnRecord> = records.iter().filter(|r| !r.deaths_now.is_empty()).collect();
        let birth_ticks: Vec<&TurnRecord> = records.iter().filter(|r| !r.births_now.is_empty()).collect();

        // Срабатывание «верно», если блок сработал ровно в полугодие перехода
        // (гибель или рождение наследника) и назвал ровно всех участников этого
        // перехода. Сверка по вхождению имени в строку блока; имя берётся из мира,
        // а не из сценария (спавненные акторы в сценарии не описаны).
        let mut correct = 0usize;
        let mut spurious = 0usize;
        let mut missed = 0usize;
        // Рождение наследника — появление, НЕ объявленное спавном: у спавнов свой
        // канал (`spawn_*` уходит в окно событий), дублировать его в блоке нельзя.
        let heirs_of = |r: &TurnRecord| -> Vec<(String, String)> {
            r.births_now.iter().zip(r.births_names.iter())
                .filter(|(id, _)| !r.spawned_now.contains(id))
                .map(|(i, n)| (i.clone(), n.clone()))
                .collect()
        };
        for r in &records {
            let block_nonempty = !r.period_block.is_empty();
            let heirs = heirs_of(r);
            let transition = !r.deaths_now.is_empty() || !heirs.is_empty();
            let named = |id: &str, name: &str| {
                r.period_block.iter().any(|l| l.contains(id) || (!name.is_empty() && l.contains(name)))
            };
            if block_nonempty && transition {
                let all_named = r.deaths_now.iter().zip(r.deaths_names.iter())
                    .all(|(id, n)| named(id, n))
                    && heirs.iter().all(|(id, n)| named(id, n));
                if all_named { correct += 1; } else { spurious += 1; }
            } else if block_nonempty {
                spurious += 1;
            } else if transition {
                missed += 1;
            }
        }
        // Критерий 3: ровно одно событие `EventType::Birth` на каждое рождение наследника.
        let heir_births: usize = records.iter().map(|r| heirs_of(r).len()).sum();
        let birth_events: usize = records.iter().map(|r| r.birth_events_now.len()).sum();
        let birth_mismatch = records.iter()
            .filter(|r| {
                let mut h: Vec<String> = heirs_of(r).into_iter().map(|(i, _)| i).collect();
                h.sort();
                h != r.birth_events_now
            })
            .count();
        let scenario_lines = records.iter()
            .flat_map(|r| r.period_block.iter())
            .filter(|l| l.contains("scenario"))
            .count();
        let prompts_with_scenario = records.iter()
            .filter(|r| r.prompt.contains("scenario"))
            .count();

        let base_fired = records.iter().filter(|r| !r.base_block.is_empty()).count();
        let base_scenario_lines: usize = records.iter()
            .flat_map(|r| r.base_block.iter())
            .filter(|l| l.contains("scenario"))
            .count();
        // Прежнее правило «верно» ровно тогда, когда напечатанный им actor_id
        // совпадает с тем, кто погиб в этом же полугодии.
        let base_correct = records.iter().filter(|r| {
            !r.base_block.is_empty()
                && !r.deaths_now.is_empty()
                && r.deaths_now.iter().all(|id| r.base_block.iter().any(|l| l.starts_with(&format!("{} ", id))))
        }).count();

        println!("=== КРИТЕРИЙ (B): {} / {} / seed {} / {} тиков ===", scenario_id, strategy, seed, ticks);
        println!("БАЗА (прежнее правило, контрфакт на этой же партии):");
        println!("  срабатываний {} / {}, верных {}, строк `scenario` {}",
            base_fired, records.len(), base_correct, base_scenario_lines);
        println!("СТАЛО:");
        println!("блок «события периода» сработал:            {} из {}", fired.len(), records.len());
        println!("  из них ВЕРНЫХ (ровно тик перехода):        {}", correct);
        println!("  из них ЛОЖНЫХ (перехода не было / не тот): {}", spurious);
        println!("полугодий с настоящей гибелью:              {}", death_ticks.len());
        println!("полугодий с рождением наследника:           {}", heir_births);
        println!("  переходов блоком ПРОПУЩЕНО:               {}", missed);
        println!("событий EventType::Birth в логе:            {} (расхождений по полугодиям: {})",
            birth_events, birth_mismatch);
        let phantoms: Vec<String> = records.iter()
            .flat_map(|r| r.phantom_heirs.iter().cloned()).collect();
        println!("объявленных, но НЕ созданных наследников:   {} {:?}", phantoms.len(), phantoms);
        println!("появлений всего (наследники + спавны):      {}", birth_ticks.len());
        println!("строк со словом `scenario` в блоке:         {}", scenario_lines);
        println!("промптов, где слово `scenario` встречается: {}", prompts_with_scenario);
        println!();
        println!("настоящие гибели: {:?}", death_ticks.iter()
            .map(|r| format!("t{}:{}", r.tick, r.deaths_now.join("+"))).collect::<Vec<_>>());
        println!("появления: {:?}", birth_ticks.iter()
            .map(|r| format!("t{}:{}{}", r.tick, r.births_now.join("+"),
                if r.spawned_now.is_empty() { "" } else { "(спавн)" })).collect::<Vec<_>>());
        println!();
        println!("выкладка ВСЕХ срабатываний блока:");
        for r in &fired {
            println!("  t{:<4} гибель={:?} рождение={:?}", r.tick, r.deaths_now, r.births_now);
            for l in &r.period_block { println!("         | {}", l); }
        }
        return;
    }

    // ------------------------------------------------------------------
    // Режим `window` — равновесный расчёт для плана (A) ДО правки продукта.
    //
    // Считает, что показал бы летописцу каждый вариант отбора на ОДНОЙ и той же
    // партии, и ничего в продукте не меняет. Варианты:
    //   base — то, что делает `build_snapshot` сегодня: filter + truncate(10), take(5)
    //   V1   — канонический отбор `db::select_relevant_events`, query_tags = []
    //   V2   — он же, query_tags = tone_tags сценария (непустой запрос)
    // ------------------------------------------------------------------
    if mode == "window" {
        use std::collections::HashSet as HS;
        let mut base_sets: HS<String> = HS::new();
        let mut v1_sets: HS<String> = HS::new();
        let mut v2_sets: HS<String> = HS::new();
        let (mut base_t0, mut v1_t0, mut v2_t0) = (0usize, 0usize, 0usize);
        let mut tagged = 0usize;
        let mut scenario_key = 0usize;
        let mut total_events = 0usize;

        // Реплей ещё раз, чтобы иметь лог на каждом тике
        let mut w2 = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
        for actor in &scenario.actors {
            if !actor.is_successor_template {
                w2.actors.insert(actor.id.clone(), actor.clone());
            }
        }
        if let Some(ref im) = scenario.initial_family_metrics {
            let pa = scenario.generation_mechanics.as_ref().map(|g| g.patriarch_start_age).unwrap_or(40) as u32;
            w2.family_state = Some(engine13::core::FamilyState {
                metrics: engine13::core::normalize_family_metrics(im),
                patriarch_age: pa,
                generation_count: 0,
            });
        }
        w2.generation_mechanics = scenario.generation_mechanics.clone();
        w2.generation_length = scenario.generation_length;
        let mut st2 = AppState {
            world_state: Some(w2),
            event_log: EventLog::new(),
            current_scenario: Some(scenario.clone()),
            rng: Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed)),
            narrative_memory: engine13::llm::NarrativeMemory::default(),
        };
        // Непустой запрос как вариант отбора отпал: он делает релевантность нулевой
        // у ВСЕХ событий, а нужный эффект дала правка `thematic_similarity` (§14.4),
        // поэтому вариант V3 считается с пустым запросом, как и продукт.
        let _tone: Vec<String> = scenario.narrative_config.tone_tags.clone();
        // событие тика 0 — маркер «пересказывает самое старое»
        let mut tick0_ids: HS<String> = HS::new();
        let (mut seen_base, mut seen_v1, mut seen_v2): (HS<String>, HS<String>, HS<String>) =
            (HS::new(), HS::new(), HS::new());
        let (mut slots_v1, mut slots_v3, mut metric_slots_v1) = (0usize, 0usize, 0usize);

        for tick_num in 0..ticks {
            let mut applied = 0u32;
            for id in &prio {
                if applied >= scenario.actions_per_tick { break; }
                let input = PlayerActionInput { action_id: id.to_string(), target_actor_id: None };
                if apply_player_action(&mut st2, &input).is_ok() { applied += 1; }
            }
            {
                let ws = st2.world_state.as_mut().unwrap();
                let sc = st2.current_scenario.as_ref().unwrap();
                let rng = st2.rng.as_mut().unwrap();
                tick(ws, sc, &mut st2.event_log, rng);
            }
            let ws = st2.world_state.as_ref().unwrap();
            let mut fg: Vec<String> = ws.actors.values()
                .filter(|a| a.narrative_status == engine13::core::NarrativeStatus::Foreground)
                .map(|a| a.id.clone()).collect();
            fg.sort();

            // base
            let mut base: Vec<engine13::core::Event> = st2.event_log.events.iter()
                .filter(|e| e.is_key || fg.contains(&e.actor_id)).cloned().collect();
            base.truncate(10);
            let base_ids: Vec<String> = base.iter().take(5).map(|e| e.id.clone()).collect();

            // Кандидаты нормализуются так же, как в build_snapshot
            let mut cand: Vec<engine13::core::Event> = st2.event_log.events.clone();
            cand.sort_by(|a, b| a.tick.cmp(&b.tick).then(a.id.cmp(&b.id)));

            let v1 = engine13::db::select_relevant_events(&cand, ws.tick, &[], &fg);
            let v1_ids: Vec<String> = v1.iter().take(5).map(|e| e.id.clone()).collect();
            // V3 — контрфакт: тот же канонический отбор, но потиковые дампы metrics_*
            // исключены из КАНДИДАТОВ. Оценка объёма правки кормильца, продукт не меняется.
            let cand3: Vec<engine13::core::Event> = cand.iter()
                .filter(|e| !e.id.starts_with("metrics_"))
                .cloned().collect();
            let v2 = engine13::db::select_relevant_events(&cand3, ws.tick, &[], &fg);
            let v2_ids: Vec<String> = v2.iter().take(5).map(|e| e.id.clone()).collect();
            slots_v1 += v1_ids.len();
            slots_v3 += v2_ids.len();
            metric_slots_v1 += v1_ids.iter().filter(|i| i.starts_with("metrics_")).count();

            if tick_num == 0 {
                tick0_ids = base_ids.iter().cloned().collect();
                total_events = st2.event_log.events.len();
                tagged = st2.event_log.events.iter().filter(|e| !e.tags.is_empty()).count();
                scenario_key = st2.event_log.events.iter().filter(|e| e.is_key && e.actor_id == "scenario").count();
            }
            for i in &base_ids { seen_base.insert(i.clone()); }
            for i in &v1_ids { seen_v1.insert(i.clone()); }
            for i in &v2_ids { seen_v2.insert(i.clone()); }
            base_sets.insert(base_ids.join("|"));
            v1_sets.insert(v1_ids.join("|"));
            v2_sets.insert(v2_ids.join("|"));
            if base_ids.iter().any(|i| tick0_ids.contains(i)) { base_t0 += 1; }
            if v1_ids.iter().any(|i| tick0_ids.contains(i)) { v1_t0 += 1; }
            if v2_ids.iter().any(|i| tick0_ids.contains(i)) { v2_t0 += 1; }
        }
        let n = ticks as f64;
        let ws = st2.world_state.as_ref().unwrap();
        let _ = ws;
        println!("=== РАВНОВЕСНЫЙ РАСЧЁТ (A): {} / {} / seed {} / {} тиков ===", scenario_id, strategy, seed, ticks);
        // Плотность и перепись id — измерения, отвечающие на вопрос «почему у сценария
        // столько различных наборов», без обращения к LLM.
        let nonmetric = st2.event_log.events.iter().filter(|e| !e.id.starts_with("metrics_")).count();
        {
            use std::collections::HashMap as HM;
            let mut per: HM<u32, usize> = HM::new();
            for e in st2.event_log.events.iter().filter(|e| !e.id.starts_with("metrics_")) {
                *per.entry(e.tick).or_insert(0) += 1;
            }
            let empty = (0..ticks).filter(|t| !per.contains_key(t)).count();
            {
                use std::collections::HashMap as HM2;
                let mut freq: HM2<String, usize> = HM2::new();
                for e in st2.event_log.events.iter().filter(|e| !e.id.starts_with("metrics_")) {
                    *freq.entry(e.id.clone()).or_insert(0) += 1;
                }
                let mut v: Vec<(String, usize)> = freq.into_iter().collect();
                v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let uniq = v.len();
                let top: Vec<String> = v.iter().take(5).map(|(i, c)| format!("{}×{}", i, c)).collect();
                println!("различных id среди не-metrics: {} | самые частые: {}", uniq, top.join(", "));
            }
            // Построчная выкладка окрестности одного окна — ею установлена причина
            // по milan (§14.6): повторяющиеся id занимают слоты, потому что отбор
            // дедуплицирует по id, а id у большинства событий не уникальны.
            if std::env::var("DUMP_TICKS").is_ok() {
                for t in 28..36u32 {
                    let ids: Vec<String> = st2.event_log.events.iter()
                        .filter(|e| e.tick == t && !e.id.starts_with("metrics_"))
                        .map(|e| format!("{}(key={})", e.id, e.is_key)).collect();
                    println!("   tick {}: {:?}", t, ids);
                }
            }
            let mut counts: Vec<usize> = (0..ticks).map(|t| *per.get(&t).unwrap_or(&0)).collect();
            counts.sort_unstable();
            println!("полугодий БЕЗ единого не-metrics события: {} из {} ({:.0}%), медиана событий на полугодие: {}",
                empty, ticks, empty as f64 / ticks as f64 * 100.0, counts[counts.len()/2]);
        }
        println!("событий в логе к концу партии: {} (из них не-metrics: {} = {:.1} на полугодие)",
            st2.event_log.events.len(), nonmetric, nonmetric as f64 / ticks as f64);
        println!("  из них с непустыми tags:      {}", st2.event_log.events.iter().filter(|e| !e.tags.is_empty()).count());
        println!("  is_key с actor_id=\"scenario\": {}", st2.event_log.events.iter().filter(|e| e.is_key && e.actor_id == "scenario").count());
        println!("  (на тике 0 было: всего {}, с tags {}, scenario-key {})", total_events, tagged, scenario_key);
        println!();
        println!("{:<6} {:>12} {:>26}", "вариант", "разл.наборов", "доля тиков с событием т.0");
        println!("{:<6} {:>12} {:>25.0}%", "base", base_sets.len(), base_t0 as f64 / n * 100.0);
        println!("{:<6} {:>12} {:>25.0}%", "V1", v1_sets.len(), v1_t0 as f64 / n * 100.0);
        println!("{:<6} {:>12} {:>25.0}%", "V3", v2_sets.len(), v2_t0 as f64 / n * 100.0);
        println!();
        println!("слотов в промпте: V1 {} (из них metrics_* {} = {:.0}%), V3 {} (metrics_* исключены)",
            slots_v1, metric_slots_v1, metric_slots_v1 as f64 / slots_v1.max(1) as f64 * 100.0, slots_v3);
        println!();
        let kinds = |set: &HS<String>| -> String {
            let d = set.iter().filter(|i| i.starts_with("death_")).count();
            let p = set.iter().filter(|i| i.starts_with("player_action_")).count();
            let t = set.iter().filter(|i| i.starts_with("tag_spread_")).count();
            let o = set.len() - d - p - t;
            format!("{:>5} всего | смертей {:>2} | действий {:>3} | тегов {:>3} | прочих {:>3}", set.len(), d, p, t, o)
        };
        println!("различных id, дошедших до летописца за партию:");
        println!("  base  {}", kinds(&seen_base));
        println!("  V1    {}", kinds(&seen_v1));
        println!("  V3    {}", kinds(&seen_v2));
        return;
    }

    // ------------------------------------------------------------------
    // Model-free измерения по промптам (LLM не нужен)
    // ------------------------------------------------------------------
    let mut identical_pairs = 0usize;
    let mut diff_bytes: Vec<usize> = Vec::new();
    for w in records.windows(2) {
        if w[0].prompt == w[1].prompt { identical_pairs += 1; }
        diff_bytes.push(byte_diff(&w[0].prompt, &w[1].prompt));
    }
    let events_frozen_from = records.iter().position(|r| r.events_shown.len() >= 5);
    let distinct_event_sets: std::collections::HashSet<String> = records.iter()
        .map(|r| r.events_shown.join("|")).collect();

    println!("=== NARRATIVE PROBE: {} / {} / seed {} ===", scenario_id, strategy, seed);
    println!("ticks: {}", records.len());
    println!("victory tick: {:?}", victory_tick);
    println!("identical adjacent prompts: {}/{}", identical_pairs, records.len().saturating_sub(1));
    if !diff_bytes.is_empty() {
        let avg = diff_bytes.iter().sum::<usize>() as f64 / diff_bytes.len() as f64;
        let maxd = diff_bytes.iter().max().unwrap();
        let avg_len = records.iter().map(|r| r.prompt.len()).sum::<usize>() as f64 / records.len() as f64;
        println!("prompt len avg: {:.0} bytes", avg_len);
        println!("adjacent prompt diff: avg {:.1} bytes, max {} bytes ({:.2}% of prompt)",
            avg, maxd, avg / avg_len * 100.0);
    }
    println!("distinct 'НЕДАВНИЕ СОБЫТИЯ' sets over the whole game: {}", distinct_event_sets.len());
    println!("first tick with 5 events shown: {:?}", events_frozen_from);
    let last = records.last().unwrap();
    println!("last tick events shown: {:?}", last.events_shown);
    println!("first tick events shown: {:?}", records[0].events_shown);

    // ------------------------------------------------------------------
    // live: генерация текста на лестнице сэмплов (соседние пары)
    // ------------------------------------------------------------------
    if mode == "live" {
        let n = records.len() as u32;
        let mut samples: Vec<u32> = vec![0, 1, 5, 6, 20, 21];
        if n > 52 { samples.extend_from_slice(&[50, 51]); }
        if n > 102 { samples.extend_from_slice(&[100, 101]); }
        if let Some(vt) = victory_tick {
            if vt >= 1 { samples.push(vt - 1); }
            samples.push(vt);
        } else if n >= 2 {
            samples.push(n - 2);
            samples.push(n - 1);
        }
        samples.retain(|t| *t < n);
        samples.sort_unstable();
        samples.dedup();

        let cfg = engine13::llm::get_llm_config();
        eprintln!("[live] provider={} model={} samples={:?}", cfg.provider, cfg.model, samples);

        for t in &samples {
            let idx = *t as usize;
            let prompt = records[idx].prompt.clone();
            // Ретраи и сам запрос живут в `llm::generate_narrative_blocking` —
            // общий путь с `sim`'s narrative_eval / narrative_pack. Своей копии
            // запроса у пробника больше нет.
            match engine13::llm::generate_narrative_blocking(&prompt, &cfg, 5) {
                Ok(text) => {
                    eprintln!("[live] tick {} -> {} chars", t, text.chars().count());
                    records[idx].narrative = Some(text);
                }
                Err(e) => {
                    eprintln!("[live] tick {} FAILED: {}", t, e);
                    records[idx].narrative = Some(format!("[LLM ERROR] {}", e));
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // dump
    // ------------------------------------------------------------------
    let base = format!("{}/{}__{}__seed{}", outdir, scenario_id, strategy, seed);

    let mut md = String::new();
    md.push_str(&format!("# narrative probe: {} / {} / seed {}\n\n", scenario_id, strategy, seed));
    md.push_str(&format!("- ticks: {}\n- victory tick: {:?}\n- identical adjacent prompts: {}/{}\n\n",
        records.len(), victory_tick, identical_pairs, records.len().saturating_sub(1)));
    for r in &records {
        if r.narrative.is_none() { continue; }
        md.push_str(&format!("\n---\n\n## tick {} — {} год, {}\n\n", r.tick, r.year, r.half_year));
        md.push_str(&format!("- victory_achieved: {}\n", r.victory));
        md.push_str(&format!("- действия игрока в этом тике: {:?}\n", r.actions));
        md.push_str(&format!("- foreground: {:?}\n", r.foreground));
        md.push_str(&format!("- павшие: {:?}\n", r.dead));
        md.push_str(&format!("- события в промпте: {:?}\n\n", r.events_shown));
        md.push_str("### narrative\n\n");
        md.push_str(r.narrative.as_ref().unwrap());
        md.push('\n');
    }
    std::fs::write(format!("{}.narrative.md", base), md).expect("write md");

    // все промпты — для сверки и диффов
    let mut pd = String::new();
    for r in &records {
        pd.push_str(&format!("\n\n########## TICK {} (year {}, {}) ##########\n", r.tick, r.year, r.half_year));
        pd.push_str(&r.prompt);
    }
    std::fs::write(format!("{}.prompts.txt", base), pd).expect("write prompts");

    // машинно-читаемая сводка
    let mut csv = String::from("tick,year,half_year,victory,n_actions,n_foreground,n_dead,events\n");
    for r in &records {
        csv.push_str(&format!("{},{},{},{},{},{},{},\"{}\"\n",
            r.tick, r.year, r.half_year, r.victory, r.actions.len(),
            r.foreground.len(), r.dead.len(), r.events_shown.join("|")));
    }
    std::fs::write(format!("{}.turns.csv", base), csv).expect("write csv");

    println!("written: {}.narrative.md / .prompts.txt / .turns.csv", base);
}

/// грубая мера различия: число байт, не совпавших при позиционном сравнении
fn byte_diff(a: &str, b: &str) -> usize {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    let common = ab.len().min(bb.len());
    let mut d = ab.len().max(bb.len()) - common;
    for i in 0..common {
        if ab[i] != bb[i] { d += 1; }
    }
    d
}

