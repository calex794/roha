//! Real Akula SQLite acceptance, lost acknowledgment and exact-proof retries.
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

use super::*;

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

#[derive(Debug, PartialEq)]
enum Admission {
    Applied,
    Duplicate,
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

    fn command(&self, request: &str) -> Result<Value> {
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
        // Bound allocation before reading, including a missing newline.
        BufReader::new(stream)
            .take(513)
            .read_line(&mut reply)
            .anyerr()?;
        ensure_any!(
            reply.len() <= 512 && reply.ends_with('\n'),
            "invalid bounded Akula response"
        );
        Ok(serde_json::from_str(&reply).anyerr()?)
    }

    fn admit(&self, frame: &[u8]) -> Result<Admission> {
        ensure_any!(frame.len() == FRAME_BYTES, "invalid admission frame");
        let proof = hex(&frame[SCOPE_BYTES..]);
        let reply = self.command(&format!("BAN {proof}"))?;
        let admission = if reply["status"] == "applied" {
            Admission::Applied
        } else {
            ensure_any!(
                reply["status"] == "duplicate",
                "Akula did not accept the exact revocation"
            );
            Admission::Duplicate
        };
        // A lost ACK can be retried after restart. Confirm the exact original
        // evidence, not just any active ban or a newer stale correction.
        let document = hex(&frame[SCOPE_BYTES..SCOPE_BYTES + 32]);
        let stored = self.command(&format!("PROOF {document}"))?;
        ensure_any!(
            stored["status"] == "proof" && stored["proof"] == proof,
            "Akula does not retain the acknowledged proof"
        );
        Ok(admission)
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
#[ignore = "requires AKULA_POLICY_LAB_BIN from the dedicated cross-repository job"]
async fn roha_quic_ack_only_after_real_akula_sqlite_acceptance() -> Result<()> {
    // Explicit --ignored invocation must fail if the binary is absent.
    // Ordinary transport CI reports this test as ignored, never as passed.
    let first = IsolatedRoots::new(1)?;
    let second = IsolatedRoots::new(2)?;
    let admin = iroh::SecretKey::from_bytes(&[87u8; 32]);
    let admin_public = admin.public();
    ensure_any!(
        first.document == second.document,
        "same source has mismatched ID"
    );
    let node_b = AkulaNode::start(&first, &admin_public)?;
    let node_c = AkulaNode::start(&second, &admin_public)?;
    let doc_id = hex(&first.document);
    for node in [&node_b, &node_c] {
        ensure_any!(
            node.command(&format!("READ {doc_id}"))?["status"] == "allowed",
            "document unreadable before revocation"
        );
    }

    let a = initiate_from_loopback().await?;
    let b = accept_on_loopback().await?;
    let c = accept_on_loopback().await?;
    let scope = [0x42u8; SCOPE_BYTES];
    let proof = synthetic_signed_frame(scope, first.document, 2, &admin);
    let mut committed_before_disconnect = false;
    let interrupted = hop(&a, &b, a.id(), scope, &admin_public, &proof, |bytes| {
        ensure_any!(
            node_b.admit(bytes)? == Admission::Applied,
            "first delivery was not newly persisted"
        );
        committed_before_disconnect = true;
        // Inject the disconnect window after COMMIT, before QUIC OK.
        ensure_any!(false, "injected loss of acknowledgment after commit");
        Ok(())
    })
    .await;
    ensure_any!(
        committed_before_disconnect && interrupted.is_err(),
        "lost-ACK injection did not exercise committed state"
    );
    drop(node_b);
    let restarted_b = AkulaNode::start(&first, &admin_public)?;
    ensure_any!(
        restarted_b.command(&format!("READ {doc_id}"))?["status"] == "blocked",
        "B lost the committed ban before retry"
    );
    let observed_b = hop(&a, &b, a.id(), scope, &admin_public, &proof, |bytes| {
        ensure_any!(
            restarted_b.admit(bytes)? == Admission::Duplicate,
            "retry was not recognized from persisted evidence"
        );
        Ok(())
    })
    .await?;
    ensure_any!(observed_b == proof, "B modified original signed evidence");

    let observed_c = hop(&b, &c, b.id(), scope, &admin_public, &observed_b, |bytes| {
        ensure_any!(
            node_c.admit(bytes)? == Admission::Applied,
            "C did not persist the forwarded revocation"
        );
        Ok(())
    })
    .await?;
    ensure_any!(observed_c == proof, "C modified original signed evidence");
    drop(node_c);
    let restarted_c = AkulaNode::start(&second, &admin_public)?;
    ensure_any!(
        restarted_c.command(&format!("READ {doc_id}"))?["status"] == "blocked",
        "C lost signed revocation after process restart"
    );
    let retried_c = hop(&b, &c, b.id(), scope, &admin_public, &observed_c, |bytes| {
        ensure_any!(
            restarted_c.admit(bytes)? == Admission::Duplicate,
            "C did not recognize the retry after restart"
        );
        Ok(())
    })
    .await?;
    ensure_any!(retried_c == proof, "retry changed original evidence");

    // Authentic but older proof must not be acknowledged as this exact proof.
    let stale = synthetic_signed_frame(scope, first.document, 1, &admin);
    let stale_attempt = hop(&b, &c, b.id(), scope, &admin_public, &stale, |bytes| {
        restarted_c.admit(bytes)?;
        Ok(())
    })
    .await;
    ensure_any!(stale_attempt.is_err(), "stale proof was acknowledged");

    // Valid transport/signature is insufficient: the real Akula catalog owns
    // admission and refuses documents outside its operator-selected entries.
    let unknown = synthetic_signed_frame(scope, [0x99; 32], 3, &admin);
    let unknown_attempt = hop(&b, &c, b.id(), scope, &admin_public, &unknown, |bytes| {
        restarted_c.admit(bytes)?;
        Ok(())
    })
    .await;
    ensure_any!(unknown_attempt.is_err(), "unknown document was acknowledged");
    let retained = restarted_c.command(&format!("PROOF {doc_id}"))?;
    ensure_any!(retained["proof"] == hex(&proof[SCOPE_BYTES..]), "proof changed");
    ensure_any!(
        restarted_c.command(&format!("READ {doc_id}"))?["status"] == "blocked",
        "rejected retries reopened the document"
    );
    first.original_unchanged()?;
    second.original_unchanged()?;
    a.close().await;
    b.close().await;
    c.close().await;
    Ok(())
}
