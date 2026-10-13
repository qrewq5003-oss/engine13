# E1: каркас engine2 — набросок к контрольной точке

**Статус: контрольная точка, 2026-10-13, кода нет.** Шаг E1 брифа `engine2_brief.md` §8:
типы, время, детерминизм, RNG, сохранение, контракт `Simulation`, поток событий, учёт
записей, сторож. Рамки владельца: сначала набросок, `src-tauri` и старый движок не трогать,
мерж только по команде владельца. Решения E0 (`engine2_e0.md`): единица — кг пшеничного
эквивалента, деньги в долях Э2, институты закрытым перечнем, поле денежной единицы для показа.

Приёмка E1 по брифу: тест детерминизма (два процесса, бит в бит); сохранение → загрузка бит в
бит; гард «движок не знает имён».

---

## 0. Где живёт крейт

**Факт по коду:** корневой `Cargo.toml` — одиночный пакет `engine13`, секции `[workspace]`
нет, хотя AGENTS.md и бриф говорят «воркспейс». Предложение: добавить в корень
`[workspace] members = ["engine2", "sentinel"]` (корневой пакет остаётся членом), `src-tauri`
по-прежнему снаружи. Тогда `cargo test --workspace` в CI-job `rust` покрывает новые крейты
без правки CI. Это правка корневого манифеста, не кода старого движка — **вопрос владельцу
№ 1**, потому что бриф §10 разрешает трогать старое только там, где мешает играть.

Два крейта, а не один: `sentinel/` без зависимости от `engine2/` — иначе переходник старого
движка (§8) потянет engine2 в старый крейт.

## 1. Типы

- **Id — числа, не строки.** `ActorId(u32)`, `ProvinceId(u32)`, `HouseId(u32)`, `PersonId(u32)`,
  `UnitId(u32)`, `InstitutionId(u8)`, `VerbId(u8)`. Номера раздаются при загрузке сценария
  по отсортированным строковым id; строки уходят в таблицу `Names` слоя представления.
  В движке нет ни одной операции над строкой-именем — это и есть гард «движок не знает имён»
  (§9), закреплённый типом, а не только тестом.
- **Запасы — целые, оценки — дробные.** `People(u64)`, `Soldiers(u64)`, `Wheat(i64)` (кг,
  казна может быть отрицательной — долг), `Rating(f64)` только для легитимности и единства.
  Целые запасы делают тесты сохранения (§7) точными: сумма переводов равна нулю без допуска.
  Порядок сложения — по id (BTreeMap), дробные суммы не зависят от хеша (урок B21).
- **Константы** — `Params` с единицами в типах: `PerYear(f64)`, `Share(f64)`, загружаются из
  `engine2/params.toml` один раз; у каждой — диапазон из E0 §5, проверяемый при загрузке.
- **Схема сценария** — `ScenarioData` с `deny_unknown_fields` на каждом уровне: только факты
  §0.2 брифа. Числовых полей правил нет по типу. Есть `display_currency { name, kg_per_unit }`
  (E0 §7, вопрос к E1/E5) — читается **только** слоем представления; гард: в `engine2/src`
  нет чтений этого поля.

## 2. Время

`Tick(u32)`; `year = start_year + tick / 2`, полугодие — `tick % 2` (AGENTS §1). Год и
полугодие — методы, не поля: нечему рассинхронизироваться.

## 3. Детерминизм и RNG

- Один поток `ChaCha8Rng` в `World`, сид — из `(seed, run_id)`. Вытягивания только в
  именованных точках: `Draw::Decision(ActorId)`, `Draw::Battle(ProvinceId)`,
  `Draw::Event(ProvinceId)`, `Draw::Mortality(PersonId)`. Каждое вытягивание пишется в учёт
  (§7) как запись `RngDraw { tick, point, value }` — журнал случайности, по которому
  расхождение двух прогонов находится по первой несовпавшей точке, а не «где-то».
- Все коллекции состояния — `BTreeMap` и `Vec`, отсортированные по id. `HashMap` в
  `engine2/src` запрещён гардом (grep-тест).
- **Тест приёмки:** два процесса (тест запускает собственный бинарь через
  `std::process::Command` с флагом) играют тестовый мир 300 тиков от одного сида и сравнивают
  хеш сериализованного состояния **и** потока событий — бит в бит. Ловит шифрование хеша по
  процессу (урок «порядок журнала событий»).

## 4. Состояние и сохранение

- `World` сериализуется целиком, **включая состояние RNG** (в старом движке после загрузки
  RNG начинается с сида заново — это известная щель; здесь загрузка продолжает ту же
  последовательность).
- Формат: JSON через `serde_json` с `float_roundtrip` (как B31), поле `format: u32` в
  шапке; `#[serde(default)]` в формате сохранения запрещён — несовместимость это смена
  номера формата, а не тихое умолчание.
