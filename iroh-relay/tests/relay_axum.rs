//! Embeds iroh-relay into an [`axum::Router`].
//!
//! The relay handler upgrades the WebSocket via axum's [`WebSocketUpgrade`]
//! extractor, wraps the resulting [`WebSocket`] into a [`BytesStreamSink`]
//! as expected by iroh-relay's protocol handler, and then runs the
//! handshake plus client registration directly.
//!
//! [`BytesStreamSink`]: iroh_relay::protos::streams::BytesStreamSink

#![cfg(feature = "server")]

use std::{
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    Router,
    extract::{
        FromRequestParts, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header::ACCESS_CONTROL_ALLOW_ORIGIN},
    response::Response,
    routing::get,
};
use bytes::Bytes;
use iroh_base::{RelayUrl, SecretKey};
use iroh_dns::dns::DnsResolver;
use iroh_relay::{
    ExportKeyingMaterial, KeyCache,
    client::ClientBuilder,
    http::{CLIENT_AUTH_HEADER, ProtocolVersion, RELAY_PATH, RELAY_PROBE_PATH},
    protos::{handshake, streams::StreamError},
    server::{
        AllowAll, ClientRequest, DynAccessControl, Metrics, client::Config, clients::Clients,
        streams::RelayedStream,
    },
    tls::{CaTlsConfig, default_provider},
};
use n0_error::{AnyError, Result, StdResultExt};
use n0_future::{Sink, Stream, task::AbortOnDropHandle};
use n0_tracing_test::traced_test;
use rand::{RngExt, SeedableRng};
use tokio::net::TcpListener;
use tracing::{trace, warn};

#[derive(Clone, Debug)]
struct RelayState {
    key_cache: KeyCache,
    access: Arc<dyn DynAccessControl>,
    metrics: Arc<Metrics>,
    clients: Clients,
}

impl RelayState {
    fn new() -> Self {
        Self {
            key_cache: KeyCache::new(1024),
            access: Arc::new(AllowAll),
            metrics: Arc::new(Metrics::default()),
            clients: Clients::default(),
        }
    }
}

async fn serve_axum() -> Result<(SocketAddr, AbortOnDropHandle<()>)> {
    let state = RelayState::new();
    let router = Router::new()
        .route(RELAY_PATH, get(relay_handler))
        .route(RELAY_PROBE_PATH, get(ping_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service()).await;
    });
    Ok((addr, AbortOnDropHandle::new(task)))
}

async fn relay_handler(
    State(state): State<RelayState>,
    request: axum::extract::Request,
) -> Result<Response, StatusCode> {
    let (mut parts, _body) = request.into_parts();
    let ws = WebSocketUpgrade::from_request_parts(&mut parts, &state)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let client_auth_header = parts.headers.get(CLIENT_AUTH_HEADER).cloned();
    let ws = ws.protocols([ProtocolVersion::V2.to_str()]);
    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(error) = handle_relay_websocket(socket, state, parts, client_auth_header).await {
            warn!("relay websocket error: {error:#}");
        }
    }))
}

async fn ping_handler() -> impl axum::response::IntoResponse {
    (StatusCode::OK, [(ACCESS_CONTROL_ALLOW_ORIGIN, "*")])
}

/// Bridges axum's [`WebSocket`] to iroh-relay's [`BytesStreamSink`].
struct AxumWebSocketAdapter {
    inner: Pin<Box<WebSocket>>,
}

impl AxumWebSocketAdapter {
    fn new(socket: WebSocket) -> Self {
        Self {
            inner: Box::pin(socket),
        }
    }
}

impl Stream for AxumWebSocketAdapter {
    type Item = Result<Bytes, StreamError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            return match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(Message::Binary(data)))) => Poll::Ready(Some(Ok(data))),
                Poll::Ready(Some(Ok(Message::Close(_)))) => Poll::Ready(None),
                Poll::Ready(Some(Ok(_))) => continue,
                Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(AnyError::from_std(error)))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            };
        }
    }
}

impl Sink<Bytes> for AxumWebSocketAdapter {
    type Error = StreamError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner
            .as_mut()
            .poll_ready(cx)
            .map_err(AnyError::from_std)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        self.inner
            .as_mut()
            .start_send(Message::Binary(item))
            .map_err(AnyError::from_std)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner
            .as_mut()
            .poll_flush(cx)
            .map_err(AnyError::from_std)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner
            .as_mut()
            .poll_close(cx)
            .map_err(AnyError::from_std)
    }
}

// Axum's WebSocket has no access to TLS keying material.
impl ExportKeyingMaterial for AxumWebSocketAdapter {
    fn export_keying_material<T: AsMut<[u8]>>(
        &self,
        _output: T,
        _label: &[u8],
        _context: Option<&[u8]>,
    ) -> Option<T> {
        None
    }
}

