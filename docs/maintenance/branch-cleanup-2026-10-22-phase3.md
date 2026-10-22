# Roha branch cleanup — phase 3, 2026-10-22

## Completed archive batches

- Phase 1: 16 branches, [PR #36](https://github.com/calex794/roha/pull/36), [archive run 39701032101](https://github.com/calex794/roha/actions/runs/39701032101).
- Phase 2: 4 branches, [PR #37](https://github.com/calex794/roha/pull/37), [archive run 39773821288](https://github.com/calex794/roha/actions/runs/39773821288).
- Independently checked the live ref inventory after phase 2: all 20 exact-SHA archive tags exist, their branch refs are absent, 36 branches remain. All 23 then-open PR heads and their stacked bases remain present.
- These are snapshot counts; concurrent development can create or close other refs.

## Phase 3: keep both Patchbay experiments on one branch

| Role | Ref | Original exact SHA |
| --- | --- | --- |
| Keeper | `lab/staya-hosted-patchbay-20261018` | `1564d2195107f0f446ce8c499d8afadc8bd9817c` |
| Retirement candidate | `lab/staya-paired-queue-capacity-20261018` | `d4eed219bffede909441cffdc8024ec285afe77f` |
| Shared experimental base | `integration/iroh-4599-4603-compat-20261018` | `a4854254bb143f91bf416f1a2601bd0394ecb034` |

Compared every blob path and SHA in the two recursive Git trees. The only
differences are the two distinct workflow files:
`.github/workflows/staya-hosted-patchbay.yml` and
`.github/workflows/staya-paired-queue-capacity.yml`.
Transport source, tests and dependency files are identical.

Keeper commit [177542dbf8a3d327f012ebd9e015c90bf2e1d1cc](https://github.com/calex794/roha/commit/177542dbf8a3d327f012ebd9e015c90bf2e1d1cc)
is a real two-parent merge of both exact heads. Both histories and both
workflow scopes are retained. All 243 other original blob paths keep the
same SHA. The paired workflow now triggers on the keeper branch, while its
baseline/corrected checkout SHAs and alternating measurements are unchanged.

[Draft PR #38](https://github.com/calex794/roha/pull/38) exposes this consolidation
against the shared experimental base. It does not promote the experimental
transport to main. The open PR also makes the keeper ineligible for ordinary
orphan archival.

### What the tests actually establish

- Historical paired run [39155929056](https://github.com/calex794/roha/actions/runs/39155929056): success.
- Historical hosted run [39156556550](https://github.com/calex794/roha/actions/runs/39156556550): failed at `unshare -Ur true`; compilation and relay scenarios were not executed.
- Keeper applies the hosted namespace setup already used in main's Patchbay
  workflow and keeps the mandatory `unshare` probe. New CI results must be
  assessed separately; preservation of a test is not evidence of a passing test.
- Local checks: YAML parses, both push triggers target the keeper, permissions
  remain read-only, embedded Bash passes `bash -n`.
- Existing archive/pruner regression suite: 21 tests passed offline.
- GitHub compare from the retirement candidate to the keeper reports
  `behind_by=0`, with the candidate itself as merge base.

### Archive conditions

Only the single paired-workflow branch is listed in the phase-3 roster.
The existing trusted-main workflow must still confirm the exact branch SHA,
keeper ancestry, an immutable matching archive tag, an unprotected ref, and
absence of open PR head/base use. Deletion remains guarded by force-with-lease.
No archive helper, provenance logic, required check or branch rule is relaxed.

Expected tag:
`archive/lab/staya-paired-queue-capacity-20261018/d4eed219bffe`.

Retirement is complete only when the main archive run succeeds, that tag
resolves to `d4eed219bffede909441cffdc8024ec285afe77f`, and the candidate branch
ref is absent. The keeper and all open PR dependencies must remain present.

## Remaining retained work

The [phase-2 audit](https://github.com/calex794/roha/pull/37#issuecomment-6339774463)
records the other nine orphan branches. They are deliberately outside this
one-row archival change:

| Branch | Next disposition |
| --- | --- |
| `fix/archive-ref-lookup-and-janitor-20261022` | Compare closed PR #30 with accepted #31/#36. Its six alternative pruner assertions have corresponding coverage in the current canonical suite; no deletion is authorized by ancestry yet. |
| `fix/iroh-4600-stale-selected-path` | Preserve functional fix and repro; prepare a focused integration decision. |
| `lab/staya-custom-path-withdrawal-repro-20261019` | Preserve the unique regression before transplantation. |
| `lab/staya-iroh-fork-census-20261019` | Retain the research corpus and its exact history; do not bulk-import generated inventories into runtime source. |
| `lab/staya-relay-4616-4617-joint-20261019` | Reconcile joint relay implementation and tests. |
| `lab/staya-relay-backend-restart-20261019` | Preserve strict TLS, datagrams and client reconnection; the branch name does not prove server restart. See the phase-4 review. |
| `lab/staya-relay-prefix-4617-20261019` | Compare prefix/client/reporting deltas with the joint branch. |
| `lab/staya-relay-tls-parallel-20261019` | Preserve distinct TLS-churn scenarios; divergence is not redundancy. |
| `repro/iroh-4603-speed-control-20261018` | Retain the independent control regression and the paired experiment's pinned baseline. |

A low branch count is not the acceptance criterion. Preserved provenance,
usable test scenarios and explicit integration decisions are.
