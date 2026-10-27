# tools/map — конвейер карты engine2

План — [`docs/engine2_map.md`](../../docs/engine2_map.md). Конвейер детерминирован: при тех же входах
и тех же версиях библиотек CSV-выход совпадает побайтово.

## Окружение

```bash
conda env create -f tools/map/environment.yml
```

Окружение называется `engine13-map`: Python 3.12 и conda-forge, версии закреплены в `environment.yml`.

## Шаги

```bash
conda run -n engine13-map python -I -X utf8 tools/map/fetch.py --cache D:/map-cache
conda run -n engine13-map python -I -X utf8 tools/map/base.py --cache D:/map-cache
conda run -n engine13-map python -I -X utf8 tools/map/preview.py --cache D:/map-cache
conda run -n engine13-map python -I -X utf8 -m pytest -p no:cacheprovider tools/map/tests
```

- `fetch.py` скачивает исходные данные (~0,7 ГБ) в `<cache>/src` и сверяет их с `sources.toml`.
- `base.py` собирает общую основу в `<cache>/out/base`:
  - `blocks.csv` — блоки: площадь, рельеф, климат, реки, хеш геометрии;
  - `borders.csv` — общие границы блоков, км;
  - `coasts.csv` — берег блоков по морским зонам, км;
  - `zones.csv` — морские зоны;
  - `blocks.gpkg` — геометрия блоков;
  - `report.txt` — замер.
- `preview.py` рисует превью: тип земли по блокам и прибрежные блоки по морским зонам.
- `density.py` подбирает плотности по типам земли вместе с классификацией по регионам
  с источниками (`density.toml`) и калибрует классы по потолкам земель (`calibrate`).
  Замер — `<cache>/out/density/report.txt`, план §11 и §11b.
- `layout.py <сценарий>` раскладывает провинции сценария по таблице `layouts/<сценарий>.toml`,
  план §12:
  - `public/geodata2/<сценарий>/provinces.geojson` — полигоны для интерфейса;
  - `public/geodata2/<сценарий>/provinces.toml` — провинции и регионы в полях схемы engine2;
  - замер и превью — `<cache>/out/<сценарий>/`.

  ```bash
  conda run -n engine13-map python -I -X utf8 tools/map/layout.py milan_1477 --cache D:/map-cache
  ```

## Файлы

- `params.toml` — параметры правил §4 плана; каждое правило записано там, в коде не подбирается.
- `rules.py` — правила §4 для провинций из блоков; в основе каждая провинция — сумма своих блоков.
- `geo.py` — длины на эллипсоиде, общие границы, берега, зональные суммы растров.
- `sources.toml` — источники с контрольными суммами.
- `ATTRIBUTION.md` — указание авторства данных.
