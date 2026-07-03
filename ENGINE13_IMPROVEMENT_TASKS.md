# ENGINE13 — Задания для Claude Code

Основано на прямом аудите репозитория (клонирован и прочитан полностью, не только README/архитектурный md). Задачи разбиты так, чтобы каждую можно было скормить Claude Code отдельно, в указанном порядке. Не объединяй задачи из разных секций в один проход — см. пояснение в конце.

Перед началом каждой задачи Claude Code должен прочитать `AGENTS.md` в корне репозитория — все задачи ниже сформулированы совместимо с его инвариантами.

**Модель по каждой задаче указана явно.** Правило простое: если задача трогает `src/engine/` (или общий load path, используемый всеми сценариями) — Opus. Если задача механическая (конфиги, gitignore, CI-yml, фронтенд-стилистика вне `engine/`) — Sonnet. Перед задачами, где меняется модель, и между независимо верифицируемыми задачами внутри одной секции — `/clear`.

---

## СЕКЦИЯ A — Гигиена репозитория

**Модель: Sonnet.**
**СТАТУС: ✅ ВЫПОЛНЕНО. Коммит `6f7c950`.**
- A1: `.gitignore` дополнен (`dist/`, `digest.txt`, `libpng16.pc`), `git rm --cached` на все три пути (39 файлов), файлы остались на диске физически. `git ls-files | grep -E "^dist/|digest.txt|libpng16.pc"` → пусто.
- A2: `rust-toolchain.toml` с `channel = "1.93.1"`, PATH-хак убран из `package.json` (скрипт `tauri` упрощён). `cargo --version` без ручного PATH резолвится в 1.93.1 через rustup — проверено.
- WIP-файлы (`App.tsx`, `db.rs`, `main.rs`, `save_load.rs`, `commands.rs` и др.) не тронуты.

### Задача A1: Очистить репозиторий от мусора и билд-артефактов

**Цель:** убрать из git файлы, которые не должны там быть.

**Что сделать:**
1. Добавить в `.gitignore`: `dist/`, `digest.txt`, `libpng16.pc`
2. Убрать эти пути из индекса git (`git rm --cached`, не удалять физически — файлы остаются на диске)
3. Проверить, что после `git status` рабочая директория чистая

**Не трогать:** ничего в `src/`, `src-tauri/src/`, конфиги сборки.

**Критерий приёмки:** `git ls-files | grep -E "^dist/|digest.txt|libpng16.pc"` — пусто.

---

### Задача A2: Зафиксировать версию Rust toolchain

**Цель:** устранить проблему из `MIGRATION.md` (конфликт rustc из apt и `~/.cargo/bin`) декларативно, а не через инструкции в markdown и PATH-хаки в package.json.

**Что сделать:**
1. Создать `rust-toolchain.toml` в корне репозитория с явной версией rustc (1.93.1 согласно MIGRATION.md)
2. Убрать хак `export PATH=/home/deck/.cargo/bin:$PATH` из `package.json` скрипта `tauri`
3. Оставить `MIGRATION.md` как есть — не трогать, это исторический документ

**Не трогать:** Cargo.toml зависимости, любой код.

**Критерий приёмки:** `cargo --version` в чистом окружении (без ручного PATH) резолвится в зафиксированную версию через rustup.

---

## СЕКЦИЯ B — Страховочная сетка

