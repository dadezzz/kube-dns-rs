use std::{net::IpAddr, sync::Arc};

use futures::future::join_all;
use hickory_server::{
    proto::rr::{self, LowerName, RecordSet, RecordType, TSigResponseContext},
    server::{Request, RequestInfo},
    zone_handler::{
        AuthLookup, AxfrPolicy, LookupControlFlow, LookupOptions, ZoneHandler, ZoneType,
    },
};
use tokio::sync::RwLock;

use super::context::ServiceEntry;
use super::context::{EndpointSliceEndpoint, KubernetesSvcContext};
use crate::utils;

const TTL: u32 = 30;

pub struct KubernetesSvcZoneHandler {
    context: Arc<RwLock<KubernetesSvcContext>>,
    cluster_domain: LowerName,
}

impl KubernetesSvcZoneHandler {
    pub fn new(cluster_domain: LowerName, context: Arc<RwLock<KubernetesSvcContext>>) -> Self {
        Self {
            context,
            cluster_domain,
        }
    }
}

#[async_trait::async_trait]
impl ZoneHandler for KubernetesSvcZoneHandler {
    fn zone_type(&self) -> ZoneType {
        ZoneType::Primary
    }

    fn axfr_policy(&self) -> AxfrPolicy {
        AxfrPolicy::Deny
    }

    fn origin(&self) -> &LowerName {
        &self.cluster_domain
    }

    async fn lookup(
        &self,
        query_name: &LowerName,
        query_type: RecordType,
        _request_info: Option<&RequestInfo<'_>>,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<AuthLookup> {
        let mut labels = utils::name_to_labels(query_name);
        // Remove the cluster domain part to use exact matches on the match block.
        for _ in 0..self.origin().num_labels() {
            labels.pop();
        }

        let lock = self.context.read().await;

        let targets: Vec<&Vec<IpAddr>> = match labels.as_slice() {
            [k8s_name, svc_name, svc_ns, "svc"] => {
                let endpointslices = lock.get_epss(svc_ns, svc_name);

                endpointslices
                    .iter()
                    .flat_map(|eps| {
                        eps.endpoints.iter().filter_map(|ep| match ep {
                            EndpointSliceEndpoint::Kubernetes { name, addresses } => {
                                if k8s_name == name {
                                    Some(addresses)
                                } else {
                                    None
                                }
                            }
                            EndpointSliceEndpoint::External { .. } => None,
                        })
                    })
                    .collect()
            }
            // ([port_name, protocol_name, svc_name, svc_ns, ..], RecordType::SRV) => {
            //     // return port
            //     // and A + AAAA records of the service
            // }
            //
            // ([svc_name, svc_ns, ..], RecordType::SRV) => {
            //     // return all ports
            //     // or endpoint slice ports if clusterip none
            // }
            [svc_name, svc_ns, "svc"] => {
                let service = lock.get_svc(svc_ns, svc_name);

                match service {
                    Some(ServiceEntry::ClusterIpService { addresses, .. }) => vec![addresses],
                    Some(ServiceEntry::HeadlessService) => {
                        let endpointslices = lock.get_epss(svc_ns, svc_name);

                        endpointslices
                            .iter()
                            .flat_map(|eps| {
                                eps.endpoints
                                    .iter()
                                    .map(|ep| ep.addresses())
                                    .collect::<Vec<_>>()
                            })
                            .collect()
                    }
                    _ => vec![],
                }
            }
            _ => vec![],
        };

        let mut record_set = None;

        match query_type {
            RecordType::A => {
                record_set = utils::new_a_record_set(
                    query_name,
                    targets
                        .clone()
                        .into_iter()
                        .flatten()
                        .filter_map(|ip| match ip {
                            IpAddr::V4(ipv4_addr) => Some(*ipv4_addr),
                            IpAddr::V6(_) => None,
                        }),
                    TTL,
                )
            }
            RecordType::AAAA => {
                record_set = utils::new_aaaa_record_set(
                    query_name,
                    targets
                        .clone()
                        .into_iter()
                        .flatten()
                        .filter_map(|ip| match ip {
                            IpAddr::V4(_) => None,
                            IpAddr::V6(ipv6_addr) => Some(*ipv6_addr),
                        }),
                    TTL,
                )
            }
            RecordType::SOA => {
                // Primary Name Server (MNAME) and Admin Email (RNAME - replace @ with .)
                let mname = self
                    .cluster_domain
                    .prepend_label("svc")
                    .unwrap()
                    .prepend_label("kube-dns-rs")
                    .unwrap()
                    .prepend_label("kube-dns-rs")
                    .unwrap();

                let rname = self.cluster_domain.prepend_label("admin").unwrap();

                let soa_rdata = rr::RData::SOA(rr::rdata::SOA::new(
                    mname, rname,      //
                    2026082101, // Serial number (YYYYMMDDNN format)
                    3600,       // Refresh: 1 hour
                    1800,       // Retry: 30 minutes
                    604800,     // Expire: 1 week
                    300,        // Minimum / Negative Caching TTL: 5 minutes (300s)
                ));

                let mut rs =
                    RecordSet::new(self.cluster_domain.clone().into(), RecordType::SOA, TTL);
                rs.add_rdata(soa_rdata);

                record_set = Some(rs)
            }
            _ => {}
        }

        if let Some(record_set) = record_set {
            utils::continue_with_recordset(lookup_options, record_set, None)
        } else if targets.is_empty() {
            // Zone doesn't exist.
            return utils::break_with_nxdomain();
        } else {
            // Zone exists (there are targets), but not for the specified Rtype.
            LookupControlFlow::Continue(Ok(AuthLookup::Empty))
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
