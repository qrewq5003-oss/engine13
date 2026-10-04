# `economic_output`: разложение и контрфакты — A46, стадия 3

Только замер, правок контента нет. Вывод `src/bin/a46_eco_probe.rs` (фича `census`), 30 сидов ×
300 тиков, все миры трёх сценариев, живые тики. Разбор — `docs/TRIAGE.md`, раздел «A46, стадия 3».

- **Разложение:** каждая запись в `economic_output` (приёмник записей A37, все метрики). Срезанное
  клампом в тике делится между положительными писателями этого тика пропорционально их запросу.
- **Контрфакты в памяти:** (а) тег-канал по `economic_output` × 0.5, (б) × 0.25 (модификаторы
  тегов целые — масштаб через крючок `census::tag_modifier`, только под фичей); (в) отток,
  пропорциональный уровню: правило-зависимость `economic_output → economic_output`, Linear,
  коэффициент −k, k = средний запрошенный приток на живой актор-тик / 70 (равновесие против
  одного только этого притока — 70; притоки измерены на 10 сидах базы).
- Пороги читателей не тронуты.

# A46 stage 3 — economic_output decomposition, 30 seeds × 300 ticks per world

## rome_375 — economic_output by writer (pooled over the worlds; living actors)

mean asked inflow per living actor-tick +8.484; inflow cut at the ceiling 80.9 %

| writer | asked inflow | share of inflow | asked outflow | inflow cut by the ceiling (attributed) |
|---|---|---|---|---|
| tag silk_road | +898718 | 22.4 % | -0 | 80.8 % |
| tag golden_age | +897166 | 22.4 % | -0 | 80.9 % |
| tag trade_networks | +451278 | 11.3 % | -0 | 80.4 % |
| tag coinage | +448317 | 11.2 % | -0 | 81.0 % |
| tag raid_economy | +0 | 0.0 % | -447541 | — |
| tag roman_contact | +445700 | 11.1 % | -0 | 81.3 % |
| tag pastoral | +442183 | 11.0 % | -0 | 81.6 % |
| tag roman_border | +262307 | 6.5 % | -0 | 85.0 % |
| engine/mod.rs:374 | +65615 | 1.6 % | -6972 | 81.5 % |
| dependency cohesion_to_economic_output | +0 | 0.0 % | -65868 | — |
| dependency military_size_to_economic_output | +0 | 0.0 % | -65575 | — |
| tag roman_frontier | +32464 | 0.8 % | -0 | 80.9 % |
| event flood | +0 | 0.0 % | -32220 | — |
| event trade_boom | +16740 | 0.4 % | -0 | 86.7 % |

## constantinople_1430 — economic_output by writer (pooled over the worlds; living actors)

mean asked inflow per living actor-tick +9.245; inflow cut at the ceiling 92.2 %

| writer | asked inflow | share of inflow | asked outflow | inflow cut by the ceiling (attributed) |
|---|---|---|---|---|
| tag maritime | +743386 | 21.6 % | -0 | 91.9 % |
| tag trade_empire | +742476 | 21.5 % | -0 | 92.0 % |
| tag banking | +739386 | 21.4 % | -0 | 92.4 % |
| tag greek_culture | +371570 | 10.8 % | -0 | 91.9 % |
| tag trade | +370505 | 10.7 % | -0 | 92.2 % |
| tag galaata | +369054 | 10.7 % | -0 | 92.5 % |
| dependency military_size_to_economic_output | +0 | 0.0 % | -97933 | — |
| engine/mod.rs:374 | +63107 | 1.8 % | -0 | 92.2 % |
| action venice_trade_deal | +29024 | 0.8 % | -10920 | 95.1 % |
| event flood | +0 | 0.0 % | -34224 | — |
| event piracy | +0 | 0.0 % | -19990 | — |
| event trade_boom | +18435 | 0.5 % | -0 | 93.4 % |
| action milan_bankers | +0 | 0.0 % | -14445 | — |
| event earthquake | +0 | 0.0 % | -10600 | — |