**Модель: Sonnet.**
**СТАТУС: ✅ ВЫПОЛНЕНО. Коммиты `ecfbc63`, `a9a024c`, `ee95fb2`.**
- B1: `.github/workflows/ci.yml` — job'ы rust (`cargo test --workspace`, `cargo clippy --workspace`, без `-D warnings` — см. D0) и frontend (`tsc --noEmit`, `npm run build`). Триггеры push/PR в main. Реально прогнан на GitHub Actions и зелёный (run `28661602558`).
  - По пути обнаружена и исправлена реальная проблема окружения раннера: `cargo test` падал без системных GTK/WebKit dev-заголовков (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`) — добавлены в `apt-get install` шаг (коммит `a9a024c`).
  - `-D warnings` на clippy снят (коммит `ee95fb2`) — 37 pre-existing clippy-находок, решено не чинить в рамках B1, см. СЕКЦИЮ D. `cargo test` остаётся строгим блокирующим.
- B2: `"test": "cargo test --workspace"` в `package.json`. 44/44 теста проходят.
- B3: `eslint.config.mjs` (flat config, ESLint 9 + typescript-eslint + eslint-plugin-react + eslint-plugin-react-hooks 5.2.0) и `.prettierrc`. `npm run lint` работает без ошибки конфигурации. Ни один существующий `.ts`/`.tsx` файл не тронут.

### Задача B1: Добавить CI

**Что сделать:** `.github/workflows/ci.yml` с шагами `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `tsc --noEmit`, `npm run build`. Триггер: push и pull_request в main.

**Не трогать:** сам код, тесты.

**Критерий приёмки:** workflow существует, синтаксически валиден, реально прогнан на GitHub Actions (не только синтаксическая проверка локально).

---

### Задача B2: Подключить тесты к npm

**Что сделать:** `"test": "cargo test --workspace"` в `package.json`, ничего больше не менять.

**Критерий приёмки:** `npm run test` из корня репозитория запускает Rust-тесты.

---

### Задача B3: Добавить ESLint + Prettier — ТОЛЬКО конфиг, без автофикса

**Что сделать:** минимальный flat config ESLint + `.prettierrc` под текущий стек, npm-скрипт `lint`. **НЕ запускать `--fix` по репозиторию**, не трогать ни один существующий `.tsx`/`.ts` файл.

**Критерий приёмки:** `npm run lint` работает без ошибки конфигурации. Diff — только новый конфиг.

---

## СЕКЦИЯ C — Ядро движка

**Модель: Opus.** Каждая задача — отдельный PR, отдельный `/clear`, обязательна ручная верификация через `cargo run --bin sim` и личное чтение diff на GitHub перед мержем.

**СТАТУС: ✅ ВЫПОЛНЕНО. Diff обоих PR прочитан лично, эквивалентность подтверждена построчно.**
- C1 — PR #1 (`fix/c1-interactions-unwrap`, коммит `d98cbf0`). 16 `.unwrap()` в `interactions.rs` заменены на `match { Some(a) => a, None => return }`.
- C2 — PR #2 (`fix/c2-mod-threshold-validation`, коммит `cbfc521`). Три `.expect()` в `apply_dependency_rule` заменены на `match rule.threshold { Some(t) if ... => ..., _ => 0.0 }` + load-time валидация в `validate_dependencies`.
- В процессе обнаружены и зафиксированы D3, D4 (см. ниже).

Эти задачи трогают `src/engine/`, защищённый правилами AGENTS.md #6 и #7 (collapse placement, tick order). Не объединять между собой и не объединять с A/B.

### Задача C1: Убрать риск паники в `engine/interactions.rs`

**Файл:** `src/engine/interactions.rs`. 16 вызовов `world.actors.get(id).unwrap()` — единая точка отказа: если будущая collapse-фаза удалит актора, чей id уже в очереди на взаимодействие в этом тике, `.unwrap()` уронит всю симуляцию.

**Что сделать:** заменить `.unwrap()` на `match world.actors.get(id) { Some(a) => a, None => return }`. Не менять сигнатуры, порядок вызовов, формулы взаимодействий.

**Обязательная верификация:**
```bash
cargo run --bin sim rome_375 50 batch 2>/dev/null
cargo run --bin sim rome_375 50 scripted balanced 2>/dev/null
cargo run --bin sim constantinople_1430 50 batch 2>/dev/null
cargo test --workspace
```
Учти: симуляция недетерминирована run-to-run (см. D3) — точное числовое совпадение невозможно, ориентир — структурные инварианты (victory tick) и диапазоны значений.

---

### Задача C2: Заменить паникующие `.expect()` в `engine/mod.rs` на валидацию сценария при загрузке

**Файл:** `src/engine/mod.rs`, `apply_dependency_rule` — три `rule.threshold.expect("threshold required for ...")`, паникующие каждый тик, если правило Deficit/Excess/Bonus создано без threshold.

**Что сделать:** валидация threshold — на этапе загрузки сценария (`load_dependencies` в каждом сценарии), с явной ошибкой, называющей сценарий/правило/режим. В `apply_dependency_rule` — `match rule.threshold { Some(t) if ... => ..., _ => 0.0 }` с SAFETY-комментарием, почему `None`-ветка недостижима для валидированного сценария.

**Обязательная верификация:** аналогично C1, плюс намеренно сломанный threshold в тестовом TOML — убедиться, что падает при загрузке, а не в середине тика.

---

## СЕКЦИЯ D — Технический долг

Обнаружено при первом реальном прогоне CI (после B1) и при верификации C1/C2. Не чинить попутно с другими секциями.

### D0: Решить, что делать с `-D warnings` на время разбора долга

**Модель: любая (решение, не код) — по факту уже решено.**
**СТАТУС: ✅ ВЫПОЛНЕНО.** Выбран вариант (a): `-D warnings` снят с clippy-шага в CI (коммит `ee95fb2`), `cargo test` остаётся строгим. Strict-режим clippy вернуть отдельным PR после разбора D1.

---

### D1: Классификация clippy/rustc-находок

**Модель: Sonnet.**
**СТАТУС: ✅ ВЫПОЛНЕНО.** Прогнан `cargo clippy --workspace` заново (после C1/C2) — находок 41, не 37 из первоначальной оценки B1 (код изменился). Включены и `clippy::*`, и rustc-линты (`unused_variables`, `dead_code`), поскольку D1 их не исключал. Ничего не закоммичено.

**Engine (18, `src/engine/`) — Opus, секция D1c:**
- `interactions.rs`: 7× `too_many_arguments` (строки 127, 311, 443, 516, 571, 660, 826/847), 2× `unused_variables` (`scenario` L850, `distance` L853)
- `mod.rs`: `unnecessary_cast` ×3 (L594, 1305, 1348), `for_kv_map` (L621), `single_match` (L897), `unwrap_or_default` (L1144), `manual_is_multiple_of` (L1299), `unnecessary_map_or` (L1316)

**Non-engine (23) — Sonnet, секция D1b:**
`application/actions.rs` (type_complexity, 2× collapsible_str_replace), `application/modes.rs` (needless_return), `application/narrative.rs` (type_complexity), `application/save_load.rs` (unused_variables: db), `core/actor.rs` (derivable_impls), `llm/mod.rs` (manual_is_multiple_of, manual_pattern_char_comparison, single_char_add_str, 2× manual_strip), `scenarios/rome_375.rs` + `scenarios/constantinople_1430.rs` (ptr_arg в `populate_actor_tags` — **см. примечание ниже**), `commands.rs` (type_complexity, 2× manual_range_contains, 2× dead_code: `tag_similarity`, `calculate_event_relevance` никогда не используются), `db.rs` (2× manual_flatten), `bin/sim.rs` (unnecessary_cast, single_char_add_str).

**Примечание (перекрёстная ссылка на D4):** `ptr_arg` в `populate_actor_tags` (rome_375.rs/constantinople_1430.rs) — функция вызывается один раз при загрузке сценария, не за тик, физически вне `src/engine/`, поэтому классифицирована как non-engine по букве правила. Но по духу это тот же паттерн, что и D4 (общий load-путь, дублируемый по сценариям) — держать в уме при централизации D4, возможно стоит чинить заодно.

---

### D1b: Правки не-engine находок

**Модель: Sonnet.**
**СТАТУС: ✅ ВЫПОЛНЕНО ЧАСТИЧНО (21/23). Коммит `21994c7`.** Diff-stat проверен: 10 файлов (`application/{actions,modes,narrative}.rs`, `bin/sim.rs`, `commands.rs`, `core/actor.rs`, `db.rs`, `llm/mod.rs`, `scenarios/{rome_375,constantinople_1430}.rs`), 34 insertions / 36 deletions, `src/engine/` не тронут. Верификация (`cargo build`/`test --workspace`, 46/46) прогнана через `git stash push --keep-index` именно на дереве, соответствующем содержимому коммита — не на полном рабочем дереве с посторонним WIP.
- Ключевое: удалены 2 мёртвые функции (`tag_similarity`, `calculate_event_relevance`) — подтверждено `grep -rn` по всему `src/`, ноль совпадений.

**⚠️ ОТКРЫТЫЙ ХВОСТ — не считать D1b полностью закрытым.** 2 находки не вошли в коммит: `db → _db` в `save_load.rs` и dead-code вокруг `get_relevant_events` в `commands.rs` — переплетены с уже существующим незакоммиченным WIP (фича `run_id`/scenario isolation) в тех же файлах, отделить без риска сломать WIP не удалось. Оставлены в рабочем дереве, поедут вместе с тем WIP при его коммите. **Риск:** это легко забыть, когда WIP через время выльется в отдельный коммит/фичу — тогда никто не вспомнит, что заодно должны закрыться эти 2 находки. Проверять при коммите того WIP явно, не полагаться на память.

---

### D1c: Правки engine-находок

**Модель: Opus.** Каждый PR — как C1/C2: отдельный `/clear`, обязательный baseline через `cargo run --bin sim` до/после, личное чтение diff перед мержем.
**СТАТУС: ✅ ВЫПОЛНЕНО, оба PR прочитаны лично и подтверждены построчно.**
- PR #3 (`fix/d1c-interactions`, коммит `b207c68`) — `interactions.rs`, +13/−2. 8 отдельных `#[allow(clippy::too_many_arguments)]` (не 7, как в изначальной оценке — clippy считает 826 и 847 отдельно), способ А, без изменения сигнатур/call site. 2× unused param → underscore-prefix.
- PR #4 (`fix/d1c-mod`, коммит `a3590c6`) — `mod.rs`, +17/−20. Все 8 находок из D1-списка. `VecDeque`-импорт вычищен как побочный эффект `or_default()`.
- Оба diff'а прочитаны на GitHub напрямую, не с чужих слов — эквивалентность подтверждена по коду (типы полей, структура match-выражения, семантика `Default` для `VecDeque`).

---

### D2: ESLint-долг

**Модель: Sonnet.**
**СТАТУС: ✅ ВЫПОЛНЕНО.**
- `_currentTick` (FamilyPanel.tsx:14) — закрыто через `varsIgnorePattern: '^_'` в `eslint.config.mjs`, без правки кода компонента (формализация уже существующей в коде конвенции).
- 3 `react-hooks/exhaustive-deps` в MapPanel.tsx — расширен dep до `worldState` (подтверждено: `setWorldState` в `App.tsx` всегда подменяет объект целиком, `worldState`/`worldState.actors` меняются синхронно — behavior-neutral, не stale-closure баг).
- Бонусом снят устаревший `eslint-disable-next-line @typescript-eslint/no-explicit-any` на `geoJsonRefs` — строка больше не содержит `any`.
- `npm run lint` → 0 problems, `tsc --noEmit` → чисто, `npm run build` → успешно. `engine/` не тронут.

---

### D3: Симуляция недетерминирована run-to-run — baseline-метод в `docs/sim_baseline.md` не ловит тонкие регрессии

**Модель: Opus.** Начинается с диагностики (подтверждение гипотезы), не с правки.
**СТАТУС: ✅ ВЫПОЛНЕНО. PR #5 (`fix/d3-sim-determinism`, коммит `36d2a58`), прочитан лично, diff подтверждён.**
- Причина подтверждена экспериментом (шаг 1 — обязательная диагностика до правки, как требовалось): `world.actors` — `HashMap`, порядок итерации рандомизирован на процесс. Два RNG-consuming пути читали этот порядок: `get_neighbor_pairs` (влияет на последовательность RNG-вызовов) и `phase_random_events` (влияет на цель события через `foreground_ids.choose(rng)` по индексу).
- Фикс — "smallest correct patch": тип `world.actors` не менялся (остался `HashMap`), сортировка добавлена только в двух точках потребления + tie-break в display-only сортировках `bin/sim.rs`.
- Верификация — seed-based, побайтовая: 10 комбинаций сценарий/режим × 3 прогона, `md5sum` идентичны все 10×3.
- **Регрессионный аудит баланса выполнен и задокументирован в `docs/sim_baseline.md`** — 15 прогонов на стратегию pre-fix vs post-fix. 5 из 6 стратегий совпадают с pre-fix большинством/единогласием. Единственный пограничный случай (constantinople scripted balanced, ~20% побед) — честно объяснён как семплирование уже существующего смешанного распределения (баланс не тронут, дрейфовал независимо от этого фикса ещё с 2026-03-11), не новый регресс.
- `docs/sim_baseline.md` получил новую секцию с методом (seed-based md5, не диапазоны "на глаз") и пометкой, что старые "Victory Tick" числа выше по файлу были из недетерминированных прогонов и не должны сравниваться напрямую.

---

### D4: `validate_dependencies` вызывается из каждого сценария отдельно, а не централизованно

**Модель: Opus.** Трогает общий load path для всех сценариев одновременно.
**СТАТУС: ✅ ВЫПОЛНЕНО. PR #6 (`fix/d4-centralize-validation`, коммит `14b2fc4`), прочитан лично, diff подтверждён построчно.**
- `validate_dependency_thresholds` вынесена как отдельная публичная функция в `engine/mod.rs`, не требует `KNOWN_METRICS`.
- `registry.rs::validate_scenario` вызывает её напрямую — подтверждено в diff: `crate::engine::validate_dependency_thresholds(&scenario.dependencies)`. Раз `validate_scenario` — часть пути `load_by_id`, choke point реален для **всех** сценариев, не только для тех, что помнят про собственный вызов.
- `debug_assert!` в начале `apply_dependency_rule` — условие `matches!(rule.mode, DependencyMode::Linear) || rule.threshold.is_some()`, целится точно в "non-Linear + None", не задевает легитимный "Some, но guard не сработал" per-tick случай.
- Новый тест инъецирует плохое правило в уже загруженный `rome_375` и вызывает `validate_scenario` напрямую, минуя per-scenario `validate_dependencies` — доказывает, что центральная точка ловит проблему сама по себе.
- Baseline: seed-based byte-exact md5, идентичен pre/post на обоих сценариях (4 seed × несколько режимов) — поведение не изменилось на существующих данных, как и требовалось.
- `_ => 0.0` release-fallback остался прежним, "warn" из первого черновика отчёта оказался неточной формулировкой — реально это существующий (не новый) `eprintln!` в `load_by_id`, PR не добавляет логирования.

**Секция D закрыта целиком** (D0–D4). Следующий шаг — вернуть `-D warnings` в clippy-шаг CI отдельным PR, раз весь долг, ради которого его снимали в D0, разобран.

---

### D5: Вернуть `-D warnings` в clippy-шаг CI

**Модель: Sonnet.**
**СТАТУС: ✅ ВЫПОЛНЕНО. PR #7 (`fix/d5-restore-clippy-strict`).** `git diff --stat`: 1 файл (`.github/workflows/ci.yml`), +1/−6 — убран флаг-комментарий про 37 находок и снятие `-D warnings`, добавлен сам флаг. Перед коммитом проверено `cargo clippy --workspace -- -D warnings` на чистом дереве `main` (посторонний WIP временно застэшен, затем восстановлен без изменений). После открытия PR оба CI-чека (`rust`, `frontend`) реально зелёные на GitHub Actions — строгий clippy не нашёл ничего нового на матрице раннера, что не находил бы локальный прогон.

**Найдено попутно, НЕ входит в объём D5, оставлено на будущее:** `cargo clippy --workspace --all-targets -- -D warnings` (CI этот флаг не использует — плейн `cargo clippy --workspace` тестовые таргеты не собирает) находит 6 находок, все в тестовом коде:
- `src/tests/core_tests.rs`: `unused_mut` (L478), 2× `manual_range_contains` (L380, L420)
- `src/tests/application_tests.rs`: `useless_format` (L141)
- `src/engine/mod.rs`: `items_after_test_module` (тестовый модуль на L1646 стоит раньше `generate_tick_explanation` на L1717)
- `src/scenarios/rome_375.rs`: `items_after_test_module` (тестовый модуль на L1708 стоит раньше `create_random_events` на L1793)

Эти находки не входили в изначальный аудит D1 (41 находка была собрана без `--all-targets`, т.е. без учёта cfg(test)-кода). Если CI когда-нибудь расширят до `--all-targets`/`--tests` на clippy-шаге — сначала нужен отдельный маленький PR на эти 6 находок, иначе шаг станет красным сразу же. `mod.rs`/`rome_375.rs` — engine-путь (Opus, т.к. трогает физическое расположение кода в файле с общим load path/interactions), два теста — Sonnet.

---

## Почему секции разделены именно так

Секции A и B — чисто механические изменения (файлы конфигурации, .gitignore, CI-yml). Диффы там читаются линейно, откат тривиален, ничего не выполняется иначе на рантайме. Их можно смело объединять и доверять Sonnet целиком.

Секции C и D1c/D3/D4 трогают код, исполняющийся каждый тик симуляции. Цена ошибки там не компиляционная, а поведенческая — баланс, инварианты из AGENTS.md. Единственный способ поймать регресс — прогон `cargo run --bin sim` до/после, а не чтение диффа глазами (хотя диф тоже нужно читать — лично, перед каждым мержем). Отсюда: маленький размер PR — не формальность, а условие верифицируемости, и модель здесь — Opus, потому что риск не в объёме кода, а в удержании инвариантов и самостоятельной постановке плана верификации.

D1 (классификация) и D2 — низкий риск, фронтенд/список без правок движка — Sonnet, можно объединять между собой, не нужен отдельный `/clear` на каждую.

Порядок выполнения: A → B → C1 → C2 → D0 → D1 → (D1b и D1c параллельно, но D1c с Opus и отдельными PR) → D2 (можно раньше, независим от остальных) → D3 → D4.
