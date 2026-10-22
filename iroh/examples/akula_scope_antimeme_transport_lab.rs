//! AK-DATA-CTRL-01D: bounded transport of an opaque Akula scope revocation.
//!
//! Roha authenticates the sender's EndpointId and encrypts transport. The
//! receiving *Akula policy engine* must independently verify the origin's
//! Ed25519 signature and scope before applying any state change.
//!
//! This is a three-endpoint direct-only lab. No public n0 relay, address
//! lookup, vault, file access, proof signing authority or network discovery.
use std::time::Duration;

use iroh::{Endpoint, EndpointId, endpoint::presets};
use n0_error::{AnyError, Result, StdResultExt, ensure_any};

const ALPN: &[u8] = b"akula/staya/scope-antimeme-transport-lab/0";
const SCOPE_BYTES: usize = 16;
const PROOF_BYTES: usize = 32 + 8 + 64; // Exact Akula lab signed-proof framing.
const FRAME_BYTES: usize = SCOPE_BYTES + PROOF_BYTES;
const MAX_WAIT: Duration = Duration::from_secs(12);
const ORIGIN_DOMAIN: &[u8] = b"AKULA:NODE:READ-SCOPE-REVOKE:LAB-V0\0";

fn signed_bytes(scope: [u8; SCOPE_BYTES], doc: [u8; 32], seq: u64) -> Vec<u8> {
    let mut data = ORIGIN_DOMAIN.to_vec();
    data.extend_from_slice(&scope);
    data.extend_from_slice(&doc);
    data.extend_from_slice(&seq.to_be_bytes());
    data
}

// Public deterministic test key. This key is separate from QUIC EndpointId.
fn synthetic_signed_frame(
    scope: [u8; SCOPE_BYTES],
    document: [u8; 32],
    sequence: u64,
    admin: &iroh::SecretKey,
) -> [u8; FRAME_BYTES] {
    let mut data = [0u8; FRAME_BYTES];
    data[..SCOPE_BYTES].copy_from_slice(&scope);
    data[SCOPE_BYTES..SCOPE_BYTES + 32].copy_from_slice(&document);
    data[SCOPE_BYTES + 32..SCOPE_BYTES + 40].copy_from_slice(&sequence.to_be_bytes());
    let signature = admin.sign(&signed_bytes(scope, document, sequence));
    data[SCOPE_BYTES + 40..].copy_from_slice(&signature.to_bytes());
    data
}

