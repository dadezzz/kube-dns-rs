pub mod context;
pub mod handler;
pub mod watcher;

use std::net::{Ipv4Addr, Ipv6Addr};

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub enum DnsRecordData {
    A { addresses: Vec<Ipv4Addr> },
    AAAA { addresses: Vec<Ipv6Addr> },
}

#[derive(CustomResource, JsonSchema, Deserialize, Debug, Clone, Serialize)]
#[kube(
    group = "zarantonello.dev",
    kind = "DnsRecord",
    version = "v1",
    namespaced
)]
pub struct DnsRecordCrd {
    fqdn: String,
    data: Vec<DnsRecordData>,
}
