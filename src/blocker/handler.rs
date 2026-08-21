use futures::future::join_all;
use hickory_server::{
    proto::rr::{LowerName, Name, RecordType, TSigResponseContext},
    server::{Request, RequestInfo},
    zone_handler::{
        AuthLookup, AxfrPolicy, LookupControlFlow, LookupOptions, ZoneHandler, ZoneType,
    },
};
use tracing::info;

use super::domains::BlockListZoneHandlerDomains;
use crate::utils;

pub struct BlockerZoneHandler {
    origin: LowerName,
    domains: BlockListZoneHandlerDomains,
}

impl BlockerZoneHandler {
    #[must_use]
    pub fn new(domains: BlockListZoneHandlerDomains) -> Self {
        Self {
            origin: Name::root().into(),
            domains,
        }
    }
}

#[async_trait::async_trait]
impl ZoneHandler for BlockerZoneHandler {
    fn zone_type(&self) -> ZoneType {
        ZoneType::External
    }

    fn axfr_policy(&self) -> AxfrPolicy {
        AxfrPolicy::Deny
    }

    fn origin(&self) -> &LowerName {
        &self.origin
    }

    async fn lookup(
        &self,
        query_name: &LowerName,
        query_type: RecordType,
        _request_info: Option<&RequestInfo<'_>>,
        _lookup_options: LookupOptions,
    ) -> LookupControlFlow<AuthLookup> {
        let (lists, result) = self.domains.status_of(query_name);

        if result != crate::blocker::domains::Status::Allowed {
            info!(
                "query {} for {} blocked by {:?}",
                query_type, query_name, lists
            );
            return utils::break_with_nxdomain();
        }

        return LookupControlFlow::Skip;
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
            utils::handler_search_aggregator(lookups, lookup_options, false),
            None,
        )
    }

    async fn nsec_records(&self, _: &LowerName, _: LookupOptions) -> LookupControlFlow<AuthLookup> {
        LookupControlFlow::Skip
    }
}
