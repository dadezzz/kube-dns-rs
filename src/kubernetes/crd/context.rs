use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use hickory_server::proto::rr::Name;
use tracing::debug;

use crate::{
    kubernetes::crd::{DnsRecord, DnsRecordData},
    utils::{self, ipnet::IpNet, trie::Trie},
};

#[derive(Default)]
pub struct KubernetesCrdContext {
    entries: Vec<DnsRecord>,
    by_k8s_ref: HashMap<String, usize>,
    by_net: HashMap<IpNet, HashSet<usize>>,
    by_labels: Trie<HashSet<usize>>,
}

impl KubernetesCrdContext {
    pub fn add_entry(&mut self, namespace: &str, name: &str, record: DnsRecord) {
        let idx = self.get_idx_by_k8s_ref(namespace, name);
        if let Some(idx) = idx {
            self.entries.remove(*idx);
        }

        self.entries.push(record);
        self.rebuild_indexes(namespace, name);
        debug!("added new record from CRD {namespace}/{name}");
    }

    pub fn remove_entry(&mut self, namespace: &str, name: &str) {
        let idx = self.get_idx_by_k8s_ref(namespace, name);
        if let Some(idx) = idx {
            self.entries.remove(*idx);
            self.rebuild_indexes(namespace, name);
            debug!("removed record from CRD {namespace}/{name}");
        }
    }

    fn rebuild_indexes(&mut self, namespace: &str, name: &str) {
        self.by_k8s_ref.clear();
        self.by_net.clear();
        self.by_labels = Trie::new();

        for (idx, record) in self.entries.iter().enumerate() {
            self.by_k8s_ref.insert(format!("{namespace}/{name}"), idx);

            let default_nets = vec![
                IpNet::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
                IpNet::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
            ];
            let nets = record.spec.clients.as_ref().unwrap_or(&default_nets);

            for net in nets {
                self.by_net.entry(net.to_owned()).or_default().insert(idx);
            }

            let name = Name::from_utf8(&record.spec.fqdn).unwrap();
            let labels = utils::name_to_path(&name);

            if let Some(hash_set) = self.by_labels.get_mut(&labels) {
                hash_set.insert(idx);
            } else {
                let mut hash_set = HashSet::new();
                hash_set.insert(idx);
                self.by_labels.insert(&labels, hash_set);
            }
        }
    }

    fn get_idx_by_k8s_ref(&self, namespace: &str, name: &str) -> Option<&usize> {
        self.by_k8s_ref.get(&format!("{namespace}/{name}"))
    }

    #[must_use]
    pub fn get_entry(&self, name: &Name, client_ip: &IpAddr) -> Option<Vec<&DnsRecordData>> {
        let labels = utils::name_to_path(name);
        let labels_set = self.by_labels.get(&labels);

        let mut nets_set = HashSet::new();
        for (net, net_set) in &self.by_net {
            if net.contains_host(client_ip) {
                nets_set.extend(net_set);
            }
        }

        labels_set.map(|ls| {
            ls.intersection(&nets_set)
                .flat_map(|i| &self.entries[*i].spec.data)
                .collect()
        })
    }
}
