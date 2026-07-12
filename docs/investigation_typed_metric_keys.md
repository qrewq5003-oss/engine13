# Задача 13 — Типобезопасный конструктор метрик-ключей

**Стадия 1 — карта объёма и дизайн. Кода нет.**

---

## 0. Вердикт стадии 1

Объём **больше**, чем в постановке, и в трёх местах постановка **фактически неверна**. Всё три расхождения меняют дизайн, а не только смету:

| # | Постановка | Что показал grep |
|---|---|---|
| 1 | «14 полей хранят метрику как `String`» | **14 скалярных полей — точно, но это не все.** Ещё **11** полей несут метрику как **ключ `HashMap`** или `Vec<String>` (`PatronAction.effects`/`cost`, `RandomEvent.effects`, `tags.metrics_modifier`, `narrative_config.key_metrics`, `inheritance_coefficients`, …). Итого **25**. |
| 2 | «16 сайтов рантайм-парсинга, семантики `parse` / `parse_scoped`» | **17 сайтов и ТРИ семантики.** Третья — `eval_metric_condition` (engine/mod.rs:819), и она **несовместима** с `parse_scoped`: игнорирует явный префикс и возвращает `false` (а не `0.0`) на отсутствующем акторе. |
| 3 | «Save/load не задет — подтвердить явно» | **Задет. Подтвердить нельзя — это опровергается.** `WorldState` **встраивает** `MetricDisplay` и `GenerationMechanics` (world.rs:139, 141) — то есть два из 25 полей лежат **внутри JSON-сейва** (save_load.rs:39) и **уходят во фронтенд**, где TS разбирает их строкой (`GlobalMetricsPanel.tsx:16`). |

Плюс: **кастомного `Deserialize` недостаточно** — `rome_375` **не имеет** `auto_deltas.toml` и `milestone_events.toml`; весь его контент собран **литералами Rust** в `rome_375.rs`. Дизайн «парсим при чтении TOML» оставил бы без защиты именно тот сценарий, в котором сидели 9 из 9 сломанных блоков задачи #19.

И — **сквозной аудит нашёл два живых дефекта того же класса** (§5). Оба **вне** этого рефакторинга (они его сломали бы: их починка — изменение баланса, а критерий приёмки — байт-в-байт).

---

## 1. Карта полей: 25, в пяти популяциях

Популяция — это **не** «где лежит поле», а **как из строки получается адрес**. Разные популяции требуют разных типов; попытка натянуть один `MetricRef` на все 25 — и есть способ внести новый баг.

### Популяция A — абсолютный ключ (строка самодостаточна, префикс несёт скоуп)

Разбор: `MetricRef::parse`. Скоуп целиком внутри строки.

| # | Поле | scenario.rs | Сайт разбора |
|---|---|---|---|
| 1 | `VictoryCondition.metric` | 260 | engine:576 |
| 2 | `Condition.metric` — **как `victory.additional_conditions`** | 289 | engine:581 |
| 3 | `StatusIndicator.metric` | 273 | commands:405 |
| 4 | `MetricDisplay.metric` | 244 | **фронтенд** (TS, не Rust) |
| 5 | `ActionCondition::Metric.metric` | 387 | actions:123, actions:192 |
| 6 | `PatronAction.effects` (ключи `HashMap`) | 378 | actions:74 |
| 7 | `PatronAction.cost` (ключи `HashMap`) | 379 | actions:66, actions:211 |
| 8 | `NarrativeConfig.key_metrics` (`Vec<String>`) | 160 | llm:257 |
| 9 | `EarlyTransfer.condition_metric` | 531 | engine:1373 |
| 10 | `Scenario.global_metric_weights` (внешние ключи) | 196 | actions:77 — **сопоставляется строкой с ключом эффекта (6)** |

### Популяция B — актор-относительный, скоуп из **соседнего поля** `actor_id`

Разбор: `MetricRef::parse_scoped(s, actor_id)`. **Разрешимо на загрузке** — `actor_id` статичен.

