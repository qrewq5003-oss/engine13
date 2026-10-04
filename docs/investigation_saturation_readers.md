# Читатели насыщенных метрик — A46, стадия 2

Только замер, правок нет. Вывод `src/bin/a46_readers_probe.rs` (фича `census`), 30 сидов × 300
тиков, все миры трёх сценариев, только живые тики. Разбор — `docs/TRIAGE.md`, раздел «A46,
стадия 2». Ниже — вывод пробы без правок.

- **Статический список читателей** собран обходом контента (зависимости с метрикой в `from`,
  условия и соотношения авто-дельт, `available_if` и цена действий, вехи, условия рангов, условия
  событий сценария и общего пула, победа, индикаторы, ключевые метрики, панель глобальных метрик)
  и исходников движка (каждая строка `get_metric("…")` вне тестов).
- **Состояние** — в точке проверки: пороговые — доля проверок, где условие истинно (проверки над
  отсутствующим актором не считаются); правила зависимостей — доля, где правило дало ненулевую
  дельту, и доля, где источник на границе; формулы движка — доля чтений со значением ≥ 99 или
  ≤ 1; интерфейс — доля живых тиков в самой частой полосе.
- **Ограничение:** доступность и цена действий видны только тогда, когда бот действие пробует;
  действия вне его приоритетов — «never evaluated».
- **Сверка:** место чтения, которого нет в статическом списке, печатается как UNLISTED — их 0.

# A46 stage 2 — readers of saturated metrics, 30 seeds × 300 ticks per world

## Readers per metric (static list; «live» = varies in every world: threshold readers 1–99 % true, formula sites ≤ 99 % of reads at the boundary somewhere, UI not stuck in one band)

| metric | readers | live |
|---|---|---|
| cohesion | 73 | 65 |
| economic_output | 44 | 24 |
| external_pressure | 67 | 60 |
| influence | 7 | 4 |
| knowledge | 4 | 4 |
| legitimacy | 66 | 52 |
| military_quality | 7 | 7 |

## Mechanisms the saturation holds on or off in every world

| scenario | metric | kind | reader | state |
|---|---|---|---|---|
| rome_375 | economic_output | auto-delta condition | [0] actor:rome.population if actor:rome.economic_output Less 20 | ALWAYS OFF |
| rome_375 | economic_output | auto-delta condition | [4] actor:rome.cohesion if actor:rome.economic_output Less 20 | ALWAYS OFF |
| rome_375 | economic_output | auto-delta condition | [12] family:family_wealth if actor:rome.economic_output Less 35 | ALWAYS OFF |
| rome_375 | economic_output | event gate | silk_road_caravan if actor:rome.economic_output Greater 30 | ALWAYS ON |
| rome_375 | influence | victory | family:influence >= 90 | ALWAYS OFF |
| rome_375 | economic_output | engine formula | engine/interactions.rs:1092 | sees a constant (> 99 % of reads at the boundary in every world) |
| rome_375 | external_pressure | engine formula | engine/mod.rs:1828 | sees a constant (> 99 % of reads at the boundary in every world) |
| constantinople_1430 | economic_output | dependency | economic_output_to_treasury (Deficit Some(50.0)) | ALWAYS OFF |
| constantinople_1430 | economic_output | dependency | economic_output_to_population (DeficitProportional Some(50.0)) | ALWAYS OFF |
| constantinople_1430 | economic_output | dependency | low_economic_output_to_population_decay (DeficitProportional Some(15.0)) | ALWAYS OFF |
| constantinople_1430 | economic_output | action available_if | venice_trade_deal Greater 60 | ALWAYS ON |
| constantinople_1430 | cohesion | action available_if | genoa_mercenaries Greater 50 | ALWAYS ON |
| constantinople_1430 | economic_output | action available_if | milan_bankers Greater 60 | ALWAYS ON |
| constantinople_1430 | legitimacy | action cost | milan_legitimacy cost actor:milan.legitimacy | ALWAYS OFF |
| constantinople_1430 | legitimacy | action cost | sabotage_federation cost actor:byzantium.legitimacy | never evaluated |
| constantinople_1430 | cohesion | milestone | constantinople_holds Greater 70 | never evaluated |
| constantinople_1430 | economic_output | rank condition | veneto Greater 85 | ALWAYS ON |
| constantinople_1430 | economic_output | event gate | famine if self.economic_output Less 30 | ALWAYS OFF |
| constantinople_1430 | economic_output | event gate | trade_boom if self.economic_output Greater 40 | ALWAYS ON |
| constantinople_1430 | external_pressure | engine formula | engine/mod.rs:1828 | sees a constant (> 99 % of reads at the boundary in every world) |
| milan_1477 | economic_output | dependency | economic_output_to_treasury (Deficit Some(50.0)) | ALWAYS OFF |
| milan_1477 | economic_output | dependency | economic_output_to_population (DeficitProportional Some(50.0)) | ALWAYS OFF |
| milan_1477 | economic_output | dependency | low_economic_output_to_population_decay (DeficitProportional Some(15.0)) | ALWAYS OFF |
| milan_1477 | legitimacy | auto-delta condition | [1] actor:milan.cohesion if actor:milan.legitimacy Greater 60 | ALWAYS OFF |
| milan_1477 | legitimacy | auto-delta condition | [5] actor:naples.cohesion if actor:naples.legitimacy Less 50 | ALWAYS OFF |
| milan_1477 | legitimacy | auto-delta condition | [6] actor:siena.cohesion if actor:siena.legitimacy Less 50 | ALWAYS ON |
| milan_1477 | legitimacy | auto-delta condition | [7] actor:bologna.external_pressure if actor:bologna.legitimacy Less 55 | ALWAYS OFF |
| milan_1477 | economic_output | action available_if | milan_banking_deal_florence Greater 60 | ALWAYS ON |
| milan_1477 | legitimacy | action cost | incite_baronial_revolt cost actor:milan.legitimacy | ALWAYS OFF |
| milan_1477 | legitimacy | action available_if | call_papal_arbitration Greater 70 | never evaluated |
| milan_1477 | legitimacy | milestone | milan_regency_stabilizes Greater 65 | ALWAYS OFF |
| milan_1477 | legitimacy | rank condition | lombardy Greater 75 | ALWAYS OFF |
| milan_1477 | economic_output | rank condition | veneto Greater 90 | ALWAYS ON |
| milan_1477 | economic_output | event gate | famine if self.economic_output Less 30 | ALWAYS OFF |
| milan_1477 | economic_output | event gate | trade_boom if self.economic_output Greater 40 | ALWAYS ON |
| milan_1477 | external_pressure | engine formula | engine/mod.rs:1785 | sees a constant (> 99 % of reads at the boundary in every world) |
| milan_1477 | external_pressure | engine formula | engine/mod.rs:1828 | sees a constant (> 99 % of reads at the boundary in every world) |

## Cross-check: call sites reading a watched metric that the static list does not name: 0


## milan: legitimacy by source (asked over living ticks, both worlds; actors sorted by net)

