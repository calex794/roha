# СТАЯ — исключение исторических коммитов исходного Iroh

Дата: 2026-10-18T20:24:18.845836+00:00

| Критерий | Количество |
|---|---:|
| Кандидатов уникальных SHA до фильтра | 1979 |
| Старые коммиты основной истории upstream main | 22 |
| Коммиты в историях иных текущих upstream branches | 195 |
| Осталось SHA, недостижимых из актуальных upstream refs | 1762 |
| Коммитов в upstream main history | 2637 |
| Коммитов в upstream all refs history | 10658 |

**Ограничение:** недостижимость НЕ означает полезную или самостоятельную разработку. Это могут быть исторические удалённые ветки, коммиты только с документацией и прочее.
Выполнена полная загрузка графа upstream Git с фильтром blob:none, без shallow/depth. Подтверждённые совпадения исключены.

## Первые 40 оставшихся SHA по формальной сортировке

| Форк | Ветка | SHA | Повторов |
|---|---|---|---:|
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | refactor-discovery-node-addr | 12f0062abc79 | 43 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | Frando/fix-spans | aa1daaaf0a75 | 35 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | fix-docs-rs | b637fd9bf1fb | 28 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | ramfox/fix-arc-discovery | 57a322ef6bc8 | 28 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | remove-examples-feature | 6805980429bb | 28 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | Frando/mp-discovery | 59d0ed8c14c4 | 27 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | ramfox/mdns_relay_urls | 41356982b30c | 27 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | Frando/fix-doctest-bind-addr | 04c91b5ee1de | 25 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | feat-disable-portmapper | 8b536257b6a6 | 25 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | feat-rtt-hints | b926b912b68c | 25 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | feat-user-transport | 12956c5b2e26 | 25 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | relay-h3 | 64134cd50318 | 23 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | Frando/addr-filter-combinator | faad30f116ab | 15 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | fix/dns-reexports | dfd143b1da89 | 15 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | Frando/dns-timings | e2fd3dc48158 | 14 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | arqu/fix-windows-ci | eaaeecc70dca | 14 |
| [HeddleCo/iroh](https://github.com/HeddleCo/iroh) | refactor/n0-dns | 6e40c0b40926 | 11 |
| [agent-habilis/iroh](https://github.com/agent-habilis/iroh) | fix/mapped-addrs-eviction-1.0.2 | dcbdc1521caf | 2 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | Frando/fix-rsa-idle | 61c79b666ba9 | 2 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | Frando/relay-access-control | 059b5078c30a | 2 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | feat-rc-1 | b37c6edace5e | 2 |
| [HeddleCo/iroh](https://github.com/HeddleCo/iroh) | feat/custom-quic-time-source | 95e24512ecbf | 1 |
| [HeddleCo/iroh](https://github.com/HeddleCo/iroh) | feat/per-path-send-backpressure | b7bc6c34983e | 1 |
| [HeddleCo/iroh](https://github.com/HeddleCo/iroh) | fix/transport-send-backpressure | 12d9825e2e90 | 1 |
| [HeddleCo/iroh](https://github.com/HeddleCo/iroh) | integration/workerd-quic-runtime | 786977035980 | 1 |
| [agent-habilis/iroh](https://github.com/agent-habilis/iroh) | fix-open-path-queue-dedup | 0e7cdb4cd739 | 1 |
| [agent-habilis/iroh](https://github.com/agent-habilis/iroh) | fix/mapped-addrs-eviction | 67cd78c96a81 | 1 |
| [agent-habilis/iroh](https://github.com/agent-habilis/iroh) | fix/mapped-addrs-eviction-1.0.1 | 195cb98d1696 | 1 |
| [agent-habilis/iroh](https://github.com/agent-habilis/iroh) | fix/open-higher-custom-path | c2e81ee200e4 | 1 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | fix/churn-memory-leak | 82f27d944945 | 1 |
| [drlukeangel/iroh](https://github.com/drlukeangel/iroh) | local/fix-actor-deadlock | 07300509a064 | 1 |
| [fofoca-network/iroh](https://github.com/fofoca-network/iroh) | fix-misc | 8268850bbf05 | 1 |
| [fofoca-network/iroh](https://github.com/fofoca-network/iroh) | fix/join-all-panics-on-cancel | f9cb1f4fd1b6 | 1 |
| [fofoca-network/iroh](https://github.com/fofoca-network/iroh) | fix/netwatch-fork-pin | a1c166834f5c | 1 |
| [fofoca-network/iroh](https://github.com/fofoca-network/iroh) | fix/netwatch-fork-pin-1.0.3 | b7e07872a6d2 | 1 |
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | feat/distributed-multi-device-identity | 45840a382802 | 1 |
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | fix/hermetic-protocol-tests | 6f5db85e3547 | 1 |
| [holon-technologies/iroh](https://github.com/holon-technologies/iroh) | fix/simulation-service-evidence | 6a93e2393185 | 1 |
| [izihawa/iroh](https://github.com/izihawa/iroh) | feature/custom-directories | c6de83309bad | 1 |
| [izihawa/iroh](https://github.com/izihawa/iroh) | feature/eth-domains | 44dfbe4d238f | 1 |

Следующий этап — найти общий предок форка с upstream, сравнить собственные коммиты и patch-id; окончательно классифицировать изменения сетевого кода, а не только SHA.

Машинные файлы: history_filtered_shas.csv и potential_independent_commits.csv.
