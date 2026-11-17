pub mod context;
pub mod handler;
pub mod watcher;

use std::{
    collections::{HashMap, hash_map::Entry},
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use hickory_server::proto::{
    ProtoError,
    rr::{LowerName, RData, RecordType, rdata},
};
use kube::{CustomResource, ResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::task::JoinError;

use crate::utils::{self, ipnet::IpNet};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub enum DnsRecordData {
    A { addresses: Vec<Ipv4Addr> },
    AAAA { addresses: Vec<Ipv6Addr> },
    CNAME { name: utils::name::Name },
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
            DnsRecordData::CNAME { name } => vec![RData::CNAME(rdata::CNAME(name.into()))],
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

#[derive(Debug)]
pub struct IndexableDnsRecord {
    pub k8s_uid: String,
    pub name: LowerName,
    // Hashmap allows to filter by rtype.
    pub rdata: HashMap<RecordType, Vec<RData>>,
    pub nets: Vec<IpNet>,
}

impl TryFrom<&DnsRecord> for IndexableDnsRecord {
    type Error = KubernetesCrdError;

    fn try_from(value: &DnsRecord) -> Result<Self, Self::Error> {
        let mut rdata = HashMap::<RecordType, Vec<RData>>::new();

        for d in &value.spec.data {
            let rtype = d.rtype();
            let entry = rdata.entry(rtype);

            if rtype == RecordType::CNAME
                && let Entry::Occupied(_) = entry
            {
                return Err(KubernetesCrdError::MultipleCNAMEs);
            }

            entry
                .and_modify(|i| i.extend(d.rdatas()))
                .or_insert_with(|| d.rdatas());
        }

        Ok(IndexableDnsRecord {
            k8s_uid: value.uid().ok_or(KubernetesCrdError::MissingUID)?,
            name: LowerName::from_str(&value.spec.fqdn).map_err(KubernetesCrdError::InvalidName)?,
            rdata,
            nets: value
                .spec
                .clients
                .clone()
                .unwrap_or(vec![IpNet::unspecified()]),
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum KubernetesCrdError {
    #[error("failed to join task: {0}")]
    TaskJoin(JoinError),
    #[error("name is in invalid format: {0}")]
    InvalidName(ProtoError),
    #[error("name cannot have multiple CNAMEs or other records defined")]
    MultipleCNAMEs,
    #[error("k8s resource is missing the UID")]
    MissingUID,
    #[error("k8s client error: {0}")]
    K8SError(kube::runtime::watcher::Error),
}
