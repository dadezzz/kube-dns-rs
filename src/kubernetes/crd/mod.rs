pub mod context;
pub mod handler;
pub mod watcher;

use std::net::{Ipv4Addr, Ipv6Addr};

use hickory_server::proto::rr::{RData, RecordType, rdata};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::utils::{self, ipnet::IpNet};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub enum DnsRecordData {
    A { addresses: Vec<Ipv4Addr> },
    AAAA { addresses: Vec<Ipv6Addr> },
    CNAME { names: Vec<utils::name::Name> },
}

impl DnsRecordData {
    fn rdatas(&self) -> Vec<RData> {
        match self {
            DnsRecordData::A { addresses } => {
                addresses.iter().map(|a| RData::A(rdata::A(*a))).collect()
            }
            DnsRecordData::AAAA { addresses } => addresses
                .iter()
                .map(|a| RData::AAAA(rdata::AAAA(*a)))
                .collect(),
            DnsRecordData::CNAME { names } => names
                .iter()
                .map(|n| RData::CNAME(rdata::CNAME(n.into())))
                .collect(),
        }
    }

    fn rtype(&self) -> RecordType {
        match self {
            DnsRecordData::A { .. } => RecordType::A,
            DnsRecordData::AAAA { .. } => RecordType::AAAA,
            DnsRecordData::CNAME { .. } => RecordType::CNAME,
        }
    }
}

#[derive(CustomResource, JsonSchema, Deserialize, Debug, Clone, Serialize)]
#[kube(
    group = "zarantonello.dev",
    kind = "DnsRecord",
    version = "v1",
    namespaced
)]
pub struct DnsRecordCrd {
    clients: Option<Vec<IpNet>>,
    fqdn: String,
    data: Vec<DnsRecordData>,
}