| # | Поле | scenario.rs | Сайт |
|---|---|---|---|
| 11 | `AutoDelta.metric` (+ `AutoDelta.actor_id`, 325) | 318 | engine:308 |
| 12 | `DeltaCondition.metric` | 331 | engine:315 |
| 13–14 | `DeltaConditionRatio.metric_a` / `.metric_b` | 341/342 | engine:288, 289 |

### Популяция C — относительный к **рантайм-цели** (`self.`) — на загрузке НЕразрешим

Разбор: `parse_scoped(s, Some(target_id))`, где `target_id` **выбирается броском RNG** в тот же тик (`foreground_ids.choose(rng)`, engine:440).

| # | Поле | scenario.rs | Сайт |
|---|---|---|---|
| 15 | `RandomEvent.conditions[].metric` (это тот же `Condition`, что и №2!) | 289 | engine:458 |
| 16 | `RandomEvent.effects` (ключи `HashMap`) | 310 | engine:468 |

> **`Condition` — одна структура на две несовместимые семантики.** В `victory_condition.additional_conditions` она абсолютна (`parse`), в `random_events.conditions` — `self.`-относительна (`parse_scoped(target)`). Одним типом её поле затипизировать **нельзя**; это надо расщепить.

### Популяция D — актор-относительный с соседним `actor_id`, но **другой** семантикой

Разбор: `eval_metric_condition` (engine:819). **Не** `parse_scoped`.

| # | Поле | scenario.rs | Сайт |
|---|---|---|---|
| 17 | `EventConditionType::Metric.metric` (+ `.actor_id`) | 461 | engine:833 — обслуживает **и** `MilestoneEvent.condition`, **и** `RankCondition.condition` |

### Популяция E — **голое имя метрики** без скоупа; `MetricRef` не участвует вообще

Адрес берётся **из контекста кода**, а не из строки: актор — это source/target правила, носитель ранга, носитель тега. Индексируется напрямую `actor.get_metric` / `add_metric` / `set_metric`.

| # | Поле | scenario.rs | Сайт | Allowlist имён? |
|---|---|---|---|---|
| 18 | `InteractionCondition.metric` | 53 | interactions:198 | `validate_interaction_rules` есть, но **никогда не вызывается** (см. §4.F) |
| 19 | `InteractionEffect.metric` | 66 | interactions:213 | то же |
| 20 | `RankBonusEffect.metric` | 100 | engine:339–345 | **нет** |
| 21 | `TagDefinition.metrics_modifier` (ключи) | 128 | engine:661 | **нет** |
| 22–23 | `DependencyRule.from` / `.to` | 26/27 | engine (dependencies) | **ДА** — `KNOWN_METRICS`, единственное место |
| 24 | `SpawnActorConfig.initial_metrics` (ключи) | 408 | напрямую в `actor.metrics` | **нет** |
| 25 | `GenerationMechanics.inheritance_coefficients` (ключи) | 517 | engine:1391 | **нет** — и **сломано**, см. §5.G |
| (25b) | `Scenario.initial_family_metrics` (ключи) | 216 | sim:104 / save_load:147 | **нет** — и **расходится между путями**, см. §5.H |

**Это ровно та дыра, про которую писал §22.5 задачи 12** («allowlist имён есть только для `dependencies.toml`»). Теперь известно, почему: валидатор для interaction-правил **написан и мёртв**, а валидатор действий (`validate_patron_actions`, scenario.rs:616) принимает `_known_metrics` и **демонстративно его не использует** («Full validation would require MetricRef parsing; skip for now»).

---

## 2. Три семантики разбора — и чем именно они расходятся

| | `MetricRef::parse` (S1) | `parse_scoped(s, Some(a))` (S2) | `eval_metric_condition(m, Some(a))` (S3) |
|---|---|---|---|
| явный `global:`/`family:`/`actor:` | уважает | **уважает** | **ИГНОРИРУЕТ** — форсит `Actor{a, m}` |
| `self.x` | → `Global{"self.x"}` (фантом) | → `Actor{a, x}` | → `Actor{a, "self.x"}` (фантом) |
| голое `x` (без `:` и `.`) | → `Global{x}` | → `Actor{a, x}` | → `Actor{a, x}` |
| `id.metric` (точка без префикса) | → `Global{"id.metric"}` — **фантом** | → то же (падает в S1) | → `Actor{a, "id.metric"}` — фантом |
| актора `a` нет в мире | — | `.get()` → **0.0** | **`false`** (ранний возврат) |
| `actor:id` (без точки) | → `Global{"actor:id"}` — **фантом** | → то же | — |

