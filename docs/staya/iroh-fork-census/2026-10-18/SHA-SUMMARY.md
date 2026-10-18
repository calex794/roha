# СТАЯ — устранение повторов веток Iroh по SHA

Время: 2026-10-18T20:15:56.975266+00:00 UTC

| Показатель | Количество |
|---|---:|
| fork_rows | 938 |
| upstream_branches | 266 |
| upstream_unique_head_shas | 265 |
| non_default_ref_occurrences | 40193 |
| unique_head_shas | 2243 |
| upstream_head_sha_duplicates | 28346 |
| upstream_name_sha_duplicates | 28346 |
| non_upstream_ref_occurrences | 11847 |
| unique_non_upstream_head_shas | 1979 |
| forks_with_other_branches | 266 |
| branch_list_errors | 175 |
| truncated | 0 |
| forks_with_non_upstream_shas | 266 |

## Что эти цифры действительно означают

- Совпадение SHA c текущей веткой upstream точно означает ту же версию commit, а не новую независимую разработку.
- SHA, не совпадающий с текущей вершиной upstream, НЕ означает наличие новых функций. Возможно старая, удалённая upstream ветка или commit той же истории.
- Если SHA встречается во многих форках, достаточно одного подробного анализа соответствующего commit; но не выбрасывать саму ветку без проверки, откуда она произошла.
- Таблица branch_sha_priority.csv — очередь проверки, не автоматически подтверждённые улучшения.
- Полный перечень all_branch_heads.csv остаётся в отдельной ветке и позволяет восстановить источники.

## Первая двадцатка кандидатов по формальному отбору

| Форк | Ветка | Число повторений SHA |
|---|---|---:|
| [0xETHengineer/iroh](https://github.com/0xETHengineer/iroh) | api-refactor-node-addr | 1 |
| [0xETHengineer/iroh](https://github.com/0xETHengineer/iroh) | fix-inactive-peers | 1 |
| [0xETHengineer/iroh](https://github.com/0xETHengineer/iroh) | no-default-features-clean | 1 |
| [0xkelvin/iroh](https://github.com/0xkelvin/iroh) | feat/discovery-data | 1 |
| [0xkelvin/iroh](https://github.com/0xkelvin/iroh) | feat/discovery-user-data | 1 |
| [0xkelvin/iroh](https://github.com/0xkelvin/iroh) | feat/discovery-watch | 1 |
| [Abhijeet1815/iroh](https://github.com/Abhijeet1815/iroh) | fix-ipv6-unreachable-warnings | 1 |
| [BuddhiAbeyratneAL/iroh](https://github.com/BuddhiAbeyratneAL/iroh) | Frando/refactor-into-discovery | 1 |
| [BuddhiAbeyratneAL/iroh](https://github.com/BuddhiAbeyratneAL/iroh) | Frando/router-add-protocol | 1 |
| [BuddhiAbeyratneAL/iroh](https://github.com/BuddhiAbeyratneAL/iroh) | refactor-address-discovery | 1 |
| [BuddhiAbeyratneAL/iroh](https://github.com/BuddhiAbeyratneAL/iroh) | refactor-transports | 1 |
| [BunsDev/iroh-p2p](https://github.com/BunsDev/iroh-p2p) | feat-multipath | 1 |
| [Complear/iroh](https://github.com/Complear/iroh) | complear/v1.0.2-cnsa-quic | 1 |
| [Complear/iroh](https://github.com/Complear/iroh) | complear/v1.0.2-tls-aws-lc-rs | 1 |
| [Complear/iroh](https://github.com/Complear/iroh) | complear/v1.0.3-cnsa-quic | 1 |
| [CyberFlameGO/iroh](https://github.com/CyberFlameGO/iroh) | arqu/funky_dns | 1 |
| [CyberFlameGO/iroh](https://github.com/CyberFlameGO/iroh) | arqu/funky_dns_2 | 1 |
| [CyberFlameGO/iroh](https://github.com/CyberFlameGO/iroh) | feat-relay-server-ping | 1 |
| [CyberFlameGO/iroh](https://github.com/CyberFlameGO/iroh) | flub/deprecate-iroh-net | 1 |
| [CyberFlameGO/iroh](https://github.com/CyberFlameGO/iroh) | flub/discovery-arc | 1 |
