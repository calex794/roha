//! AK-DATA-CTRL-01D: bounded transport of an opaque Akula scope revocation.
//!
//! Roha authenticates the sender's EndpointId and encrypts transport. The
//! receiving *Akula policy engine* must independently verify the origin's
//! Ed25519 signature and scope before applying any state change.
//!
//! This is a three-endpoint direct-only lab. No public n0 relay, address
//! lookup, vault, file access, proof signing authority or network discovery.
use iroh::{Endpoint, EndpointId, endpoint::presets};
use n0_error::{AnyError, Result, StdResultExt, ensure_any};
use std::time::Duration;

const ALPN: &[u8] = b"akula/staya/scope-antimeme-transport-lab/0";
const SCOPE_BYTES: usize = 16;
const PROOF_BYTES: usize = 32 + 8 + 64; // Exact Akula lab signed-proof framing.
const FRAME_BYTES: usize = SCOPE_BYTES + PROOF_BYTES;
const MAX_WAIT: Duration = Duration::from_secs(12);

fn frame(scope: [u8; SCOPE_BYTES], proof: [u8; PROOF_BYTES]) -> [u8; FRAME_BYTES] {
    let mut out = [0u8; FRAME_BYTES];
    out[..SCOPE_BYTES].copy_from_slice(&scope);
    out[SCOPE_BYTES..].copy_from_slice(&proof);
    out
}

/// Exactly one authenticated, bounded direct QUIC hop.
///
/// Important: the QUIC peer key is NOT the owner's policy-signing key; this
/// function has no authority to accept a BAN or trust its signed content.
async fn hop(
    source: &Endpoint,
    destination: &Endpoint,
    allowed_source: EndpointId,
    allowed_scope: [u8; SCOPE_BYTES],
    bytes: &[u8],
) -> Result<Vec<u8>> {
    ensure_any!(bytes.len() == FRAME_BYTES, "incorrect Akula revocation frame length");

    let receiving = async {
        let incoming = destination.accept().await.anyerr()?;
        let connection = incoming.await.anyerr()?;
        ensure_any!(
            connection.remote_id() == allowed_source,
            "unexpected sender identity (not a pinned neighbor)"
        );
        let (mut response, mut request) = connection.accept_bi().await.anyerr()?;
        let received = request.read_to_end(FRAME_BYTES).await.anyerr()?;
        ensure_any!(
            received.len() == FRAME_BYTES,
            "truncated Akula revocation frame"
        );
        ensure_any!(
            received[..SCOPE_BYTES] == allowed_scope,
            "scope does not match local subscription"
        );
        response.write_all(b"OK").await.anyerr()?;
        response.finish().anyerr()?;
        Ok::<Vec<u8>, AnyError>(received)
    };

    let sending = async {
        let connection = source.connect(destination.addr(), ALPN).await?;
        let (mut request, mut response) = connection.open_bi().await.anyerr()?;
        request.write_all(bytes).await.anyerr()?;
        request.finish().anyerr()?;
        let acknowledgment = response.read_to_end(2).await.anyerr()?;
        ensure_any!(acknowledgment == b"OK", "receiving neighbor did not acknowledge");
        Ok::<(), AnyError>(())
    };

    // Bounded total handshake, write, verification and ACK; no unbounded
    // connection wait when a peer becomes unavailable.
    let (received, ()) = tokio::time::timeout(MAX_WAIT, async {
        tokio::try_join!(receiving, sending)
    })
    .await
    .anyerr()??;
    Ok(received)
}

async fn demonstration() -> Result<()> {
    // Minimal has no DNS/pkarr lookup; helper binds ONLY loopback. This
    // experiment must never silently fall back to public n0 infrastructure.
    let a = initiate_from_loopback().await?;
    let b = accept_on_loopback().await?;
    let c = accept_on_loopback().await?;

    for peer in [&a, &b, &c] {
        ensure_any!(
            peer.addr().relay_urls().next().is_none(),
            "direct-only experiment unexpectedly offered a relay"
        );
        ensure_any!(
            peer.addr().ip_addrs().all(|ip| ip.ip().is_loopback()),
            "lab socket escaped the local loopback interface"
        );
    }

    // The bytes below are a synthetic stand-in for an owner/admin-signed
    // Akula proof, not an actual valid signature. We only prove transport
    // preservation; Akula must perform signature verification separately.
    let allowed_scope = [0x42u8; SCOPE_BYTES];
    let mut dummy_proof = [0u8; PROOF_BYTES];
    dummy_proof[..32].fill(0x55); // synthetic document ID
    dummy_proof[32..40].copy_from_slice(&7u64.to_be_bytes());
    dummy_proof[40..].fill(0x83);
    let original = frame(allowed_scope, dummy_proof);

    let at_b = hop(&a, &b, a.id(), allowed_scope, &original).await?;
    ensure_any!(at_b == original, "A->B proof bytes changed");
    let at_c = hop(&b, &c, b.id(), allowed_scope, &at_b).await?;
    ensure_any!(at_c == original, "B->C proof bytes changed");

    a.close().await;
    b.close().await;
    c.close().await;
    Ok(())
}


async fn initiate_from_loopback() -> Result<Endpoint> {
    Ok(Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")?
        .bind()
        .await?)
}

async fn accept_on_loopback() -> Result<Endpoint> {
    Ok(Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")?
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?)
}

#[tokio::main]
async fn main() -> Result<()> {
    demonstration().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn three_distinct_direct_roha_identities_forward_one_opaque_antimeme() -> Result<()> {
        demonstration().await
    }

    #[tokio::test]
    async fn wrong_sender_identity_does_not_complete_ack() -> Result<()> {
        let a = initiate_from_loopback().await?;
        let b = accept_on_loopback().await?;
        let other = initiate_from_loopback().await?;
        let payload = frame([0x42u8; SCOPE_BYTES], [0u8; PROOF_BYTES]);
        let result = hop(&a, &b, other.id(), [0x42u8; SCOPE_BYTES], &payload).await;
        ensure_any!(result.is_err(), "unrecognized sender was accepted");
        a.close().await;
        b.close().await;
        other.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn wrong_scope_cannot_complete_ack_and_no_extra_bytes_are_allowed() -> Result<()> {
        let a = initiate_from_loopback().await?;
        let b = accept_on_loopback().await?;
        let payload = frame([0x01u8; SCOPE_BYTES], [0u8; PROOF_BYTES]);
        let rejected = hop(&a, &b, a.id(), [0x42u8; SCOPE_BYTES], &payload).await;
        ensure_any!(rejected.is_err(), "wrong scope was accepted");
        let too_long = vec![0u8; FRAME_BYTES + 1];
        let rejected = hop(&a, &b, a.id(), [0x42u8; SCOPE_BYTES], &too_long).await;
        ensure_any!(rejected.is_err(), "oversized local frame was accepted");
        a.close().await;
        b.close().await;
        Ok(())
    }
}
