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
            conn.wait_ip(setup)
                .await
                .context("STY direct: first direct path")?;
            ping_accept(&conn, setup)
                .await
                .context("STY direct: first ping")?;

            info!("STY_PATH_DIRECT_TO_RELAY: replugging into UDP-blocked access");
            dev.iface("eth0").unwrap().replug(blocked.id()).await?;
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
    run_direct_relay_direct(Side::Client, false, false, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_replug() -> Result {
    run_direct_relay_direct(Side::Server, false, false, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_with_traffic_probe() -> Result {
    run_direct_relay_direct(Side::Server, true, false, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_with_peer_network_hint() -> Result {
    run_direct_relay_direct(Side::Server, false, true, false).await
}

#[tokio::test]
#[traced_test]
async fn direct_relay_direct_server_fresh_connection_control() -> Result {
    run_direct_relay_direct(Side::Server, false, false, true).await
}
