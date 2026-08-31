use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
    time::Instant,
};

use futures::future::join_all;
use hickory_resolver::{
    Resolver,
    config::{NameServerConfig, ResolveHosts, ResolverConfig, ResolverOpts},
};
use hickory_server::{
    net::runtime::TokioRuntimeProvider,
    proto::rr::{LowerName, Name, RecordSet, RecordType, TSigResponseContext},
    server::{Request, RequestInfo},
    zone_handler::{
        AuthLookup, AxfrPolicy, LookupControlFlow, LookupError, LookupOptions, LookupRecords,
        ZoneHandler, ZoneType,
    },
};

use crate::utils;

pub struct ResolverZoneHandler {
    origin: LowerName,
    resolver: Resolver<TokioRuntimeProvider>,
}

impl ResolverZoneHandler {
    #[must_use]
    pub fn new() -> Self {
        let mut options = ResolverOpts::default();
        options.use_hosts_file = ResolveHosts::Never;
        // TODO: set to 0 to disable the inner cache when we implement our own.
        options.cache_size = 500;

        let name_servers = vec![
            NameServerConfig::tls(
                IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)),
                Arc::from("dns.quad9.net"),
            ),
            NameServerConfig::tls(
                IpAddr::V4(Ipv4Addr::new(149, 112, 112, 112)),
                Arc::from("dns.quad9.net"),
            ),
            NameServerConfig::tls(
                IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
                Arc::from("one.one.one.one"),
            ),
            NameServerConfig::tls(
                IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)),
                Arc::from("one.one.one.one"),
            ),
        ];

        let config = ResolverConfig::from_parts(None, Vec::new(), name_servers);

        let resolver = Resolver::builder_with_config(config, TokioRuntimeProvider::new())
            .with_options(options)
            .build()
            .unwrap();

        Self {
            origin: Name::root().into(),
            resolver,
        }
    }
}

#[async_trait::async_trait]
impl ZoneHandler for ResolverZoneHandler {
    fn zone_type(&self) -> ZoneType {
        ZoneType::External
    }

    fn axfr_policy(&self) -> AxfrPolicy {
        AxfrPolicy::Deny
    }

    fn can_validate_dnssec(&self) -> bool {
        false
    }

    fn origin(&self) -> &LowerName {
        &self.origin
    }

    async fn lookup(
        &self,
        name: &LowerName,
        rtype: RecordType,
        _request_info: Option<&RequestInfo<'_>>,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<AuthLookup> {
        let lookup = self.resolver.lookup(name, rtype).await;

        match lookup {
            Ok(v) => {
                let mut answers_rset = RecordSet::with_ttl(
                    name.into(),
                    rtype,
                    (Instant::now() - v.valid_until())
                        .as_secs()
                        .try_into()
                        .unwrap(),
                );
                answers_rset.set_records(v.answers().into());

                let answers = LookupRecords::new(lookup_options, Arc::new(answers_rset));

                let mut additionals_rset = RecordSet::with_ttl(
                    name.into(),
                    rtype,
                    (Instant::now() - v.valid_until())
                        .as_secs()
                        .try_into()
                        .unwrap(),
                );
                additionals_rset.set_records(v.answers().into());

                let additionals = if additionals_rset.is_empty() {
                    None
                } else {
                    Some(LookupRecords::new(
                        lookup_options,
                        Arc::new(additionals_rset),
                    ))
                };

                LookupControlFlow::Continue(Ok(AuthLookup::answers(answers, additionals)))
            }
            Err(e) => LookupControlFlow::Break(Err(LookupError::NetError(e))),
        }
    }

    async fn search(
        &self,
        request: &Request,
        lookup_options: LookupOptions,
    ) -> (LookupControlFlow<AuthLookup>, Option<TSigResponseContext>) {
        let lookups = request
            .queries
            .queries()
            .iter()
            .map(|q| self.lookup(q.name(), q.query_type(), None, lookup_options));

        let lookups = join_all(lookups).await;

        (
            utils::handler_search_aggregator(lookups, lookup_options, true),
            None,
        )
    }

    async fn nsec_records(&self, _: &LowerName, _: LookupOptions) -> LookupControlFlow<AuthLookup> {
        LookupControlFlow::Skip
    }
}