- **ferrara** net asked +65198: tag este_court +36000, tag humanism +18000, tag patronage +18000, dependency external_pressure_to_legitimacy -8755, event patronage_ferrara +2216, dependency cohesion_to_legitimacy -877
- **florence** net asked +30863: tag humanism +18000, tag medici_faction +18000, dependency external_pressure_to_legitimacy -8660, event patronage_florence +2720, event ficino_platonic_academy +600, event italian_league_against_milan +204
- **urbino** net asked +30193: tag humanism +18000, tag patronage +18000, dependency external_pressure_to_legitimacy -8679, event patronage_urbino +2316, event montefeltro_studiolo +840, dependency cohesion_to_legitimacy -296
- **mantua** net asked +28929: tag gonzaga_court +18000, tag patronage +18000, dependency external_pressure_to_legitimacy -8814, event patronage_mantua +2088, dependency cohesion_to_legitimacy -1072, event mantegna_camera_degli_sposi +720
- **sicily** net asked +26021: tag aragonese_crown +18000, tag separate_pole +18000, dependency external_pressure_to_legitimacy -8681, dependency cohesion_to_legitimacy -3362, event patronage_sicily +1788, event italian_league_against_milan +204
- **papacy** net asked +20389: tag religious_authority +36000, dependency cohesion_to_legitimacy -9757, dependency external_pressure_to_legitimacy -8784, event patronage_papacy +2625, event italian_league_against_milan +204, event charismatic_preacher +102
- **venice** net asked +12316: tag oligarchy +18000, dependency external_pressure_to_legitimacy -8723, event patronage_venice +2835, event italian_league_against_milan +204, dependency cohesion_to_legitimacy -1, engine/mod.rs:791 +0
- **bologna** net asked +10502: tag papal_vicariate +18000, dependency external_pressure_to_legitimacy -8803, event patronage_bologna +1760, dependency cohesion_to_legitimacy -437, event court_conspiracy -24, event charismatic_preacher +6
- **naples** net asked +7240: tag aragonese_crown +18000, dependency external_pressure_to_legitimacy -8821, dependency cohesion_to_legitimacy -4003, event patronage_naples +1840, event italian_league_against_milan +204, event charismatic_preacher +38
- **siena** net asked -2211: dependency external_pressure_to_legitimacy -2312, event patronage_siena +452, event court_conspiracy -216, dependency cohesion_to_legitimacy -135, engine/mod.rs:791 +0
- **savoy** net asked -4392: dependency external_pressure_to_legitimacy -2281, dependency cohesion_to_legitimacy -2217, event patronage_savoy +372, event court_conspiracy -246, event popular_uprising -48, event charismatic_preacher +28
- **genoa** net asked -8389: dependency external_pressure_to_legitimacy -8819, event patronage_genoa +1716, event court_conspiracy -1170, dependency cohesion_to_legitimacy -119, event charismatic_preacher +2, engine/mod.rs:791 +0
- **france** net asked -9464: dependency external_pressure_to_legitimacy -8535, event court_conspiracy -876, dependency cohesion_to_legitimacy -53, engine/mod.rs:791 +0
- **milan** net asked -38583: tag regency_crisis -36000, dependency external_pressure_to_legitimacy -8825, event patronage_milan +8060, event court_conspiracy -1014, event bramante_arrives_milan +600, action incite_baronial_revolt -430

## rome_375

### Threshold readers (share of evaluations true, living actors; per world: none · balanced · influence · wealth)

| metric | kind | reader | per world | class |
|---|---|---|---|---|
| legitimacy | dependency | legitimacy_to_cohesion (Deficit Some(50.0)) (source at boundary: 47 % · 50 % · 49 % · 48 %) | 89.7 % · 89.6 % · 89.4 % · 89.6 % | live |
| cohesion | dependency | cohesion_to_legitimacy (Deficit Some(50.0)) (source at boundary: 4 % · 3 % · 3 % · 3 %) | 26.1 % · 23.0 % · 22.7 % · 25.6 % | live |
| legitimacy | dependency | legitimacy_to_military_quality (Deficit Some(50.0)) (source at boundary: 47 % · 50 % · 49 % · 48 %) | 89.7 % · 89.6 % · 89.5 % · 89.7 % | live |
| cohesion | dependency | cohesion_to_economic_output (Deficit Some(50.0)) (source at boundary: 4 % · 3 % · 3 % · 3 %) | 26.1 % · 23.0 % · 22.7 % · 25.6 % | live |
| external_pressure | dependency | external_pressure_to_cohesion (Excess Some(50.0)) (source at boundary: 85 % · 86 % · 85 % · 85 %) | 91.6 % · 91.9 % · 91.5 % · 91.9 % | live |
| external_pressure | dependency | external_pressure_to_legitimacy (Excess Some(50.0)) (source at boundary: 85 % · 86 % · 85 % · 85 %) | 91.6 % · 91.9 % · 91.5 % · 91.9 % | live |
| external_pressure | dependency | external_pressure_to_military_quality (Excess Some(50.0)) (source at boundary: 85 % · 86 % · 85 % · 85 %) | 91.6 % · 91.9 % · 91.5 % · 91.9 % | live |
| external_pressure | dependency | external_pressure_to_military_size (Excess Some(50.0)) (source at boundary: 85 % · 86 % · 85 % · 85 %) | 91.6 % · 91.9 % · 91.5 % · 91.9 % | live |
| economic_output | dependency | economic_output_to_treasury (Deficit Some(50.0)) (source at boundary: 88 % · 90 % · 91 % · 89 %) | 5.7 % · 5.9 % · 6.0 % · 6.0 % | live |
| economic_output | dependency | economic_output_to_population (DeficitProportional Some(50.0)) (source at boundary: 85 % · 87 % · 88 % · 86 %) | 4.3 % · 4.1 % · 4.3 % · 4.3 % | live |
| external_pressure | dependency | siege_rally_cohesion_bonus (Bonus Some(65.0)) (source at boundary: 85 % · 86 % · 85 % · 85 %) | 89.4 % · 89.9 % · 89.4 % · 89.7 % | live |
| cohesion | dependency | cohesion_natural_decay (Excess Some(50.0)) (source at boundary: 0 % · 1 % · 1 % · 0 %) | 76.5 % · 79.8 % · 79.9 % · 77.5 % | live |
| legitimacy | dependency | low_legitimacy_to_military_quality_decay (Deficit Some(20.0)) (source at boundary: 48 % · 50 % · 50 % · 48 %) | 66.4 % · 67.6 % · 66.6 % · 66.3 % | live |
| economic_output | dependency | low_economic_output_to_population_decay (DeficitProportional Some(15.0)) (source at boundary: 85 % · 87 % · 88 % · 86 %) | 1.7 % · 1.8 % · 2.0 % · 1.8 % | live |
| economic_output | auto-delta condition | [0] actor:rome.population if actor:rome.economic_output Less 20 | 0.0 % · 0.0 % · 0.0 % · 0.0 % | ALWAYS OFF |
| external_pressure | auto-delta condition | [0] actor:rome.population if actor:rome.external_pressure Greater 70 | 95.9 % · 96.0 % · 95.3 % · 95.7 % | live |
| external_pressure | auto-delta condition | [1] actor:rome.military_size if actor:rome.external_pressure Greater 60 | 96.3 % · 96.3 % · 95.7 % · 96.1 % | live |
| external_pressure | auto-delta condition | [2] actor:rome.military_quality if actor:rome.external_pressure Greater 70 | 95.9 % · 96.0 % · 95.3 % · 95.7 % | live |
| cohesion | auto-delta condition | [3] actor:rome.economic_output if actor:rome.cohesion Less 25 | 66.5 % · 66.0 % · 60.6 % · 65.3 % | live |
| legitimacy | auto-delta condition | [4] actor:rome.cohesion if actor:rome.legitimacy Greater 70 | 0.3 % · 1.1 % · 1.9 % · 0.4 % | always off / live / live / always off |
| economic_output | auto-delta condition | [4] actor:rome.cohesion if actor:rome.economic_output Less 20 | 0.0 % · 0.0 % · 0.0 % · 0.0 % | ALWAYS OFF |
| external_pressure | auto-delta condition | [4] actor:rome.cohesion if actor:rome.external_pressure Greater 60 | 96.3 % · 96.3 % · 95.7 % · 96.1 % | live |
| cohesion | auto-delta condition | [5] actor:rome.legitimacy if actor:rome.cohesion Greater 60 | 11.9 % · 11.3 % · 13.5 % · 10.8 % | live |
| knowledge | auto-delta condition | [5] actor:rome.legitimacy if family:family_knowledge Greater 40 | 53.0 % · 94.6 % · 96.6 % · 97.6 % | live |
| legitimacy | auto-delta condition | [7] family:family_influence if actor:rome.legitimacy Greater 60 | 4.7 % · 8.6 % · 11.9 % · 5.6 % | live |
| cohesion | auto-delta condition | [7] family:family_influence if actor:rome.cohesion Less 30 | 68.0 % · 67.3 % · 61.9 % · 66.7 % | live |
| knowledge | auto-delta condition | [8] family:family_knowledge if family:family_knowledge Greater 50 | 36.4 % · 93.6 % · 95.4 % · 96.9 % | live |
| economic_output | auto-delta condition | [9] family:family_wealth if actor:rome.economic_output Greater 60 | 98.9 % · 98.9 % · 98.9 % · 98.8 % | live |
| external_pressure | auto-delta condition | [10] family:family_connections if actor:rome.external_pressure Greater 70 | 95.9 % · 95.9 % · 95.3 % · 95.7 % | live |
| cohesion | auto-delta condition | [11] family:family_connections if actor:rome.cohesion Less 40 | 71.6 % · 70.8 % · 65.9 % · 70.5 % | live |
| external_pressure | auto-delta condition | [12] family:family_wealth if actor:rome.external_pressure Greater 60 | 96.3 % · 96.3 % · 95.7 % · 96.1 % | live |
| economic_output | auto-delta condition | [12] family:family_wealth if actor:rome.economic_output Less 35 | 0.0 % · 0.0 % · 0.0 % · 0.0 % | ALWAYS OFF |
| legitimacy | auto-delta condition | [13] family:family_influence if actor:rome.legitimacy Less 40 | 85.1 % · 74.6 % · 73.2 % · 82.5 % | live |
| influence | auto-delta condition | [14] actor:rome.legitimacy if family:family_influence Greater 40 | 0.0 % · 35.1 % · 17.8 % · 0.0 % | always off / live / live / always off |
| knowledge | auto-delta condition | [16] actor:rome.economic_output if family:family_knowledge Greater 40 | 53.4 % · 94.6 % · 96.6 % · 97.6 % | live |
| influence | action cost | lay_low cost family:family_influence | — · 43.1 % · 68.6 % · 93.0 % | live |
| influence | milestone | family_rises GreaterOrEqual 60 | 0.0 % · 1.6 % · 1.1 % · 0.0 % | always off / live / live / always off |
| external_pressure | milestone | adrianople Greater 85 | 21.0 % · 21.0 % · 20.4 % · 21.0 % | live |
| influence | milestone | family_falls Less 5 | — · 1.1 % · 1.5 % · — | live |
| legitimacy | rank condition | rome_city Less 20 | 77.0 % · 68.1 % · 65.8 % · 74.3 % | live |
| cohesion | event gate | plague if self.cohesion Less 60 | 70.7 % · 66.8 % · 69.9 % · 69.7 % | live |
| economic_output | event gate | famine if self.economic_output Less 30 | 4.4 % · 4.7 % · 4.2 % · 5.3 % | live |
| legitimacy | event gate | court_conspiracy if self.legitimacy Less 60 | 95.9 % · 96.3 % · 96.0 % · 95.5 % | live |
| economic_output | event gate | trade_boom if self.economic_output Greater 40 | 95.6 % · 94.8 % · 94.5 % · 95.5 % | live |
| cohesion | event gate | popular_uprising if self.cohesion Less 30 | 10.1 % · 7.5 % · 7.5 % · 9.1 % | live |
| legitimacy | event gate | popular_uprising if self.legitimacy Less 40 | 92.4 % · 88.5 % · 94.3 % · 93.5 % | live |
| cohesion | event gate | charismatic_preacher if self.cohesion Less 40 | 17.1 % · 12.7 % · 10.7 % · 15.2 % | live |
| cohesion | event gate | gladiator_revolt if actor:rome.cohesion Less 40 | 71.4 % · 69.3 % · 68.0 % · 71.9 % | live |
| economic_output | event gate | silk_road_caravan if actor:rome.economic_output Greater 30 | 100.0 % · 100.0 % · 100.0 % · 100.0 % | ALWAYS ON |
| influence | victory | family:influence >= 90 | 0.0 % · 0.8 % · 0.4 % · 0.0 % | ALWAYS OFF |

