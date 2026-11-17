use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures::future::join_all;
use hickory_server::{
    proto::{
        op::ResponseCode,
        rr::{self, LowerName, Name, RData, RecordSet, RecordType, TSigResponseContext},
    },
    server::{Request, RequestInfo},
    zone_handler::{
        AuthLookup, AxfrPolicy, Catalog, LookupControlFlow, LookupError, LookupOptions,
        ZoneHandler, ZoneType,
    },
};
use tokio::sync::RwLock;

use crate::{kubernetes::crd::context::KubernetesCrdContext, utils};

const TTL: u32 = 30;

pub struct KubernetesCrdZoneHandler {
    origin: LowerName,
    catalog: Catalog,
    context: Arc<RwLock<KubernetesCrdContext>>,
}

impl KubernetesCrdZoneHandler {
    pub fn new(catalog: Catalog, context: Arc<RwLock<KubernetesCrdContext>>) -> Self {
        Self {
            origin: Name::root().into(),
            catalog,
            context,
        }
    }
}

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

        let labels_idx = lock.get_idx_by_name(query_name);
        if labels_idx.is_empty() {
            return LookupControlFlow::Skip;
        }

        let client_ip = request_info.unwrap().src.ip();
        let net_idx = lock.get_idx_by_client_ip(&client_ip);
        let allowed_idx: HashSet<_> = labels_idx.intersection(&net_idx).copied().collect();
        if allowed_idx.is_empty() {
            return LookupControlFlow::Break(Ok(AuthLookup::Empty));
        }

        let rtype_idx = lock.get_idx_by_rtype(query_type);
        let mut final_set = HashSet::new();
        for i in allowed_idx {
            let entry = &lock.entries[i];
            if let Some(rdata) = entry.rdata.get(&RecordType::CNAME) {
                let record_set = utils::new_record_set(
                    query_name.clone(),
                    RecordType::CNAME,
                    TTL,
                    rdata.clone(),
                );

                let RData::CNAME(rr::rdata::CNAME(name)) = rdata.first().unwrap() else {
                    return LookupControlFlow::Break(Err(LookupError::ResponseCode(
                        ResponseCode::ServFail,
                    )));
                };

                let mut name: LowerName = name.into();
                name.set_fqdn(true);
                let zone_handlers = self.catalog.find(&name);

                if let Some(zh) = zone_handlers {
                    let lookups = zh
                        .iter()
                        .map(|zh| zh.lookup(&name, query_type, request_info, lookup_options));

                    let mut lookups = join_all(lookups).await;
                    lookups.push(utils::continue_with_recordset(
                        lookup_options,
                        vec![record_set],
                        None,
                    ));
                    return utils::handler_search_aggregator(lookups, lookup_options, false);
                }
            }

            if query_type != RecordType::ANY && !rtype_idx.contains(&i) {
                continue;
            }

            final_set.insert(i);
        }

        let mut final_by_rname_rtype = HashMap::<(Name, RecordType), RecordSet>::new();
        for i in final_set {
            let entry = &lock.entries[i];

            for d in entry
                .rdata
                .iter()
                .filter(|d| query_type == RecordType::ANY || *d.0 == query_type)
            {
                final_by_rname_rtype
                    .entry((entry.name.clone().into(), *d.0))
                    .and_modify(|rs| {
                        utils::add_rdata_to_record_set(rs, d.1.clone());
                    })
                    .or_insert_with(|| {
                        utils::new_record_set(query_name.clone(), *d.0, TTL, d.1.clone())
                    });
            }
        }

        utils::continue_with_recordset(
            lookup_options,
            final_by_rname_rtype.into_values().collect(),
            None,
        )
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
