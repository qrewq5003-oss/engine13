# Указатель по `docs/` — один вход в 53 документа

**Собрано и проверено 2026-09-15**, `main` = `b70eb86`. Здесь **нет содержания**: каждый
документ остаётся каноном по своему предмету, а этот файл отвечает на два вопроса —
**где что лежит** и **какие выводы перекрыты позднейшими**.

Проверено механически: все 53 файла существуют; каждая ссылка на документ из
`ENGINE13_INFRASTRUCTURE_TASKS.md` резолвится (21 из 21); вне себя ни разу не
упомянуты два файла — `narrative_review_pack_constantinople_1430.md` и
`narrative_review_pack_milan_1477.md` (их адресуют шаблоном `narrative_review_pack_*`,
это не сироты по смыслу).

---

## 1. Что чем перекрыто — читать до того, как опереться на число

| где | что перекрыто | чем |
|---|---|---|
| `investigation_pressure_military_form.md` §16.4, §17.3 | **«`mamluks` — контент, мёртвый в отгружаемой игре»** | **ОТОЗВАНО** в §19 и `investigation_dead_authored_content.md` §1: замер был только в мире **без игрока**, с игроком веха срабатывает 13/15 |
| `investigation_pressure_military_form.md` §14.4 | «все три спавна, население до нуля» | §15.1: до нуля падает у двух |
| `investigation_pressure_military_form.md` §15.1 | «аномалия у двух из трёх» | §16.1, затем `investigation_dead_authored_content.md` §10.3: по состоянию **на конец прогона** аномалия у всех трёх |
| `investigation_pressure_military_form.md` §17.2 | оговорка «разница несущественна» | §18: величина названа неверно, вывод держит **направление** записи, а не размер |
| `investigation_constantinople_cohesion_bonus.md` §«NB» | `federation_progress ≥ 100` | помечено в самом документе (2026-07-11): условие `≥ 80`, блокировал другой гейт |
| `investigation_external_pressure.md` (задача 2) | «путь распада через `external_pressure`» | переоткрыто и закрыто `(C)` в `investigation_pressure_ratchet.md` + `_milan` + `_rome` |
| `investigation_combat_asymmetry.md` | — | **дополнено, не отменено** `investigation_combat_self_destruction.md`: второй независимый дефект |
| `sim_baseline.md` | числа от 2026-03-09 | **исторический снимок**; актуальные ворота — в документах задач, не здесь |

## 2. Бой, смертность, наследование, границы

| документ | о чём | итог |
|---|---|---|
| `investigation_combat_asymmetry.md` | диагностика боевого дисбаланса коалиция/османы | три гипотезы опровергнуты, корень — асимметрия притока |
| `investigation_combat_self_destruction.md` | второй дефект: бой без условия завершения | 81–95 % боёв против пустой армии |
| `investigation_combat_loss_model.md` | форма боевых потерь | стадия 1, правок нет; ратифицированные ворота стоят на дефекте |
| `investigation_military_source.md` | источник для `military_size` | `k·pop^(2/3)`, ставка выведена из констант движка; **стадия 2 принята при невыполненном пункте критерия** |
| `investigation_pressure_military_form.md` | форма `ep → military_size` | **закрыта отказом**: пропорциональная форма убивает `conquest_collapse` целиком |
| `investigation_successor_entry.md` | вход наследника в мир | `split_metrics_for_successor` удалена, гард от воскрешения |
| `investigation_successor_edges.md` | граница наследника | рёбра родителя наследнику; корень — бессмертие через висячую ссылку |
| `investigation_collapse_order.md` | порядок гибелей одного тика | сортировка по id; детерминизм ловится на milan, не на сиде 42 |
| `investigation_besieged_reach.md` | `besieged` на `d ≤ 2` | `(C)`: не предлагать снова, настоящая починка — контентные рёбра |
| `investigation_d1_graph.md` | граф расстояния 1 | лимес Рима + османы–Сербия, 15 комбинаций измерены |
| `investigation_spawn_reverse_edges.md` | односторонние рёбра спавна | спавн дописывает обратное ребро |

## 3. Население, экономика, казна

| документ | о чём | итог |
|---|---|---|
| `investigation_eo_population_attractor.md` | `economic_output → population` | аттрактор; `deficit_proportional` вместо абсолютной дельты |
| `investigation_population_event_deltas.md` | абсолютные дельты населения в общем пуле | `(C)`: `flood` не решает исход 90/90 |
| `investigation_spawned_population.md` | население спавнов | структурная неплатёжеспособность с рождения |
| `investigation_treasury_budget.md` | бюджет без бюджетного ограничения | отказ; `population`/`economic_output` — сток без источника |
| `investigation_treasury_flow.md` | казна османов | адрес пункта постановки неверен |
| `investigation_event_cohesion_effects.md` | эффекты пула событий на `cohesion`/`eo` | `(D₂)`-0; глобальные метрики клампятся `0..100` |
| `investigation_migration_channel.md` | миграционный канал | перенос агрегирован по источнику; приход = уходу |