Две строки этой таблицы — не косметика:

**(1) Расхождение S2↔S3 по префиксу.** Одна и та же пара `metric = "global:federation_progress"` + `actor_id = "byzantium"` в `auto_delta` читает **настоящий глобал**, а в `milestone` — метрику актора с именем `"global:federation_progress"`, то есть **вечный 0.0**. Один и тот же TOML-облик, противоположный смысл. **Сегодня это латентно** — grep по всем трём сценариям: ни один milestone/rank не задаёт префикс вместе с `actor_id` (единственное совпадение — комментарий в `constantinople_1430/milestone_events.toml:2`). Типизация обязана эту ловушку **закрыть**, а не унаследовать.

**(2) Расхождение по отсутствующему актору — ЖИВОЕ и несущее.** `world.actors.remove(&actor_id)` при коллапсе (engine:1558). Живой контент, который на этом стоит:

- `rome_375.rs:1448` `rome_splits`: `rome.cohesion < 30`, duration 5 — **и сам `rome` при этом распадается** (инвариант 10 `AGENTS.md`).
- `rome_375.rs:1555` rank-условие `rome_city`: `rome.legitimacy < 20`.
- `constantinople_1430/milestone_events.toml:175` — `ottomans.cohesion < 40` → **спавнит мамлюков**. (Османы уходят в 0.00 в 27/30 прогонов — см. задачу 6.)

Все три — операторы **`less`**. Сегодня после смерти актора условие = `false`. Если наивно затипизировать поле в `MetricRef` и звать `.get()` (дефолт `0.0`), то `0.0 < 30` = **true** — и milestone начинает срабатывать **вечно, после смерти актора**. Это ровно тот тихий сдвиг семантики, который критерий «байт-в-байт» обязан ловить; лучше не дать ему возникнуть.

**`MetricRef::get` не умеет выражать «актора нет».** Значит, типизации поля **недостаточно**: нужен `try_get -> Option<f64>`.

---

## 3. Сайты рантайм-парсинга: 17 (постановка ожидала 16)

| Сайт | Поле | Семантика |
|---|---|---|
| commands.rs:405 | `StatusIndicator.metric` | S1 |
| engine/mod.rs:576 | `VictoryCondition.metric` | S1 |
| engine/mod.rs:581 | `victory.additional_conditions[].metric` | S1 |
| engine/mod.rs:1373 | `EarlyTransfer.condition_metric` | S1 |
| llm/mod.rs:257 | `narrative_config.key_metrics[]` | S1 |
| application/actions.rs:66 | `PatronAction.cost` (apply) | S1 |
| application/actions.rs:74 | `PatronAction.effects` (apply) | S1 |
| application/actions.rs:123 | `ActionCondition::Metric` (is_available) | S1 |
| application/actions.rs:192 | `ActionCondition::Metric` (get_available) | S1 — **дубль 123** |
| application/actions.rs:211 | `PatronAction.cost` (affordability) | S1 — **дубль 66** |
| engine/mod.rs:288 | `ratio_conditions.metric_a` | S2 |
| engine/mod.rs:289 | `ratio_conditions.metric_b` | S2 |
| engine/mod.rs:308 | `auto_delta.metric` | S2 |
| engine/mod.rs:315 | `DeltaCondition.metric` | S2 |
| engine/mod.rs:458 | `random_event.conditions[].metric` | S2 (цель — рантайм) |
| engine/mod.rs:468 | `random_event.effects` | S2 (цель — рантайм) |
| engine/mod.rs:833 | `EventConditionType::Metric` | **S3** |

Плюс **2 сайта валидации** (не рантайм): `registry.rs:47`, `registry.rs:119` (`validate_metric_ref` — гард задачи 9).

