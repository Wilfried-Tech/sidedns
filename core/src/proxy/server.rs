use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, LazyLock},
};

use anyhow::Result;
use http_body_util::{BodyExt, Empty, Full};
use hyper::{
    HeaderMap, Request, Response, StatusCode,
    body::{Bytes, Incoming},
    header::{
        CONNECTION, HeaderName, HeaderValue, LOCATION, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE,
        TRAILER, TRANSFER_ENCODING, UPGRADE,
    },
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::{
    net::{TcpListener, TcpStream},
    pin,
};
use tokio_rustls::{TlsAcceptor, rustls::ServerConfig};
use tokio_util::sync::CancellationToken;

use crate::{PROXY_LISTEN_ADDR, certs::Ca, dns::DomainResolver};

type Body = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;
type Client = hyper::client::conn::http1::Builder;

static HOP_BY_HOP_HEADERS: LazyLock<Vec<HeaderName>> = LazyLock::new(|| {
    vec![
        PROXY_AUTHORIZATION,
        PROXY_AUTHENTICATE,
        TE,
        TRANSFER_ENCODING,
        TRAILER,
        HeaderName::from_bytes(b"proxy-connection").unwrap(),
    ]
});

fn empty() -> Body {
    Empty::new().map_err(|e| match e {}).boxed()
}

fn full(data: impl Into<Bytes>) -> Body {
    Full::new(data.into()).map_err(|e| match e {}).boxed()
}

fn bad_gateway() -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(empty())
        .unwrap()
}

fn bad_gateway_with_body(body: Body) -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(body)
        .unwrap()
}

pub struct ProxyServer {
    resolver: Arc<DomainResolver>,
    token: CancellationToken,
}

impl ProxyServer {
    pub fn new(resolver: Arc<DomainResolver>, token: CancellationToken) -> Self {
        Self { resolver, token }
    }

    #[tracing::instrument(skip(self, io, client_ip, is_tls), name = "Proxy Server")]
    async fn handle_connection<T>(
        &self,
        io: TokioIo<T>,
        client_ip: IpAddr,
        is_tls: bool,
    ) -> Result<()>
    where
        T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let conn = http1::Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .serve_connection(
                io,
                service_fn(|req| self.service_handle_connection(req, client_ip, is_tls)),
            )
            .with_upgrades();

        pin!(conn);

        tokio::select! {
            biased;
            _ = self.token.cancelled()=>{
                conn.graceful_shutdown();
            }
            Err(err) = &mut conn => {
                tracing::error!("Error serving connection: {:?}", err);
            }
        };