## milan_1477 — economic_output by writer (pooled over the worlds; living actors)

mean asked inflow per living actor-tick +6.275; inflow cut at the ceiling 86.1 %

| writer | asked inflow | share of inflow | asked outflow | inflow cut by the ceiling (attributed) |
|---|---|---|---|---|
| tag maritime | +450298 | 31.8 % | -0 | 85.9 % |
| tag trade_empire | +449884 | 31.8 % | -0 | 85.9 % |
| tag banking | +449742 | 31.7 % | -0 | 86.0 % |
| dependency military_size_to_economic_output | +0 | 0.0 % | -83816 | — |
| engine/mod.rs:374 | +36371 | 2.6 % | -8231 | 88.2 % |
| dependency cohesion_to_economic_output | +0 | 0.0 % | -33499 | — |
| tag medici_faction | +18000 | 1.3 % | -0 | 93.0 % |
| event flood | +0 | 0.0 % | -16404 | — |
| event piracy | +0 | 0.0 % | -9995 | — |
| event trade_boom | +8540 | 0.6 % | -0 | 91.2 % |
| event earthquake | +0 | 0.0 % | -5580 | — |
| auto_delta[3] actor:milan.economic_output | +2547 | 0.2 % | -260 | 92.4 % |
| dependency population_to_economic_output | +1080 | 0.1 % | -0 | 96.1 % |
| event plague | +0 | 0.0 % | -585 | — |


# A46 stage 3 — economic_output counterfactuals, 30 seeds × 300 ticks per world

main writer scaled in (a), (b): the tag channel — every tag's economic_output modifier

(c) k per scenario = mean asked inflow per living actor-tick / 70 (measured on 10 seeds of the base):
- constantinople_1430: inflow +9.245 → k = 0.13207
- milan_1477: inflow +6.270 → k = 0.08957
- rome_375: inflow +8.486 → k = 0.12122

## rome_375

