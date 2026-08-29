use std::sync::Arc;

use futures::future::join_all;
use hickory_server::{
    proto::rr::{LowerName, Name, RecordType, TSigResponseContext},
    server::{Request, RequestInfo},
    zone_handler::{
        AuthLookup, AxfrPolicy, LookupControlFlow, LookupOptions, ZoneHandler, ZoneType,
    },
};
use tokio::sync::RwLock;
use tracing::info;

use super::context::{BlockerContext, ListType};
use crate::utils;

pub struct BlockerZoneHandler {
    origin: LowerName,
    context: Arc<RwLock<BlockerContext>>,
}

impl BlockerZoneHandler {
    #[must_use]
    pub fn new(context: Arc<RwLock<BlockerContext>>) -> Self {
        Self {
            origin: Name::root().into(),
            context,
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
        let lookup = self.context.read().await.lookup(query_name);

        if let Some((urls, name, list_type)) = lookup
            && list_type == ListType::Block
        {
            info!(
                "query {} for {} blocked ({}) by {:?}",
                query_type, query_name, name, urls
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
