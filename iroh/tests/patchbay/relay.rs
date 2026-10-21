//! Relay connectivity and reconnect tests.
//!
//! The relay from [`util::relay::run_relay_server`] binds `[::]` and is
//! reachable as `https://relay.test` through lab-wide DNS A and AAAA records.
//!
//! The connect tests place both peers on access networks that support only
//! one IP family, so all traffic to the relay uses that family. The
//! reconnect tests pin the connection to the relay by putting both peers
//! behind symmetric (`Nat::Strict`) NATs, for which holepunching fails
//! even though IP transports stay enabled, and then break either one
//! peer's access link or the relay server itself.

use std::time::Duration;

use iroh::endpoint::{Connection, Side};
use n0_error::{Result, StackResultExt, StdResultExt, anyerr, ensure_any};
use n0_future::task::AbortOnDropHandle;
use n0_tracing_test::traced_test;
use patchbay::{FirewallConfigBuilder, IpSupport, Lab, Nat, OutDir};
use testdir::testdir;
use tokio::sync::oneshot;
use tracing::info;

use super::util::{
    self, Pair, PathConnectionExt, is_relayed, lab_with_relay, ping_accept, ping_open,
};

/// Firewall rules that block all outbound UDP except DNS, leaving TCP open.
///
/// The lab DNS server is UDP-only, so port 53 must stay open. Everything else
/// over UDP (QAD, holepunch probes) is dropped, forcing traffic onto the relay
/// over TCP. Pass to [`RouterBuilder::firewall_custom`](patchbay::RouterBuilder).
fn block_udp_except_dns(f: &mut FirewallConfigBuilder) -> &mut FirewallConfigBuilder {
    f.block_inbound().allow_udp(&[53])
}