| variant | world | eo at ceiling / floor | eo readers held in base → live now | famine · trade_boom · silk_road | treasury @50 / @150 p10/50/90 | population @50 / @150 p10/50/90 | deaths: rome · byz · ott · milan · all | split 40 | wins, tick p10/50/90, on 40–43; np wins with Ottomans alive | endings held·fed·fell·none | Milan @150 aggressive: leg, treasury, army, eo (p50) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | none | 90.6 % / 2.7 % | 4 → — | 46 · 811 · 455 | -1289/297/13279 / 455/4950/38804 | 50/428/4155 / 253/869/4197 | 6 · 0 · 0 · 0 · 222 | 30 / 30 | 0, —, 0; 0 | — | — |
| base | balanced | 90.8 % / 3.0 % | 4 → — | 44 · 850 · 512 | -1296/298/13277 / 354/4001/38599 | 53/390/4152 / 231/774/4174 | 5 · 0 · 0 · 0 · 172 | 30 / 30 | 23, 30/36/183, 1; 0 | — | — |
| base | influence | 90.5 % / 3.1 % | 4 → — | 46 · 834 · 415 | -1267/294/13274 / 364/3830/39064 | 52/371/4163 / 230/785/4186 | 13 · 0 · 0 · 0 · 190 | 30 / 30 | 15, 30/32/77, 0; 0 | — | — |
| base | wealth | 90.3 % / 2.8 % | 4 → — | 50 · 854 · 455 | -1271/324/13279 / 421/4899/38727 | 52/417/4137 / 244/853/4175 | 8 · 0 · 0 · 0 · 215 | 30 / 30 | 0, —, 0; 0 | — | — |
| (a) × 0.5 | none | 83.7 % / 3.0 % | 4 → 0 | 59 · 850 · 438 | -1336/150/13048 / 7/4595/38541 | 41/364/4064 / 203/825/4199 | 9 · 0 · 0 · 0 · 234 | 30 / 30 | 0, —, 0; 0 | — | — |
| (a) × 0.5 | balanced | 84.2 % / 3.7 % | 4 → 0 | 62 · 806 · 520 | -1310/176/13023 / -36/4363/38374 | 39/358/4004 / 189/779/4191 | 2 · 0 · 0 · 0 · 192 | 30 / 30 | 21, 30/47/111, 0; 0 | — | — |
| (a) × 0.5 | influence | 84.4 % / 3.4 % | 4 → 0 | 55 · 867 · 467 | -1306/127/13035 / -49/4105/39294 | 34/331/4108 / 178/777/4197 | 7 · 0 · 0 · 0 · 201 | 30 / 30 | 20, 30/45/84, 1; 0 | — | — |
| (a) × 0.5 | wealth | 84.1 % / 2.9 % | 4 → 0 | 58 · 808 · 470 | -1336/150/13048 / 26/4457/38357 | 39/366/4064 / 211/827/4186 | 8 · 0 · 0 · 0 · 228 | 30 / 30 | 0, —, 0; 0 | — | — |
| (b) × 0.25 | none | 70.6 % / 3.4 % | 4 → 0 | 88 · 807 · 399 | -1314/-142/12622 / -547/3952/37819 | 0/311/4074 / 158/773/4182 | 16 · 0 · 0 · 0 · 262 | 30 / 30 | 0, —, 0; 0 | — | — |
| (b) × 0.25 | balanced | 71.7 % / 3.7 % | 4 → 0 | 80 · 797 · 455 | -1360/-158/12622 / -503/3878/37771 | 0/242/4074 / 168/756/4156 | 8 · 0 · 0 · 0 · 223 | 30 / 30 | 21, 30/37/69, 2; 0 | — | — |
| (b) × 0.25 | influence | 71.6 % / 3.9 % | 4 → 0 | 93 · 760 · 475 | -1351/-175/12664 / -455/4112/37998 | 0/229/4114 / 168/766/4182 | 5 · 0 · 0 · 0 · 217 | 30 / 30 | 16, 30/35/121, 1; 0 | — | — |
| (b) × 0.25 | wealth | 71.0 % / 3.4 % | 4 → 0 | 80 · 795 · 482 | -1328/-149/12630 / -415/4222/37895 | 0/228/4102 / 168/771/4183 | 11 · 0 · 0 · 0 · 251 | 30 / 30 | 0, —, 0; 0 | — | — |
| (c) proportional outflow | none | 0.0 % / 3.2 % | 4 → 0 | 146 · 743 · 452 | -1446/-267/7535 / -1222/1205/20848 | 0/232/4000 / 134/762/4000 | 11 · 0 · 0 · 0 · 293 | 30 / 30 | 0, —, 0; 0 | — | — |
| (c) proportional outflow | balanced | 0.0 % / 3.7 % | 4 → 0 | 148 · 723 · 504 | -1451/-272/7388 / -1169/1193/20796 | 0/217/4000 / 145/749/4028 | 6 · 0 · 0 · 0 · 262 | 30 / 30 | 24, 30/57/99, 0; 0 | — | — |
| (c) proportional outflow | influence | 0.0 % / 3.8 % | 4 → 0 | 158 · 764 · 457 | -1447/-271/7221 / -1241/1224/21080 | 0/219/4000 / 150/771/4120 | 7 · 0 · 0 · 0 · 263 | 30 / 30 | 16, 30/38/88, 1; 0 | — | — |
| (c) proportional outflow | wealth | 0.0 % / 3.4 % | 4 → 0 | 147 · 749 · 472 | -1455/-267/7518 / -1207/1161/20447 | 0/244/4000 / 124/759/4026 | 7 · 0 · 0 · 0 · 279 | 30 / 30 | 0, —, 0; 0 | — | — |


## constantinople_1430