Плюс **3 потребителя сырой строки**, которые не парсят, но требуют, чтобы строка **осталась строкой**:
- `actions.rs:214` — `metric.strip_prefix("actor:").replace(['.','_'], " ")` → человекочитаемое имя ресурса в UI;
- `actions.rs:68/88` — `applied_costs`/`applied_effects` кладут **сырой ключ** в `HashMap<String,f64>` → JSON в метаданные события → фронтенд;
- `llm/mod.rs:258` — `key_metrics.insert(metric_key.clone(), value)` → ключ уходит в промпт хрониста.

⇒ Любой типизированный `MetricRef` **обязан** иметь `Display`, дающий канонический ключ.

---

## 4. Дизайн

### 4.1. Три типа, не один

Единый тип на все 25 полей — **неверен**: популяция E адресуется контекстом, а не строкой, и `global:`-префикс в interaction-правиле сегодня означает *метрику актора с именем `"global:x"`*, то есть мёртвый ноль. Затипизировав E как `MetricRef`, мы **молча добавим** туда поддержку префиксов — это изменение семантики, а не рефакторинг.

**Тип 1 — `MetricRef` (абсолютный).** Три существующих варианта. Изменения:
- `parse(&str) -> Result<MetricRef, MetricKeyError>` — **fallible**. Ветка «invalid format → Global» (metric_ref.rs:34) становится **ошибкой**: `"actor:rome"` без точки больше не деградирует в фантом.
- `impl FromStr`, `impl Display` → канонический ключ (`actor:id.metric` / `family:key` / `global:key`).
- `impl Serialize` → `serialize_str(&self.to_string())` — **не** производный enum (§4.3).
- `impl<'de> Deserialize` → `visit_str` → `parse` → `de::Error::custom`. **Испорченный ключ роняет TOML.**
- `try_get(&WorldState) -> Option<f64>` — `None`, когда актора нет. `get` сохраняет дефолт `0.0` для путей, которым он нужен (§2, пункт 2).
- Конструкторы `MetricRef::actor(id, name)` / `::global(key)` / `::family(key)`, а **внутренние строки — приватные newtype** (`ActorId`, `MetricName`, `GlobalKey`) с проверяющими конструкторами. Тогда `MetricRef::Global { key: "rome.cohesion".into() }` **не собирается** — фантом перестаёт быть выразимым из Rust, а не только из TOML.

**Тип 2 — `RelativeMetricRef`** (только популяция C):
```
enum RelativeMetricRef { SelfRelative(MetricName), Absolute(MetricRef) }
fn resolve(&self, target: &ActorId) -> MetricRef
```
`Deserialize` из строки: `self.x` → `SelfRelative`; иначе → `MetricRef::parse`. Тип **сам говорит**, что без цели он не адрес.

**Тип 3 — `MetricName`** (популяция E): newtype над `String`, `Deserialize` **отвергает** любую строку с `:` или `.`. Это превращает «префикс, случайно написанный в interaction-правиле/теге/rank-бонусе» из тихого `0.0` в **ошибку загрузки**. Сюда же естественно вешается allowlist имён (`KNOWN_METRICS`), которого просит §22.5 задачи 12 — но это **отдельный** шаг, не обязательный для байт-идентичности.

### 4.2. Кто разрешается на загрузке, а кто нет (ответ на вопрос 3 постановки)

`parse_scoped` **как свободная функция исчезает**, но её содержимое не испаряется — оно расходится по трём местам:

| Арм `parse_scoped` | Куда переезжает |
|---|---|
| явный префикс → как написано | `MetricRef::parse` (в `Deserialize`) |
| голое имя + `actor_id`-сосед | **контейнерный `Deserialize` для `AutoDelta`** — резолвится **один раз на загрузке** |
| `self.x` + рантайм-цель | **остаётся** — как `RelativeMetricRef::resolve(target)`, ровно 2 сайта |

**Почему нужен контейнерный `Deserialize`, а не `deserialize_with` на поле.** Скоуп `AutoDelta.metric` лежит в **соседнем** поле `actor_id`; serde разбирает поля по одному и соседей не показывает. Значит — ручной `impl Deserialize for AutoDelta` через теневую структуру (`AutoDeltaRaw` со строками) и резолв всех четырёх строк (`metric`, `conditions[].metric`, `ratio_conditions.metric_a/b`) против `actor_id` **до** конструирования `AutoDelta`. Публичный тип хранит уже готовый `MetricRef`. Ровно та же техника — для `EventConditionType::Metric` (популяция D).