/// Connects two peers through the relay over a single IP family.
///
/// The relay's own network is dual-stack; both access routers support only
/// the family under test. Their firewall drops all UDP except DNS, so QAD
/// and holepunching are impossible and the relay is the only usable path:
/// the ping exchange proves the relay carries data over that family.
/// Direct-path coverage per family lives in the switch_uplink and nat
/// matrices.
async fn run_relay_connect(ip_support: IpSupport) -> Result {
    let (lab, relay_map, _relay_guard, guard) = lab_with_relay(testdir!()).await?;
    let access1 = lab
        .add_router("access1")
        .ip_support(ip_support)
        .firewall_custom(block_udp_except_dns)
        .build()
        .await?;
    let access2 = lab
        .add_router("access2")
        .ip_support(ip_support)
        .firewall_custom(block_udp_except_dns)
        .build()
        .await?;
    let server = lab
        .add_device("server")
        .uplink(access1.id())
        .build()
        .await?;
    let client = lab
        .add_device("client")
        .uplink(access2.id())
        .build()
        .await?;
    // Sanity-check the harness: only the family under test is assigned.
    for dev in [&server, &client] {
        assert_eq!(dev.ip().is_some(), ip_support.has_v4(), "IPv4 assignment");
        assert_eq!(dev.ip6().is_some(), ip_support.has_v6(), "IPv6 assignment");
    }
    let timeout = Duration::from_secs(10);
    Pair::new(relay_map)
        .server(server, async move |_dev, _ep, conn| {
            assert!(is_relayed(&conn), "connection started relayed");
            ping_accept(&conn, timeout).await.context("ping_accept")?;
            assert!(is_relayed(&conn), "still relayed with UDP blocked");
            conn.closed().await;
            Ok(())
        })
        .client(client, async move |_dev, _ep, conn| {
            assert!(is_relayed(&conn), "connection started relayed");
            ping_open(&conn, timeout).await.context("ping_open")?;
            assert!(is_relayed(&conn), "still relayed with UDP blocked");
            conn.close(0u32.into(), b"bye");
            Ok(())
        })
        .run()
        .await?;
    guard.ok();
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn relay_connect_ipv4() -> Result {
    run_relay_connect(IpSupport::V4Only).await
}

#[tokio::test]
#[traced_test]
async fn relay_connect_ipv6() -> Result {
    run_relay_connect(IpSupport::V6Only).await
}

/// Which peers sit behind a UDP-blocking access router.
#[derive(Debug, Clone, Copy)]
enum UdpBlocked {
    Both,
    ServerOnly,
    ClientOnly,
}

/// Connects two peers of which one or both cannot use UDP beyond DNS.
///
/// The blocked peers sit behind a home NAT that drops unsolicited inbound and
/// all outbound UDP except DNS (the lab DNS server is UDP-only), the hotel or
/// airport guest WiFi shape. That rules out QAD and holepunching, so they must
/// reach each other through the relay over TCP. Any unblocked peer is behind a
/// plain home NAT; a direct path needs UDP on both ends, so the connection
/// stays relayed in every variant.
async fn run_relay_udp_blocked(blocked: UdpBlocked) -> Result {
    let (lab, relay_map, _relay_guard, guard) = lab_with_relay(testdir!()).await?;
    let server_blocked = matches!(blocked, UdpBlocked::Both | UdpBlocked::ServerOnly);
    let client_blocked = matches!(blocked, UdpBlocked::Both | UdpBlocked::ClientOnly);
    let mut net1 = lab.add_router("net1").nat(Nat::Moderate);
    if server_blocked {
        net1 = net1.firewall_custom(block_udp_except_dns);
    }
    let net1 = net1.build().await?;
    let mut net2 = lab.add_router("net2").nat(Nat::Moderate);
    if client_blocked {
        net2 = net2.firewall_custom(block_udp_except_dns);
    }
    let net2 = net2.build().await?;
    let server = lab.add_device("server").uplink(net1.id()).build().await?;
    let client = lab.add_device("client").uplink(net2.id()).build().await?;

    // Long enough for a holepunch to complete if one were possible.
    let hold = Duration::from_secs(3);
    let timeout = Duration::from_secs(15);
    Pair::new(relay_map)
        .server(server, async move |_dev, _ep, conn| {
            assert!(is_relayed(&conn), "connection started relayed");
            ping_accept(&conn, timeout).await.context("ping 1")?;
            tokio::time::sleep(hold).await;
            assert!(is_relayed(&conn), "still relayed with UDP blocked");
            ping_accept(&conn, timeout).await.context("ping 2")?;
            conn.closed().await;
            Ok(())
        })
        .client(client, async move |_dev, _ep, conn| {
            assert!(is_relayed(&conn), "connection started relayed");
            ping_open(&conn, timeout).await.context("ping 1")?;
            tokio::time::sleep(hold).await;
            assert!(is_relayed(&conn), "still relayed with UDP blocked");
            ping_open(&conn, timeout).await.context("ping 2")?;
            conn.close(0u32.into(), b"bye");
            Ok(())
        })
        .run()
        .await?;
    guard.ok();
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn relay_udp_blocked_both() -> Result {
    run_relay_udp_blocked(UdpBlocked::Both).await
}

#[tokio::test]
#[traced_test]
async fn relay_udp_blocked_server() -> Result {
    run_relay_udp_blocked(UdpBlocked::ServerOnly).await
}

#[tokio::test]
#[traced_test]
async fn relay_udp_blocked_client() -> Result {
    run_relay_udp_blocked(UdpBlocked::ClientOnly).await
}

/// Takes one peer's link down and verifies the relay path recovers.
///
/// Both peers sit behind symmetric NATs, so the connection cannot escape to
/// a direct path; the affected peer's relay client notices the dead link and
/// reconnects once the link is back.
async fn run_relay_reconnect_link_outage(outage_side: Side, downtime: Duration) -> Result {
    let (lab, relay_map, _relay_guard, guard) = lab_with_relay(testdir!()).await?;
    let nat1 = lab.add_router("nat1").nat(Nat::Strict).build().await?;
    let nat2 = lab.add_router("nat2").nat(Nat::Strict).build().await?;
    let outage = lab.add_device("outage").uplink(nat1.id()).build().await?;
    let peer = lab.add_device("peer").uplink(nat2.id()).build().await?;
    let timeout = Duration::from_secs(15);
    // The reconnect backoff can sleep through the link returning: dials during
    // the outage fail fast and grow the backoff to several seconds plus jitter,
    // so the post-outage pings need generous headroom.
    let recovery_timeout = Duration::from_secs(30);
    Pair::new(relay_map)
        .left(outage_side, outage, async move |dev, _ep, conn| {
            assert!(is_relayed(&conn), "connection started relayed");
            ping_open(&conn, timeout)
                .await
                .context("ping before outage")?;
            info!("killing link for {downtime:?}");
            dev.iface("eth0").unwrap().link_down().await?;
            tokio::time::sleep(downtime).await;
            dev.iface("eth0").unwrap().link_up().await?;
            info!("link restored, waiting for relay reconnect");
            ping_open(&conn, recovery_timeout)
                .await
                .context("ping after link restored")?;
            assert!(is_relayed(&conn), "still relayed behind symmetric NAT");
            conn.close(0u32.into(), b"bye");
            Ok(())
        })
        .right(peer, async move |_dev, _ep, conn| {
            assert!(is_relayed(&conn), "connection started relayed");
            ping_accept(&conn, timeout)
                .await
                .context("ping before outage")?;
            ping_accept(&conn, recovery_timeout)
                .await
                .context("ping after link restored")?;
            conn.closed().await;
            Ok(())
        })
        .run()
        .await?;
    guard.ok();
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn relay_reconnect_link_outage_client() -> Result {
    run_relay_reconnect_link_outage(Side::Client, Duration::from_secs(5)).await
}

#[tokio::test]
#[traced_test]
async fn relay_reconnect_link_outage_server() -> Result {
    run_relay_reconnect_link_outage(Side::Server, Duration::from_secs(5)).await
}

/// Restarts the relay server underneath an established relay-pinned
/// connection and verifies data flows again once the clients reconnect.
///
/// The replacement relay binds the same ports and is reachable under the
/// same `https://relay.test` URL, like a relay deployment restarting in
/// place. Both peers' relay actors must notice the dead connection and
/// reconnect on their own.
#[tokio::test]
#[traced_test]
async fn relay_reconnect_relay_restart() -> Result {
    let mut builder = Lab::builder().outdir(OutDir::Exact(testdir!()));
    if let Some(name) = std::thread::current().name() {
        builder = builder.label(name);
    }
    let lab = builder.build().await?;
    let guard = lab.test_guard();

    let dc = lab
        .add_router("dc")
        .ip_support(IpSupport::DualStack)
        .build()
        .await?;
    let dev_relay = lab.add_device("relay").uplink(dc.id()).build().await?;
    // Register DNS before the endpoint devices below; only later devices
    // resolve it.
    let dns = lab.dns_server()?;
    dns.set_host("relay.test", dev_relay.ip().expect("relay has IPv4").into())?;
    dns.set_host(
        "relay.test",
        dev_relay.ip6().expect("relay has IPv6").into(),
    )?;

    let (map_tx, map_rx) = oneshot::channel();
    let (restart_tx, restart_rx) = oneshot::channel::<()>();
    let (restarted_tx, restarted_rx) = oneshot::channel::<()>();
    let relay_task = dev_relay.spawn(async move |_dev| {
        let (relay_map, server) = util::relay::run_relay_server().await.expect("relay spawn");
        map_tx.send(relay_map).expect("test task alive");
        restart_rx.await.expect("restart signal");
        server.shutdown().await.expect("relay shutdown");
        info!("relay stopped, starting a new relay on the same ports");
        // The old server's sockets are released asynchronously after
        // shutdown() returns; retry briefly if a port is still taken.
        let mut attempts = 0;
        let (_relay_map, _server) = loop {
            match util::relay::run_relay_server().await {
                Ok(relay) => break relay,
                Err(err) if attempts < 20 => {
                    attempts += 1;
                    info!("relay respawn attempt {attempts} failed: {err:#}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(err) => panic!("relay respawn: {err:#}"),
            }
        };
        restarted_tx.send(()).expect("test task alive");
        std::future::pending::<()>().await;
    })?;
    let _relay_guard = AbortOnDropHandle::new(relay_task);
    let relay_map = map_rx.await.std_context("relay task died before binding")?;

    let nat1 = lab.add_router("nat1").nat(Nat::Strict).build().await?;
    let nat2 = lab.add_router("nat2").nat(Nat::Strict).build().await?;
    let server = lab.add_device("server").uplink(nat1.id()).build().await?;
    let client = lab.add_device("client").uplink(nat2.id()).build().await?;

    let timeout = Duration::from_secs(20);
    Pair::new(relay_map)
        .server(server, async move |_dev, _ep, conn| {
            assert!(is_relayed(&conn), "connection started relayed");
            ping_accept(&conn, timeout)
                .await
                .context("ping before restart")?;
            ping_accept(&conn, timeout)
                .await
                .context("ping after restart")?;
            conn.closed().await;
            Ok(())
        })
        .client(client, async move |_dev, _ep, conn| {
            assert!(is_relayed(&conn), "connection started relayed");
            ping_open(&conn, timeout)
                .await
                .context("ping before restart")?;
            restart_tx
                .send(())
                .map_err(|_| anyerr!("relay task died"))?;
            restarted_rx.await.std_context("relay did not restart")?;
            info!("relay restarted, waiting for reconnect");
            ping_open(&conn, timeout)
                .await
                .context("ping after relay restart")?;
            assert!(is_relayed(&conn), "still relayed behind symmetric NAT");
            conn.close(0u32.into(), b"bye");
            Ok(())
        })
        .run()
        .await?;
    guard.ok();
    Ok(())
}

/// A bounded deterministic network fixture, *not* a file in an Akula Vault.
/// Sending and checking the payload on each selected path checks more than
/// an eight-byte ping: the QUIC stream and its bytes survive route changes.
const STY_PATH_FIXTURE_SIZE: usize = 256 * 1024;

fn sty_path_fixture() -> Vec<u8> {
    (0..STY_PATH_FIXTURE_SIZE)
        .map(|offset| {
            (offset as u8)
                .wrapping_mul(37)
                .wrapping_add((offset / 1024) as u8)
        })
        .collect()
}

async fn sty_fixture_open(conn: &Connection, timeout: Duration) -> Result {
    tokio::time::timeout(timeout, async {
        let source = sty_path_fixture();
        let expected = blake3::hash(&source);
        let (mut send, mut recv) = conn.open_bi().await.anyerr()?;
        send.write_all(&source).await.anyerr()?;
        send.finish().anyerr()?;
        let receipt = recv.read_to_end(32).await.anyerr()?;
        ensure_any!(
            receipt.as_slice() == expected.as_bytes(),
            "STY fixture: invalid BLAKE3 receipt"
        );
        Ok(())
    })
    .await
    .with_std_context(|_| format!("STY fixture sender timed out after {timeout:?}"))?
}

async fn sty_fixture_accept(conn: &Connection, timeout: Duration) -> Result {
    tokio::time::timeout(timeout, async {
        let (mut send, mut recv) = conn.accept_bi().await.anyerr()?;
        let received = recv.read_to_end(STY_PATH_FIXTURE_SIZE).await.anyerr()?;
        ensure_any!(
            received == sty_path_fixture(),
            "STY fixture: bounded bytes do not match source"
        );
        send.write_all(blake3::hash(&received).as_bytes())
            .await
            .anyerr()?;
        send.finish().anyerr()?;
        Ok(())
    })
    .await
    .with_std_context(|_| format!("STY fixture receiver timed out after {timeout:?}"))?
}

/// Read-only netfilter snapshot from the actual Patchbay router namespace.
/// Diagnostic-only: never clear NAT here or use host conntrack as a substitute.
/// The map and conntrack are distinct state stores, so capture BOTH.
fn sty_nat_state_snapshot(router: &patchbay::Router, stage: &'static str) {
    let snapshot = router.run_sync(|| {
        let mut report = String::new();
        for (label, program, args) in [
            (
                "conntrack_udp",
                "conntrack",
                &["-L", "-p", "udp", "-o", "extended"][..],
            ),
            (
                "nft_fullcone",
                "nft",
                &["-a", "list", "map", "ip", "nat", "fullcone"][..],
            ),
        ] {
            match std::process::Command::new(program).args(args).output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    report.push_str(&format!(
                        "{label}: exit={}; stdout={} stderr={}\n",
                        output.status,
                        stdout.chars().take(12_000).collect::<String>(),
                        stderr.chars().take(1_500).collect::<String>(),
                    ));
                }
                Err(err) => {
                    report.push_str(&format!("{label}: exec_error={err}\n"));
                }
            }
        }
        Ok(report)
    });
    match snapshot {
        Ok(report) => info!(stage, nat_state = %report, "STY_NAT_SNAPSHOT"),
        Err(err) => tracing::warn!(stage, error = %err, "STY_NAT_SNAPSHOT_FAILED"),
    }
}

/// STY-RN-5: change an established direct path into a relay-only path without
/// closing the QUIC connection, then restore a direct path on the same session.
///
/// This is a *transport-only* regression: it does not move user files or grant
/// any FsStore/File Gate authority. The access router becomes unable to use UDP
/// except for DNS, while its TCP relay access remains available.
async fn run_direct_relay_direct(
    replug_side: Side,
    probe_after_replug: bool,
    notify_peer: bool,
    fresh_connection_control: bool,
    flush_nat_after_restore: bool,
    notify_replugged_local: bool,
    delete_only_stale_peer_flow: bool,
) -> Result {
    let (lab, relay_map, _relay_guard, guard) = lab_with_relay(testdir!()).await?;
    let initial = lab.add_router("initial").nat(Nat::Moderate).build().await?;
    let blocked = lab
        .add_router("blocked")
        .nat(Nat::Moderate)
        .firewall_custom(block_udp_except_dns)
        .build()
        .await?;
    let peer_net = lab
        .add_router("peer_net")
        .nat(Nat::Moderate)
        .build()
        .await?;
    let peer_wan_ip = peer_net
        .uplink_ip()
        .ok_or_else(|| anyerr!("STY targeted flow: peer router has no WAN IPv4"))?;
    let replug = lab
        .add_device("replug")
        .uplink(initial.id())
        .build()
        .await?;
    let peer = lab.add_device("peer").uplink(peer_net.id()).build().await?;

    let setup = Duration::from_secs(20);
    let recovery = Duration::from_secs(45);
    let (restored_tx, restored_rx) = oneshot::channel::<()>();
    let (fresh_addr_tx, fresh_addr_rx) = oneshot::channel::<iroh::EndpointAddr>();
    Pair::new(relay_map)
        .left(replug_side, replug, async move |dev, ep, conn| {
            let initial_device_ip = dev
                .ip()
                .ok_or_else(|| anyerr!("STY targeted flow: initial device has no IPv4"))?;
            conn.wait_ip(setup)
                .await
                .context("STY direct: first direct path")?;
            if notify_replugged_local || delete_only_stale_peer_flow {
                sty_nat_state_snapshot(&initial, "initial_direct");
            }
            ping_accept(&conn, setup)
                .await
                .context("STY direct: first ping")?;

            info!("STY_PATH_DIRECT_TO_RELAY: replugging into UDP-blocked access");
            dev.iface("eth0").unwrap().replug(blocked.id()).await?;
            if notify_replugged_local || delete_only_stale_peer_flow {
                sty_nat_state_snapshot(&initial, "after_leaving_initial");
            }
            info!(advertised = ?ep.addr(), "STY address after UDP-blocked replug");
            // Drive the same connection while UDP to the peer is unavailable.
            ping_accept(&conn, recovery)
                .await
                .context("STY relay: ping over fallback")?;
            conn.wait_selected(recovery, |p| p.is_relay())
                .await
                .context("STY relay: relay path must be selected")?;
            info!("STY_PATH_RELAY_SELECTED");
            // Keep the blocked link active until the peer has observed it too.
            ping_accept(&conn, recovery)
                .await
                .context("STY relay: observation barrier")?;
            sty_fixture_accept(&conn, recovery)
                .await
                .context("STY relay: byte-for-byte payload")?;

            dev.iface("eth0").unwrap().replug(initial.id()).await?;
            if notify_replugged_local || delete_only_stale_peer_flow {
                sty_nat_state_snapshot(&initial, "restored_before_any_flush");
            }
            if delete_only_stale_peer_flow {
                // Delete only the former device's UDP flow to the remote peer WAN.
                // This is an isolated Patchbay router namespace. Do not use
                // conntrack -D against a customer's production router.
                let old_src = initial_device_ip.to_string();
                let peer_dst = peer_wan_ip.to_string();
                let diagnostic = initial.run_sync(move || {
                    let output = std::process::Command::new("conntrack")
                        .args([
                            "-D",
                            "-p",
                            "udp",
                            "--orig-src",
                            old_src.as_str(),
                            "--orig-dst",
                            peer_dst.as_str(),
                        ])
                        .output()?;
                    if !output.status.success() {
                        return Err(std::io::Error::other(format!(
                            "targeted conntrack -D failed: exit={} stdout={} stderr={}",
                            output.status,
                            String::from_utf8_lossy(&output.stdout),
                            String::from_utf8_lossy(&output.stderr),
                        ))
                        .into());
                    }
                    Ok(String::from_utf8_lossy(&output.stderr).into_owned())
                })?;
                info!(%diagnostic, "STY_TARGETED_OLD_PEER_CONNTRACK_DELETED");
            }
            if flush_nat_after_restore {
                // An isolation control on Patchbay's simulated router, not a
                // production recommendation to erase customer conntrack state.
                initial
                    .flush_nat_state()
                    .await
                    .context("STY same-connection: flush restored simulated NAT state")?;
                info!("STY_SAME_CONNECTION_NAT_FLUSH");
            }
            if notify_replugged_local {
                // Separate detection/refresh from stale NAT state.  This hint
                // is sent to the endpoint whose network actually changed.
                ep.network_change().await;
                info!("STY_SAME_CONNECTION_LOCAL_NETWORK_HINT");
            }
            if notify_replugged_local || delete_only_stale_peer_flow {
                sty_nat_state_snapshot(&initial, "restored_after_action");
            }
            info!(advertised = ?ep.addr(), "STY address immediately after restoring network");
            if notify_peer {
                let _ = restored_tx.send(());
            }
            if fresh_connection_control {
                // Control: a new QUIC connection on the *same endpoints* after
                // a server-side move. Distinguishes NAT reachability from stale
                // state of the original multipath connection.
                fresh_addr_tx
                    .send(ep.addr())
                    .map_err(|_| anyerr!("STY fresh control: peer closed address channel"))?;
                let incoming = tokio::time::timeout(recovery, ep.accept())
                    .await
                    .std_context("STY fresh control: server accept timed out")?
                    .context("STY fresh control: endpoint closed")?;
                let fresh = incoming
                    .accept()
                    .anyerr()?
                    .await
                    .context("STY fresh control: server handshake")?;
                let direct = fresh.wait_ip(Duration::from_secs(12)).await;
                info!(
                    selected_direct = direct.is_ok(),
                    fresh_paths = ?fresh
                        .paths()
                        .iter()
                        .map(|p| (p.remote_addr().to_string(), p.is_selected()))
                        .collect::<Vec<_>>(),
                    "STY fresh-control server result"
                );
                ping_accept(&fresh, recovery)
                    .await
                    .context("STY fresh control: server ping")?;
                fresh.closed().await;
            }
            if probe_after_replug {
                // Diagnose whether resumed application traffic stimulates path recovery.
                // Keep the original idle recovery test as a separate, mandatory case.
                ping_accept(&conn, recovery)
                    .await
                    .context("STY diagnostic: ping after restoring network")?;
                sty_fixture_accept(&conn, recovery)
                    .await
                    .context("STY diagnostic: checked payload after restoring network")?;
            }
            let result = conn.wait_ip(recovery).await;
            if notify_replugged_local || delete_only_stale_peer_flow {
                sty_nat_state_snapshot(&initial, "after_recovery_observation");
            }
            let paths = conn
                .paths()
                .iter()
                .map(|p| (p.remote_addr().to_string(), p.is_selected(), p.is_ip()))
                .collect::<Vec<_>>();
            info!(
                ?paths,
                advertised = ?ep.addr(),
                probe_after_replug,
                notify_peer,
                success = result.is_ok(),
                "STY server recovery paths"
            );
            result.context("STY direct: path must recover after replug")?;
            info!("STY_PATH_DIRECT_RECOVERED");
            ping_accept(&conn, recovery)
                .await
                .context("STY direct: final ping")?;
            sty_fixture_accept(&conn, recovery)
                .await
                .context("STY direct: recovered path payload")?;
            conn.closed().await;
            Ok(())
        })
        .right(peer, async move |_dev, ep, conn| {
            conn.wait_ip(setup)
                .await
                .context("STY direct: peer initial path")?;
            ping_open(&conn, setup)
                .await
                .context("STY direct: first ping")?;
            ping_open(&conn, recovery)
                .await
                .context("STY relay: fallback ping")?;
            conn.wait_selected(recovery, |p| p.is_relay())
                .await
                .context("STY relay: peer must select relay")?;
            ping_open(&conn, recovery)
                .await
                .context("STY relay: observation barrier")?;
            sty_fixture_open(&conn, recovery)
                .await
                .context("STY relay: byte-for-byte payload")?;
            if notify_peer {
                restored_rx
                    .await
                    .std_context("STY restored signal sender dropped")?;
                ep.network_change().await;
                info!("STY diagnostic: passive peer network-change hint sent");
            }
            if fresh_connection_control {
                let addr = fresh_addr_rx
                    .await
                    .std_context("STY fresh control: address channel dropped")?;
                let fresh = tokio::time::timeout(recovery, ep.connect(addr, b"test"))
                    .await
                    .std_context("STY fresh control: connection timed out")?
                    .context("STY fresh control: connection failed")?;
                let direct = fresh.wait_ip(Duration::from_secs(12)).await;
                info!(
                    selected_direct = direct.is_ok(),
                    fresh_paths = ?fresh
                        .paths()
                        .iter()
                        .map(|p| (p.remote_addr().to_string(), p.is_selected()))
                        .collect::<Vec<_>>(),
                    "STY fresh-control client result"
                );
                ping_open(&fresh, recovery)
                    .await
                    .context("STY fresh control: client ping")?;
                fresh.close(0u32.into(), b"control-complete");
            }
            if probe_after_replug {
                ping_open(&conn, recovery)
                    .await
                    .context("STY diagnostic: peer ping after network restoration")?;
                sty_fixture_open(&conn, recovery)
                    .await
                    .context("STY diagnostic: peer payload after network restoration")?;
            }
            let result = conn.wait_ip(recovery).await;
            let paths = conn
                .paths()
                .iter()
                .map(|p| (p.remote_addr().to_string(), p.is_selected(), p.is_ip()))
                .collect::<Vec<_>>();
            info!(
                ?paths,
                probe_after_replug,
                notify_peer,
                success = result.is_ok(),
                "STY peer recovery paths"
            );
            result.context("STY direct: peer must recover")?;
            ping_open(&conn, recovery)
                .await
                .context("STY direct: final ping")?;
            sty_fixture_open(&conn, recovery)
                .await
                .context("STY direct: recovered path payload")?;
            conn.close(0u32.into(), b"bye");
            Ok(())
        })
        .run()
        .await?;
    guard.ok();
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_client_replug() -> Result {
    run_direct_relay_direct(Side::Client, false, false, false, false, false, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_replug() -> Result {
    run_direct_relay_direct(Side::Server, false, false, false, false, false, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_with_traffic_probe() -> Result {
    run_direct_relay_direct(Side::Server, true, false, false, false, false, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_with_peer_network_hint() -> Result {
    run_direct_relay_direct(Side::Server, false, true, false, false, false, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_fresh_connection_control() -> Result {
    run_direct_relay_direct(Side::Server, false, false, true, false, false, false).await
}

// Separate from the nine historical A/B scenarios, which all retain their
// original call parameters.  These controls ask whether the EXISTING QUIC
// connection can regain direct routing without recreating an Endpoint/socket.
#[tokio::test]
#[traced_test]
async fn sty_nat_restore_same_connection_flush_only() -> Result {
    run_direct_relay_direct(Side::Server, false, false, false, true, false, false).await
}

#[tokio::test]
#[traced_test]
async fn sty_nat_restore_same_connection_local_network_hint_only() -> Result {
    run_direct_relay_direct(Side::Server, false, false, false, false, true, false).await
}

#[tokio::test]
#[traced_test]
async fn sty_nat_restore_same_connection_flush_and_local_network_hint() -> Result {
    run_direct_relay_direct(Side::Server, false, false, false, true, true, false).await
}

// Targeted experiment: delete ONLY the old source's direct-peer conntrack
// entry, leaving DNS/relay conntrack and the nft fullcone map untouched.
#[tokio::test]
#[traced_test]
async fn sty_nat_restore_same_connection_targeted_old_peer_flow() -> Result {
    run_direct_relay_direct(Side::Server, false, false, false, false, false, true).await
}

/// STY-RN-5 isolation control: after the *server* moves through an UDP-blocked
/// network and back, start two NEW devices with NEW QUIC endpoints and UDP
/// sockets behind the original routers.
///
/// A direct path on this fresh pair, contrasted with the established session's
/// failure to recover, points to state retained by the original endpoint or NAT
/// mapping rather than an intrinsically unpunchable restored topology. If this
/// fresh pair is also relayed, inspect the emulated NAT/firewall rules before
/// changing production routing. This intentionally uses no Akula user files.
async fn run_server_replug_new_endpoint_control(
    reuse_device: bool,
    reuse_port: bool,
    flush_nat: bool,
) -> Result {
    let (lab, relay_map, _relay_guard, guard) = lab_with_relay(testdir!()).await?;
    let initial_router = lab.add_router("initial").nat(Nat::Moderate).build().await?;
    let initial_id = initial_router.id();
    let blocked_id = lab
        .add_router("blocked")
        .nat(Nat::Moderate)
        .firewall_custom(block_udp_except_dns)
        .build()
        .await?
        .id();
    let peer_id = lab
        .add_router("peer_net")
        .nat(Nat::Moderate)
        .build()
        .await?
        .id();
    let old_server = lab
        .add_device("old_server")
        .uplink(initial_id)
        .build()
        .await?;
    let old_client = lab.add_device("old_client").uplink(peer_id).build().await?;
    // Patchbay Device is a cloneable handle; keep the same network namespace
    // for the endpoint-only control, versus fresh devices in the baseline.
    let reusable_server = old_server.clone();
    let reusable_client = old_client.clone();
    let (restored_tx, restored_rx) = oneshot::channel::<()>();
    let (server_port_tx, server_port_rx) = oneshot::channel::<u16>();
    let (client_port_tx, client_port_rx) = oneshot::channel::<u16>();
    let setup = Duration::from_secs(20);
    let recovery = Duration::from_secs(45);

    // First establish direct connectivity, switch to our own relay, and
    // restore the server's original uplink on the *old* pair.
    Pair::new(relay_map.clone())
        .server(old_server, async move |dev, ep, conn| {
            let port = ep
                .bound_sockets()
                .iter()
                .find(|addr| addr.is_ipv4())
                .context("STY original server IPv4 socket missing")?
                .port();
            server_port_tx
                .send(port)
                .map_err(|_| anyerr!("STY original server port receiver dropped"))?;
            conn.wait_ip(setup)
                .await
                .context("old server initially direct")?;
            ping_accept(&conn, setup)
                .await
                .context("old server initial ping")?;
            dev.iface("eth0").unwrap().replug(blocked_id).await?;
            ping_accept(&conn, recovery)
                .await
                .context("old server fallback ping")?;
            conn.wait_selected(recovery, |p| p.is_relay())
                .await
                .context("old server fallback must select relay")?;
            sty_fixture_accept(&conn, recovery)
                .await
                .context("old server checked relay bytes")?;
            dev.iface("eth0").unwrap().replug(initial_id).await?;
            restored_tx
                .send(())
                .map_err(|_| anyerr!("fresh nodes control: restoration notification lost"))?;
            ping_accept(&conn, recovery)
                .await
                .context("old server relay still carries traffic after restoration")?;
            info!(
                old_paths = ?conn
                    .paths()
                    .iter()
                    .map(|p| (p.remote_addr().to_string(), p.is_selected(), p.is_ip()))
                    .collect::<Vec<_>>(),
                "STY old server paths immediately after network restoration"
            );
            conn.closed().await;
            tokio::time::timeout(recovery, ep.close())
                .await
                .std_context("STY old server Endpoint close timed out")?;
            Ok(())
        })
        .client(old_client, async move |_dev, ep, conn| {
            let port = ep
                .bound_sockets()
                .iter()
                .find(|addr| addr.is_ipv4())
                .context("STY original client IPv4 socket missing")?
                .port();
            client_port_tx
                .send(port)
                .map_err(|_| anyerr!("STY original client port receiver dropped"))?;
            conn.wait_ip(setup)
                .await
                .context("old client initially direct")?;
            ping_open(&conn, setup)
                .await
                .context("old client initial ping")?;
            ping_open(&conn, recovery)
                .await
                .context("old client fallback ping")?;
            conn.wait_selected(recovery, |p| p.is_relay())
                .await
                .context("old client fallback must select relay")?;
            sty_fixture_open(&conn, recovery)
                .await
                .context("old client checked relay bytes")?;
            restored_rx
                .await
                .std_context("fresh nodes control: server restoration notification dropped")?;
            ping_open(&conn, recovery)
                .await
                .context("old client relay after server restoration")?;
            conn.close(0u32.into(), b"old-pair-control-done");
            tokio::time::timeout(recovery, ep.close())
                .await
                .std_context("STY old client Endpoint close timed out")?;
            Ok(())
        })
        .run()
        .await?;

    if flush_nat {
        initial_router
            .flush_nat_state()
            .await
            .context("STY reset simulated NAT conntrack")?;
        info!("STY restored-server NAT state flushed");
    }

    let old_server_port = server_port_rx
        .await
        .std_context("STY original server UDP port was not recorded")?;
    let old_client_port = client_port_rx
        .await
        .std_context("STY original client UDP port was not recorded")?;
    info!(
        old_server_port,
        old_client_port, reuse_device, reuse_port, flush_nat, "STY port persistence experiment"
    );

    // Same topology, but separate the two dimensions: a new Endpoint + UDP
    // socket on the ORIGINAL device, versus a new Endpoint on a NEW device.
    // The preceding Pair::run has returned and dropped its old endpoints.
    let fresh_server = if reuse_device {
        reusable_server
    } else {
        lab.add_device("fresh_server")
            .uplink(initial_id)
            .build()
            .await?
    };
    let fresh_client = if reuse_device {
        reusable_client
    } else {
        lab.add_device("fresh_client")
            .uplink(peer_id)
            .build()
            .await?
    };
    let mut new_pair = Pair::new(relay_map);
    if reuse_port {
        // The prior endpoints were explicitly closed; bind the new sockets
        // to precisely the same ports without preserving Endpoint/Noq state.
        new_pair = new_pair.udp_ports(old_server_port, old_client_port);
    }
    new_pair
        .server(fresh_server, async move |_dev, ep, conn| {
            let new_ipv4_port = ep
                .bound_sockets()
                .iter()
                .find(|addr| addr.is_ipv4())
                .context("STY replacement server IPv4 socket missing")?
                .port();
            if reuse_port {
                ensure_any!(
                    new_ipv4_port == old_server_port,
                    "STY replacement server reused wrong UDP port"
                );
            }
            info!(
                old_server_port,
                new_ipv4_port, reuse_port, "STY server UDP port comparison"
            );
            info!(
                endpoint = %ep.id().fmt_short(),
                sockets = ?ep.bound_sockets(),
                "STY fresh server endpoint and sockets"
            );
            let result = conn.wait_ip(recovery).await;
            info!(
                selected_direct = result.is_ok(),
                reuse_device,
                reuse_port,
                fresh_paths = ?conn
                    .paths()
                    .iter()
                    .map(|p| (p.remote_addr().to_string(), p.is_selected(), p.is_ip()))
                    .collect::<Vec<_>>(),
                "STY fresh NODES server control"
            );
            result.context("fresh nodes server must become direct")?;
            ping_accept(&conn, recovery)
                .await
                .context("fresh nodes server ping")?;
            sty_fixture_accept(&conn, recovery)
                .await
                .context("fresh nodes server checked direct bytes")?;
            conn.closed().await;
            Ok(())
        })
        .client(fresh_client, async move |_dev, ep, conn| {
            let new_ipv4_port = ep
                .bound_sockets()
                .iter()
                .find(|addr| addr.is_ipv4())
                .context("STY replacement client IPv4 socket missing")?
                .port();
            if reuse_port {
                ensure_any!(
                    new_ipv4_port == old_client_port,
                    "STY replacement client reused wrong UDP port"
                );
            }
            info!(
                old_client_port,
                new_ipv4_port, reuse_port, "STY client UDP port comparison"
            );
            info!(
                endpoint = %ep.id().fmt_short(),
                sockets = ?ep.bound_sockets(),
                "STY fresh client endpoint and sockets"
            );
            let result = conn.wait_ip(recovery).await;
            info!(
                selected_direct = result.is_ok(),
                reuse_device,
                reuse_port,
                fresh_paths = ?conn
                    .paths()
                    .iter()
                    .map(|p| (p.remote_addr().to_string(), p.is_selected(), p.is_ip()))
                    .collect::<Vec<_>>(),
                "STY fresh NODES client control"
            );
            result.context("fresh nodes client must become direct")?;
            ping_open(&conn, recovery)
                .await
                .context("fresh nodes client ping")?;
            sty_fixture_open(&conn, recovery)
                .await
                .context("fresh nodes client checked direct bytes")?;
            conn.close(0u32.into(), b"fresh-pair-control-done");
            Ok(())
        })
        .run()
        .await?;
    guard.ok();
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_fresh_nodes_control() -> Result {
    run_server_replug_new_endpoint_control(false, false, false).await
}

/// Discriminator for old OS-network-device state vs the state held by an old
/// Iroh/Noq endpoint: the new endpoints bind NEW UDP sockets but run in the
/// SAME device namespaces that were switched away and back.
#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_recreated_endpoints_same_devices() -> Result {
    run_server_replug_new_endpoint_control(true, false, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_recreated_endpoints_same_devices_same_ports() -> Result {
    run_server_replug_new_endpoint_control(true, true, false).await
}

/// Compare an identical new-port-binding control after flushing only
/// the restored server router's test NAT conntrack state.
#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_same_ports_after_nat_flush() -> Result {
    run_server_replug_new_endpoint_control(true, true, true).await
}
