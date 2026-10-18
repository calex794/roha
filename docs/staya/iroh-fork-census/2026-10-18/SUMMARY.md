# СТАЯ — первичный отсев форков Iroh

**Дата снимка:** 2026-10-18
**Метод:** официальный список GitHub REST /repos/n0-computer/iroh/forks,
сравнение default-ветки через REST compare с исходным main.

- Счётчик GitHub: **776**.
- Реально перечислено уникальных репозиториев: **938** на 10 страницах.
- Разница со счётчиком: **-162** (счётчик и официальный список могут иметь разную семантику или обновляться не одновременно).
- Ошибок/неопределённостей сравнения: **212**.
- **Важно:** дополнительные ветки ещё не проверены. Зеркало на
  default-ветке НЕ означает, что весь форк бесполезен.

## Первичная классификация

| Категория | Количество | Действие |
|---|---:|---|
| Свои изменения сетевого кода | 15 | Приоритет на изучение |
| Дополнительные тесты/примеры | 0 | Изучить сценарии |
| Только прочие файлы | 12 | Низкий приоритет |
| Отставшая основная ветка | 689 | Отложить main; осмотреть ветки |
| Основная ветка идентична | 10 | Отложить main; осмотреть ветки |
| Не удалось сравнить | 212 | Повторить |

## Основные ветки с сетевыми изменениями

| Форк | Впереди upstream | Производственных файлов | pushed |
|---|---:|---:|---|
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | 1379 | 16 | 2026-08-10 |
| [manaflow-ai/iroh](https://github.com/manaflow-ai/iroh) | 51 | 13 | 2026-10-05 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | 23 | 11 | 2026-10-18 |
| [agent-habilis/iroh](https://github.com/agent-habilis/iroh) | 13 | 10 | 2026-10-17 |
| [dvc94ch/iroh](https://github.com/dvc94ch/iroh) | 11 | 9 | 2024-07-03 |
| [fofoca-network/iroh](https://github.com/fofoca-network/iroh) | 2 | 8 | 2026-09-25 |
| [bxff/iroh](https://github.com/bxff/iroh) | 3 | 7 | 2026-07-22 |
| [HeddleCo/iroh](https://github.com/HeddleCo/iroh) | 3 | 7 | 2026-09-04 |
| [haydenflinner/iroh](https://github.com/haydenflinner/iroh) | 3 | 6 | 2026-10-16 |
| [izihawa/iroh](https://github.com/izihawa/iroh) | 1 | 4 | 2024-10-18 |
| [3exp8/iroh](https://github.com/3exp8/iroh) | 10 | 3 | 2024-03-27 |
| [HeavenVolkoff/iroh](https://github.com/HeavenVolkoff/iroh) | 3 | 3 | 2024-10-29 |
| [dayanruben/iroh](https://github.com/dayanruben/iroh) | 30 | 1 | 2026-10-17 |
| [julianpryde/iroh](https://github.com/julianpryde/iroh) | 5 | 1 | 2026-09-17 |
| [NoelJacob/iroh](https://github.com/NoelJacob/iroh) | 2 | 1 | 2026-10-07 |

## Границы доказательств

1. Данные all_forks.csv охватывают только официально перечисленные
   доступные репозитории. Сравнения используются для первой сортировки.
2. Отдельные ветки fix/*, feat/*, потомков форков ещё требуют прохода.
3. Патчи более 300 файлов могут быть обрезаны GitHub compare.
4. Любое заимствование — только после чтения diff, сравнения с
   нашим Roha, независимого теста и проверки лицензии.

Машинный перечень: all_forks.csv и all_forks.json; основной отбор:
network_code_candidates.csv.
