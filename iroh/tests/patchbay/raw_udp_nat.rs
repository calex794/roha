//! E03: isolate NAT mapping/replug without any Iroh, Noq, or QUIC endpoint.
//! Two plain UDP reflectors report the source address they actually observe.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use n0_error::{Result, anyerr, ensure_any};
use n0_tracing_test::traced_test;
use patchbay::{FirewallConfigBuilder, Lab, Nat, Router};
use testdir::testdir;
use tokio::{
    net::UdpSocket,
    sync::{mpsc, oneshot},
};
use tracing::info;

const E03_TIMEOUT: Duration = Duration::from_secs(8);
const E03_PEER_PORT: u16 = 43111;
const E03_DISCOVERY_PORT: u16 = 43222;

fn blocked_udp(f: &mut FirewallConfigBuilder) -> &mut FirewallConfigBuilder {
    f.block_inbound().allow_udp(&[53])
}

#[derive(Clone, Copy, Debug)]
enum E03Case {
    NoReplug,
    ReplugStale,
    ReplugDeleteOldPeer,
}

/// Reflect the observed external UDP source address and report it out-of-band.
/// The separate channel distinguishes source mapping from return-packet delivery.
async fn raw_reflector(
    addr: SocketAddr,
    ready: oneshot::Sender<()>,
    reports: mpsc::UnboundedSender<(String, SocketAddr)>,
    mut poke_rx: mpsc::UnboundedReceiver<(SocketAddr, &'static str)>,
) -> Result<()> {
    let socket = UdpSocket::bind(addr).await?;
    let _ = ready.send(());
    let mut buf = [0u8; 256];
    loop {
        tokio::select! {
            command = poke_rx.recv() => {
                let Some((destination, label)) = command else { break; };
                socket.send_to(label.as_bytes(), destination).await?;
            }
            incoming = socket.recv_from(&mut buf) => {
                let (n, source) = incoming?;
                let label = String::from_utf8_lossy(&buf[..n]).into_owned();
                let _ = reports.send((label, source));
                socket.send_to(format!("OBSERVED {source}").as_bytes(), source).await?;
            }
        }
    }
    Ok(())
}

/// Keep a SINGLE wildcard-bound socket alive across interface replug.
async fn raw_udp_sender(
    ready: oneshot::Sender<u16>,
    mut commands: mpsc::UnboundedReceiver<(SocketAddr, &'static str)>,
    incoming: mpsc::UnboundedSender<String>,
) -> Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    let _ = ready.send(socket.local_addr()?.port());
    let mut buf = [0u8; 256];
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some((dst, label)) = command else { break; };
                socket.send_to(label.as_bytes(), dst).await?;
            }
            received = socket.recv_from(&mut buf) => {
                let (n, _) = received?;
                let _ = incoming.send(String::from_utf8_lossy(&buf[..n]).into_owned());
            }
        }
    }
    Ok(())
}

