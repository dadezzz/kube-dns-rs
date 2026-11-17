use std::sync::Arc;

use futures::StreamExt;
use kube::{Api, Client, runtime::watcher};
use tokio::{sync::RwLock, task::JoinSet};
use tracing::error;

use super::{DnsRecord, KubernetesCrdError, context::KubernetesCrdContext};

pub struct KubernetesCrdWatcher {
    tasks: JoinSet<()>,
}

impl KubernetesCrdWatcher {
    pub fn new(client: Client, ctx: Arc<RwLock<KubernetesCrdContext>>) -> Self {
        let api: Api<DnsRecord> = Api::all(client);

        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            watcher(api, watcher::Config::default())
                .for_each(async |event| {
                    let res = match event {
                        Ok(watcher::Event::InitApply(cr) | watcher::Event::Apply(cr)) => {
                            ctx.write().await.add_entry(&cr)
                        }
                        Ok(watcher::Event::Delete(cr)) => ctx.write().await.remove_entry(&cr),
                        Err(e) => Err(KubernetesCrdError::K8SError(e)),
                        _ => Ok(()),
                    };

                    if let Err(e) = res {
                        error!("{e}");
                    }
                })
                .await;
        });

        Self { tasks }
    }

    pub async fn run(&mut self) -> Result<(), KubernetesCrdError> {
        self.tasks
            .join_next()
            .await
            .unwrap()
            .map_err(KubernetesCrdError::TaskJoin)
    }
}

impl Drop for KubernetesCrdWatcher {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}
