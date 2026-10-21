# Roha multi-network mailbox laboratory

This **separate** experiment extends the ten-peer local lab with two volunteer
gateway processes. Three Docker networks (alpha, beta, gamma) have **no shared
user network** and are marked Docker `internal`. gateab connects alpha+beta,
gatebc connects beta+gamma. Ten ordinary peers remain independent.

Run from Roha repository root:

```sh
python3 labs/three-net/run.py --output labs/three-net/report.json
```

Topology:

peer01 (alpha) -> gateab (alpha,beta) -> gatebc (beta,gamma) -> peer10 (gamma, OFFLINE).

- All **three stages** use real authenticated Roha/noq QUIC messages.
- The 1st and 2nd gateway have their own persistent queues and keys.
- The sender's Ed25519 signature covers sender, recipient, message id and body.
  Each gateway verifies it; recipient verifies it again independently.
- Only enrolled QUIC remote identities can deposit or retrieve the queue.
- Gateway stores an immutable signed envelope before `STORED` receipt;
  recipient obtains it **only when online** via authorized pull.
- Re-delivery does not create another local inbox record.
- All keys, inboxes and mailbox queues persist in dedicated Docker volumes.
- Isolation tests require direct alpha-to-gamma connection to fail, unauthorized
  receiver to find no messages, and stopped gateway to block new submissions.

**Crucial security limitations:** Signed plaintext stored on gateways is
**not end-to-end encrypted**. A mailbox `STORED` receipt is not an endpoint
`RECIPIENT_ACCEPTED` receipt. No mailbox deletion/garbage collection,
expiry, cryptographic route grant, revocation, quota-by-origin, chaos disk
write failure injection, malicious intermediary analysis, or resilient
automatic rerouting has been completed in this experiment. Queue length is
bounded to 128, message body to 1024 bytes, and peer admission is explicit.
Do not use real content, ship or merge to Akula until replacing this teaching
format with the actual WOW W3/W4 encrypted envelope and admission controls.

Physical WAN, NAT/CGNAT and different network operators are out of scope.
