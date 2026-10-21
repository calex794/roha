# Akula / Roha: ten-peer message laboratory

**Scope:** ten real container processes, ten persisted Ed25519 keys, direct
authenticated Roha/noq QUIC, *no* relay servers and *no* address lookup
services. All UDP traffic stays within a Docker **internal** bridge. No ports
are published to the host. The Python controller bootstraps identities
(out-of-band, analogous to exchanging invitations), but never carries
messages. This is deliberately an *application experiment*, not WOW.

## Run

From the **Roha repository root** with Docker Compose v2:

```sh
python3 labs/ten-peer/run.py --output labs/ten-peer/report.json
```

Build uses Rust 1.91 and the repository's checked-in Cargo.lock.
The script creates an isolated Docker Compose project and deletes **only its
own** temporary containers/volumes afterward. Add `--keep` to inspect
containers and each peer's private `/state/inbox.jsonl` after the run.
Do not use any real messages or production identities.

## Tested behaviours and limits

* Explicit allow-list handshake; each node independently holds its private key.
* 10 ring + 90 all-to-all messages, authenticated end-to-end transport.
* An identical replay creates no additional inbox record; a changed payload
  with the same (sender key, message id) is rejected.
* Before explicit trust enrollment, unsolicited messages get no durable ACK.
* The public key determines cryptographic identity, not the hostname. With
  learned routes Roha may reach the true key owner despite a mismatched host,
  or it may reject that connection due to an unknown certificate issuer.
  The named (wrong) container must **never** accept the message.
* If the actual public-key owner is offline, another live host cannot
  impersonate it to produce a successful durable ACK.
* Restart preserves inbox and public identity.
* An offline recipient is **not** silently marked delivered; a **manual**
  resend after restart delivers once.
* The receiver writes and synchronizes its inbox **before** ACK. This
  primitive is intentionally not a complete crash-safe transactional store.

**Not proven:** traversal of real NAT/CGNAT, multiple network domains, relay,
multi-hop forwarding, offline mailboxes, end-to-end application encryption of
stored messages, ordered group history, key rotation/revocation, adversarial
filesystem crashes, or Akula UI integration.

**Important:** transport-level encrypted QUIC is not a substitute for
end-to-end encrypted **stored** messages and durable offline delivery. Never
reuse this lab-only JSON message format as a replacement for the WOW envelope.

Plan next: run the same ten identities across 2–3 Docker network domains;
then add a voluntary mailbox with immutable encrypted envelopes, explicit
resource quotas, durable receipts, and negative authorization tests. For
real NAT behaviour, follow with independent physical machines and networks.