- **Тест приёмки:** прогон 300 тиков без остановки против прогона 150 → save → load → 150:
  байты состояния и событий совпадают.
- Слот оболочки не меняется: `src-tauri` кладёт строку в ту же таблицу `saves`; engine2 даёт
  `to_save() -> String` и `from_save(&str)`. Подключение оболочки — E5.

## 5. Контракт `Simulation`

```rust
pub trait Simulation: Sized {
    fn new(scenario: &ScenarioData, params: &Params, seed: u64) -> Result<Self, SchemaError>;
    fn tick(&mut self, player: &[VerbCall]) -> TickReport;   // события тика + дельта учёта
    fn verbs(&self, who: ActorRef) -> Vec<VerbOffer>;        // доступность и цена, одна функция для игрока и бота (B42)
    fn view(&self) -> WorldView;                             // модель представления для UI (не зеркало состояния)
    fn chronicle(&self) -> ChronicleSnapshot;                // снимок летописца — по смыслу NarrativeWorldSnapshot
    fn observe(&self) -> Observation;                        // наблюдение за тик для сторожа (крейт sentinel)
    fn to_save(&self) -> String;
    fn from_save(s: &str) -> Result<Self, SaveError>;
}
```

- `ActorRef` — `Ruler(ActorId)` или `House(HouseId)`: две роли E0.
- `VerbCall { verb: VerbId, actor: ActorRef, target: Target }`; перечень глаголов — enum
  движка (бриф §3); сценарий лишь включает подмножество и даёт имена в `Names`.
- `TickReport { events: Vec<Event>, ledger: Vec<Posting>, draws: Vec<RngDraw> }` — ровно то,
  что произошло за тик; оболочке не нужен весь журнал (урок B31: 191–392 МБ).
- На E1 контракт реализуется **пустым миром**: тик продвигает время, пишет событие
  `TickPlayed`, учёт нулевой. Системы заполняют его на E2.

## 6. Поток событий

Типизированный enum, без текста и без разбора id по префиксу (бриф §7):

```rust
pub enum Event {
    TickPlayed,
    Battle { province: ProvinceId, attacker: UnitId, defender: UnitId, outcome: Outcome, losses: (Soldiers, Soldiers) },
    Siege { province: ProvinceId, by: ActorId, state: SiegeState },
    ProvinceChangedHands { province: ProvinceId, from: ActorId, to: ActorId, how: Transfer },
    War { a: ActorId, b: ActorId }, Peace { .. }, Alliance { .. }, Vassalage { .. }, Freed { .. }, Marriage { .. },
    RulerDied { actor: ActorId, person: PersonId, cause: Cause }, Succession { .. }, Regency { .. },
    Revolt { province: ProvinceId }, Secession { .. }, ActorDied { .. }, ActorBorn { .. },
    Famine { province: ProvinceId, dead: People }, Plague { .. }, Migration { from, to, people },
    Conversion { province, to: Religion }, Loan { .. }, TaxChanged { .. },
    Verb { actor: ActorRef, verb: VerbId, target: Target },
}
pub struct Stamped { pub seq: u64, pub tick: Tick, pub event: Event }
```

Текст события делает слой представления из типизированных данных и `Names`; летописец
получает те же данные через `ChronicleSnapshot`. Это закрывает класс дефектов «annals.rs
ищет `military_conflict_`».

## 7. Учёт записей (ledger)

Каждая запись запаса идёт только через `Ledger::post(tick, Account, delta, Source)`:
`Account = { Population(ProvinceId) | Soldiers(UnitId) | Treasury(ActorId) | HouseTreasury(HouseId) }`,
`Source` — enum причин (Births, Famine, Plague, MigrationIn/Out, Tax, Upkeep, Loot, Recruit,
Desertion, BattleLoss, Tribute, Loan, Verb(VerbId), ...). Прямой записи в поле запаса нет по
типу (поля приватны, мутация только из `Ledger`).

- **Инвариант-тест:** за любой тик `Σ postings(account) == stock_after − stock_before` для
  каждого счёта; для переводов (миграция, дань) `Σ по паре == 0` точно, потому что запасы
  целые.
- Всегда включён (бриф §5.9), не feature. Память: записи тика отдаются в `TickReport` и
  затем сворачиваются в итоги по `(Account, Source)` за тик; полный список хранится только за
  последние N тиков (N — параметр, не влияет на мир).

## 8. Сторож — крейт `sentinel`

Независим от движка (бриф §6): видит только `Observation` за тик и `&[Stamped]`.