Итог по сайтам: **14 из 17 становятся прямым `.get()` / `.apply()`**. Остаются:
- 2 сайта случайных событий — `RelativeMetricRef::resolve(target)` (иначе никак: цель выбирается броском RNG);
- 1 сайт `eval_metric_condition` — но уже **без парсинга**: только `try_get()` + сравнение, ради гарантии «актора нет → false».

**`self.`-арм — принципиально не устраним.** Это не недоработка дизайна: цель события физически не существует на момент чтения TOML.

### 4.3. Save/load и провод во фронтенд (ответ на вопрос 4 — **опровержение**)

Постановка просит «явно подтвердить, что `WorldState`-сериализация не задета». **Подтвердить нельзя.**

```
WorldState.global_metrics_display: Vec<MetricDisplay>      (world.rs:139)  ← поле №4
WorldState.generation_mechanics:   Option<GenerationMechanics> (world.rs:141) ← поля №25, №9
```
`WorldState` целиком сериализуется в JSON сейва (`save_load.rs:39`) **и** уходит во фронтенд (`App.tsx:449`), где TS разбирает ключ **строкой**: `GlobalMetricsPanel.tsx:16` — `md.metric.replace('global:', '')`.

Следствия, обязательные к исполнению:

1. **`Serialize` для `MetricRef` — только строка.** Производный enum дал бы `{"metric":{"Global":{"key":"federation_progress"}}}` — это сломало бы **и** старые сейвы, **и** TS. Поэтому `serialize_str(Display)`, `Deserialize` через `visit_str`.
2. **Формат сейва при этом не меняется** — проверено по содержимому: все строки в этих двух полях **уже канонические**: `"global:federation_progress"` (constantinople_1430.rs:254), `"actor:byzantium.external_pressure"` / `"actor:rome.external_pressure"` / `"family:family_influence"` в `status_indicators`, `early_transfer.condition_metric = "actor:rome.external_pressure"` (rome_375.rs:1610). Канонизация здесь — **тождество**. Старые сейвы грузятся (Deserialize принимает и голую форму); новые байт-идентичны.
3. **`#[serde(default)]`-совместимость не задета** — ни одно из типизируемых полей не добавляется и не удаляется.
4. **`HashMap<MetricRef, f64>`** (effects/cost) как serde-мапа работает **только** при строковом ключе — что и даёт п.1 — плюс `Hash + Eq`. **`HashMap` не менять на `BTreeMap`**: сегодня итерация effects доказана безопасной (RNG в цикле не потребляется, ключи уникальны, применение коммутативно — investigation_metric_scoping.md §Determinism), а смена контейнера поменяла бы порядок ключей в `applied_effects` → в JSON метаданных события. Не трогать.
5. **`global_metric_weights` — мина, но обезврежена контентом.** Внешний ключ (`constantinople_1430.rs:212`, `"global:federation_progress"`) сопоставляется **строкой** с ключом эффекта (`actions.toml:35`, `"global:federation_progress"`). Обе стороны уже канонические ⇒ канонизация ничего не сдвинет. Но при типизации **обе стороны обязаны получить один и тот же тип ключа**, иначе промах даёт вес `1.0` молча — то есть **изменение баланса**.

### 4.4. Дыра, которую `Deserialize` не закрывает: `rome_375` — это Rust, а не TOML

```
rome_375/           actions.toml dependencies.toml eras.toml map.toml rank_bonuses.toml tags.toml
constantinople_1430/  + auto_deltas.toml + milestone_events.toml
milan_1477/           + auto_deltas.toml + milestone_events.toml
```
У `rome_375` **нет** `auto_deltas.toml` и `milestone_events.toml`: его `auto_deltas` (rome_375.rs:1217+), `milestone_events` (1435+), `victory_condition` (197), `status_indicators` (1668+), `narrative_config` (214+) — **литералы Rust**. `Deserialize` по ним **не вызывается никогда**.

