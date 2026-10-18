# СТАЯ — сверка патчей оставшихся 1762 ветвей Iroh

Время расчёта: 2026-10-18T21:32:37.762811+00:00 UTC

## Покрытие

| Показатель | Значение |
|---|---:|
| Входных SHA ветвей | 1762 |
| audited | 1519 |
| large_diff | 15 |
| fetch_failed | 0 |
| commit_missing | 0 |
| no_common_ancestor | 228 |
| analysis_error | 0 |
| Отдельных SHA коммитов в подробной проверке | 7780 |
| Разных нормализованных патчей коммитов (непустых) | 7171 |
| Коммиты с совпадающим patch-id при другом SHA (сверх первого) | 600 |
| Разных итоговых патчей ветвей | 1464 |
| Повторные итоговые патчи при разных SHA (сверх первого) | 54 |
| Разных итоговых патчей сетевого/identity кода | 1023 |
| Повторные сетевые патчи при разных SHA (сверх первого) | 82 |

## Метод и ограничения

- Каждому SHA соответствует *вершина ветви*, а не обязательно единственный независимый коммит.
- Для каждой доступной ветви получен общий предок с основным Iroh и вычислен `git patch-id --stable` от *итоговой разницы деревьев*. Это выявляет одинаковые изменения под разными SHA.
- Индивидуальные patch-id коммитов собираются отдельно; подробная проверка ограничена 80 коммитами на голову. При превышении лимита `commit_detail_complete=false`.
- Группы по сетевому коду вычисляются без документации и тестов. Это *синтаксическое совпадение* — не доказательство эквивалентности поведения, пригодности или наличия тестов.
- Сложные/большие изменения и недоступные форки не исключать автоматически. Совпадение patch-id не гарантирует одинаковое поведение на разных версиях Iroh/noq.
- Файлы: patch_branch_audit.csv, patch_commit_index.csv, patch_network_clusters.csv.

## Приоритетные различия сетевого кода (файлы, не подтверждённые функции)

| Представитель | Ветка | Сетевых .rs | Совпадений |
|---|---|---:|---:|
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | feat/distributed-multi-device-identity | 119 | 1 |
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | chore/closure-pass | 117 | 5 |
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | chore/repo-showcase-readiness | 102 | 1 |
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | codex/v2-architecture-hard-cut | 99 | 1 |
| [divagant-martian/iroh](https://github.com/divagant-martian/iroh) | netmon-split | 77 | 1 |
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | fix/simulation-service-evidence | 65 | 5 |
| [divagant-martian/iroh](https://github.com/divagant-martian/iroh) | iroh-relay-intro | 65 | 1 |
| [pinkforest/iroh](https://github.com/pinkforest/iroh) | ramfox/relay-rename | 60 | 1 |
| [0xkelvin/iroh](https://github.com/0xkelvin/iroh) | matheus23/iroh-net-wasm | 57 | 1 |
| [BuddhiAbeyratneAL/iroh](https://github.com/BuddhiAbeyratneAL/iroh) | feat-relay-errors | 50 | 1 |
| [BunsDev/iroh-p2p](https://github.com/BunsDev/iroh-p2p) | deps-update | 48 | 1 |
| [emberian/iroh](https://github.com/emberian/iroh) | refactor-address-discovery | 46 | 1 |
| [KingYoSun/iroh](https://github.com/KingYoSun/iroh) | kukuri/connection-start-loss-v1.3.0 | 43 | 1 |
| [KingYoSun/iroh](https://github.com/KingYoSun/iroh) | kukuri/close-after-another-path-v1.3.0 | 43 | 1 |
| [KingYoSun/iroh](https://github.com/KingYoSun/iroh) | kukuri/close-withdrawn-custom-paths-v1.3.0 | 43 | 1 |
| [KingYoSun/iroh](https://github.com/KingYoSun/iroh) | kukuri/add-remote-addrs-v1.3.0 | 43 | 1 |
| [cladeworld/iroh](https://github.com/cladeworld/iroh) | matheus23/snafu-relay | 43 | 1 |
| [KingYoSun/iroh](https://github.com/KingYoSun/iroh) | kukuri/cleanup-mapped-addrs-v1.3.0 | 42 | 1 |
| [cclauss/iroh](https://github.com/cclauss/iroh) | refactor-futures | 42 | 1 |
| [divagant-martian/iroh](https://github.com/divagant-martian/iroh) | netmon-split-v2 | 42 | 1 |
| [link2xt/iroh](https://github.com/link2xt/iroh) | refactor-futures | 42 | 1 |
| [osbob53/iroh](https://github.com/osbob53/iroh) | refactor-futures | 42 | 1 |
| [alerque/iroh](https://github.com/alerque/iroh) | dig/n0-error | 41 | 1 |
| [alerque/iroh](https://github.com/alerque/iroh) | Frando/dns-server-actor-handle-message | 41 | 1 |
| [alerque/iroh](https://github.com/alerque/iroh) | Frando/n0-error | 41 | 1 |
| [luisschubert/iroh](https://github.com/luisschubert/iroh) | Frando/n0-error | 41 | 1 |
| [BuddhiAbeyratneAL/iroh](https://github.com/BuddhiAbeyratneAL/iroh) | refactor-address-discovery | 40 | 1 |
| [Septias/iroh](https://github.com/Septias/iroh) | refactor-futures | 39 | 1 |
| [0xkelvin/iroh](https://github.com/0xkelvin/iroh) | refactor-normalize-reexports | 38 | 1 |
| [bluestarburst/iroh](https://github.com/bluestarburst/iroh) | openrtc-1.2.0-path-reselection | 33 | 2 |
| [0xkelvin/iroh](https://github.com/0xkelvin/iroh) | feat-iroh-net-wasm | 32 | 1 |
| [GQAdonis/iroh](https://github.com/GQAdonis/iroh) | feat-no-derp-region | 31 | 1 |
| [ewindisch/iroh](https://github.com/ewindisch/iroh) | codex/iroh-102-provider | 30 | 1 |
| [lawrenceadams/iroh](https://github.com/lawrenceadams/iroh) | matheus23/iroh-browser | 30 | 1 |
| [0xkelvin/iroh](https://github.com/0xkelvin/iroh) | matheus23/iroh-browser | 29 | 1 |

Следующий шаг: ручная проверка top-35 сетевых изменений, регрессионные тесты Roha/noq и лицензионная проверка. Ничего не сливать автоматически.