async fn handle_relay_websocket(
    socket: WebSocket,
    state: RelayState,
    request_parts: http::request::Parts,
    client_auth_header: Option<http::HeaderValue>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut adapter = AxumWebSocketAdapter::new(socket);
    let authentication = handshake::serverside(&mut adapter, client_auth_header).await?;
    trace!(?authentication.mechanism, "verified authentication");

    let request = ClientRequest::new(
        authentication.client_key,
        ProtocolVersion::V2,
        request_parts,
    );
    let guard = authentication
        .authorize_with(&request, &state.access, &mut adapter)
        .await?;
    trace!("verified authorization");

    let stream = RelayedStream::new(adapter, state.key_cache.clone());
    let config = Config::new(guard, stream, ProtocolVersion::V2);
    state.clients.register(config, state.metrics.clone());
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn relay_embed_axum() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (addr, _guard) = serve_axum().await?;

    let resp = reqwest::get(format!("http://{addr}/ping"))
        .await
        .std_context("ping request")?;
    assert_eq!(resp.status(), 200);

    // Connect a relay client to `/relay` on the same axum-fronted port.
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0u64);
    let relay_url: RelayUrl = format!("http://{addr}").parse()?;
    let tls_config = CaTlsConfig::default().client_config(default_provider())?;
    tokio::time::timeout(
        Duration::from_secs(5),
        ClientBuilder::new(
            relay_url,
            SecretKey::from_bytes(&rng.random()),
            DnsResolver::new(),
        )
        .tls_client_config(tls_config)
        .connect(),
    )
    .await
    .std_context("timeout")??;
    Ok(())
}


