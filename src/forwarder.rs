use std::net::{IpAddr, Ipv4Addr};

use hickory_server::{
    proto::{
        op::ResponseCode,
        rr::{LowerName, RecordType, TSigResponseContext},
    },
    resolver::config::{ResolveHosts, ResolverOpts, ServerGroup},
    server::{Request, RequestInfo},
    store::forwarder::{ForwardConfig, ForwardZoneHandler},
    zone_handler::{
        AuthLookup, AxfrPolicy, LookupControlFlow, LookupError, LookupOptions, ZoneHandler,
        ZoneTransfer, ZoneType,
    },
};

pub const CLOUDFLARE: ServerGroup<'static> = ServerGroup {
    ips: &[
        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)),
        // IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111)),
        // IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1001)),
    ],
    server_name: "cloudflare-dns.com",
    path: "/dns-query",
};

pub const QUAD9: ServerGroup<'static> = ServerGroup {
    ips: &[
        IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)),
        IpAddr::V4(Ipv4Addr::new(149, 112, 112, 112)),
        // IpAddr::V6(Ipv6Addr::new(0x2620, 0x00fe, 0, 0, 0, 0, 0, 0x00fe)),
        // IpAddr::V6(Ipv6Addr::new(0x2620, 0x00fe, 0, 0, 0, 0, 0, 0x0009)),
    ],
    server_name: "dns.quad9.net",
    path: "/dns-query",
};

pub struct ForwardZoneHandlerWrapper {
    inner: ForwardZoneHandler,
}

impl ForwardZoneHandlerWrapper {
    #[must_use]
    pub fn new() -> Self {
        let mut forward_handler_opts = ResolverOpts::default();
        forward_handler_opts.use_hosts_file = ResolveHosts::Never;

        let forwarder = ForwardZoneHandler::builder_tokio(ForwardConfig {
            name_servers: [CLOUDFLARE.tls(), QUAD9.tls()]
                .into_iter()
                .flatten()
                .collect(),
            options: Some(forward_handler_opts),
        })
        .build()
        .unwrap();

        Self { inner: forwarder }
    }
}

// TODO: doesn't do anything but could be extended with custom caching.
#[async_trait::async_trait]
impl ZoneHandler for ForwardZoneHandlerWrapper {
    fn zone_type(&self) -> ZoneType {
        self.inner.zone_type()
    }

    fn axfr_policy(&self) -> AxfrPolicy {
        self.inner.axfr_policy()
    }

    fn can_validate_dnssec(&self) -> bool {
        self.inner.can_validate_dnssec()
    }

    fn origin(&self) -> &LowerName {
        self.inner.origin()
    }

    async fn lookup(
        &self,
        name: &LowerName,
        rtype: RecordType,
        request_info: Option<&RequestInfo<'_>>,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<AuthLookup> {
        self.inner
            .lookup(name, rtype, request_info, lookup_options)
            .await
    }

    async fn update(
        &self,
        update: &Request,
        now: u64,
    ) -> (Result<bool, ResponseCode>, Option<TSigResponseContext>) {
        self.inner.update(update, now).await
    }

    async fn consult(
        &self,
        name: &LowerName,
        rtype: RecordType,
        request_info: Option<&RequestInfo<'_>>,
        lookup_options: LookupOptions,
        last_result: LookupControlFlow<AuthLookup>,
    ) -> (LookupControlFlow<AuthLookup>, Option<TSigResponseContext>) {
        self.inner
            .consult(name, rtype, request_info, lookup_options, last_result)
            .await
    }

    async fn zone_transfer(
        &self,
        request: &Request,
        lookup_options: LookupOptions,
        now: u64,
    ) -> Option<(
        Result<ZoneTransfer, LookupError>,
        Option<TSigResponseContext>,
    )> {
        self.inner.zone_transfer(request, lookup_options, now).await
    }

    async fn search(
        &self,
        request: &Request,
        lookup_options: LookupOptions,
    ) -> (LookupControlFlow<AuthLookup>, Option<TSigResponseContext>) {
        self.inner.search(request, lookup_options).await
    }

    async fn nsec_records(
        &self,
        name: &LowerName,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<AuthLookup> {
        self.inner.nsec_records(name, lookup_options).await
    }
}