Это тот самый сценарий, в котором задача #19 нашла **9 из 9** сломанных auto_delta-блоков.

⇒ Дизайн «парсим и валидируем при чтении TOML» защищает **2 сценария из 3**. Структурный ответ обязан быть **в типе**, а не в `Deserialize`: приватные внутренние строки + fallible-конструкторы (§4.1). Тогда `metric: "population".to_string()` **не компилируется**, и `rome_375.rs` вынужден писать `MetricRef::actor("rome", "population")` — механическая правка контента, байт-нейтральная.

### 4.5. Что происходит с гардами задачи 9

`validate_metric_ref` (registry.rs:118) проверяет **форму** ключа постфактум. После типизации форма гарантирована **конструктором**, и гард по форме становится тавтологией — но его вторая половина (`actor_id` существует в сценарии) **остаётся нужна**: тип не знает списка акторов. Итог: `validate_scenario` **сохраняется**, но худеет до проверки *ссылочной целостности* (актор есть в сценарии), а проверку *формы* забирает тип. Полностью удалять — нельзя.

---

## 5. Что аудит нашёл попутно: два живых дефекта того же класса

Оба — **вне рефакторинга** и обязаны идти **отдельными PR**: их починка **меняет поведение**, а критерий приёмки задачи 13 — байт-в-байт. Смешивать нельзя.

### 5.G. `inheritance_coefficients` не совпадают ни с чем — наследование всегда идёт по дефолту 0.7

- `family_state.metrics` ключуется **нормализованно**: `sim.rs:104–111` снимает `family:`, затем `family_` ⇒ ключи `influence` / `knowledge` / `wealth` / `connections`. Так же ведёт себя `MetricRef::Family::{get,apply}` (metric_ref.rs:92, 124) — единственные рантайм-писатели.
- `GenerationMechanics.inheritance_coefficients` ключуется **сырьём**: `rome_375.rs:1591–1594` — `"family:family_influence"`, `"family:family_knowledge"`, `"family:family_wealth"`, `"family:family_connections"`.
- `engine/mod.rs:1391`: `inheritance_coefficients.get(metric)`, где `metric` — **рантайм-ключ** (`influence`). `HashMap::get` — точное совпадение. Пересечение ключевых пространств **пусто**.

⇒ `.unwrap_or(0.7)` **срабатывает всегда**. Авторские коэффициенты Рима (0.85 / **1.0** / **1.0** / 0.8) **не применялись ни разу за историю проекта**; `knowledge` и `wealth`, задуманные как ненаследуемо-полные (1.0), на самом деле теряют 30% на каждой смене поколения. Нормализация ключа не делается нигде (grep: `inheritance_coefficients` встречается ровно в 3 файлах и нигде не нормализуется).

Это **восьмой сайт** того же класса, что и сайты 1–7, и первый, который лежит **вне** `MetricRef` целиком — поэтому его не поймал ни один гард задачи 9. В новой типизации он — популяция E (голые имена), и `MetricName`-newtype **отверг бы `family:family_influence` на загрузке**.

### 5.H. Приложение и симулятор расходятся в ключевом пространстве family-метрик

- `sim.rs:104` (**все baseline-прогоны**) — нормализует ключи перед посадкой в `family_state.metrics`.
- `save_load.rs:147` (`load_scenario` — **путь свежего старта в приложении**) — `metrics: initial_metrics.clone()`, **без нормализации** ⇒ ключи остаются `"family:family_influence"`.

⇒ В приложении `MetricRef::Family::get` ищет `influence`, находит пусто → **0.0**, а `apply` заводит **вторую** запись `influence` рядом со стухшей `family:family_influence`. Именно поэтому `FamilyPanel.tsx:29–36` содержит нормализацию с комментарием *«Prefer the canonical key if both legacy and canonical variants exist»* — **UI маскирует дефект данных**, что прямо запрещено инвариантом 12 `AGENTS.md`.

Sim-baseline этого **не видит** (он на правильном пути), поэтому байт-в-байт стадии 2 его не поймает — и поэтому он тем более не должен ехать внутри рефакторинга.

---

## 6. Прямые ответы на четыре вопроса постановки

