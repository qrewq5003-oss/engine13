# Перепись насыщений — A46, стадия 1

Только замер, правок нет. Вывод `src/bin/a46_probe.rs` (фича `census`), 30 сидов × 300 тиков,
все миры трёх сценариев (rome: без игрока, balanced, influence, wealth; constantinople: без
игрока, balanced, diplomacy, military; milan: без игрока, aggressive), только живые тики.
Сводка и разбор — `docs/TRIAGE.md`, раздел «A46, стадия 1». Ниже — вывод пробы без правок.

- «у потолка» — значение ≥ 99, «у пола» — ≤ 1, после тика; потолок считается только для метрик
  на шкале 0..100 (`legitimacy`, `cohesion`, `military_quality`, `economic_output`,
  `external_pressure`, все семейные и глобальные); у `military_size` и `population` есть только
  пол, у `treasury` границ нет.
- дрейф — среднее изменение за живой тик в фазах: первые 50 тиков / 50–249 / 250–299.
- потери на клампе — доля запрошенного притока, срезанная у потолка, и доля запрошенного оттока,
  срезанная у пола (каждая запись в метрику с запрошенной и применённой дельтой; раскол Рима —
  структурная перезапись, в запрошенное не входит).
- полнота записи проверена сверкой: сумма применённых дельт = изменение за тик.

# A46 stage 1 — saturation census, 30 seeds × 300 ticks per world

completeness: 0 of 8762626 container-metric-ticks where landed writes ≠ change (worst 3.64e-12)

## Summary: more than half of the living ticks at the ceiling (≥ 99) or the floor (≤ 1), pooled over the worlds