fn verify_signed_origin(frame: &[u8], pinned_admin: &iroh::PublicKey) -> Result<()> {
    ensure_any!(frame.len() == FRAME_BYTES, "invalid proof length");
    let scope: [u8; SCOPE_BYTES] = frame[..SCOPE_BYTES].try_into().anyerr()?;
    let doc: [u8; 32] = frame[SCOPE_BYTES..SCOPE_BYTES + 32].try_into().anyerr()?;
    let seq = u64::from_be_bytes(
        frame[SCOPE_BYTES + 32..SCOPE_BYTES + 40]
            .try_into()
            .anyerr()?,
    );
    ensure_any!(seq > 0 && seq <= i64::MAX as u64, "invalid sequence");
    let sig: [u8; 64] = frame[SCOPE_BYTES + 40..].try_into().anyerr()?;
    ensure_any!(
        pinned_admin
            .verify(
                &signed_bytes(scope, doc, seq),
                &iroh::Signature::from_bytes(&sig)
            )
            .is_ok(),
        "invalid scope administrator signature"
    );
    Ok(())
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
    pinned_admin: &iroh::PublicKey,
    bytes: &[u8],
    after_acceptance: impl FnOnce(&[u8]) -> Result<()>,
) -> Result<Vec<u8>> {
    ensure_any!(
        bytes.len() == FRAME_BYTES,
        "incorrect Akula revocation frame length"
    );

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
        verify_signed_origin(&received, pinned_admin)?;
        // The acceptance callback owns the policy boundary. The cross-repo
        // test returns Ok only after Akula confirms a durable, exact proof.
        after_acceptance(&received)?;
        response.write_all(b"OK").await.anyerr()?;
        response.finish().anyerr()?;
        // Retain the receiving connection until the sender has consumed ACK.
        // Otherwise dropping it races peer read_to_end and closes QUIC with 0.
        connection.closed().await;
        Ok::<Vec<u8>, AnyError>(received)
    };

    let sending = async {
        let connection = source.connect(destination.addr(), ALPN).await?;
        let (mut request, mut response) = connection.open_bi().await.anyerr()?;
        request.write_all(bytes).await.anyerr()?;
        request.finish().anyerr()?;
        let acknowledgment = response.read_to_end(2).await.anyerr()?;
        ensure_any!(
            acknowledgment == b"OK",
            "receiving neighbor did not acknowledge"
        );
        connection.close(0u32.into(), b"ack-consumed");
        Ok::<(), AnyError>(())
    };

    // Bounded total handshake, write, verification and ACK; no unbounded
    // connection wait when a peer becomes unavailable.
    let (received, ()) =
        tokio::time::timeout(MAX_WAIT, async { tokio::try_join!(receiving, sending) })
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

    // Exactly the Akula PR #570 origin-signing domain and 104-byte proof
    // structure. Roha's neighbor identity is NOT this administrator key.
    let allowed_scope = [0x42u8; SCOPE_BYTES];
    let admin = iroh::SecretKey::from_bytes(&[87u8; 32]);
    let pinned = admin.public();
    let original = synthetic_signed_frame(allowed_scope, [0x55; 32], 7, &admin);

    let mut accepted_b = None;
    let at_b = hop(&a, &b, a.id(), allowed_scope, &pinned, &original, |proof| {
        accepted_b = Some(proof.to_vec());
        Ok(())
    })
    .await?;
    ensure_any!(at_b == original, "A->B proof bytes changed");
    ensure_any!(accepted_b == Some(original.to_vec()), "B was not admitted");
    let mut accepted_c = None;
    let at_c = hop(&b, &c, b.id(), allowed_scope, &pinned, &at_b, |proof| {
        accepted_c = Some(proof.to_vec());
        Ok(())
    })
    .await?;
    ensure_any!(at_c == original, "B->C proof bytes changed");
    ensure_any!(accepted_c == Some(original.to_vec()), "C was not admitted");

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
        let admin = iroh::SecretKey::from_bytes(&[87u8; 32]);
        let pinned = admin.public();
        let payload = synthetic_signed_frame([0x42; SCOPE_BYTES], [0x55; 32], 7, &admin);
        let result = hop(
            &a,
            &b,
            other.id(),
            [0x42u8; SCOPE_BYTES],
            &pinned,
            &payload,
            |_| Ok(()),
        )
        .await;
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
        let admin = iroh::SecretKey::from_bytes(&[87u8; 32]);
        let pinned = admin.public();
        let payload = synthetic_signed_frame([0x01; SCOPE_BYTES], [0x55; 32], 7, &admin);
        let rejected = hop(
            &a,
            &b,
            a.id(),
            [0x42u8; SCOPE_BYTES],
            &pinned,
            &payload,
            |_| Ok(()),
        )
        .await;
        ensure_any!(rejected.is_err(), "wrong scope was accepted");
        let too_long = vec![0u8; FRAME_BYTES + 1];
        let rejected = hop(
            &a,
            &b,
            a.id(),
            [0x42u8; SCOPE_BYTES],
            &pinned,
            &too_long,
            |_| Ok(()),
        )
        .await;
        ensure_any!(rejected.is_err(), "oversized local frame was accepted");
        a.close().await;
        b.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_signed_origin_from_an_unpinned_admin_is_never_acked() -> Result<()> {
        let a = initiate_from_loopback().await?;
        let b = accept_on_loopback().await?;
        let legitimate = iroh::SecretKey::from_bytes(&[87u8; 32]);
        let impostor = iroh::SecretKey::from_bytes(&[13u8; 32]);
        let payload = synthetic_signed_frame([0x42; SCOPE_BYTES], [0x55; 32], 7, &impostor);
        let refused = hop(
            &a,
            &b,
            a.id(),
            [0x42; SCOPE_BYTES],
            &legitimate.public(),
            &payload,
            |_| Ok(()),
        )
        .await;
        ensure_any!(refused.is_err(), "forged admin was acknowledged");
        a.close().await;
        b.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn positive_ack_requires_successful_acceptance_callback() -> Result<()> {
        let a = initiate_from_loopback().await?;
        let b = accept_on_loopback().await?;
        let admin = iroh::SecretKey::from_bytes(&[87u8; 32]);
        let payload = synthetic_signed_frame([0x42; SCOPE_BYTES], [0x55; 32], 7, &admin);
        let rejected = hop(
            &a,
            &b,
            a.id(),
            [0x42; SCOPE_BYTES],
            &admin.public(),
            &payload,
            |_| {
                ensure_any!(false, "synthetic policy persistence rejected");
                Ok(())
            },
        )
        .await;
        ensure_any!(rejected.is_err(), "ACK before policy acceptance");
        a.close().await;
        b.close().await;
        Ok(())
    }
}

// The ignored cross-repository test needs the separately built Akula binary.
#[cfg(all(test, target_os = "linux"))]
#[path = "akula_scope_antimeme_transport_lab/cross_repo.rs"]
mod cross_repo;
