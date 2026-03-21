//! Concrete [`QuicTransportBackend`] and [`Transport`] implementation backed by the `quinn` crate.
//!
//! Gated behind the `quinn-transport` feature flag.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use quinn::{ClientConfig, Endpoint, ServerConfig};
use tokio::runtime::{self, Runtime};
use tokio::sync::mpsc;

use crate::network::{
    ConnectionMetrics, NetworkError, QuicChannel, QuicEvent, QuicTransportBackend,
    SendChannel, Transport, TransportEvent,
};

/// A self-signed TLS certificate + private key generated with `rcgen`.
struct SelfSigned {
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
}

#[allow(clippy::expect_used)]
fn generate_self_signed() -> SelfSigned {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
        .expect("rcgen: failed to generate self-signed cert");
    SelfSigned {
        cert_der: cert.cert.der().to_vec(),
        key_der: cert.key_pair.serialize_der(),
    }
}

/// Quinn-backed QUIC transport.
///
/// Creates a small Tokio runtime internally to drive the async Quinn state
/// machine.  All public methods are synchronous and designed to be called
/// from the game loop.
pub struct QuinnBackend {
    rt: Runtime,
    endpoint: Option<Endpoint>,
    /// Outgoing (and accepted) connections keyed by remote address.
    peers: HashMap<SocketAddr, PeerState>,
    /// Channel for events produced by background accept / recv tasks.
    event_tx: mpsc::UnboundedSender<QuicEvent>,
    event_rx: mpsc::UnboundedReceiver<QuicEvent>,
}

struct PeerState {
    connection: quinn::Connection,
}

