use std::{sync::Arc, time::Instant};

use futures::future::join_all;
use hickory_resolver::{
    Resolver,
    config::{ResolveHosts, ResolverConfig, ResolverOpts},
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

use crate::{init, settings::UpstreamSettings, utils};

pub struct ResolverZoneHandler {
    origin: LowerName,
    pub resolver: Arc<Resolver<TokioRuntimeProvider>>,
}

impl ResolverZoneHandler {
    #[must_use]
    pub fn new(upstreams: &[UpstreamSettings], tls_client: rustls::ClientConfig) -> Self {
        let upstreams = init::configure_upstreams(upstreams);
        let config = ResolverConfig::from_parts(None, Vec::new(), upstreams);

        let mut options = ResolverOpts::default();
        options.use_hosts_file = ResolveHosts::Never;
        // TODO: set to 0 to disable the inner cache when we implement our own.
        options.cache_size = 500;

        let resolver = Resolver::builder_with_config(config, TokioRuntimeProvider::new())
            .with_options(options)
            .with_tls_config(tls_client)
            .build()
            .unwrap();

        Self {
            origin: Name::root().into(),
            resolver: Arc::new(resolver),
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
                let ttl = v
                    .valid_until()
                    .duration_since(Instant::now())
                    .as_secs()
                    .try_into()
                    .unwrap();

                let mut answers_rset = RecordSet::with_ttl(name.into(), rtype, ttl);
                answers_rset.set_records(v.answers().into());

                let answers = LookupRecords::new(lookup_options, Arc::new(answers_rset));

                let mut additionals_rset = RecordSet::with_ttl(name.into(), rtype, ttl);
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

    fn metrics_label(&self) -> &'static str {
        "upstream_resolver"
    }
}
