# Доля холмов h по трём картам — замер для E4 (план карты §12b, §17)

Раскладки не меняются: таблица — рядом с рабочими классами (flip, пашня 50). Сгенерировано
`tools/map/layout.py` (файл `<кэш>/out/<сценарий>/hills_share.md` каждого прогона) и собрано
сюда как есть. h = min(1, k·доля клеток изрезанных или сухих), k — так, чтобы земля держала потолок;
плотность провинции — A·(1 − h) + H·h.

### milan_1477

Пашня 50, холмы 12. Земли с потолком между холмами и пашней; ниже холмов, орошаемые, унаследованные и по правилу 1 — без h, их класс как сейчас.

| земля | потолок, чел./км² | провинций | k | ёмкость flip, млн | невязка flip | ёмкость при h, млн | невязка при h |
|---|---|---|---|---|---|---|---|
| tuscany | 47.9 | 5 | 0.095 | 1.02 | -2.1 % | 1.05 | +0.0 % |
| italy_sud | 36.9 | 11 | 0.487 | 3.49 | +4.4 % | 3.34 | +0.0 % |
| italy_isole | 23.9 | 8 | 1.072 | 1.29 | -8.5 % | 1.41 | +0.0 % |
| italy_cn | 50.0 | 37 | 0.000 | 6.81 | -9.1 % | 7.45 | -0.5 % |