| scenario | carrier | metric | scale | at ceiling | at floor | per world (ceiling / floor) |
|---|---|---|---|---|---|---|
| constantinople_1430 | byzantium | economic_output | 0..100 | 94.0 % | 0.0 % | balanced 95/0 · diplomacy 96/0 · military 95/0 · none 77/0 |
| constantinople_1430 | byzantium | external_pressure | 0..100 | 63.1 % | 13.4 % | balanced 74/9 · diplomacy 78/9 · military 31/25 · none 90/0 |
| constantinople_1430 | genoa | cohesion | 0..100 | 83.1 % | 0.0 % | balanced 85/0 · diplomacy 86/0 · military 79/0 · none 82/0 |
| constantinople_1430 | genoa | economic_output | 0..100 | 97.6 % | 0.0 % | balanced 98/0 · diplomacy 98/0 · military 98/0 · none 97/0 |
| constantinople_1430 | genoa | external_pressure | 0..100 | 85.2 % | 0.0 % | balanced 87/0 · diplomacy 86/0 · military 83/0 · none 84/0 |
| constantinople_1430 | genoa | legitimacy | 0..100 | 0.0 % | 55.5 % | balanced 0/56 · diplomacy 0/57 · military 0/55 · none 0/54 |
| constantinople_1430 | hungary | economic_output | 0..100 | 96.0 % | 0.0 % | balanced 96/0 · diplomacy 96/0 · military 95/0 · none 96/0 |
| constantinople_1430 | hungary | external_pressure | 0..100 | 94.4 % | 0.0 % | balanced 95/0 · diplomacy 95/0 · military 91/0 · none 96/0 |
| constantinople_1430 | hungary | legitimacy | 0..100 | 0.0 % | 50.9 % | balanced 0/51 · diplomacy 0/53 · military 0/37 · none 0/58 |
| constantinople_1430 | mamluks | cohesion | 0..100 | 58.8 % | 0.3 % | balanced 55/0 · diplomacy 48/1 · military 71/0 · none 32/0 |
| constantinople_1430 | mamluks | economic_output | 0..100 | 96.5 % | 0.0 % | balanced 96/0 · diplomacy 96/0 · military 97/0 · none 95/0 |
| constantinople_1430 | mamluks | external_pressure | 0..100 | 69.5 % | 0.9 % | balanced 65/3 · diplomacy 65/0 · military 76/0 · none 49/0 |
| constantinople_1430 | milan | cohesion | 0..100 | 70.8 % | 0.0 % | balanced 72/0 · diplomacy 72/0 · military 68/0 · none 71/0 |
| constantinople_1430 | milan | economic_output | 0..100 | 97.4 % | 0.0 % | balanced 97/0 · diplomacy 97/0 · military 98/0 · none 98/0 |
| constantinople_1430 | milan | external_pressure | 0..100 | 74.5 % | 0.0 % | balanced 75/0 · diplomacy 75/0 · military 70/0 · none 77/0 |
| constantinople_1430 | ottomans | economic_output | 0..100 | 97.3 % | 0.0 % | balanced 97/0 · diplomacy 97/0 · military 96/0 · none 98/0 |
| constantinople_1430 | ottomans | military_quality | 0..100 | 81.5 % | 11.6 % | balanced 88/6 · diplomacy 85/9 · military 52/35 · none 90/5 |
| constantinople_1430 | papacy | cohesion | 0..100 | 77.6 % | 0.0 % | balanced 78/0 · diplomacy 78/0 · military 77/0 · none 78/0 |
| constantinople_1430 | papacy | economic_output | 0..100 | 97.0 % | 0.0 % | balanced 97/0 · diplomacy 97/0 · military 97/0 · none 97/0 |
| constantinople_1430 | papacy | external_pressure | 0..100 | 88.2 % | 0.0 % | balanced 88/0 · diplomacy 88/0 · military 88/0 · none 88/0 |
| constantinople_1430 | papacy | legitimacy | 0..100 | 0.0 % | 60.2 % | balanced 0/59 · diplomacy 0/62 · military 0/60 · none 0/60 |
| constantinople_1430 | poland_lithuania | cohesion | 0..100 | 66.1 % | 0.0 % | balanced 63/0 · diplomacy 64/0 · military 74/0 · none 63/0 |
| constantinople_1430 | poland_lithuania | economic_output | 0..100 | 97.5 % | 0.0 % | balanced 98/0 · diplomacy 97/0 · military 98/0 · none 97/0 |
| constantinople_1430 | poland_lithuania | external_pressure | 0..100 | 74.7 % | 0.0 % | balanced 71/0 · diplomacy 72/0 · military 87/0 · none 68/0 |
| constantinople_1430 | poland_lithuania | military_quality | 0..100 | 54.6 % | 32.9 % | balanced 57/31 · diplomacy 57/31 · military 45/42 · none 59/28 |
| constantinople_1430 | serbia | cohesion | 0..100 | 63.1 % | 0.0 % | balanced 66/0 · diplomacy 65/0 · military 59/0 · none 63/0 |
| constantinople_1430 | serbia | economic_output | 0..100 | 95.7 % | 0.0 % | balanced 96/0 · diplomacy 96/0 · military 95/0 · none 96/0 |
| constantinople_1430 | serbia | external_pressure | 0..100 | 91.0 % | 0.0 % | balanced 91/0 · diplomacy 93/0 · military 83/0 · none 96/0 |
| constantinople_1430 | serbia | legitimacy | 0..100 | 0.0 % | 67.2 % | balanced 0/68 · diplomacy 0/69 · military 0/64 · none 0/68 |
| constantinople_1430 | serbia | military_quality | 0..100 | 27.0 % | 57.9 % | balanced 27/56 · diplomacy 26/57 · military 30/55 · none 25/63 |
| constantinople_1430 | trebizond | cohesion | 0..100 | 84.1 % | 0.0 % | balanced 86/0 · diplomacy 86/0 · military 80/0 · none 84/0 |
| constantinople_1430 | trebizond | economic_output | 0..100 | 95.8 % | 0.0 % | balanced 96/0 · diplomacy 96/0 · military 96/0 · none 96/0 |
| constantinople_1430 | trebizond | external_pressure | 0..100 | 91.0 % | 0.0 % | balanced 93/0 · diplomacy 91/0 · military 92/0 · none 89/0 |
| constantinople_1430 | trebizond | legitimacy | 0..100 | 0.0 % | 63.8 % | balanced 0/64 · diplomacy 0/64 · military 0/63 · none 0/64 |
| constantinople_1430 | trebizond | military_quality | 0..100 | 29.7 % | 55.5 % | balanced 31/51 · diplomacy 30/53 · military 31/57 · none 26/61 |
| constantinople_1430 | venice | cohesion | 0..100 | 80.8 % | 0.0 % | balanced 81/0 · diplomacy 81/0 · military 80/0 · none 80/0 |
| constantinople_1430 | venice | economic_output | 0..100 | 98.1 % | 0.0 % | balanced 98/0 · diplomacy 98/0 · military 98/0 · none 98/0 |
| constantinople_1430 | venice | external_pressure | 0..100 | 88.5 % | 0.0 % | balanced 89/0 · diplomacy 88/0 · military 89/0 · none 88/0 |
| constantinople_1430 | venice | legitimacy | 0..100 | 0.0 % | 51.1 % | balanced 0/50 · diplomacy 0/51 · military 0/51 · none 0/52 |
| constantinople_1430 | wallachia | cohesion | 0..100 | 54.3 % | 0.0 % | balanced 56/0 · diplomacy 56/0 · military 57/0 · none 49/0 |
| constantinople_1430 | wallachia | economic_output | 0..100 | 93.8 % | 0.0 % | balanced 94/0 · diplomacy 94/0 · military 92/0 · none 94/0 |
| constantinople_1430 | wallachia | external_pressure | 0..100 | 96.3 % | 0.0 % | balanced 97/0 · diplomacy 96/0 · military 96/0 · none 97/0 |
| constantinople_1430 | wallachia | legitimacy | 0..100 | 0.0 % | 72.5 % | balanced 0/74 · diplomacy 0/73 · military 0/66 · none 0/75 |
| constantinople_1430 | wallachia | military_quality | 0..100 | 20.4 % | 62.4 % | balanced 20/62 · diplomacy 20/61 · military 26/53 · none 17/70 |
| milan_1477 | bologna | economic_output | 0..100 | 94.9 % | 0.0 % | aggressive 95/0 · none 95/0 |
| milan_1477 | bologna | external_pressure | 0..100 | 97.4 % | 0.0 % | aggressive 97/0 · none 97/0 |
| milan_1477 | bologna | legitimacy | 0..100 | 76.5 % | 0.0 % | aggressive 76/0 · none 77/0 |
| milan_1477 | bologna | military_quality | 0..100 | 87.7 % | 0.0 % | aggressive 88/0 · none 88/0 |
| milan_1477 | bologna | military_size | ≥ 0 | 0.0 % | 71.7 % | aggressive 0/73 · none 0/70 |
| milan_1477 | ferrara | economic_output | 0..100 | 94.8 % | 0.0 % | aggressive 95/0 · none 95/0 |
| milan_1477 | ferrara | external_pressure | 0..100 | 96.7 % | 0.0 % | aggressive 97/0 · none 97/0 |
| milan_1477 | ferrara | legitimacy | 0..100 | 97.9 % | 0.0 % | aggressive 98/0 · none 98/0 |
| milan_1477 | ferrara | military_quality | 0..100 | 87.3 % | 0.0 % | aggressive 87/0 · none 87/0 |
| milan_1477 | ferrara | military_size | ≥ 0 | 0.0 % | 86.7 % | aggressive 0/86 · none 0/87 |
| milan_1477 | florence | economic_output | 0..100 | 98.2 % | 0.0 % | aggressive 98/0 · none 98/0 |
| milan_1477 | florence | external_pressure | 0..100 | 95.5 % | 0.0 % | aggressive 95/0 · none 95/0 |
| milan_1477 | florence | legitimacy | 0..100 | 95.0 % | 0.0 % | aggressive 96/0 · none 94/0 |
| milan_1477 | france | economic_output | 0..100 | 98.6 % | 0.0 % | aggressive 99/0 · none 99/0 |
| milan_1477 | france | external_pressure | 0..100 | 96.4 % | 0.0 % | aggressive 96/0 · none 96/0 |
| milan_1477 | france | legitimacy | 0..100 | 0.0 % | 50.2 % | aggressive 0/50 · none 0/50 |
| milan_1477 | france | military_quality | 0..100 | 0.0 % | 75.8 % | aggressive 0/76 · none 0/76 |
| milan_1477 | genoa | economic_output | 0..100 | 96.6 % | 0.0 % | aggressive 96/0 · none 97/0 |
| milan_1477 | genoa | external_pressure | 0..100 | 97.1 % | 0.0 % | aggressive 98/0 · none 97/0 |
| milan_1477 | genoa | legitimacy | 0..100 | 0.0 % | 55.8 % | aggressive 0/57 · none 0/55 |
| milan_1477 | genoa | military_quality | 0..100 | 7.0 % | 68.0 % | aggressive 4/70 · none 10/66 |
| milan_1477 | mantua | economic_output | 0..100 | 93.5 % | 0.0 % | aggressive 94/0 · none 93/0 |
| milan_1477 | mantua | external_pressure | 0..100 | 97.7 % | 0.0 % | aggressive 98/0 · none 98/0 |
| milan_1477 | mantua | legitimacy | 0..100 | 93.1 % | 0.0 % | aggressive 93/0 · none 93/0 |
| milan_1477 | mantua | military_quality | 0..100 | 94.4 % | 0.0 % | aggressive 94/0 · none 94/0 |
| milan_1477 | mantua | military_size | ≥ 0 | 0.0 % | 93.9 % | aggressive 0/94 · none 0/94 |
| milan_1477 | milan | cohesion | 0..100 | 90.6 % | 0.0 % | aggressive 91/0 · none 90/0 |
| milan_1477 | milan | economic_output | 0..100 | 98.0 % | 0.0 % | aggressive 98/0 · none 98/0 |
| milan_1477 | milan | external_pressure | 0..100 | 97.6 % | 0.0 % | aggressive 98/0 · none 97/0 |
| milan_1477 | milan | legitimacy | 0..100 | 0.0 % | 85.2 % | aggressive 0/88 · none 0/83 |
| milan_1477 | milan | military_quality | 0..100 | 0.0 % | 93.4 % | aggressive 0/97 · none 0/90 |
| milan_1477 | naples | economic_output | 0..100 | 96.3 % | 0.0 % | aggressive 96/0 · none 97/0 |
| milan_1477 | naples | external_pressure | 0..100 | 96.8 % | 0.0 % | aggressive 97/0 · none 97/0 |
| milan_1477 | naples | legitimacy | 0..100 | 56.8 % | 0.0 % | aggressive 60/0 · none 54/0 |
| milan_1477 | papacy | economic_output | 0..100 | 96.1 % | 0.0 % | aggressive 96/0 · none 96/0 |
| milan_1477 | papacy | external_pressure | 0..100 | 97.2 % | 0.0 % | aggressive 97/0 · none 97/0 |
| milan_1477 | papacy | legitimacy | 0..100 | 96.8 % | 0.0 % | aggressive 98/0 · none 96/0 |
| milan_1477 | papacy | military_quality | 0..100 | 83.4 % | 0.0 % | aggressive 83/0 · none 84/0 |
| milan_1477 | savoy | economic_output | 0..100 | 83.3 % | 0.0 % | aggressive 83/0 · none 84/0 |
| milan_1477 | savoy | external_pressure | 0..100 | 90.6 % | 0.0 % | aggressive 91/0 · none 91/0 |
| milan_1477 | sicily | economic_output | 0..100 | 95.0 % | 0.0 % | aggressive 95/0 · none 95/0 |
| milan_1477 | sicily | external_pressure | 0..100 | 95.1 % | 0.0 % | aggressive 95/0 · none 95/0 |
| milan_1477 | sicily | legitimacy | 0..100 | 94.9 % | 0.0 % | aggressive 96/0 · none 94/0 |
| milan_1477 | siena | economic_output | 0..100 | 83.1 % | 0.0 % | aggressive 84/0 · none 81/0 |
| milan_1477 | siena | external_pressure | 0..100 | 87.2 % | 0.0 % | aggressive 88/0 · none 86/0 |
| milan_1477 | siena | military_size | ≥ 0 | 0.0 % | 72.0 % | aggressive 0/73 · none 0/71 |
| milan_1477 | urbino | economic_output | 0..100 | 94.1 % | 0.0 % | aggressive 94/0 · none 94/0 |
| milan_1477 | urbino | external_pressure | 0..100 | 96.1 % | 0.0 % | aggressive 96/0 · none 96/0 |
| milan_1477 | urbino | legitimacy | 0..100 | 96.2 % | 0.0 % | aggressive 96/0 · none 96/0 |
| milan_1477 | urbino | military_quality | 0..100 | 95.5 % | 0.0 % | aggressive 95/0 · none 96/0 |
| milan_1477 | urbino | military_size | ≥ 0 | 0.0 % | 82.5 % | aggressive 0/80 · none 0/85 |
| milan_1477 | venice | economic_output | 0..100 | 98.3 % | 0.0 % | aggressive 98/0 · none 98/0 |
| milan_1477 | venice | external_pressure | 0..100 | 96.7 % | 0.0 % | aggressive 97/0 · none 97/0 |
| milan_1477 | venice | legitimacy | 0..100 | 91.6 % | 0.0 % | aggressive 93/0 · none 90/0 |
| milan_1477 | venice | military_quality | 0..100 | 89.4 % | 0.0 % | aggressive 90/0 · none 88/0 |
| rome_375 | alamanni | economic_output | 0..100 | 82.8 % | 0.0 % | balanced 87/0 · influence 85/0 · none 76/0 · wealth 79/0 |
| rome_375 | alamanni | external_pressure | 0..100 | 88.1 % | 0.0 % | balanced 91/0 · influence 90/0 · none 83/0 · wealth 85/0 |
| rome_375 | armenia | economic_output | 0..100 | 93.4 % | 0.0 % | balanced 94/0 · influence 93/0 · none 93/0 · wealth 93/0 |
| rome_375 | armenia | external_pressure | 0..100 | 94.7 % | 0.0 % | balanced 95/0 · influence 95/0 · none 95/0 · wealth 95/0 |
| rome_375 | berbers | economic_output | 0..100 | 95.2 % | 0.0 % | balanced 95/0 · influence 95/0 · none 95/0 · wealth 95/0 |
| rome_375 | berbers | external_pressure | 0..100 | 81.5 % | 0.0 % | balanced 81/0 · influence 81/0 · none 82/0 · wealth 82/0 |
| rome_375 | berbers | legitimacy | 0..100 | 0.0 % | 64.2 % | balanced 0/63 · influence 0/63 · none 0/66 · wealth 0/64 |
| rome_375 | berbers | military_quality | 0..100 | 21.7 % | 62.7 % | balanced 22/62 · influence 23/62 · none 20/64 · wealth 22/63 |
| rome_375 | burgundians | economic_output | 0..100 | 88.0 % | 0.0 % | balanced 91/0 · influence 91/0 · none 83/0 · wealth 82/0 |
| rome_375 | burgundians | external_pressure | 0..100 | 90.6 % | 0.0 % | balanced 93/0 · influence 93/0 · none 87/0 · wealth 86/0 |
| rome_375 | eastern_jin | economic_output | 0..100 | 96.9 % | 0.0 % | balanced 97/0 · influence 97/0 · none 97/0 · wealth 97/0 |
| rome_375 | eastern_jin | external_pressure | 0..100 | 84.7 % | 0.0 % | balanced 85/0 · influence 85/0 · none 85/0 · wealth 85/0 |
| rome_375 | eastern_jin | legitimacy | 0..100 | 0.0 % | 56.6 % | balanced 0/57 · influence 0/56 · none 0/57 · wealth 0/56 |
| rome_375 | eastern_jin | military_quality | 0..100 | 14.9 % | 59.1 % | balanced 15/60 · influence 16/59 · none 14/60 · wealth 15/58 |
| rome_375 | family | influence | 0..100 | 1.5 % | 65.6 % | balanced 4/30 · influence 2/55 · none 0/85 · wealth 0/93 |
| rome_375 | family | knowledge | 0..100 | 67.5 % | 0.0 % | balanced 87/0 · influence 89/0 · none 0/0 · wealth 93/0 |
| rome_375 | frankish_kingdom | economic_output | 0..100 | 93.1 % | 0.0 % | balanced 93/0 · influence 92/0 · none 94/0 · wealth 93/0 |
| rome_375 | frankish_kingdom | external_pressure | 0..100 | 90.2 % | 0.0 % | balanced 89/0 · influence 90/0 · none 89/0 · wealth 92/0 |
| rome_375 | frankish_kingdom | legitimacy | 0..100 | 0.0 % | 56.1 % | balanced 0/53 · influence 0/55 · none 0/56 · wealth 0/59 |
| rome_375 | frankish_kingdom | military_quality | 0..100 | 14.6 % | 57.5 % | balanced 17/54 · influence 16/56 · none 15/57 · wealth 13/60 |
| rome_375 | franks | economic_output | 0..100 | 90.5 % | 0.0 % | balanced 93/0 · influence 92/0 · none 85/0 · wealth 88/0 |
| rome_375 | franks | external_pressure | 0..100 | 90.5 % | 0.0 % | balanced 93/0 · influence 92/0 · none 85/0 · wealth 88/0 |
| rome_375 | franks | legitimacy | 0..100 | 0.0 % | 58.4 % | balanced 0/68 · influence 0/64 · none 0/39 · wealth 0/49 |
| rome_375 | franks | military_quality | 0..100 | 6.7 % | 58.0 % | balanced 5/68 · influence 7/64 · none 8/38 · wealth 8/48 |
| rome_375 | guptas | economic_output | 0..100 | 97.7 % | 0.0 % | balanced 98/0 · influence 98/0 · none 98/0 · wealth 98/0 |
| rome_375 | guptas | external_pressure | 0..100 | 77.4 % | 0.0 % | balanced 77/0 · influence 77/0 · none 77/0 · wealth 77/0 |
| rome_375 | huns | economic_output | 0..100 | 87.7 % | 0.0 % | balanced 88/0 · influence 87/0 · none 88/0 · wealth 88/0 |
| rome_375 | huns | external_pressure | 0..100 | 79.9 % | 0.0 % | balanced 81/0 · influence 79/0 · none 80/0 · wealth 80/0 |
| rome_375 | huns | military_quality | 0..100 | 66.9 % | 21.2 % | balanced 65/23 · influence 68/20 · none 68/20 · wealth 67/22 |
| rome_375 | kushans | economic_output | 0..100 | 96.4 % | 0.0 % | balanced 96/0 · influence 96/0 · none 96/0 · wealth 96/0 |
| rome_375 | kushans | external_pressure | 0..100 | 86.8 % | 0.0 % | balanced 87/0 · influence 87/0 · none 87/0 · wealth 87/0 |
| rome_375 | kushans | legitimacy | 0..100 | 0.0 % | 65.2 % | balanced 0/65 · influence 0/65 · none 0/65 · wealth 0/66 |
| rome_375 | kushans | military_quality | 0..100 | 18.8 % | 65.4 % | balanced 19/65 · influence 19/65 · none 19/65 · wealth 19/66 |
| rome_375 | ostrogoth_kingdom | economic_output | 0..100 | 93.4 % | 0.0 % | balanced 93/0 · influence 94/0 · none 94/0 · wealth 93/0 |
| rome_375 | ostrogoth_kingdom | external_pressure | 0..100 | 88.3 % | 0.0 % | balanced 87/0 · influence 90/0 · none 91/0 · wealth 85/0 |
| rome_375 | ostrogoth_kingdom | legitimacy | 0..100 | 0.0 % | 57.9 % | balanced 0/59 · influence 0/57 · none 0/61 · wealth 0/55 |
| rome_375 | ostrogoth_kingdom | military_quality | 0..100 | 24.2 % | 55.8 % | balanced 24/56 · influence 24/55 · none 21/59 · wealth 28/53 |
| rome_375 | ostrogoths | economic_output | 0..100 | 92.2 % | 0.0 % | balanced 92/0 · influence 92/0 · none 92/0 · wealth 92/0 |
| rome_375 | ostrogoths | external_pressure | 0..100 | 97.8 % | 0.0 % | balanced 98/0 · influence 98/0 · none 98/0 · wealth 98/0 |
| rome_375 | ostrogoths | legitimacy | 0..100 | 0.0 % | 58.8 % | balanced 0/59 · influence 0/60 · none 0/58 · wealth 0/58 |
| rome_375 | ostrogoths | military_quality | 0..100 | 29.1 % | 55.7 % | balanced 29/56 · influence 29/57 · none 30/55 · wealth 29/55 |
| rome_375 | rome | economic_output | 0..100 | 94.5 % | 0.0 % | balanced 95/0 · influence 94/0 · none 95/0 · wealth 94/0 |
| rome_375 | rome | external_pressure | 0..100 | 95.5 % | 0.0 % | balanced 96/0 · influence 95/0 · none 96/0 · wealth 95/0 |
| rome_375 | rome_east | economic_output | 0..100 | 97.7 % | 0.0 % | balanced 98/0 · influence 98/0 · none 98/0 · wealth 98/0 |
| rome_375 | rome_east | external_pressure | 0..100 | 99.5 % | 0.0 % | balanced 99/0 · influence 99/0 · none 99/0 · wealth 99/0 |
| rome_375 | rome_east | legitimacy | 0..100 | 0.0 % | 81.3 % | balanced 0/81 · influence 0/81 · none 0/81 · wealth 0/82 |
| rome_375 | rome_east | military_quality | 0..100 | 4.7 % | 82.0 % | balanced 5/82 · influence 5/82 · none 4/82 · wealth 4/83 |
| rome_375 | sassanids | economic_output | 0..100 | 97.4 % | 0.0 % | balanced 97/0 · influence 97/0 · none 97/0 · wealth 97/0 |
| rome_375 | sassanids | external_pressure | 0..100 | 86.7 % | 0.0 % | balanced 86/0 · influence 86/0 · none 87/0 · wealth 87/0 |
| rome_375 | saxons | economic_output | 0..100 | 0.0 % | 83.2 % | balanced 0/84 · influence 0/86 · none 0/84 · wealth 0/79 |
| rome_375 | saxons | military_quality | 0..100 | 72.9 % | 2.2 % | balanced 73/3 · influence 77/0 · none 72/0 · wealth 68/6 |
| rome_375 | saxons | population | ≥ 0 | 0.0 % | 82.9 % | balanced 0/84 · influence 0/84 · none 0/81 · wealth 0/83 |
| rome_375 | vandal_kingdom_africa | economic_output | 0..100 | 92.3 % | 0.2 % | balanced 92/0 · influence 90/1 · none 93/0 · wealth 93/0 |
| rome_375 | vandal_kingdom_africa | external_pressure | 0..100 | 89.7 % | 0.0 % | balanced 91/0 · influence 86/0 · none 89/0 · wealth 92/0 |
| rome_375 | vandal_kingdom_africa | legitimacy | 0..100 | 0.0 % | 67.4 % | balanced 0/67 · influence 0/61 · none 0/70 · wealth 0/69 |
| rome_375 | vandal_kingdom_africa | military_quality | 0..100 | 10.9 % | 68.0 % | balanced 12/67 · influence 12/63 · none 11/70 · wealth 10/69 |
| rome_375 | vandals | economic_output | 0..100 | 88.0 % | 0.0 % | balanced 90/0 · influence 89/0 · none 84/0 · wealth 87/0 |
| rome_375 | vandals | external_pressure | 0..100 | 94.3 % | 0.0 % | balanced 95/0 · influence 95/0 · none 92/0 · wealth 94/0 |
| rome_375 | vandals | legitimacy | 0..100 | 0.0 % | 54.5 % | balanced 0/61 · influence 0/57 · none 0/43 · wealth 0/51 |
| rome_375 | vandals | military_quality | 0..100 | 10.5 % | 53.9 % | balanced 9/61 · influence 11/57 · none 11/43 · wealth 11/50 |
| rome_375 | visigoth_kingdom | economic_output | 0..100 | 93.4 % | 0.0 % | balanced 92/0 · influence 94/0 · none 93/0 · wealth 94/0 |
| rome_375 | visigoth_kingdom | external_pressure | 0..100 | 93.7 % | 0.0 % | balanced 94/0 · influence 91/0 · none 95/0 · wealth 94/0 |
| rome_375 | visigoth_kingdom | legitimacy | 0..100 | 0.0 % | 54.1 % | balanced 0/47 · influence 0/60 · none 0/55 · wealth 0/52 |
| rome_375 | visigoth_kingdom | military_quality | 0..100 | 24.5 % | 53.3 % | balanced 30/45 · influence 21/58 · none 23/56 · wealth 26/51 |
| rome_375 | visigoths | economic_output | 0..100 | 94.1 % | 0.0 % | balanced 94/0 · influence 94/0 · none 94/0 · wealth 94/0 |
| rome_375 | visigoths | external_pressure | 0..100 | 96.7 % | 0.0 % | balanced 97/0 · influence 97/0 · none 97/0 · wealth 97/0 |
| rome_375 | visigoths | legitimacy | 0..100 | 0.0 % | 66.4 % | balanced 0/68 · influence 0/66 · none 0/67 · wealth 0/65 |
| rome_375 | visigoths | military_quality | 0..100 | 20.4 % | 64.7 % | balanced 20/66 · influence 21/64 · none 20/65 · wealth 21/64 |

