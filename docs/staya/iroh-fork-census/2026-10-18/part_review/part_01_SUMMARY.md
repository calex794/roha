# STAYA, Iroh history part 01

Source: frozen history_filtered_shas.csv, 2026-10-18
Reviewed UTC: 2026-10-19T00:35:52.618259+00:00
Range: 1-104 / 1762

## Evidence labels (NOT final implementation decisions)

| Label | Heads |
|---|---:|
| large_or_truncated_requires_review | 3 |
| legacy_architecture_manual_mapping | 25 |
| non_source_diff | 6 |
| separate_historical_lineage | 16 |
| source_changes_require_roha_comparison | 54 |

## Incomplete histories

- [45840a382802](https://github.com/holon-technologies/iroh/commit/45840a382802a32a77b2b50c1f55f14d5515dbe7) holon-technologies/iroh:feat/distributed-multi-device-identity, 500 of 1378 unique commits
- [6f5db85e3547](https://github.com/holon-technologies/iroh/commit/6f5db85e3547ef36351d3735f8654d2278672ca9) holon-technologies/iroh:fix/hermetic-protocol-tests, 500 of 1341 unique commits
- [c6de83309bad](https://github.com/izihawa/iroh/commit/c6de83309bad719ef5c169bc88b782e05f4fc6c4) izihawa/iroh:feature/custom-directories, 0 of None unique commits
- [44dfbe4d238f](https://github.com/izihawa/iroh/commit/44dfbe4d238f67eb3a16ebb0bf99e592539de33b) izihawa/iroh:feature/eth-domains, 0 of None unique commits
- [70476f71f98d](https://github.com/izihawa/iroh/commit/70476f71f98dde5337c3531a3ea0dc9aa33b5ff8) izihawa/iroh:feature/head-and-recatoring, 0 of None unique commits
- [68e5b3aa66c5](https://github.com/izihawa/iroh/commit/68e5b3aa66c59210b0e4096e0665994b1ffeb1a3) izihawa/iroh:feature/header-and-queue, 0 of None unique commits
- [80dc47f8d18a](https://github.com/izihawa/iroh/commit/80dc47f8d18ad8364b5d1000be333b4489183a60) izihawa/iroh:feature/ipns, 0 of None unique commits
- [c8d2c51850ad](https://github.com/izihawa/iroh/commit/c8d2c51850ad27363a0d9b6da6fa117f481f27af) izihawa/iroh:feature/kanal, 0 of None unique commits
- [27e27cafa070](https://github.com/izihawa/iroh/commit/27e27cafa070cebdd75da7799d9d2e0fa41dec2e) izihawa/iroh:feature/peers-priority, 0 of None unique commits
- [6301cf9dd825](https://github.com/izihawa/iroh/commit/6301cf9dd8254b4874b6551a130340975c212c88) izihawa/iroh:feature/seqcast, 0 of None unique commits
- [0a5e137b1c09](https://github.com/izihawa/iroh/commit/0a5e137b1c09d845cb707474aced7ca9056a4ffb) izihawa/iroh:feature/small-fixes, 0 of None unique commits
- [dbcb2065e8a6](https://github.com/izihawa/iroh/commit/dbcb2065e8a63b75977fd4ef9d7c4fb99346c019) izihawa/iroh:feature/subdomain-support, 0 of None unique commits
- [4b446be99272](https://github.com/izihawa/iroh/commit/4b446be99272d82cfc907ea3a35517c6cc7ecd84) izihawa/iroh:feature/summa, 0 of None unique commits
- [851820bc8a04](https://github.com/izihawa/iroh/commit/851820bc8a04bd3a97688a40945ba427bbd321aa) izihawa/iroh:feature/unixfs, 0 of None unique commits
- [b20ecbf4419f](https://github.com/izihawa/iroh/commit/b20ecbf4419f24995b27d36fa4fcad706fa681b5) izihawa/iroh:fix/bs-limits, 0 of None unique commits
- [6fb533e6d31a](https://github.com/izihawa/iroh/commit/6fb533e6d31a784a139d595a2910e29cbc4e78d6) izihawa/iroh:fix/gateway-path-and-css, 0 of None unique commits
- [47c2fb557731](https://github.com/izihawa/iroh/commit/47c2fb5577316fa1d975ea6a25dd18e8e804af48) izihawa/iroh:fix/incorrect-tsize, 0 of None unique commits
- [f0cb903186c3](https://github.com/izihawa/iroh/commit/f0cb903186c33cc8899f5f1278b953c00a353f93) izihawa/iroh:fix/refactorings, 0 of None unique commits

## Boundaries
- Every SHA is a branch head, not necessarily a separate fix.
- A branch comparison to upstream is not a diff against calex794/roha.
- No entry has been marked merged, tested, or released by this script.
- Without a common ancestor or with truncated files, manual investigation remains.
