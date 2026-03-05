# ENGINE 13 — Архитектурные решения
*Все решения приняты. Готово к реализации.*

## Оглавление

1. [Контекст проекта](#контекст-проекта)
2. [Платформа и стек](#платформа-и-стек)
3. [Хранение данных](#хранение-данных)
4. [Сохранение и загрузка](#сохранение-и-загрузка)
5. [Симуляция](#симуляция)
   - Принцип альтернативной истории
   - Архитектура
   - Акторы и пороги релевантности
   - Базовые метрики и производные
   - Универсальный контракт сценария
   - Руководство по созданию сценария
   - Технологии — теги и эры
   - Граф зависимостей
   - Автономные дельты
   - Взаимодействия между акторами
   - Смерть и рождение акторов
6. [Порядок операций за тик](#порядок-операций-за-тик)
7. [География](#география)
8. [Время](#время)
9. [Механика поколений](#механика-поколений-опциональный-модуль)
10. [Взаимодействие игрока](#взаимодействие-игрока)
11. [Режимы игры](#режимы-игры)
12. [LLM](#llm)
13. [Схема базы данных](#схема-базы-данных)
14. [Tauri команды](#tauri-команды)

---

## Контекст проекта

Исторический симулятор с живым миром под капотом. Цифры меняются каждый год. Акторы взаимодействуют автоматически. Большинство акторов — фон, игрок о них не знает. LLM генерирует нарратив только для переднего плана и только когда нужно. Сценарии от 375 года до 1400+.

Ориентир по глубине — Paradox (CK3, EU4). Ориентир по нарративу — текстовый, качественный, офлайн.

---

## Платформа и стек

- **Tauri** — десктопное приложение
- **Rust бэкенд** — симуляция мира, акторы, метрики, граф зависимостей, хранение
- **React + TypeScript фронтенд** — UI, нарратив, действия игрока
- Общение через Tauri команды — React вызывает Rust-функции, Rust возвращает результат
- **Ollama** — локальная LLM, офлайн, подключается к Rust через localhost API

---

## Хранение данных

- **SQLite** — файл на диске, стандарт для десктопа
- Симуляция живёт **в памяти** во время игры (Rust)
- Периодический снапшот в SQLite по триггеру

**Трёхуровневое хранение истории:**
- Текущее состояние каждого актора — полный снапшот метрик
- Индексированное хранилище событий — полная история без потерь
- Временное затухание при поиске — старые события менее релевантны

**Формат ключевого события:**
```
event: {
  tick: 47,
  year: 422,
  actor_id: "rome",
  type: "collapse" | "war" | "migration" | "threshold" | "birth" | "death",
  metrics_snapshot: {...},
  description: "...",
  involved_actors: [],
  tags: ["war", "rome", "goths", "migration"],
  is_key: true | false
}
```

**Индексированное хранилище событий:**

Полная история без потерь — ничего не удаляется. SQLite с индексами:
```sql
CREATE INDEX idx_events_actor ON events(actor_id);
CREATE INDEX idx_events_tick ON events(tick);
CREATE INDEX idx_events_type ON events(type);
CREATE INDEX idx_events_key ON events(is_key);
```

**Временное затухание релевантности:**
```
релевантность = тематическая_близость × временной_коэффициент

тематическая_близость = совпадающие_теги / max(теги_события, теги_запроса)

теги_запроса формируются из контекста текущего тика:
  - name всех нарративных акторов
  - типы активных взаимодействий ("war", "trade", "migration")
  - регионы активных акторов
  - теги текущей эры

пример:
  событие tags: ["war", "rome", "goths", "land"]
  запрос tags:  ["rome", "goths", "war", "mediterranean"]
  совпадений: 3 из 4 → близость = 0.75

временной_коэффициент:
  0-10 тиков назад   → 1.0
  11-30 тиков назад  → 0.7
  31-60 тиков назад  → 0.4
  61-100 тиков назад → 0.2
  100+ тиков назад   → 0.05

исключение — is_key события:
  временной_коэффициент не падает ниже 0.3
```

**LLM получает:**
- топ 15 по релевантности с учётом затухания
- последние 5 событий всегда — независимо от релевантности
- is_key события текущих нарративных акторов всегда

**История мёртвых акторов:**

Что хранится:
- Полный снапшот метрик на момент смерти
- Все события за жизнь актора — полная история через индексированное хранилище
- Финальное is_key событие с типом "death"

Как работает:
- Наследник не копирует историю — только ссылка на dead_actors запись
- Когда наследник выходит на передний план LLM получает его текущее состояние + историю родителя через поиск по тегам
- Мёртвые акторы не удаляются никогда

```
dead_actor: {
  id: "rome",
  tick_death: 101,
  year_death: 476,
  final_metrics: {...},
  successor_ids: [
    { id: "rome_west", weight: 0.45 },
    { id: "rome_east", weight: 0.55 }
  ]
}
```

---

## Сохранение и загрузка

**Когда сохраняется:**
- Автосохранение каждые 5 тиков
- Сохранение перед каждым действием игрока — точка возврата

**Слоты:**
- Один автослот — перезаписывается автоматически
- Ручные именованные слоты — игрок сам решает когда сохраниться

**Структура файла сохранения:**
```
save: {
  version: "1.0",
  tick: 47,
  year: 422,
  scenario_id: "rome_375",
  world_state: {...},
  history: {...},
  dead_actors: {...},
  player_state: {...}
}
```

**Версионирование конфигов:**

Стратегия — мягкая совместимость:
- Недостающие поля получают дефолтные значения
- Лишние поля игнорируются
- Если версии конфига и сохранения не совпадают — предупреждаем игрока но не блокируем загрузку

```
config: {
  version: "1.0",
  type: "scenario" | "graph" | "auto_deltas"
}

save: {
  version: "1.0",
  config_versions: {
    scenario: "1.0",
    graph: "1.2",
    auto_deltas: "1.0"
  }
}
```

Миграция между версиями добавляется когда появятся другие игроки с сохранениями которые нельзя сломать.

---

## Симуляция

### Принцип альтернативной истории

Исторический исход — наиболее вероятный путь, не единственный.

- Стартовые метрики отражают реальное историческое состояние
- milestone_events описывают переломные моменты — не скрипт
- Игрок может изменить исход через решения
- LLM адаптирует нарратив под то что происходит — не пересказывает историю

Примеры для Rome 375:
- Рим остаётся единым и отражает гуннов
- Рим интегрирует готов как федератов и усиливается
- Рим раскалывается на Запад и Восток (исторический исход)
- Западный Рим коллапсирует раньше времени

Симуляция решает что происходит — не скрипт.

---

### Архитектура

Централизованная — не агентная. Движок проходит по всем акторам каждый тик и считает взаимодействия по правилам.

### Акторы

- Стартовый масштаб — **50-80 акторов**
- Делятся на: передний план (нарратив) и фон (только цифры)
- Движок сам замечает когда фоновый актор достиг порога релевантности

**Пороги релевантности — выход на передний план:**
- **Сила:** power_projection > 70% от среднего по активным акторам
- **Контакт:** взаимодействие с нарративным актором И power_projection > 50 И интенсивность высокая:
  ```
  военное давление > 0.4 И power_projection > 50
  ИЛИ (торговля > 2 И культура > 0.5) И power_projection > 50
  ИЛИ миграция > 5% population получателя И power_projection > 50
  ```
  Военное давление одно достаточно. Торговля или культура — нужна комбинация.
- **Внутренний перелом:** метрика изменилась на >30 за 5 тиков ИЛИ cohesion < 25 ИЛИ legitimacy < 20

**Что происходит при выходе на передний план:**
- Движок добавляет актора в world_snapshot
- LLM генерирует вводный нарратив — кто это, откуда, почему важен сейчас
- Если процедурный фоновый актор — LLM придумывает название и контекст из его истории и метрик

**Возврат в фон:**
Акторы могут уйти обратно — иначе передний план переполнится.
```
актор уходит в фон когда:
  power_projection < 40% от среднего по активным акторам
  И нет активных взаимодействий с нарративными акторами
  И нет внутреннего перелома 10+ тиков
```
При возврате в фон история актора остаётся в индексированном хранилище — ничего не теряется.

### Базовые метрики (каждый актор, любая эпоха)

| Метрика | Тип | Описание |
|---|---|---|
| population | абсолютное | численность |
| military_size | абсолютное, тысячи | количество солдат |
| military_quality | 0-100 | выучка, мораль, снаряжение |
| economic_output | 0-100 | производительность + торговля |
| cohesion | 0-100 | внутреннее единство |
| legitimacy | 0-100 | право на власть |
| external_pressure | 0-100 | давление снаружи |
| treasury | абсолютное, может быть отрицательным | накопленные ресурсы, доходы минус расходы каждый тик |

**Производные (считаются, не хранятся):**
```
stability = (legitimacy × 0.4 + cohesion × 0.4) - (external_pressure × 0.2)

power_projection = (military_size × 0.4 + military_quality × 0.4 + treasury_modifier × 0.2)
                   × era_modifier

treasury_modifier:
  treasury > 500 → 1.2
  treasury > 0   → 1.0
  treasury < 0   → 0.7

era_modifier:
  ancient        → 0.8
  early_medieval → 0.9
  high_medieval  → 1.0
  late_medieval  → 1.1
  early_modern   → 1.2
```
Пересчитываются каждый тик на шаге 4. Не хранятся.

**Сценарные метрики** — только то что уникально для роли игрока и не выводится из симуляции.

Rome 375:
- ~~huns_pressure~~ — считается автоматически
- ~~gothic_integration~~ — считается автоматически
- **family_influence** — остаётся
- **family_knowledge** — остаётся

### Универсальный контракт сценария

Добавить новый сценарий = описать акторов и роль игрока. Механика работает сама.

**Полная структура актора:**
```
actor: {
  id: "rome",                          // машинный идентификатор — только для кода
  name: "Римская Империя",             // для LLM и игрока
  name_short: "Рим",                   // для компактного упоминания
  region: "mediterranean",
  center: { lat: 41.9, lng: 12.5 },
  neighbors: [
    { id: "goths", distance: 2, border_type: "land" },
    { id: "carthage", distance: 3, border_type: "sea" }
  ],

  metrics: {
    population: 5000,
    military_size: 150,
    military_quality: 65,
    economic_output: 55,
    cohesion: 40,
    legitimacy: 45,
    external_pressure: 60,
    treasury: 200
  },

  era: "ancient",
  tags: ["bureaucracy", "roman_law", "trade_networks", "coinage"],

  actor_tags: {
    roman_imperial_tradition: {
      metrics_modifier: { legitimacy: +10, cohesion: +5 },
      spreads_via: []
    }
  },

  on_collapse: [
    { id: "visigoths_kingdom", weight: 0.5 },
    { id: "vandals_africa",    weight: 0.3 },
    { id: "burgundy",          weight: 0.2 }
  ],

  narrative_status: "foreground" | "background",

  scenario_metrics: {        // только для игрового актора
    family_influence: 60,
    family_knowledge: 40
  }
}
```

**Что обязан определить каждый сценарий:**
```
scenario: {
  id: "britain_1760",
  version: "1.0",
  start_year: 1760,
  tempo: 0.7,
  tick_label: "год",
  player_context: {...},
  llm_context: "...",
  consequence_context: "...",
  actors: [...],
  scenario_metrics: [...],
  patron_actions: [...],
  milestone_events: [...],
  rank_conditions: [...]
}
```

**Формат milestone_event:**

Сценарий не заканчивается — симуляция продолжается сколько угодно.
Переломные моменты меняют характер нарратива но не останавливают игру.
Игрок останавливается когда хочет.

```
milestone_event: {
  id: "rome_splits",
  condition: {
    type: "metric" | "actor_state" | "tick",
    metric: "cohesion",       // для type: metric
    actor_id: "rome",
    operator: "<" | ">" | "=",
    value: 30,
    duration: 5               // тиков подряд (опционально)
  },
  is_key: true,               // записывается как is_key событие в хранилище
  triggers_collapse: false,   // если true — запускает on_collapse актора
  llm_context_shift: "Империя раскололась. Западная и Восточная части теперь идут разными путями."
}
```

Milestone events проверяются на шаге 6 порядка операций.

При срабатывании:
- Записывается is_key событие в хранилище
- LLM получает llm_context_shift в следующем промпте
- Если triggers_collapse — запускается on_collapse актора
- Симуляция продолжается

**Что сценарий НЕ определяет:**
- Граф зависимостей между метриками — универсальный
- Автодельты — универсальные
- Механики взаимодействий (торговля, миграция, давление, культура, дипломатия) — универсальные
- Пороги релевантности — универсальные
- Смерть и рождение акторов — универсальные

**Уникальные теги актора** — специфика конкретной цивилизации, не распространяется:
```
actor: {
  id: "ottoman_empire",
  tags: ["heavy_cavalry", "bureaucracy", "janissary_system"],
  actor_tags: {
    janissary_system: {
      metrics_modifier: { military_quality: +20, cohesion: -5 },
      spreads_via: []
    }
  }
}
```

---

### Руководство по созданию сценария

Пошаговый процесс для любого нового сценария:

1. **Эпоха и роль** — кто игрок, в каком году, какая историческая ситуация
2. **Список акторов** — начальные метрики и базовые теги соответствующие эре
3. **География** — соседи, расстояния, тип границ для каждого актора
4. **Исторические наследники** — on_collapse для ключевых акторов
5. **Сценарные метрики роли** — только то что уникально для игрока
6. **Уникальные теги акторов** — специфика цивилизаций не покрытая базовыми тегами
7. **LLM контекст** — llm_context для сценария, consequence_context для последствий
8. **Patron actions и milestone_events** — действия игрока и переломные моменты
9. **Tempo и tick_label** — скорость симуляции для эпохи

### Технологии — теги и эры

Эры открываются через теги — не через год. Как в Цивилизации. Каждый актор имеет свою эру.

**Структура тега:**
```
tag: {
  id: "stirrup",
  metrics_modifier: { military_quality: +15 },
  unlocks: ["heavy_cavalry"],
  spreads_via: ["war", "trade"],
  requires_era: "early_medieval"
}
```

**Структура эры:**
```
era: {
  id: "high_medieval",
  requires_tags: 5,
  from_tags: [
    "heavy_cavalry", "feudalism", "organized_church",
    "crossbow", "coinage", "guilds"
  ],
  auto_delta_modifier: 0.8,  // метрики стабильнее в развитой эре
  unlocks_tags: [            // теги доступные только в этой эре
    "plate_armor", "banking", "codified_law"
  ]
}
```

**Эры (от простого к сложному):**
```
ancient        → требует 0 тегов    (старт)
early_medieval → требует 4 из ancient тегов
high_medieval  → требует 5 из early_medieval тегов
late_medieval  → требует 5 из high_medieval тегов
early_modern   → требует 6 из late_medieval тегов
```

**Базовые теги по категориям:**

Военные:
```
iron_weapons        ancient
heavy_cavalry       early_medieval  spreads_via: war, trade
stirrup             early_medieval  spreads_via: war, trade
crossbow            high_medieval   spreads_via: war, trade
plate_armor         late_medieval   spreads_via: war
gunpowder           late_medieval   spreads_via: trade, war
professional_army   late_medieval   spreads_via: culture
```

Экономические:
```
trade_networks      ancient         spreads_via: trade
coinage             ancient         spreads_via: trade
guilds              early_medieval  spreads_via: culture, trade
banking             high_medieval   spreads_via: trade
printing_press      early_modern    spreads_via: trade, culture
```

Административные:
```
bureaucracy         ancient         spreads_via: culture, conquest
roman_law           ancient         spreads_via: culture
feudalism           early_medieval  spreads_via: war, culture
codified_law        high_medieval   spreads_via: culture
```

Культурные / религиозные:
```
monotheism          ancient         spreads_via: culture, migration
organized_church    early_medieval  spreads_via: culture
crusading_ideal     high_medieval   spreads_via: culture
```

Сценарий может добавлять специфические теги поверх — например ottoman_1526 добавляет janissary_system.

**Что эра даёт актору:**
- Открывает новые теги недоступные ранее
- Модифицирует автодельты — метрики стабильнее
- LLM получает контекст эры и описывает мир соответственно

### Граф зависимостей внутри актора

Применяется на шаге 4 — после автодельт, до взаимодействий. Дельта за тик умножается на коэффициент.

```
legitimacy ↓10 → cohesion ↓3              (коэф 0.3)
cohesion ↓10 → legitimacy ↓2              (коэф 0.2)
legitimacy ↓10 → military_quality ↓2      (коэф 0.2)
cohesion ↓10 → economic_output ↓3         (коэф 0.3)
external_pressure ↑10 → cohesion ↓2       (коэф 0.2)
external_pressure ↑10 → legitimacy ↓1     (коэф 0.1)
external_pressure ↑10 → military_quality ↓2 (коэф 0.2)
external_pressure ↑10 → military_size ↓1  (коэф 0.1)
economic_output ↓10 → treasury ↓15        (коэф 1.5)
military_size ↓10 → economic_output ↓1    (коэф 0.1)
population ↑1000 → economic_output ↑0.5   (коэф 0.0005)
economic_output ↓10 → population ↓200     (коэф 20)
```

**Эффект сплочения — исключение:**
```
если external_pressure выросла на >15 за 1 тик
И legitimacy > 60:
  cohesion += 5  // вместо падения
```

**Пороговые эффекты:**
```
cohesion < 25 → любое падение legitimacy удваивается
legitimacy < 20 → military_quality падает само по себе -0.5/тик
economic_output < 15 → population падает независимо -100/тик
external_pressure > 80 → триггер миграции для соседей
```

Намеренно убрана прямая связь military → legitimacy.
```
доходы:
  economic_output × population × 0.001
  + торговый_прирост × 0.5

расходы:
  military_size × 0.8
  + action_cost если было действие игрока

treasury += доходы - расходы
```
Большая армия при слабой экономике быстро опустошает казну.

### Автономные дельты

Дефолтные значения для всех базовых метрик:

```
population:
  base: +0.3
  conditions:
    economic_output < 20 → -0.5
    external_pressure > 70 → -0.3
    treasury < 0 → -0.2

military_size:
  base: -0.2
  conditions:
    treasury < 0 → -1.0
    external_pressure > 60 → +0.3

military_quality:
  base: -0.1
  conditions:
    treasury > 200 → +0.2
    external_pressure > 70 → -0.3

economic_output:
  base: +0.1
  conditions:
    treasury < 0 → -0.4
    cohesion < 25 → -0.5
    тег trade_networks → +0.2

cohesion:
  base: -0.1
  conditions:
    legitimacy > 70 → +0.1
    economic_output < 20 → -0.4
    external_pressure > 60 → -0.2

legitimacy:
  base: -0.1
  conditions:
    cohesion > 60 → +0.1
    treasury < 0 → -0.3
    military_size < 10 → -0.2

external_pressure:
  base: -0.3   // спадает без источника
  conditions:  // остальное через взаимодействия акторов

treasury:      // считается отдельной формулой
power_projection: // производная
```

Каждая метрика имеет лёгкий естественный спад. Без управления государство медленно деградирует.

Noise — случайный элемент каждый тик в диапазоне ±noise. Обеспечивает реиграбельность:
```
population:        noise: 0.1
military_size:     noise: 0.3
military_quality:  noise: 0.2
economic_output:   noise: 0.4
cohesion:          noise: 0.2
legitimacy:        noise: 0.1
external_pressure: noise: 0.3
treasury:          noise: 0.2
```

Отдельный конфиг — меняешь цифры без изменения кода.

### Взаимодействия между акторами

1. **Торговля** — граничат ИЛИ тег trade_networks:
   ```
   актор А (богаче):
     прирост = economic_output А × 0.05 × расстояние_модификатор
     максимум +5 за тик

   актор Б (беднее):
     прирост = economic_output Б × 0.02 × расстояние_модификатор
     максимум +2 за тик

   расстояние_модификатор:
     distance 1 → 1.0
     distance 2 → 0.7
     distance 3 → 0.4
     distance 4+ → 0.0

   тег trade_networks: снимает ограничение расстояния,
                       бедный получает наравне с богатым
   ```
   Богатый извлекает больше — исторически точно. Бедный выравнивает через trade_networks.
2. **Миграция:**
   ```
   migration_rate:
     external_pressure > 70 → 0.05
     economic_output < 20   → 0.03
     cohesion < 25          → 0.04
     комбинация двух        → 0.08
     все три                → 0.12
   максимум за тик: 15% population
   направление: к соседу с наибольшей stability
   если равны → к ближайшему

   эффект на источник А:
     population -= объём
     military_size -= объём × 0.3
     economic_output -= объём_относительный × 0.2

   эффект на получателя Б:
     population += объём
     cohesion -= объём_относительный × 0.5
     external_pressure += объём_относительный × 0.3

   объём_относительный = объём / population Б
   ```
   Повторяется каждый тик пока условие держится. Большая волна в маленький актор бьёт сильнее.
3. **Военное давление** — вектор силы между акторами:
   ```
   давление = (power_projection атакующего / max(power_projection жертвы, 1))
              × расстояние_модификатор
              × тип_границы_модификатор

   расстояние_модификатор:
     distance 1 → 1.0
     distance 2 → 0.7
     distance 3 → 0.4
     distance 4+ → 0.1

   тип_границы_модификатор:
     land → 1.0
     sea  → 0.5
   ```
   Результат добавляется к external_pressure жертвы.
   Если давление > 0.8 → триггер события "война".
4. **Культурное / религиозное влияние:**
   ```
   cultural_strength А на Б =
     (legitimacy А × 0.4 + cohesion А × 0.3 + economic_output А × 0.3)
     × расстояние_модификатор
     × общий_тег_модификатор

   общий_тег_модификатор:
     общая религия → × 1.5
     общая эра     → × 1.2
     нет общего    → × 1.0

   если cultural_strength А > cultural_strength Б:
     разница = cultural_strength А - cultural_strength Б
     cohesion Б -= разница × 0.1
     legitimacy Б -= разница × 0.05

   если А > Б × 2.0 (подавляющее превосходство):
     оба эффекта × 1.5

   если cohesion Б > 60 (сопротивление):
     оба эффекта × 0.3

   если А > Б × 1.5:
     тег Б может быть вытеснен тегом А
     например: local_religion → monotheism
   ```
   Чем больше разрыв — тем быстрее вытеснение. Сопротивление возможно только при высокой cohesion.
5. **Дипломатия / союзы:**
   ```
   союз создаётся когда:
     общий враг (источник давления > 0.5 для обоих)
     И stability обоих > 40
     И distance ≤ 3

   союз держится пока:
     общий враг существует
     ИЛИ оба выигрывают от торговли между собой

   союз ломается когда:
     общий враг исчез
     ИЛИ cohesion одного < 25
     ИЛИ один начал давить на другого (давление > 0.3)

   эффект союза:
     power_projection обоих × 1.3
     military_size суммируется при общей угрозе
   ```

### Смерть и рождение акторов

**Смерть** — комбинация держится 3+ тика:
```
legitimacy < 10 И cohesion < 15 И external_pressure > 85
```

**Рождение:**
- Раскол — метрики делятся пропорционально population и geography
- Миграция — группа осела в новом месте
- Органический рост — фоновый актор достиг порога

**Наследники:**
- Исторические с весами:
  ```
  on_collapse: [
    { id: "visigoths_kingdom", weight: 0.5 },
    { id: "vandals_africa",    weight: 0.3 },
    { id: "burgundy",          weight: 0.2 }
  ]
  ```
- Фоновые: процедурный раскол с равными весами (1/N), LLM придумывает название

**Формула раскола:**
```
каждый наследник получает долю = weight_i / sum(weights)

population:       × доля
military_size:    × доля × 0.7   // потери при расколе
treasury:         × доля × 0.5   // разграбление
military_quality: родитель × 0.8  // деградация
economic_output:  родитель × 0.7
cohesion:         старт 20        // раскол это травма
legitimacy:       старт 30        // новая власть не устоялась
external_pressure: родитель × 1.3 // враги чувствуют слабость
```

---

## Порядок операций за тик

Фиксированный порядок — менять нельзя, влияет на результат симуляции.

```
1.  Собрать действие игрока если есть
2.  Применить действие игрока к метрикам
3.  Автономные дельты — каждый актор меняет метрики сам по себе
4.  Граф зависимостей — пересчитать метрики с учётом связей
5.  Взаимодействия между акторами:
    5а. Торговля
    5б. Культурное влияние
    5в. Дипломатия / союзы
    5г. Военное давление
    5д. Миграция (последней — зависит от результатов давления)
6.  Проверить пороговые эффекты, rank_conditions и milestone_events
7.  Проверить смерть акторов
8.  Проверить рождение акторов
9.  Проверить пороги релевантности
10. Записать новые события в индексированное хранилище
11. Проверить триггеры LLM
12. Сохранить снапшот если нужно
```

**Логика порядка:**
- Действие игрока первым — влияет на весь тик
- Миграция последней из взаимодействий — она результат давления, не причина
- Смерть до рождения — сначала коллапс, потом наследники
- Релевантность после всех изменений — проверяем финальное состояние тика

---

## География

**Ранги регионов** — универсальная механика для всех сценариев. Шкала D → C → B → A → S.

```
trade_bonus по рангу:
  S → +25% economic_output
  A → +15%
  B → +8%
  C → +0%
  D → -10%

legitimacy_bonus:
  S — особый статус. Актор контролирующий регион S получает +20 legitimacy глобально
  Пример: Рим S → кто держит Рим, тот легитимнее
```

Начальные ранги и условия изменения определяются в каждом сценарии через `rank_conditions`.

**Примеры рангов Rome 375:**
```
Milan (Медиолан): A  — фактическая столица Запада
Rome:             S  — символический центр, legitimacy +20
Carthage:         B  — западный торговый узел
Alexandria:       B  — интеллектуальный центр
Constantinople:   A  — столица Востока
Steppe:           D  — базово, растёт с размером орды
```

**rank_conditions** — в структуре сценария:
```
rank_conditions: [
  {
    region_id: "carthage",
    condition: { metric: "economic_output", actor_id: "carthage", operator: ">", value: 70 },
    result: { rank: "A" },
    is_key: true
  },
  {
    region_id: "steppe",
    condition: { metric: "military_size", actor_id: "huns", operator: ">", value: 200 },
    result: { rank: "C" }
  },
  {
    region_id: "steppe",
    condition: { metric: "military_size", actor_id: "huns", operator: ">", value: 400 },
    result: { rank: "B" }
  }
]
```

Проверяется на шаге 6 порядка операций — вместе с пороговыми эффектами.

**Полная структура актора с region_rank:**
```
actor: {
  id: "rome",
  name: "Римская Империя",
  name_short: "Рим",
  region: "mediterranean",
  region_rank: "S",              // D | C | B | A | S
  center: { lat: 41.9, lng: 12.5 },
  neighbors: [
    { id: "goths", distance: 2, border_type: "land" },
    { id: "carthage", distance: 3, border_type: "sea" }
  ]
}
```

- Симуляция работает через граф соседства
- Координаты хранятся — основа для будущей карты
- border_type: land / sea — морская замедляет военное давление но не торговлю

---

## Время

Триггерная модель — не real-time.

**Цикл:** игрок видит мир → принимает решение → тик → движок считает → LLM если нужно → игрок читает.

```
scenario: {
  tempo: 1.0,
  tick_label: "год"
}
```

Rome 375 → tempo 1.5 | Britain 1760 → tempo 0.7

---

## Механика поколений (опциональный модуль)

Активируется если сценарий определяет `generation_mechanics: true`.
Используется когда игрок управляет семьёй или династией а не государством напрямую.

```
generation_mechanics: {
  enabled: true,
  head_metric: "patriarch",     // имя персонажа в scenario_metrics
  start_age: 42,
  tick_span_years: 5,           // лет за тик

  transfer_trigger: {
    age: 75,                    // обычная передача
    early: {                    // ранняя при кризисе
      age: 65,
      condition: { metric: "external_pressure", actor_id: "rome", operator: ">", value: 70 }
    }
  },

  inheritance: {                // коэффициенты наследования метрик
    // задаются в сценарии — пример для Rome 375:
    family_influence:    0.85
    family_knowledge:    1.0
    family_wealth:       1.0
    family_connections:  0.8
  },

  modifiers: [
    // бонусы и штрафы при передаче — задаются в сценарии
  ]
}
```

При срабатывании:
- LLM генерирует сцену передачи власти
- is_key событие записывается в хранилище
- generation счётчик растёт
- Новый глава получает случайный возраст и новые теги от LLM
- Метрики наследуются с коэффициентами

---

## Взаимодействие игрока

Динамические patron_actions — список генерируется движком каждый тик.

```
action: {
  id: "hire_mercenaries",
  name: "Нанять наёмников",
  available_if: "treasury > 100 AND external_pressure > 40",
  effects: {
    military_size: +20,
    military_quality: +10
  },
  cost: {
    treasury: -80,
    legitimacy: -5
  }
}
```

Cost — прямые изменения метрик при выборе действия. Применяются на шаге 2 порядка операций. В treasury delta: `action_cost = action.cost.treasury если действие было, иначе 0`.

**Универсальные действия** — для режимов consequences и free:
```
observe:
  name: "Наблюдать"
  available_if: always
  effects: {}
  cost: {}

support_stability:
  name: "Поддержать стабильность"
  available_if: treasury > 50
  effects: { cohesion: +3, legitimacy: +2 }
  cost: { treasury: -50 }

raise_taxes:
  name: "Поднять налоги"
  available_if: always
  effects: { treasury: +80, legitimacy: -5, cohesion: -3 }
  cost: {}

recruit_soldiers:
  name: "Набрать солдат"
  available_if: treasury > 100
  effects: { military_size: +10, military_quality: -5 }
  cost: { treasury: -100 }
```

---

## Режимы игры

**scenario** — основной режим:
- Все механики работают полностью
- Доступны сценарные patron_actions
- milestone_events активны
- LLM использует llm_context сценария

**consequences** — после срабатывания milestone с triggers_collapse:
- Симуляция мира продолжается полностью
- Сценарные patron_actions заменяются лёгкими универсальными (3-4 действия)
- Игрок становится наблюдателем с ограниченным влиянием
- LLM использует consequence_context
- Длится пока игрок не перейдёт в free вручную

**free** — свободная симуляция:
- Симуляция мира продолжается полностью
- Только универсальные patron_actions
- Нет фиксированной роли — нарратив от третьего лица
- LLM получает только world_state без сценарного контекста
- Бесконечно

**Переходы:**
```
scenario → consequences: при срабатывании milestone с triggers_collapse: true
                         ИЛИ вручную игроком
consequences → free:     вручную игроком
free → ничего:           финальное состояние
```

Игрок может остановить симуляцию в любой момент в любом режиме.

---

## LLM

**Три триггера:**
- Действие игрока — всегда
- Пороговое событие — актор пересёк порог, вышел на передний план, миграция затронула игровую зону
- Временной — каждые 5 тиков

**Структура промпта по триггеру:**

Триггер 1 — действие игрока:
```
trigger_type: "player_action"
game_mode: "scenario" | "consequences" | "free"
world_snapshot: [...]         // нарративные акторы
actor_deltas: [...]           // что изменилось за тик
relevant_events: [...]        // топ 15 + последние 5 + is_key
pending_action: {
  id, name, effects, cost     // что сделал игрок
}
llm_context: "..."
player_context: "..."
```

Триггер 2 — пороговое событие:
```
trigger_type: "threshold_event"
game_mode: "scenario" | "consequences" | "free"
world_snapshot: [...]
actor_deltas: [...]
relevant_events: [...]
threshold_context: {
  actor_id: "goths"
  actor_name: "Готы"
  event_type: "relevance_gained" | "metric_threshold" | "migration"
  description: "Готы пересекли Дунай"
}
llm_context: "..."
```

Триггер 3 — временной:
```
trigger_type: "time"
game_mode: "scenario" | "consequences" | "free"
world_snapshot: [...]
actor_deltas: [...]           // суммарные дельты за 5 тиков
relevant_events: [...]
ticks_since_last: 5
llm_context: "..."
```

Фоновые акторы не попадают в промпт пока не вышли на передний план.

---

## Схема базы данных

```sql
-- акторы
CREATE TABLE actors (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  name_short TEXT,
  scenario_id TEXT NOT NULL,
  is_alive BOOLEAN DEFAULT TRUE,
  narrative_status TEXT DEFAULT 'background',
  era TEXT NOT NULL,
  region_rank TEXT DEFAULT 'C',  -- D | C | B | A | S
  data JSON NOT NULL
);

-- ранги регионов (текущие)
CREATE TABLE region_ranks (
  region_id TEXT NOT NULL,
  scenario_id TEXT NOT NULL,
  rank TEXT NOT NULL,
  changed_at_tick INTEGER,
  PRIMARY KEY (region_id, scenario_id)
);

-- метрики (снапшот текущего состояния)
CREATE TABLE actor_metrics (
  actor_id TEXT PRIMARY KEY,
  tick INTEGER NOT NULL,
  population REAL,
  military_size REAL,
  military_quality REAL,
  economic_output REAL,
  cohesion REAL,
  legitimacy REAL,
  external_pressure REAL,
  treasury REAL,
  FOREIGN KEY (actor_id) REFERENCES actors(id)
);

-- события
CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tick INTEGER NOT NULL,
  year INTEGER NOT NULL,
  actor_id TEXT NOT NULL,
  type TEXT NOT NULL,
  is_key BOOLEAN DEFAULT FALSE,
  description TEXT,
  involved_actors JSON,
  metrics_snapshot JSON,
  tags JSON
);

CREATE INDEX idx_events_actor ON events(actor_id);
CREATE INDEX idx_events_tick ON events(tick);
CREATE INDEX idx_events_type ON events(type);
CREATE INDEX idx_events_key ON events(is_key);

-- мёртвые акторы
CREATE TABLE dead_actors (
  id TEXT PRIMARY KEY,
  tick_death INTEGER NOT NULL,
  year_death INTEGER NOT NULL,
  final_metrics JSON NOT NULL,
  successor_ids JSON
);

-- сохранения
CREATE TABLE saves (
  id TEXT PRIMARY KEY,
  slot TEXT NOT NULL,
  tick INTEGER NOT NULL,
  year INTEGER NOT NULL,
  scenario_id TEXT NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  world_state JSON NOT NULL,
  player_state JSON NOT NULL,
  config_versions JSON NOT NULL
);

-- союзы (текущие активные)
CREATE TABLE alliances (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  actor_a TEXT NOT NULL,
  actor_b TEXT NOT NULL,
  formed_tick INTEGER NOT NULL,
  common_enemy TEXT
);
```

---

## Tauri команды

Контракт между React фронтендом и Rust бэкендом. Фронтенд только вызывает команды и отображает результат — вся логика в Rust.

```
// Симуляция
get_world_state()
  → { actors, tick, year, game_mode }

advance_tick(action?: PlayerAction)
  → { world_state, events, llm_trigger? }
  // llm_trigger присутствует если нужна генерация нарратива

get_narrative_actors()
  → Actor[]

// Действия игрока
get_available_actions()
  → Action[]

submit_action(action_id: string)
  → { effects, new_state, llm_trigger? }

// LLM
generate_narrative(trigger: LlmTrigger)
  → { text: string }
  // вызывается отдельно после advance_tick — симуляция не ждёт LLM

// Сохранение
save_game(slot?: string)
  → { success: bool }

load_game(save_id: string)
  → { world_state, player_state }

list_saves()
  → Save[]

// История
get_relevant_events(actor_ids: string[], current_tick: int)
  → Event[]

// Сценарий
load_scenario(scenario_id: string)
  → { success: bool }

get_scenario_list()
  → Scenario[]
```

**Принцип:** `advance_tick` и `generate_narrative` разделены — UI не блокируется пока LLM генерирует текст. Фронтенд показывает новое состояние мира сразу, нарратив подгружается отдельно.