### Formula readers (engine call sites reading the metric; share of reads at the boundary ≥ 99 or ≤ 1)

| site | metric | reads per world | at boundary per world | listed |
|---|---|---|---|---|
| application/actions.rs:203 | influence | — · 5879 · 6949 · 9000 | — · 44 % · 69 % · 93 % | content-driven |
| core/actor.rs:226 | military_quality | 231469 · 243286 · 239233 · 232664 | 75 % · 76 % · 76 % · 75 % | literal |
| engine/interactions.rs:1092 | economic_output | 2 · — · — · 113 | 100 % · — · — · 99 % | literal |
| engine/interactions.rs:1208 | economic_output | 86270 · 93912 · 87543 · 87217 | 75 % · 75 % · 77 % · 74 % | literal |
| engine/interactions.rs:1209 | economic_output | 86270 · 93912 · 87543 · 87217 | 51 % · 54 % · 62 % · 53 % | literal |
| engine/interactions.rs:1221 | external_pressure | 29575 · 30024 · 30148 · 29602 | 2 % · 2 % · 2 % · 2 % | literal |
| engine/interactions.rs:510 | external_pressure | 10229 · 11494 · 10236 · 10237 | 83 % · 85 % · 83 % · 83 % | literal |
| engine/interactions.rs:541 | cohesion | 7380 · 8237 · 7296 · 7262 | 7 % · 10 % · 7 % · 7 % | literal |
| engine/interactions.rs:542 | cohesion | 7380 · 8237 · 7296 · 7262 | 7 % · 10 % · 7 % · 7 % | content-driven |
| engine/interactions.rs:588 | external_pressure | 333376 · 350048 · 332104 · 330952 | 88 % · 88 % · 87 % · 88 % | literal |
| engine/interactions.rs:592 | economic_output | 18719 · 20395 · 20571 · 19013 | 53 % · 57 % · 58 % · 54 % | literal |
| engine/interactions.rs:611 | economic_output | 4988 · 5022 · 5058 · 4982 | 51 % · 51 % · 52 % · 51 % | literal |
| engine/interactions.rs:661 | legitimacy | 333376 · 350048 · 332104 · 330952 | 40 % · 41 % · 41 % · 40 % | literal |
| engine/interactions.rs:667 | legitimacy | 93442 · 104814 · 102118 · 95632 | 12 % · 13 % · 12 % · 12 % | literal |
| engine/interactions.rs:676 | cohesion | 46721 · 52407 · 51059 · 47816 | 1 % · 1 % · 1 % · 1 % | literal |
| engine/interactions.rs:677 | cohesion | 46721 · 52407 · 51059 · 47816 | 1 % · 1 % · 1 % · 1 % | content-driven |
| engine/interactions.rs:738 | cohesion | 317001 · 337342 · 314269 · 311181 | 4 % · 5 % · 4 % · 3 % | literal |
| engine/interactions.rs:738 | external_pressure | 335526 · 355623 · 332834 · 329766 | 90 % · 90 % · 90 % · 90 % | literal |
| engine/interactions.rs:789 | external_pressure | 14231 · 12672 · 11708 · 13242 | 93 % · 92 % · 91 % · 92 % | literal |
| engine/interactions.rs:862 | cohesion | 166688 · 175024 · 166052 · 165476 | 3 % · 4 % · 3 % · 3 % | literal |
| engine/interactions.rs:863 | cohesion | 166688 · 175024 · 166052 · 165476 | 3 % · 4 % · 3 % · 3 % | content-driven |
| engine/interactions.rs:866 | cohesion | 166688 · 175024 · 166052 · 165476 | 2 % · 3 % · 2 % · 2 % | literal |
| engine/interactions.rs:867 | cohesion | 166688 · 175024 · 166052 · 165476 | 2 % · 3 % · 2 % · 2 % | content-driven |
| engine/interactions.rs:900 | external_pressure | 115412 · 121342 · 119294 · 116122 | 86 % · 87 % · 86 % · 86 % | literal |
| engine/interactions.rs:901 | legitimacy | 115412 · 121342 · 119294 · 116122 | 48 % · 50 % · 50 % · 48 % | literal |
| engine/interactions.rs:902 | cohesion | 115412 · 121342 · 119294 · 116122 | 3 % · 3 % · 2 % · 2 % | literal |
| engine/mod.rs:1443 | cohesion | 401 · 397 · 414 · 399 | 0 % · 0 % · 0 % · 0 % | literal |
| engine/mod.rs:1444 | legitimacy | 401 · 397 · 414 · 399 | 2 % · 2 % · 6 % · 2 % | literal |
| engine/mod.rs:1573 | military_quality | 115410 · 121342 · 119294 · 116009 | 75 % · 76 % · 76 % · 75 % | literal |
| engine/mod.rs:1574 | economic_output | 115410 · 121342 · 119294 · 116009 | 93 % · 94 % · 94 % · 93 % | literal |
| engine/mod.rs:1575 | cohesion | 115410 · 121342 · 119294 · 116009 | 3 % · 3 % · 2 % · 2 % | literal |
| engine/mod.rs:1576 | legitimacy | 115410 · 121342 · 119294 · 116009 | 48 % · 50 % · 50 % · 48 % | literal |
| engine/mod.rs:1577 | external_pressure | 115410 · 121342 · 119294 · 116009 | 86 % · 87 % · 86 % · 86 % | literal |
| engine/mod.rs:1783 | legitimacy | 115564 · 121474 · 119439 · 116162 | 48 % · 50 % · 50 % · 48 % | literal |
| engine/mod.rs:1784 | cohesion | 62783 · 68842 · 66959 · 63538 | 1 % · 1 % · 1 % · 1 % | literal |
| engine/mod.rs:1785 | external_pressure | 1439 · 1250 · 1193 · 1345 | 83 % · 81 % · 80 % · 83 % | literal |
| engine/mod.rs:1789 | legitimacy | 115564 · 121474 · 119439 · 116162 | 48 % · 50 % · 50 % · 48 % | literal |
| engine/mod.rs:1790 | cohesion | 58492 · 64547 · 62659 · 59148 | 1 % · 1 % · 1 % · 1 % | literal |
| engine/mod.rs:1827 | legitimacy | 237 · 372 · 285 · 530 | 11 % · 29 % · 2 % · 53 % | literal |
| engine/mod.rs:1828 | external_pressure | 64 · 151 · 46 · 328 | 100 % · 100 % · 100 % · 100 % | literal |
| engine/mod.rs:345 | cohesion | 33816 · 34060 · 29584 · 32396 | 30 % · 32 % · 28 % · 29 % | content-driven |
| engine/mod.rs:345 | economic_output | 33816 · 34060 · 29584 · 32396 | 94 % · 94 % · 94 % · 94 % | content-driven |
| engine/mod.rs:345 | external_pressure | 50724 · 51090 · 44376 · 48594 | 92 % · 92 % · 91 % · 92 % | content-driven |
| engine/mod.rs:345 | influence | 9000 · 9000 · 9000 · 9000 | 87 % · 33 % · 57 % · 95 % | content-driven |
| engine/mod.rs:345 | knowledge | 27000 · 27000 · 27000 · 27000 | 0 % · 87 % · 89 % · 93 % | content-driven |
| engine/mod.rs:345 | legitimacy | 25362 · 25545 · 22188 · 24297 | 0 % · 0 % · 0 % · 0 % | content-driven |
| engine/mod.rs:369 | legitimacy | 8454 · 8515 · 7396 · 8099 | 0 % · 0 % · 0 % · 0 % | content-driven |
| engine/mod.rs:371 | legitimacy | 5911 · 5325 · 4510 · 5484 | 0 % · 0 % · 1 % · 0 % | content-driven |
| engine/mod.rs:505 | cohesion | 2052 · 2058 · 2030 · 2075 | 7 % · 8 % · 7 % · 7 % | content-driven |
| engine/mod.rs:505 | economic_output | 2360 · 2353 · 2390 · 2290 | 76 % · 77 % · 83 % · 77 % | content-driven |
| engine/mod.rs:505 | legitimacy | 1146 · 1097 · 1059 · 1113 | 52 % · 53 % · 53 % · 53 % | content-driven |
| engine/mod.rs:642 | influence | 8100 · 2798 · 4274 · 8100 | 90 % · 43 % · 64 % · 95 % | content-driven |
| engine/mod.rs:732 | economic_output | 115534 · 121444 · 119409 · 116132 | 93 % · 93 % · 93 % · 93 % | literal |
| engine/mod.rs:820 | cohesion | 115534 · 121444 · 119409 · 116132 | 3 % · 3 % · 2 % · 2 % | literal |
| engine/mod.rs:823 | legitimacy | 9845 · 9066 · 7875 · 9158 | 11 % · 10 % · 13 % · 12 % | literal |
| engine/mod.rs:833 | cohesion | 8891 · 8039 · 7002 · 8286 | 32 % · 36 % · 34 % · 32 % | literal |
| engine/mod.rs:841 | external_pressure | 115534 · 121444 · 119409 · 116132 | 86 % · 87 % · 86 % · 86 % | literal |
| engine/mod.rs:844 | external_pressure | 288257 · 304139 · 286475 · 285463 | 94 % · 95 % · 94 % · 94 % | literal |
| engine/mod.rs:947 | external_pressure | 428 · 428 · 441 · 428 | 15 % · 15 % · 16 % · 14 % | content-driven |
| engine/mod.rs:947 | legitimacy | 8454 · 8515 · 7396 · 8099 | 0 % · 0 % · 0 % · 0 % | content-driven |
| engine/mod.rs:953 | influence | 9000 · 4563 · 4004 · 9000 | 85 % · 22 % · 38 % · 93 % | content-driven |
| engine/mod.rs:99 | cohesion | 346602 · 364332 · 358227 · 348396 | 3 % · 2 % · 2 % · 2 % | content-driven |
| engine/mod.rs:99 | economic_output | 346602 · 364332 · 358227 · 348396 | 86 % · 88 % · 89 % · 87 % | content-driven |
| engine/mod.rs:99 | external_pressure | 577670 · 607220 · 597045 · 580660 | 85 % · 86 % · 85 % · 85 % | content-driven |
| engine/mod.rs:99 | legitimacy | 346602 · 364332 · 358227 · 348396 | 47 % · 50 % · 49 % · 48 % | content-driven |

