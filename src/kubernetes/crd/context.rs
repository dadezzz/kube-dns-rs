use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use hickory_server::proto::rr::{Name, RecordType};
use kube::ResourceExt;
use tracing::debug;

use crate::{
    kubernetes::crd::{DnsRecord, IndexableDnsRecord, KubernetesCrdError},
    utils::{self, ipnet::IpNet, trie::Trie},
};

#[derive(Default)]
pub struct KubernetesCrdContext {
    pub entries: Vec<IndexableDnsRecord>,
    by_rtype: HashMap<RecordType, HashSet<usize>>,
    by_k8s_ref: HashMap<String, usize>,
    by_net: HashMap<IpNet, HashSet<usize>>,
    by_labels: Trie<HashSet<usize>>,
}

impl KubernetesCrdContext {
    pub fn add_entry(&mut self, record: &DnsRecord) -> Result<(), KubernetesCrdError> {
        let indexable = IndexableDnsRecord::try_from(record)?;
        let k8s_uid = indexable.k8s_uid.clone();

        let idx = self.get_idx_by_k8s_uid(&k8s_uid);
        if let Some(idx) = idx {
            self.entries.swap_remove(*idx);
        }

        self.entries.push(indexable);
        self.rebuild_indexes();
        debug!("added new record from CRD {k8s_uid}");
        Ok(())
    }

    pub fn remove_entry(&mut self, record: &DnsRecord) -> Result<(), KubernetesCrdError> {
        let k8s_uid = record.uid().ok_or(KubernetesCrdError::MissingUID)?;
        let idx = self.get_idx_by_k8s_uid(&k8s_uid);
        if let Some(idx) = idx {
            self.entries.swap_remove(*idx);
            self.rebuild_indexes();
            debug!("removed record from CRD {k8s_uid}");
        }
        Ok(())
    }

    fn rebuild_indexes(&mut self) {
        self.by_k8s_ref.clear();
        self.by_net.clear();
        self.by_rtype.clear();
        self.by_labels = Trie::new();

        for (idx, record) in self.entries.iter().enumerate() {
            self.by_k8s_ref.insert(record.k8s_uid.clone(), idx);

            for d in &record.rdata {
                self.by_rtype.entry(*d.0).or_default().insert(idx);
            }

            for net in &record.nets {
                self.by_net.entry(net.to_owned()).or_default().insert(idx);
            }

            let labels = utils::name_to_path(&record.name);

            if let Some(hash_set) = self.by_labels.get_mut(&labels) {
                hash_set.insert(idx);
            } else {
                let mut hash_set = HashSet::new();
                hash_set.insert(idx);
                self.by_labels.insert(&labels, hash_set);
            }
        }
    }

    /// Get index that was created by a specific kubernetes CRD.
    #[must_use]
    fn get_idx_by_k8s_uid(&self, k8s_ref: &str) -> Option<&usize> {
        self.by_k8s_ref.get(k8s_ref)
    }

    /// Get all the entries that are accessible to client.
    #[must_use]
    pub fn get_idx_by_client_ip(&self, client_ip: &IpAddr) -> HashSet<usize> {
        self.by_net
            .iter()
            .filter_map(|(n, i)| {
                if n.contains_host(client_ip) {
                    Some(i)
                } else {
                    None
                }
            })
            .fold(HashSet::new(), |a, b| a.union(b).copied().collect())
    }

    /// Get all records defined for a specific name.
    #[must_use]
    pub fn get_idx_by_name(&self, name: &Name) -> HashSet<usize> {
        let labels = utils::name_to_path(name);
        self.by_labels.get(&labels).cloned().unwrap_or_default()
    }

    /// Get all entries for a specific record type.
    #[must_use]
    pub fn get_idx_by_rtype(&self, rtype: RecordType) -> HashSet<usize> {
        self.by_rtype.get(&rtype).cloned().unwrap_or_default()
    }
}
