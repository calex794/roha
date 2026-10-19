# STAYA, Iroh history part 10

Source: frozen history_filtered_shas.csv, 2026-10-18
Reviewed UTC: 2026-10-19T11:14:10.835427+00:00
Range: 937-1040 / 1762

## Evidence labels (NOT final implementation decisions)

| Label | Heads |
|---|---:|
| legacy_architecture_manual_mapping | 10 |
| non_source_diff | 17 |
| separate_historical_lineage | 33 |
| source_changes_require_roha_comparison | 44 |

## Incomplete histories

- [da56508355ef](https://github.com/mwatts/iroh/commit/da56508355efdb48073a8d74b83ea7c6ba9c8bae) mwatts/iroh:arqu/ctl_put_test, 0 of None unique commits
- [65ea431e7154](https://github.com/mwatts/iroh/commit/65ea431e7154572f74f63326ead17a807629ed9b) mwatts/iroh:arqu/experiment_tokio, 0 of None unique commits
- [a9442b3c73bd](https://github.com/mwatts/iroh/commit/a9442b3c73bd0cce125877fd765eee386eb5b434) mwatts/iroh:arqu/one_exp, 0 of None unique commits
- [8149767b5192](https://github.com/mwatts/iroh/commit/8149767b519270fb8eeb361c2eb95da883d0e8eb) mwatts/iroh:arqu/prof, 0 of None unique commits
- [63f0875466d1](https://github.com/mwatts/iroh/commit/63f0875466d10699851ebb35cbd7d971e6f0ad18) mwatts/iroh:arqu/two_exp, 0 of None unique commits
- [cef852688535](https://github.com/mwatts/iroh/commit/cef8526885358bfa9d3cbb3f0f9cc1d330bea82f) mwatts/iroh:x-inline-store, 0 of None unique commits
- [24664c7bab8a](https://github.com/mwatts/iroh/commit/24664c7bab8a6bbdf20be99e7538c798b255088e) mwatts/iroh:arqu/moar_speed, 0 of None unique commits
- [1a93d55b0d67](https://github.com/mwatts/iroh/commit/1a93d55b0d67357b5c5d62c1d09cefeda7d6459e) mwatts/iroh:arqu/testing_p2p, 0 of None unique commits
- [489e71144595](https://github.com/mwatts/iroh/commit/489e7114459582aea6a78f6752e971cf9291e16f) mwatts/iroh:arqu/testing_p2p_2, 0 of None unique commits
- [99fbe1b09b89](https://github.com/mwatts/iroh/commit/99fbe1b09b899cf4cb61104420face0af0ceeec9) mwatts/iroh:arqu/balance_channels, 0 of None unique commits
- [64e8cd312563](https://github.com/mwatts/iroh/commit/64e8cd31256312bb9a326aaa017477e14b9a0f96) mwatts/iroh:arqu/fbp_test, 0 of None unique commits
- [014e3e14c08a](https://github.com/mwatts/iroh/commit/014e3e14c08a106f250144a8eac504e7fe67d548) mwatts/iroh:arqu/pool_party, 0 of None unique commits
- [5e4533ad988f](https://github.com/mwatts/iroh/commit/5e4533ad988f2addedd6dd40c178c931201ff946) mwatts/iroh:arqu/runtime_tests, 0 of None unique commits
- [723cde31e92c](https://github.com/mwatts/iroh/commit/723cde31e92c6643598353646b990151b5d3d1d3) mwatts/iroh:arqu/tpc_gw, 0 of None unique commits
- [6540b6ffb7fe](https://github.com/mwatts/iroh/commit/6540b6ffb7fec012e2ec75516ab55d50e7295682) mwatts/iroh:b5/cli_docs_spike, 0 of None unique commits
- [72af8c7e231e](https://github.com/mwatts/iroh/commit/72af8c7e231ebfb0befedd6d258ccfb86187fcff) mwatts/iroh:b5/xtask_install, 0 of None unique commits
- [02216a95b9e0](https://github.com/mwatts/iroh/commit/02216a95b9e079dfdb02d5de354206ba18c422cd) mwatts/iroh:flub/reconfig-rpc-client, 0 of None unique commits
- [602d9310d5b4](https://github.com/mwatts/iroh/commit/602d9310d5b4153e89093e3d5bd570879af62612) mwatts/iroh:iroh-0-49, 0 of None unique commits
- [74cb35341125](https://github.com/mwatts/iroh/commit/74cb35341125b2b97b31ea82bbae2b7cbceb98fe) mwatts/iroh:arqu/conn_pool_new, 0 of None unique commits
- [4d581e770a1f](https://github.com/mwatts/iroh/commit/4d581e770a1f7d1b5e047d927640c15f7c953f97) mwatts/iroh:arqu/connection_pooling, 0 of None unique commits
- [93089cefb606](https://github.com/mwatts/iroh/commit/93089cefb60623ccda40eaac1eb6e27f91a758a4) mwatts/iroh:b5/iroh_start, 0 of None unique commits
- [ec3c8096758f](https://github.com/mwatts/iroh/commit/ec3c8096758f9df93c463abac9206fe7d3b9bb28) mwatts/iroh:b5/iroh_start_simple, 0 of None unique commits
- [394a7b0dca69](https://github.com/mwatts/iroh/commit/394a7b0dca691a574e0fb08766468212e8114409) mwatts/iroh:b5/macos-config-dir, 0 of None unique commits
- [24d2023f7bcc](https://github.com/mwatts/iroh/commit/24d2023f7bcc1ffb4bcf6cdc89d26f2012000aa4) mwatts/iroh:bitswap2, 0 of None unique commits
- [01665daa83b9](https://github.com/mwatts/iroh/commit/01665daa83b90aee85614e1320a8199f6c0ed8bb) mwatts/iroh:debug-message-sending, 0 of None unique commits
- [85b1672f43fd](https://github.com/mwatts/iroh/commit/85b1672f43fdfe61f0a207d10489f88133782701) mwatts/iroh:dig-load-balance, 0 of None unique commits
- [ac1ad759fabb](https://github.com/mwatts/iroh/commit/ac1ad759fabbd3e95674cf132b89e1fbd4d4117f) mwatts/iroh:refactor-bitswap-structures, 0 of None unique commits
- [e730e2e2c5c6](https://github.com/mwatts/iroh/commit/e730e2e2c5c6402fd165911a92edcfd16501cbbb) mwatts/iroh:rklaehn/chunky-add, 0 of None unique commits
- [a7be2f695f61](https://github.com/mwatts/iroh/commit/a7be2f695f616836d2f4ba8dda3ca35019d1b5d6) mwatts/iroh:rklaehn/get-bug-investigatin, 0 of None unique commits
- [7b8eeba46d1e](https://github.com/mwatts/iroh/commit/7b8eeba46d1e66f2f357dace7f6cd7655be10442) mwatts/iroh:streaming-add-test, 0 of None unique commits
- [925b6f7614d7](https://github.com/mwatts/iroh/commit/925b6f7614d7e37b1784e3f9bce039dbb2ce0bd4) mwatts/iroh:arqu/bench_meme, 0 of None unique commits
- [4d598f58a435](https://github.com/mwatts/iroh/commit/4d598f58a435023313ffec8652df3809e1de29cf) mwatts/iroh:b5/cli_commands, 0 of None unique commits
- [288bf3768e5d](https://github.com/mwatts/iroh/commit/288bf3768e5d21e18f440d7fa6fcc5847ac7f721) mwatts/iroh:b5/iroh-embed-spike, 0 of None unique commits

## Boundaries
- Every SHA is a branch head, not necessarily a separate fix.
- A branch comparison to upstream is not a diff against calex794/roha.
- No entry has been marked merged, tested, or released by this script.
- Without a common ancestor or with truncated files, manual investigation remains.