### UI readers (share of living ticks in the single most frequent band)

| metric | kind | reader | per world | |
|---|---|---|---|---|
| external_pressure | status indicator | Западная Империя | 96 % · 96 % · 96 % · 96 % |  |
| influence | status indicator | Семья Анициев | 100 % · 57 % · 79 % · 100 % |  |
| influence | key metric (chronicler) | Семья Анициев | 100 % · 57 % · 79 % · 100 % |  |
| knowledge | key metric (chronicler) | Учёность Анициев | 45 % · 93 % · 94 % · 96 % |  |
| legitimacy | key metric (chronicler) | Власть Рима | 79 % · 69 % · 68 % · 76 % |  |
| cohesion | key metric (chronicler) | Единство Рима | 67 % · 66 % · 61 % · 66 % |  |

## constantinople_1430

### Threshold readers (share of evaluations true, living actors; per world: none · balanced · diplomacy · military)

| metric | kind | reader | per world | class |
|---|---|---|---|---|
| legitimacy | dependency | legitimacy_to_cohesion (Deficit Some(50.0)) (source at boundary: 50 % · 51 % · 51 % · 52 %) | 77.8 % · 81.0 % · 80.2 % · 85.1 % | live |
| cohesion | dependency | cohesion_to_legitimacy (Deficit Some(50.0)) (source at boundary: 18 % · 17 % · 18 % · 17 %) | 2.4 % · 4.3 % · 4.0 % · 5.8 % | live |
| legitimacy | dependency | legitimacy_to_military_quality (Deficit Some(50.0)) (source at boundary: 50 % · 51 % · 51 % · 52 %) | 77.8 % · 81.0 % · 80.3 % · 85.1 % | live |
| cohesion | dependency | cohesion_to_economic_output (Deficit Some(50.0)) (source at boundary: 18 % · 17 % · 18 % · 17 %) | 2.4 % · 4.3 % · 4.0 % · 5.8 % | live |
| external_pressure | dependency | external_pressure_to_cohesion (Excess Some(50.0)) (source at boundary: 80 % · 80 % · 81 % · 80 %) | 89.6 % · 89.7 % · 89.0 % · 93.2 % | live |
| external_pressure | dependency | external_pressure_to_legitimacy (Excess Some(50.0)) (source at boundary: 80 % · 80 % · 81 % · 80 %) | 89.6 % · 89.7 % · 89.0 % · 93.2 % | live |
| external_pressure | dependency | external_pressure_to_military_quality (Excess Some(50.0)) (source at boundary: 80 % · 80 % · 81 % · 80 %) | 89.6 % · 89.7 % · 89.0 % · 93.2 % | live |
| external_pressure | dependency | external_pressure_to_military_size (Excess Some(50.0)) (source at boundary: 80 % · 80 % · 81 % · 80 %) | 89.6 % · 89.7 % · 89.0 % · 93.2 % | live |
| economic_output | dependency | economic_output_to_treasury (Deficit Some(50.0)) (source at boundary: 96 % · 94 % · 94 % · 93 %) | 0.8 % · 0.8 % · 0.8 % · 0.9 % | ALWAYS OFF |
| economic_output | dependency | economic_output_to_population (DeficitProportional Some(50.0)) (source at boundary: 96 % · 93 % · 93 % · 93 %) | 0.8 % · 0.8 % · 0.8 % · 0.9 % | ALWAYS OFF |
| external_pressure | dependency | siege_rally_cohesion_bonus (Bonus Some(65.0)) (source at boundary: 80 % · 80 % · 81 % · 80 %) | 86.6 % · 87.0 % · 86.2 % · 90.7 % | live |
| legitimacy | dependency | low_legitimacy_to_military_quality_decay (Deficit Some(20.0)) (source at boundary: 50 % · 51 % · 51 % · 52 %) | 60.9 % · 63.7 % · 63.0 % · 65.8 % | live |
| economic_output | dependency | low_economic_output_to_population_decay (DeficitProportional Some(15.0)) (source at boundary: 96 % · 93 % · 93 % · 93 %) | 0.0 % · 0.0 % · 0.0 % · 0.0 % | ALWAYS OFF |
| external_pressure | auto-delta condition | [3] actor:byzantium.external_pressure if actor:byzantium.external_pressure Less 50 | 0.0 % · 13.1 % · 12.6 % · 33.5 % | always off / live / live / live |
| cohesion | auto-delta condition | [8] global:federation_progress if actor:venice.cohesion Greater 65 | 91.4 % · 92.0 % · 91.7 % · 92.1 % | live |
| cohesion | auto-delta condition | [8] global:federation_progress if actor:genoa.cohesion Greater 55 | 98.9 % · 99.7 % · 99.7 % · 98.8 % | live / always on / always on / live |
| legitimacy | auto-delta condition | [8] global:federation_progress if actor:milan.legitimacy Greater 58 | 20.3 % · 14.9 % · 15.2 % · 19.8 % | live |
| external_pressure | auto-delta condition | [8] global:federation_progress if actor:byzantium.external_pressure Greater 70 | 95.5 % · 83.3 % · 84.4 % · 57.4 % | live |
| economic_output | action available_if | venice_trade_deal Greater 60 | — · 100.0 % · 100.0 % · 100.0 % | ALWAYS ON |
| legitimacy | action available_if | venice_diplomacy Greater 60 | — · 14.7 % · 14.9 % · 12.5 % | live |
| cohesion | action available_if | genoa_mercenaries Greater 50 | — · 100.0 % · 100.0 % · 100.0 % | ALWAYS ON |
| economic_output | action available_if | milan_bankers Greater 60 | — · 100.0 % · 100.0 % · 100.0 % | ALWAYS ON |
| legitimacy | action available_if | milan_legitimacy Greater 60 | — · 0.5 % · 0.5 % · 21.2 % | unread / always off / always off / live |
| legitimacy | action cost | milan_legitimacy cost actor:milan.legitimacy | — · 0.0 % · 0.0 % · 0.0 % | ALWAYS OFF |
| legitimacy | action cost | sabotage_federation cost actor:byzantium.legitimacy | — · — · — · — | never evaluated |
| cohesion | milestone | constantinople_holds Greater 70 | — · — · — · — | never evaluated |
| cohesion | milestone | mamluks_watch Less 40 | 0.1 % · 0.5 % · 0.5 % · 7.2 % | always off / always off / always off / live |
| economic_output | rank condition | veneto Greater 85 | 99.6 % · 99.6 % · 99.6 % · 99.6 % | ALWAYS ON |
| cohesion | event gate | plague if self.cohesion Less 60 | 8.4 % · 13.9 % · 15.0 % · 12.2 % | live |
| economic_output | event gate | famine if self.economic_output Less 30 | 0.1 % · 0.1 % · 0.1 % · 0.2 % | ALWAYS OFF |
| legitimacy | event gate | court_conspiracy if self.legitimacy Less 60 | 82.8 % · 86.7 % · 88.6 % · 92.6 % | live |
| economic_output | event gate | trade_boom if self.economic_output Greater 40 | 99.6 % · 99.4 % · 99.6 % · 99.5 % | ALWAYS ON |
| cohesion | event gate | popular_uprising if self.cohesion Less 30 | 0.7 % · 1.7 % · 1.2 % · 2.6 % | always off / live / live / live |
| legitimacy | event gate | popular_uprising if self.legitimacy Less 40 | 100.0 % · 91.7 % · 77.8 % · 78.9 % | always on / live / live / live |
| cohesion | event gate | charismatic_preacher if self.cohesion Less 40 | 0.9 % · 2.7 % · 2.9 % · 3.1 % | always off / live / live / live |
| external_pressure | event gate | ottoman_embassy if actor:byzantium.external_pressure Greater 60 | 100.0 % · 85.3 % · 85.7 % · 66.1 % | always on / live / live / live |
| external_pressure | event gate | greek_scholars_flee if actor:byzantium.external_pressure Greater 70 | 97.3 % · 82.1 % · 84.1 % · 58.0 % | live |
| legitimacy | event gate | crusade_call if actor:papacy.legitimacy Greater 60 | 70.7 % · 32.1 % · 70.7 % · 48.3 % | live |

