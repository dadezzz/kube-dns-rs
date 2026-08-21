use std::sync::Arc;

use futures::StreamExt;
use kube::{Api, Client, runtime::watcher};
use tokio::{sync::RwLock, task::JoinHandle};

use crate::kubernetes::crd::DnsRecord;

use super::context::KubernetesCrdContext;

pub struct KubernetesCrdWatcher {
    task: JoinHandle<()>,
}

impl KubernetesCrdWatcher {
    pub fn new(client: Client, ctx: Arc<RwLock<KubernetesCrdContext>>) -> Self {
        let api: Api<DnsRecord> = Api::all(client);

        let task = tokio::spawn(async move {
            watcher(api, watcher::Config::default())
                .for_each(async |event| match event.unwrap() {
                    watcher::Event::InitApply(cr) | watcher::Event::Apply(cr) => {
                        ctx.write().await.add_entry(
                            cr.metadata.namespace.as_ref().unwrap(),
                            cr.metadata.name.as_ref().unwrap(),
                            cr.clone(),
                        );
                    }
                    watcher::Event::Delete(cr) => {
                        ctx.write().await.remove_entry(
                            cr.metadata.namespace.as_ref().unwrap(),
                            cr.metadata.name.as_ref().unwrap(),
                        );
                    }
                    _ => {}
                })
                .await;
        });

        Self { task }
    }

    pub async fn run(self) {
        self.task.await.unwrap();
    }
}