        Ok(())
    }

    fn is_websocket(headers: &HeaderMap) -> bool {
        let upgrade = headers
            .get(UPGRADE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_ascii_lowercase());

        let connection = headers
            .get(CONNECTION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();

        let has_upgrade = upgrade.as_deref() == Some("websocket");
        let has_connection_upgrade = connection.contains("upgrade");

        has_upgrade && has_connection_upgrade
    }
    fn extract_host(headers: &HeaderMap) -> Option<String> {
        headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h).to_lowercase())
    }

    fn add_proxy_headers(headers: &mut HeaderMap, client_ip: IpAddr, is_tls: bool) {
        let ip_str = client_ip.to_string();

        if let Ok(v) = HeaderValue::from_str(&ip_str) {
            headers.insert("X-Real-IP", v);
        }
        if let Ok(v) = HeaderValue::from_str(&ip_str) {
            headers.insert("X-Forwarded-For", v);
        }
        if let Ok(v) = HeaderValue::from_str(if is_tls { "https" } else { "http" }) {
            headers.insert("X-Forwarded-Proto", v);
        }
    }

    fn strip_hop_by_hop(headers: &mut HeaderMap) {
        for name in HOP_BY_HOP_HEADERS.iter() {
            headers.remove(name);
        }
        if !Self::is_websocket(headers) {
            headers.remove(CONNECTION);
        }
    }

    async fn service_handle_connection(
        &self,
        mut request: Request<Incoming>,
        client_ip: IpAddr,
        is_tls: bool,
    ) -> Result<Response<Body>> {
        let host = Self::extract_host(request.headers())
            .ok_or_else(|| anyhow::anyhow!("Missing Host header"))?;

        let Some(rule) = self.resolver.resolve_domain(&host) else {
            return Ok(bad_gateway());
        };

        if rule.https && !is_tls {
            let url = format!(
                "https://{}{}",
                host,
                request
                    .uri()
                    .path_and_query()
                    .map(|pq| pq.as_str())
                    .unwrap_or("/")
            );
            return Ok(Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header(LOCATION, url)
                .body(empty())
                .unwrap());
        }

        let target = match rule.target.ip() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED) => {
                SocketAddr::new(Ipv4Addr::LOCALHOST.into(), rule.target.port())
            },
            IpAddr::V6(Ipv6Addr::UNSPECIFIED) => {
                SocketAddr::new(Ipv6Addr::LOCALHOST.into(), rule.target.port())
            },
            _ => rule.target,
        };

        Self::strip_hop_by_hop(request.headers_mut());
        Self::add_proxy_headers(request.headers_mut(), client_ip, is_tls);
        let Ok(upstream) = TcpStream::connect(target).await else {
            return Ok(bad_gateway_with_body(full("Target service is unreachable")));
        };

        let (mut sender, conn) = Client::new()
            .handshake::<TokioIo<TcpStream>, Body>(TokioIo::new(upstream))
            .await?;

        tokio::spawn(conn.with_upgrades());

        let mut client_upgrade = None;
        if Self::is_websocket(request.headers()) {
            client_upgrade = Some(hyper::upgrade::on(&mut request));
        }

        let uri = request.uri().to_string();

        let mut server_resp = sender.send_request(request.map(|b| b.boxed())).await?;

        if server_resp.status() == StatusCode::SWITCHING_PROTOCOLS
            && let Some(client_upgrade) = client_upgrade
        {
            tracing::info!("WebSocket upgrade detected on {uri}, upgrading connection");
            let server_upgrade = hyper::upgrade::on(&mut server_resp);
            tokio::spawn(async move {
                if let (Ok(client_stream), Ok(server_stream)) =
                    tokio::join!(client_upgrade, server_upgrade)
                {
                    let _ = tokio::io::copy_bidirectional(
                        &mut TokioIo::new(client_stream),
                        &mut TokioIo::new(server_stream),
                    )
                    .await;
                } else {
                    tracing::error!("WebSocket upgrade failed");
                }
            });
        }

        Ok(server_resp.map(|b| b.boxed()))
    }
}

#[tracing::instrument(skip(resolver, token), name = "Proxy Server")]
pub async fn run_proxy_server(
    resolver: Arc<DomainResolver>,
    token: CancellationToken,
) -> Result<()> {
    tracing::info!("Started");
    let proxy_server = Arc::new(ProxyServer::new(resolver, token.clone()));

    let http_handle = {
        let proxy_server = proxy_server.clone();
        tokio::spawn(async move {
            if let Err(err) = serve_http(proxy_server).await {
                tracing::error!("HTTP server error: {:?}", err);
            }
        })
    };

    let https_handle = {
        let proxy_server = proxy_server.clone();
        tokio::spawn(async move {
            if !Ca::is_installed() {
                tracing::warn!(
                    "CA certificate is not installed, HTTPS proxy will not work. Run 'sidedns cert install'."
                );
                return;
            }

            if let Err(err) = serve_https(proxy_server).await {
                tracing::error!("HTTPS server error: {:?}", err);
            }
        })
    };

    let (http_res, https_res) = tokio::join!(http_handle, https_handle);

    http_res?;
    https_res?;

    tracing::info!("Stopped");
    Ok(())
}