1. **Полный список полей и сайтов.** 25 полей (не 14: 14 скалярных + 11 коллекционных), 17 рантайм-сайтов (не 16) + 2 валидационных + 3 потребителя сырой строки. Семантик **три**, не две (§2). Расхождение семантик — да, часть проблемы: S2 и S3 **противоположно** трактуют явный префикс при заданном `actor_id`.
2. **Кастомный `Deserialize`.** Нужен, но его **недостаточно** (§4.4 — `rome_375` собран литералами Rust). **Не один тип на все поля, а три** (§4.1): `MetricRef` (абсолютный), `RelativeMetricRef` (`self.`, нужна рантайм-цель), `MetricName` (голое имя, скоуп из контекста кода). Actor-relative поля с соседним `actor_id` (`AutoDelta`, `EventConditionType::Metric`) разрешаются **контейнерным** `Deserialize`, потому что `deserialize_with` на поле соседей не видит.
3. **Нужна ли `parse_scoped` рантайму.** Как свободная функция — **нет**, исчезает. Но её `self.`-арм **обязан выжить** в виде `RelativeMetricRef::resolve(target)`: цель случайного события выбирается броском RNG в тот же тик и на загрузке не существует. 14 из 17 сайтов становятся прямым `.get()`/`.apply()`; 2 остаются с `resolve`; 1 (`eval_metric_condition`) остаётся ради **`try_get`** — гарантии «актора нет → `false`», которую `MetricRef::get` (дефолт 0.0) выразить не может, и на которой стоит живой контент всех трёх сценариев (§2).
4. **Save/load не задет.** **Опровергнуто.** `WorldState` встраивает `MetricDisplay` и `GenerationMechanics` (world.rs:139, 141) ⇒ два поля лежат в JSON-сейве и в payload фронтенда, где TS разбирает их строкой. `Serialize` обязан отдавать канонический ключ строкой; при этом формат сейва **фактически не меняется**, потому что весь контент в этих полях уже канонический (§4.3). `#[serde(default)]`-совместимость не затронута.

---

## 7. Что обязана сделать стадия 2

**Критерий приёмки — единственный: sim байт-в-байт на всех трёх сценариях.** Любое расхождение = баг рефакторинга, а не найденный баланс.

Порядок, снижающий риск:
1. Типы + `Display`/`Serialize`/`Deserialize`/`try_get`/fallible-конструкторы. **Ноль изменений в поведении** (типы ещё никем не используются) → baseline обязан быть байт-идентичен тривиально.
2. Популяция A (10 полей, семантика S1) — самая безопасная: строка самодостаточна, резолв идемпотентен.
3. Популяция B (`AutoDelta`, контейнерный `Deserialize`) + перевод `rome_375.rs` на конструкторы.
4. Популяция D (`EventConditionType`) — **здесь ожидается основной риск**; `try_get` обязателен, иначе `rome_splits` / `rome_city` / спавн мамлюков поплывут (§2).
5. Популяция C (`RelativeMetricRef`) + расщепление `Condition` на абсолютную и относительную.
6. Популяция E (`MetricName`) — и здесь **ожидается падение загрузки на `inheritance_coefficients`** (§5.G). Это **правильное** падение: тип нашёл восьмой сайт. Но чинить его в этом PR **нельзя** — починка меняет баланс. Варианты: (а) временно оставить `inheritance_coefficients` как `String` и вынести в отдельную задачу; (б) сделать `MetricName` принимающим `family:`-префикс. **Рекомендую (а)** — не прятать дефект внутри типа.
7. Тест на порчу ключа: намеренно испорченный ключ в тестовом сценарии ⇒ падение **на TOML** (`Deserialize`), не на позднем `validate_scenario`.
8. Конвенционный тест §22.5: контент не может писать в имена, зарезервированные движком.

**Явно НЕ входит:** дефекты §5.G и §5.H (отдельные PR, каждый — изменение поведения); пункт `submission` → `GUARDED_METRICS`; оживление мёртвого `validate_interaction_rules` (это добавит проверок к контенту, который сейчас не проверяется — может вскрыть ещё сайты, и это отдельный разговор).