### Formula readers (engine call sites reading the metric; share of reads at the boundary ≥ 99 or ≤ 1)

| site | metric | reads per world | at boundary per world | listed |
|---|---|---|---|---|
| application/actions.rs:188 | cohesion | — · 6279 · 5873 · 6326 | — · 86 % · 88 % · 76 % | content-driven |
| application/actions.rs:188 | economic_output | — · 12703 · 12279 · 12021 | — · 98 % · 97 % · 99 % | content-driven |
| application/actions.rs:188 | legitimacy | — · 15565 · 15254 · 14636 | — · 44 % · 44 % · 41 % | content-driven |
| application/actions.rs:203 | legitimacy | — · 30 · 30 · 1239 | — · 0 % · 0 % · 0 % | content-driven |
| core/actor.rs:226 | military_quality | 180963 · 191194 · 190333 · 184858 | 86 % · 81 % · 82 % · 83 % | literal |
| engine/interactions.rs:1208 | economic_output | 6181 · 6967 · 6402 · 7928 | 17 % · 26 % · 19 % · 28 % | literal |
| engine/interactions.rs:1209 | economic_output | 6181 · 6967 · 6402 · 7928 | 2 % · 1 % · 1 % · 1 % | literal |
| engine/interactions.rs:510 | external_pressure | 9802 · 9551 · 9493 · 5793 | 47 % · 45 % · 49 % · 62 % | literal |
| engine/interactions.rs:541 | cohesion | 5891 · 5582 · 5587 · 3478 | 67 % · 65 % · 66 % · 38 % | literal |
| engine/interactions.rs:542 | cohesion | 5891 · 5582 · 5587 · 3478 | 67 % · 65 % · 66 % · 38 % | content-driven |
| engine/interactions.rs:588 | external_pressure | 357058 · 377114 · 377494 · 332412 | 76 % · 77 % · 79 % · 79 % | literal |
| engine/interactions.rs:592 | economic_output | 40170 · 50660 · 54820 · 32343 | 82 % · 81 % · 81 % · 72 % | literal |
| engine/interactions.rs:611 | economic_output | 14196 · 17854 · 19232 · 11218 | 80 % · 80 % · 80 % · 70 % | literal |
| engine/interactions.rs:661 | legitimacy | 357058 · 377114 · 377494 · 332412 | 45 % · 46 % · 46 % · 43 % | literal |
| engine/interactions.rs:667 | legitimacy | 161988 · 151498 · 155110 · 102436 | 24 % · 22 % · 23 % · 16 % | literal |
| engine/interactions.rs:676 | cohesion | 80994 · 75749 · 77555 · 51218 | 67 % · 64 % · 65 % · 54 % | literal |
| engine/interactions.rs:677 | cohesion | 80994 · 75749 · 77555 · 51218 | 67 % · 64 % · 65 % · 54 % | content-driven |
| engine/interactions.rs:738 | cohesion | 250463 · 247004 · 247253 · 223776 | 67 % · 66 % · 67 % · 59 % | literal |
| engine/interactions.rs:738 | external_pressure | 296297 · 292852 · 292491 · 243685 | 78 % · 77 % · 78 % · 80 % | literal |
| engine/interactions.rs:789 | external_pressure | 903 · 1562 · 1504 · 2716 | 90 % · 78 % · 84 % · 78 % | literal |
| engine/interactions.rs:862 | cohesion | 178529 · 188557 · 188747 · 166206 | 55 % · 55 % · 55 % · 56 % | literal |
| engine/interactions.rs:863 | cohesion | 178529 · 188557 · 188747 · 166206 | 55 % · 55 % · 55 % · 56 % | content-driven |
| engine/interactions.rs:866 | cohesion | 178529 · 188557 · 188747 · 166206 | 65 % · 68 % · 69 % · 68 % | literal |
| engine/interactions.rs:867 | cohesion | 178529 · 188557 · 188747 · 166206 | 65 % · 68 % · 69 % · 68 % | content-driven |
| engine/interactions.rs:900 | external_pressure | 90326 · 95420 · 94992 · 92218 | 81 % · 82 % · 82 % · 82 % | literal |
| engine/interactions.rs:901 | legitimacy | 90326 · 95420 · 94992 · 92218 | 50 % · 51 % · 51 % · 52 % | literal |
| engine/interactions.rs:902 | cohesion | 90326 · 95420 · 94992 · 92218 | 64 % · 65 % · 65 % · 66 % | literal |
| engine/mod.rs:1152 | military_quality | — · 30 · 30 · 19 | — · 80 % · 80 % · 100 % | literal |
| engine/mod.rs:1153 | military_quality | — · 30 · 30 · 19 | — · 80 % · 80 % · 100 % | content-driven |
| engine/mod.rs:1155 | cohesion | — · 30 · 30 · 19 | — · 0 % · 0 % · 0 % | literal |
| engine/mod.rs:1156 | cohesion | — · 30 · 30 · 19 | — · 0 % · 0 % · 0 % | content-driven |
| engine/mod.rs:1443 | cohesion | 189 · 202 · 203 · 210 | 0 % · 0 % · 0 % · 0 % | literal |
| engine/mod.rs:1444 | legitimacy | 189 · 202 · 203 · 210 | 0 % · 0 % · 0 % · 0 % | literal |
| engine/mod.rs:1573 | military_quality | 90326 · 95420 · 94992 · 92218 | 86 % · 81 % · 82 % · 83 % | literal |
| engine/mod.rs:1574 | economic_output | 90326 · 95420 · 94992 · 92218 | 96 % · 96 % · 96 % · 96 % | literal |
| engine/mod.rs:1575 | cohesion | 90326 · 95420 · 94992 · 92218 | 64 % · 65 % · 65 % · 66 % | literal |
| engine/mod.rs:1576 | legitimacy | 90326 · 95420 · 94992 · 92218 | 50 % · 51 % · 51 % · 52 % | literal |
| engine/mod.rs:1577 | external_pressure | 90326 · 95420 · 94992 · 92218 | 81 % · 82 % · 82 % · 82 % | literal |
| engine/mod.rs:1783 | legitimacy | 90387 · 95496 · 95065 · 92324 | 50 % · 51 % · 51 % · 52 % | literal |
| engine/mod.rs:1784 | cohesion | 49927 · 55069 · 54332 · 54261 | 77 % · 77 % · 77 % · 78 % | literal |
| engine/mod.rs:1785 | external_pressure | 121 · 319 · 296 · 479 | 93 % · 92 % · 93 % · 89 % | literal |
| engine/mod.rs:1789 | legitimacy | 90387 · 95496 · 95065 · 92324 | 50 % · 51 % · 51 % · 52 % | literal |
| engine/mod.rs:1790 | cohesion | 47366 · 51972 · 51333 · 50915 | 76 % · 77 % · 77 % · 78 % | literal |
| engine/mod.rs:1827 | legitimacy | 2316 · 1379 · 1954 · 884 | 74 % · 70 % · 81 % · 71 % | literal |
| engine/mod.rs:1828 | external_pressure | 1868 · 1032 · 1624 · 657 | 100 % · 100 % · 100 % · 100 % | literal |
| engine/mod.rs:345 | cohesion | 18000 · 18000 · 18000 · 18000 | 81 % · 83 % · 83 % · 80 % | content-driven |
| engine/mod.rs:345 | external_pressure | 2662 · 13130 · 12508 · 12652 | 89 % · 60 % · 70 % · 34 % | content-driven |
| engine/mod.rs:345 | legitimacy | 9000 · 9000 · 9000 · 9000 | 47 % · 49 % · 47 % · 46 % | content-driven |
| engine/mod.rs:505 | cohesion | 1403 · 1348 · 1441 · 1467 | 62 % · 63 % · 63 % · 68 % | content-driven |
| engine/mod.rs:505 | economic_output | 2014 · 1971 · 2023 · 1996 | 95 % · 93 % · 92 % · 92 % | content-driven |
| engine/mod.rs:505 | external_pressure | 182 · 876 · 882 · 857 | 91 % · 69 % · 76 % · 52 % | content-driven |
| engine/mod.rs:505 | legitimacy | 1168 · 1195 · 1133 · 1173 | 51 % · 53 % · 53 % · 57 % | content-driven |
| engine/mod.rs:732 | economic_output | 90318 · 95414 · 94982 · 92234 | 96 % · 94 % · 94 % · 94 % | literal |
| engine/mod.rs:820 | cohesion | 90318 · 95414 · 94982 · 92234 | 64 % · 65 % · 65 % · 66 % | literal |
| engine/mod.rs:823 | legitimacy | 379 · 1267 · 1175 · 1983 | 75 % · 32 % · 31 % · 34 % | literal |
| engine/mod.rs:833 | cohesion | 344 · 870 · 811 · 1269 | 3 % · 10 % · 9 % · 12 % | literal |
| engine/mod.rs:841 | external_pressure | 90318 · 95414 · 94982 · 92234 | 80 % · 82 % · 82 % · 82 % | literal |
| engine/mod.rs:844 | external_pressure | 284132 · 303095 · 303440 · 284524 | 79 % · 83 % · 84 % · 86 % | literal |
| engine/mod.rs:947 | cohesion | 7423 · 4508 · 4393 · 417 | 24 % · 17 % · 15 % · 0 % | content-driven |
| engine/mod.rs:953 | economic_output | 9000 · 9000 · 9000 · 9000 | 98 % · 98 % · 98 % · 98 % | content-driven |
| engine/mod.rs:99 | cohesion | 180636 · 190828 · 189964 · 184468 | 18 % · 17 % · 18 % · 17 % | content-driven |
| engine/mod.rs:99 | economic_output | 270954 · 286242 · 284946 · 276702 | 96 % · 93 % · 93 % · 93 % | content-driven |
| engine/mod.rs:99 | external_pressure | 451590 · 477070 · 474910 · 461170 | 80 % · 80 % · 81 % · 80 % | content-driven |
| engine/mod.rs:99 | legitimacy | 270954 · 286242 · 284946 · 276702 | 50 % · 51 % · 51 % · 52 % | content-driven |

