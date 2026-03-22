# СЦЕНАРИЙ: ROME 375 — СЕМЬЯ ДИ МИЛАНО

## Акторы и начальные метрики

### 1. Римская Империя (игровой актор)
```
id: "rome"
name: "Римская Империя"
name_short: "Рим"
region: "mediterranean"
region_rank: "S"
era: "ancient"
narrative_status: "foreground"
tags: ["bureaucracy", "roman_law", "trade_networks", "coinage", "christianity"]

metrics:
  population:        8000   // тысяч — ~60-70млн исторически
  military_size:     350    // тысяч — легионы + федераты
  military_quality:  58     // деградация со времён принципата
  economic_output:   48     // высокая коррупция, инфляция
  cohesion:          42     // внутренние противоречия
  legitimacy:        62     // династия Валентиниана
  external_pressure: 38     // пока гунны далеко
  treasury:          1800

scenario_metrics:
  family_influence:    8
  family_knowledge:   12
  family_wealth:      22
  family_connections: 15

on_collapse:
  - { id: "rome_west", weight: 0.45 }
  - { id: "rome_east", weight: 0.55 }
```

### 2. Гунны
```
id: "huns"
name: "Гунны"
name_short: "Гунны"
region: "steppe"
region_rank: "D"
era: "ancient"
narrative_status: "foreground"
tags: ["nomadic", "cavalry", "raid_economy", "pastoral"]

metrics:
  population:        800    // очень мобильны, численность неточная
  military_size:     120    // но качество исключительное
  military_quality:  88     // лучшая конница эпохи
  economic_output:   15     // кочевая экономика — грабёж
  cohesion:          72     // сильная племенная связь
  legitimacy:        60     // власть вождя через силу
  external_pressure: 5      // никто не давит на гуннов
  treasury:          80     // добыча, скот

on_collapse: []  // гунны не имеют наследников в 375
```

### 3. Вестготы
```
id: "visigoths"
name: "Вестготы"
name_short: "Вестготы"
region: "balkans"
region_rank: "C"
era: "ancient"
narrative_status: "foreground"
tags: ["tribal_confederation", "christianity_arian", "federati_potential"]

metrics:
  population:        400
  military_size:     48
  military_quality:  62
  economic_output:   22
  cohesion:          52
  legitimacy:        55     // власть Фритигерна
  external_pressure: 65     // гунны давят с востока
  treasury:          40

on_collapse:
  - { id: "visigoth_kingdom", weight: 1.0 }
```

### 4. Остготы
```
id: "ostrogoths"
name: "Остготы"
name_short: "Остготы"
region: "pontic_steppe"
region_rank: "C"
era: "ancient"
narrative_status: "foreground"
tags: ["tribal_confederation", "steppe_adjacent"]

metrics:
  population:        350
  military_size:     55
  military_quality:  65
  economic_output:   18
  cohesion:          60
  legitimacy:        58
  external_pressure: 78     // гунны давят сильнее чем на вестготов
  treasury:          30

on_collapse:
  - { id: "ostrogoth_kingdom", weight: 1.0 }
```

### 5. Сасанидская Персия
```
id: "sassanids"
name: "Сасанидская Персия"
name_short: "Персия"
region: "mesopotamia"
region_rank: "A"
era: "ancient"
narrative_status: "background"
tags: ["bureaucracy", "zoroastrianism", "silk_road", "cavalry_heavy"]

metrics:
  population:        3000
  military_size:     200
  military_quality:  72
  economic_output:   62
  cohesion:          65
  legitimacy:        75     // Шапур II только умер, Ардашир II
  external_pressure: 30
  treasury:          900

on_collapse:
  - { id: "late_sassanids", weight: 1.0 }
```

### 6. Вандалы
```
id: "vandals"
name: "Вандалы"
name_short: "Вандалы"
region: "dacia"
region_rank: "C"
era: "ancient"
narrative_status: "background"
tags: ["tribal_confederation", "christianity_arian", "migrating"]

metrics:
  population:        180
  military_size:     28
  military_quality:  60
  economic_output:   20
  cohesion:          58
  legitimacy:        52
  external_pressure: 55
  treasury:          25

on_collapse:
  - { id: "vandal_kingdom_africa", weight: 1.0 }
```