| провинция | владелец | земля | класс flip | изрезано или сухо | h | чел./км² при h | ёмкость flip → при h, тыс. |
|---|---|---|---|---|---|---|---|
| abruzzo | naples | italy_sud | Hills | 0.77 | 0.38 | 35.7 | 130 → 386 |
| agrigento | sicily | italy_isole | Arable | 0.52 | 0.56 | 28.7 | 385 → 221 |
| alessandria | milan | italy_cn | Arable | 0.23 | 0.00 | 50.0 | 178 → 178 |
| aosta | savoy | italy_alps | Hills | 1.00 | 0.00 | 50.0 | 39 → 164 |
| arezzo | florence | tuscany | Arable | 0.68 | 0.06 | 47.5 | 162 → 154 |
| bari | naples | italy_sud | Arable | 0.47 | 0.23 | 41.3 | 271 → 224 |
| basilicata | naples | italy_sud | Arable | 0.87 | 0.43 | 33.8 | 500 → 338 |
| belluno | venice | italy_alps | Hills | 0.99 | 0.00 | 50.0 | 43 → 180 |
| bergamo | venice | italy_cn | Arable | 0.72 | 0.00 | 50.0 | 138 → 138 |
| bologna | bologna | italy_cn | Arable | 0.35 | 0.00 | 50.0 | 185 → 185 |
| brescia | venice | italy_cn | Arable | 0.62 | 0.00 | 50.0 | 230 → 230 |
| cagliari | sicily | italy_isole | Hills | 0.56 | 0.60 | 27.1 | 93 → 210 |
| calabria_citra | naples | italy_sud | Hills | 0.85 | 0.41 | 34.3 | 80 → 229 |
| calabria_ultra | naples | italy_sud | Arable | 0.74 | 0.36 | 36.3 | 421 → 305 |
| campagna | papacy | italy_sud | Arable | 0.68 | 0.33 | 37.4 | 274 → 205 |
| capitanata | naples | italy_sud | Arable | 0.97 | 0.47 | 32.1 | 572 → 367 |
| catania | sicily | italy_isole | Arable | 0.49 | 0.53 | 29.8 | 366 → 218 |
| como | milan | italy_cn | Arable | 0.64 | 0.00 | 50.0 | 160 → 160 |
| corsica | genoa | italy_isole | Hills | 0.89 | 0.95 | 13.7 | 104 → 119 |
| cremona | milan | italy_cn | Arable | 0.00 | 0.00 | 50.0 | 89 → 89 |
| cuneo | savoy | italy_cn | Arable | 0.61 | 0.00 | 50.0 | 345 → 345 |
| ferrara | ferrara | italy_cn | Arable | 0.00 | 0.00 | 50.0 | 221 → 221 |
| firenze | florence | tuscany | Arable | 0.66 | 0.06 | 47.6 | 242 → 231 |
| friuli | venice | italy_cn | Arable | 0.59 | 0.00 | 50.0 | 357 → 357 |
| liguria | genoa | italy_cn | Arable | 0.94 | 0.00 | 50.0 | 325 → 325 |
| lucca | lucca | tuscany | Hills | 0.86 | 0.08 | 46.9 | 21 → 82 |
| malta | sicily | italy_isole | Arable | 0.00 | 0.00 | 50.0 | 16 → 16 |
| mantova | mantua | italy_cn | Arable | 0.00 | 0.00 | 50.0 | 117 → 117 |
| marca | papacy | italy_cn | Arable | 0.44 | 0.00 | 50.0 | 343 → 343 |
| messina | sicily | italy_isole | Hills | 0.97 | 1.00 | 12.0 | 40 → 40 |
| milano | milan | italy_cn | Arable | 0.00 | 0.00 | 50.0 | 138 → 138 |
| modena | ferrara | italy_cn | Arable | 0.44 | 0.00 | 50.0 | 249 → 249 |
| monferrato | montferrat | italy_cn | Arable | 0.07 | 0.00 | 50.0 | 76 → 76 |
| napoli | naples | italy_sud | Arable | 0.52 | 0.25 | 40.3 | 190 → 153 |
| novara | milan | italy_cn | Arable | 0.14 | 0.00 | 50.0 | 67 → 67 |
| otranto | naples | italy_sud | Arable | 0.34 | 0.17 | 43.6 | 353 → 308 |
| padova | venice | italy_cn | Arable | 0.03 | 0.00 | 50.0 | 107 → 107 |
| palermo | sicily | italy_isole | Hills | 0.69 | 0.74 | 21.7 | 90 → 164 |
| parma | milan | italy_cn | Arable | 0.56 | 0.00 | 50.0 | 302 → 302 |
| pavia | milan | italy_cn | Arable | 0.14 | 0.00 | 50.0 | 149 → 149 |
| pisa | florence | tuscany | Arable | 0.35 | 0.03 | 48.7 | 183 → 179 |
| principato | naples | italy_sud | Hills | 0.84 | 0.41 | 34.5 | 117 → 338 |
| ravenna | venice | italy_cn | Arable | 0.09 | 0.00 | 50.0 | 93 → 93 |
| roma | papacy | italy_sud | Arable | 0.43 | 0.21 | 42.0 | 580 → 487 |
| romagna | papacy | italy_cn | Arable | 0.51 | 0.00 | 50.0 | 169 → 169 |
| sassari | sicily | italy_isole | Hills | 0.59 | 0.64 | 25.8 | 196 → 422 |
| siena | siena | tuscany | Arable | 0.51 | 0.05 | 48.1 | 416 → 400 |
| torino | savoy | italy_cn | Arable | 0.64 | 0.00 | 50.0 | 341 → 341 |
| trento | austria | italy_alps | Hills | 1.00 | 0.00 | 50.0 | 74 → 310 |
| umbria | papacy | italy_cn | Arable | 0.61 | 0.00 | 50.0 | 423 → 423 |
| urbino | urbino | italy_cn | Arable | 0.45 | 0.00 | 50.0 | 126 → 126 |
| valtellina | milan | italy_alps | Hills | 1.00 | 0.00 | 50.0 | 39 → 162 |
| venezia | venice | italy_cn | Arable | 0.11 | 0.00 | 50.0 | 229 → 229 |
| vercelli | savoy | italy_cn | Arable | 0.44 | 0.00 | 50.0 | 150 → 150 |
| verona | venice | italy_cn | Arable | 0.35 | 0.00 | 50.0 | 147 → 147 |
| vicenza | venice | italy_cn | Arable | 0.58 | 0.00 | 50.0 | 136 → 136 |

### constantinople_1430

Пашня 50, холмы 12. Земли с потолком между холмами и пашней; ниже холмов, орошаемые, унаследованные и по правилу 1 — без h, их класс как сейчас.

| земля | потолок, чел./км² | провинций | k | ёмкость flip, млн | невязка flip | ёмкость при h, млн | невязка при h |
|---|---|---|---|---|---|---|---|
| balkans_1340 | 13.3 | 26 | 2.551 | 5.54 | -7.7 % | 6.00 | +0.0 % |
| asia_minor_165 | 14.5 | 11 | 1.207 | 8.36 | +4.9 % | 7.97 | +0.0 % |
| syria_165 | 39.5 | 2 | 0.391 | 7.66 | +21.0 % | 6.33 | -0.0 % |