/// Verify the real TLS reverse-proxy configuration, not merely path concatenation.
///
/// The test boots an axum relay backend on a private port, then a separate nginx
/// process with TLS termination and a /prefix/ path rewrite.  A native relay
/// client must complete the protocol handshake through nginx's WebSocket upgrade.
///
/// This test is intentionally ignored by the normal suite.  The dedicated CI
/// job installs the three system tools and invokes it with \`--ignored\`.
#[cfg(unix)]
#[tokio::test]
#[traced_test]
#[ignore = "requires nginx, openssl and curl; run using the dedicated STAYA proxy CI job"]
async fn relay_prefix_nginx_https_end_to_end() -> Result<()> {
    use std::{
        path::PathBuf,
        process::{Child, Command, Stdio},
    };

    struct NginxGuard {
        child: Child,
        dir: PathBuf,
    }
    impl Drop for NginxGuard {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    let _ = rustls::crypto::ring::default_provider().install_default();
    let (backend_addr, _backend_guard) = serve_axum().await?;

    // Reserve a real loopback port for the independent TLS proxy.
    let port_picker = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_port = port_picker.local_addr()?.port();
    drop(port_picker);

    let workdir = std::env::temp_dir().join(format!(
        "staya-relay-nginx-{}-{proxy_port}",
        std::process::id()
    ));
    std::fs::create_dir_all(&workdir)?;
    let cert = workdir.join("server.pem");
    let key = workdir.join("server.key");
    let openssl = Command::new("openssl")
        .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1"])
        .arg("-keyout")
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .args(["-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1"])
        .output()?;
    assert!(
        openssl.status.success(),
        "could not create a local TLS certificate: {}",
        String::from_utf8_lossy(&openssl.stderr)
    );

    // nginx strips /prefix/ before forwarding to the unmodified relay backend.
    // WebSocket upgrades must survive the extra proxy hop.
    let config = format!(
        r#"worker_processes 1;
daemon off;
pid {dir}/nginx.pid;
error_log stderr warn;
events {{ worker_connections 128; }}
http {{
    access_log off;
    server {{
        listen 127.0.0.1:{proxy_port} ssl;
        server_name localhost;
        ssl_certificate {cert};
        ssl_certificate_key {key};
        location /prefix/ {{
            proxy_pass http://{backend_addr}/;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_read_timeout 30s;
        }}
    }}
}}
"#,
        dir=workdir.display(),
        cert=cert.display(),
        key=key.display(),
    );
    let conf = workdir.join("nginx.conf");
    std::fs::write(&conf, config)?;
    let child = Command::new("nginx")
        .arg("-p")
        .arg(&workdir)
        .arg("-c")
        .arg(&conf)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    let _proxy_guard = NginxGuard {
        child,
        dir: workdir,
    };

    let base_url = format!("https://127.0.0.1:{proxy_port}");
    let ping_url = format!("{base_url}/prefix/ping");
    // Command::output blocks its calling thread.  This integration test
    // serves the relay on the Tokio executor, so running curl directly here
    // starves that server of runtime turns on the default current-thread
    // Tokio test runtime.  Keep the process call on Tokio's blocking pool.
    let status_for = |target: String| async move {
        tokio::task::spawn_blocking(move || -> Result<String> {
            let output = Command::new("curl")
                .args(["--insecure", "--silent", "--show-error", "--output", "/dev/null", "--max-time", "2"])
                .args(["--write-out", "%{http_code}"])
                .arg(&target)
                .output()?;
            if !output.status.success() {
                eprintln!(
                    "curl to {target} returned {:?}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        })
        .await
        .expect("curl blocking worker exited unexpectedly")
    };

    let mut ready = false;
    for _ in 0..40 {
        if status_for(ping_url.clone()).await? == "200" {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(ready, "HTTPS /prefix/ping never became reachable through nginx");
    assert_eq!(
        status_for(format!("{base_url}/ping")).await?,
        "404",
        "unprefixed probe must not be served by the reverse proxy"
    );

    // The native client must retain /prefix/relay while changing HTTPS to WSS,
    // then successfully upgrade and authenticate against the real relay.
    let relay_url: RelayUrl = format!("{base_url}/prefix/").parse()?;
    let tls_config = CaTlsConfig::insecure_skip_verify().client_config(default_provider())?;
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    // CI can increase the number of real TLS/WebSocket handshakes on the
    // same nginx/backend process. No requests go to an external service.
    let rounds: usize = std::env::var("STAYA_STRESS_ROUNDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    assert!((1..=64).contains(&rounds), "bounded to 64 local handshakes");
    let mut succeeded = 0usize;
    for i in 0..rounds {
        let conn = tokio::time::timeout(
            Duration::from_secs(10),
            ClientBuilder::new(
                relay_url.clone(),
                SecretKey::from_bytes(&rng.random()),
                DnsResolver::new(),
            )
            .tls_client_config(tls_config.clone())
            .connect(),
        )
        .await
        .std_context("timeout connecting to relay over nginx HTTPS")??;
        succeeded += 1;
        // Exercise client disconnect and reconnection on one running relay.
        drop(conn);
        if (i + 1) % 8 == 0 {
            assert_eq!(
                status_for(ping_url.clone()).await?,
                "200",
                "relay probe failed during connection churn"
            );
        }
    }
    assert_eq!(succeeded, rounds);
    eprintln!("STAYA relay local HTTPS nginx connections: {succeeded}/{rounds}");

    // A successful ping only confirms the relay control plane.
    // Check actual payload delivery A -> B and B -> A across HTTPS nginx,
    // including the authenticated sending endpoint and exact bytes.
    if std::env::var("STAYA_DATAGRAM_E2E").ok().as_deref() == Some("1") {
        use iroh_relay::protos::relay::{ClientToRelayMsg, Datagrams, RelayToClientMsg};
        use n0_future::{SinkExt, StreamExt};

        let key_a = SecretKey::from_bytes(&rng.random());
        let key_b = SecretKey::from_bytes(&rng.random());
        let id_a = key_a.public();
        let id_b = key_b.public();

        let connect = |key: SecretKey| {
            ClientBuilder::new(relay_url.clone(), key, DnsResolver::new())
                .tls_client_config(tls_config.clone())
        };
        let mut a = tokio::time::timeout(Duration::from_secs(10), connect(key_a).connect())
            .await
            .std_context("relay client A connect timed out")??;
        let mut b = tokio::time::timeout(Duration::from_secs(10), connect(key_b).connect())
            .await
            .std_context("relay client B connect timed out")??;

        for round in 0..4 {
            let sent_a = Datagrams::from(
                format!("STAYA/A-to-B/round-{round}/{}", "data".repeat(120)).as_bytes()
            );
            a.send(ClientToRelayMsg::Datagrams {
                dst_endpoint_id: id_b,
                datagrams: sent_a.clone(),
            }).await?;
            let (from_a, received_b) = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let frame = b.next().await.expect("client B stream ended").expect("client B receive failed");
                    if let RelayToClientMsg::Datagrams { remote_endpoint_id, datagrams } = frame {
                        break (remote_endpoint_id, datagrams);
                    }
                }
            }).await.std_context("A-to-B datagram was not delivered")?;
            assert_eq!(from_a, id_a, "wrong sender for A-to-B datagram");
            assert_eq!(received_b, sent_a, "payload changed A-to-B");

            let sent_b = Datagrams::from(
                format!("STAYA/B-to-A/round-{round}/{}", "reply".repeat(90)).as_bytes()
            );
            b.send(ClientToRelayMsg::Datagrams {
                dst_endpoint_id: id_a,
                datagrams: sent_b.clone(),
            }).await?;
            let (from_b, received_a) = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let frame = a.next().await.expect("client A stream ended").expect("client A receive failed");
                    if let RelayToClientMsg::Datagrams { remote_endpoint_id, datagrams } = frame {
                        break (remote_endpoint_id, datagrams);
                    }
                }
            }).await.std_context("B-to-A datagram was not delivered")?;
            assert_eq!(from_b, id_b, "wrong sender for B-to-A datagram");
            assert_eq!(received_a, sent_b, "payload changed B-to-A");
        }
        assert_eq!(status_for(ping_url.clone()).await?, "200");
        a.close().await?;
        b.close().await?;
        eprintln!("STAYA relay data path: 8/8 directional datagrams delivered through nginx TLS");
    }
    Ok(())
}