### 7. Бургунды
```
id: "burgundians"
name: "Бургунды"
name_short: "Бургунды"
region: "rhine"
region_rank: "C"
era: "ancient"
narrative_status: "background"
tags: ["tribal_confederation", "rhine_border"]

metrics:
  population:        120
  military_size:     18
  military_quality:  55
  economic_output:   22
  cohesion:          62
  legitimacy:        58
  external_pressure: 35
  treasury:          20
```

### 8. Франки
```
id: "franks"
name: "Франки"
name_short: "Франки"
region: "gaul_north"
region_rank: "C"
era: "ancient"
narrative_status: "background"
tags: ["tribal_confederation", "rhine_border", "roman_contact"]

metrics:
  population:        200
  military_size:     30
  military_quality:  58
  economic_output:   25
  cohesion:          55
  legitimacy:        50
  external_pressure: 25
  treasury:          30

on_collapse:
  - { id: "frankish_kingdom", weight: 1.0 }
```

### 9. Саксы
```
id: "saxons"
name: "Саксы"
name_short: "Саксы"
region: "germania_north"
region_rank: "D"
era: "ancient"
narrative_status: "background"
tags: ["tribal_confederation", "seafaring", "raid_economy"]

metrics:
  population:        150
  military_size:     20
  military_quality:  55
  economic_output:   18
  cohesion:          60
  legitimacy:        48
  external_pressure: 15
  treasury:          15
```

### 10. Аламанны
```
id: "alamanni"
name: "Аламанны"
name_short: "Аламанны"
region: "rhine_upper"
region_rank: "C"
era: "ancient"
narrative_status: "background"
tags: ["tribal_confederation", "rhine_border"]

metrics:
  population:        180
  military_size:     28
  military_quality:  60
  economic_output:   20
  cohesion:          58
  legitimacy:        52
  external_pressure: 30
  treasury:          22
```

### 11. Берберы
```
id: "berbers"
name: "Берберские племена"
name_short: "Берберы"
region: "north_africa"
region_rank: "C"
era: "ancient"
narrative_status: "background"
tags: ["tribal_confederation", "desert_warfare", "roman_frontier"]

metrics:
  population:        300
  military_size:     35
  military_quality:  55
  economic_output:   28
  cohesion:          45
  legitimacy:        42
  external_pressure: 20
  treasury:          35
```

### 12. Армения
```
id: "armenia"
name: "Армения"
name_short: "Армения"
region: "caucasus"
region_rank: "C"
era: "ancient"
narrative_status: "background"
tags: ["buffer_state", "christianity", "persian_border", "roman_border"]

metrics:
  population:        500
  military_size:     40
  military_quality:  58
  economic_output:   35
  cohesion:          55
  legitimacy:        60
  external_pressure: 55     // между Римом и Персией
  treasury:          120
```

### 13. Кушанское царство
```
id: "kushans"
name: "Кушанское царство"
name_short: "Кушаны"
region: "bactria"
region_rank: "B"
era: "ancient"
narrative_status: "background"
tags: ["silk_road", "buddhism", "trade_networks", "declining"]

metrics:
  population:        800
  military_size:     60
  military_quality:  55
  economic_output:   45     // контроль части Шёлкового пути
  cohesion:          40     // распад начался
  legitimacy:        45
  external_pressure: 50     // Гупты давят с юга
  treasury:          300
```

### 14. Гуптская империя
```
id: "guptas"
name: "Гуптская империя"
name_short: "Гупты"
region: "india"
region_rank: "A"
era: "ancient"
narrative_status: "background"
tags: ["silk_road", "hinduism", "trade_networks", "golden_age"]

metrics:
  population:        4000
  military_size:     180
  military_quality:  65
  economic_output:   70     // золотой век — пик индийской цивилизации
  cohesion:          72
  legitimacy:        78     // Самудрагупта — великий правитель
  external_pressure: 15
  treasury:          1200
```

### 15. Восточная Цзинь (Китай)
```
id: "eastern_jin"
name: "Восточная Цзинь"
name_short: "Китай"
region: "china"
region_rank: "A"
era: "ancient"
narrative_status: "background"
tags: ["silk_road", "confucianism", "trade_networks", "southern_exile"]

metrics:
  population:        5000   // только южный Китай
  military_size:     150
  military_quality:  55     // слабее чем в период расцвета
  economic_output:   58
  cohesion:          45     // нестабильность, северный Китай потерян
  legitimacy:        55
  external_pressure: 40     // северные варварские государства
  treasury:          800
```

---

## Граф соседства