## 4. Давление, вассалитет, теги

| документ | о чём | итог |
|---|---|---|
| `investigation_external_pressure.md` | задача 2: затухание давления | опровергнуто как починка |
| `investigation_vassalage_conquest.md` | вассалитет и conquest на пересечении | полоса пуста, граф d1 почти пуст |
| `investigation_pressure_ratchet.md`, `investigation_pressure_ratchet_milan.md`, `investigation_pressure_ratchet_rome.md` | храповик давления, три захода | `(C)` во всех трёх; полоса `0 из 314 026` актор-тиков |
| `investigation_tag_channel.md` | заразные теги, пишущие `ep` | снятие канала снимает `0 из 254` |
| `investigation_knowledge_gates.md` | ворота знания | расследование без правки |

## 5. Спавны и авторский контент

| документ | о чём | итог |
|---|---|---|
| `investigation_spawn_completeness.md` | неполный `initial_metrics` трёх спавнов | `military_quality = 50`, выведено тремя путями |
| `investigation_dead_authored_content.md` | **перепись 93 гейтированных объектов по двум мирам** | шесть мертвы в обоих; `piracy` гибнет в щели двух словарей тегов; кульминацию const игрок не видит |
| `investigation_dead_authored_fields.md` | мёртвые авторские поля | гард; `tick_span` инертен |
| `investigation_world_features.md` | `features` не доходил до `WorldState` | три панели UI не отрисовывались никогда |
| `investigation_split_as_shrink.md` | раскол как усадка | Рим переживает свой перелом |
| `investigation_rome_splits_threshold.md` | калибровка вехи раскола | рычаг — длительность, а не порог |
| `investigation_triggers_collapse.md` | `triggers_collapse` не запускает `on_collapse` | честная веха вместо эмуляции |
| `investigation_early_transfer.md` | ранняя смена поколения rome | условие, которое никогда не ложно |
| `investigation_rome_arc.md` | что задачи 14/15 сделали с дугой Рима | дуга не изменилась |

## 6. Нарратив

| документ | о чём | итог |
|---|---|---|
| `narrative_state_2026_08.md` | оценка продукта целиком | обе оценочные инфраструктуры мерили заглушку |
| `investigation_consequence_context.md` | премиса и ложные альтернативы | премиса возвращена в режим Consequences |
| `investigation_third_relevance_path.md` | третий путь релевантности | не мёртвый код: он затирал события пустым списком |
| `investigation_update_memory.md` | память нарратива | удалена: непригодна к достижению, срез по байтам паникует на кириллице |
| `investigation_paragraph_contract.md` | объём абзацев | одно требование вместо четырёх противоречивых |
| `investigation_forbidden_claims.md` | авторский запрет на выдумку | доведён до модели |
| `investigation_event_log_order.md` | порядок журнала событий | детерминизм; два мира в одном процессе как инструмент |
| `investigation_player_action_attribution.md` | приписка действия игрока | объявленному игроку, а не первому по алфавиту |
| `narrative_evaluation.md`, `narrative_regression_baseline.md` | процедура ручной оценки и база регрессии | процедурные документы |
| `narrative_review_pack_*.md` (3) | **живые выходы модели** | готовый материал для проверки правил извлечения текста без обращения к LLM |

## 7. Ключи метрик, конвенции, базовые замеры

| документ | о чём | итог |
|---|---|---|
| `investigation_typed_metric_keys.md` | типизированные ключи | **§5.G и §5.H — источник классификации дефектов, на который ссылается вся остальная работа** |
| `investigation_metric_scoping.md` | шесть сайтов области видимости | `<`-гейты срабатывали всегда при `0.0` |
| `investigation_event_target_addressing.md` | адресация эффектов событий | гард на несовпадение гейта и эффекта |
| `investigation_sim_family_keys.md` | `SimStats` читает family-ключи сырыми | десятый сайт §5.G |
| `investigation_constantinople_cohesion_bonus.md` | коэффициент `5.0 → 0.1` | механизм объяснён данными; **часть чисел помечена устаревшими внутри документа** |
| `sim_baseline.md` | снимок баланса 2026-03-09 | **исторический**, актуальные ворота см. в задачных документах |

---

## 8. Как этим пользоваться

1. **Сначала §1.** Половина числовых выводов в этом корпусе уточнялась позднее, и
   почти всегда — не потому, что замер был неверен, а потому, что утверждение не
   называло мир, квантор, выдержку или направление.
2. **Документ задачи — канон по своему предмету.** Здесь только указатель; переносить
   выводы сюда значит завести второй источник правды, который разойдётся с первым, как
   разошлась очередь в `ENGINE13_INFRASTRUCTURE_TASKS.md`.
3. **Очередь — там, а не тут.** Открытые пункты живут в
   `ENGINE13_INFRASTRUCTURE_TASKS.md` §2 «Открытая очередь на сейчас».