| провинция | владелец | земля | класс flip | изрезано или сухо | h | чел./км² при h | ёмкость flip → при h, тыс. |
|---|---|---|---|---|---|---|---|
| albania | ottomans | balkans_1340 | Hills | 0.82 | 1.00 | 12.0 | 301 → 301 |
| albania_veneta | venice | balkans_1340 | Hills | 0.71 | 1.00 | 12.0 | 46 → 46 |
| amasya | ottomans | asia_minor_165 | Hills | 0.89 | 1.00 | 12.0 | 801 → 801 |
| ankara | ottomans | asia_minor_165 | Arable | 0.96 | 1.00 | 12.0 | 2,320 → 557 |
| arcipelago | venice | balkans_1340 | Hills | 0.67 | 1.00 | 12.0 | 29 → 29 |
| aydin | ottomans | asia_minor_165 | Hills | 0.71 | 0.86 | 17.2 | 767 → 1,102 |
| bitinia | ottomans | asia_minor_165 | Hills | 0.70 | 0.85 | 17.8 | 771 → 1,143 |
| bosnia | bosnia | balkans_1340 | Hills | 0.70 | 1.00 | 12.0 | 581 → 581 |
| bulgaria_n | ottomans | balkans_1340 | Hills | 0.47 | 1.00 | 12.0 | 792 → 792 |
| bulgaria_s | ottomans | balkans_1340 | Hills | 0.52 | 1.00 | 12.0 | 681 → 681 |
| candar | candar | asia_minor_165 | Hills | 0.89 | 1.00 | 12.0 | 419 → 419 |
| cappadocia | karaman | asia_minor_165 | Hills | 0.95 | 1.00 | 12.0 | 490 → 490 |
| chio | genoa | balkans_1340 | Hills | 0.74 | 1.00 | 12.0 | 11 → 11 |
| cilicia | mamluks | asia_minor_165 | Hills | 0.68 | 0.82 | 19.0 | 1,109 → 1,754 |
| cipro | cyprus | syria_165 | Hills | 0.66 | 0.26 | 40.2 | 111 → 373 |
| corfu | venice | balkans_1340 | Hills | 0.55 | 1.00 | 12.0 | 11 → 11 |
| costantinopoli | byzantium | balkans_1340 | Arable | 0.02 | 0.04 | 48.4 | 165 → 160 |
| creta | venice | balkans_1340 | Hills | 0.87 | 1.00 | 12.0 | 100 → 100 |
| epiro | ottomans | balkans_1340 | Hills | 0.85 | 1.00 | 12.0 | 156 → 156 |
| germiyan | ottomans | asia_minor_165 | Hills | 0.81 | 0.98 | 12.8 | 338 → 360 |
| konya | karaman | asia_minor_165 | Hills | 0.88 | 1.00 | 12.0 | 624 → 624 |
| lemno | byzantium | balkans_1340 | Hills | 0.30 | 0.77 | 20.8 | 10 → 17 |
| lepanto | venice | balkans_1340 | Hills | 0.99 | 1.00 | 12.0 | 10 → 10 |
| lesbo | genoa | balkans_1340 | Hills | 0.70 | 1.00 | 12.0 | 20 → 20 |
| macedonia | ottomans | balkans_1340 | Hills | 0.93 | 1.00 | 12.0 | 700 → 700 |
| mesembria | byzantium | balkans_1340 | Hills | 0.60 | 1.00 | 12.0 | 10 → 10 |
| modone | venice | balkans_1340 | Hills | 0.60 | 1.00 | 12.0 | 7 → 7 |
| morea | byzantium | balkans_1340 | Hills | 0.81 | 1.00 | 12.0 | 239 → 239 |
| nauplia | venice | balkans_1340 | Hills | 0.87 | 1.00 | 12.0 | 16 → 16 |
| negroponte | venice | balkans_1340 | Hills | 0.79 | 1.00 | 12.0 | 47 → 47 |
| pera | genoa | balkans_1340 | Arable | 0.00 | 0.00 | 50.0 | 11 → 11 |
| rodi | hospitallers | balkans_1340 | Hills | 0.55 | 1.00 | 12.0 | 31 → 31 |
| serbia | serbia | balkans_1340 | Hills | 0.58 | 1.00 | 12.0 | 747 → 747 |
| siria | mamluks | syria_165 | Arable | 0.71 | 0.28 | 39.5 | 7,546 → 5,955 |
| teke | ottomans | asia_minor_165 | Hills | 0.85 | 1.00 | 12.0 | 522 → 522 |
| tessaglia | ottomans | balkans_1340 | Hills | 0.82 | 1.00 | 12.0 | 315 → 315 |
| tracia | ottomans | balkans_1340 | Hills | 0.23 | 0.58 | 27.8 | 347 → 805 |
| trebisonda | trebizond | asia_minor_165 | Hills | 0.99 | 1.00 | 12.0 | 198 → 198 |
| zeta | serbia | balkans_1340 | Hills | 0.91 | 1.00 | 12.0 | 155 → 155 |

### rome_375

