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
    let seq = u64::from_be_bytes(frame[SCOPE_BYTES + 32..SCOPE_BYTES + 40].try_into().anyerr()?);
    ensure_any!(seq > 0 && seq <= i64::MAX as u64, "invalid sequence");
    let sig: [u8; 64] = frame[SCOPE_BYTES + 40..].try_into().anyerr()?;
    ensure_any!(
        pinned_admin
            .verify(&signed_bytes(scope, doc, seq), &iroh::Signature::from_bytes(&sig))
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
        // A production Akula adapter must only return Ok after durable policy
        // acceptance. This lab callback uses memory and does NOT prove fsync.
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
        let result = hop(&a, &b, other.id(), [0x42u8; SCOPE_BYTES], &pinned, &payload, |_| {
            Ok(())
        })
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
        let rejected = hop(&a, &b, a.id(), [0x42u8; SCOPE_BYTES], &pinned, &payload, |_| {
            Ok(())
        })
        .await;
        ensure_any!(rejected.is_err(), "wrong scope was accepted");
        let too_long = vec![0u8; FRAME_BYTES + 1];
        let rejected = hop(&a, &b, a.id(), [0x42u8; SCOPE_BYTES], &pinned, &too_long, |_| {
            Ok(())
        })
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
            &a, &b, a.id(), [0x42; SCOPE_BYTES], &legitimate.public(), &payload, |_| Ok(()),
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
            &a, &b, a.id(), [0x42; SCOPE_BYTES], &admin.public(), &payload, |_| {
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


// This test alone crosses the repository boundary. It runs only when the
// dedicated CI supplies the feature-gated Akula PR #570 Linux node binary.
// It never makes a production Roha listener or mutates user-owned source files.
#[cfg(all(test, target_os = "linux"))]
mod cross_repo {
    use super::*;
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{Shutdown, TcpStream};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    const SCOPE_TEXT: &str = "42424242-4242-4242-4242-424242424242";
    const TEST_TOKEN: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    const FILE_NAME: &str = "public.md";
    const ORIGINAL: &[u8] = b"# Allowed synthetic public data\nAkula and Roha integration\n";

    struct IsolatedRoots {
        path: PathBuf,
        vault: PathBuf,
        appdata: PathBuf,
        document: [u8; 32],
    }

    impl IsolatedRoots {
        fn new(suffix: u8) -> Result<Self> {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .anyerr()?
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "akula-roha-bridge-{}-{nanos}-{suffix}",
                std::process::id()
            ));
            fs::create_dir(&path).anyerr()?;
            let vault = path.join("vault");
            let appdata = path.join("appdata");
            fs::create_dir(&vault).anyerr()?;
            fs::create_dir(&appdata).anyerr()?;
            fs::write(vault.join(FILE_NAME), ORIGINAL).anyerr()?;
            let mut hash = blake3::Hasher::new_derive_key("Akula Node lab document ID v1");
            hash.update(&[0x42; SCOPE_BYTES]);
            hash.update(FILE_NAME.as_bytes());
            let document = *hash.finalize().as_bytes();
            Ok(Self {
                path,
                vault,
                appdata,
                document,
            })
        }

        fn original_unchanged(&self) -> Result<()> {
            let observed = fs::read(self.vault.join(FILE_NAME)).anyerr()?;
            ensure_any!(observed == ORIGINAL, "original document was changed");
            ensure_any!(
                fs::read_dir(&self.vault).anyerr()?.count() == 1,
                "extra source-vault entry created"
            );
            Ok(())
        }
    }

    impl Drop for IsolatedRoots {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct AkulaNode {
        child: Child,
        addr: String,
    }

    impl AkulaNode {
        fn start(roots: &IsolatedRoots, admin: &iroh::PublicKey) -> Result<Self> {
            let binary = std::env::var("AKULA_POLICY_LAB_BIN").anyerr()?;
            let mut child = Command::new(binary)
                .env("AKULA_POLICY_LAB_TOKEN", TEST_TOKEN)
                .arg("--vault-root")
                .arg(&roots.vault)
                .arg("--appdata")
                .arg(&roots.appdata)
                .arg("--vault-id")
                .arg(SCOPE_TEXT)
                .arg("--note")
                .arg(FILE_NAME)
                .arg("--admin-key")
                .arg(hex(admin.as_bytes()))
                .arg("--port")
                .arg("0")
                .arg("--max-requests")
                .arg("80")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .anyerr()?;
            let mut line = String::new();
            let stdout = child.stdout.take().expect("stdout explicitly piped");
            BufReader::new(stdout).read_line(&mut line).anyerr()?;
            let address = line
                .trim()
                .strip_prefix("AKULA_SCOPE_LAB_BOUND=")
                .ok_or_else(|| std::io::Error::other("Akula refused startup"))
                .anyerr()?;
            ensure_any!(
                address.starts_with("127.0.0.1:"),
                "Akula node listening outside localhost"
            );
            Ok(Self {
                child,
                addr: address.to_owned(),
            })
        }

        fn command(&self, request: &str) -> Result<String> {
            let mut stream = TcpStream::connect(&self.addr).anyerr()?;
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .anyerr()?;
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .anyerr()?;
            write!(stream, "AUTH {TEST_TOKEN}\n{request}\n").anyerr()?;
            stream.shutdown(Shutdown::Write).anyerr()?;
            let mut reply = String::new();
            BufReader::new(stream).read_line(&mut reply).anyerr()?;
            ensure_any!(reply.len() <= 256, "unbounded Akula response");
            Ok(reply)
        }
    }

    impl Drop for AkulaNode {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn hex(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 15) as usize] as char);
        }
        out
    }

    #[tokio::test]
    async fn roha_quic_ack_only_after_real_akula_sqlite_acceptance() -> Result<()> {
        // Regular Roha CI runs the five pure transport tests. This separate
        // targeted invocation is mandatory in the cross-repository CI step.
        if std::env::var_os("AKULA_POLICY_LAB_BIN").is_none() {
            return Ok(());
        }
        let first = IsolatedRoots::new(1)?;
        let second = IsolatedRoots::new(2)?;
        let admin = iroh::SecretKey::from_bytes(&[87u8; 32]);
        let admin_public = admin.public();
        ensure_any!(first.document == second.document, "same source has mismatched ID");
        let node_b = AkulaNode::start(&first, &admin_public)?;
        let node_c = AkulaNode::start(&second, &admin_public)?;
        let doc_id = hex(&first.document);
        ensure_any!(
            node_b.command(&format!("READ {doc_id}"))?.contains(r#""status":"allowed""#),
            "B could not read before revocation"
        );
        ensure_any!(
            node_c.command(&format!("READ {doc_id}"))?.contains(r#""status":"allowed""#),
            "C could not read before revocation"
        );

        let a = initiate_from_loopback().await?;
        let b = accept_on_loopback().await?;
        let c = accept_on_loopback().await?;
        let scope = [0x42u8; SCOPE_BYTES];
        let proof = synthetic_signed_frame(scope, first.document, 2, &admin);
        let observed_b = hop(&a, &b, a.id(), scope, &admin_public, &proof, |bytes| {
            let response = node_b.command(&format!("BAN {}", hex(&bytes[SCOPE_BYTES..])))?;
            ensure_any!(
                response.contains(r#""status":"applied""#),
                "Akula B did not commit the signed revocation"
            );
            Ok(())
        })
        .await?;
        ensure_any!(observed_b == proof, "B modified original signed evidence");
        ensure_any!(
            node_b.command(&format!("READ {doc_id}"))?.contains(r#""status":"blocked""#),
            "B still distributes a revoked document"
        );

        let observed_c = hop(&b, &c, b.id(), scope, &admin_public, &observed_b, |bytes| {
            let response = node_c.command(&format!("BAN {}", hex(&bytes[SCOPE_BYTES..])))?;
            ensure_any!(
                response.contains(r#""status":"applied""#),
                "Akula C did not commit the signed revocation"
            );
            Ok(())
        })
        .await?;
        ensure_any!(observed_c == proof, "C modified original signed evidence");
        ensure_any!(
            node_c.command(&format!("READ {doc_id}"))?.contains(r#""status":"blocked""#),
            "C still distributes a revoked document"
        );
        drop(node_c);
        let restarted_c = AkulaNode::start(&second, &admin_public)?;
        ensure_any!(
            restarted_c
                .command(&format!("READ {doc_id}"))?
                .contains(r#""status":"blocked""#),
            "C lost signed revocation after process restart"
        );
        first.original_unchanged()?;
        second.original_unchanged()?;
        a.close().await;
        b.close().await;
        c.close().await;
        Ok(())
    }
}
