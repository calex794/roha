# AK-DATA-CTRL-01D: Roha transport of Akula scoped anti-meme bytes

**Experimental only; independent Roha fork, not integrated into production Akula.**

Akula's [read-side lab PR #570](https://github.com/Lalalalendia/Akula/pull/570) already defines an exact 104-byte signed-proof wire record: document digest (32 bytes), monotone sequence (8 bytes), origin scope-administrator Ed25519 signature (64 bytes). The receiving Akula node must verify the signature over the local pinned VaultId, document ID and sequence. **Roha never decides whether this proof is authorized.**

This Roha example uses a 16-byte scope prefix plus the exact 104-byte opaque Akula record, with fixed ALPN, a 120-byte maximum frame, an authenticated and explicitly pinned sending EndpointId, a scope match, a bounded handshake timeout, and a two-byte receipt only after the frame has been accepted as a transport frame.

The code uses `presets::Minimal`: no public n0.computer relay server and no automatic address lookup. Three independently keyed Iroh QUIC endpoints exchange the same opaque 120-byte frame A to B and B to C using direct IP connectivity. The exercise tests that the frame arrives unmodified and a wrong neighbor identity or scope is rejected. It does not decode or validate the embedded Ed25519 signature or make any File Gate / catalog state changes.

Run from Roha repo root:

    cargo test --locked -p iroh --example akula_scope_antimeme_transport_lab -- --nocapture

Existing Roha laboratory [PR #19](https://github.com/calex794/roha/pull/19) already covers ten independently keyed Docker nodes and hundreds of authenticated messages; [PR #20](https://github.com/calex794/roha/pull/20) covers multi-network store-and-forward. This deliberately small example is a **transport adapter contract**, not a second messenger implementation.

## Boundaries and remaining milestones

- No actual owner/admin granting, no content-hash or similarity classification, no delete or remote file mutation.
- The frame only proves QUIC delivery and neighbor identity. Authenticity of the *origin revocation* still requires a pinned Akula scope key and signature verification; a peer can forward or corrupt bytes, not acquire the admin's signing rights.
- No public upstream, self-hosted relay or NAT traversal measured in this example; direct-only peers on one host are not a real wide-area mesh.
- Still required: exchange of Akula-signed proof between separate Roha Node processes, handoff to production authorized scope policy, durable independent rollback witness, and replay/anti-entropy reconciliation through STAYA.
- The code is additive to Roha examples and tests; it does not modify the Iroh 1.3 transport implementation or API and should not be merged before exact-head CI.

## Verified failure investigation (new change pending CI)

Initial direct-only CI compiled the example and passed both rejection tests, but the A→B→C positive test failed with `connection lost: closed by peer 0`. This was a teardown/ACK ordering bug: the receiving coroutine returned as soon as its OK stream was finished, closing the connection before the sending coroutine had consumed the final ACK. The receiver now waits for the sender to close the authenticated QUIC connection **after** it reads OK. The transport still does not imply Akula policy authorization. The prior CI also failed source formatting and the repository's pinact/zizmor checks, so the workflow now uses the already vetted pinned rust-toolchain action SHA and the example follows rustfmt. Exact-head CI is required to validate the fixes.

## Ed25519 origin verification before QUIC acknowledgment

The next experimental version replaces the previously meaningless 104-byte dummy proof with a **real Ed25519-signed Akula-compatible test record**. The signed input is identical to [Akula PR #570](https://github.com/Lalalalendia/Akula/pull/570): `AKULA:NODE:READ-SCOPE-REVOKE:LAB-V0\0 || VaultId(16) || DocumentId(32) || sequence(u64 big-endian)`. The 120-byte Roha frame adds a 16-byte scope prefix before Akula's 104-byte record. The fixed `[87;32]` private signing seed is an openly documented fixture and is **not a production key**; QUIC endpoint identities remain separate.

The receiver validates the pin for the scope, checks the source signature using Iroh's Ed25519 PublicKey, and calls a separate admission callback. Only after that callback returns success does it send QUIC `OK`. The callback is **in-memory only** in current positive tests: it proves acknowledgment ordering, not real durable SQLite acceptance. Negative tests reject a properly transported frame bearing another administrator's signature and withhold acknowledgment when the acceptance callback refuses.

**Still missing to complete the requested cross-repository integration:** connect the callback to a real Akula `ScopedRevocations::ingest()` transaction through its loopback Node interface, after which a positive ACK will only follow confirmed SQLite commit. That must run with both compiled runtimes, not simulated in Iroh. Do not claim that current changes are production device ACL, durable witness or genuine Roha ↔ Akula cross-process acceptance.
