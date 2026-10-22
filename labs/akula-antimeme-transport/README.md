# AK-DATA-CTRL-01D: Roha delivery with Akula policy acceptance

**Draft laboratory; no production integration or authority to mutate source files.**

The lab delivers the original signed scope revocation A → B → C through three
independently keyed, direct-only Iroh QUIC endpoints. Two separate Akula processes
use the real pinned ReadCatalog and ScopedRevocations SQLite store from
[Akula PR #570](https://github.com/Lalalalendia/Akula/pull/570), pinned to commit
`ed0291f8db71b9072e547b2aee0f85d5d39125c1`.

## Authority and acknowledgment

The fixed 120-byte frame contains VaultId (16 bytes), document ID (32), sequence
(8, big-endian) and the origin administrator's Ed25519 signature (64). Signed
input is `AKULA:NODE:READ-SCOPE-REVOKE:LAB-V0\0 || VaultId || DocumentId || sequence`.
The public deterministic seed `[87; 32]` is a test fixture, never a production key.

Roha pins the QUIC neighbor, scope and origin verification key. QUIC authentication
does not grant administrator or file-owner authority. The real Akula process
independently verifies the signature and scope and admits only catalogued documents.
No default transport API, File Gate writer or common Akula `lib.rs` is changed.

QUIC `OK` is sent only when the admission callback succeeds:
- `applied`: Akula committed the original proof to appdata-only SQLite.
- `duplicate`: Akula already retains that exact signed proof, including on restart.
- Both statuses require a bounded, parsed `PROOF` response matching the original
  104 bytes before acknowledgment.
- `stale`, `rejected`, missing evidence, malformed responses and IPC errors do not
  produce a positive acknowledgment. A newer ban is not a receipt for older bytes.

The receiver keeps its QUIC connection alive until the sender consumes `OK`,
fixing the earlier `connection lost: closed by peer 0` teardown race.

## Acceptance scenario

The dedicated cross-repository test reads both synthetic originals, commits a ban
at B, then injects failure **after SQLite acceptance but before QUIC OK**. It restarts
B and retries the same signed frame: Akula returns `duplicate`, exact evidence is
confirmed and the retry is acknowledged. B forwards to C; C persists the ban,
restarts and also accepts an exact retry. Both nodes keep reads blocked.

The same test rejects an older correctly signed sequence and an unknown document
with a valid administrator signature. Neither changes the retained evidence or
reopens the document. Both source files and source directory entries are unchanged.
This is a controlled disconnect window, not a WAN packet-loss or power-cut test.

## Run

Pure transport and rejection checks:

```sh
cargo test --locked -p iroh --example akula_scope_antimeme_transport_lab -- --nocapture
```

The cross-repository test is explicitly ignored in ordinary runs, so absence of
Akula cannot silently count as a passing integration check. With the separately
built feature-gated Linux binary:

```sh
export AKULA_POLICY_LAB_BIN=/absolute/path/to/akula-node-policy-lab
cargo test --locked -p iroh --example akula_scope_antimeme_transport_lab -- cross_repo::roha_quic_ack_only_after_real_akula_sqlite_acceptance --exact --ignored --nocapture
```

The existing dedicated workflow builds the pinned Akula binary and requires this
invocation in its existing job. No new job or matrix is added. The test support
module lives under `iroh/examples/akula_scope_antimeme_transport_lab/`.

## Limits and related work

Only loopback with `presets::Minimal`: no public relay, discovery or NAT traversal.
The lab proves process-restart persistence, not resistance to a malicious host
restoring an old valid SQLite snapshot. No independent rollback witness, owner/admin
two-party grant, automatic retry queue, production device ACL or source deletion
is provided. A receipt is evidence of current acceptance, not irreversible erasure.

[Roha PR #19](https://github.com/calex794/roha/pull/19) covers ten Docker nodes and
[PR #20](https://github.com/calex794/roha/pull/20) covers multi-network forwarding.
This example remains a small adapter contract, not a second messenger.