#[tracing::instrument(skip(proxy_server), name = "HTTP Proxy Server")]
async fn serve_http(proxy_server: Arc<ProxyServer>) -> Result<()> {
    tracing::info!("Started");

    let mut proxy_addr = PROXY_LISTEN_ADDR;
    proxy_addr.set_port(80);

    let listener = TcpListener::bind(proxy_addr).await?;
    tracing::info!("Listening on {}", proxy_addr);

    loop {
        tokio::select! {
            biased;
            _ = proxy_server.token.cancelled() => {
                tracing::info!("Shutdown requested, stopping...");
                break;
            }
            conn = listener.accept() => {
                let (stream, addr) = match conn {
                    Ok(v)  => v,
                    Err(e) => {
                        tracing::error!("Accept error: {e}");
                        continue;
                    }
                };

                tracing::info!("Accepted connection from {}", addr);
                let proxy_server = proxy_server.clone();

                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    if let Err(e) = proxy_server.handle_connection(io, addr.ip(), false).await {
                        tracing::error!("Error handling connection from {}: {:?}", addr, e);
                    }
                });
            }
        }
    }

    tracing::info!("Stopped");
    Ok(())
}

#[tracing::instrument(skip(proxy_server), name = "HTTPS Proxy Server")]
async fn serve_https(proxy_server: Arc<ProxyServer>) -> Result<()> {
    tracing::info!("Started");

    let mut proxy_addr = PROXY_LISTEN_ADDR;
    proxy_addr.set_port(443);

    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(proxy_server.resolver.clone());

    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind(proxy_addr).await?;
    tracing::info!("Listening on {}", proxy_addr);

    loop {
        tokio::select! {
            biased;
            _ = proxy_server.token.cancelled() => {
                tracing::info!("Shutdown requested, stopping...");
                break;
            }
            conn = listener.accept() => {
                let Ok((stream, addr)) = conn else {
                    tracing::error!("Failed to accept connection");
                    continue;
                };

                tracing::info!("Accepted connection from {}", addr);
                let proxy_server = proxy_server.clone();
                let acceptor = acceptor.clone();

                tokio::spawn(async move {
                    match acceptor.accept(stream).await {
                        Ok(tls_stream) => {
                            let io = TokioIo::new(tls_stream);
                            if let Err(e) = proxy_server.handle_connection(io, addr.ip(), true).await {
                                tracing::error!("Error handling connection from {}: {:?}", addr, e);
                            }
                        },
                        Err(e) => {
                            tracing::error!("TLS accept error from {}: {:?}", addr, e);
                        }
                    }
                });
            }
        }
    }

    tracing::info!("Stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    #[test]
    fn ws_upgrade_and_connection_upgrade() {
        let h = headers(&[("upgrade", "websocket"), ("connection", "Upgrade")]);
        assert!(ProxyServer::is_websocket(&h));
    }

    #[test]
    fn ws_upgrade_mixed_case() {
        let h = headers(&[
            ("upgrade", "WebSocket"),
            ("connection", "keep-alive, Upgrade"),
        ]);
        assert!(ProxyServer::is_websocket(&h));
    }

    #[test]
    fn ws_false_without_upgrade_header() {
        let h = headers(&[("connection", "Upgrade")]);
        assert!(!ProxyServer::is_websocket(&h));
    }

    #[test]
    fn ws_false_without_connection_header() {
        let h = headers(&[("upgrade", "websocket")]);
        assert!(!ProxyServer::is_websocket(&h));
    }

    #[test]
    fn ws_false_for_h2c_upgrade() {
        let h = headers(&[("upgrade", "h2c"), ("connection", "Upgrade")]);
        assert!(!ProxyServer::is_websocket(&h));
    }

    #[test]
    fn ws_false_for_empty_headers() {
        assert!(!ProxyServer::is_websocket(&HeaderMap::new()));
    }

    #[test]
    fn extract_host_strips_port() {
        let h = headers(&[("host", "api.local:8080")]);
        assert_eq!(ProxyServer::extract_host(&h), Some("api.local".into()));
    }

    #[test]
    fn extract_host_without_port() {
        let h = headers(&[("host", "api.local")]);
        assert_eq!(ProxyServer::extract_host(&h), Some("api.local".into()));
    }

    #[test]
    fn extract_host_lowercases() {
        let h = headers(&[("host", "API.LOCAL:80")]);
        assert_eq!(ProxyServer::extract_host(&h), Some("api.local".into()));
    }

    #[test]
    fn extract_host_none_when_missing() {
        assert_eq!(ProxyServer::extract_host(&HeaderMap::new()), None);
    }

    #[test]
    fn proxy_headers_http_proto() {
        let mut h = HeaderMap::new();
        ProxyServer::add_proxy_headers(&mut h, IpAddr::V4(Ipv4Addr::LOCALHOST), false);
        assert_eq!(h.get("X-Forwarded-Proto").unwrap(), "http");
    }

    #[test]
    fn proxy_headers_https_proto() {
        let mut h = HeaderMap::new();
        ProxyServer::add_proxy_headers(&mut h, IpAddr::V4(Ipv4Addr::LOCALHOST), true);
        assert_eq!(h.get("X-Forwarded-Proto").unwrap(), "https");
    }

    #[test]
    fn proxy_headers_sets_real_ip_and_forwarded_for() {
        let mut h = HeaderMap::new();
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        ProxyServer::add_proxy_headers(&mut h, ip, false);
        assert_eq!(h.get("X-Real-IP").unwrap(), "10.0.0.1");
        assert_eq!(h.get("X-Forwarded-For").unwrap(), "10.0.0.1");
    }

    #[test]
    fn proxy_headers_ipv6() {
        let mut h = HeaderMap::new();
        let ip: IpAddr = "::1".parse().unwrap();
        ProxyServer::add_proxy_headers(&mut h, ip, false);
        assert!(h.get("X-Real-IP").is_some());
        assert!(h.get("X-Forwarded-For").is_some());
    }

    #[test]
    fn strip_removes_proxy_authorization() {
        let mut h = headers(&[("proxy-authorization", "Basic xyz")]);
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.get("proxy-authorization").is_none());
    }

    #[test]
    fn strip_removes_transfer_encoding() {
        let mut h = headers(&[("transfer-encoding", "chunked")]);
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.get("transfer-encoding").is_none());
    }

    #[test]
    fn strip_removes_te() {
        let mut h = headers(&[("te", "trailers")]);
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.get("te").is_none());
    }

    #[test]
    fn strip_removes_trailer() {
        let mut h = headers(&[("trailer", "Expires")]);
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.get("trailer").is_none());
    }

    #[test]
    fn strip_removes_connection_for_plain_http() {
        let mut h = headers(&[
            ("connection", "keep-alive"),
            ("content-type", "application/json"),
        ]);
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.get("connection").is_none());
        assert!(h.get("content-type").is_some());
    }

    #[test]
    fn strip_keeps_connection_upgrade_for_websocket() {
        let mut h = headers(&[("upgrade", "websocket"), ("connection", "Upgrade")]);
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.get("connection").is_some());
        assert!(h.get("upgrade").is_some());
    }

    #[test]
    fn strip_preserves_unrelated_headers() {
        let mut h = headers(&[
            ("authorization", "Bearer token"),
            ("cookie", "session=abc"),
            ("content-type", "application/json"),
        ]);
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.get("authorization").is_some());
        assert!(h.get("cookie").is_some());
        assert!(h.get("content-type").is_some());
    }

    #[test]
    fn strip_removes_proxy_connection() {
        let mut h = headers(&[("proxy-connection", "keep-alive")]);
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.get("proxy-connection").is_none());
    }

    #[test]
    fn strip_noop_on_empty_headers() {
        let mut h = HeaderMap::new();
        ProxyServer::strip_hop_by_hop(&mut h);
        assert!(h.is_empty());
    }
}
