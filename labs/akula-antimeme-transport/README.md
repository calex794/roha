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
