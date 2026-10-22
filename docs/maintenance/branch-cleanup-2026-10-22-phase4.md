# Roha branch cleanup — phase 4, 2026-10-22

## Scope and provenance

Phase 3 finished in [PR #39](https://github.com/calex794/roha/pull/39):
21 legacy branch tips are preserved by exact-SHA archive tags. Its temporary
cleanup branch has also been removed.

This phase consolidates two relay laboratories that differ in exactly two
blob paths: `iroh-relay/tests/relay_axum.rs` and
`.github/workflows/staya-relay-prefix-nginx-e2e.yml`.

| Role | Branch | Audited original tip |
| --- | --- | --- |
| Keeper | `lab/staya-relay-backend-restart-20261019` | `2ef1faf72eef98dc4496527a85ce91fcd82b07b2` |
| Retirement candidate | `lab/staya-relay-tls-parallel-20261019` | `30e732746a3ccc110010da01227800dbb0013100` |
| Shared ancestor | History retained through both parents | `8dee11dd0a875f865abde01dfc0a7edddf003603` |

Consolidation commit
[509c71fe09486e6b29ab0a18d0bd2158cc89d006](https://github.com/calex794/roha/commit/509c71fe09486e6b29ab0a18d0bd2158cc89d006)
has both original tips as parents. A complete recursive-tree comparison
confirms that all 237 other blob paths are unchanged. Changes are limited to
one integration-test file and two workflow files (the second workflow only
gets verifiable action version comments and nonpersistent checkout credentials).
Production Rust source and every dependency file retain their original SHAs.

GitHub confirms that the retirement candidate is an ancestor of this keeper
(`behind_by=0` and merge base equal to the candidate tip).

## Preserve the scenarios, not just the branch names

The backend-named branch contains strict TLS verification, actual datagram
delivery and reconnection of **client A** with its original key. It does not
establish that the relay server itself was stopped and restarted. Its nginx
workflow also omitted the current keeper name from the push trigger.

The TLS-parallel sibling contains a distinct concurrent-client scenario,
but lacks the strict-certificate, payload and reconnection sections.
Replacing either whole file with the other would lose coverage.

The keeper preserves both sets in the existing test and repairs the push
trigger. A reusable barrier coordinates the simultaneous clients' release;
this removes the lost-wakeup window of a one-shot notification without
weakening the eight-second hold or the post-hold pings.

## Acceptance matrix

The dedicated `STAYA consolidated relay scenarios` workflow executes these
eight commands on GitHub-hosted Linux with real local nginx, OpenSSL and curl:

| Scenario | Required observation |
| --- | --- |
| HTTPS prefix handshake and probe | Authenticated relay connection; prefixed probe works; unprefixed probe is rejected. |
| Existing relay-route unit test | Path-prefix behavior remains backward compatible. |
| 32 sequential connections | All 32 TLS/WebSocket handshakes complete and the live probe stays available. |
| Bidirectional datagrams | Eight directional messages arrive with exact payloads and authenticated sender IDs. |
| Client reconnection | A reconnects with its original identity; B stays online; both post-reconnect messages arrive. |
| Strict TLS plus payloads | Local-CA chain accepted, unrelated roots rejected, payloads delivered. |
| 16 simultaneous clients | All 16 authenticate, stay connected for eight seconds and answer post-hold pings. |
| Combined modes | Strict TLS, datagrams, client reconnection and all 16 parallel clients work in the same test invocation. |

Original scoped evidence:
[backend/strict-TLS run 39295366961](https://github.com/calex794/roha/actions/runs/39295366961)
and [parallel run 39286185577](https://github.com/calex794/roha/actions/runs/39286185577).

Consolidated execution begins with
[run 39786973172](https://github.com/calex794/roha/actions/runs/39786973172).
Its logs and any subsequent keeper run are the runtime evidence. Local checks
cover YAML structure, the eight scenario commands, combined environment flags,
read-only permissions and embedded Bash syntax. Rust compilation and real
network behavior are checked by the dedicated workflow, not claimed locally.

The review must verify actual nonzero test counts and all scenario markers.
The exact lockfile is also checked for mutations after the scenarios.
This is laboratory preservation, not acceptance of all experimental relay
changes into main.

## Guarded retirement

The main-only archive roster has one row: the TLS-parallel candidate above.
The existing helper must still establish the exact original SHA, strict
keeper ancestry, a verified matching archive tag, an unprotected branch and
no open PR head/base dependencies, then delete with force-with-lease.

Expected tag:
`archive/lab/staya-relay-tls-parallel-20261019/30e732746a3c`.

Do not mark this phase complete solely because its PR merged. Verify the
archive run, the tag's full SHA, disappearance of the candidate ref, and
continued existence of the keeper and all open PR refs.

## Remaining work

Retain the keeper, the joint/prefix relay variants, fork census, stale-path
fix, custom-path regression, speed-control repro and the closed janitor
alternative for separate decisions. The next relay comparison must preserve
the distinct production changes in the joint and prefix branches. A real
backend-restart scenario remains a separate coverage gap.
