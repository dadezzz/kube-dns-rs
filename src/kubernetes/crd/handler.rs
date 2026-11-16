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
        request_info: Option<&RequestInfo<'_>>,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<AuthLookup> {
        let lock = self.context.read().await;

        let Some(entries) = lock.get_entry(query_name, &request_info.unwrap().src.ip()) else {
            // No CRD entry for hostname.
            return LookupControlFlow::Skip;
        };

        if entries.is_empty() {
            // Name exists, but was filtered by client rules, so requests from
            // this client go upstream.
            return LookupControlFlow::Skip;
        }

        // CNAME takes precedence if present. The first entry that is found
        // prevails on the others, so it best practice to ensure that only one
        // DnsRecord k8s resource is created for the CNAME domain.
        let record_set = if let Some(e) = entries.iter().find(|e| e.rtype() == RecordType::CNAME) {
            utils::new_record_set(query_name, RecordType::CNAME, TTL, e.rdatas())
        } else {
            let rdata = entries
                .iter()
                .filter(|e| e.rtype() == query_type)
                .flat_map(|e| e.rdatas());
            utils::new_record_set(query_name, query_type, TTL, rdata)
        };

        if let Some(rs) = record_set {
            utils::continue_with_recordset(lookup_options, rs, None)
        } else {
            // CRD entry present but not for record type / client address.
            LookupControlFlow::Break(Ok(AuthLookup::Empty))
        }
    }

    async fn search(
        &self,
        request: &Request,
        lookup_options: LookupOptions,
    ) -> (LookupControlFlow<AuthLookup>, Option<TSigResponseContext>) {
        let request_info = request.request_info().unwrap();

        let lookups = request.queries.queries().iter().map(|q| {
            self.lookup(
                q.name(),
                q.query_type(),
                Some(&request_info),
                lookup_options,
            )
        });

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
