use std::sync::Arc;

use futures::StreamExt;
use k8s_openapi::api::{core::v1::Service, discovery::v1::EndpointSlice};
use kube::{Api, Client, runtime::watcher};
use tokio::{
    sync::RwLock,
    task::{JoinError, JoinSet},
};

use super::context::{EndpointSliceEntry, KubernetesSvcContext, ServiceEntry};

#[derive(thiserror::Error, Debug)]
pub enum KubernetesSvcWatcherError {
    #[error("failed to join task: {0}")]
    TaskJoin(JoinError),
}

pub struct KubernetesSvcWatcher {
    tasks: JoinSet<()>,
}

impl KubernetesSvcWatcher {
    pub fn new(client: Client, ctx: Arc<RwLock<KubernetesSvcContext>>) -> Self {
        let svc_api: Api<Service> = Api::all(client.clone());
        let svc_ctx = ctx.clone();
        let eps_api: Api<EndpointSlice> = Api::all(client);
        let eps_ctx = ctx;

        let mut tasks = JoinSet::new();

        tasks.spawn(async move {
            watcher(svc_api, watcher::Config::default())
                .for_each(async |event| match event.unwrap() {
                    watcher::Event::InitApply(svc) | watcher::Event::Apply(svc) => {
                        svc_ctx.write().await.add_svc(
                            svc.metadata.namespace.as_ref().unwrap(),
                            svc.metadata.name.as_ref().unwrap(),
                            ServiceEntry::try_from(&svc).unwrap(),
                        );
                    }
                    watcher::Event::Delete(svc) => {
                        svc_ctx.write().await.remove_svc(
                            svc.metadata.namespace.as_ref().unwrap(),
                            svc.metadata.name.as_ref().unwrap(),
                        );
                    }
                    _ => {}
                })
                .await;
        });

        tasks.spawn(async move {
            watcher(eps_api, watcher::Config::default())
                .for_each(async |event| match event.unwrap() {
                    watcher::Event::InitApply(eps) | watcher::Event::Apply(eps) => {
                        eps_ctx.write().await.add_eps(
                            eps.metadata.namespace.as_ref().unwrap(),
                            get_service_name_from_eps(&eps).unwrap(),
                            eps.metadata.name.as_ref().unwrap(),
                            EndpointSliceEntry::try_from(&eps).unwrap(),
                        );
                    }
                    watcher::Event::Delete(eps) => {
                        eps_ctx.write().await.remove_eps(
                            eps.metadata.namespace.as_ref().unwrap(),
                            get_service_name_from_eps(&eps).unwrap(),
                            eps.metadata.name.as_ref().unwrap(),
                        );
                    }
                    _ => {}
                })
                .await;
        });

        Self { tasks }
    }

    pub async fn run(&mut self) -> Result<(), KubernetesSvcWatcherError> {
        if self.tasks.is_empty() {
            // Handle case where there are no lists. The server needs to keep
            // running.
            self.tasks.spawn(std::future::pending());
        }

        while let Some(res) = self.tasks.join_next().await {
            match res {
                Ok(()) => {}
                Err(e) => return Err(KubernetesSvcWatcherError::TaskJoin(e)),
            }
        }

        Ok(())
    }
}

impl Drop for KubernetesSvcWatcher {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}

fn get_service_name_from_eps(eps: &EndpointSlice) -> Option<&String> {
    eps.metadata
        .labels
        .as_ref()
        .and_then(|l| l.get("kubernetes.io/service-name"))
}
