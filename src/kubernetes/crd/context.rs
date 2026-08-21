use std::collections::HashMap;

use hickory_server::proto::rr::Name;
use tracing::debug;

use crate::{
    kubernetes::crd::{DnsRecord, DnsRecordData},
    trie::Trie,
    utils,
};

#[derive(Default)]
pub struct KubernetesCrdContext {
    by_k8s_ref: HashMap<String, DnsRecord>,
    by_labels: Trie<Vec<DnsRecordData>>,
}

impl KubernetesCrdContext {
    pub fn add_entry(&mut self, namespace: &str, name: &str, cr: DnsRecord) {
        self.by_k8s_ref
            .entry(format!("{namespace}/{name}"))
            .insert_entry(cr);

        self.rebuild_labels_from_refs();
        debug!("added new record from CRD {namespace}/{name}");
    }

    pub fn remove_entry(&mut self, namespace: &str, name: &str) {
        self.by_k8s_ref.remove(&format!("{namespace}/{name}"));
        self.rebuild_labels_from_refs();
        debug!("removed record from CRD {namespace}/{name}");
    }

    fn rebuild_labels_from_refs(&mut self) {
        self.by_labels = Trie::new();

        for record in self.by_k8s_ref.values() {
            let name = Name::from_utf8(&record.spec.fqdn).unwrap();
            let labels = utils::name_to_labels(&name);
            let data = record.spec.data.clone();

            if let Some(datas) = self.by_labels.get_mut(&labels) {
                datas.extend(data);
            } else {
                self.by_labels.insert(&labels, data);
            }
        }
    }

    pub fn get_entry(&self, name: &Name) -> Option<&Vec<DnsRecordData>> {
        let labels = utils::name_to_labels(name);
        self.by_labels.get(labels)
    }
}
