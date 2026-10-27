# Источники данных карты engine2 и указание авторства

Карта engine2 (`tools/map/`, данные — `public/geodata2/`) выведена из открытых данных. Данные под
CC BY и подобными лицензиями требуют указать авторство: оно указано здесь. Файл сопровождает
производные данные: при выкладке `public/geodata2/` он копируется туда же. Контрольные суммы
исходных файлов — `tools/map/sources.toml`.

| данные | автор, источник | лицензия |
|---|---|---|
| административные единицы (CGAZ ADM2, выпуск 2024-04-20) | Runfola D. et al. (2020). geoBoundaries: A global database of political administrative boundaries. *PLoS ONE* 15(4): e0231866. https://www.geoboundaries.org | CC BY 4.0 |
| провинции Италии (geoBoundaries ITA ADM3) | ISTAT — Istituto Nazionale di Statistica, через geoBoundaries | CC BY 3.0 |
| районы Германии (geoBoundaries DEU ADM3) | © GeoBasis-DE / BKG (2021), через geoBoundaries | Datenlizenz Deutschland – Namensnennung – Version 2.0 |
| общины Греции (geoBoundaries GRC ADM3) | Wikimedia Commons, через geoBoundaries | CC0 1.0 |
| морские зоны | Flanders Marine Institute (2018). IHO Sea Areas, version 3. https://www.marineregions.org, doi:10.14284/323 | CC BY 4.0 |
| климат | Beck H. E., Zimmermann N. E., McVicar T. R., Vergopolan N., Berg A., Wood E. F. (2018). Present and future Köppen-Geiger climate classification maps at 1-km resolution. *Scientific Data* 5: 180214, doi:10.1038/sdata.2018.214 | CC BY 4.0 |
| рельеф | NOAA National Centers for Environmental Information (2022). ETOPO 2022 Global Relief Model, doi:10.25921/fd45-gt74 | общественное достояние |
| суша, озёра, реки | Made with Natural Earth. https://www.naturalearthdata.com | общественное достояние |

Производные данные изменены: единицы обрезаны по береговой линии Natural Earth, озёра вычтены,
единицы объединены в исторические провинции по таблицам раскладки сценариев.