| variant | world | eo at ceiling / floor | eo readers held in base → live now | famine · trade_boom · silk_road | treasury @50 / @150 p10/50/90 | population @50 / @150 p10/50/90 | deaths: rome · byz · ott · milan · all | split 40 | wins, tick p10/50/90, on 40–43; np wins with Ottomans alive | endings held·fed·fell·none | Milan @150 aggressive: leg, treasury, army, eo (p50) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | none | 96.4 % / 0.0 % | 6 → — | 1 · 938 · 0 | 36/565/2639 / -3/1510/9539 | 80/188/788 / 75/220/1171 | 0 · 30 · 4 · 0 · 61 | — | 0, —, 0; 0 | 11·0·19·0 | — |
| base | balanced | 96.4 % / 0.0 % | 8 → — | 1 · 886 · 0 | -92/289/2608 / -44/1012/8636 | 65/185/885 / 68/236/1004 | 0 · 25 · 16 · 0 · 76 | — | 22, 42/105/196, 5; 0 | 24·0·0·6 | — |
| base | diplomacy | 96.5 % / 0.0 % | 8 → — | 1 · 947 · 0 | -86/308/2613 / -25/1051/8658 | 66/188/857 / 65/235/993 | 0 · 22 · 11 · 0 · 73 | — | 21, 42/75/221, 3; 0 | 25·0·2·3 | — |
| base | military | 96.4 % / 0.0 % | 8 → — | 2 · 919 · 0 | -154/431/2798 / -390/1026/9171 | 105/261/955 / 90/305/1029 | 0 · 26 · 21 · 0 · 106 | — | 25, 42/115/169, 4; 0 | 26·0·0·4 | — |
| (a) × 0.5 | none | 92.6 % / 0.0 % | 6 → 2 | 5 · 881 · 0 | 20/502/2533 / 205/1214/8564 | 80/197/788 / 68/185/784 | 0 · 30 · 0 · 0 · 60 | — | 0, —, 0; 0 | 7·0·23·0 | — |
| (a) × 0.5 | balanced | 92.6 % / 0.0 % | 8 → 2 | 7 · 887 · 0 | -122/162/2525 / -36/772/8548 | 65/181/826 / 66/210/959 | 0 · 25 · 7 · 0 · 75 | — | 11, 43/99/198, 2; 0 | 26·0·2·2 | — |
| (a) × 0.5 | diplomacy | 92.7 % / 0.0 % | 8 → 2 | 5 · 897 · 0 | -118/173/2488 / 21/877/8252 | 65/183/792 / 69/221/998 | 0 · 28 · 8 · 0 · 84 | — | 13, 43/95/191, 2; 0 | 20·1·6·3 | — |
| (a) × 0.5 | military | 92.4 % / 0.0 % | 8 → 3 | 6 · 898 · 0 | -353/273/2637 / -216/716/8948 | 90/244/945 / 90/271/1009 | 0 · 29 · 19 · 0 · 111 | — | 25, 42/126/151, 6; 0 | 23·0·0·7 | — |
| (b) × 0.25 | none | 82.8 % / 0.3 % | 6 → 5 | 24 · 858 · 0 | -215/334/2320 / 45/1110/40038 | 61/184/780 / 63/181/3976 | 0 · 30 · 0 · 0 · 62 | — | 0, —, 0; 0 | 13·0·17·0 | — |
| (b) × 0.25 | balanced | 82.0 % / 0.3 % | 8 → 6 | 20 · 871 · 0 | -339/64/2166 / -45/586/32615 | 28/180/875 / 48/180/2850 | 0 · 22 · 0 · 0 · 65 | — | 14, 42/47/171, 6; 0 | 21·0·2·7 | — |
| (b) × 0.25 | diplomacy | 81.9 % / 0.2 % | 8 → 6 | 17 · 937 · 0 | -322/77/2114 / -24/628/33484 | 20/181/809 / 58/189/2587 | 0 · 22 · 1 · 0 · 65 | — | 12, 42/96/143, 4; 0 | 24·0·2·4 | — |
| (b) × 0.25 | military | 82.4 % / 0.3 % | 8 → 6 | 18 · 851 · 0 | -530/166/2372 / -160/621/9125 | 80/240/959 / 85/255/1061 | 0 · 29 · 8 · 0 · 120 | — | 17, 58/102/167, 1; 0 | 29·0·0·1 | — |
| (c) proportional outflow | none | 0.0 % / 0.0 % | 6 → 5 | 37 · 905 · 0 | -522/166/6923 / -179/507/22396 | 44/184/3630 / 53/191/3938 | 0 · 30 · 0 · 0 · 61 | — | 0, —, 0; 0 | 8·0·22·0 | — |
| (c) proportional outflow | balanced | 0.0 % / 0.0 % | 8 → 7 | 29 · 863 · 0 | -508/-8/1173 / -572/318/22461 | 34/180/812 / 55/189/3360 | 0 · 27 · 0 · 0 · 67 | — | 1, 42/42/42, 1; 0 | 25·0·4·1 | — |
| (c) proportional outflow | diplomacy | 0.0 % / 0.0 % | 8 → 7 | 16 · 891 · 0 | -424/13/1305 / -536/335/16687 | 40/180/800 / 51/202/3165 | 0 · 29 · 3 · 0 · 75 | — | 1, 42/42/42, 1; 0 | 25·0·4·1 | — |
| (c) proportional outflow | military | 0.0 % / 0.0 % | 8 → 7 | 29 · 883 · 0 | -605/18/1311 / -655/267/4854 | 78/250/943 / 80/246/1026 | 0 · 30 · 6 · 0 · 131 | — | 3, 51/158/166, 0; 0 | 30·0·0·0 | — |