```rust
pub struct Observation { tick: Tick, actors: Vec<ActorObs>, ratings: Vec<(RatingKey, f64, Bounds)>, stocks: Vec<(Account, i64)>, alive: Vec<ActorId> }
pub trait Observed { fn observe(&self) -> Observation; fn events(&self) -> &[Stamped]; }
pub struct Sentinel { flags: Vec<Flag> }  // Flag::{PinnedToBound, BoundHit, Never, Always, ReadAbsent, Stagnation, StrategyIndependent, SeedIndependent, HistoryLock}
```

- Одиночные флаги (прижатость, срабатывание границы, застой, чтение отсутствующего) считаются
  по одному прогону; парные (сид, стратегия) и `history_lock` — по матрице прогонов, которую
  запускает тестовый бинарь, а не сторож.
- `history_lock` требует описания реального хода в фактах мира: сценарий несёт секцию
  `historical_course: [Outcome]` (например, «провинция X у актора Y», «актор Z мёртв»)
  **только для сторожа** — движок её не читает (гард как у `display_currency`). Порог ≥ 50 %
  партий без игрока (бриф §11 №5). **Вопрос владельцу № 2:** секция в схеме сценария
  допустима? Это не число правила, но и не факт старта.
- CI: короткий прогон тестового мира (50 тиков, 3 сида) с нулём флагов — в `cargo test`;
  полная матрица — отдельный бинарь перед мержем PR, меняющего мир (E2+).
- **Переходник старого движка.** Интерфейс `Observed` определён на E1 в крейте `sentinel`
  без зависимостей, но **реализация для старого движка — на E6**, когда идёт сравнение: один
  новый файл в старом крейте с `impl Observed for (WorldState, EventLog)`, старый код не
  меняется. Делать её на E1 рано: `Observation` ещё будет меняться с каждой системой E2, и
  переходник пришлось бы переписывать. Это рекомендация, решает владелец (**вопрос № 3**).

## 9. Гарды с первого дня (в `engine2/tests`)

1. `engine_knows_no_names`: в `engine2/src` нет ни одного строкового литерала, совпадающего с
   id из `src/scenarios/**` (по образцу `engine_knows_authored_content_only_by_these_names`),
   и нет сравнений `&str` с полями `Names`.
2. `no_hashmap_in_engine`: `HashMap`/`HashSet` в `engine2/src` отсутствуют.
3. `display_only_fields`: `display_currency` и `historical_course` читаются только вне
   `engine2/src` (grep по использованию полей).
4. `params_in_range`: каждая константа `params.toml` внутри диапазона E0 §5 — тест падает
   на выходе за полосу, а не молчит.
5. `save_format_has_no_defaults`: в типах сохранения нет `#[serde(default)]`.

## 10. Тестовый мир

Генератор в `engine2/tests/common`: пять держав, сетка 5×5 провинций, без устремлений, без
институтов — и вариант со всеми институтами (E0 §4, ограничение 4). На E1 нужен для
детерминизма и сохранения; на E2 — база сторожа (§0.3 брифа).

## 11. Один PR или несколько

E1 целиком — слишком много для одного обзора. Предложение — три PR, каждый с приёмкой:

| PR | содержание | приёмка |
|---|---|---|
| **E1a** | воркспейс, крейт `engine2`, типы (§1), время (§2), RNG и журнал вытягиваний (§3), `World`, сохранение (§4), тестовый мир (§10), гарды 2 и 5 | два процесса бит в бит; save → load бит в бит |
| **E1b** | поток событий (§6), учёт (§7), `Simulation` на пустом мире (§5), схема сценария с `deny_unknown_fields` и `display_currency`, гарды 1, 3, 4 | инвариант учёта; схема отвергает любое числовое поле правила; гард имён |
| **E1c** | крейт `sentinel` (§8): `Observation`, одиночные флаги, короткий прогон в CI; матричные флаги — интерфейс и бинарь без проверки стратегий (их нечем различать до E3) | тестовый мир 50 тиков × 3 сида без флагов |

Порядок жёсткий: E1b стоит на типах E1a, E1c — на событиях E1b.

## 12. Вопросы владельцу

1. §0: правка корневого `Cargo.toml` (секция `[workspace]`) — допустима как «не код старого
   движка»?
2. §8: секция `historical_course` в схеме сценария только для сторожа — допустима?
3. §8: переходник старого движка на `Observed` — на E6, не на E1 (рекомендация).
4. §11: три PR вместо одного.
5. Бриф §1 и §6 ссылаются на `docs/adaptive_engine_brief.md` — файла в репозитории нет
   (ни в дереве, ни в истории `git log --all`). Сторож спроектирован по §6 брифа engine2;
   если у вас есть тот документ, пришлите — сверю флаги.
6. Запасы целыми (§1) — подтвердить: это меняет арифметику систем E2 (деления с остатком
   вместо дробей).