Пашня 50, холмы 12. Земли с потолком между холмами и пашней; ниже холмов, орошаемые, унаследованные и по правилу 1 — без h, их класс как сейчас.

| земля | потолок, чел./км² | провинций | k | ёмкость flip, млн | невязка flip | ёмкость при h, млн | невязка при h |
|---|---|---|---|---|---|---|---|
| tuscany | 47.9 | 1 | 0.103 | 2.01 | +4.3 % | 1.93 | +0.0 % |
| italy_sud | 36.9 | 4 | 0.458 | 3.40 | +12.7 % | 3.01 | +0.0 % |
| italy_isole | 23.9 | 2 | 1.069 | 1.70 | +20.7 % | 1.41 | +0.0 % |
| italy_cn | 50.0 | 4 | 0.000 | 8.79 | +0.1 % | 8.71 | -0.9 % |
| gaul_france | 30.8 | 8 | 7.128 | 19.45 | +6.6 % | 18.24 | +0.0 % |
| iberia | 15.3 | 5 | 2.100 | 7.05 | -21.6 % | 9.00 | +0.0 % |
| greece_165 | 17.5 | 5 | 1.006 | 1.87 | -31.4 % | 2.73 | -0.0 % |
| asia_minor_165 | 14.5 | 12 | 1.160 | 7.64 | -4.8 % | 8.03 | +0.0 % |
| syria_roman | 42.4 | 5 | 0.272 | 5.04 | -8.4 % | 5.50 | +0.0 % |
| n_africa_165 | 18.0 | 6 | 0.682 | 9.93 | +12.0 % | 8.87 | +0.0 % |
| germany | 33.6 | 6 | 14.064 | 9.80 | -2.3 % | 10.03 | +0.0 % |
| sawad | 26.7 | 2 | 0.615 | 6.36 | +9.7 % | 5.80 | +0.0 % |

