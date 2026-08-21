use std::sync::Arc;

use futures::StreamExt;
use k8s_openapi::api::{core::v1::Service, discovery::v1::EndpointSlice};
use kube::{Api, Client, runtime::watcher};
use tokio::{join, sync::RwLock, task::JoinHandle};

use super::context::{EndpointSliceEntry, KubernetesSvcContext, ServiceEntry};

pub struct KubernetesSvcWatcher {
    svc_task: JoinHandle<()>,
    eps_task: JoinHandle<()>,
}

impl KubernetesSvcWatcher {
    pub fn new(client: Client, ctx: Arc<RwLock<KubernetesSvcContext>>) -> Self {
        let svc_api: Api<Service> = Api::all(client.clone());
        let svc_ctx = ctx.clone();
        let eps_api: Api<EndpointSlice> = Api::all(client);
        let eps_ctx = ctx;

        let svc_task = tokio::spawn(async move {
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

        let eps_task = tokio::spawn(async move {
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

        Self { svc_task, eps_task }
    }

    pub async fn run(self) {
        let (r1, r2) = join!(self.svc_task, self.eps_task);
        r1.unwrap();
        r2.unwrap();
    }
}

fn get_service_name_from_eps(eps: &EndpointSlice) -> Option<&String> {
    eps.metadata
        .labels
        .as_ref()
        .and_then(|l| l.get("kubernetes.io/service-name"))
}
