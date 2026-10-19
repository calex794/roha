# STAYA, Iroh history part 07

Source: frozen history_filtered_shas.csv, 2026-10-18
Reviewed UTC: 2026-10-19T01:12:47.356343+00:00
Range: 625-728 / 1762

## Evidence labels (NOT final implementation decisions)

| Label | Heads |
|---|---:|
| large_or_truncated_requires_review | 3 |
| legacy_architecture_manual_mapping | 2 |
| non_source_diff | 14 |
| separate_historical_lineage | 20 |
| source_changes_require_roha_comparison | 65 |

## Incomplete histories

- [848e2ffe6e2d](https://github.com/charroch/iroh/commit/848e2ffe6e2dc88f38b33947ee713bd4988db5cf) charroch/iroh:fix-bitswap-experiments, 0 of None unique commits
- [d6cb9be3e365](https://github.com/fabricedesre/iroh/commit/d6cb9be3e365fcb25b788302f18ceac5399053e2) fabricedesre/iroh:feat-recursive-resolve, 0 of None unique commits
- [541ff889bc18](https://github.com/charroch/iroh/commit/541ff889bc182b54300e9f88bb64f8649133618d) charroch/iroh:fix-improve-bs-handler, 0 of None unique commits
- [91c04401c27e](https://github.com/Frando/iroh/commit/91c04401c27ea593cd7cdd24a44def2186f7ce0d) Frando/iroh:resolver-map-path, 0 of None unique commits
- [7ec0f9ad1c22](https://github.com/OnsagerHe/iroh/commit/7ec0f9ad1c227104b6feddf492b396f1253095d0) OnsagerHe/iroh:fix-documentation-iroh-ctl, 0 of None unique commits
- [47ebcec25ebe](https://github.com/charroch/iroh/commit/47ebcec25ebe423d2bb9038c13c8f76e561a9ac6) charroch/iroh:feat-memesync, 0 of None unique commits
- [105681046fe5](https://github.com/charroch/iroh/commit/105681046fe59b25140e18f83b4af76a87dabe4f) charroch/iroh:feat-redb, 0 of None unique commits
- [95ddef046d6a](https://github.com/faassen/iroh/commit/95ddef046d6af13cc60ae2e8f24cc4fdbbca690e) faassen/iroh:test_make_dir_from_path_deterministic_comment, 0 of None unique commits
- [3d1c1c71c4b3](https://github.com/huitseeker/iroh/commit/3d1c1c71c4b3ed7a47eff4923e74228eb2a2096a) huitseeker/iroh:feat-bitswap-client, 0 of None unique commits
- [eebab4dbcb8e](https://github.com/huitseeker/iroh/commit/eebab4dbcb8e755f5a40f6be10385344c70817da) huitseeker/iroh:feat-indexer-resolution, 0 of None unique commits
- [5f5a6ef65101](https://github.com/huitseeker/iroh/commit/5f5a6ef651010e60cef9fafdefd0d0f04361c765) huitseeker/iroh:feat-tarpc, 0 of None unique commits
- [1dfc0de39715](https://github.com/huitseeker/iroh/commit/1dfc0de3971503eb41921d78c0869ab7891cd0bc) huitseeker/iroh:fix-cleanpu-mocking, 0 of None unique commits
- [f340fee7e324](https://github.com/mishmosh/iroh/commit/f340fee7e324fbdfc90755d2606ccfd1a2e85b94) mishmosh/iroh:feat-bitswap-providers, 0 of None unique commits
- [1999ea51cfee](https://github.com/plauche/iroh/commit/1999ea51cfeea4cde6601089264b3e8fbfa639e4) plauche/iroh:poc-change-p2p-transport, 0 of None unique commits
- [a30d437b1c90](https://github.com/studna/iroh/commit/a30d437b1c901bc18f780e6119668ffb1e76536d) studna/iroh:dependabot/cargo/multihash-0.18, 0 of None unique commits
- [6a96391c1ac0](https://github.com/studna/iroh/commit/6a96391c1ac066d4aa00219c88600a054d724e41) studna/iroh:feat-memesync, 0 of None unique commits
- [cd92be69773c](https://github.com/studna/iroh/commit/cd92be69773cc1cdeea20f11012a281c5fd8eef5) studna/iroh:feat-redb, 0 of None unique commits
- [0a77947e637b](https://github.com/studna/iroh/commit/0a77947e637bf89eb1a54319d437f1bcda9671c0) studna/iroh:rklaehn/gossipsub-quic-rpc, 0 of None unique commits
- [12bc70f76786](https://github.com/HeddleCo/iroh/commit/12bc70f76786d7ca4628e88232a1daf71187f0bd) HeddleCo/iroh:generated-docs-preview, 0 of None unique commits
- [a4f1d026c7f5](https://github.com/drlukeangel/iroh/commit/a4f1d026c7f5218b4fac5ee47e32d0e58b8d792c) drlukeangel/iroh:generated-docs-preview, 0 of None unique commits
- [3045594805cc](https://github.com/holon-technologies/iroh/commit/3045594805ccafb7fbca542d3c70a55ea7202ab9) holon-technologies/iroh:chore/closure-pass, 500 of 1329 unique commits
- [229560450340](https://github.com/holon-technologies/iroh/commit/229560450340a786b0e83dfd21d9a54cc5a9a668) holon-technologies/iroh:chore/repo-showcase-readiness, 500 of 1290 unique commits

## Boundaries
- Every SHA is a branch head, not necessarily a separate fix.
- A branch comparison to upstream is not a diff against calex794/roha.
- No entry has been marked merged, tested, or released by this script.
- Without a common ancestor or with truncated files, manual investigation remains.