| провинция | владелец | земля | класс flip | изрезано или сухо | h | чел./км² при h | ёмкость flip → при h, тыс. |
|---|---|---|---|---|---|---|---|
| achaea | rome | greece_165 | Hills | 0.78 | 0.79 | 20.0 | 456 → 760 |
| aemilia | rome | italy_cn | Arable | 0.39 | 0.00 | 50.0 | 1,469 → 1,469 |
| africa_proconsularis | rome | n_africa_165 | Arable | 0.63 | 0.43 | 33.6 | 1,761 → 1,184 |
| alamannia | alamanni | germany | Hills | 0.11 | 1.00 | 12.0 | 585 → 585 |
| apulia | rome | italy_sud | Arable | 0.61 | 0.28 | 39.4 | 973 → 766 |
| aquitania | rome | gaul_france | Hills | 0.10 | 0.71 | 23.1 | 1,655 → 3,184 |
| arabia | rome | syria_roman | Arable | 0.93 | 0.25 | 40.4 | 1,735 → 1,402 |
| armenia_minor | rome | asia_minor_165 | Hills | 0.95 | 1.00 | 12.0 | 326 → 326 |
| armorica | rome | gaul_france | Arable | 0.00 | 0.00 | 50.0 | 4,782 → 4,782 |
| asia | rome | asia_minor_165 | Hills | 0.64 | 0.74 | 21.7 | 831 → 1,502 |
| asorestan | sassanids | sawad | Hills | 0.99 | 0.61 | 26.8 | 1,419 → 3,166 |
| baetica | rome | iberia | Hills | 0.45 | 0.95 | 13.8 | 946 → 1,085 |
| belgica | rome | gaul_france | Arable | 0.02 | 0.11 | 45.7 | 5,531 → 5,060 |
| bithynia | rome | asia_minor_165 | Hills | 0.76 | 0.88 | 16.6 | 501 → 692 |
| bruttii | rome | italy_sud | Arable | 0.82 | 0.38 | 35.7 | 1,254 → 895 |
| burgundia | burgundians | germany | Arable | 0.04 | 0.51 | 30.8 | 1,120 → 690 |
| byzacena | rome | n_africa_165 | Desert | 1.00 | — | 0.5 | 29 → 29 |
| campania | rome | italy_sud | Arable | 0.73 | 0.33 | 37.3 | 954 → 711 |
| cappadocia | rome | asia_minor_165 | Hills | 0.89 | 1.00 | 12.0 | 1,009 → 1,009 |
| caria_lycia | rome | asia_minor_165 | Hills | 0.87 | 1.00 | 12.0 | 374 → 374 |
| carthaginiensis | rome | iberia | Hills | 0.92 | 1.00 | 12.0 | 1,730 → 1,730 |
| cilicia | rome | asia_minor_165 | Hills | 0.62 | 0.72 | 22.5 | 341 → 640 |
| creta | rome | greece_165 | Hills | 0.87 | 0.87 | 16.9 | 100 → 140 |
| cyprus | rome | syria_roman | Hills | 0.66 | 0.18 | 43.2 | 111 → 401 |
| epirus | rome | greece_165 | Hills | 0.80 | 0.81 | 19.3 | 450 → 724 |
| francia | franks | germany | Arable | 0.00 | 0.04 | 48.4 | 2,263 → 2,191 |
| galatia | rome | asia_minor_165 | Hills | 0.98 | 1.00 | 12.0 | 483 → 483 |
| gallaecia | rome | iberia | Hills | 0.63 | 1.00 | 12.0 | 1,125 → 1,125 |
| germania | rome | gaul_france | Arable | 0.07 | 0.48 | 31.6 | 1,786 → 1,129 |
| langobardia | lombards | germany | Arable | 0.00 | 0.00 | 50.0 | 1,362 → 1,362 |
| liguria | rome | italy_cn | Arable | 0.58 | 0.00 | 50.0 | 2,414 → 2,414 |
| lugdunensis | rome | gaul_france | Arable | 0.07 | 0.51 | 30.6 | 4,135 → 2,529 |
| lusitania | rome | iberia | Hills | 0.30 | 0.64 | 25.7 | 1,577 → 3,382 |
| lycaonia | rome | asia_minor_165 | Arable | 0.92 | 1.00 | 12.0 | 1,315 → 316 |
| macedonia | rome | greece_165 | Hills | 0.92 | 0.93 | 14.7 | 637 → 779 |
| mauretania | rome | n_africa_165 | Arable | 0.83 | 0.57 | 28.5 | 6,467 → 3,686 |
| mesene | sassanids | sawad | Arable | 1.00 | 0.61 | 26.6 | 4,943 → 2,634 |
| narbonensis | rome | gaul_france | Hills | 0.40 | 1.00 | 12.0 | 407 → 407 |
| novempopulana | rome | gaul_france | Hills | 0.17 | 1.00 | 12.0 | 399 → 399 |
| numidia | rome | n_africa_165 | Hills | 0.89 | 0.61 | 26.9 | 739 → 1,655 |
| palaestina | rome | syria_roman | Arable | 0.39 | 0.11 | 46.0 | 375 → 345 |
| pamphylia | rome | asia_minor_165 | Hills | 0.80 | 0.93 | 14.5 | 431 → 522 |
| paphlagonia | rome | asia_minor_165 | Hills | 0.85 | 0.99 | 12.4 | 255 → 263 |
| phoenicia | rome | syria_roman | Hills | 0.82 | 0.22 | 41.6 | 345 → 1,195 |
| phrygia | rome | asia_minor_165 | Hills | 0.82 | 0.95 | 14.0 | 724 → 847 |
| pontus | rome | asia_minor_165 | Hills | 0.93 | 1.00 | 12.0 | 1,052 → 1,052 |
| samnium | rome | italy_sud | Hills | 0.83 | 0.38 | 35.6 | 216 → 640 |
| sardinia | rome | italy_isole | Hills | 0.66 | 0.71 | 23.0 | 393 → 754 |
| saxonia | saxons | germany | Arable | 0.00 | 0.00 | 50.0 | 3,436 → 3,436 |
| sicilia | rome | italy_isole | Arable | 0.61 | 0.66 | 25.1 | 1,308 → 656 |
| syria | rome | syria_roman | Arable | 0.62 | 0.17 | 43.6 | 2,472 → 2,157 |
| tarraconensis | rome | iberia | Hills | 0.76 | 1.00 | 12.0 | 1,677 → 1,677 |
| thessalia | rome | greece_165 | Hills | 0.86 | 0.87 | 17.1 | 231 → 328 |
| thuringia | thuringians | germany | Hills | 0.06 | 0.78 | 20.5 | 1,035 → 1,770 |
| tingitana | rome | n_africa_165 | Hills | 0.73 | 0.50 | 31.2 | 864 → 2,243 |
| tripolitania | rome | n_africa_165 | Desert | 1.00 | — | 0.5 | 68 → 68 |
| tuscia | rome | tuscany | Arable | 0.53 | 0.05 | 47.9 | 2,013 → 1,930 |
| venetia | rome | italy_cn | Arable | 0.56 | 0.00 | 50.0 | 2,895 → 2,895 |
| viennensis | rome | gaul_france | Hills | 0.71 | 1.00 | 12.0 | 751 → 751 |

