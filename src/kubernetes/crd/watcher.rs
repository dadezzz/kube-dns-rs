use std::sync::Arc;

use futures::StreamExt;
use kube::{Api, Client, runtime::watcher};
use tokio::{
    sync::RwLock,
    task::{JoinError, JoinSet},
};

use crate::kubernetes::crd::DnsRecord;

use super::context::KubernetesCrdContext;

#[derive(thiserror::Error, Debug)]
pub enum KubernetesCrdWatcherError {
    #[error("failed to join task: {0}")]
    TaskJoin(JoinError),
}

pub struct KubernetesCrdWatcher {
    tasks: JoinSet<()>,
}

impl KubernetesCrdWatcher {
    pub fn new(client: Client, ctx: Arc<RwLock<KubernetesCrdContext>>) -> Self {
        let api: Api<DnsRecord> = Api::all(client);

        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
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

        Self { tasks }
    }

    pub async fn run(&mut self) -> Result<(), KubernetesCrdWatcherError> {
        self.tasks
            .join_next()
            .await
            .unwrap()
            .map_err(KubernetesCrdWatcherError::TaskJoin)
    }
}

impl Drop for KubernetesCrdWatcher {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}
