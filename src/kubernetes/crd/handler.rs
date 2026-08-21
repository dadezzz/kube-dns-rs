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

use super::DnsRecordData;
use crate::{kubernetes::crd::context::KubernetesCrdContext, utils};

pub struct KubernetesCrdZoneHandler {
    origin: LowerName,
    context: Arc<RwLock<KubernetesCrdContext>>,
}

impl KubernetesCrdZoneHandler {
    pub fn new(context: Arc<RwLock<KubernetesCrdContext>>) -> Self {
        Self {
            origin: Name::root().into(),
            context,
        }
    }
}

const TTL: u32 = 30;

#[async_trait::async_trait]
impl ZoneHandler for KubernetesCrdZoneHandler {
    fn zone_type(&self) -> ZoneType {
        ZoneType::Primary
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
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<AuthLookup> {
        let record_set = match query_type {
            RecordType::A => {
                let lock = self.context.read().await;

                lock.get_entry(&query_name).and_then(|data| {
                    utils::new_a_record_set(
                        query_name,
                        data.into_iter()
                            .filter_map(|d| match d {
                                DnsRecordData::A { addresses } => Some(addresses),
                                _ => None,
                            })
                            .flatten()
                            .map(|ip| ip.to_owned()),
                        TTL,
                    )
                })
            }
            RecordType::AAAA => {
                let lock = self.context.read().await;

                lock.get_entry(&query_name).and_then(|data| {
                    utils::new_aaaa_record_set(
                        query_name,
                        data.into_iter()
                            .filter_map(|d| match d {
                                DnsRecordData::AAAA { addresses } => Some(addresses),
                                _ => None,
                            })
                            .flatten()
                            .map(|ip| ip.to_owned()),
                        TTL,
                    )
                })
            }
            _ => None,
        };

        if let Some(rs) = record_set {
            utils::continue_with_recordset(lookup_options, rs, None)
        } else {
            LookupControlFlow::Skip
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
            utils::handler_search_aggregator(lookups, lookup_options, false),
            None,
        )
    }

    async fn nsec_records(&self, _: &LowerName, _: LookupOptions) -> LookupControlFlow<AuthLookup> {
        LookupControlFlow::Skip
    }
}