async fn observed(
    tx: &mpsc::UnboundedSender<(SocketAddr, &'static str)>,
    reports: &mut mpsc::UnboundedReceiver<(String, SocketAddr)>,
    target: SocketAddr,
    label: &'static str,
) -> Result<SocketAddr> {
    tx.send((target, label))
        .map_err(|_| anyerr!("E03 sender exited before {label}"))?;
    tokio::time::timeout(E03_TIMEOUT, async {
        loop {
            let (name, src) = reports
                .recv()
                .await
                .ok_or_else(|| anyerr!("E03 reflector exited before {label}"))?;
            if name == label {
                return Ok(src);
            }
        }
    })
    .await
    .map_err(|_| anyerr!("E03 reflector timeout for {label}"))?
}

fn delete_exact_old_peer(
    router: &Router,
    old_ip: Ipv4Addr,
    socket_port: u16,
    peer_addr: SocketAddr,
) -> Result<()> {
    let old_src = old_ip.to_string();
    let old_sport = socket_port.to_string();
    let old_dest = peer_addr.ip().to_string();
    let old_dport = peer_addr.port().to_string();
    let summary = router.run_sync(move || {
        let output = std::process::Command::new("conntrack")
            .args([
                "-D",
                "-p",
                "udp",
                "--orig-src",
                old_src.as_str(),
                "--orig-dst",
                old_dest.as_str(),
                "--orig-port-src",
                old_sport.as_str(),
                "--orig-port-dst",
                old_dport.as_str(),
            ])
            .output()?;
        let result = format!(
            "status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        if !output.status.success() || !result.contains("1 flow entries have been deleted") {
            return Err(std::io::Error::other(format!(
                "E03 expected exactly one old UDP peer tuple deletion: {result}"
            ))
            .into());
        }
        Ok(result)
    })?;
    info!(%summary, "STY_E03_EXACT_OLD_FLOW_DELETED");
    Ok(())
}

async fn run_e03(case: E03Case) -> Result<()> {
    let (lab, guard) = Lab::for_test(testdir!()).await?;
    let initial = lab.add_router("initial").nat(Nat::Moderate).build().await?;
    let blocked = lab
        .add_router("blocked")
        .nat(Nat::Moderate)
        .firewall_custom(blocked_udp)
        .build()
        .await?;
    let peer_router = lab.add_router("peer_reflector").build().await?;
    let discovery_router = lab.add_router("discovery_reflector").build().await?;
    let peer_ip = peer_router
        .uplink_ip()
        .ok_or_else(|| anyerr!("peer WAN missing"))?;
    let discovery_ip = discovery_router
        .uplink_ip()
        .ok_or_else(|| anyerr!("discovery WAN missing"))?;
    let peer_addr = SocketAddr::new(IpAddr::V4(peer_ip), E03_PEER_PORT);
    let discovery_addr = SocketAddr::new(IpAddr::V4(discovery_ip), E03_DISCOVERY_PORT);

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (peer_ready_tx, peer_ready_rx) = oneshot::channel();
    let (discovery_ready_tx, discovery_ready_rx) = oneshot::channel();
    let (peer_poke_tx, peer_poke_rx) = mpsc::unbounded_channel();
    let (_discovery_poke_tx, discovery_poke_rx) = mpsc::unbounded_channel();
    let peer_events = event_tx.clone();
    let peer_task = peer_router.spawn(async move |_| {
        raw_reflector(peer_addr, peer_ready_tx, peer_events, peer_poke_rx).await
    })?;
    let discovery_task = discovery_router.spawn(async move |_| {
        raw_reflector(
            discovery_addr,
            discovery_ready_tx,
            event_tx,
            discovery_poke_rx,
        )
        .await
    })?;
    tokio::time::timeout(E03_TIMEOUT, peer_ready_rx)
        .await
        .map_err(|_| anyerr!("E03 peer reflector startup timeout"))?
        .map_err(|_| anyerr!("E03 peer reflector startup failed"))?;
    tokio::time::timeout(E03_TIMEOUT, discovery_ready_rx)
        .await
        .map_err(|_| anyerr!("E03 discovery reflector startup timeout"))?
        .map_err(|_| anyerr!("E03 discovery reflector startup failed"))?;

    let device = lab
        .add_device("udp_sender")
        .uplink(initial.id())
        .build()
        .await?;
    let initial_private = device
        .ip()
        .ok_or_else(|| anyerr!("E03 initial private IP missing"))?;
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (rx_tx, mut rx_rx) = mpsc::unbounded_channel();
    let (port_tx, port_rx) = oneshot::channel();
    let sender_task = device.spawn(async move |_| {
        raw_udp_sender(port_tx, cmd_rx, rx_tx).await
    })?;
    let local_port = tokio::time::timeout(E03_TIMEOUT, port_rx)
        .await
        .map_err(|_| anyerr!("E03 UDP sender startup timeout"))?
        .map_err(|_| anyerr!("E03 sender closed without port"))?;

    // Establish an OLD flow to the peer and a second discovery destination.
    let old_peer = observed(&cmd_tx, &mut event_rx, peer_addr, "OLD_PEER").await?;
    let old_discovery = observed(&cmd_tx, &mut event_rx, discovery_addr, "OLD_DISCOVERY").await?;
    info!(?case, local_port, %initial_private, %old_peer, %old_discovery, "STY_E03_INITIAL");
    ensure_any!(
        old_peer == old_discovery,
        "E03 initial home NAT mapping must be endpoint-independent"
    );

    if !matches!(case, E03Case::NoReplug) {
        device.iface("eth0").unwrap().replug(blocked.id()).await?;
        device.iface("eth0").unwrap().replug(initial.id()).await?;
    }
    let restored_private = device
        .ip()
        .ok_or_else(|| anyerr!("E03 restored private IP missing"))?;
    if !matches!(case, E03Case::NoReplug) {
        ensure_any!(
            initial_private != restored_private,
            "E03 replug must allocate a different internal IP"
        );
    }
    if matches!(case, E03Case::ReplugDeleteOldPeer) {
        delete_exact_old_peer(&initial, initial_private, local_port, peer_addr)?;
    }

    // Revisit the discovery endpoint FIRST, as Iroh does before peer punches.
    // Out-of-band reports work even if the NAT discards UDP responses.
    let after_discovery =
        observed(&cmd_tx, &mut event_rx, discovery_addr, "AFTER_DISCOVERY").await?;
    let after_peer = observed(&cmd_tx, &mut event_rx, peer_addr, "AFTER_PEER").await?;
    info!(
        ?case,
        %initial_private,
        %restored_private,
        local_port,
        %old_peer,
        %after_discovery,
        %after_peer,
        same_public_mapping = after_discovery == after_peer,
        "STY_E03_MAPPING_COMPARISON"
    );

    // The peer now knows both observed ports. Send from the SAME peer source
    // UDP socket to each public address and observe which probes actually
    // make it back to the sender. This does NOT rely on any QUIC path selector.
    peer_poke_tx
        .send((after_discovery, "PROBE_DISCOVERY_MAPPING"))
        .map_err(|_| anyerr!("E03 peer reflector closed"))?;
    peer_poke_tx
        .send((after_peer, "PROBE_PEER_MAPPING"))
        .map_err(|_| anyerr!("E03 peer reflector closed"))?;
    let mut received_discovery = false;
    let mut received_peer = false;
    let probe_until = tokio::time::sleep(Duration::from_millis(700));
    tokio::pin!(probe_until);
    loop {
        tokio::select! {
            msg = rx_rx.recv() => {
                match msg.as_deref() {
                    Some("PROBE_DISCOVERY_MAPPING") => received_discovery = true,
                    Some("PROBE_PEER_MAPPING") => received_peer = true,
                    None => break,
                    _ => (),
                }
            }
            () = &mut probe_until => break,
        }
    }
    info!(
        ?case,
        local_port,
        %after_discovery,
        %after_peer,
        received_discovery,
        received_peer,
        "STY_E03_INBOUND_PROBE_RESULTS"
    );

    // A descriptive experiment. Do not encode the unproven H13 hypothesis as
    // a required PASS: mapping equality vs divergence is the actual result.
    if matches!(case, E03Case::NoReplug | E03Case::ReplugDeleteOldPeer) {
        ensure_any!(
            received_peer,
            "E03 established peer-specific reverse UDP path did not deliver"
        );
    }
    if matches!(case, E03Case::NoReplug) {
        ensure_any!(
            after_discovery == after_peer && received_discovery,
            "E03 baseline must preserve EIM and return-path delivery"
        );
    }
    drop(cmd_tx);
    drop(peer_poke_tx);
    sender_task.abort();
    peer_task.abort();
    discovery_task.abort();
    guard.ok();
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn sty_e03_raw_udp_baseline_no_replug() -> Result<()> {
    run_e03(E03Case::NoReplug).await
}

#[tokio::test]
#[traced_test]
async fn sty_e03_raw_udp_replug_stale_state() -> Result<()> {
    run_e03(E03Case::ReplugStale).await
}

#[tokio::test]
#[traced_test]
async fn sty_e03_raw_udp_replug_targeted_delete() -> Result<()> {
    run_e03(E03Case::ReplugDeleteOldPeer).await
}