- revived in a: dependency economic_output_to_treasury (Deficit Some(50.0)) — none 1 %, balanced 1 %, diplomacy 1 %, military 2 %
- revived in a: dependency economic_output_to_population (DeficitProportional Some(50.0)) — none 2 %, balanced 1 %, diplomacy 1 %, military 2 %
- revived in a: event gate trade_boom if self.economic_output Greater 40 — military 98 %
- revived in b: dependency economic_output_to_treasury (Deficit Some(50.0)) — none 4 %, balanced 3 %, diplomacy 3 %, military 4 %
- revived in b: dependency economic_output_to_population (DeficitProportional Some(50.0)) — none 3 %, balanced 3 %, diplomacy 3 %, military 4 %
- revived in b: dependency low_economic_output_to_population_decay (DeficitProportional Some(15.0)) — military 1 %
- revived in b: action available_if milan_bankers Greater 60 — balanced 98 %, diplomacy 97 %
- revived in b: rank condition veneto Greater 85 — none 98 %, balanced 98 %, diplomacy 95 %, military 98 %
- revived in b: event gate famine if self.economic_output Less 30 — none 2 %, balanced 2 %, diplomacy 2 %, military 2 %
- revived in b: event gate trade_boom if self.economic_output Greater 40 — none 97 %, balanced 97 %, diplomacy 98 %, military 97 %
- revived in c: dependency economic_output_to_treasury (Deficit Some(50.0)) — none 4 %, balanced 4 %, diplomacy 3 %, military 4 %
- revived in c: dependency economic_output_to_population (DeficitProportional Some(50.0)) — none 3 %, balanced 4 %, diplomacy 3 %, military 4 %
- revived in c: dependency low_economic_output_to_population_decay (DeficitProportional Some(15.0)) — none 2 %, balanced 2 %, diplomacy 2 %, military 2 %
- revived in c: action available_if venice_trade_deal Greater 60 — balanced 90 %, diplomacy 88 %, military 91 %
- revived in c: action available_if milan_bankers Greater 60 — balanced 90 %, diplomacy 90 %, military 97 %
- revived in c: event gate famine if self.economic_output Less 30 — none 4 %, balanced 3 %, diplomacy 2 %, military 3 %
- revived in c: event gate trade_boom if self.economic_output Greater 40 — none 97 %, balanced 98 %, diplomacy 97 %, military 96 %

## milan_1477