### UI readers (share of living ticks in the single most frequent band)

| metric | kind | reader | per world | |
|---|---|---|---|---|
| external_pressure | status indicator | Константинополь | 93 % · 81 % · 83 % · 51 % |  |
| external_pressure | key metric (chronicler) | Константинополь | 93 % · 81 % · 83 % · 51 % |  |
| legitimacy | key metric (chronicler) | Власть базилевса | 61 % · 84 % · 80 % · 71 % |  |
| cohesion | key metric (chronicler) | Единство греков | 79 % · 79 % · 77 % · 66 % |  |

## milan_1477

### Threshold readers (share of evaluations true, living actors; per world: none · aggressive)

| metric | kind | reader | per world | class |
|---|---|---|---|---|
| legitimacy | dependency | legitimacy_to_cohesion (Deficit Some(50.0)) (source at boundary: 78 % · 80 %) | 26.2 % · 26.4 % | live |
| cohesion | dependency | cohesion_to_legitimacy (Deficit Some(50.0)) (source at boundary: 2 % · 2 %) | 20.6 % · 20.1 % | live |
| legitimacy | dependency | legitimacy_to_military_quality (Deficit Some(50.0)) (source at boundary: 78 % · 80 %) | 26.3 % · 26.4 % | live |
| cohesion | dependency | cohesion_to_economic_output (Deficit Some(50.0)) (source at boundary: 2 % · 2 %) | 20.6 % · 20.1 % | live |
| external_pressure | dependency | external_pressure_to_cohesion (Excess Some(50.0)) (source at boundary: 96 % · 96 %) | 98.1 % · 98.3 % | live |
| external_pressure | dependency | external_pressure_to_legitimacy (Excess Some(50.0)) (source at boundary: 96 % · 96 %) | 98.1 % · 98.3 % | live |
| external_pressure | dependency | external_pressure_to_military_quality (Excess Some(50.0)) (source at boundary: 96 % · 96 %) | 98.1 % · 98.3 % | live |
| external_pressure | dependency | external_pressure_to_military_size (Excess Some(50.0)) (source at boundary: 96 % · 96 %) | 98.1 % · 98.3 % | live |
| economic_output | dependency | economic_output_to_treasury (Deficit Some(50.0)) (source at boundary: 91 % · 90 %) | 0.8 % · 0.8 % | ALWAYS OFF |
| economic_output | dependency | economic_output_to_population (DeficitProportional Some(50.0)) (source at boundary: 86 % · 86 %) | 0.8 % · 0.8 % | ALWAYS OFF |
| external_pressure | dependency | siege_rally_cohesion_bonus (Bonus Some(65.0)) (source at boundary: 96 % · 96 %) | 97.4 % · 97.7 % | live |
| cohesion | dependency | cohesion_natural_decay (Excess Some(50.0)) (source at boundary: 17 % · 17 %) | 80.8 % · 81.5 % | live |
| legitimacy | dependency | low_legitimacy_to_military_quality_decay (Deficit Some(20.0)) (source at boundary: 72 % · 74 %) | 20.5 % · 21.0 % | live |
| economic_output | dependency | low_economic_output_to_population_decay (DeficitProportional Some(15.0)) (source at boundary: 86 % · 86 %) | 0.0 % · 0.0 % | ALWAYS OFF |
| cohesion | auto-delta condition | [0] actor:milan.legitimacy if actor:milan.cohesion Greater 60 | 98.9 % · 99.0 % | live / always on |
| legitimacy | auto-delta condition | [1] actor:milan.cohesion if actor:milan.legitimacy Greater 60 | 0.0 % · 0.0 % | ALWAYS OFF |
| external_pressure | auto-delta condition | [1] actor:milan.cohesion if actor:milan.external_pressure Greater 60 | 98.0 % · 99.3 % | live / always on |
| legitimacy | auto-delta condition | [2] actor:milan.external_pressure if actor:milan.legitimacy Less 40 | 98.3 % · 100.0 % | live / always on |
| legitimacy | auto-delta condition | [5] actor:naples.cohesion if actor:naples.legitimacy Less 50 | 0.0 % · 0.0 % | ALWAYS OFF |
| legitimacy | auto-delta condition | [6] actor:siena.cohesion if actor:siena.legitimacy Less 50 | 99.9 % · 100.0 % | ALWAYS ON |
| legitimacy | auto-delta condition | [7] actor:bologna.external_pressure if actor:bologna.legitimacy Less 55 | 0.1 % · 0.1 % | ALWAYS OFF |
| cohesion | auto-delta condition | [8] actor:savoy.external_pressure if actor:savoy.cohesion Less 50 | 76.2 % · 79.5 % | live |
| economic_output | action available_if | milan_banking_deal_florence Greater 60 | — · 100.0 % | ALWAYS ON |
| cohesion | action available_if | incite_baronial_revolt Less 45 | — · 98.9 % | live |
| legitimacy | action cost | incite_baronial_revolt cost actor:milan.legitimacy | — · 0.0 % | ALWAYS OFF |
| legitimacy | action available_if | call_papal_arbitration Greater 70 | — · — | never evaluated |
| external_pressure | milestone | otranto_threat_rises Greater 60 | 46.4 % · 75.0 % | live |
| external_pressure | milestone | otranto_falls Greater 80 | 20.8 % · 26.2 % | live |
| cohesion | milestone | baronial_fronde_erupts Less 35 | 1.5 % · 100.0 % | live / always on |
| legitimacy | milestone | milan_regency_stabilizes Greater 65 | 0.0 % · 0.0 % | ALWAYS OFF |
| legitimacy | milestone | milan_regency_crisis_deepens Less 25 | 19.5 % · 74.4 % | live |
| external_pressure | milestone | france_intervenes Greater 55 | 42.7 % · 45.0 % | live |
| legitimacy | rank condition | lombardy Greater 75 | 0.0 % · 0.0 % | ALWAYS OFF |
| economic_output | rank condition | veneto Greater 90 | 99.7 % · 99.8 % | ALWAYS ON |
| cohesion | event gate | plague if self.cohesion Less 60 | 53.4 % · 55.4 % | live |
| economic_output | event gate | famine if self.economic_output Less 30 | 0.0 % · 0.0 % | ALWAYS OFF |
| legitimacy | event gate | court_conspiracy if self.legitimacy Less 60 | 26.5 % · 27.5 % | live |
| economic_output | event gate | trade_boom if self.economic_output Greater 40 | 99.9 % · 99.9 % | ALWAYS ON |
| cohesion | event gate | popular_uprising if self.cohesion Less 30 | 10.3 % · 10.2 % | live |
| legitimacy | event gate | popular_uprising if self.legitimacy Less 40 | 7.1 % · 4.2 % | live |
| cohesion | event gate | charismatic_preacher if self.cohesion Less 40 | 16.6 % · 14.5 % | live |

