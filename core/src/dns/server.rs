use std::net::IpAddr;
use std::sync::Arc;

use hickory_resolver::TokioAsyncResolver;
use hickory_resolver::error::ResolveErrorKind;
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_server::authority::MessageResponseBuilder;
use hickory_server::proto::op::{Header, MessageType, ResponseCode};
use hickory_server::proto::rr::{Name, RData, Record, RecordType};
use hickory_server::server::{Request, RequestHandler, ResponseHandler, ResponseInfo};

use crate::{DNS_RECORD_TTL_SECONDS, PROXY_LISTEN_ADDR, dns::DomainResolver};

/// Build a [`TokioAsyncResolver`] from the system's DNS configuration.
pub fn build_upstream_resolver() -> anyhow::Result<TokioAsyncResolver> {
    let (config, opts) = hickory_resolver::system_conf::read_system_conf()
        .map_err(|e| anyhow::anyhow!("Failed to read system DNS config: {e}"))?;

    Ok(TokioAsyncResolver::new(
        config,
        opts,
        TokioConnectionProvider::default(),
    ))
}

/// [`RequestHandler`] implementation that routes DNS queries through [`DomainResolver`].
pub(super) struct DnsHandler {
    resolver: Arc<DomainResolver>,
    upstream: TokioAsyncResolver,
}

#[async_trait::async_trait]
impl RequestHandler for DnsHandler {
    #[tracing::instrument(skip(self, request, responder), name = "DNS Server Handler")]
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        mut responder: R,
    ) -> ResponseInfo {
        let query = request.query();
        let name = query.name().to_string();
        let domain = name.trim_end_matches('.').to_lowercase();
        let qtype = query.query_type();

        tracing::debug!("DNS query: {domain} type={qtype}");

        match self.resolve(&domain, qtype, request, &mut responder).await {
            Ok(info) => info,
            Err(e) => {
                tracing::error!("DNS handler error for {domain}: {e}");
                let mut header = Header::response_from_request(request.header());
                header.set_response_code(ResponseCode::ServFail);
                let response = MessageResponseBuilder::from_message_request(request).build(
                    header,
                    &[],
                    &[],
                    &[],
                    &[],
                );
                responder.send_response(response).await.unwrap_or_else(|e| {
                    tracing::error!("Failed to send SERVFAIL for {domain}: {e}");
                    ResponseInfo::from(Header::new())
                })
            },
        }
    }
}

impl DnsHandler {
    pub fn new(resolver: Arc<DomainResolver>, upstream: TokioAsyncResolver) -> Self {
        Self { resolver, upstream }
    }

    /// Only intercept A and AAAA — forward everything else (MX, TXT, SRV…)
    async fn resolve<R: ResponseHandler>(
        &self,
        domain: &str,
        qtype: RecordType,
        request: &Request,
        responder: &mut R,
    ) -> anyhow::Result<ResponseInfo> {
        tracing::info!("New Incoming Dns Request: {domain} {qtype}");
        if matches!(qtype, RecordType::A | RecordType::AAAA)
            && self.resolver.resolve_domain(domain).is_some()
        {
            return self
                .respond_local(PROXY_LISTEN_ADDR.ip(), domain, qtype, request, responder)
                .await;
        }

        self.forward(domain, qtype, request, responder).await
    }

    async fn respond_local<R: ResponseHandler>(
        &self,
        ip: IpAddr,
        domain: &str,
        qtype: RecordType,
        request: &Request,
        responder: &mut R,
    ) -> anyhow::Result<ResponseInfo> {
        tracing::info!("Redirecting {domain} to reverse proxy at http://{ip}");

        let name = Name::from_ascii(format!("{domain}."))?;
        let mut header = Header::response_from_request(request.header());
        header.set_message_type(MessageType::Response);
        header.set_authoritative(true);
        header.set_response_code(ResponseCode::NoError);

        let rdata = match ip {
            IpAddr::V4(v4) if qtype == RecordType::A => RData::A(v4.into()),
            IpAddr::V6(v6) if qtype == RecordType::AAAA => RData::AAAA(v6.into()),
            IpAddr::V4(v4) => {
                tracing::warn!("AAAA query for {domain} but rule has IPv4 {v4}, returning empty");
                let response = MessageResponseBuilder::from_message_request(request).build(
                    header,
                    &[],
                    &[],
                    &[],
                    &[],
                );
                return Ok(responder.send_response(response).await?);
            },
            IpAddr::V6(v6) => {
                tracing::warn!("A query for {domain} but rule has IPv6 {v6}, returning empty");
                let response = MessageResponseBuilder::from_message_request(request).build(
                    header,
                    &[],
                    &[],
                    &[],
                    &[],
                );
                return Ok(responder.send_response(response).await?);
            },
        };

        let record = Record::from_rdata(name, DNS_RECORD_TTL_SECONDS, rdata);
        let answers = [&record];
        let response = MessageResponseBuilder::from_message_request(request).build(
            header,
            answers,
            &[],
            &[],
            &[],
        );

        Ok(responder.send_response(response).await?)
    }

    async fn forward<R: ResponseHandler>(
        &self,
        domain: &str,
        qtype: RecordType,
        request: &Request,
        responder: &mut R,
    ) -> anyhow::Result<ResponseInfo> {
        tracing::info!("Forwarding DNS request: {domain} type={qtype}");

        let lookup_result = self.upstream.lookup(format!("{domain}."), qtype).await;

        let mut header = Header::response_from_request(request.header());
        header.set_message_type(MessageType::Response);
        header.set_response_code(ResponseCode::NoError);

        match lookup_result {
            Ok(lookup) => {
                let records: Vec<&Record> = lookup.record_iter().collect();
                let response = MessageResponseBuilder::from_message_request(request).build(
                    header,
                    records.iter().copied(),
                    &[],
                    &[],
                    &[],
                );
                Ok(responder.send_response(response).await?)
            },
            Err(e)
                if let ResolveErrorKind::NoRecordsFound {
                    query: _,
                    soa: _,
                    negative_ttl: _,
                    response_code: _,
                    trusted: _,
                } = e.kind() =>
            {
                header.set_response_code(ResponseCode::NXDomain);
                let response = MessageResponseBuilder::from_message_request(request).build(
                    header,
                    &[],
                    &[],
                    &[],
                    &[],
                );
                Ok(responder.send_response(response).await?)
            },
            Err(e) => {
                tracing::error!("Upstream DNS error for {domain}: {e}");
                header.set_response_code(ResponseCode::ServFail);
                let response = MessageResponseBuilder::from_message_request(request).build(
                    header,
                    &[],
                    &[],
                    &[],
                    &[],
                );
                Ok(responder.send_response(response).await?)
            },
        }
    }
}