## Full table, pooled over the worlds

| scenario | carrier | metric | scale | living ticks | at ceiling | at floor | drift per tick: first 50 / middle / tail | inflow cut at ceiling | outflow cut at floor |
|---|---|---|---|---|---|---|---|---|---|
| constantinople_1430 | byzantium | cohesion | 0..100 | 20373 | 49.1 % | 0.7 % | +1.123 / -0.397 / -0.427 | 0.1 % | 0.1 % |
| constantinople_1430 | byzantium | economic_output | 0..100 | 20373 | 94.0 % | 0.0 % | +1.586 / -0.000 / +0.000 | 91.7 % | 0.0 % |
| constantinople_1430 | byzantium | external_pressure | 0..100 | 20373 | 63.1 % | 13.4 % | +0.841 / -0.343 / -0.089 | 80.8 % | 27.5 % |
| constantinople_1430 | byzantium | legitimacy | 0..100 | 20373 | 0.1 % | 39.2 % | -0.851 / -0.039 / -0.060 | 0.1 % | 48.2 % |
| constantinople_1430 | byzantium | military_quality | 0..100 | 20373 | 35.8 % | 31.3 % | +0.736 / -0.309 / +0.017 | 26.7 % | 9.0 % |
| constantinople_1430 | byzantium | military_size | ≥ 0 | 20373 | — | 5.1 % | +0.208 / +0.099 / +0.020 | 0.0 % | 0.2 % |
| constantinople_1430 | byzantium | population | ≥ 0 | 20373 | — | 2.2 % | +0.628 / +0.514 / +0.465 | 0.0 % | 0.7 % |
| constantinople_1430 | byzantium | treasury | none | 20373 | — | — | -7.791 / -0.922 / -1.546 | 0.0 % | 0.0 % |
| constantinople_1430 | genoa | cohesion | 0..100 | 36000 | 83.1 % | 0.0 % | +0.922 / +0.008 / -0.009 | 0.1 % | 0.0 % |
| constantinople_1430 | genoa | economic_output | 0..100 | 36000 | 97.6 % | 0.0 % | +0.699 / +0.000 / -0.001 | 90.2 % | 0.0 % |
| constantinople_1430 | genoa | external_pressure | 0..100 | 36000 | 85.2 % | 0.0 % | +1.109 / +0.023 / +0.000 | 85.6 % | — |
| constantinople_1430 | genoa | legitimacy | 0..100 | 36000 | 0.0 % | 55.5 % | -0.270 / -0.243 / +0.000 | — | 60.7 % |
| constantinople_1430 | genoa | military_quality | 0..100 | 36000 | 36.4 % | 45.3 % | +0.719 / -0.490 / +0.000 | 26.9 % | 18.2 % |
| constantinople_1430 | genoa | military_size | ≥ 0 | 36000 | — | 1.1 % | -0.064 / -0.022 / +0.005 | 0.0 % | 0.1 % |
| constantinople_1430 | genoa | population | ≥ 0 | 36000 | — | 0.0 % | -0.084 / -0.121 / -0.130 | 0.0 % | 0.0 % |
| constantinople_1430 | genoa | treasury | none | 36000 | — | — | -5.208 / +4.399 / +9.261 | 0.0 % | 0.0 % |
| constantinople_1430 | global | federation_progress | 0..100 | 35880 | 11.1 % | 9.5 % | +0.469 / +0.170 / -0.072 | 23.2 % | 16.2 % |
| constantinople_1430 | hungary | cohesion | 0..100 | 28043 | 38.4 % | 0.0 % | +0.935 / -0.355 / -0.331 | 0.0 % | 0.0 % |
| constantinople_1430 | hungary | economic_output | 0..100 | 28043 | 96.0 % | 0.0 % | +1.099 / +0.000 / +0.000 | 93.4 % | 0.0 % |
| constantinople_1430 | hungary | external_pressure | 0..100 | 28043 | 94.4 % | 0.0 % | +0.900 / -0.000 / -0.000 | 95.9 % | 0.0 % |
| constantinople_1430 | hungary | legitimacy | 0..100 | 28043 | 0.0 % | 50.9 % | -0.488 / -0.229 / +0.000 | 0.0 % | 53.9 % |
| constantinople_1430 | hungary | military_quality | 0..100 | 28043 | 41.0 % | 43.1 % | +0.840 / -0.599 / +0.000 | 27.8 % | 23.7 % |
| constantinople_1430 | hungary | military_size | ≥ 0 | 28043 | — | 0.0 % | -0.365 / -0.010 / +0.060 | 0.0 % | 0.0 % |
| constantinople_1430 | hungary | population | ≥ 0 | 28043 | — | 0.0 % | +0.141 / -0.015 / -0.482 | 0.0 % | 0.0 % |
| constantinople_1430 | hungary | treasury | none | 28043 | — | — | +45.592 / +61.574 / +53.361 | 0.0 % | 0.0 % |
| constantinople_1430 | mamluks | cohesion | 0..100 | 18385 | 58.8 % | 0.3 % | +1.093 / +0.042 / -0.004 | 0.0 % | 0.1 % |
| constantinople_1430 | mamluks | economic_output | 0..100 | 18385 | 96.5 % | 0.0 % | +1.721 / +0.143 / +0.093 | 95.6 % | 0.0 % |
| constantinople_1430 | mamluks | external_pressure | 0..100 | 16567 | 69.5 % | 0.9 % | +2.639 / +0.257 / +0.075 | 46.0 % | — |
| constantinople_1430 | mamluks | legitimacy | 0..100 | 18385 | 0.0 % | 42.3 % | -0.145 / -0.239 / -0.040 | 0.0 % | 55.3 % |
| constantinople_1430 | mamluks | military_quality | 0..100 | 18385 | 43.3 % | 41.3 % | +1.492 / -0.302 / +0.041 | 36.5 % | 46.5 % |
| constantinople_1430 | mamluks | military_size | ≥ 0 | 18385 | — | 0.0 % | +0.084 / +0.082 / +0.083 | 0.0 % | 0.0 % |
| constantinople_1430 | mamluks | population | ≥ 0 | 18385 | — | 0.0 % | +3.299 / +0.511 / -0.157 | 0.0 % | 0.0 % |
| constantinople_1430 | mamluks | treasury | none | 18301 | — | — | +28.989 / +34.823 / +33.366 | 0.0 % | 0.0 % |
| constantinople_1430 | milan | cohesion | 0..100 | 36000 | 70.8 % | 0.0 % | +0.484 / +0.100 / -0.000 | 0.0 % | 0.0 % |
| constantinople_1430 | milan | economic_output | 0..100 | 36000 | 97.4 % | 0.0 % | +0.599 / +0.000 / +0.000 | 90.7 % | 0.0 % |
| constantinople_1430 | milan | external_pressure | 0..100 | 36000 | 74.5 % | 0.0 % | +0.902 / +0.124 / +0.000 | 75.8 % | — |
| constantinople_1430 | milan | legitimacy | 0..100 | 36000 | 0.0 % | 47.5 % | -0.132 / -0.292 / +0.000 | — | 55.2 % |
| constantinople_1430 | milan | military_quality | 0..100 | 36000 | 22.8 % | 49.2 % | +0.640 / -0.500 / +0.000 | 7.9 % | 51.6 % |
| constantinople_1430 | milan | military_size | ≥ 0 | 36000 | — | 0.0 % | -0.008 / -0.070 / +0.057 | 0.0 % | 0.0 % |
| constantinople_1430 | milan | population | ≥ 0 | 36000 | — | 0.0 % | -0.105 / -0.101 / -0.152 | 0.0 % | 0.0 % |
| constantinople_1430 | milan | treasury | none | 36000 | — | — | -10.670 / +2.468 / +2.874 | 0.0 % | 0.0 % |
| constantinople_1430 | ottomans | cohesion | 0..100 | 27495 | 24.6 % | 0.5 % | -0.215 / -0.017 / -0.039 | 0.0 % | 0.1 % |
| constantinople_1430 | ottomans | economic_output | 0..100 | 27495 | 97.3 % | 0.0 % | +0.699 / +0.000 / +0.000 | 95.6 % | 0.0 % |
| constantinople_1430 | ottomans | expansion_count | none | 12367 | — | — | +0.000 / +0.002 / +0.001 | 0.0 % | — |
| constantinople_1430 | ottomans | external_pressure | 0..100 | 27495 | 32.0 % | 3.3 % | +0.642 / +0.198 / +0.202 | 75.0 % | 5.5 % |
| constantinople_1430 | ottomans | legitimacy | 0..100 | 27495 | 0.0 % | 12.9 % | -0.206 / -0.261 / -0.187 | 0.0 % | 25.0 % |
| constantinople_1430 | ottomans | military_quality | 0..100 | 27495 | 81.5 % | 11.6 % | +0.560 / -0.222 / -0.263 | 74.2 % | 30.9 % |
| constantinople_1430 | ottomans | military_size | ≥ 0 | 27495 | — | 0.0 % | -1.641 / +0.038 / +0.217 | 0.0 % | 0.0 % |
| constantinople_1430 | ottomans | population | ≥ 0 | 27495 | — | 0.0 % | -6.184 / -3.683 / -1.087 | 0.0 % | 0.0 % |
| constantinople_1430 | ottomans | treasury | none | 27495 | — | — | +282.593 / +279.791 / +259.846 | 0.0 % | 0.0 % |
| constantinople_1430 | papacy | cohesion | 0..100 | 36000 | 77.6 % | 0.0 % | +0.790 / -0.000 / +0.002 | 0.0 % | 0.0 % |
| constantinople_1430 | papacy | economic_output | 0..100 | 36000 | 97.0 % | 0.0 % | +0.999 / +0.000 / +0.000 | 92.4 % | 0.0 % |
| constantinople_1430 | papacy | external_pressure | 0..100 | 36000 | 88.2 % | 0.0 % | +1.500 / +0.000 / +0.000 | 84.6 % | — |
| constantinople_1430 | papacy | legitimacy | 0..100 | 36000 | 0.0 % | 60.2 % | -0.550 / -0.288 / +0.000 | — | 64.9 % |
| constantinople_1430 | papacy | military_quality | 0..100 | 36000 | 35.9 % | 48.7 % | +0.900 / -0.500 / +0.000 | 28.6 % | 18.3 % |
| constantinople_1430 | papacy | military_size | ≥ 0 | 36000 | — | 13.1 % | -0.078 / +0.002 / +0.004 | 0.0 % | 2.3 % |
| constantinople_1430 | papacy | population | ≥ 0 | 36000 | — | 0.2 % | -0.096 / -0.019 / -0.066 | 0.0 % | 0.9 % |
| constantinople_1430 | papacy | treasury | none | 36000 | — | — | -5.394 / +2.511 / +0.371 | 0.0 % | 0.0 % |
| constantinople_1430 | poland_lithuania | cohesion | 0..100 | 35130 | 66.1 % | 0.0 % | +0.175 / +0.128 / -0.009 | 1.2 % | 0.0 % |
| constantinople_1430 | poland_lithuania | economic_output | 0..100 | 35130 | 97.5 % | 0.0 % | +1.135 / +0.000 / +0.000 | 96.4 % | 0.0 % |
| constantinople_1430 | poland_lithuania | external_pressure | 0..100 | 34384 | 74.7 % | 0.0 % | +1.455 / +0.213 / +0.000 | 76.0 % | — |
| constantinople_1430 | poland_lithuania | legitimacy | 0..100 | 35130 | 0.0 % | 35.8 % | -0.046 / -0.341 / +0.000 | — | 46.4 % |
| constantinople_1430 | poland_lithuania | military_quality | 0..100 | 35130 | 54.6 % | 32.9 % | +1.136 / -0.501 / +0.000 | 44.2 % | 35.4 % |
| constantinople_1430 | poland_lithuania | military_size | ≥ 0 | 35130 | — | 0.0 % | -0.272 / +0.030 / +0.091 | 0.0 % | 0.0 % |
| constantinople_1430 | poland_lithuania | population | ≥ 0 | 35130 | — | 0.0 % | +0.891 / +0.408 / +0.027 | 0.0 % | 0.0 % |
| constantinople_1430 | poland_lithuania | treasury | none | 35010 | — | — | +29.679 / +41.197 / +36.723 | 0.0 % | 0.0 % |
| constantinople_1430 | serbia | cohesion | 0..100 | 33642 | 63.1 % | 0.0 % | +1.032 / -0.074 / -0.128 | 0.5 % | 0.0 % |
| constantinople_1430 | serbia | economic_output | 0..100 | 33642 | 95.7 % | 0.0 % | +1.399 / +0.000 / +0.001 | 92.7 % | 0.0 % |
| constantinople_1430 | serbia | external_pressure | 0..100 | 33642 | 91.0 % | 0.0 % | +0.700 / -0.002 / -0.002 | 97.3 % | 0.0 % |
| constantinople_1430 | serbia | legitimacy | 0..100 | 33642 | 0.0 % | 67.2 % | -0.553 / -0.129 / +0.000 | 0.0 % | 67.8 % |
| constantinople_1430 | serbia | military_quality | 0..100 | 33642 | 27.0 % | 57.9 % | +0.900 / -0.524 / +0.000 | 17.9 % | 23.1 % |
| constantinople_1430 | serbia | military_size | ≥ 0 | 33642 | — | 0.0 % | -0.126 / +0.059 / +0.087 | 0.0 % | 0.0 % |
| constantinople_1430 | serbia | population | ≥ 0 | 33642 | — | 0.0 % | -0.164 / +0.359 / +0.010 | 0.0 % | 0.0 % |
| constantinople_1430 | serbia | treasury | none | 33642 | — | — | +12.409 / +16.047 / +12.198 | 0.0 % | 0.0 % |
| constantinople_1430 | trebizond | cohesion | 0..100 | 36000 | 84.1 % | 0.0 % | +1.030 / -0.001 / +0.006 | 0.0 % | 0.0 % |
| constantinople_1430 | trebizond | economic_output | 0..100 | 36000 | 95.8 % | 0.0 % | +1.299 / +0.000 / +0.000 | 92.2 % | 0.0 % |
| constantinople_1430 | trebizond | external_pressure | 0..100 | 36000 | 91.0 % | 0.0 % | +1.000 / +0.000 / +0.000 | 90.4 % | — |
| constantinople_1430 | trebizond | legitimacy | 0..100 | 36000 | 0.0 % | 63.8 % | -0.426 / -0.169 / +0.000 | — | 66.7 % |
| constantinople_1430 | trebizond | military_quality | 0..100 | 36000 | 29.7 % | 55.5 % | +1.000 / -0.500 / +0.000 | 21.2 % | 26.7 % |
| constantinople_1430 | trebizond | military_size | ≥ 0 | 36000 | — | 4.7 % | -0.024 / +0.063 / +0.029 | 0.0 % | 1.5 % |
| constantinople_1430 | trebizond | population | ≥ 0 | 36000 | — | 0.9 % | +0.530 / +0.294 / -0.068 | 0.0 % | 1.0 % |
| constantinople_1430 | trebizond | treasury | none | 36000 | — | — | +2.237 / +4.346 / +0.867 | 0.0 % | 0.0 % |
| constantinople_1430 | venice | cohesion | 0..100 | 36000 | 80.8 % | 0.0 % | +0.835 / -0.001 / -0.008 | 0.0 % | 0.0 % |
| constantinople_1430 | venice | economic_output | 0..100 | 36000 | 98.1 % | 0.0 % | +0.500 / +0.000 / -0.001 | 87.1 % | 0.0 % |
| constantinople_1430 | venice | external_pressure | 0..100 | 36000 | 88.5 % | 0.0 % | +1.300 / +0.000 / +0.000 | 86.4 % | — |
| constantinople_1430 | venice | legitimacy | 0..100 | 36000 | 0.0 % | 51.1 % | -0.269 / -0.283 / +0.000 | — | 55.7 % |
| constantinople_1430 | venice | military_quality | 0..100 | 36000 | 45.2 % | 40.2 % | +0.700 / -0.500 / +0.000 | 36.4 % | 17.0 % |
| constantinople_1430 | venice | military_size | ≥ 0 | 36000 | — | 11.1 % | -0.359 / +0.001 / -0.011 | 0.0 % | 52.4 % |
| constantinople_1430 | venice | population | ≥ 0 | 36000 | — | 0.0 % | -0.077 / -0.037 / -0.065 | 0.0 % | 0.0 % |
| constantinople_1430 | venice | treasury | none | 36000 | — | — | -10.264 / +1.333 / +1.835 | 0.0 % | 0.0 % |
| constantinople_1430 | wallachia | cohesion | 0..100 | 29564 | 54.3 % | 0.0 % | +1.163 / -0.185 / -0.112 | 0.0 % | 0.0 % |
| constantinople_1430 | wallachia | economic_output | 0..100 | 29564 | 93.8 % | 0.0 % | +1.607 / +0.002 / +0.001 | 90.5 % | 0.0 % |
| constantinople_1430 | wallachia | external_pressure | 0..100 | 29254 | 96.3 % | 0.0 % | +1.758 / +0.000 / +0.000 | 94.9 % | — |
| constantinople_1430 | wallachia | legitimacy | 0..100 | 29564 | 0.0 % | 72.5 % | -0.479 / -0.068 / +0.000 | 0.0 % | 74.9 % |
| constantinople_1430 | wallachia | military_quality | 0..100 | 29564 | 20.4 % | 62.4 % | +0.998 / -0.574 / +0.000 | 11.2 % | 23.3 % |
| constantinople_1430 | wallachia | military_size | ≥ 0 | 29564 | — | 0.6 % | -0.493 / +0.057 / +0.014 | 0.0 % | 0.1 % |
| constantinople_1430 | wallachia | population | ≥ 0 | 29564 | — | 0.1 % | -0.998 / +0.312 / +0.049 | 0.0 % | 0.0 % |
| constantinople_1430 | wallachia | treasury | none | 29444 | — | — | +8.490 / +15.041 / +9.582 | 0.0 % | 0.0 % |
| milan_1477 | bologna | cohesion | 0..100 | 18000 | 7.3 % | 0.1 % | +0.710 / -0.104 / -0.045 | 0.0 % | 0.0 % |
| milan_1477 | bologna | economic_output | 0..100 | 18000 | 94.9 % | 0.0 % | +1.200 / +0.000 / +0.000 | 86.7 % | 0.0 % |
| milan_1477 | bologna | external_pressure | 0..100 | 18000 | 97.4 % | 0.0 % | +1.200 / +0.000 / +0.000 | 98.1 % | — |
| milan_1477 | bologna | legitimacy | 0..100 | 18000 | 76.5 % | 0.0 % | +0.659 / +0.060 / +0.001 | 39.5 % | 0.0 % |
| milan_1477 | bologna | military_quality | 0..100 | 18000 | 87.7 % | 0.0 % | +0.900 / +0.000 / +0.000 | 43.6 % | 0.0 % |
| milan_1477 | bologna | military_size | ≥ 0 | 18000 | — | 71.7 % | -0.280 / +0.023 / -0.033 | 0.0 % | 10.1 % |
| milan_1477 | bologna | population | ≥ 0 | 18000 | — | 2.0 % | +0.013 / +0.010 / +0.031 | 0.0 % | 4.3 % |
| milan_1477 | bologna | treasury | none | 18000 | — | — | +0.684 / +2.027 / +1.530 | 0.0 % | 0.0 % |
| milan_1477 | ferrara | cohesion | 0..100 | 18000 | 0.0 % | 0.4 % | +0.129 / +0.030 / +0.008 | 0.0 % | 0.0 % |
| milan_1477 | ferrara | economic_output | 0..100 | 18000 | 94.8 % | 0.0 % | +1.200 / -0.001 / -0.000 | 85.8 % | 0.0 % |
| milan_1477 | ferrara | external_pressure | 0..100 | 18000 | 96.7 % | 0.0 % | +1.300 / +0.000 / +0.000 | 94.8 % | — |
| milan_1477 | ferrara | legitimacy | 0..100 | 18000 | 97.9 % | 0.0 % | +0.600 / +0.000 / +0.000 | 84.7 % | 0.0 % |
| milan_1477 | ferrara | military_quality | 0..100 | 18000 | 87.3 % | 0.0 % | +0.898 / +0.000 / +0.000 | 43.6 % | 0.0 % |
| milan_1477 | ferrara | military_size | ≥ 0 | 18000 | — | 86.7 % | -0.300 / +0.003 / +0.022 | 0.0 % | 35.9 % |
| milan_1477 | ferrara | population | ≥ 0 | 18000 | — | 9.7 % | -0.249 / -0.063 / -0.000 | 0.0 % | 12.7 % |
| milan_1477 | ferrara | treasury | none | 18000 | — | — | -0.422 / +0.973 / +0.435 | 0.0 % | 0.0 % |
| milan_1477 | florence | cohesion | 0..100 | 18000 | 0.0 % | 0.0 % | +0.372 / +0.001 / -0.006 | 0.0 % | 0.0 % |
| milan_1477 | florence | economic_output | 0..100 | 18000 | 98.2 % | 0.0 % | +0.400 / +0.000 / +0.000 | 93.2 % | 0.0 % |
| milan_1477 | florence | external_pressure | 0..100 | 18000 | 95.5 % | 0.0 % | +1.300 / +0.000 / +0.000 | 96.1 % | — |
| milan_1477 | florence | legitimacy | 0..100 | 18000 | 95.0 % | 0.0 % | +0.800 / +0.000 / +0.000 | 72.0 % | 0.0 % |
| milan_1477 | florence | military_quality | 0..100 | 18000 | 49.6 % | 9.3 % | -0.350 / +0.256 / +0.067 | 33.0 % | 9.3 % |
| milan_1477 | florence | military_size | ≥ 0 | 18000 | — | 0.0 % | -0.113 / +0.013 / -0.013 | 0.0 % | 0.0 % |
| milan_1477 | florence | population | ≥ 0 | 18000 | — | 0.0 % | +0.066 / +0.068 / +0.088 | 0.0 % | 0.0 % |
| milan_1477 | florence | treasury | none | 18000 | — | — | +0.580 / +1.315 / +2.892 | 0.0 % | 0.0 % |
| milan_1477 | france | cohesion | 0..100 | 17589 | 0.0 % | 0.0 % | +0.047 / -0.071 / +0.007 | 0.0 % | 0.0 % |
| milan_1477 | france | economic_output | 0..100 | 17589 | 98.6 % | 0.0 % | +0.232 / +0.000 / -0.001 | 96.7 % | 0.0 % |
| milan_1477 | france | external_pressure | 0..100 | 17589 | 96.4 % | 0.0 % | +1.970 / +0.000 / +0.000 | 88.7 % | — |
| milan_1477 | france | legitimacy | 0..100 | 17589 | 0.0 % | 50.2 % | -0.409 / -0.287 / +0.000 | — | 52.4 % |
| milan_1477 | france | military_quality | 0..100 | 17589 | 0.0 % | 75.8 % | -0.799 / -0.153 / +0.000 | 0.0 % | 95.9 % |
| milan_1477 | france | military_size | ≥ 0 | 17589 | — | 0.0 % | -0.990 / -0.023 / -0.010 | 0.0 % | 0.0 % |
| milan_1477 | france | population | ≥ 0 | 17589 | — | 0.0 % | +0.167 / -0.201 / -0.283 | 0.0 % | 0.0 % |
| milan_1477 | france | treasury | none | 17589 | — | — | +28.765 / +43.651 / +42.160 | 0.0 % | 0.0 % |
| milan_1477 | genoa | cohesion | 0..100 | 18000 | 0.0 % | 0.0 % | +1.112 / +0.001 / -0.005 | 0.0 % | 0.0 % |
| milan_1477 | genoa | economic_output | 0..100 | 18000 | 96.6 % | 0.0 % | +0.798 / +0.000 / -0.004 | 86.6 % | 0.0 % |
| milan_1477 | genoa | external_pressure | 0..100 | 18000 | 97.1 % | 0.0 % | +1.100 / +0.000 / +0.000 | 96.0 % | — |
| milan_1477 | genoa | legitimacy | 0..100 | 18000 | 0.0 % | 55.8 % | -0.451 / -0.135 / -0.001 | 0.0 % | 53.5 % |
| milan_1477 | genoa | military_quality | 0..100 | 18000 | 7.0 % | 68.0 % | +0.737 / -0.474 / +0.000 | 1.6 % | 75.8 % |
| milan_1477 | genoa | military_size | ≥ 0 | 18000 | — | 0.0 % | -0.205 / +0.032 / -0.058 | 0.0 % | 0.0 % |
| milan_1477 | genoa | population | ≥ 0 | 18000 | — | 0.0 % | +0.079 / -0.079 / -0.100 | 0.0 % | 0.0 % |
| milan_1477 | genoa | treasury | none | 18000 | — | — | -0.184 / -0.572 / -1.886 | 0.0 % | 0.0 % |
| milan_1477 | mantua | cohesion | 0..100 | 18000 | 0.0 % | 0.6 % | +0.218 / +0.012 / +0.017 | 0.0 % | 0.0 % |
| milan_1477 | mantua | economic_output | 0..100 | 18000 | 93.5 % | 0.0 % | +1.400 / +0.000 / -0.005 | 81.5 % | 0.0 % |
| milan_1477 | mantua | external_pressure | 0..100 | 18000 | 97.7 % | 0.0 % | +1.300 / +0.000 / +0.000 | 94.7 % | — |
| milan_1477 | mantua | legitimacy | 0..100 | 18000 | 93.1 % | 0.0 % | +0.700 / +0.000 / +0.000 | 69.1 % | 0.0 % |
| milan_1477 | mantua | military_quality | 0..100 | 18000 | 94.4 % | 0.0 % | +0.840 / +0.000 / +0.000 | 62.7 % | 0.0 % |
| milan_1477 | mantua | military_size | ≥ 0 | 18000 | — | 93.9 % | -0.240 / +0.002 / -0.002 | 0.0 % | 63.3 % |
| milan_1477 | mantua | population | ≥ 0 | 18000 | — | 27.4 % | -0.221 / -0.040 / -0.027 | 0.0 % | 36.6 % |
| milan_1477 | mantua | treasury | none | 18000 | — | — | -0.377 / +0.703 / +0.676 | 0.0 % | 0.0 % |
| milan_1477 | milan | cohesion | 0..100 | 18000 | 90.6 % | 0.0 % | +1.020 / -0.001 / +0.001 | 0.0 % | 0.0 % |
| milan_1477 | milan | economic_output | 0..100 | 18000 | 98.0 % | 0.0 % | +0.500 / +0.000 / -0.002 | 92.3 % | 0.0 % |
| milan_1477 | milan | expansion_count | none | 13000 | — | — | +0.000 / +0.000 / +0.000 | 0.0 % | — |
| milan_1477 | milan | external_pressure | 0..100 | 18000 | 97.6 % | 0.0 % | +1.100 / +0.000 / +0.000 | 97.8 % | — |
| milan_1477 | milan | legitimacy | 0..100 | 18000 | 0.0 % | 85.2 % | -0.892 / -0.000 / +0.000 | 0.0 % | 75.0 % |
| milan_1477 | milan | military_quality | 0..100 | 18000 | 0.0 % | 93.4 % | -1.300 / +0.000 / +0.000 | 0.0 % | 80.9 % |
| milan_1477 | milan | military_size | ≥ 0 | 18000 | — | 0.0 % | -0.392 / -0.016 / -0.015 | 0.0 % | 0.0 % |
| milan_1477 | milan | population | ≥ 0 | 18000 | — | 0.0 % | +0.242 / -0.061 / -0.080 | 0.0 % | 0.0 % |
| milan_1477 | milan | treasury | none | 18000 | — | — | -2.816 / +6.512 / +6.881 | 0.0 % | 0.0 % |
| milan_1477 | naples | cohesion | 0..100 | 18000 | 0.0 % | 3.5 % | +0.568 / -0.189 / -0.022 | 0.0 % | 0.8 % |
| milan_1477 | naples | economic_output | 0..100 | 18000 | 96.3 % | 0.0 % | +0.898 / +0.000 / -0.001 | 86.6 % | 0.0 % |
| milan_1477 | naples | external_pressure | 0..100 | 18000 | 96.8 % | 0.0 % | +1.000 / +0.000 / +0.000 | 97.9 % | — |
| milan_1477 | naples | legitimacy | 0..100 | 18000 | 56.8 % | 0.0 % | +0.661 / +0.024 / -0.062 | 25.6 % | 0.0 % |
| milan_1477 | naples | military_quality | 0..100 | 18000 | 36.8 % | 20.6 % | -0.609 / +0.257 / +0.153 | 29.3 % | 20.4 % |
| milan_1477 | naples | military_size | ≥ 0 | 18000 | — | 0.2 % | -0.384 / -0.042 / -0.030 | 0.0 % | 0.0 % |
| milan_1477 | naples | population | ≥ 0 | 18000 | — | 0.0 % | -0.281 / -1.095 / -1.248 | 0.0 % | 0.0 % |
| milan_1477 | naples | treasury | none | 18000 | — | — | +25.904 / +23.463 / +14.920 | 0.0 % | 0.0 % |
| milan_1477 | papacy | cohesion | 0..100 | 18000 | 0.0 % | 9.7 % | -0.986 / +0.137 / +0.019 | 0.0 % | 2.5 % |
| milan_1477 | papacy | economic_output | 0..100 | 18000 | 96.1 % | 0.0 % | +0.899 / +0.000 / -0.001 | 76.2 % | 0.0 % |
| milan_1477 | papacy | external_pressure | 0..100 | 18000 | 97.2 % | 0.0 % | +1.400 / +0.000 / +0.000 | 97.2 % | — |
| milan_1477 | papacy | legitimacy | 0..100 | 18000 | 96.8 % | 0.0 % | +0.400 / +0.000 / +0.000 | 49.3 % | 0.0 % |
| milan_1477 | papacy | military_quality | 0..100 | 18000 | 83.4 % | 0.0 % | +0.937 / +0.016 / +0.000 | 42.0 % | 0.0 % |
| milan_1477 | papacy | military_size | ≥ 0 | 18000 | — | 38.6 % | -0.253 / +0.017 / +0.004 | 0.0 % | 6.9 % |
| milan_1477 | papacy | population | ≥ 0 | 18000 | — | 2.0 % | -0.527 / +0.257 / +0.440 | 0.0 % | 0.5 % |
| milan_1477 | papacy | treasury | none | 18000 | — | — | +2.432 / +4.451 / +6.422 | 0.0 % | 0.0 % |
| milan_1477 | savoy | cohesion | 0..100 | 4940 | 0.0 % | 4.7 % | -0.414 / -0.514 / — | 0.0 % | 0.0 % |
| milan_1477 | savoy | economic_output | 0..100 | 4940 | 83.3 % | 0.0 % | +1.316 / +0.000 / — | 66.7 % | 0.0 % |
| milan_1477 | savoy | external_pressure | 0..100 | 4940 | 90.6 % | 0.0 % | +1.115 / +0.000 / — | 94.7 % | — |
| milan_1477 | savoy | legitimacy | 0..100 | 4940 | 0.0 % | 31.9 % | -0.993 / -0.149 / — | 0.0 % | 25.1 % |
| milan_1477 | savoy | military_quality | 0..100 | 4940 | 0.0 % | 37.8 % | -0.371 / -0.683 / — | 0.0 % | 60.8 % |
| milan_1477 | savoy | military_size | ≥ 0 | 4940 | — | 33.7 % | -0.386 / -0.000 / — | 0.0 % | 0.0 % |
| milan_1477 | savoy | population | ≥ 0 | 4940 | — | 0.0 % | -1.037 / -0.364 / — | 0.0 % | 0.0 % |
| milan_1477 | savoy | treasury | none | 4940 | — | — | +3.662 / +3.976 / — | 0.0 % | 0.0 % |
| milan_1477 | sicily | cohesion | 0..100 | 18000 | 0.0 % | 0.2 % | -0.394 / +0.089 / +0.072 | 0.0 % | 0.0 % |
| milan_1477 | sicily | economic_output | 0..100 | 18000 | 95.0 % | 0.0 % | +1.099 / -0.000 / +0.002 | 81.8 % | 0.0 % |
| milan_1477 | sicily | external_pressure | 0..100 | 18000 | 95.1 % | 0.0 % | +1.300 / +0.000 / +0.000 | 95.1 % | — |
| milan_1477 | sicily | legitimacy | 0..100 | 18000 | 94.9 % | 0.0 % | +0.600 / +0.000 / +0.000 | 63.6 % | 0.0 % |
| milan_1477 | sicily | military_quality | 0..100 | 18000 | 35.1 % | 22.5 % | -0.547 / +0.261 / +0.122 | 29.0 % | 22.8 % |
| milan_1477 | sicily | military_size | ≥ 0 | 18000 | — | 0.0 % | -0.287 / +0.017 / +0.009 | 0.0 % | 0.0 % |
| milan_1477 | sicily | population | ≥ 0 | 18000 | — | 0.0 % | -0.159 / -0.079 / -0.100 | — | 0.0 % |
| milan_1477 | sicily | treasury | none | 18000 | — | — | +4.330 / +6.330 / +4.515 | 0.0 % | 0.0 % |
| milan_1477 | siena | cohesion | 0..100 | 5157 | 0.0 % | 0.0 % | +1.198 / +0.001 / — | 0.0 % | 0.0 % |
| milan_1477 | siena | economic_output | 0..100 | 5157 | 83.1 % | 0.0 % | +1.300 / +0.000 / — | 73.9 % | 0.0 % |
| milan_1477 | siena | external_pressure | 0..100 | 5157 | 87.2 % | 0.0 % | +1.200 / +0.000 / — | 90.2 % | — |
| milan_1477 | siena | legitimacy | 0..100 | 5157 | 0.0 % | 1.3 % | -0.415 / -0.435 / — | 0.0 % | 1.1 % |
| milan_1477 | siena | military_quality | 0..100 | 5157 | 1.5 % | 3.8 % | +0.282 / -1.229 / — | 0.3 % | 11.8 % |
| milan_1477 | siena | military_size | ≥ 0 | 5157 | — | 72.0 % | -0.200 / +0.000 / — | 0.0 % | 14.1 % |
| milan_1477 | siena | population | ≥ 0 | 5157 | — | 0.4 % | -0.046 / +0.019 / — | 0.0 % | 1.8 % |
| milan_1477 | siena | treasury | none | 5157 | — | — | +0.361 / +3.089 / — | 0.0 % | 0.0 % |
| milan_1477 | urbino | cohesion | 0..100 | 18000 | 0.0 % | 0.0 % | +0.206 / -0.029 / +0.038 | 0.0 % | 0.0 % |
| milan_1477 | urbino | economic_output | 0..100 | 18000 | 94.1 % | 0.0 % | +1.300 / -0.000 / -0.001 | 82.7 % | 0.0 % |
| milan_1477 | urbino | external_pressure | 0..100 | 18000 | 96.1 % | 0.0 % | +1.400 / +0.000 / +0.000 | 97.2 % | — |
| milan_1477 | urbino | legitimacy | 0..100 | 18000 | 96.2 % | 0.0 % | +0.640 / +0.000 / +0.000 | 72.2 % | 0.0 % |
| milan_1477 | urbino | military_quality | 0..100 | 18000 | 95.5 % | 0.0 % | +0.560 / +0.000 / +0.000 | 47.6 % | 0.0 % |
| milan_1477 | urbino | military_size | ≥ 0 | 18000 | — | 82.5 % | -0.300 / +0.006 / -0.010 | 0.0 % | 40.6 % |
| milan_1477 | urbino | population | ≥ 0 | 18000 | — | 11.4 % | +0.041 / +0.049 / +0.035 | 0.0 % | 18.2 % |
| milan_1477 | urbino | treasury | none | 18000 | — | — | -3.736 / +1.474 / +1.840 | 0.0 % | 0.0 % |
| milan_1477 | venice | cohesion | 0..100 | 18000 | 0.0 % | 0.0 % | +0.372 / -0.006 / +0.009 | 0.0 % | 0.0 % |
| milan_1477 | venice | economic_output | 0..100 | 18000 | 98.3 % | 0.0 % | +0.300 / -0.001 / +0.004 | 90.1 % | 0.0 % |
| milan_1477 | venice | external_pressure | 0..100 | 18000 | 96.7 % | 0.0 % | +1.400 / +0.000 / +0.000 | 96.1 % | — |
| milan_1477 | venice | legitimacy | 0..100 | 18000 | 91.6 % | 0.0 % | +0.500 / +0.000 / +0.000 | 51.4 % | 0.0 % |
| milan_1477 | venice | military_quality | 0..100 | 18000 | 89.4 % | 0.0 % | +0.578 / +0.016 / +0.000 | 45.2 % | 0.0 % |
| milan_1477 | venice | military_size | ≥ 0 | 18000 | — | 0.0 % | -0.199 / -0.001 / +0.080 | 0.0 % | 0.0 % |
| milan_1477 | venice | population | ≥ 0 | 18000 | — | 0.0 % | +0.151 / +0.043 / +0.099 | 0.0 % | 0.0 % |
| milan_1477 | venice | treasury | none | 18000 | — | — | -0.090 / +2.141 / +0.879 | 0.0 % | 0.0 % |
| rome_375 | alamanni | cohesion | 0..100 | 15129 | 0.0 % | 0.3 % | -0.196 / -0.225 / +0.016 | 0.0 % | 0.0 % |
| rome_375 | alamanni | economic_output | 0..100 | 15129 | 82.8 % | 0.0 % | +1.623 / +0.000 / +0.000 | 70.5 % | 0.0 % |
| rome_375 | alamanni | external_pressure | 0..100 | 15129 | 88.1 % | 0.0 % | +1.421 / +0.000 / +0.000 | 92.0 % | — |
| rome_375 | alamanni | legitimacy | 0..100 | 15129 | 0.0 % | 44.6 % | -0.739 / -0.192 / +0.000 | 0.0 % | 40.7 % |
| rome_375 | alamanni | military_quality | 0..100 | 15129 | 11.8 % | 44.4 % | +0.466 / -0.845 / +0.000 | 5.6 % | 43.9 % |
| rome_375 | alamanni | military_size | ≥ 0 | 15129 | — | 8.5 % | -0.522 / +0.088 / +0.054 | 0.0 % | 0.8 % |
| rome_375 | alamanni | population | ≥ 0 | 15129 | — | 0.0 % | -2.133 / +1.023 / +0.241 | 0.0 % | 0.0 % |
| rome_375 | alamanni | treasury | none | 15129 | — | — | -3.037 / +13.025 / +19.810 | 0.0 % | 0.0 % |
| rome_375 | armenia | cohesion | 0..100 | 19854 | 0.0 % | 0.0 % | +0.299 / -0.525 / — | 0.0 % | 0.0 % |
| rome_375 | armenia | economic_output | 0..100 | 19854 | 93.4 % | 0.0 % | +1.298 / +0.001 / — | 80.4 % | 0.0 % |
| rome_375 | armenia | external_pressure | 0..100 | 19854 | 94.7 % | 0.0 % | +0.900 / +0.000 / — | 96.7 % | — |
| rome_375 | armenia | legitimacy | 0..100 | 19854 | 0.0 % | 32.2 % | -0.498 / -0.304 / — | 0.0 % | 39.0 % |
| rome_375 | armenia | military_quality | 0..100 | 19854 | 40.7 % | 31.9 % | +0.840 / -0.866 / — | 24.4 % | 42.6 % |
| rome_375 | armenia | military_size | ≥ 0 | 19854 | — | 0.0 % | -0.331 / +0.025 / — | 0.0 % | 0.0 % |
| rome_375 | armenia | population | ≥ 0 | 19854 | — | 0.0 % | +1.294 / +0.958 / — | 0.0 % | 0.0 % |
| rome_375 | armenia | treasury | none | 19854 | — | — | +24.110 / +45.806 / — | 0.0 % | 0.0 % |
| rome_375 | berbers | cohesion | 0..100 | 36000 | 0.0 % | 0.0 % | +0.093 / +0.034 / +0.000 | 0.0 % | 0.0 % |
| rome_375 | berbers | economic_output | 0..100 | 36000 | 95.2 % | 0.0 % | +1.440 / -0.000 / -0.001 | 82.3 % | 0.0 % |
| rome_375 | berbers | external_pressure | 0..100 | 36000 | 81.5 % | 0.0 % | +1.329 / +0.068 / +0.000 | 87.1 % | — |
| rome_375 | berbers | legitimacy | 0..100 | 36000 | 0.0 % | 64.2 % | -0.191 / -0.162 / +0.000 | 0.0 % | 71.9 % |
| rome_375 | berbers | military_quality | 0..100 | 36000 | 21.7 % | 62.7 % | +0.900 / -0.500 / +0.000 | 12.3 % | 42.3 % |
| rome_375 | berbers | military_size | ≥ 0 | 36000 | — | 0.0 % | -0.065 / -0.043 / +0.036 | 0.0 % | 0.0 % |
| rome_375 | berbers | population | ≥ 0 | 36000 | — | 0.0 % | -1.319 / -0.076 / -0.107 | — | 0.0 % |
| rome_375 | berbers | treasury | none | 36000 | — | — | -6.793 / +5.414 / +2.229 | 0.0 % | 0.0 % |
| rome_375 | burgundians | cohesion | 0..100 | 16256 | 0.0 % | 0.1 % | -0.385 / -0.211 / -0.015 | 0.0 % | 0.0 % |
| rome_375 | burgundians | economic_output | 0..100 | 16256 | 88.0 % | 0.0 % | +1.560 / -0.000 / +0.000 | 73.8 % | 0.0 % |
| rome_375 | burgundians | external_pressure | 0..100 | 16256 | 90.6 % | 0.0 % | +1.300 / +0.000 / +0.000 | 93.8 % | — |
| rome_375 | burgundians | legitimacy | 0..100 | 16256 | 0.0 % | 44.3 % | -0.750 / -0.244 / +0.000 | 0.0 % | 39.5 % |
| rome_375 | burgundians | military_quality | 0..100 | 16256 | 29.4 % | 41.6 % | +0.849 / -0.869 / +0.000 | 16.6 % | 38.4 % |
| rome_375 | burgundians | military_size | ≥ 0 | 16256 | — | 10.0 % | -0.328 / +0.078 / +0.045 | 0.0 % | 0.2 % |
| rome_375 | burgundians | population | ≥ 0 | 16256 | — | 0.0 % | -1.068 / +1.052 / +0.323 | 0.0 % | 0.0 % |
| rome_375 | burgundians | treasury | none | 16256 | — | — | +0.544 / +11.225 / +18.080 | 0.0 % | 0.0 % |
| rome_375 | eastern_jin | cohesion | 0..100 | 36000 | 0.0 % | 0.0 % | +0.495 / -0.065 / -0.004 | 0.0 % | 0.0 % |
| rome_375 | eastern_jin | economic_output | 0..100 | 36000 | 96.9 % | 0.0 % | +0.840 / -0.000 / -0.001 | 87.2 % | 0.0 % |
| rome_375 | eastern_jin | external_pressure | 0..100 | 36000 | 84.7 % | 0.0 % | +1.126 / +0.019 / +0.000 | 90.3 % | — |
| rome_375 | eastern_jin | legitimacy | 0..100 | 36000 | 0.0 % | 56.6 % | -0.200 / -0.225 / +0.000 | — | 63.1 % |
| rome_375 | eastern_jin | military_quality | 0..100 | 36000 | 14.9 % | 59.1 % | +0.767 / -0.467 / +0.000 | 4.7 % | 75.4 % |
| rome_375 | eastern_jin | military_size | ≥ 0 | 36000 | — | 0.0 % | +1.329 / +0.013 / +0.043 | 0.0 % | 0.0 % |
| rome_375 | eastern_jin | population | ≥ 0 | 36000 | — | 0.0 % | -0.137 / -0.012 / +0.058 | 0.0 % | 0.0 % |
| rome_375 | eastern_jin | treasury | none | 36000 | — | — | +321.361 / +324.232 / +323.722 | 0.0 % | 0.0 % |
| rome_375 | family | connections | 0..100 | 36000 | 1.8 % | 44.8 % | +0.201 / -0.028 / -0.080 | 20.0 % | 38.2 % |
| rome_375 | family | influence | 0..100 | 36000 | 1.5 % | 65.6 % | +0.383 / -0.098 / -0.038 | 10.8 % | 63.6 % |
| rome_375 | family | knowledge | 0..100 | 36000 | 67.5 % | 0.0 % | +1.331 / +0.067 / +0.075 | 84.8 % | — |
| rome_375 | family | wealth | 0..100 | 36000 | 6.3 % | 46.0 % | +0.212 / -0.026 / -0.142 | 36.6 % | 14.6 % |
| rome_375 | frankish_kingdom | cohesion | 0..100 | 16251 | 0.0 % | 0.1 % | -0.876 / -0.022 / -0.009 | 0.0 % | 0.0 % |
| rome_375 | frankish_kingdom | economic_output | 0..100 | 16251 | 93.1 % | 0.0 % | +5.373 / +0.446 / -0.001 | 81.1 % | 0.0 % |
| rome_375 | frankish_kingdom | external_pressure | 0..100 | 16251 | 90.2 % | 0.0 % | +1.469 / +0.490 / +0.000 | 95.4 % | — |
| rome_375 | frankish_kingdom | legitimacy | 0..100 | 16251 | 0.0 % | 56.1 % | +0.000 / -0.281 / -0.010 | — | 59.6 % |
| rome_375 | frankish_kingdom | military_quality | 0..100 | 16251 | 14.6 % | 57.5 % | -0.019 / -0.357 / -0.012 | 11.3 % | 62.4 % |
| rome_375 | frankish_kingdom | military_size | ≥ 0 | 16251 | — | 0.0 % | +0.571 / +0.120 / +0.083 | 0.0 % | 0.0 % |
| rome_375 | frankish_kingdom | population | ≥ 0 | 16251 | — | 0.0 % | -30.353 / +1.336 / +0.580 | 0.0 % | 0.0 % |
| rome_375 | frankish_kingdom | treasury | none | 16251 | — | — | -4.512 / +50.300 / +63.179 | 0.0 % | 0.0 % |
| rome_375 | franks | cohesion | 0..100 | 19607 | 0.2 % | 0.3 % | -0.141 / -0.238 / -0.000 | 0.0 % | 0.0 % |
| rome_375 | franks | economic_output | 0..100 | 19607 | 90.5 % | 0.0 % | +1.531 / -0.001 / +0.001 | 74.5 % | 0.0 % |
| rome_375 | franks | external_pressure | 0..100 | 19607 | 90.5 % | 0.0 % | +1.532 / +0.000 / +0.000 | 93.6 % | — |
| rome_375 | franks | legitimacy | 0..100 | 19607 | 0.0 % | 58.4 % | -0.736 / -0.130 / +0.000 | 0.0 % | 53.9 % |
| rome_375 | franks | military_quality | 0..100 | 19607 | 6.7 % | 58.0 % | +0.282 / -0.604 / +0.000 | 2.8 % | 49.7 % |
| rome_375 | franks | military_size | ≥ 0 | 19607 | — | 0.0 % | -0.513 / +0.094 / +0.061 | 0.0 % | 0.0 % |
| rome_375 | franks | population | ≥ 0 | 19607 | — | 0.0 % | -1.614 / +1.253 / +0.281 | 0.0 % | 0.0 % |
| rome_375 | franks | treasury | none | 19607 | — | — | +1.038 / +14.724 / +21.068 | 0.0 % | 0.0 % |
| rome_375 | guptas | cohesion | 0..100 | 36000 | 0.0 % | 0.0 % | -0.445 / +0.037 / -0.002 | 0.0 % | 0.0 % |
| rome_375 | guptas | economic_output | 0..100 | 36000 | 97.7 % | 0.0 % | +0.600 / -0.000 / +0.001 | 88.3 % | 0.0 % |
| rome_375 | guptas | external_pressure | 0..100 | 36000 | 77.4 % | 0.0 % | +0.964 / +0.184 / +0.000 | 85.5 % | — |
| rome_375 | guptas | legitimacy | 0..100 | 36000 | 0.0 % | 34.0 % | -0.018 / -0.386 / +0.000 | — | 41.9 % |
| rome_375 | guptas | military_quality | 0..100 | 36000 | 34.2 % | 36.2 % | +0.237 / -0.384 / +0.000 | 16.0 % | 68.8 % |
| rome_375 | guptas | military_size | ≥ 0 | 36000 | — | 0.0 % | +0.320 / -0.027 / +0.010 | 0.0 % | 0.0 % |
| rome_375 | guptas | population | ≥ 0 | 36000 | — | 0.0 % | -0.204 / -0.147 / +0.037 | 0.0 % | 0.0 % |
| rome_375 | guptas | treasury | none | 36000 | — | — | +240.771 / +243.442 / +243.575 | 0.0 % | 0.0 % |
| rome_375 | huns | cohesion | 0..100 | 18468 | 0.0 % | 0.1 % | -0.183 / -0.519 / +0.031 | 0.0 % | 0.0 % |
| rome_375 | huns | economic_output | 0..100 | 18468 | 87.7 % | 0.0 % | +1.700 / -0.001 / +0.000 | 74.8 % | 0.0 % |
| rome_375 | huns | external_pressure | 0..100 | 18468 | 79.9 % | 0.0 % | +1.813 / +0.042 / +0.000 | 86.8 % | — |
| rome_375 | huns | legitimacy | 0..100 | 18468 | 0.0 % | 21.8 % | -0.290 / -0.422 / +0.000 | 0.0 % | 28.9 % |
| rome_375 | huns | military_quality | 0..100 | 18468 | 66.9 % | 21.2 % | +0.240 / -0.588 / +0.000 | 38.0 % | 32.2 % |
| rome_375 | huns | military_size | ≥ 0 | 18468 | — | 0.3 % | -1.714 / -0.215 / +0.003 | 0.0 % | 0.0 % |
| rome_375 | huns | population | ≥ 0 | 18468 | — | 0.0 % | -9.481 / -0.531 / -0.150 | 0.0 % | 0.0 % |
| rome_375 | huns | treasury | none | 18468 | — | — | -33.431 / +19.155 / +15.214 | 0.0 % | 0.0 % |
| rome_375 | kushans | cohesion | 0..100 | 35893 | 1.3 % | 0.0 % | +0.953 / -0.218 / -0.048 | 0.0 % | 0.0 % |
| rome_375 | kushans | economic_output | 0..100 | 35893 | 96.4 % | 0.0 % | +1.100 / -0.000 / +0.001 | 85.5 % | 0.0 % |
| rome_375 | kushans | external_pressure | 0..100 | 35893 | 86.8 % | 0.0 % | +0.976 / +0.006 / +0.000 | 90.7 % | — |
| rome_375 | kushans | legitimacy | 0..100 | 35893 | 0.0 % | 65.2 % | -0.287 / -0.153 / +0.000 | 0.0 % | 73.3 % |
| rome_375 | kushans | military_quality | 0..100 | 35893 | 18.8 % | 65.4 % | +0.900 / -0.500 / +0.000 | 11.6 % | 60.1 % |
| rome_375 | kushans | military_size | ≥ 0 | 35893 | — | 0.0 % | +0.161 / -0.032 / -0.057 | 0.0 % | 0.0 % |
| rome_375 | kushans | population | ≥ 0 | 35893 | — | 0.0 % | -0.326 / -0.335 / -1.545 | 0.0 % | 0.0 % |
| rome_375 | kushans | treasury | none | 35893 | — | — | +20.324 / +27.729 / +20.094 | 0.0 % | 0.0 % |
| rome_375 | ostrogoth_kingdom | cohesion | 0..100 | 7832 | 0.0 % | 0.0 % | — / +0.047 / -0.007 | 0.0 % | 0.0 % |
| rome_375 | ostrogoth_kingdom | economic_output | 0..100 | 7832 | 93.4 % | 0.0 % | — / +0.694 / +0.000 | 81.8 % | 0.0 % |
| rome_375 | ostrogoth_kingdom | external_pressure | 0..100 | 7832 | 88.3 % | 0.0 % | — / +0.562 / +0.000 | 92.1 % | — |
| rome_375 | ostrogoth_kingdom | legitimacy | 0..100 | 7832 | 0.0 % | 57.9 % | — / -0.301 / -0.002 | 0.0 % | 59.6 % |
| rome_375 | ostrogoth_kingdom | military_quality | 0..100 | 7832 | 24.2 % | 55.8 % | — / -0.493 / -0.026 | 14.1 % | 46.3 % |
| rome_375 | ostrogoth_kingdom | military_size | ≥ 0 | 7832 | — | 0.0 % | — / +0.180 / +0.155 | 0.0 % | 0.0 % |
| rome_375 | ostrogoth_kingdom | population | ≥ 0 | 7832 | — | 0.0 % | — / +0.852 / +0.697 | 0.0 % | 0.0 % |
| rome_375 | ostrogoth_kingdom | treasury | none | 7832 | — | — | — / +51.792 / +51.165 | 0.0 % | 0.0 % |
| rome_375 | ostrogoths | cohesion | 0..100 | 27900 | 0.0 % | 0.0 % | -0.099 / -0.114 / -0.013 | 0.0 % | 0.0 % |
| rome_375 | ostrogoths | economic_output | 0..100 | 27900 | 92.2 % | 0.0 % | +1.639 / +0.000 / -0.000 | 80.3 % | 0.0 % |
| rome_375 | ostrogoths | external_pressure | 0..100 | 27900 | 97.8 % | 0.0 % | +0.440 / +0.000 / +0.000 | 98.3 % | — |
| rome_375 | ostrogoths | legitimacy | 0..100 | 27900 | 0.0 % | 58.8 % | -0.597 / -0.184 / +0.000 | 0.0 % | 58.2 % |
| rome_375 | ostrogoths | military_quality | 0..100 | 27900 | 29.1 % | 55.7 % | +0.700 / -0.650 / +0.000 | 17.4 % | 40.9 % |
| rome_375 | ostrogoths | military_size | ≥ 0 | 27900 | — | 0.0 % | -0.883 / +0.181 / +0.006 | 0.0 % | 0.0 % |
| rome_375 | ostrogoths | population | ≥ 0 | 27900 | — | 0.0 % | -1.395 / +1.669 / +0.471 | 0.0 % | 0.0 % |
| rome_375 | ostrogoths | treasury | none | 27900 | — | — | +0.350 / +23.302 / +28.495 | 0.0 % | 0.0 % |
| rome_375 | rome | cohesion | 0..100 | 32432 | 0.0 % | 29.8 % | +0.185 / -0.274 / +0.007 | 0.0 % | 9.1 % |
| rome_375 | rome | economic_output | 0..100 | 32432 | 94.5 % | 0.0 % | +1.036 / +0.001 / -0.000 | 68.5 % | 0.0 % |
| rome_375 | rome | expansion_count | none | 160 | — | — | — / +0.000 / +0.000 | — | — |
| rome_375 | rome | external_pressure | 0..100 | 32432 | 95.5 % | 0.0 % | +1.225 / +0.004 / -0.000 | 95.5 % | 0.0 % |
| rome_375 | rome | legitimacy | 0..100 | 32432 | 0.0 % | 0.2 % | -0.436 / -0.173 / -0.005 | 0.0 % | 0.3 % |
| rome_375 | rome | military_quality | 0..100 | 32432 | 16.1 % | 25.0 % | +0.723 / -0.387 / +0.261 | 7.4 % | 6.2 % |
| rome_375 | rome | military_size | ≥ 0 | 32432 | — | 0.0 % | -6.148 / -0.140 / -0.096 | 0.0 % | 0.0 % |
| rome_375 | rome | population | ≥ 0 | 32432 | — | 0.0 % | -90.625 / -13.945 / -5.892 | 0.0 % | 0.0 % |
| rome_375 | rome | treasury | none | 32432 | — | — | +138.351 / +185.451 / +47.549 | 0.0 % | 0.0 % |
| rome_375 | rome_east | cohesion | 0..100 | 31080 | 0.0 % | 0.0 % | +3.245 / +0.039 / +0.001 | 0.0 % | 0.0 % |
| rome_375 | rome_east | economic_output | 0..100 | 31080 | 97.7 % | 0.0 % | +3.333 / -0.000 / -0.000 | 86.3 % | 0.0 % |
| rome_375 | rome_east | expansion_count | none | 113 | — | — | — / +0.000 / +0.000 | — | — |
| rome_375 | rome_east | external_pressure | 0..100 | 31080 | 99.5 % | 0.0 % | +0.033 / +0.010 / +0.000 | 99.6 % | — |
| rome_375 | rome_east | legitimacy | 0..100 | 31080 | 0.0 % | 81.3 % | -0.893 / -0.110 / +0.000 | — | 79.6 % |
| rome_375 | rome_east | military_quality | 0..100 | 31080 | 4.7 % | 82.0 % | +1.784 / -0.461 / +0.000 | 1.5 % | 63.0 % |
| rome_375 | rome_east | military_size | ≥ 0 | 31080 | — | 0.0 % | +6.284 / +0.501 / +0.034 | 0.0 % | 0.0 % |
| rome_375 | rome_east | population | ≥ 0 | 31080 | — | 0.0 % | -27.469 / +0.064 / -0.296 | 0.0 % | 0.0 % |
| rome_375 | rome_east | treasury | none | 31080 | — | — | +322.318 / +260.460 / +248.590 | 0.0 % | 0.0 % |
| rome_375 | sassanids | cohesion | 0..100 | 36000 | 0.0 % | 0.0 % | +0.007 / -0.040 / +0.004 | 0.0 % | 0.0 % |
| rome_375 | sassanids | economic_output | 0..100 | 36000 | 97.4 % | 0.0 % | +0.759 / +0.000 / -0.001 | 87.2 % | 0.0 % |
| rome_375 | sassanids | external_pressure | 0..100 | 36000 | 86.7 % | 0.0 % | +1.377 / +0.006 / +0.000 | 94.4 % | — |
| rome_375 | sassanids | legitimacy | 0..100 | 36000 | 0.0 % | 43.9 % | -0.193 / -0.327 / +0.000 | — | 49.5 % |
| rome_375 | sassanids | military_quality | 0..100 | 36000 | 44.2 % | 43.8 % | +0.560 / -0.500 / +0.000 | 31.2 % | 55.3 % |
| rome_375 | sassanids | military_size | ≥ 0 | 36000 | — | 0.0 % | -1.668 / +0.289 / +0.108 | 0.0 % | 0.0 % |
| rome_375 | sassanids | population | ≥ 0 | 36000 | — | 0.0 % | +2.250 / +2.425 / +0.973 | 0.0 % | 0.0 % |
| rome_375 | sassanids | treasury | none | 36000 | — | — | +179.317 / +229.553 / +219.550 | 0.0 % | 0.0 % |
| rome_375 | saxons | cohesion | 0..100 | 16394 | 0.0 % | 5.1 % | -0.332 / -0.491 / +0.004 | 0.0 % | 2.0 % |
| rome_375 | saxons | economic_output | 0..100 | 16394 | 0.0 % | 83.2 % | -0.360 / +0.001 / +0.024 | 0.0 % | 53.7 % |
| rome_375 | saxons | external_pressure | 0..100 | 16394 | 2.7 % | 0.0 % | +0.000 / +0.057 / +0.000 | 59.9 % | — |
| rome_375 | saxons | legitimacy | 0..100 | 16394 | 0.0 % | 2.5 % | -0.095 / -0.471 / +0.000 | 0.0 % | 5.1 % |
| rome_375 | saxons | military_quality | 0..100 | 16394 | 72.9 % | 2.2 % | +0.900 / -0.299 / +0.000 | 70.1 % | 10.5 % |
| rome_375 | saxons | military_size | ≥ 0 | 16394 | — | 2.2 % | +0.002 / -0.006 / +0.000 | 0.0 % | 73.6 % |
| rome_375 | saxons | population | ≥ 0 | 16394 | — | 82.9 % | -3.000 / -0.000 / +0.000 | — | 16.4 % |
| rome_375 | saxons | treasury | none | 16394 | — | — | -27.849 / -28.996 / -13.205 | 0.0 % | 0.0 % |
| rome_375 | vandal_kingdom_africa | cohesion | 0..100 | 14449 | 0.0 % | 0.0 % | -0.420 / +0.035 / +0.018 | 0.0 % | 0.0 % |
| rome_375 | vandal_kingdom_africa | economic_output | 0..100 | 14449 | 92.3 % | 0.2 % | +4.336 / +0.462 / +0.014 | 80.1 % | 0.2 % |
| rome_375 | vandal_kingdom_africa | external_pressure | 0..100 | 14449 | 89.7 % | 0.0 % | +1.680 / +0.491 / +0.005 | 94.6 % | — |
| rome_375 | vandal_kingdom_africa | legitimacy | 0..100 | 14449 | 0.0 % | 67.4 % | -0.141 / -0.230 / -0.010 | 0.0 % | 68.0 % |
| rome_375 | vandal_kingdom_africa | military_quality | 0..100 | 14449 | 10.9 % | 68.0 % | +1.526 / -0.352 / -0.015 | 6.9 % | 62.3 % |
| rome_375 | vandal_kingdom_africa | military_size | ≥ 0 | 14449 | — | 0.0 % | +0.647 / +0.158 / +0.041 | 0.0 % | 0.0 % |
| rome_375 | vandal_kingdom_africa | population | ≥ 0 | 14449 | — | 0.1 % | -20.107 / +1.325 / +0.546 | 0.0 % | 0.0 % |
| rome_375 | vandal_kingdom_africa | treasury | none | 14449 | — | — | +2.989 / +48.624 / +60.411 | 0.0 % | 0.0 % |
| rome_375 | vandals | cohesion | 0..100 | 20189 | 0.0 % | 0.0 % | -0.099 / -0.198 / -0.011 | 0.0 % | 0.0 % |
| rome_375 | vandals | economic_output | 0..100 | 20189 | 88.0 % | 0.0 % | +1.610 / -0.001 / +0.000 | 74.4 % | 0.0 % |
| rome_375 | vandals | external_pressure | 0..100 | 20189 | 94.3 % | 0.0 % | +0.905 / +0.000 / +0.000 | 96.1 % | — |
| rome_375 | vandals | legitimacy | 0..100 | 20189 | 0.0 % | 54.5 % | -0.662 / -0.176 / +0.000 | 0.0 % | 52.1 % |
| rome_375 | vandals | military_quality | 0..100 | 20189 | 10.5 % | 53.9 % | +0.595 / -0.740 / +0.000 | 4.8 % | 47.3 % |
| rome_375 | vandals | military_size | ≥ 0 | 20189 | — | 3.6 % | -0.473 / +0.102 / +0.050 | 0.0 % | 0.3 % |
| rome_375 | vandals | population | ≥ 0 | 20189 | — | 0.0 % | -1.264 / +1.553 / +0.349 | 0.0 % | 0.0 % |
| rome_375 | vandals | treasury | none | 20189 | — | — | -0.086 / +17.994 / +28.609 | 0.0 % | 0.0 % |
| rome_375 | visigoth_kingdom | cohesion | 0..100 | 2327 | 0.0 % | 0.0 % | — / +0.030 / -0.028 | 0.0 % | 0.0 % |
| rome_375 | visigoth_kingdom | economic_output | 0..100 | 2327 | 93.4 % | 0.0 % | — / +0.645 / +0.000 | 82.2 % | 0.0 % |
| rome_375 | visigoth_kingdom | external_pressure | 0..100 | 2327 | 93.7 % | 0.0 % | — / +0.602 / +0.000 | 95.6 % | — |
| rome_375 | visigoth_kingdom | legitimacy | 0..100 | 2327 | 0.0 % | 54.1 % | — / -0.332 / -0.027 | — | 54.8 % |
| rome_375 | visigoth_kingdom | military_quality | 0..100 | 2327 | 24.5 % | 53.3 % | — / -0.448 / -0.259 | 13.0 % | 51.0 % |
| rome_375 | visigoth_kingdom | military_size | ≥ 0 | 2327 | — | 0.0 % | — / +0.177 / +0.189 | 0.0 % | 0.0 % |
| rome_375 | visigoth_kingdom | population | ≥ 0 | 2327 | — | 0.0 % | — / +2.766 / +1.204 | 0.0 % | 0.0 % |
| rome_375 | visigoth_kingdom | treasury | none | 2327 | — | — | — / +80.777 / +99.379 | 0.0 % | 0.0 % |
| rome_375 | visigoths | cohesion | 0..100 | 33659 | 2.0 % | 0.0 % | +0.496 / -0.136 / -0.023 | 0.0 % | 0.0 % |
| rome_375 | visigoths | economic_output | 0..100 | 33659 | 94.1 % | 0.0 % | +1.560 / +0.000 / -0.001 | 81.9 % | 0.0 % |
| rome_375 | visigoths | external_pressure | 0..100 | 33659 | 96.7 % | 0.0 % | +0.700 / +0.000 / +0.000 | 98.5 % | — |
| rome_375 | visigoths | legitimacy | 0..100 | 33659 | 0.0 % | 66.4 % | -0.603 / -0.133 / +0.000 | 0.0 % | 65.8 % |
| rome_375 | visigoths | military_quality | 0..100 | 33659 | 20.4 % | 64.7 % | +0.753 / -0.535 / +0.000 | 10.6 % | 45.5 % |
| rome_375 | visigoths | military_size | ≥ 0 | 33659 | — | 0.0 % | -0.619 / +0.138 / +0.065 | 0.0 % | 0.0 % |
| rome_375 | visigoths | population | ≥ 0 | 33659 | — | 0.0 % | -0.593 / +2.103 / +0.353 | 0.0 % | 0.0 % |
| rome_375 | visigoths | treasury | none | 33659 | — | — | +7.576 / +34.621 / +45.445 | 0.0 % | 0.0 % |