```
rome ↔ visigoths      distance: 2, border: land
rome ↔ ostrogoths     distance: 3, border: land
rome ↔ sassanids      distance: 3, border: land (через Армению)
rome ↔ vandals        distance: 2, border: land
rome ↔ burgundians    distance: 2, border: land
rome ↔ franks         distance: 2, border: land
rome ↔ alamanni       distance: 2, border: land
rome ↔ saxons         distance: 3, border: sea
rome ↔ berbers        distance: 2, border: sea
rome ↔ armenia        distance: 2, border: land

huns ↔ ostrogoths     distance: 1, border: land   // прямой контакт
huns ↔ visigoths      distance: 2, border: land
huns ↔ eastern_jin    distance: 4, border: land    // слишком далеко — нет взаимодействия

visigoths ↔ ostrogoths distance: 2, border: land
visigoths ↔ rome       distance: 2, border: land
visigoths ↔ burgundians distance: 2, border: land

sassanids ↔ armenia    distance: 1, border: land
sassanids ↔ rome       distance: 3, border: land
sassanids ↔ kushans    distance: 2, border: land
sassanids ↔ guptas     distance: 3, border: land

kushans ↔ guptas       distance: 2, border: land
kushans ↔ eastern_jin  distance: 3, border: land
kushans ↔ sassanids    distance: 2, border: land

guptas ↔ kushans       distance: 2, border: land
guptas ↔ eastern_jin   distance: 3, border: sea

eastern_jin ↔ guptas   distance: 3, border: sea
eastern_jin ↔ kushans  distance: 3, border: land
```

**Шёлковый путь — цепочка:**
eastern_jin → kushans → sassanids → rome
Торговля течёт по этой цепочке. Каждый посредник берёт долю.

---

## Region ranks

```
Rome (город):          S  — legitimacy +20 глобально
Milan (Медиолан):      A  — trade +15%, governance +12%
Constantinople:        A  — (Восток, не игровой)
Carthage:              B
Alexandria:            B
Antioch:               B
Balkans:               C
Steppe:                D  — базово
Bactria (Кушаны):      B
India (Гупты):         A
China:                 A
```

---

## rank_conditions

```
rank_conditions: [
  // Степь растёт с размером гуннской орды
  {
    region_id: "steppe",
    condition: { metric: "military_size", actor_id: "huns", operator: ">", value: 150 },
    result: { rank: "C" }
  },
  {
    region_id: "steppe",
    condition: { metric: "military_size", actor_id: "huns", operator: ">", value: 300 },
    result: { rank: "B" }
  },

  // Рим теряет символический статус при коллапсе
  {
    region_id: "rome_city",
    condition: { metric: "legitimacy", actor_id: "rome", operator: "<", value: 20 },
    result: { rank: "A" },  // падает с S до A
    is_key: true
  },

  // Медиолан падает если Рим коллапсирует
  {
    region_id: "milan",
    condition: { actor_state: "dead", actor_id: "rome" },
    result: { rank: "B" }
  }
]
```

---

## milestone_events (вместо end_conditions)

Сценарий не заканчивается — симуляция продолжается сколько угодно.
Переломные моменты меняют характер нарратива но не останавливают игру.
Игрок останавливается когда хочет.

```
milestone_events: [

  // Семья поднялась
  {
    id: "family_rises",
    condition: { metric: "family_influence", operator: ">=", value: 60 },
    is_key: true,
    llm_context_shift: "Семья Ди Милано стала одной из значимых сил города. Их больше не игнорируют."
  },

  // Рим раскололся
  {
    id: "rome_splits",
    condition: { metric: "cohesion", actor_id: "rome", operator: "<", value: 30, duration: 5 },
    is_key: true,
    triggers_collapse: true,  // запускает on_collapse Рима
    llm_context_shift: "Империя раскололась. Западная и Восточная части теперь идут разными путями."
  },

  // Адрианополь
  {
    id: "adrianople",
    condition: { metric: "external_pressure", actor_id: "rome", operator: ">", value: 85, duration: 3 },
    is_key: true,
    llm_context_shift: "Готы перешли черту. Адрианополь. Валент мёртв. Мир изменился навсегда."
  },

  // Гунны стали реальной угрозой
  {
    id: "huns_visible",
    condition: { metric: "military_size", actor_id: "huns", operator: ">", value: 200 },
    is_key: true,
    llm_context_shift: "Гунны больше не слухи. Их видели у Дуная. Паника нарастает."
  },

  // Семья потеряла позиции
  {
    id: "family_falls",
    condition: { metric: "family_influence", operator: "<", value: 5 },
    is_key: true,
    llm_context_shift: "Семья Ди Милано потеряла всё что нажила. Они снова никто."
  }
]
```

