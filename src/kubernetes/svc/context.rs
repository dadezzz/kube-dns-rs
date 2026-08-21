use std::{collections::HashMap, net::IpAddr};

use k8s_openapi::api::{core::v1::Service, discovery::v1::EndpointSlice};
use tracing::debug;

pub enum ServiceEntry {
    ClusterIpService { addresses: Vec<IpAddr> },
    HeadlessService,
}

impl ServiceEntry {
    pub fn cluster_ip(addresses: Vec<IpAddr>) -> Self {
        Self::ClusterIpService { addresses }
    }

    pub fn headless() -> Self {
        Self::HeadlessService
    }
}

impl TryFrom<&Service> for ServiceEntry {
    type Error = ContextError;

    fn try_from(svc: &Service) -> Result<Self, Self::Error> {
        let spec = svc.spec.as_ref().unwrap();

        if spec.cluster_ip.as_ref().unwrap() == "None" {
            return Ok(Self::HeadlessService);
        }

        Ok(Self::ClusterIpService {
            addresses: spec
                .cluster_ips
                .as_ref()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|ip| ip.parse().unwrap())
                .collect(),
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ContextError {
    #[error("the endpointSlice has an unsupported addressType: {_0}")]
    InvalidEndpointSliceAddressType(String),
}

#[derive(PartialEq, Eq, Hash)]
pub struct EndpointSlicePortKey {
    name: String,
    protocol: String,
}

pub enum EndpointSliceEndpoint {
    Kubernetes {
        name: String,
        addresses: Vec<IpAddr>,
    },
    External {
        addresses: Vec<IpAddr>,
    },
}

impl EndpointSliceEndpoint {
    pub fn addresses(&self) -> &Vec<IpAddr> {
        match self {
            EndpointSliceEndpoint::Kubernetes { addresses, .. } => addresses,
            EndpointSliceEndpoint::External { addresses } => addresses,
        }
    }
}

pub struct EndpointSliceEntry {
    pub endpoints: Vec<EndpointSliceEndpoint>,
    pub ports: HashMap<EndpointSlicePortKey, i32>,
}

impl TryFrom<&EndpointSlice> for EndpointSliceEntry {
    type Error = ContextError;

    fn try_from(eps: &EndpointSlice) -> Result<Self, Self::Error> {
        if eps.address_type != "IPv4" && eps.address_type != "IPv6" {
            return Err(ContextError::InvalidEndpointSliceAddressType(
                eps.address_type.clone(),
            ));
        }

        let mut dns_endpoints = Vec::new();

        eps.endpoints.as_ref().map(|endpoints| {
            for ep in endpoints {
                // Filter endpoints that aren't ready or that are terminating.
                if let Some(conditions) = ep.conditions.as_ref() {
                    let ready = conditions.ready.unwrap_or(true);
                    let terminating = conditions.terminating.unwrap_or(false);

                    if !ready || terminating {
                        continue;
                    }
                }

                if let Some(target) = ep.target_ref.as_ref()
                    && let Some(name) = target.name.as_ref()
                {
                    dns_endpoints.push(EndpointSliceEndpoint::Kubernetes {
                        name: name.to_owned(),
                        addresses: ep.addresses.iter().map(|a| a.parse().unwrap()).collect(),
                    })
                } else {
                    dns_endpoints.push(EndpointSliceEndpoint::External {
                        addresses: ep.addresses.iter().map(|a| a.parse().unwrap()).collect(),
                    })
                }
            }
        });

        let mut dns_ports = HashMap::new();

        eps.ports.as_ref().map(|ports| {
            for p in ports {
                let default_p_name = "".to_string();
                let p_name = p.name.as_ref().unwrap_or(&default_p_name);
                let default_p_proto = "TCP".to_string();
                let p_proto = p.protocol.as_ref().unwrap_or(&default_p_proto);
                let p_number = p.port;

                if let Some(p_number) = p_number {
                    dns_ports.insert(
                        EndpointSlicePortKey {
                            name: format!("_{}", p_name.to_lowercase()),
                            protocol: format!("_{}", p_proto.to_lowercase()),
                        },
                        p_number,
                    );
                }
            }
        });

        Ok(Self {
            endpoints: dns_endpoints,
            ports: dns_ports,
        })
    }
}

#[derive(Default)]
pub struct KubernetesSvcContext {
    svc_entries: HashMap<String, ServiceEntry>,
    eps_entries: HashMap<String, EndpointSliceEntry>,
}

impl KubernetesSvcContext {
    pub fn add_svc(&mut self, namespace: &str, name: &str, entry: ServiceEntry) {
        self.svc_entries
            .entry(format!("{namespace}/{name}"))
            .insert_entry(entry);
        debug!("added service {namespace}/{name}");
    }

    pub fn remove_svc(&mut self, namespace: &str, name: &str) {
        self.svc_entries.remove(&format!("{namespace}/{name}"));
        debug!("removed service {namespace}/{name}");
    }

    pub fn get_svc(&self, namespace: &str, name: &str) -> Option<&ServiceEntry> {
        self.svc_entries.get(&format!("{namespace}/{name}"))
    }

    pub fn add_eps(
        &mut self,
        namespace: &str,
        svc_name: &str,
        eps_name: &str,
        entry: EndpointSliceEntry,
    ) {
        self.eps_entries
            .entry(format!("{namespace}/{svc_name}/{eps_name}"))
            .insert_entry(entry);
        debug!("added endpointslice {namespace}/{eps_name}");
    }

    pub fn remove_eps(&mut self, namespace: &str, svc_name: &str, eps_name: &str) {
        self.eps_entries
            .remove(&format!("{namespace}/{svc_name}/{eps_name}"));
        debug!("removed endpointslice for {namespace}/{eps_name}");
    }

    pub fn get_epss(&self, namespace: &str, svc_name: &str) -> Vec<&EndpointSliceEntry> {
        self.eps_entries
            .iter()
            .filter_map(|(k, v)| {
                if k.starts_with(&format!("{namespace}/{svc_name}/")) {
                    Some(v)
                } else {
                    None
                }
            })
            .collect()
    }
}