impl QuinnBackend {
    pub fn new() -> Result<Self, NetworkError> {
        let rt = runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| NetworkError(format!("tokio runtime: {e}")))?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            rt,
            endpoint: None,
            peers: HashMap::new(),
            event_tx,
            event_rx,
        })
    }

    fn build_server_config() -> Result<ServerConfig, NetworkError> {
        let ss = generate_self_signed();
        let key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(ss.key_der).into();
        let certs = vec![rustls::pki_types::CertificateDer::from(ss.cert_der)];

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| NetworkError(format!("rustls server config: {e}")))?;
        server_crypto.alpn_protocols = vec![b"quasar".to_vec()];

        Ok(ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
                .map_err(|e| NetworkError(format!("quinn server config: {e}")))?,
        )))
    }

    fn build_client_config() -> ClientConfig {
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        #[allow(clippy::expect_used)]
        let quic_config = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .expect("quinn client config");
        
        ClientConfig::new(Arc::new(quic_config))
    }

    /// Spawn a background task that accepts new incoming connections.
    fn spawn_accept_loop(
        endpoint: Endpoint,
        tx: mpsc::UnboundedSender<QuicEvent>,
    ) {
        tokio::spawn(async move {
            while let Some(incoming) = endpoint.accept().await {
                let tx = tx.clone();
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(conn) => {
                            let addr = conn.remote_address();
                            let _ = tx.send(QuicEvent::Connected(addr));
                            Self::spawn_recv_loop(conn, addr, tx);
                        }
                        Err(e) => {
                            log::warn!("quinn: incoming connection failed: {e}");
                        }
                    }
                });
            }
        });
    }

    /// Read datagrams and uni streams from a connection.
    fn spawn_recv_loop(
        conn: quinn::Connection,
        addr: SocketAddr,
        tx: mpsc::UnboundedSender<QuicEvent>,
    ) {
        // Datagram reader (unreliable channel).
        {
            let conn = conn.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                loop {
                    match conn.read_datagram().await {
                        Ok(data) => {
                            let _ = tx.send(QuicEvent::Data {
                                from: addr,
                                channel: QuicChannel::Unreliable,
                                payload: data.to_vec(),
                            });
                        }
                        Err(_) => break,
                    }
                }
                let _ = tx.send(QuicEvent::Disconnected(
                    addr,
                    "datagram stream closed".into(),
                ));
            });
        }

        // Uni-stream reader (reliable / bulk).
        {
            let conn = conn.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                loop {
                    match conn.accept_uni().await {
                        Ok(mut recv) => {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let mut buf = Vec::new();
                                if recv.read_to_end(1024 * 1024).await.is_ok() {
                                    buf = recv
                                        .read_to_end(1024 * 1024)
                                        .await
                                        .unwrap_or_default()
                                        .to_vec();
                                }
                                if !buf.is_empty() {
                                    let _ = tx.send(QuicEvent::Data {
                                        from: addr,
                                        channel: QuicChannel::Reliable,
                                        payload: buf,
                                    });
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    }
}

impl QuicTransportBackend for QuinnBackend {
    fn connect(&mut self, addr: SocketAddr) -> Result<(), NetworkError> {
        let client_cfg = Self::build_client_config();

        let endpoint = if let Some(ep) = self.endpoint.as_ref() {
            ep.clone()
        } else {
            #[allow(clippy::unwrap_used)]
            let mut ep = Endpoint::client("0.0.0.0:0".parse().unwrap())
                .map_err(|e| NetworkError(format!("quinn endpoint: {e}")))?;
            ep.set_default_client_config(client_cfg);
            self.endpoint = Some(ep.clone());
            ep
        };

        let tx = self.event_tx.clone();
        self.rt.spawn(async move {
            match endpoint.connect(addr, "localhost") {
                Ok(connecting) => match connecting.await {
                    Ok(conn) => {
                        let _ = tx.send(QuicEvent::Connected(addr));
                        QuinnBackend::spawn_recv_loop(conn, addr, tx);
                    }
                    Err(e) => {
                        let _ = tx.send(QuicEvent::Disconnected(
                            addr,
                            format!("connect failed: {e}"),
                        ));
                    }
                },
                Err(e) => {
                    let _ = tx.send(QuicEvent::Disconnected(
                        addr,
                        format!("connect error: {e}"),
                    ));
                }
            }
        });

        Ok(())
    }

    fn listen(&mut self, addr: SocketAddr) -> Result<(), NetworkError> {
        let server_cfg = Self::build_server_config()?;

        let endpoint = Endpoint::server(server_cfg, addr)
            .map_err(|e| NetworkError(format!("quinn listen: {e}")))?;

        let ep_clone = endpoint.clone();
        let tx = self.event_tx.clone();
        self.rt.spawn(async move {
            QuinnBackend::spawn_accept_loop(ep_clone, tx);
        });

        self.endpoint = Some(endpoint);
        Ok(())
    }

    fn poll(&mut self) -> Vec<QuicEvent> {
        let mut events = Vec::new();
        while let Ok(ev) = self.event_rx.try_recv() {
            // Track peers on connect/disconnect.
            match &ev {
                QuicEvent::Connected(_addr) => {}
                QuicEvent::Disconnected(addr, _) => {
                    self.peers.remove(addr);
                }
                _ => {}
            }
            events.push(ev);
        }
        events
    }

    fn send(
        &mut self,
        addr: SocketAddr,
        channel: QuicChannel,
        data: &[u8],
    ) -> Result<(), NetworkError> {
        // Find the connection for this peer. If we have an endpoint, try
        // looking through our peer map.
        let conn = self
            .peers
            .get(&addr)
            .map(|p| p.connection.clone())
            .ok_or_else(|| NetworkError(format!("no connection to {addr}")))?;

        let payload = data.to_vec();
        match channel {
            QuicChannel::Unreliable => {
                conn.send_datagram(payload.into())
                    .map_err(|e| NetworkError(format!("datagram send: {e}")))?;
            }
            QuicChannel::Reliable | QuicChannel::BulkTransfer => {
                self.rt.spawn(async move {
                    if let Ok(mut send) = conn.open_uni().await {
                        let _ = send.write_all(&payload).await;
                        let _ = send.finish();
                    }
                });
            }
        }
        Ok(())
    }

    fn peer_count(&self) -> usize {
        self.peers.len()
    }

    fn disconnect(&mut self, addr: SocketAddr) {
        if let Some(peer) = self.peers.remove(&addr) {
            peer.connection.close(0u32.into(), b"disconnect");
        }
    }
}

// ---------------------------------------------------------------------------
// Insecure cert verifier for development / LAN play.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

impl Transport for QuinnBackend {
    fn connect(&mut self, addr: SocketAddr) -> Result<(), NetworkError> {
        QuicTransportBackend::connect(self, addr)
    }

    fn listen(&mut self, addr: SocketAddr) -> Result<(), NetworkError> {
        QuicTransportBackend::listen(self, addr)
    }

    fn poll(&mut self) -> Vec<TransportEvent> {
        QuicTransportBackend::poll(self)
            .into_iter()
            .map(|e| match e {
                QuicEvent::Connected(addr) => TransportEvent::Connected(addr),
                QuicEvent::Disconnected(addr, reason) => TransportEvent::Disconnected(addr, reason),
                QuicEvent::Data { from, channel, payload } => {
                    let ch = match channel {
                        QuicChannel::Unreliable => SendChannel::Unreliable,
                        QuicChannel::Reliable => SendChannel::Reliable,
                        QuicChannel::BulkTransfer => SendChannel::Bulk,
                    };
                    TransportEvent::Data { from, channel: ch, payload }
                }
            })
            .collect()
    }

    fn send(&mut self, addr: SocketAddr, channel: SendChannel, data: &[u8]) -> Result<(), NetworkError> {
        let ch = match channel {
            SendChannel::Unreliable => QuicChannel::Unreliable,
            SendChannel::Reliable => QuicChannel::Reliable,
            SendChannel::Bulk => QuicChannel::BulkTransfer,
        };
        QuicTransportBackend::send(self, addr, ch, data)
    }

    fn peer_count(&self) -> usize {
        QuicTransportBackend::peer_count(self)
    }

    fn disconnect(&mut self, addr: SocketAddr) {
        QuicTransportBackend::disconnect(self, addr)
    }

      fn metrics(&self, addr: SocketAddr) -> Option<ConnectionMetrics> {
        self.peers.get(&addr).map(|peer| {
            let stats = peer.connection.stats();
            let path = &stats.path;
            ConnectionMetrics {
                rtt_ms: path.rtt.as_millis() as f32,
                packet_loss: if path.sent_packets > 0 {
                    path.lost_packets as f32 / path.sent_packets as f32
                } else {
                    0.0
                },
                bytes_sent: stats.udp_tx.bytes as u64,
                bytes_received: stats.udp_rx.bytes as u64,
                last_update: Some(std::time::Instant::now()),
            }
        })
    }

    fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        self.endpoint
            .as_ref()
            .map(|e| e.local_addr().map_err(|e| NetworkError(format!("local_addr: {e}"))))
            .unwrap_or_else(|| {
                #[allow(clippy::unwrap_used)]
                {
                    Ok("0.0.0.0:0".parse().unwrap())
                }
            })
    }
}