---

## patron_actions (Rome 375)

Нейтральные действия — описывают ЧТО делает семья, не КАК.
LLM интерпретирует контекстуально исходя из состояния мира.

**Действия семьи:**
```
expand_network:
  name: "Расширить связи"
  available_if: family_wealth > 10
  effects: { family_connections: +6 }
  cost: { family_wealth: -4 }

gather_information:
  name: "Собрать информацию"
  available_if: always
  effects: { family_knowledge: +6 }
  cost: { family_wealth: -2 }

invest_wealth:
  name: "Вложить средства"
  available_if: family_wealth > 20
  effects: { family_wealth: +8 }
  cost: { family_connections: -2 }

build_reputation:
  name: "Укрепить репутацию"
  available_if: family_connections > 15
  effects: { family_influence: +6 }
  cost: { family_wealth: -5 }

educate_family:
  name: "Образование семьи"
  available_if: family_wealth > 10
  effects: { family_knowledge: +10 }
  cost: { family_wealth: -6 }

lay_low:
  name: "Затаиться"
  available_if: always
  effects: { family_wealth: +3 }
  cost: { family_influence: -2 }
```

**Действия через Медиолан — укрепляют Рим:**
```
support_city:
  name: "Поддержать город"
  available_if: family_wealth > 15
  effects: { family_influence: +4, rome.economic_output: +2, rome.cohesion: +1 }
  cost: { family_wealth: -8 }
  // LLM решает как — церковь, акведук, зерновые склады

back_administration:
  name: "Поддержать администрацию"
  available_if: family_connections > 15
  effects: { family_connections: +5, rome.legitimacy: +2 }
  cost: { family_wealth: -6 }
  // LLM решает как — двор, чиновники, петиции

fund_defense:
  name: "Вложить в оборону"
  available_if: family_wealth > 20
  effects: { family_influence: +3, rome.military_quality: +2 }
  cost: { family_wealth: -10 }
  // LLM решает как — снаряжение, наёмники, укрепления
```

---

## llm_context

```
СЦЕНАРИЙ: Рим 375 — Семья Ди Милано
РОЛЬ ИГРОКА: Глава незаметной семьи в Медиолане. Не Валент, не Амброзий. Человек который видит.

КОНТЕКСТ:
375 год. Медиолан — фактическая столица Западной Империи.
Гунны за горизонтом давят на готов. Готы просятся за Дунай.
Через три года Адрианополь. Но это ещё не случилось.
Гунны в 375 году — слухи на краю ойкумены, не факт.

СЕМЬЯ ДИ МИЛАНО:
Никто не знает кто они. Незаметные. Читающие. Осторожные.
Если Рим выстоит — семья поднимается. Но начинают с нуля.

МЕТРИКИ СЕМЬИ:
family_influence (0-100): политический вес в городе и при дворе
family_knowledge (0-100): накопленная учёность, архивы, контакты
family_wealth (0-100): финансовая база, торговые связи
family_connections (0-100): сеть людей которые тебе должны

МЕХАНИКА ПОКОЛЕНИЙ:
2 тика = 1 год. Главы семьи сменяются.
Patriarch начинает в 42 года. При ~75 — передача власти.
Новый глава наследует метрики семьи, характер генерируется заново.

ТРИ ПУТИ:
1. Рим выстоял — германо-римский синтез, семья в новой элите
2. Классический распад — семья выживает в хаосе варварских королевств
3. Катастрофа — семья теряется

ТОНАЛЬНОСТЬ:
Поздняя античность. Латынь живая. Христианство новое но уже власть.
Рим ещё существует — но что-то изменилось, люди это чувствуют.
Нарратив от третьего лица, через конкретные сцены жизни семьи.
Имена персонажей латинские. 3–5 абзацев за тик.

НЕ ДЕЛАТЬ:
- Не предрешать падение Рима
- Не игнорировать масштаб семьи — они малые люди в большой истории
- Гунны в 375 году невидимы для большинства
```

---

## consequence_context

```
Сценарный период завершён. Симуляция продолжается.
Семья Ди Милано пережила первый кризис — или не пережила.
Нарратив охватывает более широкий период истории.
Роль игрока — наблюдатель с ограниченным влиянием.
Семья продолжает существовать в том мире который сложился.
```
