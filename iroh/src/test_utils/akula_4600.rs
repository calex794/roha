//! Akula laboratory reproduction for n0-computer/iroh#4600.
//! Original test and scenario: https://github.com/n0-computer/iroh/issues/4600
//! Adapted locally solely for independent RED / GREEN regression verification.
//! This test intentionally exercises loss of a remote process without a QUIC
//! CONNECTION_CLOSE while its peer still retains the previous connection.

use std::time::{Duration, Instant};

use crate::{
    Endpoint, EndpointAddr, RelayMap, RelayMode, SecretKey,
    endpoint::{Connection, presets},
    tls::CaTlsConfig,
};

const ALPN: &[u8] = b"akula-4600-repro";

async fn accept_while_online(endpoint: Endpoint) {
    let mut connections: Vec<Connection> = Vec::new();
    while let Some(incoming) = endpoint.accept().await {
        if let Ok(connection) = incoming.await {
            connections.push(connection);
        }
    }
}

/// A new handshake must honor the newly supplied relay even while an old
/// connection to the same EndpointId has a stale selected path.
#[tokio::test]
async fn reconnect_after_peer_restarts_on_new_relay() {
    let (_, relay_a, _guard_a) = super::run_relay_server().await.unwrap();
    let (_, relay_b, _guard_b) = super::run_relay_server().await.unwrap();
    let host_key = SecretKey::generate();

    let builder = |relays: RelayMap| {
        Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Custom(relays))
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
            .clear_ip_transports()
            .alpns(vec![ALPN.to_vec()])
    };

    // The first host owns its own runtime. Destroying that runtime without a
    // graceful shutdown approximates process termination without close frames.
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();
    let first_host = {
        let host_builder = builder(RelayMap::from(relay_a.clone())).secret_key(host_key.clone());
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                let endpoint = host_builder.bind().await.unwrap();
                endpoint.online().await;
                tokio::spawn(accept_while_online(endpoint.clone()));
                ready_tx.send(()).unwrap();
                kill_rx.await.ok();
                // Do not gracefully close the endpoint, which would mask #4600.
                std::mem::forget(endpoint);
            });
            runtime.shutdown_background();
        })
    };
    ready_rx.await.unwrap();

    let client = builder(RelayMap::from_iter([relay_a.clone(), relay_b.clone()]))
        .bind()
        .await
        .unwrap();
    let old_connection = client
        .connect(
            EndpointAddr::new(host_key.public()).with_relay_url(relay_a),
            ALPN,
        )
        .await
        .unwrap();

    kill_tx.send(()).unwrap();
    tokio::task::spawn_blocking(move || first_host.join().unwrap())
        .await
        .unwrap();

    // Same authenticated identity now reachable through relay B only.
    let host = builder(RelayMap::from(relay_b.clone()))
        .secret_key(host_key.clone())
        .bind()
        .await
        .unwrap();
    host.online().await;
    let accept_task = tokio::spawn(accept_while_online(host.clone()));

    let started = Instant::now();
    let new_connection = tokio::time::timeout(
        Duration::from_secs(10),
        client.connect(
            EndpointAddr::new(host_key.public()).with_relay_url(relay_b),
            ALPN,
        ),
    )
    .await;
    eprintln!(
        "iroh #4600: elapsed={:?}, reconnected={:?}",
        started.elapsed(),
        new_connection.as_ref().map(Result::is_ok)
    );
    assert!(
        matches!(new_connection, Ok(Ok(_))),
        "new connection failed while stale selected path was retained"
    );

    drop(old_connection);
    client.close().await;
    host.close().await;
    accept_task.abort();
}