| variant | world | eo at ceiling / floor | eo readers held in base → live now | famine · trade_boom · silk_road | treasury @50 / @150 p10/50/90 | population @50 / @150 p10/50/90 | deaths: rome · byz · ott · milan · all | split 40 | wins, tick p10/50/90, on 40–43; np wins with Ottomans alive | endings held·fed·fell·none | Milan @150 aggressive: leg, treasury, army, eo (p50) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | none | 95.6 % / 0.0 % | 6 → — | 0 · 857 · 0 | 48/345/1663 / 111/614/4921 | 18/100/555 / 7/147/541 | 0 · 0 · 0 · 0 · 60 | — | 0, —, 0; 0 | — | — |
| base | aggressive | 95.6 % / 0.0 % | 7 → — | 0 · 853 · 0 | 36/300/1568 / 47/574/4781 | 20/99/530 / 13/147/513 | 0 · 0 · 0 · 0 · 60 | — | 0, —, 0; 0 | — | 0, 47, 21, 100 |
| (a) × 0.5 | none | 88.3 % / 0.0 % | 6 → 2 | 0 · 865 · 0 | 25/303/1555 / 80/695/4699 | 16/93/555 / 11/134/528 | 0 · 0 · 0 · 0 · 61 | — | 0, —, 0; 0 | — | — |
| (a) × 0.5 | aggressive | 88.4 % / 0.0 % | 7 → 2 | 0 · 873 · 0 | -0/234/1464 / 41/506/4669 | 16/98/530 / 13/147/517 | 0 · 0 · 0 · 0 · 61 | — | 0, —, 0; 0 | — | 0, 43, 22, 100 |
| (b) × 0.25 | none | 68.0 % / 0.0 % | 6 → 4 | 6 · 931 · 0 | -100/177/1411 / -87/495/4582 | 4/95/555 / 2/117/532 | 0 · 0 · 0 · 0 · 60 | — | 0, —, 0; 0 | — | — |
| (b) × 0.25 | aggressive | 68.1 % / 0.0 % | 7 → 4 | 10 · 878 · 0 | -108/111/1289 / -99/389/4336 | 8/101/530 / 3/116/533 | 0 · 0 · 0 · 0 · 60 | — | 0, —, 0; 0 | — | 0, 43, 24, 100 |
| (c) proportional outflow | none | 0.0 % / 0.0 % | 6 → 3 | 1 · 894 · 0 | -40/138/768 / -164/212/2171 | 13/102/555 / 1/117/545 | 0 · 0 · 0 · 0 · 60 | — | 0, —, 0; 0 | — | — |
| (c) proportional outflow | aggressive | 0.0 % / 0.0 % | 7 → 3 | 4 · 894 · 0 | -118/98/681 / -135/154/1939 | 13/108/529 / 8/118/514 | 0 · 0 · 0 · 0 · 60 | — | 0, —, 0; 0 | — | 0, 17, 15, 67 |

- revived in a: dependency economic_output_to_treasury (Deficit Some(50.0)) — none 2 %, aggressive 2 %
- revived in a: dependency economic_output_to_population (DeficitProportional Some(50.0)) — none 2 %, aggressive 2 %
- revived in b: dependency economic_output_to_treasury (Deficit Some(50.0)) — none 6 %, aggressive 6 %
- revived in b: dependency economic_output_to_population (DeficitProportional Some(50.0)) — none 6 %, aggressive 6 %
- revived in b: rank condition veneto Greater 90 — none 98 %, aggressive 97 %
- revived in b: event gate trade_boom if self.economic_output Greater 40 — none 98 %, aggressive 97 %
- revived in c: dependency economic_output_to_treasury (Deficit Some(50.0)) — none 6 %, aggressive 6 %
- revived in c: dependency economic_output_to_population (DeficitProportional Some(50.0)) — none 6 %, aggressive 7 %
- revived in c: event gate trade_boom if self.economic_output Greater 40 — none 98 %, aggressive 98 %