### Formula readers (engine call sites reading the metric; share of reads at the boundary ≥ 99 or ≤ 1)

| site | metric | reads per world | at boundary per world | listed |
|---|---|---|---|---|
| application/actions.rs:188 | cohesion | — · 87 | — · 0 % | content-driven |
| application/actions.rs:188 | economic_output | — · 6 | — · 67 % | content-driven |
| application/actions.rs:203 | legitimacy | — · 86 | — · 0 % | content-driven |
| core/actor.rs:226 | military_quality | 225842 · 226430 | 77 % · 77 % | literal |
| engine/interactions.rs:1208 | economic_output | 3134 · 3176 | 5 % · 5 % | literal |
| engine/interactions.rs:1209 | economic_output | 3134 · 3176 | 2 % · 2 % | literal |
| engine/interactions.rs:510 | external_pressure | 14628 · 14729 | 90 % · 90 % | literal |
| engine/interactions.rs:541 | cohesion | 6480 · 6448 | 18 % · 19 % | literal |
| engine/interactions.rs:542 | cohesion | 6480 · 6448 | 18 % · 19 % | content-driven |
| engine/interactions.rs:588 | external_pressure | 451024 · 452200 | 96 % · 96 % | literal |
| engine/interactions.rs:592 | economic_output | 9500 · 8400 | 15 % · 12 % | literal |
| engine/interactions.rs:611 | economic_output | 3794 · 3360 | 13 % · 11 % | literal |
| engine/interactions.rs:661 | legitimacy | 451024 · 452200 | 72 % · 74 % | literal |
| engine/interactions.rs:667 | legitimacy | 162764 · 163768 | 58 % · 60 % | literal |
| engine/interactions.rs:676 | cohesion | 81382 · 81884 | 48 % · 49 % | literal |
| engine/interactions.rs:677 | cohesion | 81382 · 81884 | 48 % · 49 % | content-driven |
| engine/interactions.rs:738 | cohesion | 383128 · 384665 | 17 % · 17 % | literal |
| engine/interactions.rs:738 | external_pressure | 391875 · 392245 | 96 % · 96 % | literal |
| engine/interactions.rs:789 | external_pressure | 13003 · 12522 | 96 % · 95 % | literal |
| engine/interactions.rs:862 | cohesion | 225512 · 226100 | 19 % · 19 % | literal |
| engine/interactions.rs:863 | cohesion | 225512 · 226100 | 19 % · 19 % | content-driven |
| engine/interactions.rs:866 | cohesion | 225512 · 226100 | 20 % · 21 % | literal |
| engine/interactions.rs:867 | cohesion | 225512 · 226100 | 20 % · 21 % | content-driven |
| engine/interactions.rs:900 | external_pressure | 112726 · 113020 | 96 % · 96 % | literal |
| engine/interactions.rs:901 | legitimacy | 112726 · 113020 | 79 % · 80 % | literal |
| engine/interactions.rs:902 | cohesion | 112726 · 113020 | 9 % · 9 % | literal |
| engine/mod.rs:1443 | cohesion | 270 · 270 | 0 % · 0 % | literal |
| engine/mod.rs:1444 | legitimacy | 270 · 270 | 0 % · 0 % | literal |
| engine/mod.rs:1573 | military_quality | 112726 · 113020 | 77 % · 77 % | literal |
| engine/mod.rs:1574 | economic_output | 112726 · 113020 | 96 % · 96 % | literal |
| engine/mod.rs:1575 | cohesion | 112726 · 113020 | 9 % · 9 % | literal |
| engine/mod.rs:1576 | legitimacy | 112726 · 113020 | 79 % · 80 % | literal |
| engine/mod.rs:1577 | external_pressure | 112726 · 113020 | 96 % · 96 % | literal |
| engine/mod.rs:1783 | legitimacy | 112786 · 113080 | 79 % · 80 % | literal |
| engine/mod.rs:1784 | cohesion | 20823 · 21558 | 37 % · 38 % | literal |
| engine/mod.rs:1785 | external_pressure | 133 · 136 | 100 % · 100 % | literal |
| engine/mod.rs:1789 | legitimacy | 112786 · 113080 | 79 % · 80 % | literal |
| engine/mod.rs:1790 | cohesion | 19762 · 20536 | 38 % · 40 % | literal |
| engine/mod.rs:1827 | legitimacy | 31600 · 31293 | 92 % · 91 % | literal |
| engine/mod.rs:1828 | external_pressure | 105 · 108 | 100 % · 100 % | literal |
| engine/mod.rs:345 | cohesion | 11595 · 11405 | 70 % · 73 % | content-driven |
| engine/mod.rs:345 | external_pressure | 9000 · 9000 | 97 % · 97 % | content-driven |
| engine/mod.rs:345 | legitimacy | 38372 · 38845 | 69 % · 72 % | content-driven |
| engine/mod.rs:505 | cohesion | 1218 · 1283 | 16 % · 17 % | content-driven |
| engine/mod.rs:505 | economic_output | 1928 · 1904 | 86 % · 88 % | content-driven |
| engine/mod.rs:505 | legitimacy | 1208 · 1141 | 71 % · 72 % | content-driven |
| engine/mod.rs:732 | economic_output | 112756 · 113050 | 95 % · 95 % | literal |
| engine/mod.rs:820 | cohesion | 112756 · 113050 | 9 % · 9 % | literal |
| engine/mod.rs:823 | legitimacy | 10768 · 10235 | 68 % · 71 % | literal |
| engine/mod.rs:833 | cohesion | 695 · 687 | 7 % · 9 % | literal |
| engine/mod.rs:841 | external_pressure | 112756 · 113050 | 96 % · 96 % | literal |
| engine/mod.rs:844 | external_pressure | 420713 · 422683 | 99 % · 99 % | literal |
| engine/mod.rs:947 | cohesion | 5227 · 90 | 0 % · 0 % | content-driven |
| engine/mod.rs:947 | external_pressure | 693 · 549 | 1 % · 2 % | content-driven |
| engine/mod.rs:947 | legitimacy | 9466 · 9121 | 78 % · 87 % | content-driven |
| engine/mod.rs:953 | economic_output | 9000 · 9000 | 98 % · 98 % | content-driven |
| engine/mod.rs:953 | legitimacy | 9000 · 9000 | 83 % · 88 % | content-driven |
| engine/mod.rs:99 | cohesion | 338268 · 339150 | 7 % · 7 % | content-driven |
| engine/mod.rs:99 | economic_output | 338268 · 339150 | 88 % · 88 % | content-driven |
| engine/mod.rs:99 | external_pressure | 563780 · 565250 | 96 % · 96 % | content-driven |
| engine/mod.rs:99 | legitimacy | 338268 · 339150 | 76 % · 78 % | content-driven |

### UI readers (share of living ticks in the single most frequent band)

| metric | kind | reader | per world | |
|---|---|---|---|---|
| legitimacy | status indicator | Регентство в Милане | 97 % · 100 % |  |
| external_pressure | status indicator | Неаполь | 98 % · 99 % |  |
| cohesion | status indicator | Баронская фронда | 50 % · 53 % |  |
| legitimacy | key metric (chronicler) | Регентство в Милане | 97 % · 100 % |  |
| cohesion | key metric (chronicler) | Единство Милана | 98 % · 99 % |  |
| external_pressure | key metric (chronicler) | Давление на Милан | 98 % · 99 % |  |
| external_pressure | key metric (chronicler) | Неаполь | 98 % · 99 % |  |
| cohesion | key metric (chronicler) | Баронская фронда | 50 % · 53 % |  |

